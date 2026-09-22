//! Bot processes — one runner per bot (PLAN-QUANTALGO §3.2).
//!
//! `start_bot` spawns `python -m quantalgo.runner <strategy.py>` for one bot
//! row, warms it with recent candles, and gives it three threads of its own:
//! a stdout reader (the strategy's JSON-RPC: orders, logs), a market feed
//! (polls the exchange's public candles and pushes `on_candle`), and a
//! watcher that reports the exit. Every event the module emits carries the
//! `bot_id`; the fill bookkeeping is `runtime.rs`, the rows are `bots.rs`.
//!
//! The suite's process register gets ONE entry, `algo.bots`: Running while
//! any bot runs. The register's commands take no arguments (ARCHITECTURE §6),
//! so per-bot rows there would have no buttons — the Bots page is the place
//! for those.

use chrono::Utc;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::bots::{insert_bot, load_bot, set_bot_status, validate_mode, validate_timeframe, Bot, BotDraft};
use crate::broker::{Broker, LiveBroker, SymbolFilters, Venue};
use crate::exchanges::{
    fetch_exchange_pairs_for_provider,
    load_credentials,
    fetch_latest_market_candle,
    fetch_recent_market_candles,
    market_candle_json,
};
use crate::logs::{push_bot_log, push_bot_log_for};
use crate::runtime::{BotRuntime, Effect, Mode};
use crate::settings::{
    build_python_command,
    hide_console_window,
    resolve_python_path,
    MAX_CONCURRENT_POSITIONS,
    MAX_PAPER_FEE_PCT,
    MAX_RISK_PER_TRADE_PCT,
    MAX_SLIPPAGE_TOLERANCE_PCT,
    WARN_CONCURRENT_POSITIONS,
    WARN_PAPER_FEE_PCT,
    WARN_RISK_PER_TRADE_PCT,
    WARN_SLIPPAGE_TOLERANCE_PCT,
};
use crate::{AppState, BotStatus, LogEntry, Trade, BOTS_PROCESS_ID};

/// Candles the runner is warmed with before the live feed starts. A regime
/// strategy may want more (its indicator's warm-up); the runner keeps up to
/// 5 000 candles, so the config can ask for more via `warmup_candles`.
const DEFAULT_WARMUP_CANDLES: usize = 200;
const MAX_WARMUP_CANDLES: usize = 3000;

/// A running bot's process and state — the value of `AppState.bots`.
pub struct BotHandle {
    pub child: Arc<Mutex<Child>>,
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stop_flag: Arc<AtomicBool>,
    pub runtime: Arc<Mutex<BotRuntime>>,
}

