const COMMANDS: &[&str] = &[
    "get_ohlcv",
    "get_tickers",
    "get_order_book",
    "get_screener",
    "get_correlation",
    "get_marketcap",
    "get_daily_change",
    "get_sentiment",
    "get_metric_history",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
