//! The Usage page (card 18; T3 Code's usage.md, PandaCode's agent-usage.md):
//! estimated API cost and tokens from the agents' own logs, and the
//! subscription windows. Nothing runs at startup or on a timer — a scan
//! happens when the page opens or refreshes, off the UI thread, and reads
//! only what changed since the last one. The price list is the one network
//! request: LiteLLM's public file, at most daily, saved in the data dir,
//! with a snapshot built in for offline use. No usage data leaves the machine.

pub mod pricing;
pub mod scan;

use crate::events::RateWindow;
use crate::PilotState;
use chrono::{Local, TimeZone};
use pricing::{Cost, PriceList};
use scan::Ledger;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, State};

const PRICES_URL: &str = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const DAY_MS: i64 = 86_400_000;

/// One bucket of one model: tokens and what they would have cost.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    /// Start of the hour (24h) or the local day, Unix ms.
    pub bucket: i64,
    pub provider: String,
    pub model: String,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    /// `None`: the model has no public price.
    pub cost: Option<Cost>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub period: String,
    /// The first moment counted, Unix ms.
    pub since: i64,
    /// `hour` for 24h, `day` otherwise.
    pub bucket: String,
    pub rows: Vec<UsageRow>,
    /// Where the rates came from: "LiteLLM, fetched today", "saved …", "built-in snapshot".
    pub prices: String,
    /// The folders read.
    pub sources: Vec<String>,
    pub files: usize,
    pub unreadable: usize,
    pub scan_ms: u64,
}

fn period_hours(period: &str) -> i64 {
    match period {
        "24h" => 24,
        "7d" => 24 * 7,
        "30d" => 24 * 30,
        _ => 24 * 90,
    }
}

/// The local day an hour belongs to, as its midnight in Unix ms.
fn local_day(hour: i64) -> i64 {
    match Local.timestamp_opt(hour * 3600, 0).single() {
        Some(t) => t.date_naive().and_hms_opt(0, 0, 0).and_then(|d| Local.from_local_datetime(&d).earliest()).map_or(hour * 3_600_000, |d| d.timestamp_millis()),
        None => hour * 3_600_000,
    }
}

/// Where a period starts, Unix ms: the last 24 whole hours, or today and the
/// days before it as whole local days (7 days = 7 bars).
pub fn period_start(hours: i64, now: chrono::DateTime<Local>) -> i64 {
    if hours <= 24 {
        return (now.timestamp().div_euclid(3600) - hours + 1) * 3_600_000;
    }
    let first = now.date_naive() - chrono::Duration::days(hours / 24 - 1);
    first.and_hms_opt(0, 0, 0).and_then(|d| Local.from_local_datetime(&d).earliest()).map_or(now.timestamp_millis() - hours * 3_600_000, |d| d.timestamp_millis())
}

/// The rows of a period: per bucket, provider and model, priced per size class.
pub fn report_rows(ledger: &Ledger, prices: &PriceList, since_ms: i64, hourly: bool) -> Vec<UsageRow> {
    let mut rows: BTreeMap<(i64, &'static str, String), UsageRow> = BTreeMap::new();
    for (provider, r) in ledger.records(since_ms.div_euclid(3_600_000)) {
        let bucket = if hourly { r.hour * 3_600_000 } else { local_day(r.hour) };
        // A zone with a half-hour offset starts its day inside an hour.
        if bucket < since_ms {
            continue;
        }
        let row = rows.entry((bucket, provider.id(), r.model.clone())).or_insert_with(|| UsageRow {
            bucket,
            provider: provider.id().into(),
            model: r.model.clone(),
            cost: prices.lookup(&r.model).map(|_| Cost::default()),
            ..Default::default()
        });
        row.input += r.tokens.input;
        row.cache_read += r.tokens.cache_read;
        row.cache_write += r.tokens.cache_write_5m + r.tokens.cache_write_1h;
        row.output += r.tokens.output;
        if let (Some(total), Some(rates)) = (row.cost.as_mut(), prices.lookup(&r.model)) {
            total.add(&rates.cost(r.class, &r.tokens));
        }
    }
    rows.into_values().collect()
}

fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(Ledger::default()))
}

// ── Prices ──

struct Saved {
    list: PriceList,
    fetched_at: i64,
}

