//! Historical QuantMetrics feeds. Only upstream observations cross this boundary;
//! absent or malformed values are omitted, never replaced by zero or sample data.
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[path = "metrics_archive.rs"]
mod archive;
#[path = "metrics_utxo.rs"]
mod utxo;

const NETWORK_FIELDS: &str = "PriceUSD,CapMVRVCur,CapMrktCurUSD,AdrActCnt,TxCnt,HashRate,SplyCur,FeeTotNtv,IssTotUSD,FlowInExNtv,FlowOutExNtv,SplyExNtv";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub rows: Vec<Row>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub time: i64,
    pub values: BTreeMap<String, f64>,
    pub provisional: Vec<String>,
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite())
}

async fn json(url: &str, params: &[(&str, String)]) -> Result<Value, String> {
    let response = super::market::client()?
        .get(url)
        .query(params)
        .send()
        .await
        .map_err(|e| format!("Feed request failed: {e}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid feed response: {e}"))?;
    if !status.is_success() {
        let message = body
            .pointer("/error/message")
            .or_else(|| body.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("Provider unavailable");
        return Err(format!("{status}: {message}"));
    }
    Ok(body)
}

fn network_row(row: &Value) -> Option<Row> {
    let time = chrono::DateTime::parse_from_rfc3339(row.get("time")?.as_str()?)
        .ok()?
        .timestamp();
    let mut values = BTreeMap::new();
    let mut provisional = Vec::new();
    for field in NETWORK_FIELDS.split(',') {
        if let Some(value) = row.get(field).and_then(number) {
            values.insert(field.to_string(), value);
            if row
                .get(format!("{field}-status"))
                .and_then(Value::as_str)
                .is_some_and(|s| s != "final")
            {
                provisional.push(field.to_string());
            }
        }
    }
    Some(Row {
        time,
        values,
        provisional,
    })
}

async fn network() -> Result<Vec<Row>, String> {
    let body = json(
        "https://community-api.coinmetrics.io/v4/timeseries/asset-metrics",
        &[
            ("assets", "btc".into()),
            ("metrics", NETWORK_FIELDS.into()),
            ("frequency", "1d".into()),
            ("start_time", "2009-01-03".into()),
            ("end_time", Utc::now().format("%Y-%m-%d").to_string()),
            ("page_size", "10000".into()),
            ("paging_from", "start".into()),
        ],
    )
    .await?;
    if body
        .get("next_page_token")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err("Network history exceeded the requested page; refusing truncated data".into());
    }
    let rows = body
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Network feed has no data array")?;
    Ok(rows.iter().filter_map(network_row).collect())
}

async fn price(asset: &str) -> Result<Vec<Row>, String> {
    let symbol = format!("{asset}USDT");
    candles("https://api.binance.com/api/v3/klines", &symbol, false).await
}

async fn candles(url: &str, symbol: &str, futures: bool) -> Result<Vec<Row>, String> {
    let now = Utc::now().timestamp_millis();
    let mut start: i64 = 0;
    let mut result = Vec::new();
    for _ in 0..20 {
        let body = json(
            url,
            &[
                ("symbol", symbol.into()),
                ("interval", "1d".into()),
                ("limit", "1000".into()),
                ("startTime", start.to_string()),
                ("endTime", now.to_string()),
            ],
        )
        .await?;
        let rows = body.as_array().ok_or("Price feed has no candle array")?;
        for candle in rows {
            if let Some(row) = if futures {
                futures_row(candle, now)
            } else {
                candle_row(candle, now)
            } {
                result.push(row);
            }
        }
        if rows.len() < 1000 {
            return Ok(result);
        }
        let next = rows
            .last()
            .and_then(|r| r.get(0))
            .and_then(Value::as_i64)
            .ok_or("Invalid price pagination")?
            + 86_400_000;
        if next <= start {
            return Err("Price history did not advance".into());
        }
        start = next;
    }
    Err("Candle history exceeded pagination limit; refusing truncated data".into())
}

fn futures_row(candle: &Value, now: i64) -> Option<Row> {
    let mut row = candle_row(candle, now)?;
    for (index, field) in [(7, "quoteVolume"), (8, "trades"), (10, "takerBuyQuote")] {
        let value = number(candle.get(index)?)?;
        if value < 0.0 {
            return None;
        }
        row.values.insert(field.into(), value);
    }
    if row.values["takerBuyQuote"] > row.values["quoteVolume"] {
        return None;
    }
    row.values.insert(
        "takerSellQuote".into(),
        row.values["quoteVolume"] - row.values["takerBuyQuote"],
    );
    Some(row)
}

