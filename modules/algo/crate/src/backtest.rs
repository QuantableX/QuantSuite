use chrono::Utc;
use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use crate::{
    AppState,
    BacktestConfig,
    BacktestResult,
    BacktestStats,
    BacktestSummary,
    EquityPoint,
    Trade,
};
use crate::bot::emit_bot_error;
// The backtest runner belongs to no bot: its spawn errors carry no bot id.
use crate::exchanges::fetch_historical_candles_with_progress;
use crate::settings::{build_python_command, resolve_python_path};

/// Progress share of the whole run: the candle fetch runs 10 → 20 %, the
/// strategy 20 → 95 %, the rest is bookkeeping.
const FETCH_FROM: f64 = 10.0;
const FETCH_TO: f64 = 20.0;
const RUN_TO: f64 = 95.0;

/// The runner's stderr line `PROGRESS <done>/<total>` (its logging prefix
/// before it), emitted every percent of the candles.
fn parse_runner_progress(line: &str) -> Option<(u64, u64)> {
    let rest = line.split("PROGRESS ").nth(1)?;
    let (done, total) = rest.trim().split_once('/')?;
    let total: u64 = total.trim().parse().ok()?;
    if total == 0 {
        return None;
    }
    Some((done.trim().parse().ok()?, total))
}

// ---------------------------------------------------------------------------
// Backtest Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) async fn run_backtest(
    config: BacktestConfig,
    app_handle: AppHandle,
) -> Result<BacktestResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_backtest_blocking(config, app_handle))
        .await
        .map_err(|e| format!("Backtest task failed: {e}"))?
}