fn write_json_line(stdin: &Arc<Mutex<ChildStdin>>, payload: Value) -> Result<(), String> {
    let mut writer = stdin.lock().map_err(|e| format!("stdin lock: {e}"))?;
    writer
        .write_all(payload.to_string().as_bytes())
        .map_err(|e| format!("stdin write: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("stdin newline: {e}"))?;
    writer.flush().map_err(|e| format!("stdin flush: {e}"))
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

fn emit_bot_status(app_handle: &AppHandle, bot: &Bot) {
    let _ = app_handle.emit(
        "bot:status",
        json!({
            "bot_id": bot.id,
            "status": bot.status,
            "strategy_id": bot.strategy_id,
            "exchange_id": bot.exchange_id,
            "pair": bot.pair,
            "started_at": bot.started_at,
            "trading_mode": bot.trading_mode,
            "last_error": bot.last_error,
        }),
    );
}

/// Log the error, mark the bot's row (when the error belongs to one) and
/// announce it. Module-level errors (a backtest runner that would not spawn)
/// carry no bot id.
pub(crate) fn emit_bot_error(
    app_handle: &AppHandle,
    bot_id: Option<&str>,
    message: impl Into<String>,
    details: Option<String>,
) {
    let message = message.into();
    push_bot_log_for(app_handle, bot_id, "error", message.clone());

    if let Some(id) = bot_id {
        if let Some(state) = app_handle.try_state::<AppState>() {
            if let Ok(db) = state.db.lock() {
                let _ = set_bot_status(&db, id, "error", Some(&message));
            }
        }
    }

    let _ = app_handle.emit(
        "bot:error",
        json!({
            "bot_id": bot_id,
            "message": message,
            "details": details,
        }),
    );
}

fn persist_trade(app_handle: &AppHandle, trade: &Trade) -> Result<(), String> {
    let state = app_handle
        .try_state::<AppState>()
        .ok_or_else(|| "App state unavailable".to_string())?;
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    if trade.exit_price.is_none() {
        db.execute(
            "INSERT OR REPLACE INTO trades (id, bot_id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, backtest_id, notes, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,NULL,?15,?16)",
            params![
                trade.id,
                trade.bot_id,
                trade.strategy_id,
                trade.exchange,
                trade.pair,
                trade.side,
                trade.entry_price,
                trade.exit_price,
                trade.quantity,
                trade.entry_time,
                trade.exit_time,
                trade.pnl,
                trade.pnl_pct,
                trade.fee,
                trade.notes,
                trade.created_at,
            ],
        )
        .map_err(|e| format!("Insert trade: {e}"))?;
    } else {
        let updated = db
            .execute(
                "UPDATE trades SET exit_price = ?1, exit_time = ?2, pnl = ?3, pnl_pct = ?4, fee = ?5, notes = ?6 WHERE id = ?7",
                params![
                    trade.exit_price,
                    trade.exit_time,
                    trade.pnl,
                    trade.pnl_pct,
                    trade.fee,
                    trade.notes,
                    trade.id,
                ],
            )
            .map_err(|e| format!("Update trade: {e}"))?;
        if updated == 0 {
            db.execute(
                "INSERT INTO trades (id, bot_id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, backtest_id, notes, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,NULL,?15,?16)",
                params![
                    trade.id,
                    trade.bot_id,
                    trade.strategy_id,
                    trade.exchange,
                    trade.pair,
                    trade.side,
                    trade.entry_price,
                    trade.exit_price,
                    trade.quantity,
                    trade.entry_time,
                    trade.exit_time,
                    trade.pnl,
                    trade.pnl_pct,
                    trade.fee,
                    trade.notes,
                    trade.created_at,
                ],
            )
            .map_err(|e| format!("Insert closed trade: {e}"))?;
        }
    }
    Ok(())
}

/// Snapshots older than this are dropped so the table cannot grow without bound.
const EQUITY_SNAPSHOT_RETENTION_DAYS: i64 = 90;
/// Pruning is not worth a full scan per insert — run it every N snapshots instead.
const EQUITY_SNAPSHOT_PRUNE_EVERY: u64 = 500;
static EQUITY_SNAPSHOT_COUNT: AtomicU64 = AtomicU64::new(0);

fn persist_equity_snapshot(app_handle: &AppHandle, bot_id: &str, timestamp: &str, equity: f64, source: &str) {
    if let Some(state) = app_handle.try_state::<AppState>() {
        if let Ok(db) = state.db.lock() {
            let _ = db.execute(
                "INSERT INTO equity_snapshots (bot_id, timestamp, equity, source) VALUES (?1, ?2, ?3, ?4)",
                params![bot_id, timestamp, equity, source],
            );
            let inserted = EQUITY_SNAPSHOT_COUNT.fetch_add(1, Ordering::Relaxed);
            if inserted % EQUITY_SNAPSHOT_PRUNE_EVERY == 0 {
                let cutoff = (Utc::now() - chrono::Duration::days(EQUITY_SNAPSHOT_RETENTION_DAYS)).to_rfc3339();
                let _ = db.execute(
                    "DELETE FROM equity_snapshots WHERE timestamp < ?1",
                    params![cutoff],
                );
            }
        }
    }
}

/// Announce the equity; `snapshot` also stores it (new candles do, ticks
/// between candles only refresh the UI).
fn emit_equity(app_handle: &AppHandle, runtime: &BotRuntime, snapshot: bool) {
    let timestamp = Utc::now().to_rfc3339();
    let equity = runtime.equity();
    if snapshot {
        persist_equity_snapshot(app_handle, &runtime.bot_id, &timestamp, equity, runtime.mode.as_str());
    }
    let _ = app_handle.emit(
        "bot:equity",
        json!({
            "bot_id": runtime.bot_id,
            "timestamp": timestamp,
            "equity": equity,
            "last_price": runtime.last_price,
            "pair": runtime.pair,
            "balance": runtime.balance,
            "open_position_count": runtime.open_positions.len(),
            "trading_mode": runtime.mode.as_str(),
        }),
    );
}

fn notify_strategy_trade(
    stdin: &Arc<Mutex<ChildStdin>>,
    runtime: &BotRuntime,
    trade_id: &str,
    pair: &str,
    side: &str,
    price: f64,
    quantity: f64,
    pnl: f64,
    action: &str,
) {
    let _ = write_json_line(
        stdin,
        json!({
            "method": "on_trade",
            "params": {
                "id": trade_id,
                "pair": pair,
                "side": side,
                "price": price,
                "quantity": quantity,
                "time": Utc::now().to_rfc3339(),
                "pnl": pnl,
                "action": action,
                "balance": runtime.balance_json(),
                "positions": runtime.positions_json(),
                "mark_price": runtime.last_price,
            }
        }),
    );
}

/// Apply what the engine asked for, in order (see `runtime::Effect`).
fn apply_effects(
    app_handle: &AppHandle,
    stdin: &Arc<Mutex<ChildStdin>>,
    runtime: &BotRuntime,
    effects: Vec<Effect>,
) {
    for effect in effects {
        match effect {
            Effect::Log { level, message } => push_bot_log_for(app_handle, Some(&runtime.bot_id), level, message),
            Effect::Trade(trade) => {
                if let Err(err) = persist_trade(app_handle, &trade) {
                    push_bot_log_for(app_handle, Some(&runtime.bot_id), "error", format!("Journal write failed: {err}"));
                }
                let _ = app_handle.emit("bot:trade", json!({ "bot_id": runtime.bot_id, "trade": *trade }));
            }
            Effect::Notify { trade_id, pair, side, price, quantity, pnl, action } => {
                notify_strategy_trade(stdin, runtime, &trade_id, &pair, &side, price, quantity, pnl, action);
            }
            Effect::Equity => emit_equity(app_handle, runtime, true),
        }
    }
}

/// The register believes what it is told: Running while any bot runs.
fn sync_register(app_handle: &AppHandle) {
    let running = app_handle
        .try_state::<AppState>()
        .and_then(|state| state.bots.lock().ok().map(|bots| bots.len()))
        .unwrap_or(0);
    if running > 0 {
        qs_core::processes::mark_running(app_handle, BOTS_PROCESS_ID, None);
    } else {
        qs_core::processes::mark_stopped(app_handle, BOTS_PROCESS_ID);
    }
}

// ---------------------------------------------------------------------------
// The strategy's JSON-RPC
// ---------------------------------------------------------------------------

fn handle_strategy_rpc_line(
    app_handle: &AppHandle,
    stdin: &Arc<Mutex<ChildStdin>>,
    runtime: &Arc<Mutex<BotRuntime>>,
    bot_id: &str,
    line: &str,
) {
    let parsed = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) => {
            push_bot_log_for(app_handle, Some(bot_id), "info", line.to_string());
            return;
        }
    };
    let method = parsed.get("method").and_then(|v| v.as_str()).unwrap_or_default();
    let params = parsed.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "log" => {
            let level = params.get("level").and_then(|v| v.as_str()).unwrap_or("info");
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Strategy log")
                .to_string();
            push_bot_log_for(app_handle, Some(bot_id), level, message);
        }
        "buy" | "sell" | "reverse" => {
            let mut rt = match runtime.lock() {
                Ok(rt) => rt,
                Err(err) => {
                    push_bot_log_for(app_handle, Some(bot_id), "error", format!("Bot runtime lock failed: {err}"));
                    return;
                }
            };
            let request = match rt.parse_order(method, &params) {
                Ok(request) => request,
                Err(err) => {
                    push_bot_log_for(app_handle, Some(bot_id), "warn", err);
                    return;
                }
            };
            let now = Utc::now().to_rfc3339();
            let effects = rt.place(request, &now);
            apply_effects(app_handle, stdin, &rt, effects);
        }
        "close" => {
            let position_id = params
                .get("position_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let mut rt = match runtime.lock() {
                Ok(rt) => rt,
                Err(err) => {
                    push_bot_log_for(app_handle, Some(bot_id), "error", format!("Bot runtime lock failed: {err}"));
                    return;
                }
            };
            let now = Utc::now().to_rfc3339();
            let effects = rt.close(position_id.as_deref(), &now);
            apply_effects(app_handle, stdin, &rt, effects);
        }
        "cancel" => push_bot_log_for(app_handle, Some(bot_id), "info", "Cancel request received."),
        "get_candles" => {}
        _ => push_bot_log_for(app_handle, Some(bot_id), "info", line.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Preflight
// ---------------------------------------------------------------------------

fn add_preflight_check(
    checks: &mut Vec<Value>,
    has_fatal: &mut bool,
    app_handle: &AppHandle,
    bot_id: Option<&str>,
    id: &str,
    label: &str,
    status: &str,
    message: impl Into<String>,
) {
    let message = message.into();
    if status == "error" {
        *has_fatal = true;
    }
    let log_level = match status {
        "error" => "error",
        "warn" => "warn",
        _ => "info",
    };
    push_bot_log_for(app_handle, bot_id, log_level, format!("Preflight {label}: {message}"));
    checks.push(json!({ "id": id, "label": label, "status": status, "message": message }));
}

fn validate_strategy_with_runner(
    python_path: &str,
    strategy_path: &str,
) -> Result<(String, Vec<String>), String> {
    let mut cmd = build_python_command(python_path);
    let output = cmd
        .arg("-m")
        .arg("quantalgo.runner")
        .arg(strategy_path)
        .arg("--validate")
        .output()
        .map_err(|e| format!("Strategy validation failed to start: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let parsed = stdout
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .next();

    if output.status.success() {
        if let Some(ref value) = parsed {
            if value.get("ok").and_then(|flag| flag.as_bool()).unwrap_or(false) {
                let class_name = value.get("class_name").and_then(|item| item.as_str()).unwrap_or("Strategy");
                let warnings = value
                    .get("warnings")
                    .and_then(|item| item.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(|value| value.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return Ok((format!("Strategy validated: {class_name}"), warnings));
            }
        }
    }

    let runner_error = parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let detail = runner_error
        .or_else(|| {
            let combined = format!("{}\n{}", stderr.trim(), stdout.trim());
            if combined.trim().is_empty() {
                None
            } else {
                Some(combined)
            }
        })
        .unwrap_or_else(|| format!("Runner exited with status {:?}", output.status.code()));
    Err(detail)
}

/// The risk numbers a bot runs with: its own overrides, else the settings.
#[derive(Debug, Clone)]
pub(crate) struct RunConfig {
    pub risk_per_trade: f64,
    pub max_positions: u64,
    pub slippage: f64,
    pub fee: f64,
    pub warmup_candles: usize,
    pub strategy_params: Option<Value>,
}

impl RunConfig {
    pub(crate) fn resolve(bot: &Bot, state: &AppState) -> Result<Self, String> {
        let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        let overrides: Value = bot
            .config_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        let num = |key: &str, fallback: f64| overrides.get(key).and_then(|v| v.as_f64()).unwrap_or(fallback);
        Ok(Self {
            risk_per_trade: num("risk_per_trade", settings.risk_per_trade),
            max_positions: overrides
                .get("max_positions")
                .and_then(|v| v.as_u64())
                .unwrap_or(settings.max_concurrent_positions as u64),
            slippage: num("slippage", settings.slippage_tolerance),
            fee: overrides
                .get("fee")
                .or_else(|| overrides.get("paper_fee_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(settings.paper_fee_pct),
            warmup_candles: overrides
                .get("warmup_candles")
                .and_then(|v| v.as_u64())
                .map(|n| (n as usize).clamp(2, MAX_WARMUP_CANDLES))
                .unwrap_or(DEFAULT_WARMUP_CANDLES),
            strategy_params: overrides.get("strategy_params").cloned(),
        })
    }

    fn validate(&self) -> Result<(), String> {
        if !(self.risk_per_trade > 0.0 && self.risk_per_trade <= MAX_RISK_PER_TRADE_PCT) {
            return Err(format!("Risk per trade must be greater than 0 and no more than {MAX_RISK_PER_TRADE_PCT:.0}%."));
        }
        if !(1..=MAX_CONCURRENT_POSITIONS).contains(&self.max_positions) {
            return Err(format!("Max positions must be between 1 and {MAX_CONCURRENT_POSITIONS}."));
        }
        if !(0.0..=MAX_SLIPPAGE_TOLERANCE_PCT).contains(&self.slippage) {
            return Err(format!("Slippage must be between 0 and {MAX_SLIPPAGE_TOLERANCE_PCT:.0} percent."));
        }
        if !(0.0..=MAX_PAPER_FEE_PCT).contains(&self.fee) {
            return Err(format!("Paper fee must be between 0 and {MAX_PAPER_FEE_PCT:.0} percent."));
        }
        Ok(())
    }
}

fn check_run_config(
    checks: &mut Vec<Value>,
    has_fatal: &mut bool,
    app_handle: &AppHandle,
    bot_id: Option<&str>,
    bot: &Bot,
    run: &RunConfig,
) {
    let live = matches!(validate_mode(&bot.trading_mode), Ok(Mode::Live));
    let timeframe_ok = validate_timeframe(&bot.timeframe).is_ok();
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "timeframe", "Timeframe",
        if timeframe_ok { "ok" } else { "error" },
        if timeframe_ok { format!("Timeframe: {}", bot.timeframe) } else { "Choose a supported timeframe".to_string() },
    );
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "budget", "Budget",
        if bot.budget >= 100.0 { "ok" } else { "error" },
        if bot.budget >= 100.0 { format!("Budget: {:.2} {}", bot.budget, bot.pair.split('/').nth(1).unwrap_or("")) } else { "The budget must be at least 100".to_string() },
    );
    let risk_ok = run.risk_per_trade > 0.0 && run.risk_per_trade <= MAX_RISK_PER_TRADE_PCT;
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "risk_per_trade", "Risk per trade",
        if risk_ok { if run.risk_per_trade > WARN_RISK_PER_TRADE_PCT { "warn" } else { "ok" } } else { "error" },
        if risk_ok { format!("Risk per trade: {:.2}%", run.risk_per_trade) } else { format!("Risk per trade must be greater than 0 and no more than {MAX_RISK_PER_TRADE_PCT:.0}%") },
    );
    let positions_ok = (1..=MAX_CONCURRENT_POSITIONS).contains(&run.max_positions);
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "max_positions", "Max positions",
        if positions_ok { if run.max_positions > WARN_CONCURRENT_POSITIONS { "warn" } else { "ok" } } else { "error" },
        if positions_ok { format!("Max open positions: {}", run.max_positions) } else { format!("Max positions must be between 1 and {MAX_CONCURRENT_POSITIONS}") },
    );
    let slippage_ok = (0.0..=MAX_SLIPPAGE_TOLERANCE_PCT).contains(&run.slippage);
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "slippage", "Slippage",
        if slippage_ok { if run.slippage > WARN_SLIPPAGE_TOLERANCE_PCT { "warn" } else { "ok" } } else { "error" },
        if slippage_ok { if live { format!("Slippage tolerance {:.4}% — paper only; live fills are booked as the venue reports them", run.slippage) } else { format!("Slippage tolerance: {:.4}%", run.slippage) } } else { format!("Slippage must be between 0 and {MAX_SLIPPAGE_TOLERANCE_PCT:.0} percent") },
    );
    let fee_ok = (0.0..=MAX_PAPER_FEE_PCT).contains(&run.fee);
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "paper_fee", "Fee",
        if fee_ok { if run.fee > WARN_PAPER_FEE_PCT { "warn" } else { "ok" } } else { "error" },
        if fee_ok { if live { format!("Fee {:.4}% — paper only; live fees are booked as the venue charges them", run.fee) } else { format!("Fee: {:.4}%", run.fee) } } else { format!("Fee must be between 0 and {MAX_PAPER_FEE_PCT:.0} percent") },
    );
    if live {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "position_model", "Position model", "warn",
            "Spot live trading is long-only: a sell signal closes the long, a short is never opened",
        );
    } else {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "position_model", "Position model", "ok",
            "One open position per pair and side; shorts reserve 100% collateral",
        );
    }
}

/// Validate a strategy on its own (the Strategies page's "Validate").
#[tauri::command]
pub(crate) async fn validate_strategy(
    strategy_id: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let mut checks: Vec<Value> = Vec::new();
    let mut has_fatal = false;

    let strategy_row: Option<(String, Option<String>)> = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT file_path, params_json FROM strategies WHERE id = ?1",
            params![strategy_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .ok()
    };
    let Some((strategy_file, strategy_params_json)) = strategy_row else {
        add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_exists", "Strategy file", "error", "Strategy not found in database");
        return Ok(json!({ "checks": checks, "can_start": false }));
    };

    let file_exists = std::path::Path::new(&strategy_file).exists();
    add_preflight_check(
        &mut checks, &mut has_fatal, &app_handle, None, "strategy_exists", "Strategy file",
        if file_exists { "ok" } else { "error" },
        if file_exists { "Strategy file found and readable".to_string() } else { format!("Strategy file not found: {strategy_file}") },
    );

    let python_path = {
        let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        resolve_python_path(&settings)
    };
    let python_ok = {
        let mut probe = Command::new(&python_path);
        hide_console_window(&mut probe);
        probe.arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    };
    add_preflight_check(
        &mut checks, &mut has_fatal, &app_handle, None, "python_available", "Python runtime",
        if python_ok { "ok" } else { "error" },
        if python_ok { format!("Python found at '{python_path}'") } else { format!("Python not found at '{python_path}'. Configure it in Settings.") },
    );

    if python_ok && file_exists {
        match validate_strategy_with_runner(&python_path, &strategy_file) {
            Ok((message, warnings)) => {
                add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_runner_validation", "Strategy runner validation", "ok", message);
                for warning in warnings {
                    add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_selection", "Strategy selection", "warn", warning);
                }
            }
            Err(message) => add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_runner_validation", "Strategy runner validation", "error", message),
        }
    }

    match strategy_params_json.as_deref() {
        Some(params_json) => match serde_json::from_str::<Value>(params_json) {
            Ok(_) => add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_params", "Strategy params", "ok", "Strategy params JSON is valid"),
            Err(err) => add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_params", "Strategy params", "error", format!("Strategy params JSON is invalid: {err}")),
        },
        None => add_preflight_check(&mut checks, &mut has_fatal, &app_handle, None, "strategy_params", "Strategy params", "ok", "No custom strategy params"),
    }

    Ok(json!({ "checks": checks, "can_start": !has_fatal }))
}

/// Everything a bot needs to start, checked without starting it. Takes a
/// draft (the create modal) or an existing bot's id.
fn preflight_bot(app_handle: &AppHandle, state: &AppState, bot: &Bot, persisted: bool) -> Result<Value, String> {
    let mut checks: Vec<Value> = Vec::new();
    let mut has_fatal = false;
    let bot_id = if persisted { Some(bot.id.as_str()) } else { None };

    push_bot_log_for(
        app_handle,
        bot_id,
        "info",
        format!("Running deploy preflight for '{}' — {} on {} ({}).", bot.name, bot.strategy_id, bot.pair, bot.trading_mode),
    );

    // Strategy
    let strategy_row: Option<(String, Option<String>)> = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT file_path, params_json FROM strategies WHERE id = ?1",
            params![bot.strategy_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .ok()
    };
    let strategy_file = strategy_row.as_ref().map(|(path, _)| path.clone());
    let strategy_params_json = strategy_row.as_ref().and_then(|(_, p)| p.clone());
    match strategy_file.as_deref() {
        Some(path) if std::path::Path::new(path).exists() => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_exists", "Strategy file exists", "ok", "Strategy file found and readable"),
        Some(path) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_exists", "Strategy file exists", "error", format!("Strategy file not found: {path}")),
        None => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_exists", "Strategy file exists", "error", "Strategy not found in database"),
    }

    // Python
    let python_path = {
        let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        resolve_python_path(&settings)
    };
    let python_ok = {
        let mut probe = Command::new(&python_path);
        hide_console_window(&mut probe);
        probe.arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    };
    add_preflight_check(
        &mut checks, &mut has_fatal, app_handle, bot_id, "python_available", "Python runtime",
        if python_ok { "ok" } else { "error" },
        if python_ok { format!("Python found at '{python_path}'") } else { format!("Python not found at '{python_path}'. Configure it in Settings.") },
    );
    if python_ok {
        if let Some(path) = strategy_file.as_deref().filter(|p| std::path::Path::new(p).exists()) {
            match validate_strategy_with_runner(&python_path, path) {
                Ok((message, warnings)) => {
                    add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_runner_validation", "Strategy runner validation", "ok", message);
                    for warning in warnings {
                        add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_selection", "Strategy selection", "warn", warning);
                    }
                }
                Err(message) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_runner_validation", "Strategy runner validation", "error", message),
            }
        }
    }
    match strategy_params_json.as_deref() {
        Some(params_json) => match serde_json::from_str::<Value>(params_json) {
            Ok(_) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_params", "Strategy params", "ok", "Strategy params JSON is valid"),
            Err(err) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_params", "Strategy params", "error", format!("Strategy params JSON is invalid: {err}")),
        },
        None => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "strategy_params", "Strategy params", "ok", "No custom strategy params"),
    }

    // Exchange
    let exchange_provider: Option<String> = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT provider FROM exchanges WHERE id = ?1",
            params![bot.exchange_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
    };
    match exchange_provider.as_deref() {
        Some(provider) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "exchange_configured", "Exchange configured", "ok", format!("Connected exchange found ({provider})")),
        None => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "exchange_configured", "Exchange configured", "error", "The selected exchange is not connected"),
    }

    // Pair
    if bot.pair.trim().is_empty() {
        add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "pair_selected", "Trading pair", "error", "No trading pair selected");
    } else {
        add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "pair_selected", "Trading pair", "ok", format!("Pair: {}", bot.pair));
        if let Some(provider) = exchange_provider.as_deref() {
            match fetch_exchange_pairs_for_provider(provider) {
                Ok(pairs) if pairs.iter().any(|p| p == &bot.pair) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "pair_supported", "Trading pair support", "ok", format!("{} is listed by {provider}", bot.pair)),
                Ok(_) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "pair_supported", "Trading pair support", "error", format!("{} was not found in {provider} market metadata", bot.pair)),
                Err(err) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "pair_supported", "Trading pair support", "error", format!("Could not verify pair support: {err}")),
            }
        }
    }

    // Not already running
    {
        let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        if persisted && bots.contains_key(&bot.id) {
            add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "bot_not_running", "Bot available", "error", "This bot is already running. Stop it first.");
        } else {
            add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "bot_not_running", "Bot available", "ok", format!("{} bot(s) running; this one can start", bots.len()));
        }
    }

    // Mode
    let mode = validate_mode(&bot.trading_mode);
    match &mode {
        Ok(Mode::Paper) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "trading_mode", "Trading mode", "ok", "Paper trading mode — simulated fills on the exchange's live candles"),
        Ok(Mode::Live) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "trading_mode", "Trading mode", "warn", "LIVE trading mode — real market orders on the exchange"),
        Err(err) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "trading_mode", "Trading mode", "error", err.clone()),
    }

    // Risk numbers
    let run = RunConfig::resolve(bot, state)?;
    check_run_config(&mut checks, &mut has_fatal, app_handle, bot_id, bot, &run);

    // Live gates (PLAN-QUANTALGO §4.4): venue, credentials, limits, budget, dry run
    if matches!(mode, Ok(Mode::Live)) {
        preflight_live(&mut checks, &mut has_fatal, app_handle, bot_id, state, bot, &run);
    }

    // Market data
    if !bot.pair.trim().is_empty() && validate_timeframe(&bot.timeframe).is_ok() {
        if let Some(provider) = exchange_provider.as_deref() {
            match fetch_latest_market_candle(provider, &bot.pair, &bot.timeframe) {
                Ok(candle) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "market_data", "Public market data", "ok", format!("{provider} returned {} {} close {:.4}", bot.pair, bot.timeframe, candle.close)),
                Err(err) => add_preflight_check(&mut checks, &mut has_fatal, app_handle, bot_id, "market_data", "Public market data", "error", format!("Could not load public market data: {err}")),
            }
        }
    }

    push_bot_log_for(
        app_handle,
        bot_id,
        if has_fatal { "error" } else { "info" },
        if has_fatal { "Deploy preflight blocked start; resolve error checks first." } else { "Deploy preflight passed." },
    );
    Ok(json!({ "checks": checks, "can_start": !has_fatal }))
}

