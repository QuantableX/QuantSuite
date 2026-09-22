use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Wry,
};
use crate::bot::BotHandle;
use crate::db::{init_db, run_migrations, seed_strategies};
use crate::logs::{load_persisted_bot_logs, push_bot_log};
use crate::settings::{PYTHON_SDK_DIR, get_data_dir, import_legacy_data, load_settings_from_disk};

mod backtest;
mod bot;
mod bots;
mod broker;
mod data;
mod db;
mod exchanges;
mod journal;
mod logs;
mod runtime;
mod settings;
mod smithery;
mod strategies;

// ---------------------------------------------------------------------------
// Data Structures
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Strategy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub params_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Trade {
    pub id: String,
    /// The bot that made the trade; `None` on backtest rows and on rows from
    /// the single-bot era (PLAN-QUANTALGO §3.1).
    #[serde(default)]
    pub bot_id: Option<String>,
    /// `paper` | `live` — the bot's mode when the row is read (NULL bot_id
    /// counts as paper); `None` on backtest rows.
    #[serde(default)]
    pub trading_mode: Option<String>,
    pub strategy_id: String,
    pub exchange: String,
    pub pair: String,
    pub side: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub entry_time: String,
    pub exit_time: Option<String>,
    pub pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
    pub fee: f64,
    pub is_backtest: bool,
    pub backtest_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BacktestConfig {
    pub strategy_id: String,
    /// The data provider (`binance`, `bybit`, …). Derived from `exchange_id`
    /// when that is set; kept as its own field because saved backtests and
    /// the MCP tool carry the provider name, not an exchange row.
    pub exchange: String,
    /// The connected exchange the candles come from (PLAN-QUANTALGO §5) —
    /// the form only offers connected exchanges, never a typed provider.
    #[serde(default)]
    pub exchange_id: Option<String>,
    pub pair: String,
    pub timeframe: String,
    pub start_date: String,
    pub end_date: String,
    pub initial_capital: f64,
    pub commission: f64,
    #[serde(default)]
    pub strategy_params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BacktestStats {
    pub total_return: f64,
    pub total_return_pct: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: i64,
    pub avg_trade_duration_secs: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EquityPoint {
    pub time: String,
    pub equity: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BacktestResult {
    pub id: String,
    pub name: String,
    pub strategy_id: String,
    pub config: BacktestConfig,
    pub stats: BacktestStats,
    pub equity_curve: Vec<EquityPoint>,
    pub trades: Vec<Trade>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BacktestSummary {
    pub id: String,
    pub name: String,
    pub strategy_id: String,
    pub config_json: String,
    pub stats_json: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Exchange {
    pub id: String,
    pub name: String,
    pub exchange_type: String,
    pub provider: String,
    pub is_active: bool,
    /// Private calls go to the venue's testnet / demo environment
    /// (PLAN-QUANTALGO §4.2); public market data stays on production.
    #[serde(default)]
    pub sandbox: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExchangeConfig {
    pub name: String,
    pub exchange_type: String,
    pub provider: String,
    #[serde(default)]
    pub sandbox: Option<bool>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
    pub wallet_address: Option<String>,
    pub private_key: Option<String>,
    pub rpc_endpoint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Balance {
    pub asset: String,
    pub total: f64,
    pub available: f64,
    pub in_positions: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BotStatus {
    pub status: String,
    pub strategy_id: Option<String>,
    pub exchange_id: Option<String>,
    pub pair: Option<String>,
    pub started_at: Option<String>,
    pub config_json: Option<String>,
    pub trading_mode: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    /// The bot the line belongs to; module-level lines (preflight of a
    /// draft, backtest runner errors) have none.
    #[serde(default)]
    pub bot_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub theme: String,
    pub font_size: u32,
    /// The connected exchange every exchange dropdown starts on; `None` until
    /// the user picks one (or the exchange was deleted) — the dropdowns then
    /// fall back to the first connected exchange.
    #[serde(default)]
    pub default_exchange_id: Option<String>,
    pub default_pair: String,
    pub default_timeframe: String,
    pub python_path: String,
    pub strategy_dir: String,
    pub backtest_dir: String,
    pub risk_per_trade: f64,
    pub max_concurrent_positions: u32,
    pub slippage_tolerance: f64,
    #[serde(default = "settings::default_paper_fee_pct")]
    pub paper_fee_pct: f64,
    /// A new bot's budget (quote); the backtest starts with the same capital.
    #[serde(default = "settings::default_budget")]
    pub default_budget: f64,
    #[serde(default = "settings::default_warmup_candles")]
    pub default_warmup_candles: u32,
    /// Which set of bot defaults this config has received (`settings::apply_bot_defaults`).
    #[serde(default)]
    pub defaults_version: u32,
    pub notify_on_trade: bool,
    pub notify_on_error: bool,
    pub notify_on_daily_summary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeFilters {
    #[serde(default)]
    pub bot_id: Option<String>,
    /// `paper` | `live`; backtest rows never match.
    #[serde(default)]
    pub trading_mode: Option<String>,
    pub strategy_id: Option<String>,
    pub exchange: Option<String>,
    pub pair: Option<String>,
    pub side: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    /// Window over the exit side — from_date/to_date bind entry_time, and a
    /// position held overnight belongs to the day it was closed, not opened.
    /// Still-open trades have no exit_time and drop out of either bound.
    pub exited_from: Option<String>,
    pub exited_to: Option<String>,
    pub min_pnl: Option<f64>,
    pub is_backtest: Option<bool>,
    pub backtest_id: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// The distinct values the journal can be filtered by — what the trades table
/// actually holds, so the filter dropdowns never offer a value that matches
/// nothing (and never take free text; PLAN-QUANTALGO §5).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TradeFacets {
    /// `trades.exchange` values: a connected exchange's id for bot trades, a
    /// provider name on backtest rows. The frontend maps ids to names.
    pub exchanges: Vec<String>,
    pub pairs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeStats {
    pub total_trades: f64,
    pub win_rate: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub expectancy: f64,
    pub best_trade: f64,
    pub worst_trade: f64,
    pub total_pnl: f64,
    pub total_pnl_pct: f64,
    pub avg_duration_secs: f64,
}

/// One bot's share of `get_trade_stats_breakdown`. `bot_id` is `None` for
/// the two catch-all groups (deleted bots, the single-bot era).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BotTradeStats {
    pub bot_id: Option<String>,
    pub name: String,
    pub trading_mode: String,
    pub stats: TradeStats,
}

/// The journal's statistics split by mode and by bot (PLAN-QUANTALGO §3.4).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeStatsBreakdown {
    pub paper: TradeStats,
    pub live: TradeStats,
    pub bots: Vec<BotTradeStats>,
}

/// One calendar day of realised PnL (`get_daily_pnl`), keyed by the day a
/// trade closed in the caller's time zone.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DailyPnl {
    /// `YYYY-MM-DD` in the caller's zone.
    pub date: String,
    pub pnl: f64,
    pub trades: u32,
}

// ---------------------------------------------------------------------------
// App State
// ---------------------------------------------------------------------------

/// Key the bots are announced under in the suite's process register
/// (`qs_core::processes`): one entry for all of them — Running while any bot
/// runs (PLAN-QUANTALGO §3.2). The register holds no handle and cannot look:
/// every transition is *reported*, from `start_bot`, `stop_bot` and the
/// watcher thread that learns of a runner's exit.
pub(crate) const BOTS_PROCESS_ID: &str = "algo.bots";

pub struct AppState {
    pub db: Mutex<Connection>,
    /// The running bots by id — process, stdin, stop flag and runtime.
    pub bots: Mutex<HashMap<String, BotHandle>>,
    pub settings: Mutex<AppSettings>,
    pub bot_logs: Mutex<Vec<LogEntry>>,
    /// The forge's registry (`python -m smithery.registry --json`), read once
    /// per session — the Smithery page asks for it on every visit.
    pub indicators: Mutex<Option<serde_json::Value>>,
    /// The forge's jobs and its cached vault listing (PLAN-QUANTALGO §6).
    pub smithery: smithery::ForgeState,
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

/// Build the `algo` plugin. Pinned to `Wry` — see the note on `systems`.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("algo")
        .setup(|app, _api| {
            // A miss is logged here, once, instead of surfacing per-spawn: the
            // runner still starts and fails with a readable import error.
            let sdk_dir = qs_core::paths::python_sidecar_dir(app, "quantalgo");
            if sdk_dir.is_none() {
                log::warn!(
                    target: "algo",
                    "quantalgo Python package not found (no sidecars/python nearby, \
                     no bundled resources, QUANTSUITE_PYTHON_DIR unset)"
                );
            }
            let _ = PYTHON_SDK_DIR.set(sdk_dir);

            let data_dir = get_data_dir();
            std::fs::create_dir_all(&data_dir)?;
            import_legacy_data(&data_dir);
            std::fs::create_dir_all(data_dir.join("strategies"))?;
            std::fs::create_dir_all(data_dir.join("backtests"))?;
            std::fs::create_dir_all(data_dir.join("logs"))?;

            let db = Connection::open(data_dir.join("quantalgo.db"))?;
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
            init_db(&db)?;

            let strategy_dir = data_dir.join("strategies");
            if let Err(e) = seed_strategies(&db, &strategy_dir) {
                eprintln!("[quantalgo] seed_strategies: {e}");
            }
            if let Err(e) = run_migrations(&db, &strategy_dir) {
                eprintln!("[quantalgo] run_migrations: {e}");
            }

            let settings = load_settings_from_disk();

            let state = AppState {
                db: Mutex::new(db),
                bots: Mutex::new(HashMap::new()),
                settings: Mutex::new(settings),
                bot_logs: Mutex::new(load_persisted_bot_logs(1_000)),
                indicators: Mutex::new(None),
                smithery: smithery::ForgeState::default(),
            };

            app.manage(state);

            // Reconcile: a bot left "running" by a previous session has no
            // process in this one. Bots are never auto-restarted
            // (PLAN-QUANTALGO §3.1).
            {
                let state = app.state::<AppState>();
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                let now = chrono::Utc::now().to_rfc3339();
                let reconciled = db
                    .execute(
                        "UPDATE bots SET status = 'stopped', stopped_at = ?1, last_error = 'suite restarted', updated_at = ?1 WHERE status = 'running'",
                        rusqlite::params![now],
                    )
                    .unwrap_or(0);
                if reconciled > 0 {
                    push_bot_log(
                        app,
                        "warn",
                        format!("{reconciled} bot(s) were running when the suite last exited; marked stopped (suite restarted)."),
                    );
                }
            }

            // One entry for all bots in the suite's process register: the
            // register's commands take no arguments, so per-bot rows there
            // would have no buttons. Starting a bot needs a choice and stays
            // on the Bots page; stopping all of them and tailing their log
            // is what this row can honestly offer.
            qs_core::processes::announce(
                app,
                qs_core::processes::ProcessInfo::new(BOTS_PROCESS_ID, "algo", "Trading bots")
                    .stop_with("plugin:algo|stop_all_bots")
                    .logs_with("plugin:algo|process_bot_logs"),
            );

            // The forge: one entry, Running while a gauntlet, a walk-forward
            // or a shelf refresh runs (PLAN-QUANTALGO §6).
            qs_core::processes::announce(
                app,
                qs_core::processes::ProcessInfo::new(smithery::FORGE_PROCESS_ID, "algo", "Indicator Smithery forge")
                    .stop_with("plugin:algo|smithery_cancel")
                    .logs_with("plugin:algo|smithery_process_logs"),
            );

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "algo".into(),
                    label: "QuantAlgo".into(),
                    route: "/algo".into(),
                    toggle_window: None,
                },
            );

            // Running bots survive the main window closing to the tray; they
            // are stopped only on a real Quit (ARCHITECTURE.md §10).
            let handle = app.clone();
            qs_core::shutdown::on_shutdown(
                app,
                "algo:bots",
                Box::new(move || {
                    let Some(state) = handle.try_state::<AppState>() else { return };
                    let Ok(mut bots) = state.bots.lock() else { return };
                    bot::kill_all(&mut bots);
                }),
            );
            // A gauntlet in flight dies with the app, pool workers included.
            let handle = app.clone();
            qs_core::shutdown::on_shutdown(
                app,
                "algo:smithery",
                Box::new(move || {
                    let Some(state) = handle.try_state::<AppState>() else { return };
                    smithery::kill_running(&state);
                }),
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            strategies::list_strategies,
            strategies::get_strategy,
            strategies::create_strategy,
            strategies::save_strategy,
            strategies::update_strategy_meta,
            strategies::delete_strategy,
            strategies::read_strategy_file,
            strategies::list_indicators,
            strategies::create_strategy_from_indicator,
            smithery::smithery_info,
            smithery::smithery_report,
            smithery::smithery_source,
            smithery::smithery_run,
            smithery::smithery_cancel,
            smithery::smithery_jobs,
            smithery::smithery_process_logs,
            backtest::run_backtest,
            backtest::list_backtests,
            backtest::get_backtest,
            backtest::save_backtest,
            backtest::delete_backtest,
            bot::validate_strategy,
            bot::validate_bot_deploy,
            bot::start_bot,
            bot::stop_bot,
            bot::stop_all_bots,
            bot::close_bot_positions,
            bot::get_bot_status,
            bot::get_bot_logs,
            bot::process_bot_logs,
            bots::create_bot,
            bots::update_bot,
            bots::delete_bot,
            bots::list_bots,
            bots::get_bot,
            bots::get_bot_positions,
            exchanges::list_exchanges,
            exchanges::add_exchange,
            exchanges::update_exchange,
            exchanges::delete_exchange,
            exchanges::test_exchange_connection,
            exchanges::test_exchange_credentials,
            exchanges::get_balances,
            exchanges::get_exchange_pairs,
            journal::list_trades,
            journal::list_trade_facets,
            journal::get_trade_stats,
            journal::get_trade_stats_breakdown,
            journal::get_daily_pnl,
            journal::update_trade_notes,
            journal::get_equity_curve,
            data::export_all_data,
            data::import_data,
            settings::get_settings,
            settings::update_settings,
            settings::detect_python,
        ]))
        .build()
}
