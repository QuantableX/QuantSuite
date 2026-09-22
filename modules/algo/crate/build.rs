const COMMANDS: &[&str] = &[
    "list_strategies",
    "get_strategy",
    "create_strategy",
    "save_strategy",
    "update_strategy_meta",
    "delete_strategy",
    "read_strategy_file",
    // The Indicator Smithery's registry and the RegimeTrend template.
    "list_indicators",
    "create_strategy_from_indicator",
    // The forge inside the suite (PLAN-QUANTALGO §6): vault listing, one
    // report, an indicator's source, and the jobs — gauntlet, walk-forward,
    // shelf refresh — one at a time, polled by the Smithery page.
    "smithery_info",
    "smithery_report",
    "smithery_source",
    "smithery_run",
    "smithery_cancel",
    "smithery_jobs",
    // `{ limit }` → lines, for the suite's process page.
    "smithery_process_logs",
    "run_backtest",
    "list_backtests",
    "get_backtest",
    "save_backtest",
    "delete_backtest",
    "validate_strategy",
    "validate_bot_deploy",
    "start_bot",
    "stop_bot",
    // No arguments: the suite's process page and the shutdown path.
    "stop_all_bots",
    "close_bot_positions",
    "get_bot_status",
    "get_bot_logs",
    // `{ limit }` → lines, for the suite's process page (see `process_bot_logs`).
    "process_bot_logs",
    // The bots table (PLAN-QUANTALGO §3.3).
    "create_bot",
    "update_bot",
    "delete_bot",
    "list_bots",
    "get_bot",
    "get_bot_positions",
    "list_exchanges",
    "add_exchange",
    "update_exchange",
    "delete_exchange",
    "test_exchange_connection",
    // Same check as above on credentials that are not saved yet — the form
    // tests before it stores.
    "test_exchange_credentials",
    "get_balances",
    "get_exchange_pairs",
    "list_trades",
    // Distinct exchange / pair values of the journal, for its filter dropdowns.
    "list_trade_facets",
    "get_trade_stats",
    // The same statistics split by mode and by bot, in one call.
    "get_trade_stats_breakdown",
    // Realised PnL per calendar day in the caller's zone, for the heatmaps.
    "get_daily_pnl",
    "update_trade_notes",
    "get_equity_curve",
    "export_all_data",
    "import_data",
    "get_settings",
    "update_settings",
    "detect_python",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
