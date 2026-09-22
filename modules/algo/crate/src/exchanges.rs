use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::State;
use uuid::Uuid;
use crate::{AppState, BacktestConfig, Balance, ConnectionResult, Exchange, ExchangeConfig};
use crate::settings::get_data_dir;

type HmacSha256 = Hmac<Sha256>;

const PAIR_CACHE_TTL_SECS: i64 = 15 * 60;

#[derive(Clone)]
struct CachedPairs {
    fetched_at: chrono::DateTime<Utc>,
    pairs: Vec<String>,
}

static PAIR_CACHE: OnceLock<Mutex<HashMap<String, CachedPairs>>> = OnceLock::new();

fn pair_cache() -> &'static Mutex<HashMap<String, CachedPairs>> {
    PAIR_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const LEGACY_APP_ENCRYPTION_KEY: &[u8; 32] = b"QuantAlgo_AES256_Key_2024!@#$%^&";

fn derive_encryption_key() -> [u8; 32] {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "local-user".to_string());
    let material = format!("quantalgo:v2:{}:{}", user, get_data_dir().to_string_lossy());
    let digest = Sha256::digest(material.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

pub fn encrypt_string(plaintext: &str) -> Result<String, String> {
    let key = derive_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Cipher init: {e}"))?;
    let nonce_bytes: [u8; 12] = {
        use aes_gcm::aead::rand_core::RngCore;
        let mut buf = [0u8; 12];
        OsRng.fill_bytes(&mut buf);
        buf
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encrypt: {e}"))?;
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(hex::encode(combined))
}

fn decrypt_with_key(data: &[u8], key: &[u8; 32]) -> Result<String, String> {
    if data.len() < 13 {
        return Err("Ciphertext too short".into());
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Cipher init: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decrypt: {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("UTF-8: {e}"))
}

pub fn decrypt_string(hex_str: &str) -> Result<String, String> {
    let data = hex::decode(hex_str).map_err(|e| format!("Hex decode: {e}"))?;
    let key = derive_encryption_key();
    decrypt_with_key(&data, &key).or_else(|_| decrypt_with_key(&data, LEGACY_APP_ENCRYPTION_KEY))
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC key length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

fn hmac_sha256_base64(key: &[u8], message: &[u8]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC key length");
    mac.update(message);
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// Fetch the exchange's server time in milliseconds to avoid clock drift rejections.
/// Falls back to local time if the request fails.
fn get_server_time_ms(client: &reqwest::blocking::Client, provider: &str) -> i64 {
    let fallback = chrono::Utc::now().timestamp_millis();
    let result: Option<i64> = match provider {
        "binance" => client
            .get("https://api.binance.com/api/v3/time")
            .send()
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|j| j.get("serverTime").and_then(|v| v.as_i64())),
        "bybit" => client
            .get("https://api.bybit.com/v5/market/time")
            .send()
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|j| {
                j.pointer("/result/timeSecond")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|s| (s * 1000.0) as i64)
                    .or_else(|| j.get("time").and_then(|v| v.as_i64()))
            }),
        "kucoin" => client
            .get("https://api.kucoin.com/api/v1/timestamp")
            .send()
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|j| j.get("data").and_then(|v| v.as_i64())),
        _ => None,
    };
    result.unwrap_or(fallback)
}

/// One HTTP client shape for every exchange call. Coinbase rejects requests
/// without a `User-Agent` outright (HTTP 400 "User-Agent header is
/// required"), and the other venues are happier identifying their callers.
fn http_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))
}

fn parse_date_to_utc(input: &str, end_of_day: bool) -> Result<chrono::DateTime<Utc>, String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(ndt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt));
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date '{input}': {e}"))?;
    let time = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    }
    .ok_or_else(|| format!("Invalid date '{input}'"))?;

    Ok(Utc.from_utc_datetime(&time))
}

