//! Market data for QuantView, replacing the six Nitro routes it used to run.
//!
//! The standalone app reached Binance through CCXT inside a Nitro server. A
//! bundled Tauri app has no Nitro, and CCXT's surface here was four methods
//! against one exchange, all unauthenticated (ARCHITECTURE.md Â§8). These are the
//! same four endpoints, called directly.
//!
//! Response shapes are deliberately identical to what the Vue code already
//! consumes â€” field names, units and formatting all match the old routes, so no
//! component had to change beyond swapping `$fetch` for `invoke`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const BINANCE: &str = "https://api.binance.com/api/v3";
const COINGECKO: &str = "https://api.coingecko.com/api/v3/coins/markets";
const CMC: &str = "https://pro-api.coinmarketcap.com/v1/cryptocurrency/listings/latest";

/// `BTC/USDT` -> `BTCUSDT`. CCXT's unified symbols are what the frontend speaks.
fn to_binance_symbol(symbol: &str) -> String {
    symbol.replace('/', "").to_uppercase()
}

/// `BTCUSDT` -> `BTC/USDT`, so responses keep the shape the frontend expects.
fn from_binance_symbol(symbol: &str) -> String {
    for quote in ["USDT", "BUSD", "USDC", "BTC", "ETH", "BNB"] {
        if let Some(base) = symbol.strip_suffix(quote) {
            if !base.is_empty() {
                return format!("{base}/{quote}");
            }
        }
    }
    symbol.to_string()
}