/// The blocking half of [`run_backtest`], run on the blocking pool.
///
/// Every expensive step in here blocks the calling thread: an HTTP fetch of the
/// candle history, then a Python child process run to completion. Declaring the
/// command `async` moved it off the main thread but not off a *runtime worker*,
/// which it then held for the whole backtest — minutes, on a long series. The
/// sibling engine commands in `systems` already went through `spawn_blocking`
/// for exactly this reason; this one had been missed.
///
/// State is re-acquired from the handle rather than passed in: `State<'_, _>`
/// borrows the app and cannot cross into a `'static` closure.
fn run_backtest_blocking(
    mut config: BacktestConfig,
    app_handle: AppHandle,
) -> Result<BacktestResult, String> {
    let state = app_handle.state::<AppState>();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _ = app_handle.emit(
        "backtest:progress",
        serde_json::json!({ "pct": 5.0, "message": "Preparing backtest environment..." }),
    );

    let (strategy_file_path, strategy_params_json) = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT file_path, params_json FROM strategies WHERE id = ?1",
            params![config.strategy_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(|e| format!("Strategy lookup failed: {e}"))?
    };

    // The form sends the connected exchange; the candles come from that
    // exchange's provider. A provider typed straight into the MCP tool (no
    // exchange_id) still works — `exchange` is then taken as given.
    if let Some(exchange_id) = config.exchange_id.as_deref() {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        config.exchange = db
            .query_row(
                "SELECT provider FROM exchanges WHERE id = ?1",
                params![exchange_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| format!("Exchange {exchange_id} is not connected any more."))?;
    }
    if config.exchange.trim().is_empty() {
        return Err("Choose a connected exchange for the candle data.".into());
    }

    let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
    let python_path = resolve_python_path(&settings);
    let slippage_pct = settings.slippage_tolerance;
    drop(settings);

    let fetch_label = format!("Fetching {} candles from {}", config.pair, config.exchange);
    let _ = app_handle.emit(
        "backtest:progress",
        serde_json::json!({ "pct": FETCH_FROM, "message": format!("{fetch_label}...") }),
    );
    let candles = {
        let app = app_handle.clone();
        let label = fetch_label.clone();
        fetch_historical_candles_with_progress(&config, &mut |share| {
            let _ = app.emit(
                "backtest:progress",
                serde_json::json!({
                    "pct": FETCH_FROM + (FETCH_TO - FETCH_FROM) * share,
                    "message": format!("{label}... {:.0}%", share * 100.0),
                }),
            );
        })?
    };
    let candle_times: Vec<String> = candles
        .iter()
        .filter_map(|candle| {
            candle
                .get("time")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();

    let mut strategy_params = strategy_params_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    // Merge UI-provided param overrides (e.g. direction dropdown)
    if let Some(overrides) = &config.strategy_params {
        if let (Some(base), Some(ovr)) = (strategy_params.as_object_mut(), overrides.as_object()) {
            for (k, v) in ovr {
                base.insert(k.clone(), v.clone());
            }
        }
    }

    let payload = serde_json::json!({
        "config": {
            "initial_balance": config.initial_capital,
            "commission_pct": config.commission,
            "slippage_pct": slippage_pct,
            "base_asset": config.pair.split('/').nth(1).unwrap_or("USDT"),
            "pair": config.pair,
        },
        "params": strategy_params,
        "candles": candles,
    });

    let total_candles = candle_times.len();
    let _ = app_handle.emit(
        "backtest:progress",
        serde_json::json!({ "pct": FETCH_TO, "message": format!("Running {total_candles} candles through the Python strategy...") }),
    );

    let mut command = build_python_command(&python_path);
    command
        .arg("-m")
        .arg("quantalgo.runner")
        .arg(&strategy_file_path)
        .arg("--backtest")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        let message = format!("Failed to spawn strategy runner: {e}");
        emit_bot_error(&app_handle, None, message.clone(), None);
        message
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.to_string().as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Write stdin: {e}"))?;
        // Dropped here: the runner reads stdin to EOF before it starts.
    } else {
        return Err("Backtest runner stdin unavailable".into());
    }

    // The runner narrates on stderr while it walks the candles (a month of
    // 1-minute bars is minutes of work); it is read line by line on its own
    // thread so the progress reaches the page as it happens, while stdout
    // (the result, large) is drained here — both pipes always have a reader.
    let stderr_text = Arc::new(Mutex::new(String::new()));
    let stderr_thread = child.stderr.take().map(|stderr| {
        let app = app_handle.clone();
        let collected = Arc::clone(&stderr_text);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some((done, total)) = parse_runner_progress(&line) {
                    let share = done as f64 / total as f64;
                    let _ = app.emit(
                        "backtest:progress",
                        serde_json::json!({
                            "pct": FETCH_TO + (RUN_TO - FETCH_TO) * share,
                            "message": format!("Running the strategy... candle {done} of {total}"),
                        }),
                    );
                    continue;
                }
                if let Ok(mut buf) = collected.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
        })
    });
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout)
            .map_err(|e| format!("Read backtest output: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("Wait for backtest: {e}"))?;
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }
    let stderr = stderr_text
        .lock()
        .map(|buf| buf.clone())
        .unwrap_or_default();

    if !status.success() {
        return Err(format!(
            "Backtest runner failed with status {}{}",
            status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        ));
    }
    let _ = app_handle.emit(
        "backtest:progress",
        serde_json::json!({ "pct": RUN_TO, "message": "Collecting the results..." }),
    );

    let (stats, equity_curve, trades) = parse_backtest_output(
        &stdout,
        &config,
        &id,
        &config.strategy_id,
        &now,
        &candle_times,
    )?;

    let result = BacktestResult {
        id: id.clone(),
        name: format!(
            "Backtest {} {}",
            config.pair,
            Utc::now().format("%Y-%m-%d %H:%M")
        ),
        strategy_id: config.strategy_id.clone(),
        config,
        stats,
        equity_curve,
        trades,
        created_at: now,
    };

    let _ = app_handle.emit(
        "backtest:progress",
        serde_json::json!({ "pct": 100.0, "message": "Backtest complete." }),
    );
    let _ = app_handle.emit(
        "backtest:complete",
        serde_json::json!({ "result": &result }),
    );

    Ok(result)
}