/// The live gates of the preflight (PLAN-QUANTALGO §4.4). Each stops at the
/// first failure that makes the later ones meaningless; nothing here places
/// an order — Binance's dry run validates one without executing it.
fn preflight_live(
    checks: &mut Vec<Value>,
    has_fatal: &mut bool,
    app_handle: &AppHandle,
    bot_id: Option<&str>,
    state: &AppState,
    bot: &Bot,
    run: &RunConfig,
) {
    let creds = match state.db.lock() {
        Ok(db) => load_credentials(&db, &bot.exchange_id),
        Err(e) => Err(format!("Lock: {e}")),
    };
    let creds = match creds {
        Ok(creds) => creds,
        Err(err) => {
            add_preflight_check(checks, has_fatal, app_handle, bot_id, "live_credentials", "Exchange credentials", "error", err);
            return;
        }
    };
    let Some(venue) = Venue::parse(&creds.provider) else {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_venue", "Live venue", "error",
            format!("Live order routing is implemented for Binance and Bybit spot; {} bots run in paper mode only.", creds.provider),
        );
        return;
    };
    add_preflight_check(
        checks, has_fatal, app_handle, bot_id, "live_venue", "Live venue", if creds.sandbox { "ok" } else { "warn" },
        if creds.sandbox {
            format!("{} — sandbox environment (testnet / demo), no real funds", venue.label())
        } else {
            format!("{} — PRODUCTION account, real funds", venue.label())
        },
    );
    if creds.api_key.is_empty() || creds.api_secret.is_empty() {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_credentials", "Exchange credentials", "error",
            "The exchange has no API key and secret stored; add them on the Exchange page.",
        );
        return;
    }
    let broker = match LiveBroker::new(&creds.provider, &creds.api_key, &creds.api_secret, creds.sandbox) {
        Ok(broker) => broker,
        Err(err) => {
            add_preflight_check(checks, has_fatal, app_handle, bot_id, "live_credentials", "Exchange credentials", "error", err);
            return;
        }
    };
    let quote = bot.pair.split('/').nth(1).unwrap_or("USDT").to_string();
    let available = match broker.available_balance(&quote) {
        Ok(available) => {
            add_preflight_check(
                checks, has_fatal, app_handle, bot_id, "live_credentials", "Exchange credentials", "ok",
                format!("Authenticated with {}; {available:.2} {quote} available", venue.label()),
            );
            available
        }
        Err(err) => {
            add_preflight_check(
                checks, has_fatal, app_handle, bot_id, "live_credentials", "Exchange credentials", "error",
                format!("Authentication with {} failed: {err}", venue.label()),
            );
            return;
        }
    };
    let filters = match broker.symbol_filters(&bot.pair) {
        Ok(filters) => {
            add_preflight_check(
                checks, has_fatal, app_handle, bot_id, "live_filters", "Order limits", "ok",
                format!(
                    "{}: lot step {} {}, minimum order {} {}",
                    filters.symbol, filters.qty_string(filters.qty_step), filters.base, filters.quote_string(filters.min_notional), filters.quote
                ),
            );
            filters
        }
        Err(err) => {
            add_preflight_check(
                checks, has_fatal, app_handle, bot_id, "live_filters", "Order limits", "error",
                format!("Could not load the pair's order limits: {err}"),
            );
            return;
        }
    };
    if bot.budget > available + 1e-9 {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_budget", "Budget covered", "error",
            format!("Budget {:.2} {quote} exceeds the {available:.2} {quote} available on the exchange.", bot.budget),
        );
    } else {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_budget", "Budget covered", "ok",
            format!("Budget {:.2} {quote} of {available:.2} {quote} available", bot.budget),
        );
    }
    let first_order = bot.budget * run.risk_per_trade / 100.0;
    if first_order < filters.min_notional {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_order_size", "Order size", "error",
            format!(
                "Budget × risk per trade = {first_order:.2} {quote}, below the venue minimum of {:.2} {quote}. Raise the budget or the risk per trade.",
                filters.min_notional
            ),
        );
    } else {
        add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_order_size", "Order size", "ok",
            format!("Orders of at least {first_order:.2} {quote} (venue minimum {:.2} {quote})", filters.min_notional),
        );
    }
    match broker.test_market_buy(&filters, first_order.max(filters.min_notional)) {
        Ok(message) => add_preflight_check(checks, has_fatal, app_handle, bot_id, "live_dry_run", "Order dry run", "ok", message),
        Err(err) => add_preflight_check(
            checks, has_fatal, app_handle, bot_id, "live_dry_run", "Order dry run", "error",
            format!("The venue rejected a test order: {err}"),
        ),
    }
}

