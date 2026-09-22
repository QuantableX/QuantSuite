const COMMANDS: &[&str] = &[
    "get_app_settings",
    "update_app_settings",
    "list_systems",
    "get_system_config",
    "save_system_config",
    "start_engine",
    "stop_engine",
    "engine_status",
    "live_eval",
    "run_backtest",
    "browse_universe",
    "cache_stats",
    "clear_cache",
    "screenshot_to_clipboard",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