/// The one HTTP client for this module, built once.
///
/// It used to be built per request, while the frontend polls the orderbook
/// every 800 ms and tickers every second: a fresh connection pool each time,
/// so no keep-alive, a full TCP + TLS handshake per call, and a socket left in
/// TIME_WAIT afterwards. A `reqwest::Client` is a handle around a shared pool —
/// reusing one is what makes those polls cheap.
pub(crate) fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent("QuantSuite/0.1")
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .map_err(|e| format!("http client: {e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

// â”€â”€ OHLCV â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Clone, Serialize)]
pub struct Candle {
    /// Unix seconds, matching the old route (CCXT returns millis).
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

fn f(v: &serde_json::Value) -> f64 {
    v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64()).unwrap_or(0.0)
}

pub async fn ohlcv(
    symbol: &str,
    timeframe: &str,
    limit: u32,
    since: Option<i64>,
) -> Result<Vec<Candle>, String> {
    let limit = limit.clamp(1, 1000);
    let mut url = format!(
        "{BINANCE}/klines?symbol={}&interval={}&limit={}",
        to_binance_symbol(symbol),
        timeframe,
        limit
    );
    if let Some(since) = since {
        url.push_str(&format!("&startTime={since}"));
    }

    let rows: Vec<Vec<serde_json::Value>> = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch OHLCV: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch OHLCV: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse OHLCV: {e}"))?;

    Ok(rows
        .iter()
        .filter(|r| r.len() >= 6)
        .map(|r| Candle {
            time: r[0].as_i64().unwrap_or(0) / 1000,
            open: f(&r[1]),
            high: f(&r[2]),
            low: f(&r[3]),
            close: f(&r[4]),
            volume: f(&r[5]),
        })
        .collect())
}

// â”€â”€ 24h tickers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Deserialize)]
struct BinanceTicker {
    symbol: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
    #[serde(rename = "highPrice")]
    high_price: String,
    #[serde(rename = "lowPrice")]
    low_price: String,
    volume: String,
    #[serde(rename = "quoteVolume")]
    quote_volume: String,
}

fn p(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

async fn fetch_tickers(symbols: &[&str]) -> Result<HashMap<String, BinanceTicker>, String> {
    let list: Vec<String> = symbols.iter().map(|s| format!("\"{}\"", to_binance_symbol(s))).collect();
    let url = format!("{BINANCE}/ticker/24hr?symbols=[{}]", list.join(","));

    let rows: Vec<BinanceTicker> = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch tickers: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch tickers: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse tickers: {e}"))?;

    Ok(rows.into_iter().map(|t| (t.symbol.clone(), t)).collect())
}

/// Matches the old route's `formatVolume` exactly, including its rounding.
fn format_volume(vol: f64) -> String {
    if vol >= 1e9 {
        format!("{:.1}B", vol / 1e9)
    } else if vol >= 1e6 {
        format!("{:.1}M", vol / 1e6)
    } else if vol >= 1e3 {
        format!("{:.1}K", vol / 1e3)
    } else {
        format!("{vol:.0}")
    }
}

pub const DASHBOARD_SYMBOLS: &[&str] =
    &["BTC/USDT", "ETH/USDT", "SOL/USDT", "BNB/USDT", "XRP/USDT", "AVAX/USDT"];

#[derive(Debug, Serialize)]
pub struct TickerRow {
    pub symbol: String,
    pub price: f64,
    pub change: f64,
    pub high: f64,
    pub low: f64,
    /// Pre-formatted ("1.2B") â€” the old route returned a string and the
    /// dashboard renders it verbatim.
    pub volume: String,
    #[serde(rename = "sparklineData")]
    pub sparkline_data: Vec<f64>,
}

pub async fn tickers() -> Result<Vec<TickerRow>, String> {
    let raw = fetch_tickers(DASHBOARD_SYMBOLS).await?;
    let mut out = Vec::new();

    for symbol in DASHBOARD_SYMBOLS {
        let Some(t) = raw.get(&to_binance_symbol(symbol)) else { continue };

        // 30 hourly closes for the sparkline. The old route fell back to jitter
        // around the last price on failure; an empty series is honest instead.
        let sparkline = match ohlcv(symbol, "1h", 30, None).await {
            Ok(c) => c.iter().map(|x| x.close).collect(),
            Err(_) => Vec::new(),
        };

        let quote = p(&t.quote_volume);
        out.push(TickerRow {
            symbol: from_binance_symbol(&t.symbol),
            price: p(&t.last_price),
            change: p(&t.price_change_percent),
            high: p(&t.high_price),
            low: p(&t.low_price),
            volume: format_volume(if quote > 0.0 { quote } else { p(&t.volume) }),
            sparkline_data: sparkline,
        });
    }

    Ok(out)
}

// â”€â”€ Order book â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Serialize)]
pub struct Level {
    pub price: f64,
    pub amount: f64,
}

#[derive(Debug, Serialize)]
pub struct OrderBook {
    pub asks: Vec<Level>,
    pub bids: Vec<Level>,
}

#[derive(Debug, Deserialize)]
struct RawBook {
    bids: Vec<Vec<String>>,
    asks: Vec<Vec<String>>,
}

pub async fn order_book(symbol: &str, limit: u32) -> Result<OrderBook, String> {
    // Binance only accepts a fixed ladder of depths; round up to the next one.
    let limit = limit.clamp(1, 500);
    let api_limit = [5u32, 10, 20, 50, 100, 500]
        .into_iter()
        .find(|d| *d >= limit)
        .unwrap_or(500);

    let url = format!("{BINANCE}/depth?symbol={}&limit={}", to_binance_symbol(symbol), api_limit);
    let raw: RawBook = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch orderbook: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch orderbook: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse orderbook: {e}"))?;

    let take = |rows: Vec<Vec<String>>| -> Vec<Level> {
        rows.into_iter()
            .filter(|r| r.len() >= 2)
            .take(limit as usize)
            .map(|r| Level { price: p(&r[0]), amount: p(&r[1]) })
            .collect()
    };

    Ok(OrderBook { asks: take(raw.asks), bids: take(raw.bids) })
}

// â”€â”€ Screener â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

pub const SCREENER_SYMBOLS: &[&str] = &[
    "BTC/USDT", "ETH/USDT", "BNB/USDT", "SOL/USDT", "XRP/USDT",
    "ADA/USDT", "DOGE/USDT", "AVAX/USDT", "DOT/USDT", "LINK/USDT",
    "MATIC/USDT", "SHIB/USDT", "UNI/USDT", "LTC/USDT", "ATOM/USDT",
    "ETC/USDT", "XLM/USDT", "FIL/USDT", "NEAR/USDT", "APT/USDT",
    "OP/USDT", "ARB/USDT", "SUI/USDT", "INJ/USDT", "TIA/USDT",
    "SEI/USDT", "RUNE/USDT", "FET/USDT", "RENDER/USDT", "IMX/USDT",
    "GRT/USDT", "AAVE/USDT", "MKR/USDT", "SNX/USDT", "CRV/USDT",
    "ALGO/USDT", "SAND/USDT", "MANA/USDT", "AXS/USDT", "GALA/USDT",
    "ENJ/USDT", "CHZ/USDT", "COMP/USDT", "ZEC/USDT", "DASH/USDT",
    "XMR/USDT", "EOS/USDT", "NEO/USDT", "IOTA/USDT", "XTZ/USDT",
    "THETA/USDT", "VET/USDT", "HBAR/USDT", "EGLD/USDT", "FTM/USDT",
    "FLOW/USDT", "KAVA/USDT", "ROSE/USDT", "ZIL/USDT", "ONE/USDT",
    "CELO/USDT", "MASK/USDT", "LDO/USDT", "RPL/USDT", "SSV/USDT",
    "PENDLE/USDT", "GMX/USDT", "DYDX/USDT", "JOE/USDT", "SUSHI/USDT",
    "BAL/USDT", "LOOM/USDT", "ANKR/USDT", "STORJ/USDT", "SKL/USDT",
    "CELR/USDT", "MTL/USDT", "OGN/USDT", "BAND/USDT", "KNC/USDT",
    "BAT/USDT", "ZRX/USDT", "ENS/USDT", "ICX/USDT", "QTUM/USDT",
    "ONT/USDT", "RVN/USDT", "SC/USDT", "WAVES/USDT", "BTT/USDT",
    "TRX/USDT", "BCH/USDT", "PEPE/USDT", "WIF/USDT", "BONK/USDT",
    "FLOKI/USDT", "JUP/USDT", "WLD/USDT", "PYTH/USDT", "STX/USDT",
];

#[derive(Debug, Serialize)]
pub struct ScreenerRow {
    pub symbol: String,
    pub price: f64,
    #[serde(rename = "change24h")]
    pub change_24h: f64,
    pub volume: f64,
    pub high: f64,
    pub low: f64,
    #[serde(rename = "quoteVolume")]
    pub quote_volume: f64,
}

pub async fn screener() -> Result<Vec<ScreenerRow>, String> {
    let raw = fetch_tickers(SCREENER_SYMBOLS).await?;

    Ok(SCREENER_SYMBOLS
        .iter()
        .filter_map(|s| raw.get(&to_binance_symbol(s)))
        .filter(|t| p(&t.last_price) > 0.0) // old route skipped entries without a last price
        .map(|t| ScreenerRow {
            symbol: from_binance_symbol(&t.symbol),
            price: p(&t.last_price),
            change_24h: p(&t.price_change_percent),
            volume: p(&t.volume),
            high: p(&t.high_price),
            low: p(&t.low_price),
            quote_volume: p(&t.quote_volume),
        })
        .collect())
}

// â”€â”€ Correlation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Serialize)]
pub struct Correlation {
    pub symbols: Vec<String>,
    pub matrix: HashMap<String, HashMap<String, f64>>,
    pub timeframe: String,
    #[serde(rename = "dataPoints")]
    pub data_points: usize,
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len());
    if n < 2 {
        return 0.0;
    }
    let (mut sx, mut sy, mut sxy, mut sx2, mut sy2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        sx += x[i];
        sy += y[i];
        sxy += x[i] * y[i];
        sx2 += x[i] * x[i];
        sy2 += y[i] * y[i];
    }
    let nf = n as f64;
    let numerator = nf * sxy - sx * sy;
    let denominator = ((nf * sx2 - sx * sx) * (nf * sy2 - sy * sy)).sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn log_returns(prices: &[f64]) -> Vec<f64> {
    prices
        .windows(2)
        .filter(|w| w[0] > 0.0 && w[1] > 0.0)
        .map(|w| (w[1] / w[0]).ln())
        .collect()
}

pub async fn correlation(symbols: &[String], timeframe: &str) -> Result<Correlation, String> {
    if symbols.len() < 2 || symbols.len() > 12 {
        return Err("Please provide between 2 and 12 symbols".into());
    }

    let (tf, limit) = match timeframe {
        "7d" => ("1h", 168u32),
        "30d" => ("4h", 180),
        "90d" => ("1d", 90),
        _ => return Err("Invalid timeframe. Use 7d, 30d, or 90d".into()),
    };

    let mut prices: HashMap<String, Vec<f64>> = HashMap::new();
    for symbol in symbols {
        let pair = format!("{}/USDT", symbol.to_uppercase());
        // A symbol that fails to fetch is skipped, as in the old route.
        if let Ok(candles) = ohlcv(&pair, tf, limit, None).await {
            prices.insert(symbol.to_uppercase(), candles.iter().map(|c| c.close).collect());
        }
    }

    let valid: Vec<String> = symbols
        .iter()
        .map(|s| s.to_uppercase())
        .filter(|s| prices.get(s).is_some_and(|p| p.len() > 10))
        .collect();

    if valid.len() < 2 {
        return Err("Could not fetch enough data to calculate correlations".into());
    }

    let returns: HashMap<&String, Vec<f64>> =
        valid.iter().map(|s| (s, log_returns(&prices[s]))).collect();

    let mut matrix = HashMap::new();
    for a in &valid {
        let mut row = HashMap::new();
        for b in &valid {
            let v = if a == b {
                1.0
            } else {
                (pearson(&returns[a], &returns[b]) * 10_000.0).round() / 10_000.0
            };
            row.insert(b.clone(), v);
        }
        matrix.insert(a.clone(), row);
    }

    let data_points = valid.iter().map(|s| prices[s].len()).min().unwrap_or(0);

    Ok(Correlation { symbols: valid, matrix, timeframe: timeframe.to_string(), data_points })
}

// â”€â”€ Market cap â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[derive(Debug, Clone, Serialize)]
pub struct MarketCapCoin {
    pub rank: u32,
    pub symbol: String,
    pub name: String,
    pub price: f64,
    #[serde(rename = "marketCap")]
    pub market_cap: f64,
    pub change: f64,
}

#[derive(Debug, Deserialize)]
struct GeckoCoin {
    symbol: String,
    name: String,
    current_price: Option<f64>,
    price_change_percentage_24h: Option<f64>,
    market_cap_rank: Option<u32>,
    market_cap: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CmcQuote {
    price: Option<f64>,
    percent_change_24h: Option<f64>,
    market_cap: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CmcCoin {
    cmc_rank: Option<u32>,
    symbol: String,
    name: String,
    quote: HashMap<String, CmcQuote>,
}

#[derive(Debug, Deserialize)]
struct CmcResponse {
    data: Vec<CmcCoin>,
}

async fn from_coingecko(page: u32) -> Result<Vec<MarketCapCoin>, String> {
    let url = format!(
        "{COINGECKO}?vs_currency=usd&order=market_cap_desc&per_page=100&page={page}\
         &sparkline=false&price_change_percentage=24h"
    );
    let coins: Vec<GeckoCoin> = client()?
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("coingecko: {e}"))?
        .error_for_status()
        .map_err(|e| format!("coingecko: {e}"))?
        .json()
        .await
        .map_err(|e| format!("coingecko: unexpected response: {e}"))?;

    Ok(coins
        .into_iter()
        .map(|c| MarketCapCoin {
            rank: c.market_cap_rank.unwrap_or(0),
            symbol: c.symbol.to_uppercase(),
            name: c.name,
            price: c.current_price.unwrap_or(0.0),
            market_cap: c.market_cap.unwrap_or(0.0),
            change: c.price_change_percentage_24h.unwrap_or(0.0),
        })
        .collect())
}

async fn from_coinmarketcap(page: u32, api_key: &str) -> Result<Vec<MarketCapCoin>, String> {
    if api_key.is_empty() {
        return Err("CoinMarketCap API key is not set â€” add it in Settings.".into());
    }
    let start = (page.saturating_sub(1)) * 100 + 1;
    let url = format!("{CMC}?start={start}&limit=100&convert=USD&sort=market_cap");

    let body: CmcResponse = client()?
        .get(&url)
        .header("X-CMC_PRO_API_KEY", api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("coinmarketcap: {e}"))?
        .error_for_status()
        .map_err(|e| format!("coinmarketcap: {e}"))?
        .json()
        .await
        .map_err(|e| format!("coinmarketcap: unexpected response: {e}"))?;

    Ok(body
        .data
        .into_iter()
        .map(|c| {
            let q = c.quote.get("USD");
            MarketCapCoin {
                rank: c.cmc_rank.unwrap_or(0),
                symbol: c.symbol,
                name: c.name,
                price: q.and_then(|q| q.price).unwrap_or(0.0),
                market_cap: q.and_then(|q| q.market_cap).unwrap_or(0.0),
                change: q.and_then(|q| q.percent_change_24h).unwrap_or(0.0),
            }
        })
        .collect())
}

/// Five-minute cache, and stale-on-error â€” both carried over from the old route,
/// which needed them because CoinGecko rate-limits aggressively.
pub struct MarketCapCache {
    entries: std::sync::Mutex<HashMap<String, (std::time::Instant, Vec<MarketCapCoin>)>>,
}

const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl MarketCapCache {
    pub fn new() -> Self {
        Self { entries: std::sync::Mutex::new(HashMap::new()) }
    }

    fn get(&self, key: &str, fresh_only: bool) -> Option<Vec<MarketCapCoin>> {
        let map = self.entries.lock().ok()?;
        let (at, data) = map.get(key)?;
        if fresh_only && at.elapsed() >= CACHE_TTL {
            return None;
        }
        Some(data.clone())
    }

    fn put(&self, key: String, data: Vec<MarketCapCoin>) {
        if let Ok(mut map) = self.entries.lock() {
            map.insert(key, (std::time::Instant::now(), data));
        }
    }
}

impl Default for MarketCapCache {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn market_cap(
    cache: &MarketCapCache,
    page: u32,
    provider: &str,
    cmc_api_key: &str,
) -> Result<Vec<MarketCapCoin>, String> {
    let page = page.max(1);
    let key = format!("{provider}:{page}");

    if let Some(fresh) = cache.get(&key, true) {
        return Ok(fresh);
    }

    let result = if provider == "coinmarketcap" {
        from_coinmarketcap(page, cmc_api_key).await
    } else {
        from_coingecko(page).await
    };

    match result {
        Ok(data) => {
            cache.put(key, data.clone());
            Ok(data)
        }
        Err(e) => match cache.get(&key, false) {
            Some(stale) => Ok(stale),
            None => Err(format!("Failed to fetch from {provider}: {e}")),
        },
    }
}

// ── Daily change (since the 00:00 UTC open) ──────────────────────────────────
//
// The ladder's change column toggles between the rolling 24h number (already
// on `MarketCapCoin`) and the change since the trading day opened. Neither
// CoinGecko's markets endpoint nor CMC's listings carry the latter, so it
// comes from Binance's trading-day ticker for the coins that have a USDT pair
// there. Coins without one (stablecoins, off-Binance listings) are absent
// from the result and the frontend renders them as "—".

const SYMBOL_SET_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The symbol set together with the time it was fetched. Named rather than
/// spelled out at each use: the bare `Mutex<Option<(Instant, HashSet<_>)>>` is
/// over `clippy::type_complexity`'s threshold, which fails the workspace lint
/// gate outright.
type SymbolSetCache =
    std::sync::Mutex<Option<(std::time::Instant, std::collections::HashSet<String>)>>;

fn symbol_set_cache() -> &'static SymbolSetCache {
    static CACHE: std::sync::OnceLock<SymbolSetCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Every symbol Binance currently trades, cached for an hour. Needed because a
/// batched ticker request 400s WHOLE if any one symbol is unknown — the list
/// must be filtered before asking.
async fn binance_listed_symbols() -> Result<std::collections::HashSet<String>, String> {
    if let Ok(guard) = symbol_set_cache().lock() {
        if let Some((at, set)) = guard.as_ref() {
            if at.elapsed() < SYMBOL_SET_TTL {
                return Ok(set.clone());
            }
        }
    }

    #[derive(Deserialize)]
    struct PriceRow {
        symbol: String,
    }

    let rows: Vec<PriceRow> = client()?
        .get(format!("{BINANCE}/ticker/price"))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch symbol list: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch symbol list: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse symbol list: {e}"))?;

    let set: std::collections::HashSet<String> = rows.into_iter().map(|r| r.symbol).collect();
    if let Ok(mut guard) = symbol_set_cache().lock() {
        *guard = Some((std::time::Instant::now(), set.clone()));
    }
    Ok(set)
}

#[derive(Debug, Deserialize)]
struct TradingDayRow {
    symbol: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
}

/// Percent change since the UTC trading-day open, keyed by coin symbol
/// (`"BTC"` → +1.23). Only Binance-listed USDT pairs appear in the map.
pub async fn daily_change(symbols: &[String]) -> Result<HashMap<String, f64>, String> {
    let listed = binance_listed_symbols().await?;

    // pair -> coin, keeping only what Binance actually lists.
    let by_pair: HashMap<String, String> = symbols
        .iter()
        .map(|s| s.trim().to_uppercase())
        .filter(|c| !c.is_empty())
        .map(|c| (format!("{c}USDT"), c))
        .filter(|(pair, _)| listed.contains(pair))
        .collect();

    let pair_list: Vec<String> = by_pair.keys().cloned().collect();
    let mut out = HashMap::new();

    // The endpoint takes at most 100 symbols per request.
    for chunk in pair_list.chunks(100) {
        let list: Vec<String> = chunk.iter().map(|p| format!("\"{p}\"")).collect();
        let url = format!("{BINANCE}/ticker/tradingDay?symbols=[{}]", list.join(","));

        let rows: Vec<TradingDayRow> = client()?
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch daily change: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Failed to fetch daily change: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse daily change: {e}"))?;

        for r in rows {
            if let Some(coin) = by_pair.get(&r.symbol) {
                out.insert(coin.clone(), p(&r.price_change_percent));
            }
        }
    }

    Ok(out)
}

// ── Sentiment — crypto Fear & Greed Index ────────────────────────────────────
//
// Real market sentiment from alternative.me's Fear & Greed Index — a free,
// keyless public API. This replaces the simulated sentiment cards that used
// to live on the metrics page (V3.1, user request 2026-08-15).

#[derive(Debug, Clone, Serialize)]
pub struct SentimentPoint {
    /// Unix seconds.
    pub time: i64,
    /// 0 (extreme fear) – 100 (extreme greed).
    pub value: u8,
    /// The API's own label, e.g. "Fear", "Greed", "Extreme Greed".
    pub classification: String,
}

/// Oldest-first history of the Fear & Greed Index, `limit` days deep.
pub async fn sentiment(limit: u32) -> Result<Vec<SentimentPoint>, String> {
    // Alternative.me defines zero as its complete available history.
    let limit = limit.min(10000);
    let url = format!("https://api.alternative.me/fng/?limit={limit}&format=json");

    let body: serde_json::Value = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch sentiment: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Failed to fetch sentiment: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse sentiment: {e}"))?;

    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    let mut points: Vec<SentimentPoint> = rows
        .iter()
        .filter_map(|r| {
            Some(SentimentPoint {
                time: r.get("timestamp")?.as_str()?.parse().ok()?,
                value: r.get("value")?.as_str()?.parse().ok()?,
                classification: r.get("value_classification")?.as_str()?.to_string(),
            })
        })
        .collect();

    if points.is_empty() {
        return Err("Sentiment feed returned no data".into());
    }

    // The API returns newest first; sparklines want oldest first.
    points.reverse();
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_roundtrip() {
        assert_eq!(to_binance_symbol("BTC/USDT"), "BTCUSDT");
        assert_eq!(from_binance_symbol("BTCUSDT"), "BTC/USDT");
        assert_eq!(from_binance_symbol("PEPEUSDT"), "PEPE/USDT");
    }

    #[test]
    fn volume_formatting_matches_the_old_route() {
        assert_eq!(format_volume(1.25e9), "1.2B");
        assert_eq!(format_volume(3.5e6), "3.5M");
        assert_eq!(format_volume(1500.0), "1.5K");
        assert_eq!(format_volume(42.4), "42");
    }

    #[test]
    fn perfect_correlation_is_one() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert!((pearson(&a, &a) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn inverse_correlation_is_minus_one() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [4.0, 3.0, 2.0, 1.0];
        assert!((pearson(&a, &b) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn log_returns_drop_the_first_point_and_skip_zeros() {
        assert_eq!(log_returns(&[1.0, 1.0, 1.0]), vec![0.0, 0.0]);
        assert_eq!(log_returns(&[0.0, 5.0]).len(), 0);
    }

    // â”€â”€ Live upstream checks â”€â”€
    //
    // Ignored by default: they hit Binance and CoinGecko, so they need network
    // and they are subject to rate limits. Run them when touching this file:
    //
    //     cargo test -p tauri-plugin-terminal -- --ignored
    //
    // They exist because a port like this compiles perfectly while silently
    // mapping the wrong field.

    #[tokio::test]
    #[ignore = "hits the live Binance API"]
    async fn live_ohlcv_has_sane_candles() {
        let candles = ohlcv("BTC/USDT", "1d", 5, None).await.expect("ohlcv");
        assert_eq!(candles.len(), 5);
        for c in &candles {
            assert!(c.time > 1_500_000_000, "time must be unix SECONDS, got {}", c.time);
            assert!(c.high >= c.low, "high < low");
            assert!(c.open > 0.0 && c.close > 0.0 && c.volume >= 0.0);
        }
        assert!(candles[0].time < candles[4].time, "candles must be chronological");
    }

    #[tokio::test]
    #[ignore = "hits the live Binance API"]
    async fn live_tickers_keep_the_frontend_shape() {
        let rows = tickers().await.expect("tickers");
        assert!(!rows.is_empty());
        for r in &rows {
            assert!(r.symbol.contains('/'), "symbol must stay CCXT-style, got {}", r.symbol);
            assert!(r.price > 0.0);
            assert!(!r.volume.is_empty(), "volume is a preformatted string");
        }
    }

    #[tokio::test]
    #[ignore = "hits the live Binance API"]
    async fn live_order_book_is_sorted_and_capped() {
        let book = order_book("BTC/USDT", 10).await.expect("order book");
        assert_eq!(book.bids.len(), 10);
        assert_eq!(book.asks.len(), 10);
        assert!(book.asks[0].price >= book.bids[0].price, "ask below bid");
    }

    #[tokio::test]
    #[ignore = "hits the live Binance API"]
    async fn live_correlation_is_symmetric_with_unit_diagonal() {
        let symbols = vec!["BTC".to_string(), "ETH".to_string()];
        let c = correlation(&symbols, "7d").await.expect("correlation");
        assert_eq!(c.symbols.len(), 2);
        for s in &c.symbols {
            assert_eq!(c.matrix[s][s], 1.0, "diagonal must be exactly 1");
        }
        let ab = c.matrix["BTC"]["ETH"];
        let ba = c.matrix["ETH"]["BTC"];
        assert!((ab - ba).abs() < 1e-9, "matrix must be symmetric");
        assert!((-1.0..=1.0).contains(&ab), "correlation out of range: {ab}");
    }

    #[tokio::test]
    #[ignore = "hits the live Binance API"]
    async fn live_daily_change_covers_majors_and_skips_unlisted() {
        let symbols = vec!["BTC".to_string(), "eth".to_string(), "NOTACOIN".to_string()];
        let out = daily_change(&symbols).await.expect("daily change");
        assert!(out.contains_key("BTC"), "BTC/USDT is on Binance");
        assert!(out.contains_key("ETH"), "lowercase input must still resolve");
        assert!(!out.contains_key("NOTACOIN"), "unlisted coins must be absent, not zero");
    }

    #[tokio::test]
    #[ignore = "hits the live CoinGecko API"]
    async fn live_market_cap_is_ranked() {
        let cache = MarketCapCache::new();
        let coins = market_cap(&cache, 1, "coingecko", "").await.expect("market cap");
        assert!(!coins.is_empty());
        assert_eq!(coins[0].rank, 1, "page 1 should start at rank 1");
        assert!(coins[0].market_cap > 0.0);
        assert!(coins[0].symbol.chars().all(|c| !c.is_lowercase()), "symbol must be uppercased");
    }
}