fn read_saved(dir: &Path) -> Option<Saved> {
    let text = std::fs::read_to_string(dir.join("usage-prices.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = PriceList::from_json(&v["models"].to_string())?;
    Some(Saved { list, fetched_at: v["fetchedAt"].as_i64().unwrap_or(0) })
}

async fn fetch_prices() -> Result<PriceList, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    let resp = client.get(PRICES_URL).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("price list: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("price list: unexpectedly large".into());
    }
    let all: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    PriceList::from_litellm(&all).ok_or_else(|| "price list: no Anthropic or OpenAI models in it".into())
}

/// The rates to use: fetched when the saved copy is a day old (an hour on
/// Refresh), else the saved copy, else the built-in snapshot.
async fn prices(dir: &Path, refresh: bool) -> (PriceList, String) {
    let now = chrono::Utc::now().timestamp_millis();
    let saved = read_saved(dir);
    let age = saved.as_ref().map_or(i64::MAX, |s| now - s.fetched_at);
    if age > if refresh { DAY_MS / 24 } else { DAY_MS } {
        match fetch_prices().await {
            Ok(list) => {
                let body = format!("{{\"fetchedAt\":{now},\"models\":{}}}", list.to_json());
                let _ = std::fs::write(dir.join("usage-prices.json"), body);
                return (list, "LiteLLM price list, fetched today".into());
            }
            Err(e) => eprintln!("pilot: usage prices: {e}"),
        }
    }
    match saved {
        Some(s) => {
            let when = Local.timestamp_millis_opt(s.fetched_at).single().map(|t| t.format("%b %-d").to_string()).unwrap_or_default();
            (s.list, format!("LiteLLM price list from {when}"))
        }
        None => (PriceList::bundled(), "built-in price snapshot (offline)".into()),
    }
}

/// Cost and tokens of a period (`24h`, `7d`, `30d`, `90d`).
#[tauri::command]
pub async fn pilot_usage_report(state: State<'_, PilotState>, period: String, refresh: Option<bool>) -> Result<UsageReport, String> {
    let (prices, source) = prices(&state.data_dir, refresh.unwrap_or(false)).await;
    let hours = period_hours(&period);
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let roots = scan::roots();
        let files = scan::files(&roots);
        let mut ledger = ledger().lock().unwrap_or_else(|e| e.into_inner());
        let stats = ledger.refresh(&files);
        let since = period_start(hours, Local::now());
        let rows = report_rows(&ledger, &prices, since, hours <= 24);
        UsageReport {
            since,
            bucket: if hours <= 24 { "hour".into() } else { "day".into() },
            period,
            rows,
            prices: source,
            sources: roots.iter().map(|(p, d)| format!("{}: {}", p.id(), d.display())).collect(),
            files: stats.files,
            unreadable: stats.unreadable,
            scan_ms: started.elapsed().as_millis() as u64,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

// ── Limits ──

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLimits {
    pub provider: String,
    pub windows: Vec<RateWindow>,
    /// When they were reported, Unix ms; `None` = never.
    pub at: Option<i64>,
    /// Asked for now and not answered (the last reading stays).
    pub error: Option<String>,
}

/// Codex is asked at most every five minutes, unless the page refreshes.
const CODEX_MIN_AGE_MS: i64 = 5 * 60_000;

/// The subscription windows: Codex answers `account/rateLimits/read` without
/// a turn; Claude reports its windows during turns, and the last report stays.
#[tauri::command(async)]
pub fn pilot_usage_limits(app: AppHandle, state: State<'_, PilotState>, refresh: Option<bool>) -> Result<Vec<ProviderLimits>, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let stored = state.db.limits().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for provider in ["claude", "codex"] {
        let (windows, at) = stored
            .iter()
            .find(|(p, _, _)| p == provider)
            .map(|(_, w, at)| (serde_json::from_str::<Vec<RateWindow>>(w).unwrap_or_default(), Some(*at)))
            .unwrap_or_default();
        let mut limits = ProviderLimits { provider: provider.into(), windows, at, error: None };
        let max_age = if refresh.unwrap_or(false) { 0 } else { CODEX_MIN_AGE_MS };
        let stale = at.map_or(true, |t| now - t > max_age);
        if provider == "codex" && stale {
            match crate::threads::codex_limits(&app, &state) {
                Ok(windows) => {
                    let _ = state.db.set_limits("codex", &serde_json::to_string(&windows).unwrap_or_default(), now);
                    limits.windows = windows;
                    limits.at = Some(now);
                }
                Err(e) => limits.error = Some(e),
            }
        }
        out.push(limits);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
