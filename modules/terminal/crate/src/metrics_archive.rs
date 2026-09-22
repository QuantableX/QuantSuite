//! Public Binance positioning archive, joined to current observations at exact UTC midnight.
//! Persist compact daily snapshots; subsequent launches only fetch new/revised archive files.
use super::{derivatives, Row};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    sync::OnceLock,
};
use tokio::sync::Mutex;

const PREFIX: &str = "data/futures/um/daily/metrics/BTCUSDT/";
const FIELDS: [(&str, &str); 4] = [
    ("sum_open_interest_value", "sumOpenInterestValue"),
    ("count_long_short_ratio", "longShortRatio"),
    ("count_toptrader_long_short_ratio", "topAccountRatio"),
    ("sum_toptrader_long_short_ratio", "topPositionRatio"),
];
#[derive(Clone, Serialize, Deserialize)]
struct CachedDay {
    etag: String,
    row: Option<Row>,
}
#[derive(Default)]
struct Cache {
    checked: i64,
    days: BTreeMap<String, CachedDay>,
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    xml.split_once(&format!("<{name}>"))?
        .1
        .split_once(&format!("</{name}>"))
        .map(|v| v.0)
}

async fn listing() -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::new();
    let mut marker = String::new();
    for _ in 0..30 {
        let response = super::super::market::client()?
            .get("https://s3-ap-northeast-1.amazonaws.com/data.binance.vision")
            .query(&[
                ("prefix", PREFIX),
                ("max-keys", "1000"),
                ("marker", &marker),
            ])
            .send()
            .await
            .map_err(|e| format!("Archive listing: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Archive listing: {e}"))?;
        let body = response.text().await.map_err(|e| e.to_string())?;
        let mut last_key = None;
        for item in body.split("<Contents>").skip(1) {
            let key = tag(item, "Key").ok_or("Archive listing missing key")?;
            last_key = Some(key.to_string());
            let Some(date) = key
                .strip_prefix(PREFIX)
                .and_then(|v| v.strip_prefix("BTCUSDT-metrics-"))
                .and_then(|v| v.strip_suffix(".zip"))
            else {
                continue;
            };
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| e.to_string())?;
            if parsed >= Utc::now().date_naive() {
                continue;
            }
            files.push((
                date.to_string(),
                tag(item, "ETag")
                    .ok_or("Archive listing missing version")?
                    .to_string(),
            ));
        }
        if tag(&body, "IsTruncated") == Some("false") {
            return Ok(files);
        }
        let next = last_key.ok_or("Archive listing did not advance")?;
        if next <= marker {
            return Err("Archive listing did not advance".into());
        }
        marker = next;
    }
    Err("Archive listing exceeded pagination limit".into())
}

fn parse_csv(csv: &str, date: &str) -> Result<Option<Row>, String> {
    let mut lines = csv.lines();
    let header: Vec<_> = lines
        .next()
        .ok_or("Empty archive CSV")?
        .trim_start_matches('\u{feff}')
        .split(',')
        .collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|c| *c == name)
            .ok_or_else(|| format!("Archive missing {name}"))
    };
    let time_col = column("create_time")?;
    let symbol_col = column("symbol")?;
    let columns = FIELDS
        .iter()
        .map(|(name, _)| column(name))
        .collect::<Result<Vec<_>, _>>()?;
    let expected = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| e.to_string())?
        .and_hms_opt(0, 0, 0)
        .ok_or("Invalid archive date")?
        .and_utc()
        .timestamp();
    let mut result: Option<Row> = None;
    for line in lines {
        let cells: Vec<_> = line.split(',').collect();
        if cells.len() != header.len() {
            return Err("Malformed archive CSV row".into());
        }
        if cells[symbol_col] != "BTCUSDT" {
            return Err("Archive symbol mismatch".into());
        }
        let time = NaiveDateTime::parse_from_str(cells[time_col], "%Y-%m-%d %H:%M:%S")
            .map_err(|e| e.to_string())?
            .and_utc()
            .timestamp();
        // Use the same exact midnight sampling as the 1d REST endpoint. Never substitute 00:05.
        if time != expected {
            continue;
        }
        let mut values = BTreeMap::new();
        for (i, (_, field)) in FIELDS.iter().enumerate() {
            if let Ok(value) = cells[columns[i]].parse::<f64>() {
                if value.is_finite() && value >= 0.0 {
                    values.insert((*field).into(), value);
                }
            }
        }
        if let Some(previous) = &result {
            if previous.values != values {
                return Err("Conflicting duplicate archive snapshots".into());
            }
        }
        if !values.is_empty() {
            result = Some(Row {
                time,
                values,
                provisional: vec![],
            });
        }
    }
    Ok(result)
}