fn timeframe_seconds(timeframe: &str) -> i64 {
    match timeframe {
        "1m" => 60,
        "5m" => 300,
        "15m" => 900,
        "1h" => 3600,
        "4h" => 14_400,
        "1d" => 86_400,
        "1w" => 604_800,
        _ => 3600,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PaperMarketCandle {
    pub(crate) time: String,
    pub(crate) open: f64,
    pub(crate) high: f64,
    pub(crate) low: f64,
    pub(crate) close: f64,
    pub(crate) volume: f64,
}

// ---------------------------------------------------------------------------
// Real exchange candle fetching
// ---------------------------------------------------------------------------

fn pair_for_exchange(pair: &str, provider: &str) -> String {
    // "BTC/USDT" -> exchange-specific format
    match provider {
        "binance" => pair.replace('/', ""),   // BTCUSDT
        "bybit" => pair.replace('/', ""),     // BTCUSDT
        "okx" => pair.replace('/', "-"),      // BTC-USDT
        "coinbase" => pair.replace('/', "-"), // BTC-USDT
        "kucoin" => pair.replace('/', "-"),   // BTC-USDT
        "kraken" => pair.replace("BTC", "XBT").replace('/', ""), // XBTUSDT
        _ => pair.replace('/', ""),
    }
}

fn interval_for_exchange(timeframe: &str, provider: &str) -> String {
    match provider {
        "binance" => timeframe.to_string(), // 1m, 5m, 15m, 1h, 4h, 1d
        "bybit" => match timeframe {
            // 1, 5, 15, 60, 240, D
            "1m" => "1",
            "5m" => "5",
            "15m" => "15",
            "1h" => "60",
            "4h" => "240",
            "1d" => "D",
            "1w" => "W",
            _ => "60",
        }
        .to_string(),
        "okx" => match timeframe {
            // 1m, 5m, 15m, 1H, 4H, 1D
            "1h" => "1H".to_string(),
            "4h" => "4H".to_string(),
            "1d" => "1D".to_string(),
            "1w" => "1W".to_string(),
            other => other.to_string(),
        },
        "coinbase" => match timeframe {
            // seconds: 60, 300, 900, 3600, 21600, 86400
            "1m" => "60",
            "5m" => "300",
            "15m" => "900",
            "1h" => "3600",
            "4h" => "21600",
            "1d" => "86400",
            _ => "3600",
        }
        .to_string(),
        "kraken" => match timeframe {
            // 1, 5, 15, 30, 60, 240, 1440, 10080
            "1m" => "1",
            "5m" => "5",
            "15m" => "15",
            "1h" => "60",
            "4h" => "240",
            "1d" => "1440",
            "1w" => "10080",
            _ => "60",
        }
        .to_string(),
        "kucoin" => match timeframe {
            "1m" => "1min",
            "5m" => "5min",
            "15m" => "15min",
            "1h" => "1hour",
            "4h" => "4hour",
            "1d" => "1day",
            "1w" => "1week",
            _ => "1hour",
        }
        .to_string(),
        _ => timeframe.to_string(),
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<f64>().ok()))
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
}

fn candle_time_from_millis(ts_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn candle_time_from_seconds(ts_secs: i64) -> String {
    chrono::DateTime::from_timestamp(ts_secs, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

fn candle_from_array(
    row: &[Value],
    ts_idx: usize,
    open_idx: usize,
    high_idx: usize,
    low_idx: usize,
    close_idx: usize,
    volume_idx: usize,
    timestamp_in_seconds: bool,
) -> Result<PaperMarketCandle, String> {
    let ts = row
        .get(ts_idx)
        .and_then(value_as_i64)
        .ok_or_else(|| "candle timestamp missing".to_string())?;
    let open = row
        .get(open_idx)
        .and_then(value_as_f64)
        .ok_or_else(|| "candle open missing".to_string())?;
    let high = row
        .get(high_idx)
        .and_then(value_as_f64)
        .ok_or_else(|| "candle high missing".to_string())?;
    let low = row
        .get(low_idx)
        .and_then(value_as_f64)
        .ok_or_else(|| "candle low missing".to_string())?;
    let close = row
        .get(close_idx)
        .and_then(value_as_f64)
        .ok_or_else(|| "candle close missing".to_string())?;
    let volume = row.get(volume_idx).and_then(value_as_f64).unwrap_or(0.0);

    Ok(PaperMarketCandle {
        time: if timestamp_in_seconds {
            candle_time_from_seconds(ts)
        } else {
            candle_time_from_millis(ts)
        },
        open,
        high,
        low,
        close,
        volume,
    })
}

pub(crate) fn fetch_latest_market_candle(
    provider: &str,
    pair: &str,
    timeframe: &str,
) -> Result<PaperMarketCandle, String> {
    let provider = provider.to_lowercase();
    let symbol = pair_for_exchange(pair, &provider);
    let interval = interval_for_exchange(timeframe, &provider);
    let client = http_client(10)?;

    match provider.as_str() {
        "binance" => {
            let resp = client
                .get("https://api.binance.com/api/v3/klines")
                .query(&[
                    ("symbol", symbol.as_str()),
                    ("interval", interval.as_str()),
                    ("limit", "2"),
                ])
                .send()
                .map_err(|e| format!("Binance market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Binance market data HTTP {}", resp.status()));
            }
            let body: Vec<Vec<Value>> = resp
                .json()
                .map_err(|e| format!("Binance market data parse: {e}"))?;
            let row = body
                .last()
                .ok_or_else(|| "Binance returned no candles".to_string())?;
            candle_from_array(row, 0, 1, 2, 3, 4, 5, false)
        }
        "bybit" => {
            let resp = client
                .get("https://api.bybit.com/v5/market/kline")
                .query(&[
                    ("category", "spot"),
                    ("symbol", symbol.as_str()),
                    ("interval", interval.as_str()),
                    ("limit", "2"),
                ])
                .send()
                .map_err(|e| format!("Bybit market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Bybit market data HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("Bybit market data parse: {e}"))?;
            if body
                .get("retCode")
                .and_then(|value| value.as_i64())
                .unwrap_or(0)
                != 0
            {
                let msg = body
                    .get("retMsg")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("Bybit market data error: {msg}"));
            }
            let list = body
                .pointer("/result/list")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "Bybit returned no candles".to_string())?;
            let row = list
                .first()
                .and_then(|value| value.as_array())
                .ok_or_else(|| "Bybit returned malformed candle".to_string())?;
            candle_from_array(row, 0, 1, 2, 3, 4, 5, false)
        }
        "okx" => {
            let resp = client
                .get("https://www.okx.com/api/v5/market/candles")
                .query(&[
                    ("instId", symbol.as_str()),
                    ("bar", interval.as_str()),
                    ("limit", "2"),
                ])
                .send()
                .map_err(|e| format!("OKX market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("OKX market data HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("OKX market data parse: {e}"))?;
            let data = body
                .get("data")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "OKX returned no candles".to_string())?;
            let row = data
                .first()
                .and_then(|value| value.as_array())
                .ok_or_else(|| "OKX returned malformed candle".to_string())?;
            candle_from_array(row, 0, 1, 2, 3, 4, 5, false)
        }
        "coinbase" => {
            let url = format!("https://api.exchange.coinbase.com/products/{symbol}/candles");
            let resp = client
                .get(&url)
                .query(&[("granularity", interval.as_str())])
                .send()
                .map_err(|e| format!("Coinbase market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Coinbase market data HTTP {}", resp.status()));
            }
            let body: Vec<Vec<Value>> = resp
                .json()
                .map_err(|e| format!("Coinbase market data parse: {e}"))?;
            let row = body
                .iter()
                .max_by_key(|row| row.first().and_then(value_as_i64).unwrap_or(0))
                .ok_or_else(|| "Coinbase returned no candles".to_string())?;
            // Coinbase candle layout: [time, low, high, open, close, volume].
            candle_from_array(row, 0, 3, 2, 1, 4, 5, true)
        }
        "kraken" => {
            let resp = client
                .get("https://api.kraken.com/0/public/OHLC")
                .query(&[("pair", symbol.as_str()), ("interval", interval.as_str())])
                .send()
                .map_err(|e| format!("Kraken market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Kraken market data HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("Kraken market data parse: {e}"))?;
            if let Some(err) = body.get("error").and_then(|value| value.as_array()) {
                if !err.is_empty() {
                    return Err(format!("Kraken market data error: {:?}", err));
                }
            }
            let result = body
                .get("result")
                .and_then(|value| value.as_object())
                .ok_or_else(|| "Kraken returned no result".to_string())?;
            let pair_data = result
                .iter()
                .find(|(key, _)| key.as_str() != "last")
                .and_then(|(_, value)| value.as_array())
                .ok_or_else(|| "Kraken returned no candles".to_string())?;
            let row = pair_data
                .last()
                .and_then(|value| value.as_array())
                .ok_or_else(|| "Kraken returned malformed candle".to_string())?;
            // Kraken candle layout: [time, open, high, low, close, vwap, volume, count].
            candle_from_array(row, 0, 1, 2, 3, 4, 6, true)
        }
        "kucoin" => {
            let resp = client
                .get("https://api.kucoin.com/api/v1/market/candles")
                .query(&[("type", interval.as_str()), ("symbol", symbol.as_str())])
                .send()
                .map_err(|e| format!("KuCoin market data request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("KuCoin market data HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("KuCoin market data parse: {e}"))?;
            if body
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("200000")
                != "200000"
            {
                return Err(format!("KuCoin market data error: {}", body));
            }
            let data = body
                .get("data")
                .and_then(|value| value.as_array())
                .ok_or_else(|| "KuCoin returned no candles".to_string())?;
            let row = data
                .first()
                .and_then(|value| value.as_array())
                .ok_or_else(|| "KuCoin returned malformed candle".to_string())?;
            // KuCoin candle layout: [time, open, close, high, low, volume, turnover].
            candle_from_array(row, 0, 1, 3, 4, 2, 5, true)
        }
        other => Err(format!(
            "Public market data is not supported for {other}; choose a supported CEX provider."
        )),
    }
}

fn sort_and_trim_recent_candles(
    mut candles: Vec<PaperMarketCandle>,
    limit: usize,
) -> Vec<PaperMarketCandle> {
    candles.sort_by(|a, b| a.time.cmp(&b.time));
    candles.dedup_by(|a, b| a.time == b.time);
    if candles.len() > limit {
        candles.split_off(candles.len() - limit)
    } else {
        candles
    }
}

pub(crate) fn fetch_recent_market_candles(
    provider: &str,
    pair: &str,
    timeframe: &str,
    limit: usize,
) -> Result<Vec<PaperMarketCandle>, String> {
    let provider = provider.to_lowercase();
    let symbol = pair_for_exchange(pair, &provider);
    let interval = interval_for_exchange(timeframe, &provider);
    let limit = limit.clamp(2, 500);
    let limit_s = limit.to_string();
    let client = http_client(15)?;

    let candles = match provider.as_str() {
        "binance" => {
            let resp = client
                .get("https://api.binance.com/api/v3/klines")
                .query(&[
                    ("symbol", symbol.as_str()),
                    ("interval", interval.as_str()),
                    ("limit", limit_s.as_str()),
                ])
                .send()
                .map_err(|e| format!("Binance warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Binance warm-up HTTP {}", resp.status()));
            }
            let body: Vec<Vec<Value>> = resp
                .json()
                .map_err(|e| format!("Binance warm-up parse: {e}"))?;
            body.iter()
                .filter_map(|row| candle_from_array(row, 0, 1, 2, 3, 4, 5, false).ok())
                .collect::<Vec<_>>()
        }
        "bybit" => {
            let resp = client
                .get("https://api.bybit.com/v5/market/kline")
                .query(&[
                    ("category", "spot"),
                    ("symbol", symbol.as_str()),
                    ("interval", interval.as_str()),
                    ("limit", limit_s.as_str()),
                ])
                .send()
                .map_err(|e| format!("Bybit warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Bybit warm-up HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("Bybit warm-up parse: {e}"))?;
            if body
                .get("retCode")
                .and_then(|value| value.as_i64())
                .unwrap_or(0)
                != 0
            {
                let msg = body
                    .get("retMsg")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("Bybit warm-up error: {msg}"));
            }
            body.pointer("/result/list")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_array())
                        .filter_map(|row| candle_from_array(row, 0, 1, 2, 3, 4, 5, false).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "okx" => {
            let resp = client
                .get("https://www.okx.com/api/v5/market/candles")
                .query(&[
                    ("instId", symbol.as_str()),
                    ("bar", interval.as_str()),
                    ("limit", limit_s.as_str()),
                ])
                .send()
                .map_err(|e| format!("OKX warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("OKX warm-up HTTP {}", resp.status()));
            }
            let body: Value = resp.json().map_err(|e| format!("OKX warm-up parse: {e}"))?;
            body.get("data")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_array())
                        .filter_map(|row| candle_from_array(row, 0, 1, 2, 3, 4, 5, false).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "coinbase" => {
            let url = format!("https://api.exchange.coinbase.com/products/{symbol}/candles");
            let end = Utc::now();
            let start =
                end - chrono::Duration::seconds(timeframe_seconds(timeframe) * limit as i64);
            let start_s = start.to_rfc3339();
            let end_s = end.to_rfc3339();
            let resp = client
                .get(&url)
                .query(&[
                    ("granularity", interval.as_str()),
                    ("start", start_s.as_str()),
                    ("end", end_s.as_str()),
                ])
                .send()
                .map_err(|e| format!("Coinbase warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Coinbase warm-up HTTP {}", resp.status()));
            }
            let body: Vec<Vec<Value>> = resp
                .json()
                .map_err(|e| format!("Coinbase warm-up parse: {e}"))?;
            body.iter()
                .filter_map(|row| candle_from_array(row, 0, 3, 2, 1, 4, 5, true).ok())
                .collect::<Vec<_>>()
        }
        "kraken" => {
            let since = (Utc::now()
                - chrono::Duration::seconds(timeframe_seconds(timeframe) * limit as i64))
            .timestamp()
            .to_string();
            let resp = client
                .get("https://api.kraken.com/0/public/OHLC")
                .query(&[
                    ("pair", symbol.as_str()),
                    ("interval", interval.as_str()),
                    ("since", since.as_str()),
                ])
                .send()
                .map_err(|e| format!("Kraken warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Kraken warm-up HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("Kraken warm-up parse: {e}"))?;
            if let Some(err) = body.get("error").and_then(|value| value.as_array()) {
                if !err.is_empty() {
                    return Err(format!("Kraken warm-up error: {:?}", err));
                }
            }
            body.get("result")
                .and_then(|value| value.as_object())
                .and_then(|result| {
                    result
                        .iter()
                        .find(|(key, _)| key.as_str() != "last")
                        .and_then(|(_, value)| value.as_array())
                })
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_array())
                        .filter_map(|row| candle_from_array(row, 0, 1, 2, 3, 4, 6, true).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "kucoin" => {
            let end = Utc::now().timestamp().to_string();
            let start = (Utc::now()
                - chrono::Duration::seconds(timeframe_seconds(timeframe) * limit as i64))
            .timestamp()
            .to_string();
            let resp = client
                .get("https://api.kucoin.com/api/v1/market/candles")
                .query(&[
                    ("type", interval.as_str()),
                    ("symbol", symbol.as_str()),
                    ("startAt", start.as_str()),
                    ("endAt", end.as_str()),
                ])
                .send()
                .map_err(|e| format!("KuCoin warm-up request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("KuCoin warm-up HTTP {}", resp.status()));
            }
            let body: Value = resp
                .json()
                .map_err(|e| format!("KuCoin warm-up parse: {e}"))?;
            if body
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("200000")
                != "200000"
            {
                return Err(format!("KuCoin warm-up error: {}", body));
            }
            body.get("data")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_array())
                        .filter_map(|row| candle_from_array(row, 0, 1, 3, 4, 2, 5, true).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        other => {
            return Err(format!(
                "Public market data warm-up is not supported for {other}; choose a supported CEX provider."
            ));
        }
    };

    let candles = sort_and_trim_recent_candles(candles, limit);
    if candles.is_empty() {
        Err(format!(
            "No warm-up candles returned from {provider} for {pair} ({timeframe})."
        ))
    } else {
        Ok(candles)
    }
}

pub(crate) fn market_candle_json(candle: &PaperMarketCandle, pair: &str) -> Value {
    serde_json::json!({
        "time": candle.time.clone(),
        "open": candle.open,
        "high": candle.high,
        "low": candle.low,
        "close": candle.close,
        "volume": candle.volume,
        "pair": pair,
    })
}

/// The progress-free form the provider tests call.
#[cfg(test)]
pub(crate) fn fetch_historical_candles(config: &BacktestConfig) -> Result<Vec<Value>, String> {
    fetch_historical_candles_with_progress(config, &mut |_| {})
}

/// [`fetch_historical_candles`] reporting the share of the requested range
/// fetched so far (0..=1) before every page — a month of 1-minute candles is
/// over 200 Bybit pages, minutes in which the caller would otherwise be silent.
pub(crate) fn fetch_historical_candles_with_progress(
    config: &BacktestConfig,
    on_progress: &mut dyn FnMut(f64),
) -> Result<Vec<Value>, String> {
    let provider = config.exchange.to_lowercase();
    let start = parse_date_to_utc(&config.start_date, false)?;
    let end = parse_date_to_utc(&config.end_date, true)?;
    let symbol = pair_for_exchange(&config.pair, &provider);
    let interval = interval_for_exchange(&config.timeframe, &provider);
    let interval_ms = timeframe_seconds(&config.timeframe) * 1000;

    let client = http_client(30)?;

    let mut all_candles: Vec<Value> = Vec::new();
    let start_ms = start.timestamp_millis();
    let mut cursor_ms = start_ms;
    let end_ms = end.timestamp_millis();
    let span_ms = (end_ms - start_ms).max(1) as f64;
    let max_per_page: i64 = match provider.as_str() {
        "binance" => 1000,
        "bybit" => 200,
        "okx" => 100,
        "coinbase" => 300,
        "kraken" => 720,
        "kucoin" => 1500,
        _ => 1000,
    };

    // Bybit, OKX, KuCoin and Coinbase are asked for one window of bars per
    // call — `[cursor, page_end]`, inclusive at both ends, sized so it holds
    // exactly one page. Bybit and OKX answer a plain start-plus-limit with the
    // NEWEST page of the whole range (that used to drop everything before the
    // last 200 bars), so they are windowed too. Binance and Kraken page
    // forward from a start on their own.
    let windowed = matches!(provider.as_str(), "bybit" | "okx" | "kucoin" | "coinbase");

    loop {
        if cursor_ms >= end_ms {
            break;
        }
        on_progress(((cursor_ms - start_ms) as f64 / span_ms).clamp(0.0, 1.0));
        let page_end_ms = (cursor_ms + (max_per_page - 1) * interval_ms).min(end_ms);

        let raw: Vec<Value> = match provider.as_str() {
            "binance" => {
                let resp = client
                    .get("https://api.binance.com/api/v3/klines")
                    .query(&[
                        ("symbol", symbol.as_str()),
                        ("interval", interval.as_str()),
                        ("limit", &max_per_page.to_string()),
                    ])
                    .query(&[
                        ("startTime", &cursor_ms.to_string()),
                        ("endTime", &end_ms.to_string()),
                    ])
                    .send()
                    .map_err(|e| format!("Binance request: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("Binance API error: HTTP {}", resp.status()));
                }
                let body: Vec<Vec<Value>> =
                    resp.json().map_err(|e| format!("Binance parse: {e}"))?;
                body.into_iter()
                    .map(|k| {
                        let ts_ms = k[0].as_i64().unwrap_or(0);
                        let time = chrono::DateTime::from_timestamp_millis(ts_ms)
                            .unwrap_or(Utc::now())
                            .to_rfc3339();
                        serde_json::json!({
                            "time": time,
                            "open": k[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "high": k[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "low": k[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "close": k[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "volume": k[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                        })
                    })
                    .collect()
            }
            "bybit" => {
                let resp = client
                    .get("https://api.bybit.com/v5/market/kline")
                    .query(&[
                        ("category", "spot"),
                        ("symbol", &symbol),
                        ("interval", &interval),
                        ("limit", &max_per_page.to_string()),
                    ])
                    .query(&[
                        ("start", &cursor_ms.to_string()),
                        ("end", &page_end_ms.to_string()),
                    ])
                    .send()
                    .map_err(|e| format!("Bybit request: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("Bybit API error: HTTP {}", resp.status()));
                }
                let body: Value = resp.json().map_err(|e| format!("Bybit parse: {e}"))?;
                let list = body["result"]["list"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut candles: Vec<Value> = list
                    .into_iter()
                    .map(|k| {
                        let arr = k.as_array().unwrap();
                        let ts_ms = arr[0].as_str().unwrap_or("0").parse::<i64>().unwrap_or(0);
                        let time = chrono::DateTime::from_timestamp_millis(ts_ms)
                            .unwrap_or(Utc::now())
                            .to_rfc3339();
                        serde_json::json!({
                            "time": time,
                            "open": arr[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "high": arr[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "low": arr[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "close": arr[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "volume": arr[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                        })
                    })
                    .collect();
                // Bybit returns newest-first
                candles.reverse();
                candles
            }
            "okx" => {
                // `after` = "records earlier than this ts", newest first, at
                // most `limit` of them: asking from one bar past the window's
                // end returns exactly the window. (`before` alongside it made
                // the two bounds contradict each other and the answer empty.)
                let after = (page_end_ms + interval_ms).to_string();
                let resp = client
                    .get("https://www.okx.com/api/v5/market/history-candles")
                    .query(&[
                        ("instId", symbol.as_str()),
                        ("bar", interval.as_str()),
                        ("limit", &max_per_page.to_string()),
                        ("after", after.as_str()),
                    ])
                    .send()
                    .map_err(|e| format!("OKX request: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("OKX API error: HTTP {}", resp.status()));
                }
                let body: Value = resp.json().map_err(|e| format!("OKX parse: {e}"))?;
                let data = body["data"].as_array().cloned().unwrap_or_default();
                let mut candles: Vec<Value> = data
                    .into_iter()
                    .map(|k| {
                        let arr = k.as_array().unwrap();
                        let ts_ms = arr[0].as_str().unwrap_or("0").parse::<i64>().unwrap_or(0);
                        let time = chrono::DateTime::from_timestamp_millis(ts_ms)
                            .unwrap_or(Utc::now())
                            .to_rfc3339();
                        serde_json::json!({
                            "time": time,
                            "open": arr[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "high": arr[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "low": arr[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "close": arr[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "volume": arr[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                        })
                    })
                    .collect();
                // OKX returns newest-first
                candles.reverse();
                candles
            }
            "kraken" => {
                let since_secs = cursor_ms / 1000;
                let resp = client
                    .get("https://api.kraken.com/0/public/OHLC")
                    .query(&[
                        ("pair", symbol.as_str()),
                        ("interval", interval.as_str()),
                        ("since", &since_secs.to_string()),
                    ])
                    .send()
                    .map_err(|e| format!("Kraken request: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("Kraken API error: HTTP {}", resp.status()));
                }
                let body: Value = resp.json().map_err(|e| format!("Kraken parse: {e}"))?;
                if let Some(err) = body["error"].as_array() {
                    if !err.is_empty() {
                        return Err(format!("Kraken error: {:?}", err));
                    }
                }
                let result = body["result"].as_object().ok_or("Kraken: no result")?;
                // The first key that isn't "last" is the pair data
                let pair_data = result
                    .iter()
                    .find(|(k, _)| *k != "last")
                    .map(|(_, v)| v.as_array().cloned().unwrap_or_default())
                    .unwrap_or_default();
                // Kraken's public OHLC serves only the newest 720 bars of an
                // interval, whatever `since` says. A range that ends before
                // the oldest bar it still has is not a paging problem but a
                // limit of the venue — say so instead of "no data".
                let oldest_ms = pair_data
                    .first()
                    .and_then(|k| k.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(value_as_i64)
                    .map(|secs| secs * 1000);
                if let Some(oldest) = oldest_ms {
                    if oldest > end_ms {
                        let oldest_date = chrono::DateTime::from_timestamp_millis(oldest)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        return Err(format!(
                            "Kraken's public API only serves the most recent 720 candles per interval; \
                             for {} bars that history starts at {oldest_date} UTC. Choose a later start date or a larger timeframe.",
                            config.timeframe
                        ));
                    }
                }
                pair_data
                    .into_iter()
                    .filter_map(|k| {
                        let arr = k.as_array()?;
                        let ts_secs = arr[0].as_i64()?;
                        if ts_secs * 1000 > end_ms {
                            return None;
                        }
                        let time = chrono::DateTime::from_timestamp(ts_secs, 0)?.to_rfc3339();
                        Some(serde_json::json!({
                            "time": time,
                            "open": arr[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "high": arr[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "low": arr[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "close": arr[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "volume": arr[6].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                        }))
                    })
                    .collect()
            }
            "kucoin" => {
                // startAt/endAt in seconds; at most 1500 candles per call,
                // newest first: [time, open, close, high, low, volume, turnover].
                let start_s = (cursor_ms / 1000).to_string();
                let end_s = (page_end_ms / 1000).to_string();
                let resp = client
                    .get("https://api.kucoin.com/api/v1/market/candles")
                    .query(&[
                        ("type", interval.as_str()),
                        ("symbol", symbol.as_str()),
                        ("startAt", start_s.as_str()),
                        ("endAt", end_s.as_str()),
                    ])
                    .send()
                    .map_err(|e| format!("KuCoin request: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("KuCoin API error: HTTP {}", resp.status()));
                }
                let body: Value = resp.json().map_err(|e| format!("KuCoin parse: {e}"))?;
                if body
                    .get("code")
                    .and_then(|value| value.as_str())
                    .unwrap_or("200000")
                    != "200000"
                {
                    return Err(format!("KuCoin error: {}", body));
                }
                let mut candles: Vec<Value> = body
                    .get("data")
                    .and_then(|value| value.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|value| value.as_array())
                            .filter_map(|row| candle_from_array(row, 0, 1, 3, 4, 2, 5, true).ok())
                            .map(|c| market_candle_json(&c, ""))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                candles.sort_by(|a, b| a["time"].as_str().cmp(&b["time"].as_str()));
                candles
            }
            "coinbase" => {
                // At most 300 candles per call, newest first:
                // [time, low, high, open, close, volume], numbers not strings.
                let start_iso = chrono::DateTime::from_timestamp_millis(cursor_ms)
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339();
                let end_iso = chrono::DateTime::from_timestamp_millis(page_end_ms)
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339();
                let url = format!("https://api.exchange.coinbase.com/products/{symbol}/candles");
                let resp = client
                    .get(&url)
                    .query(&[
                        ("granularity", interval.as_str()),
                        ("start", start_iso.as_str()),
                        ("end", end_iso.as_str()),
                    ])
                    .send()
                    .map_err(|e| format!("Coinbase request: {e}"))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let detail = resp.text().unwrap_or_default();
                    return Err(format!(
                        "Coinbase API error: HTTP {status} {}",
                        detail.trim()
                    ));
                }
                let body: Vec<Vec<Value>> =
                    resp.json().map_err(|e| format!("Coinbase parse: {e}"))?;
                let mut candles: Vec<Value> = body
                    .iter()
                    .filter_map(|row| candle_from_array(row, 0, 3, 2, 1, 4, 5, true).ok())
                    .map(|c| market_candle_json(&c, ""))
                    .collect();
                candles.sort_by(|a, b| a["time"].as_str().cmp(&b["time"].as_str()));
                candles
            }
            other => {
                return Err(format!(
                    "Exchange '{}' is not supported for historical candles. Supported: binance, bybit, okx, kraken, kucoin, coinbase",
                    other
                ));
            }
        };

        if windowed {
            // A short or empty window is a gap (an outage, a late listing)
            // or the live edge — not the end of the data. Walk the windows
            // up to the requested end; the boundary bar both windows return
            // is folded by the dedup below.
            all_candles.extend(raw);
            if page_end_ms >= end_ms {
                break;
            }
            cursor_ms = page_end_ms;
            continue;
        }

        if raw.is_empty() {
            break;
        }

        // Advance cursor past the last candle we received
        if let Some(last) = raw.last() {
            if let Some(t) = last["time"].as_str() {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
                    cursor_ms = dt.timestamp_millis() + interval_ms;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let fetched = raw.len();
        all_candles.extend(raw);

        // If we got fewer than a full page, we've reached the end
        if (fetched as i64) < max_per_page {
            break;
        }
    }

    if all_candles.is_empty() {
        return Err(format!(
            "No candle data returned from {} for {} ({}). Check the pair and date range.",
            provider, config.pair, config.timeframe
        ));
    }

    // Every provider's page comes out ascending, but the windowed ones repeat
    // the bar on a window boundary — one sort and one dedup make the series
    // strictly increasing whatever the source did.
    all_candles.sort_by(|a, b| a["time"].as_str().cmp(&b["time"].as_str()));
    all_candles.dedup_by(|a, b| a["time"].as_str() == b["time"].as_str());

    Ok(all_candles)
}

// ---------------------------------------------------------------------------
// Exchange Commands
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub(crate) fn list_exchanges(state: State<'_, AppState>) -> Result<Vec<Exchange>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let mut stmt = db
        .prepare("SELECT id, name, exchange_type, provider, is_active, created_at, updated_at, sandbox FROM exchanges ORDER BY name")
        .map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Exchange {
                id: row.get(0)?,
                name: row.get(1)?,
                exchange_type: row.get(2)?,
                provider: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
                sandbox: row.get::<_, i32>(7)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query: {e}"))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("Row: {e}"))?);
    }
    Ok(result)
}

#[tauri::command]
pub(crate) async fn add_exchange(config: ExchangeConfig, state: State<'_, AppState>) -> Result<Exchange, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let sandbox = sandbox_flag(&config)?;

    let sensitive = serde_json::json!({
        "api_key": config.api_key,
        "api_secret": config.api_secret,
        "passphrase": config.passphrase,
        "wallet_address": config.wallet_address,
        "private_key": config.private_key,
        "rpc_endpoint": config.rpc_endpoint,
    });
    let encrypted = encrypt_string(&sensitive.to_string())?;

    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    db.execute(
        "INSERT INTO exchanges (id, name, exchange_type, provider, config_encrypted, is_active, sandbox, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8)",
        params![id, config.name, config.exchange_type, config.provider, encrypted, sandbox as i32, now, now],
    )
    .map_err(|e| format!("Insert: {e}"))?;

    Ok(Exchange {
        id,
        name: config.name,
        exchange_type: config.exchange_type,
        provider: config.provider,
        is_active: true,
        sandbox,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub(crate) async fn update_exchange(
    id: String,
    config: ExchangeConfig,
    state: State<'_, AppState>,
) -> Result<Exchange, String> {
    let now = Utc::now().to_rfc3339();
    let sandbox = sandbox_flag(&config)?;

    let sensitive = serde_json::json!({
        "api_key": config.api_key,
        "api_secret": config.api_secret,
        "passphrase": config.passphrase,
        "wallet_address": config.wallet_address,
        "private_key": config.private_key,
        "rpc_endpoint": config.rpc_endpoint,
    });
    let encrypted = encrypt_string(&sensitive.to_string())?;

    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let affected = db
        .execute(
            "UPDATE exchanges SET name = ?1, exchange_type = ?2, provider = ?3, config_encrypted = ?4, updated_at = ?5, sandbox = ?7 WHERE id = ?6",
            params![config.name, config.exchange_type, config.provider, encrypted, now, id, sandbox as i32],
        )
        .map_err(|e| format!("Update: {e}"))?;

    if affected == 0 {
        return Err("Exchange not found".into());
    }

    let created_at: String = db
        .query_row(
            "SELECT created_at FROM exchanges WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Query: {e}"))?;

    Ok(Exchange {
        id,
        name: config.name,
        exchange_type: config.exchange_type,
        provider: config.provider,
        is_active: true,
        sandbox,
        created_at,
        updated_at: now,
    })
}

/// The sandbox flag of an exchange config — only the venues with a testnet /
/// demo environment may set it (PLAN-QUANTALGO §4.2).
fn sandbox_flag(config: &ExchangeConfig) -> Result<bool, String> {
    let sandbox = config.sandbox.unwrap_or(false);
    if sandbox && !crate::broker::Venue::supports_live(&config.provider) {
        return Err(format!(
            "Sandbox mode is available for Binance (spot testnet) and Bybit (demo trading), not {}.",
            config.provider
        ));
    }
    Ok(sandbox)
}

/// An exchange row's decrypted credentials, for the live broker and the
/// connection probes.
pub(crate) struct StoredCredentials {
    pub provider: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub sandbox: bool,
}

pub(crate) fn load_credentials(conn: &rusqlite::Connection, exchange_id: &str) -> Result<StoredCredentials, String> {
    let (provider, config_encrypted, sandbox): (String, Option<String>, i32) = conn
        .query_row(
            "SELECT provider, config_encrypted, sandbox FROM exchanges WHERE id = ?1",
            params![exchange_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| format!("Exchange not found: {e}"))?;
    let creds: Value = match config_encrypted {
        Some(ref enc) => serde_json::from_str(&decrypt_string(enc)?).map_err(|e| format!("Parse credentials: {e}"))?,
        None => Value::Null,
    };
    let field = |name: &str| -> String { creds.get(name).and_then(|v| v.as_str()).unwrap_or_default().to_string() };
    Ok(StoredCredentials {
        provider,
        api_key: field("api_key"),
        api_secret: field("api_secret"),
        passphrase: field("passphrase"),
        sandbox: sandbox != 0,
    })
}

#[tauri::command(async)]
pub(crate) fn delete_exchange(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let affected = db
        .execute("DELETE FROM exchanges WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete: {e}"))?;
    Ok(affected > 0)
}

#[tauri::command]
pub(crate) async fn test_exchange_connection(
    id: String,
    state: State<'_, AppState>,
) -> Result<ConnectionResult, String> {
    let creds = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_credentials(&db, &id)?
    };
    tauri::async_runtime::spawn_blocking(move || {
        test_provider_credentials(&creds.provider, &creds.api_key, &creds.api_secret, &creds.passphrase, creds.sandbox)
    })
    .await
    .map_err(|e| format!("Connection test task failed: {e}"))?
}

/// The same authenticated probe on credentials that are not stored yet — the
/// exchange form tests before it saves (PLAN-QUANTALGO §5), so a typo in a key
/// is caught while the field is still on screen. Nothing is written.
#[tauri::command]
pub(crate) async fn test_exchange_credentials(
    config: ExchangeConfig,
) -> Result<ConnectionResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sandbox = sandbox_flag(&config)?;
        test_provider_credentials(
            &config.provider,
            config.api_key.as_deref().unwrap_or(""),
            config.api_secret.as_deref().unwrap_or(""),
            config.passphrase.as_deref().unwrap_or(""),
            sandbox,
        )
    })
    .await
    .map_err(|e| format!("Credential test task failed: {e}"))?
}

/// One signed, read-only request against the provider's account endpoint.
/// Blocking HTTP on purpose: the callers are async commands off the main
/// thread, and the exchange adapters below are all `reqwest::blocking`.
pub(crate) fn test_provider_credentials(
    provider: &str,
    api_key: &str,
    api_secret: &str,
    passphrase: &str,
    sandbox: bool,
) -> Result<ConnectionResult, String> {
    if api_key.is_empty() || api_secret.is_empty() {
        return Ok(ConnectionResult {
            success: false,
            message: "API key or secret is empty".into(),
            latency_ms: None,
        });
    }

    let client = http_client(15)?;

    let start = Instant::now();

    match provider.to_lowercase().as_str() {
        "binance" => {
            // GET /api/v3/account — requires valid API key + HMAC-SHA256 signature
            let ts = get_server_time_ms(&client, "binance");
            let query = format!("timestamp={}", ts);
            let signature = hmac_sha256_hex(api_secret.as_bytes(), query.as_bytes());
            let base = crate::broker::Venue::Binance.private_base(sandbox);
            let url = format!("{base}/api/v3/account?{query}&signature={signature}");

            match client.get(&url).header("X-MBX-APIKEY", api_key).send() {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let status = resp.status();
                    if status.is_success() {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with Binance{} ({}ms)", if sandbox { " spot testnet" } else { "" }, latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let body = resp.text().unwrap_or_default();
                        let msg = serde_json::from_str::<Value>(&body)
                            .ok()
                            .and_then(|v| v.get("msg").and_then(|m| m.as_str()).map(String::from))
                            .unwrap_or_else(|| format!("HTTP {}", status));
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("Binance: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        "bybit" => {
            // GET /v5/account/wallet-balance — requires API key + HMAC-SHA256
            let ts = get_server_time_ms(&client, "bybit").to_string();
            let recv_window = "20000";
            let query_string = "accountType=UNIFIED";
            let sign_payload = format!("{}{}{}{}", ts, api_key, recv_window, query_string);
            let signature = hmac_sha256_hex(api_secret.as_bytes(), sign_payload.as_bytes());
            let base = crate::broker::Venue::Bybit.private_base(sandbox);
            let url = format!("{base}/v5/account/wallet-balance?{query_string}");

            match client.get(&url)
                .header("X-BAPI-API-KEY", api_key)
                .header("X-BAPI-TIMESTAMP", &ts)
                .header("X-BAPI-SIGN", &signature)
                .header("X-BAPI-RECV-WINDOW", recv_window)
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let body = resp.text().unwrap_or_default();
                    let json: Value = serde_json::from_str(&body).unwrap_or_default();
                    let ret_code = json.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
                    if ret_code == 0 {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with Bybit{} ({}ms)", if sandbox { " demo" } else { "" }, latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let msg = json.get("retMsg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("Bybit: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        "okx" => {
            // GET /api/v5/account/balance — requires API key + HMAC-SHA256 (base64) + passphrase
            if passphrase.is_empty() {
                return Ok(ConnectionResult {
                    success: false,
                    message: "OKX requires a passphrase".into(),
                    latency_ms: None,
                });
            }
            let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            let method = "GET";
            let path = "/api/v5/account/balance";
            let sign_payload = format!("{}{}{}", ts, method, path);
            let signature = hmac_sha256_base64(api_secret.as_bytes(), sign_payload.as_bytes());
            let url = format!("https://www.okx.com{}", path);

            match client.get(&url)
                .header("OK-ACCESS-KEY", api_key)
                .header("OK-ACCESS-SIGN", &signature)
                .header("OK-ACCESS-TIMESTAMP", &ts)
                .header("OK-ACCESS-PASSPHRASE", passphrase)
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let body = resp.text().unwrap_or_default();
                    let json: Value = serde_json::from_str(&body).unwrap_or_default();
                    let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("-1");
                    if code == "0" {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with OKX ({}ms)", latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let msg = json.get("msg").and_then(|v| v.as_str()).unwrap_or("Authentication failed");
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("OKX: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        "coinbase" => {
            // GET /api/v3/brokerage/accounts — Coinbase Advanced Trade API
            let ts = chrono::Utc::now().timestamp().to_string();
            let method = "GET";
            let path = "/api/v3/brokerage/accounts";
            let sign_payload = format!("{}{}{}", ts, method, path);
            let signature = hmac_sha256_hex(api_secret.as_bytes(), sign_payload.as_bytes());
            let url = format!("https://api.coinbase.com{}", path);

            match client.get(&url)
                .header("CB-ACCESS-KEY", api_key)
                .header("CB-ACCESS-SIGN", &signature)
                .header("CB-ACCESS-TIMESTAMP", &ts)
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with Coinbase ({}ms)", latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let body = resp.text().unwrap_or_default();
                        let json: Value = serde_json::from_str(&body).unwrap_or_default();
                        let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("Authentication failed");
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("Coinbase: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        "kraken" => {
            // POST /0/private/Balance — Kraken uses a different signing scheme (nonce-based, local time is fine)
            let ts = chrono::Utc::now().timestamp_millis().to_string();
            let post_data = format!("nonce={}", ts);
            let path = "/0/private/Balance";

            // Kraken signature: HMAC-SHA512(path + SHA256(nonce + post_data), base64_decode(secret))
            let mut sha = sha2::Sha256::new();
            sha.update(ts.as_bytes());
            sha.update(post_data.as_bytes());
            let sha_hash = sha.finalize();

            let secret_bytes = base64::engine::general_purpose::STANDARD.decode(api_secret)
                .unwrap_or_default();
            let mut path_hash = path.as_bytes().to_vec();
            path_hash.extend_from_slice(&sha_hash);

            type HmacSha512 = hmac::Hmac<sha2::Sha512>;
            let mut mac = <HmacSha512 as Mac>::new_from_slice(&secret_bytes).unwrap_or_else(|_|
                <HmacSha512 as Mac>::new_from_slice(b"invalidkeypadded_to_min_len_xx").unwrap()
            );
            mac.update(&path_hash);
            let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

            let url = format!("https://api.kraken.com{}", path);

            match client.post(&url)
                .header("API-Key", api_key)
                .header("API-Sign", &signature)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(post_data)
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let body = resp.text().unwrap_or_default();
                    let json: Value = serde_json::from_str(&body).unwrap_or_default();
                    let errors = json.get("error").and_then(|v| v.as_array());
                    if errors.map(|e| e.is_empty()).unwrap_or(false) {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with Kraken ({}ms)", latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let msg = errors
                            .and_then(|e| e.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("Authentication failed");
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("Kraken: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        "kucoin" => {
            // GET /api/v1/accounts — requires API key + HMAC-SHA256 (base64) + passphrase
            if passphrase.is_empty() {
                return Ok(ConnectionResult {
                    success: false,
                    message: "KuCoin requires a passphrase".into(),
                    latency_ms: None,
                });
            }
            let ts = get_server_time_ms(&client, "kucoin").to_string();
            let method = "GET";
            let path = "/api/v1/accounts";
            let sign_payload = format!("{}{}{}", ts, method, path);
            let signature = hmac_sha256_base64(api_secret.as_bytes(), sign_payload.as_bytes());
            let passphrase_sign = hmac_sha256_base64(api_secret.as_bytes(), passphrase.as_bytes());
            let url = format!("https://api.kucoin.com{}", path);

            match client.get(&url)
                .header("KC-API-KEY", api_key)
                .header("KC-API-SIGN", &signature)
                .header("KC-API-TIMESTAMP", &ts)
                .header("KC-API-PASSPHRASE", &passphrase_sign)
                .header("KC-API-KEY-VERSION", "2")
                .send()
            {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    let body = resp.text().unwrap_or_default();
                    let json: Value = serde_json::from_str(&body).unwrap_or_default();
                    let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("-1");
                    if code == "200000" {
                        Ok(ConnectionResult {
                            success: true,
                            message: format!("Authenticated with KuCoin ({}ms)", latency),
                            latency_ms: Some(latency),
                        })
                    } else {
                        let msg = json.get("msg").and_then(|v| v.as_str()).unwrap_or("Authentication failed");
                        Ok(ConnectionResult {
                            success: false,
                            message: format!("KuCoin: {}", msg),
                            latency_ms: Some(latency),
                        })
                    }
                }
                Err(e) => Ok(ConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    latency_ms: None,
                }),
            }
        }
        other => {
            Ok(ConnectionResult {
                success: false,
                message: format!(
                    "{other} credential validation is not supported. Stored credentials cannot be used for deploy."
                ),
                latency_ms: Some(start.elapsed().as_millis() as u64),
            })
        }
    }
}

#[tauri::command]
pub(crate) async fn get_balances(exchange_id: String, state: State<'_, AppState>) -> Result<Vec<Balance>, String> {
    let creds = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_credentials(&db, &exchange_id)?
    };
    tauri::async_runtime::spawn_blocking(move || fetch_balances(&creds))
        .await
        .map_err(|e| format!("Balance task failed: {e}"))?
}

/// The signed balance read of one provider — blocking HTTP, so it runs on
/// the blocking pool (see `get_exchange_pairs`).
fn fetch_balances(creds: &StoredCredentials) -> Result<Vec<Balance>, String> {
    let provider = creds.provider.clone();
    let sandbox = creds.sandbox;
    let api_key = creds.api_key.as_str();
    let api_secret = creds.api_secret.as_str();

    if api_key.is_empty() || api_secret.is_empty() {
        return Err("API key or secret is empty".into());
    }

    let client = http_client(15)?;

    match provider.to_lowercase().as_str() {
        "binance" => {
            let ts = get_server_time_ms(&client, "binance");
            let query = format!("timestamp={}", ts);
            let signature = hmac_sha256_hex(api_secret.as_bytes(), query.as_bytes());
            let base = crate::broker::Venue::Binance.private_base(sandbox);
            let url = format!("{base}/api/v3/account?{query}&signature={signature}");

            let resp = client
                .get(&url)
                .header("X-MBX-APIKEY", api_key)
                .send()
                .map_err(|e| format!("Request failed: {e}"))?;

            if !resp.status().is_success() {
                let body = resp.text().unwrap_or_default();
                let msg = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|v| v.get("msg").and_then(|m| m.as_str()).map(String::from))
                    .unwrap_or_else(|| "Failed to fetch balances".into());
                return Err(format!("Binance: {}", msg));
            }

            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            let balances = json
                .get("balances")
                .and_then(|b| b.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| {
                            let asset = b.get("asset").and_then(|v| v.as_str())?;
                            let free: f64 = b.get("free").and_then(|v| v.as_str())?.parse().ok()?;
                            let locked: f64 =
                                b.get("locked").and_then(|v| v.as_str())?.parse().ok()?;
                            let total = free + locked;
                            if total < 0.000001 {
                                return None;
                            }
                            Some(Balance {
                                asset: asset.to_string(),
                                total,
                                available: free,
                                in_positions: locked,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(balances)
        }
        "bybit" => {
            let ts = get_server_time_ms(&client, "bybit").to_string();
            let recv_window = "20000";
            let query_string = "accountType=UNIFIED";
            let sign_payload = format!("{}{}{}{}", ts, api_key, recv_window, query_string);
            let signature = hmac_sha256_hex(api_secret.as_bytes(), sign_payload.as_bytes());
            let base = crate::broker::Venue::Bybit.private_base(sandbox);
            let url = format!("{base}/v5/account/wallet-balance?{query_string}");

            let resp = client
                .get(&url)
                .header("X-BAPI-API-KEY", api_key)
                .header("X-BAPI-TIMESTAMP", &ts)
                .header("X-BAPI-SIGN", &signature)
                .header("X-BAPI-RECV-WINDOW", recv_window)
                .send()
                .map_err(|e| format!("Request failed: {e}"))?;

            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            let ret_code = json.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
            if ret_code != 0 {
                let msg = json
                    .get("retMsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Failed");
                return Err(format!("Bybit: {}", msg));
            }

            let mut balances = Vec::new();
            if let Some(accounts) = json.pointer("/result/list").and_then(|l| l.as_array()) {
                for account in accounts {
                    if let Some(coins) = account.get("coin").and_then(|c| c.as_array()) {
                        for coin in coins {
                            let asset = coin.get("coin").and_then(|v| v.as_str()).unwrap_or("");
                            let equity: f64 = coin
                                .get("equity")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            let available: f64 = coin
                                .get("availableToWithdraw")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            if equity < 0.000001 {
                                continue;
                            }
                            balances.push(Balance {
                                asset: asset.to_string(),
                                total: equity,
                                available,
                                in_positions: (equity - available).max(0.0),
                            });
                        }
                    }
                }
            }
            Ok(balances)
        }
        _ => Err(format!(
            "Balance fetching not yet supported for {}",
            provider
        )),
    }
}

pub(crate) fn fetch_exchange_pairs_for_provider(provider: &str) -> Result<Vec<String>, String> {
    let provider_key = provider.to_lowercase();
    if let Ok(cache) = pair_cache().lock() {
        if let Some(cached) = cache.get(&provider_key) {
            if (Utc::now() - cached.fetched_at).num_seconds() < PAIR_CACHE_TTL_SECS {
                return Ok(cached.pairs.clone());
            }
        }
    }

    let client = http_client(15)?;

    let pairs = match provider_key.as_str() {
        "binance" => {
            let resp = client
                .get("https://api.binance.com/api/v3/exchangeInfo")
                .send()
                .map_err(|e| format!("Binance request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            json.get("symbols")
                .and_then(|s| s.as_array())
                .map(|syms| {
                    syms.iter()
                        .filter_map(|s| {
                            let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("");
                            if status != "TRADING" {
                                return None;
                            }
                            let base = s.get("baseAsset").and_then(|v| v.as_str())?;
                            let quote = s.get("quoteAsset").and_then(|v| v.as_str())?;
                            Some(format!("{}/{}", base, quote))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "bybit" => {
            let resp = client
                .get("https://api.bybit.com/v5/market/instruments-info?category=spot&limit=500")
                .send()
                .map_err(|e| format!("Bybit request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            json.pointer("/result/list")
                .and_then(|l| l.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("");
                            if status != "Trading" {
                                return None;
                            }
                            let base = item.get("baseCoin").and_then(|v| v.as_str())?;
                            let quote = item.get("quoteCoin").and_then(|v| v.as_str())?;
                            Some(format!("{}/{}", base, quote))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "okx" => {
            let resp = client
                .get("https://www.okx.com/api/v5/public/instruments?instType=SPOT")
                .send()
                .map_err(|e| format!("OKX request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            json.get("data")
                .and_then(|d| d.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("");
                            if state != "live" {
                                return None;
                            }
                            let base = item.get("baseCcy").and_then(|v| v.as_str())?;
                            let quote = item.get("quoteCcy").and_then(|v| v.as_str())?;
                            Some(format!("{}/{}", base, quote))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "kraken" => {
            let resp = client
                .get("https://api.kraken.com/0/public/AssetPairs")
                .send()
                .map_err(|e| format!("Kraken request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            // `wsname` is the pair in Kraken's display spelling ("XBT/USD",
            // "XTZ/EUR"); the `base`/`quote` fields carry the legacy X/Z
            // prefixes ("XXBT", "ZUSD") that a naive strip turned "XTZ" into
            // "TZ". Kraken's own asset codes for Bitcoin and Dogecoin become
            // the spelling the rest of the module uses (`pair_for_exchange`
            // maps them back on the way out).
            json.get("result")
                .and_then(|r| r.as_object())
                .map(|pairs_map| {
                    pairs_map
                        .values()
                        .filter_map(|pair| {
                            let wsname = pair.get("wsname").and_then(|v| v.as_str())?;
                            let (base, quote) = wsname.split_once('/')?;
                            let rename = |asset: &str| match asset {
                                "XBT" => "BTC".to_string(),
                                "XDG" => "DOGE".to_string(),
                                other => other.to_string(),
                            };
                            Some(format!("{}/{}", rename(base), rename(quote)))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "kucoin" => {
            let resp = client
                .get("https://api.kucoin.com/api/v1/symbols")
                .send()
                .map_err(|e| format!("KuCoin request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            json.get("data")
                .and_then(|d| d.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let enabled = item
                                .get("enableTrading")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if !enabled {
                                return None;
                            }
                            let base = item.get("baseCurrency").and_then(|v| v.as_str())?;
                            let quote = item.get("quoteCurrency").and_then(|v| v.as_str())?;
                            Some(format!("{}/{}", base, quote))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        "coinbase" => {
            let resp = client
                .get("https://api.exchange.coinbase.com/products")
                .send()
                .map_err(|e| format!("Coinbase request failed: {e}"))?;
            let json: Value = resp.json().map_err(|e| format!("Parse: {e}"))?;
            json.as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("");
                            if status != "online" {
                                return None;
                            }
                            let base = item.get("base_currency").and_then(|v| v.as_str())?;
                            let quote = item.get("quote_currency").and_then(|v| v.as_str())?;
                            Some(format!("{}/{}", base, quote))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        other => {
            return Err(format!(
                "Public pair discovery is not supported for {other}; configure a supported CEX provider before deploy."
            ));
        }
    };

    if pairs.is_empty() {
        return Err(format!(
            "No active spot pairs returned by {}. Check provider availability before deploy.",
            provider
        ));
    }

    let mut sorted = pairs;
    sorted.sort_by(|a, b| {
        let a_major = a.starts_with("BTC/") || a.starts_with("ETH/") || a.starts_with("SOL/");
        let b_major = b.starts_with("BTC/") || b.starts_with("ETH/") || b.starts_with("SOL/");
        b_major.cmp(&a_major).then(a.cmp(b))
    });
    sorted.dedup();
    if let Ok(mut cache) = pair_cache().lock() {
        cache.insert(
            provider_key,
            CachedPairs {
                fetched_at: Utc::now(),
                pairs: sorted.clone(),
            },
        );
    }
    Ok(sorted)
}

#[tauri::command]
pub(crate) async fn get_exchange_pairs(
    exchange_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let provider: String = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        db.query_row(
            "SELECT provider FROM exchanges WHERE id = ?1",
            params![exchange_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Exchange not found: {e}"))?
    };
    // Blocking HTTP runs on the blocking pool, never on a runtime worker: in
    // debug builds reqwest's blocking client panics there (tokio refuses to
    // drop a runtime inside an async context) and the command never answers.
    tauri::async_runtime::spawn_blocking(move || fetch_exchange_pairs_for_provider(&provider))
        .await
        .map_err(|e| format!("Pair task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The blocking reqwest client on a Tokio worker thread panics in debug
    /// builds (tokio refuses to drop a runtime inside an async context) —
    /// what stalled the pair dropdown on 2026-09-07. On the blocking pool the
    /// same call simply fails like any other request; the commands therefore
    /// hand every blocking HTTP call to `spawn_blocking`.
    #[test]
    fn blocking_http_belongs_on_the_blocking_pool() {
        fn probe() -> Result<(), String> {
            let client = http_client(1)?;
            client.get("http://127.0.0.1:9/").send().map(|_| ()).map_err(|e| e.to_string())
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let on_pool = rt.block_on(async { tokio::task::spawn_blocking(probe).await });
        assert!(matches!(on_pool, Ok(Err(_))), "on the blocking pool the request fails normally: {on_pool:?}");
        if cfg!(debug_assertions) {
            let on_worker = rt.block_on(async { tokio::spawn(async { probe() }).await });
            assert!(on_worker.is_err(), "a runtime worker must panic in debug builds, not answer");
        }
    }

    fn history(provider: &str, pair: &str, timeframe: &str, start: &str, end: &str) -> BacktestConfig {
        BacktestConfig {
            strategy_id: "test".into(),
            exchange: provider.into(),
            exchange_id: None,
            pair: pair.into(),
            timeframe: timeframe.into(),
            start_date: start.into(),
            end_date: end.into(),
            initial_capital: 10_000.0,
            commission: 0.1,
            strategy_params: None,
        }
    }

    /// Every provider's history has to come out as one strictly increasing
    /// series of sane bars — the backtest engine indexes the equity curve by
    /// candle position and the strategies assume chronological order.
    fn assert_series(candles: &[Value], at_least: usize, timeframe_secs: i64) {
        assert!(
            candles.len() >= at_least,
            "expected at least {at_least} candles, got {}",
            candles.len()
        );
        let mut prev: Option<i64> = None;
        for c in candles {
            let t = chrono::DateTime::parse_from_rfc3339(c["time"].as_str().expect("time"))
                .expect("rfc3339 time")
                .timestamp();
            if let Some(p) = prev {
                assert!(t > p, "candles must be strictly increasing ({p} then {t})");
                assert_eq!((t - p) % timeframe_secs, 0, "bars must sit on the timeframe grid");
            }
            prev = Some(t);
            let (o, h, l, cl) = (
                c["open"].as_f64().unwrap(),
                c["high"].as_f64().unwrap(),
                c["low"].as_f64().unwrap(),
                c["close"].as_f64().unwrap(),
            );
            assert!(h >= l && o > 0.0 && cl > 0.0, "bad bar {c}");
            assert!(c["volume"].as_f64().unwrap() >= 0.0);
        }
    }

    #[test]
    #[ignore = "hits the live KuCoin API"]
    fn live_historical_candles_kucoin() {
        // Four days of hourly bars: 96 bars, one window.
        let candles =
            fetch_historical_candles(&history("kucoin", "BTC/USDT", "1h", "2026-08-01", "2026-08-04"))
                .expect("kucoin history");
        assert_series(&candles, 90, 3600);
    }

    #[test]
    #[ignore = "hits the live Coinbase API"]
    fn live_historical_candles_coinbase() {
        // Twenty days of hourly bars: 480 bars over two 300-bar windows —
        // proves the windowed paging and the boundary dedup.
        let candles =
            fetch_historical_candles(&history("coinbase", "BTC/USD", "1h", "2026-08-01", "2026-08-20"))
                .expect("coinbase history");
        assert_series(&candles, 470, 3600);
    }

    #[test]
    #[ignore = "hits the live Binance API"]
    fn live_historical_candles_binance() {
        let candles =
            fetch_historical_candles(&history("binance", "BTC/USDT", "1d", "2026-06-01", "2026-07-31"))
                .expect("binance history");
        assert_series(&candles, 60, 86_400);
    }

    #[test]
    #[ignore = "hits the live Bybit API"]
    fn live_historical_candles_bybit() {
        // 240 hourly bars over the 200-bar page size — proves the cursor paging.
        let candles =
            fetch_historical_candles(&history("bybit", "BTC/USDT", "1h", "2026-08-01", "2026-08-10"))
                .expect("bybit history");
        assert_series(&candles, 230, 3600);
    }

    #[test]
    #[ignore = "hits the live OKX and Kraken APIs"]
    fn live_historical_candles_okx_and_kraken() {
        // 120 hourly bars over OKX's 100-bar page — proves the `after` paging.
        let okx =
            fetch_historical_candles(&history("okx", "BTC/USDT", "1h", "2026-08-01", "2026-08-06"))
                .expect("okx history");
        assert_series(&okx, 110, 3600);

        // Kraken serves only the newest 720 bars per interval, so the window
        // has to sit inside the last 30 days of hourly bars.
        let end = Utc::now() - chrono::Duration::days(1);
        let start = end - chrono::Duration::days(5);
        let kraken = fetch_historical_candles(&history(
            "kraken",
            "BTC/USD",
            "1h",
            &start.format("%Y-%m-%d").to_string(),
            &end.format("%Y-%m-%d").to_string(),
        ))
        .expect("kraken history");
        assert_series(&kraken, 90, 3600);

        // …and a range older than that is reported as the venue's limit.
        let err =
            fetch_historical_candles(&history("kraken", "BTC/USD", "1h", "2026-01-01", "2026-01-06"))
                .expect_err("kraken must refuse a range older than its 720-bar window");
        assert!(err.contains("720 candles"), "unexpected error: {err}");
    }

    /// The pair dropdowns are fed from here for every connected exchange;
    /// each provider has to list the majors under the module's `BASE/QUOTE`
    /// spelling, or the dropdown would show nothing a strategy can trade.
    #[test]
    #[ignore = "hits every provider's public market API"]
    fn live_pairs_every_provider() {
        for provider in ["binance", "bybit", "okx", "kraken", "kucoin", "coinbase"] {
            let pairs = fetch_exchange_pairs_for_provider(provider)
                .unwrap_or_else(|e| panic!("{provider}: {e}"));
            assert!(
                pairs.iter().any(|p| p == "BTC/USDT" || p == "BTC/USD"),
                "{provider} lists {} pairs but no BTC major: {:?}",
                pairs.len(),
                pairs.iter().take(5).collect::<Vec<_>>()
            );
            assert!(pairs.windows(2).all(|w| w[0] != w[1]), "{provider}: duplicate pairs");
        }
    }
}