/// Build the live broker for a bot that is about to start: the same gates as
/// the preflight, failing hard (PLAN-QUANTALGO §4.4).
fn connect_live(
    app_handle: &AppHandle,
    state: &AppState,
    bot: &Bot,
    run: &RunConfig,
) -> Result<(Box<dyn Broker>, SymbolFilters), String> {
    let creds = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_credentials(&db, &bot.exchange_id)?
    };
    let venue = Venue::parse(&creds.provider).ok_or_else(|| {
        format!("Live order routing is implemented for Binance and Bybit spot; {} bots run in paper mode only.", creds.provider)
    })?;
    if creds.api_key.is_empty() || creds.api_secret.is_empty() {
        return Err("The exchange has no API key and secret stored; add them on the Exchange page.".into());
    }
    let broker = LiveBroker::new(&creds.provider, &creds.api_key, &creds.api_secret, creds.sandbox)?;
    let quote = bot.pair.split('/').nth(1).unwrap_or("USDT").to_string();
    let available = broker
        .available_balance(&quote)
        .map_err(|e| format!("Authentication with {} failed: {e}", venue.label()))?;
    if bot.budget > available + 1e-9 {
        return Err(format!(
            "Budget {:.2} {quote} exceeds the {available:.2} {quote} available on the exchange.",
            bot.budget
        ));
    }
    let filters = broker
        .symbol_filters(&bot.pair)
        .map_err(|e| format!("Could not load the pair's order limits: {e}"))?;
    let first_order = bot.budget * run.risk_per_trade / 100.0;
    if first_order < filters.min_notional {
        return Err(format!(
            "Budget × risk per trade = {first_order:.2} {quote}, below the venue minimum of {:.2} {quote}.",
            filters.min_notional
        ));
    }
    push_bot_log_for(
        app_handle,
        Some(&bot.id),
        "warn",
        format!(
            "LIVE bot on {}{}: budget {:.2} {quote} of {available:.2} available; {} lot step {} {}, minimum order {} {quote}; long-only.",
            venue.label(),
            if creds.sandbox { " (sandbox)" } else { " (PRODUCTION)" },
            bot.budget,
            filters.symbol,
            filters.qty_string(filters.qty_step),
            filters.base,
            filters.quote_string(filters.min_notional),
        ),
    );
    Ok((Box::new(broker), filters))
}