async fn download(date: String, etag: String) -> Result<(String, CachedDay), String> {
    let url = format!("https://data.binance.vision/{PREFIX}BTCUSDT-metrics-{date}.zip");
    let response = super::super::market::client()?
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Archive {date}: {e}"))?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("Archive file exceeds size limit".into());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if archive.len() != 1 {
        return Err("Archive has unexpected entries".into());
    }
    let entry = archive.by_index(0).map_err(|e| e.to_string())?;
    if entry.size() > 8_000_000 {
        return Err("Archive CSV exceeds size limit".into());
    }
    let mut csv = String::new();
    entry
        .take(8_000_001)
        .read_to_string(&mut csv)
        .map_err(|e| e.to_string())?;
    let row = parse_csv(&csv, &date)?;
    Ok((date, CachedDay { etag, row }))
}

async fn archived() -> Result<Vec<Row>, String> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Cache::default()))
        .lock()
        .await;
    let now = Utc::now().timestamp();
    if now - cache.checked < 3600 {
        return Ok(cache
            .days
            .values()
            .filter_map(|day| day.row.clone())
            .collect());
    }
    let directory = qs_core::paths::ensure_module("terminal")
        .map_err(|e| e.to_string())?
        .join("metrics");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("binance-positioning-v1.json");
    if cache.days.is_empty() {
        if let Ok(bytes) = std::fs::read(&path) {
            cache.days = serde_json::from_slice(&bytes).unwrap_or_default();
        }
    }
    let files = listing().await?;
    if files.is_empty() {
        return Err("Public positioning archive is empty".into());
    }
    let pending: Vec<_> = files
        .iter()
        .filter(|(date, etag)| cache.days.get(date).map(|v| &v.etag) != Some(etag))
        .cloned()
        .collect();
    for chunk in pending.chunks(8) {
        let mut jobs = tokio::task::JoinSet::new();
        for (date, etag) in chunk {
            jobs.spawn(download(date.clone(), etag.clone()));
        }
        let mut failure = None;
        while let Some(result) = jobs.join_next().await {
            match result.map_err(|e| e.to_string()).and_then(|v| v) {
                Ok((date, day)) => {
                    cache.days.insert(date, day);
                }
                Err(error) => {
                    failure = Some(error);
                }
            }
        }
        qs_core::paths::write_atomic(
            &path,
            &serde_json::to_vec(&cache.days).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if let Some(error) = failure {
            return Err(error);
        }
    }
    // Discard withdrawn files, while retaining only provider-listed, successfully parsed snapshots.
    cache
        .days
        .retain(|date, _| files.iter().any(|(d, _)| d == date));
    cache.checked = now;
    Ok(cache
        .days
        .values()
        .filter_map(|day| day.row.clone())
        .collect())
}

pub async fn history() -> Result<Vec<Row>, String> {
    let mut rows: BTreeMap<i64, Row> = archived()
        .await?
        .into_iter()
        .map(|row| (row.time, row))
        .collect();
    for (source, original, field) in [
        (
            "open-interest",
            "sumOpenInterestValue",
            "sumOpenInterestValue",
        ),
        ("long-short", "longShortRatio", "longShortRatio"),
        ("top-accounts", "longShortRatio", "topAccountRatio"),
        ("top-positions", "longShortRatio", "topPositionRatio"),
    ] {
        for row in derivatives(source).await? {
            if row.time % 86400 != 0 {
                continue;
            }
            if let Some(value) = row.values.get(original).copied().filter(|v| *v >= 0.0) {
                rows.entry(row.time)
                    .or_insert_with(|| Row {
                        time: row.time,
                        values: BTreeMap::new(),
                        provisional: vec![],
                    })
                    .values
                    .insert(field.into(), value);
            }
        }
    }
    Ok(rows.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "create_time,symbol,sum_open_interest_value,count_long_short_ratio,count_toptrader_long_short_ratio,sum_toptrader_long_short_ratio\n";
    #[test]
    fn exact_midnight_dedup_and_missing_values() {
        let line = "2020-09-01 00:00:00,BTCUSDT,100,1.1,NaN,0\n";
        let row = parse_csv(&format!("{HEADER}{line}{line}"), "2020-09-01")
            .unwrap()
            .unwrap();
        assert_eq!(row.values["sumOpenInterestValue"], 100.0);
        assert_eq!(row.values["topPositionRatio"], 0.0);
        assert!(!row.values.contains_key("topAccountRatio"));
        assert!(parse_csv(
            &format!("{HEADER}{}", line.replace("00:00:00", "00:05:00")),
            "2020-09-01"
        )
        .unwrap()
        .is_none());
        assert!(parse_csv(
            &format!("{HEADER}{line}{}", line.replace(",100,", ",200,")),
            "2020-09-01"
        )
        .is_err());
    }
}