fn candle_row(candle: &Value, now: i64) -> Option<Row> {
    let time = candle.get(0)?.as_i64()? / 1000;
    // Daily comparisons only use completed candles.
    if candle.get(6)?.as_i64()? >= now {
        return None;
    }
    let mut values = BTreeMap::new();
    for (index, key) in [(1, "open"), (2, "high"), (3, "low"), (4, "close")] {
        let value = number(candle.get(index)?)?;
        if value <= 0.0 {
            return None;
        }
        values.insert(key.into(), value);
    }
    if values["high"] < values["open"].max(values["close"])
        || values["low"] > values["open"].min(values["close"])
    {
        return None;
    }
    Some(Row {
        time,
        values,
        provisional: vec![],
    })
}

async fn derivatives(source: &str) -> Result<Vec<Row>, String> {
    let (path, field, time_field, params) = match source {
        "funding" => (
            "fapi/v1/fundingRate",
            "fundingRate",
            "fundingTime",
            vec![("limit", "1000".into())],
        ),
        "open-interest" => (
            "futures/data/openInterestHist",
            "sumOpenInterestValue",
            "timestamp",
            vec![("period", "1d".into()), ("limit", "30".into())],
        ),
        "long-short" => (
            "futures/data/globalLongShortAccountRatio",
            "longShortRatio",
            "timestamp",
            vec![("period", "1d".into()), ("limit", "30".into())],
        ),
        "top-accounts" => (
            "futures/data/topLongShortAccountRatio",
            "longShortRatio",
            "timestamp",
            vec![("period", "1d".into()), ("limit", "30".into())],
        ),
        "top-positions" => (
            "futures/data/topLongShortPositionRatio",
            "longShortRatio",
            "timestamp",
            vec![("period", "1d".into()), ("limit", "30".into())],
        ),
        _ => return Err("Unsupported derivatives source".into()),
    };
    let mut params = params;
    params.push(("symbol", "BTCUSDT".into()));
    let body = json(&format!("https://fapi.binance.com/{path}"), &params).await?;
    let rows = body
        .as_array()
        .ok_or("Derivatives feed has no data array")?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let time = r.get(time_field)?.as_i64()? / 1000;
            let value = number(r.get(field)?)?;
            Some(Row {
                time,
                values: [(field.to_string(), value)].into(),
                provisional: vec![],
            })
        })
        .collect())
}

async fn funding() -> Result<Vec<Row>, String> {
    let now = Utc::now().timestamp_millis();
    // Binance treats zero as an omitted startTime (recent records only).
    let mut start: i64 = 1;
    let mut result = Vec::new();
    for _ in 0..40 {
        let body = json(
            "https://fapi.binance.com/fapi/v1/fundingRate",
            &[
                ("symbol", "BTCUSDT".into()),
                ("limit", "1000".into()),
                ("startTime", start.to_string()),
                ("endTime", now.to_string()),
            ],
        )
        .await?;
        let rows = body.as_array().ok_or("Funding feed has no data array")?;
        for point in rows {
            if let (Some(time), Some(value)) =
                (point["fundingTime"].as_i64(), number(&point["fundingRate"]))
            {
                result.push(Row {
                    time: time / 1000,
                    values: [("fundingRate".into(), value)].into(),
                    provisional: vec![],
                });
            }
        }
        if rows.len() < 1000 {
            return Ok(result);
        }
        let next = rows
            .last()
            .and_then(|r| r["fundingTime"].as_i64())
            .ok_or("Invalid funding pagination")?
            + 1;
        if next <= start {
            return Err("Funding history did not advance".into());
        }
        start = next;
    }
    Err("Funding history exceeded pagination limit; refusing truncated data".into())
}

fn daily_point(time: i64, value: &Value, field: &str) -> Option<Row> {
    let value = number(value)?;
    if time <= 0
        || time % 86400 != 0
        || time >= Utc::now().timestamp().div_euclid(86400) * 86400
        || value < 0.0
    {
        return None;
    }
    Some(Row {
        time,
        values: [(field.into(), value)].into(),
        provisional: vec![],
    })
}