/// A draft as a bot row that was never inserted — the preflight of the
/// create modal runs on this.
fn draft_bot(draft: &BotDraft) -> Bot {
    let now = Utc::now().to_rfc3339();
    Bot {
        id: String::new(),
        name: draft.name.clone().unwrap_or_default(),
        strategy_id: draft.strategy_id.clone(),
        exchange_id: draft.exchange_id.clone(),
        pair: draft.pair.clone(),
        timeframe: draft.timeframe.clone(),
        trading_mode: draft.trading_mode.clone(),
        budget: draft.budget,
        status: "stopped".into(),
        started_at: None,
        stopped_at: None,
        last_error: None,
        config_json: draft.config.as_ref().map(|v| v.to_string()),
        created_at: now.clone(),
        updated_at: now,
    }
}

/// The deploy preflight: `bot_id` for an existing bot, `draft` for one the
/// create modal has not saved yet. The old positional arguments
/// (`strategy_id`, `exchange_id`, `pair`, `trading_mode`, `config`) are still
/// accepted and folded into a draft.
#[tauri::command]
pub(crate) async fn validate_bot_deploy(
    bot_id: Option<String>,
    draft: Option<BotDraft>,
    strategy_id: Option<String>,
    exchange_id: Option<String>,
    pair: Option<String>,
    trading_mode: Option<String>,
    config: Option<Value>,
    app_handle: AppHandle,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        if let Some(id) = bot_id {
            let bot = {
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                load_bot(&db, &id)?
            };
            return preflight_bot(&app_handle, &state, &bot, true);
        }
        let draft = match draft {
            Some(d) => d,
            None => legacy_draft(strategy_id, exchange_id, pair, trading_mode, config)?,
        };
        preflight_bot(&app_handle, &state, &draft_bot(&draft), false)
    })
    .await
    .map_err(|e| format!("Preflight task failed: {e}"))?
}