fn parse_backtest_output(
    output: &str,
    config: &BacktestConfig,
    backtest_id: &str,
    strategy_id: &str,
    created_at: &str,
    candle_times: &[String],
) -> Result<(BacktestStats, Vec<EquityPoint>, Vec<Trade>), String> {
    #[derive(Debug, Deserialize)]
    struct PythonTrade {
        id: String,
        pair: String,
        side: String,
        entry_price: f64,
        exit_price: f64,
        quantity: f64,
        pnl: f64,
        entry_time: String,
        exit_time: String,
        #[serde(default)]
        commission: f64,
    }

    #[derive(Debug, Deserialize)]
    struct PythonBacktestOutput {
        #[serde(default)]
        initial_balance: f64,
        #[serde(default)]
        final_balance: f64,
        #[serde(default)]
        equity_curve: Vec<f64>,
        #[serde(default)]
        trades: Vec<PythonTrade>,
        #[serde(default)]
        stats: Value,
    }

    for line in output.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<PythonBacktestOutput>(trimmed) {
                let gross_profit = parsed
                    .stats
                    .get("gross_profit")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0);
                let gross_loss = parsed
                    .stats
                    .get("gross_loss")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0);
                let profit_factor = match parsed.stats.get("profit_factor") {
                    Some(Value::Number(value)) => value.as_f64().unwrap_or(0.0),
                    Some(Value::String(value)) if value.eq_ignore_ascii_case("infinity") => {
                        gross_profit.max(1.0)
                    }
                    _ => 0.0,
                };

                let stats = BacktestStats {
                    total_return: parsed.final_balance - parsed.initial_balance,
                    total_return_pct: parsed
                        .stats
                        .get("total_return_pct")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                    sharpe_ratio: parsed
                        .stats
                        .get("sharpe_ratio")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                    max_drawdown: config.initial_capital
                        * parsed
                            .stats
                            .get("max_drawdown_pct")
                            .and_then(|value| value.as_f64())
                            .unwrap_or(0.0)
                        / 100.0,
                    max_drawdown_pct: parsed
                        .stats
                        .get("max_drawdown_pct")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                    win_rate: parsed
                        .stats
                        .get("win_rate_pct")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                    profit_factor,
                    total_trades: parsed
                        .stats
                        .get("total_trades")
                        .and_then(|value| value.as_i64())
                        .unwrap_or(parsed.trades.len() as i64),
                    avg_trade_duration_secs: parsed
                        .stats
                        .get("avg_trade_duration_seconds")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                };

                let equity_curve = parsed
                    .equity_curve
                    .iter()
                    .enumerate()
                    .map(|(idx, equity)| EquityPoint {
                        time: candle_times
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| config.start_date.clone()),
                        equity: *equity,
                    })
                    .collect::<Vec<_>>();

                let trades = parsed
                    .trades
                    .into_iter()
                    .map(|trade| {
                        let notional = trade.entry_price * trade.quantity;
                        let pnl_pct = if notional > 0.0 {
                            (trade.pnl / notional) * 100.0
                        } else {
                            0.0
                        };

                        Trade {
                            id: trade.id,
                            bot_id: None,
                            trading_mode: None,
                            strategy_id: strategy_id.to_string(),
                            // Bot trades hold the connected exchange's id in
                            // this column; a backtest run from the form does
                            // the same so the journal filters by one key.
                            exchange: config
                                .exchange_id
                                .clone()
                                .unwrap_or_else(|| config.exchange.clone()),
                            pair: trade.pair,
                            side: trade.side,
                            entry_price: trade.entry_price,
                            exit_price: Some(trade.exit_price),
                            quantity: trade.quantity,
                            entry_time: trade.entry_time,
                            exit_time: Some(trade.exit_time),
                            pnl: Some(trade.pnl),
                            pnl_pct: Some(pnl_pct),
                            fee: trade.commission,
                            is_backtest: true,
                            backtest_id: Some(backtest_id.to_string()),
                            notes: None,
                            created_at: created_at.to_string(),
                        }
                    })
                    .collect::<Vec<_>>();

                let _ = gross_loss;
                return Ok((stats, equity_curve, trades));
            }
        }
    }

    Err("Backtest runner returned no parseable result.".into())
}