async fn chain(source: &str) -> Result<Vec<Row>, String> {
    let (chart, field) = match source {
        "chain-volume" => ("estimated-transaction-volume-usd", "transferUSD"),
        "chain-difficulty" => ("difficulty", "difficulty"),
        "chain-transactions" => ("n-transactions", "transactions"),
        _ => return Err("Unsupported chain source".into()),
    };
    let body = json(
        &format!("https://api.blockchain.info/charts/{chart}"),
        &[
            ("timespan", "all".into()),
            ("sampled", "false".into()),
            ("format", "json".into()),
        ],
    )
    .await?;
    if body["period"].as_str() != Some("day") {
        return Err("Expected daily chain observations".into());
    }
    let rows = body["values"]
        .as_array()
        .ok_or("Chain feed has no values")?;
    Ok(rows
        .iter()
        .filter_map(|r| daily_point(r["x"].as_i64()?, &r["y"], field))
        .collect())
}

async fn defi(source: &str) -> Result<Vec<Row>, String> {
    let (path, data_type) = match source {
        "defi-tvl" => ("v2/historicalChainTvl", None),
        "defi-dex" => ("overview/dexs", Some("dailyVolume")),
        "defi-fees" => ("overview/fees", Some("dailyFees")),
        "defi-revenue" => ("overview/fees", Some("dailyRevenue")),
        _ => return Err("Unsupported DeFi source".into()),
    };
    let params = if let Some(kind) = data_type {
        vec![
            ("excludeTotalDataChart", "false".into()),
            ("excludeTotalDataChartBreakdown", "true".into()),
            ("dataType", kind.into()),
        ]
    } else {
        vec![]
    };
    let body = json(&format!("https://api.llama.fi/{path}"), &params).await?;
    let rows = if source == "defi-tvl" {
        body.as_array()
    } else {
        body["totalDataChart"].as_array()
    }
    .ok_or("DeFi feed has no daily history")?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            if source == "defi-tvl" {
                daily_point(r["date"].as_i64()?, &r["tvl"], "value")
            } else {
                daily_point(r[0].as_i64()?, &r[1], "value")
            }
        })
        .collect())
}