/// The deploy modal of the single-bot era sent the fields loose; they still
/// make a draft (timeframe, balance and risk numbers ride in `config`).
fn legacy_draft(
    strategy_id: Option<String>,
    exchange_id: Option<String>,
    pair: Option<String>,
    trading_mode: Option<String>,
    config: Option<Value>,
) -> Result<BotDraft, String> {
    let config = config.unwrap_or(Value::Null);
    let timeframe = config
        .get("timeframe")
        .and_then(|v| v.as_str())
        .unwrap_or("1h")
        .to_string();
    let budget = config
        .get("initial_balance")
        .or_else(|| config.get("budget"))
        .and_then(|v| v.as_f64())
        .unwrap_or(10_000.0);
    Ok(BotDraft {
        name: None,
        strategy_id: strategy_id.ok_or("A strategy is required.")?,
        exchange_id: exchange_id.ok_or("An exchange is required.")?,
        pair: pair.ok_or("A trading pair is required.")?,
        timeframe,
        trading_mode: trading_mode.unwrap_or_else(|| "paper".into()),
        budget,
        config: if config.is_null() { None } else { Some(config) },
    })
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Start a bot by id. Without `bot_id` the single-bot era's arguments create
/// the bot first (the deploy modal until card C lands). Never auto-approved
/// through MCP: `quantsuite.algo.start_bot` is `external`.
#[tauri::command]
pub(crate) async fn start_bot(
    bot_id: Option<String>,
    strategy_id: Option<String>,
    exchange_id: Option<String>,
    pair: Option<String>,
    config: Option<Value>,
    trading_mode: Option<String>,
    app_handle: AppHandle,
) -> Result<Bot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let bot = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            match bot_id {
                Some(id) => load_bot(&db, &id)?,
                None => insert_bot(&db, &legacy_draft(strategy_id, exchange_id, pair, trading_mode, config)?)?,
            }
        };
        start_bot_inner(&app_handle, &state, bot)
    })
    .await
    .map_err(|e| format!("Start task failed: {e}"))?
}

fn strategy_warmup_candles(configured: usize, params: &Value) -> usize {
    let requested = params.get("warmup_candles").and_then(Value::as_u64)
        .unwrap_or(0).min(MAX_WARMUP_CANDLES as u64) as usize;
    configured.max(requested).clamp(2, MAX_WARMUP_CANDLES)
}

#[cfg(test)]
mod smithery_warmup_tests {
    use super::*;

    #[test]
    fn registry_warmup_is_honored_without_reducing_a_larger_bot_setting() {
        assert_eq!(strategy_warmup_candles(200, &json!({"warmup_candles": 1000})), 1000);
        assert_eq!(strategy_warmup_candles(2000, &json!({"warmup_candles": 1000})), 2000);
        assert_eq!(strategy_warmup_candles(200, &json!({})), 200);
        assert_eq!(strategy_warmup_candles(200, &json!({"warmup_candles": u64::MAX})), MAX_WARMUP_CANDLES);
    }
}