#[tauri::command(async)]
pub(crate) fn list_backtests(state: State<'_, AppState>) -> Result<Vec<BacktestSummary>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let mut stmt = db
        .prepare("SELECT id, name, strategy_id, config_json, stats_json, created_at FROM backtests ORDER BY created_at DESC")
        .map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(BacktestSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                strategy_id: row.get(2)?,
                config_json: row.get(3)?,
                stats_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query: {e}"))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("Row: {e}"))?);
    }
    Ok(result)
}

#[tauri::command(async)]
pub(crate) fn get_backtest(id: String, state: State<'_, AppState>) -> Result<BacktestResult, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let (name, strategy_id, config_json, stats_json, eq_json, created_at): (
        String,
        String,
        String,
        String,
        String,
        String,
    ) = db
        .query_row(
            "SELECT name, strategy_id, config_json, stats_json, equity_curve_json, created_at FROM backtests WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .map_err(|e| format!("Not found: {e}"))?;

    let config: BacktestConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Parse config: {e}"))?;
    let stats: BacktestStats =
        serde_json::from_str(&stats_json).map_err(|e| format!("Parse stats: {e}"))?;

    #[derive(Deserialize)]
    struct EqData {
        equity_curve: Vec<EquityPoint>,
        trades: Vec<Trade>,
    }
    let eq_data: EqData = serde_json::from_str(&eq_json).unwrap_or(EqData {
        equity_curve: Vec::new(),
        trades: Vec::new(),
    });

    Ok(BacktestResult {
        id,
        name,
        strategy_id,
        config,
        stats,
        equity_curve: eq_data.equity_curve,
        trades: eq_data.trades,
        created_at,
    })
}

#[tauri::command(async)]
pub(crate) fn save_backtest(
    result: BacktestResult,
    name: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let config_json =
        serde_json::to_string(&result.config).map_err(|e| format!("Serialize: {e}"))?;
    let stats_json = serde_json::to_string(&result.stats).map_err(|e| format!("Serialize: {e}"))?;
    let eq_json = serde_json::to_string(&serde_json::json!({
        "equity_curve": result.equity_curve,
        "trades": result.trades,
    }))
    .map_err(|e| format!("Serialize: {e}"))?;

    // One transaction — a backtest can carry thousands of trades, and each
    // autocommitted INSERT would pay for its own fsync.
    let tx = db
        .transaction()
        .map_err(|e| format!("Begin save backtest: {e}"))?;

    tx.execute(
        "INSERT OR REPLACE INTO backtests (id, name, strategy_id, config_json, stats_json, equity_curve_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![result.id, name, result.strategy_id, config_json, stats_json, eq_json, result.created_at],
    )
    .map_err(|e| format!("Insert: {e}"))?;

    // Also persist backtest trades into the trades table
    for trade in &result.trades {
        tx.execute(
            "INSERT OR IGNORE INTO trades (id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, backtest_id, notes, created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1,?14,?15,?16)",
            params![
                trade.id,
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
                trade.backtest_id,
                trade.notes,
                trade.created_at,
            ],
        )
        .map_err(|e| format!("Insert trade: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("Commit save backtest: {e}"))?;

    Ok(result.id)
}

#[tauri::command(async)]
pub(crate) fn delete_backtest(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    db.execute("DELETE FROM trades WHERE backtest_id = ?1", params![id])
        .map_err(|e| format!("Delete trades: {e}"))?;
    let affected = db
        .execute("DELETE FROM backtests WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete: {e}"))?;
    Ok(affected > 0)
}
