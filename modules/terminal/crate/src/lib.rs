//! QuantView as a QuantSuite module.
//!
//! The standalone app had **zero** Tauri commands — all its market data came
//! from six Nitro server routes. A bundled Tauri app has no Nitro, so those
//! routes are these commands, backed by [`market`] (ARCHITECTURE.md §8).
//!
//! `better-sqlite3` was in the standalone `package.json` but referenced by no
//! code, so there is no database to port.

mod market;
mod metrics;

use market::MarketCapCache;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, State, Wry};

/// Build the `terminal` plugin.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("terminal")
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            get_ohlcv,
            get_tickers,
            get_order_book,
            get_screener,
            get_correlation,
            get_marketcap,
            get_daily_change,
            get_sentiment,
            get_metric_history,
        ]))
        .setup(|app, _api| {
            app.manage(MarketCapCache::new());

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "terminal".into(),
                    label: "QuantTerminal".into(),
                    route: "/terminal".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .build()
}

#[tauri::command]
async fn get_ohlcv(
    symbol: Option<String>,
    timeframe: Option<String>,
    limit: Option<u32>,
    since: Option<i64>,
) -> Result<Vec<market::Candle>, String> {
    market::ohlcv(
        symbol.as_deref().unwrap_or("BTC/USDT"),
        timeframe.as_deref().unwrap_or("1d"),
        limit.unwrap_or(1000),
        since,
    )
    .await
}

#[tauri::command]
async fn get_tickers() -> Result<Vec<market::TickerRow>, String> {
    market::tickers().await
}

#[tauri::command]
async fn get_order_book(
    symbol: Option<String>,
    limit: Option<u32>,
) -> Result<market::OrderBook, String> {
    market::order_book(symbol.as_deref().unwrap_or("BTC/USDT"), limit.unwrap_or(25)).await
}

#[tauri::command]
async fn get_screener() -> Result<Vec<market::ScreenerRow>, String> {
    market::screener().await
}

/// Live crypto Fear & Greed Index (alternative.me) — real sentiment, not the
/// simulated cards the metrics page used to carry.
#[tauri::command]
async fn get_sentiment(limit: Option<u32>) -> Result<Vec<market::SentimentPoint>, String> {
    market::sentiment(limit.unwrap_or(30)).await
}

#[tauri::command]
async fn get_metric_history(source: String, asset: Option<String>) -> Result<metrics::History, String> {
    // Bound the complete paginated operation, not just each HTTP request. Cancelling
    // here drops the provider future, so retries cannot leave old downloads running.
    tokio::time::timeout(
        std::time::Duration::from_secs(10 * 60),
        metrics::history(&source, asset.as_deref().unwrap_or("BTC")),
    )
    .await
    .map_err(|_| "Metrics history timed out. Please retry.".to_string())?
}

#[tauri::command]
async fn get_correlation(
    symbols: Option<String>,
    timeframe: Option<String>,
) -> Result<market::Correlation, String> {
    let symbols: Vec<String> = symbols
        .unwrap_or_else(|| "BTC,ETH,SOL".into())
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    market::correlation(&symbols, timeframe.as_deref().unwrap_or("30d")).await
}

/// Change since the 00:00 UTC open per coin symbol, for the ladder's toggled
/// change column. Coins without a Binance USDT pair are absent from the map.
#[tauri::command]
async fn get_daily_change(
    symbols: Vec<String>,
) -> Result<std::collections::HashMap<String, f64>, String> {
    market::daily_change(&symbols).await
}

/// The CoinMarketCap key was `runtimeConfig.cmcApiKey` (a `.env` value) in the
/// standalone app. It now lives in suite settings under the `view` scope.
#[tauri::command]
async fn get_marketcap(
    cache: State<'_, MarketCapCache>,
    db: State<'_, qs_core::Db>,
    page: Option<u32>,
    provider: Option<String>,
) -> Result<Vec<market::MarketCapCoin>, String> {
    let key = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        qs_core::db::get_setting(&conn, "terminal", "cmcApiKey")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };

    market::market_cap(
        &cache,
        page.unwrap_or(1),
        provider.as_deref().unwrap_or("coingecko"),
        &key,
    )
    .await
}