pub async fn history(source: &str, asset: &str) -> Result<History, String> {
    if !["BTC", "ETH", "SOL", "BNB", "XRP", "AVAX"].contains(&asset) {
        return Err("Unsupported comparison asset".into());
    }
    let mut rows = match source {
        "network" => network().await?,
        "price" => price(asset).await?,
        "funding" => funding().await?,
        "positioning" => archive::history().await?,
        "utxo-profit" | "utxo-supply" | "utxo-value" | "utxo-activity" => {
            utxo::history(source).await?
        }
        "open-interest" | "long-short" | "top-accounts" | "top-positions" => {
            derivatives(source).await?
        }
        "futures" => candles("https://fapi.binance.com/fapi/v1/klines", "BTCUSDT", true).await?,
        "chain-volume" | "chain-difficulty" | "chain-transactions" => chain(source).await?,
        "defi-tvl" | "defi-dex" | "defi-fees" | "defi-revenue" => defi(source).await?,
        "sentiment" => super::market::sentiment(0)
            .await?
            .into_iter()
            .map(|p| Row {
                time: p.time,
                values: [("value".into(), f64::from(p.value))].into(),
                provisional: vec![],
            })
            .collect(),
        _ => return Err("Unsupported metrics source".into()),
    };
    rows.retain(|r| r.time > 0 && !r.values.is_empty());
    rows.sort_by_key(|r| r.time);
    rows.dedup_by_key(|r| r.time);
    if rows.is_empty() {
        return Err("Provider returned no usable observations".into());
    }
    Ok(History {
        rows,
        fetched_at: Utc::now().timestamp(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "downloads and caches the public archive and BRK history"]
    async fn live_extended_histories() {
        for source in [
            "utxo-profit",
            "utxo-supply",
            "utxo-value",
            "utxo-activity",
            "positioning",
        ] {
            let data = history(source, "BTC").await.expect(source);
            assert!(
                data.rows.len() > 1800,
                "{source} must have multi-year history"
            );
            assert!(data.rows.windows(2).all(|w| w[0].time < w[1].time));
            println!(
                "{source}: {} days, {} to {}",
                data.rows.len(),
                data.rows[0].time,
                data.rows.last().unwrap().time
            );
            if let Ok(dir) = std::env::var("QUANTMETRICS_CAPTURE_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{source}-BTC.json")),
                    serde_json::to_vec(&data).unwrap(),
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn missing_values_are_not_zero_and_quality_is_preserved() {
        let row = network_row(
            &json!({"time":"2026-09-13T00:00:00Z", "CapMVRVCur":"1.4", "PriceUSD":null,
            "FlowInExNtv":"0", "FlowInExNtv-status":"flash", "HashRate":"NaN"}),
        )
        .unwrap();
        assert_eq!(row.values.len(), 2);
        assert_eq!(row.values["FlowInExNtv"], 0.0);
        assert_eq!(row.provisional, ["FlowInExNtv"]);
        assert!(network_row(&json!({"time":"invalid"})).is_none());
    }

    #[test]
    fn incomplete_or_malformed_candles_are_excluded() {
        let candle = json!([1000000, "10", "12", "9", "11", "100", 2000000]);
        assert!(candle_row(&candle, 1500000).is_none());
        assert_eq!(candle_row(&candle, 3000000).unwrap().values["close"], 11.0);
        assert!(candle_row(
            &json!([1000000, "10", "9", "8", "11", "100", 2000000]),
            3000000
        )
        .is_none());
    }

    #[test]
    fn futures_flows_use_quote_volume_and_keep_zero_sales() {
        let candle = json!([1000000, "10", "12", "9", "11", "9", 2000000, "100", 7, "4", "60"]);
        let row = futures_row(&candle, 3000000).unwrap();
        assert_eq!(row.values["takerSellQuote"], 40.0);
        assert_eq!(row.values["trades"], 7.0);
        let mut all_buys = candle.clone();
        all_buys[10] = json!("100");
        assert_eq!(
            futures_row(&all_buys, 3000000).unwrap().values["takerSellQuote"],
            0.0
        );
        all_buys[10] = json!("101");
        assert!(futures_row(&all_buys, 3000000).is_none());
        assert!(futures_row(&candle, 1500000).is_none());
    }

    #[test]
    fn aggregate_days_exclude_partial_days_and_preserve_observed_zero() {
        let today = Utc::now().timestamp().div_euclid(86400) * 86400;
        assert!(daily_point(today, &json!(12), "value").is_none());
        assert!(daily_point(today - 1, &json!(12), "value").is_none());
        assert!(daily_point(today - 86400, &Value::Null, "value").is_none());
        assert_eq!(
            daily_point(today - 86400, &json!(0), "value")
                .unwrap()
                .values["value"],
            0.0
        );
    }

    #[tokio::test]
    #[ignore = "requires public provider connectivity"]
    async fn live_network_history_is_real_and_chronological() {
        let data = history("network", "BTC").await.expect("network");
        assert!(data.rows.len() > 1800);
        assert!(data.rows.windows(2).all(|w| w[0].time < w[1].time));
        assert!(data.rows.last().unwrap().values["CapMVRVCur"] > 0.0);
    }

    #[tokio::test]
    #[ignore = "requires public provider connectivity"]
    async fn live_feeds_have_usable_history() {
        for (source, asset) in [
            ("network", "BTC"),
            ("sentiment", "BTC"),
            ("funding", "BTC"),
            ("open-interest", "BTC"),
            ("long-short", "BTC"),
            ("price", "BTC"),
            ("price", "ETH"),
            ("futures", "BTC"),
            ("top-accounts", "BTC"),
            ("top-positions", "BTC"),
            ("chain-volume", "BTC"),
            ("chain-transactions", "BTC"),
            ("chain-difficulty", "BTC"),
            ("defi-tvl", "BTC"),
            ("defi-dex", "BTC"),
            ("defi-fees", "BTC"),
            ("defi-revenue", "BTC"),
        ] {
            let data = history(source, asset).await.expect(source);
            assert!(data.rows.len() > 10, "{source}: too few observations");
            assert!(data.rows.windows(2).all(|w| w[0].time < w[1].time));
            if matches!(
                source,
                "network" | "price" | "funding" | "sentiment" | "futures"
            ) {
                assert!(
                    data.rows.first().unwrap().time < Utc::now().timestamp() - 2192 * 86400,
                    "{source}: Max history must reach earlier than the previous six-year cutoff"
                );
            }
            println!("{source}/{asset}: {} observations", data.rows.len());
            // Optional capture for browser QA through a mocked IPC transport.
            // The fixture contains the actual Rust provider output, not sample prices.
            if let Ok(dir) = std::env::var("QUANTMETRICS_CAPTURE_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{source}-{asset}.json")),
                    serde_json::to_vec(&data).unwrap(),
                )
                .unwrap();
            }
        }
    }
}