fn start_bot_inner(app_handle: &AppHandle, state: &AppState, bot: Bot) -> Result<Bot, String> {
    let mode = validate_mode(&bot.trading_mode)?;
    validate_timeframe(&bot.timeframe)?;
    if bot.pair.trim().is_empty() {
        return Err("Trading pair is required.".into());
    }
    if bot.budget < 100.0 {
        return Err("The budget must be at least 100.".into());
    }
    let run = RunConfig::resolve(&bot, state)?;
    run.validate()?;
    {
        let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        if bots.contains_key(&bot.id) {
            return Err("This bot is already running. Stop it first.".into());
        }
    }

    let (file_path, strategy_params_json) = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT file_path, params_json FROM strategies WHERE id = ?1",
            params![bot.strategy_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(|e| format!("Strategy not found: {e}"))?
    };
    let exchange_provider = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT provider FROM exchanges WHERE id = ?1",
            params![bot.exchange_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| format!("Exchange not found: {e}"))?
    };
    let pairs = fetch_exchange_pairs_for_provider(&exchange_provider)?;
    if !pairs.iter().any(|p| p == &bot.pair) {
        return Err(format!("{} was not found in {exchange_provider} market metadata.", bot.pair));
    }

    // Live: the venue, keys, limits and balance are checked before a runner
    // process exists; a failure here starts nothing (PLAN-QUANTALGO §4.4).
    let (live_broker, live_filters): (Option<Box<dyn Broker>>, Option<SymbolFilters>) = if mode == Mode::Live {
        let (broker, filters) = connect_live(app_handle, state, &bot, &run)
            .inspect_err(|message| emit_bot_error(app_handle, Some(&bot.id), message.clone(), None))?;
        (Some(broker), Some(filters))
    } else {
        (None, None)
    };

    let python_path = {
        let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        resolve_python_path(&settings)
    };

    let mut strategy_params = strategy_params_json
        .as_deref()
        .map(serde_json::from_str::<Value>)
        .transpose()
        .map_err(|err| format!("Strategy params JSON is invalid: {err}"))?
        .unwrap_or_else(|| json!({}));
    if let (Some(base), Some(overrides)) = (strategy_params.as_object_mut(), run.strategy_params.as_ref().and_then(|v| v.as_object())) {
        for (k, v) in overrides {
            base.insert(k.clone(), v.clone());
        }
    }
    let warmup_candles = strategy_warmup_candles(run.warmup_candles, &strategy_params);
    let mut startup_candles =
        match fetch_recent_market_candles(&exchange_provider, &bot.pair, &bot.timeframe, warmup_candles) {
            Ok(candles) => candles,
            Err(err) => {
                push_bot_log_for(app_handle, Some(&bot.id), "warn", format!("Could not load strategy warm-up candles: {err}"));
                Vec::new()
            }
        };
    let initial_candle = match startup_candles.pop() {
        Some(candle) => candle,
        None => fetch_latest_market_candle(&exchange_provider, &bot.pair, &bot.timeframe)?,
    };
    let warmup_json = startup_candles
        .iter()
        .map(|c| market_candle_json(c, &bot.pair))
        .collect::<Vec<_>>();

    let mut command = build_python_command(&python_path);
    command
        .arg("-m")
        .arg("quantalgo.runner")
        .arg(&file_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| {
        let message = format!("Failed to spawn strategy runner: {e}");
        emit_bot_error(app_handle, Some(&bot.id), message.clone(), None);
        message
    })?;
    let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or_else(|| "Bot stdin unavailable".to_string())?));
    let stdout = child.stdout.take().ok_or_else(|| "Bot stdout unavailable".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "Bot stderr unavailable".to_string())?;
    let child = Arc::new(Mutex::new(child));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let runtime = Arc::new(Mutex::new(BotRuntime {
        bot_id: bot.id.clone(),
        strategy_id: bot.strategy_id.clone(),
        exchange_id: bot.exchange_id.clone(),
        pair: bot.pair.clone(),
        mode,
        balance: bot.budget,
        last_price: initial_candle.close,
        open_positions: Vec::new(),
        fee_rate: run.fee / 100.0,
        slippage_pct: run.slippage,
        risk_per_trade: run.risk_per_trade,
        max_positions: run.max_positions as usize,
        broker: live_broker,
        filters: live_filters,
    }));

    // stdout: the strategy's RPC, with the startup acknowledgement first
    let (startup_tx, startup_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    {
        let app = app_handle.clone();
        let stdin = Arc::clone(&stdin);
        let runtime = Arc::clone(&runtime);
        let tx = startup_tx.clone();
        let bot_id = bot.id.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line_result in reader.lines() {
                match line_result {
                    Ok(line) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&line) {
                            if value.get("id").and_then(|v| v.as_str()) == Some("startup") {
                                let result = match value.get("error") {
                                    Some(error) => Err(error
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Strategy startup failed")
                                        .to_string()),
                                    None => Ok(()),
                                };
                                let _ = tx.send(result);
                                continue;
                            }
                        }
                        handle_strategy_rpc_line(&app, &stdin, &runtime, &bot_id, &line);
                    }
                    Err(err) => {
                        push_bot_log_for(&app, Some(&bot_id), "error", format!("stdout read failed: {err}"));
                        emit_bot_error(&app, Some(&bot_id), "Bot stdout reader failed.", Some(err.to_string()));
                        let _ = tx.send(Err(format!("stdout read failed: {err}")));
                        break;
                    }
                }
            }
            let _ = tx.send(Err("Bot process stdout closed before startup acknowledgement.".into()));
        });
    }
    // stderr: the runner's log lines
    {
        let app = app_handle.clone();
        let bot_id = bot.id.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line_result in reader.lines() {
                match line_result {
                    Ok(line) => {
                        let level = if line.contains("ERROR") {
                            "error"
                        } else if line.contains("WARN") {
                            "warn"
                        } else {
                            "info"
                        };
                        push_bot_log_for(&app, Some(&bot_id), level, line);
                    }
                    Err(err) => {
                        push_bot_log_for(&app, Some(&bot_id), "error", format!("stderr read failed: {err}"));
                        break;
                    }
                }
            }
        });
    }

    let quote_asset = bot.pair.split('/').nth(1).unwrap_or("USDT").to_string();
    write_json_line(
        &stdin,
        json!({
            "id": "startup",
            "method": "start",
            "params": {
                "params": strategy_params,
                "pair": bot.pair,
                "balance": { quote_asset: bot.budget },
                "positions": {},
                "mark_price": initial_candle.close,
                "candles": warmup_json,
            }
        }),
    )?;
    match startup_rx.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
            emit_bot_error(app_handle, Some(&bot.id), "Strategy startup failed.", Some(err.clone()));
            return Err(format!("Strategy startup failed: {err}"));
        }
        Err(_) => {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
            let message = "Timed out waiting for strategy startup acknowledgement.".to_string();
            emit_bot_error(app_handle, Some(&bot.id), message.clone(), None);
            return Err(message);
        }
    }
    write_json_line(
        &stdin,
        json!({ "method": "on_candle", "params": market_candle_json(&initial_candle, &bot.pair) }),
    )?;

    {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        set_bot_status(&db, &bot.id, "running", None)?;
    }
    {
        let mut bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        bots.insert(
            bot.id.clone(),
            BotHandle { child: Arc::clone(&child), stdin: Arc::clone(&stdin), stop_flag: Arc::clone(&stop_flag), runtime: Arc::clone(&runtime) },
        );
    }
    sync_register(app_handle);
    let started = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_bot(&db, &bot.id)?
    };
    emit_bot_status(app_handle, &started);
    push_bot_log_for(
        app_handle,
        Some(&bot.id),
        "info",
        format!(
            "{} bot '{}' started on {} {} ({}); warmed with {} prior candles.",
            mode.as_str(), bot.name, bot.pair, bot.timeframe, exchange_provider, warmup_json.len()
        ),
    );
    if let Ok(rt) = runtime.lock() {
        emit_equity(app_handle, &rt, true);
    }

    // market feed
    {
        let app = app_handle.clone();
        let stdin = Arc::clone(&stdin);
        let runtime = Arc::clone(&runtime);
        let stop = Arc::clone(&stop_flag);
        let child = Arc::clone(&child);
        let pair = bot.pair.clone();
        let timeframe = bot.timeframe.clone();
        let provider = exchange_provider.clone();
        let bot_id = bot.id.clone();
        let mut last_candle_time = Some(initial_candle.time.clone());
        std::thread::spawn(move || {
            let mut idx = 0_usize;
            let mut consecutive_errors = 0_usize;
            // A snapshot per candle, and at least one a minute: an hourly
            // bot's equity curve follows the mark price, not just its candles.
            let mut last_snapshot = Instant::now();
            let poll_seconds = match timeframe.as_str() {
                "1m" => 10,
                "5m" | "15m" => 15,
                _ => 30,
            };
            while !stop.load(Ordering::Relaxed) {
                match fetch_latest_market_candle(&provider, &pair, &timeframe) {
                    Ok(candle) => {
                        consecutive_errors = 0;
                        let is_new = last_candle_time.as_deref() != Some(candle.time.as_str());
                        let snapshot = is_new || last_snapshot.elapsed() >= std::time::Duration::from_secs(60);
                        if snapshot {
                            last_snapshot = Instant::now();
                        }
                        if let Ok(mut rt) = runtime.lock() {
                            rt.last_price = candle.close;
                            emit_equity(&app, &rt, snapshot);
                        }
                        if is_new {
                            let _ = write_json_line(
                                &stdin,
                                json!({ "method": "on_candle", "params": market_candle_json(&candle, &pair) }),
                            );
                            last_candle_time = Some(candle.time.clone());
                            push_bot_log_for(&app, Some(&bot_id), "info", format!("Market candle {provider} {pair} close {:.4}", candle.close));
                        } else if idx == 0 || idx % 6 == 0 {
                            push_bot_log_for(&app, Some(&bot_id), "info", format!("Market price {provider} {pair} close {:.4}", candle.close));
                        }
                    }
                    Err(err) => {
                        consecutive_errors += 1;
                        push_bot_log_for(&app, Some(&bot_id), "warn", format!("Public market data fetch failed for {provider} {pair}: {err}"));
                        if consecutive_errors >= 3 {
                            emit_bot_error(
                                &app,
                                Some(&bot_id),
                                "Market data feed failed.",
                                Some(format!("Could not fetch {provider} {pair} after {consecutive_errors} attempts: {err}")),
                            );
                            if let Ok(mut c) = child.lock() {
                                let _ = c.kill();
                            }
                            break;
                        }
                    }
                }
                idx += 1;
                // Sleep in short steps so a stop is honoured within a second.
                for _ in 0..(poll_seconds * 4) {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
            let _ = write_json_line(&stdin, json!({ "method": "stop", "params": {} }));
        });
    }

    // watcher: the only place that learns of an exit on its own
    {
        let app = app_handle.clone();
        let child = Arc::clone(&child);
        let stop = Arc::clone(&stop_flag);
        let bot_id = bot.id.clone();
        std::thread::spawn(move || loop {
            let status = {
                let mut c = match child.lock() {
                    Ok(c) => c,
                    Err(_) => break,
                };
                c.try_wait().ok().flatten()
            };
            if let Some(exit_status) = status {
                let state = app.try_state::<AppState>();
                if let Some(state) = state.as_ref() {
                    if let Ok(mut bots) = state.bots.lock() {
                        bots.remove(&bot_id);
                    }
                }
                if stop.load(Ordering::Relaxed) {
                    if let Some(state) = state.as_ref() {
                        if let Ok(db) = state.db.lock() {
                            let _ = set_bot_status(&db, &bot_id, "stopped", None);
                        }
                    }
                    push_bot_log_for(&app, Some(&bot_id), "info", "Bot process exited after stop.");
                } else {
                    let details = format!("Process exit status: {exit_status}");
                    emit_bot_error(&app, Some(&bot_id), "Bot process exited unexpectedly.", Some(details));
                }
                if let Some(state) = state.as_ref() {
                    if let Ok(db) = state.db.lock() {
                        if let Ok(bot) = load_bot(&db, &bot_id) {
                            emit_bot_status(&app, &bot);
                        }
                    }
                }
                sync_register(&app);
                break;
            }
            if stop.load(Ordering::Relaxed) {
                // The stop path kills the child itself; give the exit a
                // moment to be observed, then leave.
                std::thread::sleep(std::time::Duration::from_millis(300));
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        });
    }

    Ok(started)
}

/// Stop one running bot: raise the flag, ask the runner to stop, wait 3 s,
/// then kill. Returns the bot's row after the stop.
pub(crate) fn stop_bot_inner(app_handle: &AppHandle, state: &AppState, bot_id: &str) -> Result<Bot, String> {
    let handle = {
        let mut bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        bots.remove(bot_id)
    };
    if let Some(handle) = handle {
        // A live bot's holdings outlive its process: say what stays on the
        // exchange rather than selling it behind the user's back (§4.3).
        if let Ok(rt) = handle.runtime.lock() {
            if rt.mode == Mode::Live && !rt.open_positions.is_empty() {
                let held: Vec<String> = rt
                    .open_positions
                    .iter()
                    .map(|p| {
                        format!(
                            "{:.8} {} bought at {:.4}{}",
                            p.quantity,
                            rt.base_asset(),
                            p.entry_price,
                            p.entry_order_id.as_deref().map(|id| format!(" (order {id})")).unwrap_or_default()
                        )
                    })
                    .collect();
                push_bot_log_for(
                    app_handle,
                    Some(bot_id),
                    "warn",
                    format!(
                        "Stopped with {} live position(s) still on the exchange: {}. They stay in the account and their journal rows stay open; use Close positions before stopping to flatten.",
                        held.len(),
                        held.join("; ")
                    ),
                );
            }
        }
        handle.stop_flag.store(true, Ordering::Relaxed);
        let _ = write_json_line(&handle.stdin, json!({ "method": "stop", "params": {} }));
        if let Ok(mut child) = handle.child.lock() {
            let deadline = Instant::now() + std::time::Duration::from_secs(3);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(100)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
    }
    let bot = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        set_bot_status(&db, bot_id, "stopped", None)?;
        load_bot(&db, bot_id)?
    };
    sync_register(app_handle);
    emit_bot_status(app_handle, &bot);
    push_bot_log_for(app_handle, Some(bot_id), "info", format!("Bot '{}' stopped.", bot.name));
    Ok(bot)
}

/// Stop a bot by id. Without an id every bot stops (the single-bot era's
/// call) and the reply is a summary rather than a row.
#[tauri::command]
pub(crate) async fn stop_bot(bot_id: Option<String>, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        match bot_id {
            Some(id) => {
                let bot = stop_bot_inner(&app_handle, &state, &id)?;
                serde_json::to_value(bot).map_err(|e| format!("Serialize: {e}"))
            }
            None => {
                let stopped = stop_every_bot(&app_handle, &state)?;
                Ok(json!({ "status": "stopped", "stopped": stopped, "strategy_id": null, "exchange_id": null, "pair": null, "started_at": null, "config_json": null, "trading_mode": "paper" }))
            }
        }
    })
    .await
    .map_err(|e| format!("Stop task failed: {e}"))?
}

pub(crate) fn stop_every_bot(app_handle: &AppHandle, state: &AppState) -> Result<usize, String> {
    let ids: Vec<String> = {
        let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        bots.keys().cloned().collect()
    };
    let mut stopped = 0;
    for id in ids {
        if stop_bot_inner(app_handle, state, &id).is_ok() {
            stopped += 1;
        }
    }
    Ok(stopped)
}

/// The register's stop button and the shutdown hook.
#[tauri::command]
pub(crate) async fn stop_all_bots(app_handle: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        stop_every_bot(&app_handle, &state)
    })
    .await
    .map_err(|e| format!("Stop task failed: {e}"))?
}

/// Market-close every open position of a running bot (user action).
#[tauri::command]
pub(crate) async fn close_bot_positions(bot_id: String, app_handle: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let (stdin, runtime) = {
            let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
            let handle = bots.get(&bot_id).ok_or_else(|| "The bot is not running.".to_string())?;
            (Arc::clone(&handle.stdin), Arc::clone(&handle.runtime))
        };
        let mut rt = runtime.lock().map_err(|e| format!("Lock: {e}"))?;
        let count = rt.open_positions.len();
        let now = Utc::now().to_rfc3339();
        let effects = rt.close(None, &now);
        apply_effects(&app_handle, &stdin, &rt, effects);
        push_bot_log_for(&app_handle, Some(&bot_id), "info", format!("Closed {count} position(s) on request."));
        Ok(count)
    })
    .await
    .map_err(|e| format!("Close task failed: {e}"))?
}

/// A single-bot-shaped status: the named bot, else the first running bot,
/// else stopped. Kept for the pages of the single-bot era; the Bots page
/// reads `list_bots`.
#[tauri::command(async)]
pub(crate) fn get_bot_status(bot_id: Option<String>, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let bot = match bot_id {
        Some(id) => Some(load_bot(&db, &id)?),
        None => crate::bots::load_bots(&db)?.into_iter().find(|b| b.status == "running"),
    };
    Ok(match bot {
        Some(bot) => BotStatus {
            status: bot.status,
            strategy_id: Some(bot.strategy_id),
            exchange_id: Some(bot.exchange_id),
            pair: Some(bot.pair),
            started_at: bot.started_at,
            config_json: bot.config_json,
            trading_mode: bot.trading_mode,
        },
        None => BotStatus {
            status: "stopped".into(),
            strategy_id: None,
            exchange_id: None,
            pair: None,
            started_at: None,
            config_json: None,
            trading_mode: "paper".into(),
        },
    })
}

/// The in-memory log tail: one bot's lines, or every line.
#[tauri::command(async)]
pub(crate) fn get_bot_logs(
    bot_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<LogEntry>, String> {
    let logs = state.bot_logs.lock().map_err(|e| format!("Lock: {e}"))?;
    let off = offset.unwrap_or(0);
    let lim = limit.unwrap_or(100);
    let filtered: Vec<&LogEntry> = match bot_id.as_deref() {
        Some(id) => logs.iter().filter(|e| e.bot_id.as_deref() == Some(id)).collect(),
        None => logs.iter().collect(),
    };
    let slice = if off < filtered.len() {
        let end = filtered.len().saturating_sub(off);
        let start = end.saturating_sub(lim);
        &filtered[start..end]
    } else {
        &[]
    };
    Ok(slice.iter().map(|e| (*e).clone()).collect())
}

/// The suite process page's log tail: `{ limit }` in, oldest-first lines out.
#[tauri::command(async)]
pub(crate) fn process_bot_logs(limit: usize, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let logs = state.bot_logs.lock().map_err(|e| format!("Lock: {e}"))?;
    let start = logs.len().saturating_sub(limit);
    Ok(logs[start..]
        .iter()
        .map(|entry| match entry.bot_id.as_deref() {
            Some(id) => format!("{} [{}] [{}] {}", entry.timestamp, entry.level.to_uppercase(), &id[..id.len().min(8)], entry.message),
            None => format!("{} [{}] {}", entry.timestamp, entry.level.to_uppercase(), entry.message),
        })
        .collect())
}

/// Every runner this session holds, for the shutdown hook.
pub(crate) fn kill_all(bots: &mut HashMap<String, BotHandle>) {
    for (_, handle) in bots.drain() {
        handle.stop_flag.store(true, Ordering::Relaxed);
        let _ = write_json_line(&handle.stdin, json!({ "method": "stop", "params": {} }));
        if let Ok(mut child) = handle.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// Unused-import guard for the module-level log helper: the preflight of a
// draft logs without a bot id through `push_bot_log_for(.., None, ..)`;
// `push_bot_log` stays the entry point for module-level lines elsewhere.
#[allow(dead_code)]
fn _module_level_log(app_handle: &AppHandle, message: &str) {
    push_bot_log(app_handle, "info", message.to_string());
}
