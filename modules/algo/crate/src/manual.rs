//! Manual systems trade by themselves (PLAN-AUTO-EXECUTION §2.3–§2.5, card 3).
//!
//! Every saved QuantSystems system gets an Execution setting: Off (default),
//! Paper or Live, the account, the capital, the run time after the daily
//! close (00:15 UTC) and a retry window (6 h). An in-process scheduler sleeps
//! until the next due time; at it, the system's live signal is evaluated,
//! checked for fresh data (the closed candle of every coin it scored), turned
//! into a target and handed to the execution core. Exactly one run per
//! (system, close date): the run id is `manual-<system>-<date>`, a finished
//! run is never run again, an interrupted one resumes.
//!
//! The settings live here in quantalgo.db, not in the system's config: a
//! saved system stays exactly as it was.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, NaiveTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State, Wry};

use crate::execution::engine::{Engine, RunRequest};
use crate::execution::guards::GuardConfig;
use crate::execution::paper::PaperVenue;
use crate::execution::store::{self as exec_store, Holdings, OrderRecord, PortfolioTarget, RunRecord, RunStatus, RunStore, SqliteStore, Target};
use crate::execution::mapping::quote_for;
use crate::execution::venue::{free_balance, ExecutionVenue, MarketType};
use crate::runtime::Mode;
use crate::AppState;

/// The event every finished, skipped or stopped run sends to the pages.
pub const RUN_EVENT: &str = "algo:manual-run";
/// The account quote of the CEX venues.
const USD: &str = "USD";
/// Data that is not there yet is asked again this often inside the window.
const RETRY_EVERY: ChronoDuration = ChronoDuration::minutes(5);

// ── settings ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecMode {
    #[default]
    Off,
    Paper,
    Live,
}

/// How much a system trades with: a fixed amount, a share of the account's
/// quote balance, or the whole (sub-)account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capital {
    /// `fixed` | `percent` | `whole`
    pub kind: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualSettings {
    pub mode: ExecMode,
    pub exchange_id: Option<String>,
    /// `spot`, or `perp` on a venue that has one (Hyperliquid).
    pub market: String,
    /// Perp: the position's leverage, isolated margin (1x default; capped
    /// in Settings, Venues 4/5). Always 1 on spot.
    pub leverage: f64,
    pub capital: Capital,
    /// `HH:MM`, UTC.
    pub run_time: String,
    /// The run may wait for data until this many hours after the close.
    pub retry_hours: u32,
    /// A coin the venue cannot trade: `hold_usd` (hold USD when it is the
    /// pick; backtest ≠ live) or `restrict` (drop `restricted_symbols` from
    /// the universe in backtest and live alike; card 3b).
    pub coverage_policy: String,
    /// The coins dropped under `restrict`.
    pub restricted_symbols: Vec<String>,
    pub max_order_value: Option<f64>,
    pub daily_turnover_cap: Option<f64>,
    pub paper_fee_pct: f64,
    pub paper_slippage_pct: f64,
}

impl Default for ManualSettings {
    fn default() -> Self {
        Self {
            mode: ExecMode::Off,
            exchange_id: None,
            market: "spot".into(),
            capital: Capital { kind: "fixed".into(), value: 1_000.0 },
            leverage: 1.0,
            run_time: "00:15".into(),
            retry_hours: 6,
            coverage_policy: "hold_usd".into(),
            restricted_symbols: Vec::new(),
            max_order_value: None,
            daily_turnover_cap: None,
            paper_fee_pct: 0.1,
            paper_slippage_pct: 0.05,
        }
    }
}

impl ManualSettings {
    pub fn validate(&self) -> Result<(), String> {
        parse_run_time(&self.run_time)?;
        if !(1..=23).contains(&self.retry_hours) {
            return Err("The retry window is 1 to 23 hours.".into());
        }
        match self.capital.kind.as_str() {
            "fixed" if self.capital.value > 0.0 => {}
            "percent" if self.capital.value > 0.0 && self.capital.value <= 100.0 => {}
            "whole" => {}
            "fixed" => return Err("A fixed capital needs an amount above 0.".into()),
            "percent" => return Err("A share of the balance is between 0 and 100 %.".into()),
            other => return Err(format!("Unknown capital kind {other:?}.")),
        }
        if !matches!(self.market.as_str(), "spot" | "perp") {
            return Err(format!("Unknown market {:?}.", self.market));
        }
        if !self.leverage.is_finite() || self.leverage < 1.0 {
            return Err("Leverage is 1x or more.".into());
        }
        if self.market == "spot" && self.leverage != 1.0 {
            return Err("Leverage is a perp setting; spot is always 1x.".into());
        }
        if !matches!(self.coverage_policy.as_str(), "hold_usd" | "restrict") {
            return Err(format!("Unknown coverage policy {:?}.", self.coverage_policy));
        }
        if self.restricted_symbols.iter().any(|s| s.trim().is_empty() || s.contains(['/', ' '])) {
            return Err("Restricted coins are plain symbols like SOL.".into());
        }
        if self.mode != ExecMode::Off && self.exchange_id.as_deref().unwrap_or("").is_empty() {
            return Err("Choose the account the system trades on.".into());
        }
        for (label, v) in [("max order value", self.max_order_value), ("daily turnover cap", self.daily_turnover_cap)] {
            if v.is_some_and(|v| v <= 0.0) {
                return Err(format!("The {label} must be above 0 (or empty for none)."));
            }
        }
        if !(0.0..=1.0).contains(&self.paper_fee_pct) || !(0.0..=1.0).contains(&self.paper_slippage_pct) {
            return Err("Paper fee and slippage are between 0 and 1 %.".into());
        }
        Ok(())
    }

    /// The fields whose change disarms Live.
    fn arming_key(&self) -> (ExecMode, Option<String>, String, String, u64) {
        (self.mode, self.exchange_id.clone(), self.market.clone(), self.capital.kind.clone(), self.capital.value.to_bits())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualRow {
    pub system_id: String,
    pub settings: ManualSettings,
    pub armed: bool,
    pub armed_at: Option<String>,
    pub ack_not_robust: bool,
    pub updated_at: String,
}

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS manual_execution (
            system_id TEXT PRIMARY KEY,
            settings_json TEXT NOT NULL,
            armed INTEGER NOT NULL DEFAULT 0,
            armed_at TEXT,
            ack_not_robust INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );",
    )
}

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<ManualRow> {
    let json: String = r.get(1)?;
    Ok(ManualRow {
        system_id: r.get(0)?,
        settings: serde_json::from_str(&json).unwrap_or_default(),
        armed: r.get::<_, i64>(2)? != 0,
        armed_at: r.get(3)?,
        ack_not_robust: r.get::<_, i64>(4)? != 0,
        updated_at: r.get(5)?,
    })
}

pub fn load_row(conn: &Connection, system_id: &str) -> Result<Option<ManualRow>, String> {
    conn.query_row(
        "SELECT system_id, settings_json, armed, armed_at, ack_not_robust, updated_at FROM manual_execution WHERE system_id = ?1",
        params![system_id],
        row_from,
    )
    .optional()
    .map_err(|e| format!("manual_execution: {e}"))
}

pub fn active_rows(conn: &Connection) -> Result<Vec<ManualRow>, String> {
    let mut stmt = conn
        .prepare("SELECT system_id, settings_json, armed, armed_at, ack_not_robust, updated_at FROM manual_execution")
        .map_err(|e| format!("manual_execution: {e}"))?;
    let rows = stmt
        .query_map([], row_from)
        .map_err(|e| format!("manual_execution: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("manual_execution: {e}"))?;
    Ok(rows.into_iter().filter(|r| r.settings.mode != ExecMode::Off).collect())
}

/// Save settings; a change of mode, account, market or capital disarms Live.
pub fn save_settings(conn: &Connection, system_id: &str, settings: &ManualSettings) -> Result<ManualRow, String> {
    settings.validate()?;
    let now = Utc::now().to_rfc3339();
    let previous = load_row(conn, system_id)?;
    let keep_armed = previous
        .as_ref()
        .is_some_and(|p| p.armed && p.settings.arming_key() == settings.arming_key() && settings.mode == ExecMode::Live);
    let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO manual_execution (system_id, settings_json, armed, armed_at, ack_not_robust, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(system_id) DO UPDATE SET settings_json = excluded.settings_json, armed = excluded.armed,
           armed_at = excluded.armed_at, ack_not_robust = excluded.ack_not_robust, updated_at = excluded.updated_at",
        params![
            system_id,
            json,
            keep_armed as i64,
            previous.as_ref().filter(|_| keep_armed).and_then(|p| p.armed_at.clone()),
            previous.as_ref().filter(|_| keep_armed).map(|p| p.ack_not_robust as i64).unwrap_or(0),
            now
        ],
    )
    .map_err(|e| format!("manual_execution: {e}"))?;
    let row = load_row(conn, system_id)?.ok_or_else(|| "manual_execution: row missing".to_string())?;
    write_restrictions(conn)?;
    Ok(row)
}

/// Every `restrict` system's coins, for QuantSystems to merge into the
/// engine config of its backtests and live evaluations (card 3b).
pub fn restrictions(conn: &Connection) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let mut stmt = conn
        .prepare("SELECT system_id, settings_json FROM manual_execution")
        .map_err(|e| format!("manual_execution: {e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("manual_execution: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("manual_execution: {e}"))?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, json)| {
            let s: ManualSettings = serde_json::from_str(&json).ok()?;
            let mut coins: Vec<String> = s.restricted_symbols.iter().map(|c| c.trim().to_uppercase()).collect();
            coins.sort();
            coins.dedup();
            (s.coverage_policy == "restrict" && !coins.is_empty()).then_some((id, coins))
        })
        .collect())
}

fn write_restrictions(conn: &Connection) -> Result<(), String> {
    if cfg!(test) {
        // Unit tests never write into a real data root.
        return Ok(());
    }
    let map = restrictions(conn)?;
    let path = qs_mod_systems::restrictions_path();
    if map.is_empty() && !path.exists() {
        return Ok(());
    }
    let json = serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    qs_core::paths::write_atomic(&path, &json).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn set_armed(conn: &Connection, system_id: &str, armed: bool, ack_not_robust: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE manual_execution SET armed = ?2, armed_at = ?3, ack_not_robust = ?4, updated_at = ?5 WHERE system_id = ?1",
        params![
            system_id,
            armed as i64,
            armed.then(|| Utc::now().to_rfc3339()),
            (armed && ack_not_robust) as i64,
            Utc::now().to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("manual_execution: {e}"))
}

/// Disarm every Manual system (the kill switch, card 4).
pub fn disarm_all(conn: &Connection) -> Result<usize, String> {
    conn.execute("UPDATE manual_execution SET armed = 0, armed_at = NULL WHERE armed = 1", [])
        .map_err(|e| format!("manual_execution: {e}"))
}

// ── the schedule ────────────────────────────────────────────────────────────

pub fn parse_run_time(text: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").map_err(|_| format!("{text:?} is not a time like 00:15 (UTC)."))
}

/// When the run for the close at `close` 00:00 UTC is due.
pub fn due_at(close: NaiveDate, run_time: NaiveTime) -> DateTime<Utc> {
    Utc.from_utc_datetime(&close.and_time(run_time))
}

/// The end of the retry window: `retry_hours` after the close, at least half
/// an hour after the due time.
pub fn window_end(close: NaiveDate, run_time: NaiveTime, retry_hours: u32) -> DateTime<Utc> {
    let by_close = Utc.from_utc_datetime(&close.and_time(NaiveTime::MIN)) + ChronoDuration::hours(retry_hours as i64);
    by_close.max(due_at(close, run_time) + ChronoDuration::minutes(30))
}

/// The close whose run is due now, if `now` is inside its window. A suite
/// started after the due time runs once if the window is still open, else
/// the day is skipped (§2.3 "If the suite was off at run time").
pub fn current_close(now: DateTime<Utc>, run_time: NaiveTime, retry_hours: u32) -> Option<NaiveDate> {
    let today = now.date_naive();
    (due_at(today, run_time) <= now && now < window_end(today, run_time, retry_hours)).then_some(today)
}

/// The next due time strictly after `now`.
pub fn next_due(now: DateTime<Utc>, run_time: NaiveTime) -> DateTime<Utc> {
    let today = now.date_naive();
    let due = due_at(today, run_time);
    if due > now {
        due
    } else {
        due_at(today + ChronoDuration::days(1), run_time)
    }
}

pub fn run_id(system_id: &str, close: NaiveDate) -> String {
    format!("manual-{system_id}-{close}")
}

pub fn owner(system_id: &str) -> String {
    format!("system:{system_id}")
}

// ── the signal ──────────────────────────────────────────────────────────────

/// What a run trades on, read from the engine's live result.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub as_of: String,
    /// The coin to hold, or `None` for USD / cash.
    pub hold: Option<String>,
    /// Every coin the signal scored (the closed candle of each must exist).
    pub scored: Vec<String>,
    pub single_asset: bool,
    /// Single asset: when the signal's bar closed; its close price.
    pub closed_at: Option<String>,
    pub close: Option<f64>,
    /// The signal is short (single-asset systems): a short on perp, cash
    /// on spot.
    pub short: bool,
    /// Portfolio mode: tomorrow's weights from the engine (`portfolioTarget`),
    /// cash as `USD`. `hold` keeps the single pick's meaning and is not traded.
    pub portfolio: Option<BTreeMap<String, f64>>,
}

/// A saved run config with portfolio mode switched on (rotation systems only, as the engine reads it).
pub fn portfolio_on(config: &Value) -> bool {
    config.get("mode").and_then(|v| v.as_str()).unwrap_or("rotation") == "rotation"
        && config.pointer("/portfolio/enabled").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// What makes two portfolio configs the same system for its verdict: the
/// portfolio block with its defaults (`withUsd` = `with_usd`). `None` = off.
pub fn portfolio_key(config: &Value) -> Option<(i64, String, String, String)> {
    if !portfolio_on(config) {
        return None;
    }
    let p = config.get("portfolio")?;
    let text = |keys: [&str; 2], default: &str| {
        keys.iter()
            .find_map(|k| p.get(*k).and_then(|v| v.as_str()))
            .unwrap_or(default)
            .to_lowercase()
            .replace('_', "")
    };
    Some((
        p.get("positions").and_then(|v| v.as_i64()).unwrap_or(0),
        text(["weighting", "weighting"], "equal"),
        text(["notTrending", "not_trending"], "withUsd"),
        text(["rebalance", "rebalance"], "onChange"),
    ))
}

/// The backtest's rebalancing rule of a portfolio config: every day, or
/// only when the set of assets changes (the default).
pub fn portfolio_rebalances_daily(config: &Value) -> bool {
    portfolio_key(config).is_some_and(|k| k.3 == "daily")
}

/// The engine's `live` result → the signal.
pub fn parse_signal(live: &Value) -> Result<Signal, String> {
    let as_of = live.get("asOf").and_then(|v| v.as_str()).ok_or("the live result has no asOf")?.to_string();
    let portfolio = match live.get("portfolioTarget") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => {
            let mut weights = BTreeMap::new();
            for (asset, w) in map {
                let w = w.as_f64().filter(|w| w.is_finite() && *w >= 0.0).ok_or_else(|| format!("the portfolio weight of {asset} is not a share"))?;
                if w > 0.0 {
                    weights.insert(asset.clone(), w);
                }
            }
            let total: f64 = weights.values().sum();
            if (total - 1.0).abs() > 1e-6 {
                return Err(format!("the portfolio weights add up to {total}, not 1"));
            }
            Some(weights)
        }
        Some(_) => return Err("the portfolio target is not a list of weights".into()),
    };
    if live.get("mode").and_then(|v| v.as_str()) == Some("single_asset") {
        let sig = live.get("singleAssetSignal").ok_or("the single-asset result has no signal")?;
        let base = live
            .get("symbols")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .ok_or("the single-asset result names no pair")?
            .to_string();
        let position = sig.get("position").and_then(|v| v.as_str()).unwrap_or("cash");
        return Ok(Signal {
            as_of,
            hold: (position == "long").then(|| base.clone()),
            scored: vec![base],
            single_asset: true,
            closed_at: sig.get("closedAt").and_then(|v| v.as_str()).map(String::from),
            close: sig.get("close").and_then(|v| v.as_f64()),
            short: position == "short",
            portfolio: None,
        });
    }
    let best = live.get("best").and_then(|v| v.as_str());
    let scored: Vec<String> = live
        .get("symbols")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).filter(|s| *s != USD).map(String::from).collect())
        .unwrap_or_default();
    if scored.is_empty() {
        return Err("the live result has no scored coins (no ranking for the day?)".into());
    }
    Ok(Signal {
        as_of,
        hold: best.filter(|b| *b != USD).map(String::from),
        scored,
        single_asset: false,
        closed_at: None,
        close: None,
        short: false,
        portfolio,
    })
}

/// Whether the data the signal used is the closed candle of `close - 1 day`:
/// `Ok(())` or the reason. `closes` holds the reference exchange's close of
/// that candle per coin (missing = not there yet).
pub fn freshness(signal: &Signal, close: NaiveDate, closes: &HashMap<String, f64>) -> Result<(), String> {
    if signal.as_of != close.to_string() {
        return Err(format!("the signal is as of {}, not {close}", signal.as_of));
    }
    let closed_day = close - ChronoDuration::days(1);
    if signal.single_asset {
        let closed = signal
            .closed_at
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(&t.replace("+00:00Z", "Z")).ok())
            .map(|t| t.with_timezone(&Utc));
        let need = Utc.from_utc_datetime(&close.and_time(NaiveTime::MIN));
        return match closed {
            Some(t) if t >= need => Ok(()),
            Some(t) => Err(format!("the newest closed bar ends {t}, before {need}")),
            None => Err("the signal does not say when its bar closed".into()),
        };
    }
    let missing: Vec<&str> = signal.scored.iter().filter(|s| !closes.contains_key(*s)).map(String::as_str).collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("no {closed_day} candle yet for {}", missing.join(", ")))
    }
}

/// The run's target and its note: the pick, or USD when the venue cannot
/// trade it (policy `hold_usd`, flagged backtest ≠ live). A short signal is a
/// short on perp and cash on spot (spot stays long-only).
pub fn target_for(signal: &Signal, tradable: &dyn Fn(&str) -> bool, perp: bool) -> (Target, Vec<String>) {
    let mut notes = Vec::new();
    if signal.short {
        let coin = signal.scored.first().cloned().unwrap_or_default();
        if !perp {
            notes.push("the signal is short; spot is long-only, so the system holds USD (choose Perp to short)".into());
            return (Target::Flat, notes);
        }
        if !tradable(&coin) {
            notes.push(format!("{coin} has no perp here: holding USD instead (backtest ≠ live)"));
            return (Target::Flat, notes);
        }
        return (Target::Short(coin), notes);
    }
    match &signal.hold {
        None => (Target::Flat, notes),
        Some(coin) if tradable(coin) => (Target::Hold(coin.clone()), notes),
        Some(coin) => {
            notes.push(format!("{coin} is not tradable on this venue: holding USD instead (backtest ≠ live)"));
            (Target::Flat, notes)
        }
    }
}

/// A portfolio signal → the weights the account trades to (card 791a9140).
/// `USD` becomes the account's quote. A coin the venue cannot trade gives
/// its share to the quote, flagged backtest ≠ live (policy `hold_usd`; under
/// `restrict` the coin is out of the universe and never in the weights).
pub fn portfolio_weights(
    weights: &BTreeMap<String, f64>,
    quote: &str,
    tradable: &dyn Fn(&str) -> bool,
) -> (BTreeMap<String, f64>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut notes = Vec::new();
    for (asset, w) in weights {
        let key = if asset.eq_ignore_ascii_case(USD) || asset.eq_ignore_ascii_case(quote) {
            quote.to_string()
        } else if tradable(asset) {
            asset.clone()
        } else {
            notes.push(format!(
                "{asset} ({:.1} %) is not tradable on this venue: that share is held as {quote} (backtest ≠ live)",
                w * 100.0
            ));
            quote.to_string()
        };
        *out.entry(key).or_insert(0.0) += w;
    }
    (out, notes)
}

/// The backtest's rebalancing rule, live (card 791a9140): back to the
/// weights every day (`daily`), or only when the set of assets changes
/// against the last run's target (`onChange`); otherwise nothing trades and
/// the weights drift. The last run must have finished: after a partial,
/// failed or stopped run the account is not what its target said, so the
/// run trades back to the weights.
pub fn portfolio_rebalance(daily: bool, previous: Option<&RunRecord>, today: &PortfolioTarget) -> (bool, String) {
    if daily {
        return (true, "rebalancing every day, as the backtest does".into());
    }
    let Some(prev) = previous else {
        return (true, "the system's first run: trading to the weights".into());
    };
    let day = prev.close_date.as_deref().unwrap_or("?");
    if prev.status != RunStatus::Done {
        return (true, format!("the last run ({day}) ended {}: trading back to the weights", prev.status.as_str()));
    }
    let before: BTreeSet<String> = match &prev.target {
        Target::Portfolio(p) => p.assets(),
        Target::Flat => [prev.quote.clone()].into(),
        Target::Hold(a) if a.eq_ignore_ascii_case(&prev.quote) => [prev.quote.clone()].into(),
        Target::Hold(a) | Target::Short(a) => [a.clone()].into(),
    };
    let now = today.assets();
    let list = |s: &BTreeSet<String>| s.iter().cloned().collect::<Vec<_>>().join(", ");
    if before == now {
        (false, format!("the same assets as after the last run ({day}: {}): the weights drift, as in the backtest", list(&now)))
    } else {
        (true, format!("the assets change from {} to {}: trading to the weights", list(&before), list(&now)))
    }
}

/// The account's venue and market, for its reference prices.
struct VenueMarket<'a> {
    provider: &'a str,
    testnet: bool,
    market: MarketType,
    quote: &'a str,
}

/// The closed daily candle of `day` for every coin: the venue's own market
/// when it lists one (Hyperliquid's UBTC/USDC for BTC), else Binance (the
/// suite's first data source).
fn reference_closes(venue: &VenueMarket, coins: &[String], day: NaiveDate, listed: &dyn Fn(&str) -> bool) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for coin in coins {
        let binance = || crate::venues::daily_close("binance", false, MarketType::Spot, coin, "USDT", day);
        // A venue without candles (Jupiter) falls back to Binance too.
        let close = if listed(coin) {
            crate::venues::daily_close(venue.provider, venue.testnet, venue.market, coin, venue.quote, day).or_else(binance)
        } else {
            binance()
        };
        if let Some(c) = close {
            out.insert(coin.clone(), c);
        }
    }
    out
}

// ── robustness ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Verdict {
    pub run_id: String,
    pub status: String,
    pub robust: bool,
    pub finished_at: String,
}

/// The newest finished System Gauntlet verdict of a system (docs/ROBUSTNESS.md),
/// read from QuantScript's research runs. `None` when there is none.
///
/// `portfolio` is the system's portfolio block today ([`portfolio_key`]):
/// only a run of the same block counts, so a portfolio system is never armed
/// on its single-pick verdict, nor a single-pick system on a portfolio one.
pub fn robustness_verdict(research_dir: &Path, system_id: &str, portfolio: Option<&(i64, String, String, String)>) -> Option<Verdict> {
    let mut best: Option<Verdict> = None;
    for entry in std::fs::read_dir(research_dir).ok()?.flatten() {
        let dir = entry.path();
        let Ok(run) = std::fs::read_to_string(dir.join("run.json")) else { continue };
        let Ok(run) = serde_json::from_str::<Value>(&run) else { continue };
        if run.get("kind").and_then(|v| v.as_str()) != Some("robustness")
            || run.get("status").and_then(|v| v.as_str()) != Some("done")
            || run.pointer("/system/id").and_then(|v| v.as_str()) != Some(system_id)
            || run.get("config").and_then(portfolio_key).as_ref() != portfolio
        {
            continue;
        }
        let verdict = std::fs::read_to_string(dir.join("verdict.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .or_else(|| {
                std::fs::read_to_string(dir.join("results.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    .and_then(|r| r.get("verdict").cloned())
            });
        let Some(verdict) = verdict else { continue };
        let finished = run.get("finished_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let candidate = Verdict {
            run_id: run.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            status: verdict.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            robust: verdict.get("robust").and_then(|v| v.as_bool()).unwrap_or(false),
            finished_at: finished,
        };
        if best.as_ref().map_or(true, |b| b.finished_at < candidate.finished_at) {
            best = Some(candidate);
        }
    }
    best
}

fn research_dir() -> std::path::PathBuf {
    qs_core::paths::module_dir("script").join("research")
}

// ── running ─────────────────────────────────────────────────────────────────

fn exchange_provider(conn: &Connection, exchange_id: &str) -> Result<(String, bool), String> {
    conn.query_row("SELECT provider, sandbox FROM exchanges WHERE id = ?1", params![exchange_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
    })
    .map_err(|_| "the system's account is not connected any more".to_string())
}

/// The leverage against the user's cap (QuantAlgo Settings, default 3x).
pub fn check_leverage(leverage: f64, cap: f64) -> Result<(), String> {
    if leverage.fract() != 0.0 {
        return Err("Leverage is a whole number (1x, 2x, 3x).".into());
    }
    if leverage > cap + 1e-9 {
        return Err(format!(
            "{leverage}x is above the leverage cap of {cap}x. A rotation system has no reason for leverage; raise the cap in QuantAlgo Settings if you mean it."
        ));
    }
    Ok(())
}

impl ManualSettings {
    pub fn market_type(&self) -> MarketType {
        if self.market == "perp" {
            MarketType::Perp
        } else {
            MarketType::Spot
        }
    }
}

/// The live venue (keys from the vault) of an account, for the system's
/// market and leverage.
fn live_venue(state: &AppState, exchange_id: &str, market: MarketType, leverage: f64) -> Result<Box<dyn ExecutionVenue>, String> {
    let creds = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        crate::exchanges::load_credentials(&db, exchange_id)?
    };
    crate::venues::open_leveraged(&creds, market, leverage.round() as u32)
}

/// What a run starts with when the system holds nothing yet.
fn starting_capital(
    settings: &ManualSettings,
    account: Option<&dyn ExecutionVenue>,
    quote: &str,
) -> Result<(Option<f64>, Option<Holdings>), String> {
    match settings.capital.kind.as_str() {
        "fixed" => Ok((Some(settings.capital.value), None)),
        kind => {
            let venue = account.ok_or("a share of the balance or the whole account needs the account's keys to read it")?;
            let balances = venue.balances()?;
            if kind == "percent" {
                Ok((Some(free_balance(&balances, quote) * settings.capital.value / 100.0), None))
            } else {
                let all: Holdings = balances.into_iter().filter(|b| b.free > 0.0).map(|b| (b.asset, b.free)).collect();
                Ok((None, Some(all)))
            }
        }
    }
}

/// One attempt at the run of `system` for the close at `close`.
/// `Ok(None)`: the data is not there yet and the window is still open.
pub fn attempt(app: &AppHandle<Wry>, system_id: &str, close: NaiveDate) -> Result<Option<RunRecord>, String> {
    // One trader: a PC that became a client leaves the run to the server.
    if qs_core::server::executors_allowed().is_err() {
        return Ok(None);
    }
    let state = app.state::<AppState>();
    let row = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_row(&db, system_id)?
    };
    let Some(row) = row.filter(|r| r.settings.mode != ExecMode::Off) else { return Ok(None) };
    let settings = row.settings.clone();
    let exchange_id = settings.exchange_id.clone().ok_or("no account chosen")?;
    let (provider, testnet) = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        exchange_provider(&db, &exchange_id)?
    };
    let market = settings.market_type();
    let quote = quote_for(&provider);
    let live = settings.mode == ExecMode::Live;
    let rt = parse_run_time(&settings.run_time)?;

    // The signal.
    let config = qs_mod_systems::signals::run_config(app, system_id)?;
    let rebalance_daily = portfolio_rebalances_daily(&config);
    let result = qs_mod_systems::signals::live(app, config)?;
    let signal = parse_signal(&result)?;

    // Coverage and freshness, from the venue's public data.
    let public = crate::venues::public(&provider, testnet, market)?;
    let listed: HashSet<String> = public
        .instruments()?
        .into_iter()
        .filter(|i| i.quote().eq_ignore_ascii_case(quote))
        .map(|i| i.base().to_uppercase())
        .collect();
    let is_listed = |coin: &str| listed.contains(&coin.to_uppercase());
    let closed_day = close - ChronoDuration::days(1);
    let venue_market = VenueMarket { provider: &provider, testnet, market, quote };
    let mut closes = reference_closes(&venue_market, &signal.scored, closed_day, &is_listed);
    if let (true, Some(c)) = (signal.single_asset, signal.close) {
        closes.insert(signal.scored[0].clone(), c);
    }
    let fresh = freshness(&signal, close, &closes);
    let window = window_end(close, rt, settings.retry_hours);
    if let Err(why) = &fresh {
        if Utc::now() + RETRY_EVERY < window {
            log::info!(target: "algo", "manual {system_id} {close}: waiting for data ({why})");
            return Ok(None);
        }
    }

    let (target, mut notes) = match &signal.portfolio {
        Some(weights) => {
            let (weights, mut notes) = portfolio_weights(weights, quote, &is_listed);
            let previous = {
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                exec_store::previous_run(&db, &owner(system_id), &close.to_string())?
            };
            let mut p = PortfolioTarget { weights, rebalance: true };
            let (rebalance, why) = portfolio_rebalance(rebalance_daily, previous.as_ref(), &p);
            p.rebalance = rebalance;
            notes.push(why);
            (Target::Portfolio(p), notes)
        }
        None => target_for(&signal, &is_listed, market == MarketType::Perp),
    };
    let mut reference_prices: std::collections::BTreeMap<String, f64> = closes.into_iter().collect();
    reference_prices.insert(quote.into(), 1.0);
    let account = if live { Some(live_venue(&state, &exchange_id, market, settings.leverage)?) } else { None };
    if let (true, Some(venue)) = (testnet, account.as_deref()) {
        // Test networks trade play money at prices of their own (testnet
        // ETH was 499 USDC against a 120 close on 2026-11-04): the 2 % check
        // against the data source would stop every run there.
        for (coin, price) in reference_prices.iter_mut().filter(|(c, _)| !c.eq_ignore_ascii_case(quote)) {
            if let Some(mid) = venue.instrument(&format!("{coin}/{quote}")).and_then(|i| venue.order_book(&i)).ok().and_then(|b| b.mid()) {
                *price = mid;
            }
        }
        notes.push("test network: its prices are its own, so the price check compares with the venue's mid".into());
    }
    let (initial_capital, initial_holdings) = starting_capital(&settings, account.as_deref(), quote)?;
    let mut req = RunRequest {
        run_id: run_id(system_id, close),
        owner: owner(system_id),
        exchange_id: exchange_id.clone(),
        mode: if live { Mode::Live } else { Mode::Paper },
        quote: quote.into(),
        target,
        reference_prices,
        data_fresh: fresh.is_ok(),
        armed: row.armed,
        account_belongs: true,
        close_date: Some(close.to_string()),
        signal: json!({
            "asOf": signal.as_of,
            "best": result.get("best"),
            "scores": result.get("scores"),
            "marketFilter": result.get("marketFilter"),
            "singleAssetSignal": result.get("singleAssetSignal"),
            "freshness": fresh.as_ref().err(),
            "notes": notes,
        }),
        initial_capital,
        initial_holdings,
        leverage: if market == MarketType::Perp { settings.leverage } else { 1.0 },
    };
    // Portfolio mode: the engine's weights stay with the run (Live Eval's expected holdings).
    if let (Some(weights), Some(signal)) = (result.get("portfolioTarget"), req.signal.as_object_mut()) {
        signal.insert("portfolioTarget".into(), weights.clone());
    }
    let min_liquidation_distance = state
        .settings
        .lock()
        .map(|s| s.perp_min_liquidation_distance_pct / 100.0)
        .unwrap_or(GuardConfig::default().min_liquidation_distance);
    let engine = Engine::new(GuardConfig {
        max_order_value: settings.max_order_value,
        daily_turnover_cap: settings.daily_turnover_cap,
        min_liquidation_distance,
    });
    let store = SqliteStore { conn: &state.db };
    let record = match &account {
        Some(venue) => engine.run(&store, venue.as_ref(), &req)?,
        None => {
            let paper = PaperVenue::new(
                public.as_ref(),
                &state.db,
                &req.owner,
                &exchange_id,
                settings.paper_fee_pct / 100.0,
                settings.paper_slippage_pct / 100.0,
            );
            if let Some(capital) = req.initial_capital {
                paper.fund_if_empty(quote, capital)?;
            }
            engine.run(&store, &paper, &req)?
        }
    };
    let mut record = record;
    if !notes.is_empty() {
        record.notes.extend(notes);
        store.save_run(&record)?;
    }
    Ok(Some(record))
}

/// What a portfolio holds as shares at the run's prices, largest first:
/// "BTC 50 % · ETH 30 % · USD 20 %". Below 0.5 % (dust, rounding) is left out.
pub fn shares(holdings: &Holdings, prices: &BTreeMap<String, f64>, quote: &str) -> String {
    let mut values: Vec<(String, f64)> = holdings
        .iter()
        .filter(|(_, q)| **q > 0.0)
        .filter_map(|(a, q)| {
            let cash = a.eq_ignore_ascii_case(quote);
            let price = if cash { 1.0 } else { *prices.get(a)? };
            Some((if cash { USD.to_string() } else { a.clone() }, q * price))
        })
        .collect();
    let total: f64 = values.iter().map(|(_, v)| v).sum();
    if total <= 0.0 {
        return USD.into();
    }
    values.retain(|(_, v)| v / total >= 0.005);
    values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    values.iter().map(|(a, v)| format!("{a} {:.0} %", v / total * 100.0)).collect::<Vec<_>>().join(" · ")
}

/// One line for a run: "LCES: BTC → ETH, 2 orders, fees 13.98$".
pub fn summary(name: &str, record: &RunRecord, orders: &[OrderRecord]) -> String {
    let portfolio = matches!(record.target, Target::Portfolio(_));
    let held = |h: &Option<Holdings>| -> String {
        if portfolio {
            return h.as_ref().map(|h| shares(h, &record.reference_prices, &record.quote)).unwrap_or_else(|| USD.into());
        }
        h.as_ref()
            .and_then(|h| {
                h.iter()
                    .filter(|(a, _)| !a.eq_ignore_ascii_case(&record.quote))
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(a, _)| a.clone())
            })
            .unwrap_or_else(|| USD.into())
    };
    let filled = orders.iter().filter(|o| o.filled().is_some()).count();
    match record.status {
        RunStatus::Done if filled == 0 => format!("{name}: holds {}, nothing to trade", held(&record.holdings_after)),
        RunStatus::Done => format!(
            "{name}: {} → {}, {filled} order{}, fees {:.2}$",
            held(&record.holdings_before),
            held(&record.holdings_after),
            if filled == 1 { "" } else { "s" },
            record.fees
        ),
        RunStatus::GuardStopped if record.errors.iter().any(|e| e.starts_with("data_fresh")) => format!(
            "{name}: skipped — {}. Still holding {}.",
            record.signal.get("freshness").and_then(|v| v.as_str()).unwrap_or("no fresh data"),
            held(&record.holdings_before)
        ),
        RunStatus::GuardStopped => format!("{name}: stopped by a guard — {}", record.errors.last().cloned().unwrap_or_default()),
        RunStatus::Partial => format!("{name}: partly done — {}", record.errors.last().cloned().unwrap_or_default()),
        _ => format!("{name}: error — {}", record.errors.last().cloned().unwrap_or_default()),
    }
}

fn notify(app: &AppHandle<Wry>, system_id: &str, record: &RunRecord, line: &str) {
    let _ = app.emit(
        RUN_EVENT,
        json!({ "systemId": system_id, "runId": record.id, "status": record.status, "summary": line }),
    );
    crate::logs::push_bot_log(app, if record.status == RunStatus::Done { "info" } else { "warn" }, format!("Manual · {line}"));
    toast_line(app, "QuantAlgo · Manual execution", line);
}

/// A Windows toast (§2.5): the WinRT wrapper tauri-plugin-notification uses.
#[cfg(windows)]
pub fn toast_line(app: &AppHandle<Wry>, title: &str, line: &str) {
    use tauri_winrt_notification::Toast;
    // A dev build has no registered AppUserModelID; it borrows PowerShell's
    // (the rule tauri-plugin-notification uses).
    let app_id = if cfg!(debug_assertions) { Toast::POWERSHELL_APP_ID.to_string() } else { app.config().identifier.clone() };
    if let Err(e) = Toast::new(&app_id).title(title).text1(line).show() {
        log::warn!(target: "algo", "the toast could not be shown: {e}");
    }
}

#[cfg(not(windows))]
pub fn toast_line(_app: &AppHandle<Wry>, _title: &str, _line: &str) {}

// ── the scheduler ───────────────────────────────────────────────────────────

fn wake_signal() -> &'static (Mutex<u64>, Condvar) {
    static WAKE: OnceLock<(Mutex<u64>, Condvar)> = OnceLock::new();
    WAKE.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

/// Re-plan now (settings changed).
pub fn wake() {
    let (lock, cv) = wake_signal();
    if let Ok(mut n) = lock.lock() {
        *n += 1;
        cv.notify_all();
    }
}

fn finished(conn: &Connection, run_id: &str) -> bool {
    conn.query_row("SELECT status FROM execution_runs WHERE id = ?1", params![run_id], |r| r.get::<_, String>(0))
        .optional()
        .ok()
        .flatten()
        // A partial or failed run has not done what the signal said: it goes on
        // while its window is open (RunStatus::is_final).
        .is_some_and(|s| !matches!(s.as_str(), "running" | "interrupted" | "partial" | "failed"))
}

/// The status a run ended its last attempt with.
fn run_status(conn: &Connection, run_id: &str) -> Option<String> {
    conn.query_row("SELECT status FROM execution_runs WHERE id = ?1", params![run_id], |r| r.get::<_, String>(0))
        .optional()
        .ok()
        .flatten()
}

/// Sleep until the next due time (no polling), wake on settings changes,
/// run every system whose window is open and whose run is not finished.
pub fn start(app: AppHandle<Wry>) {
    std::thread::Builder::new()
        .name("manual-execution".into())
        .spawn(move || {
            let in_flight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut retry_at: HashMap<String, DateTime<Utc>> = HashMap::new();
            loop {
                let now = Utc::now();
                let rows = {
                    let state = app.state::<AppState>();
                    let db = match state.db.lock() {
                        Ok(db) => db,
                        Err(_) => return,
                    };
                    active_rows(&db).unwrap_or_default()
                };
                let mut next = now + ChronoDuration::hours(1);
                for row in rows {
                    let Ok(rt) = parse_run_time(&row.settings.run_time) else { continue };
                    next = next.min(next_due(now, rt));
                    let Some(close) = current_close(now, rt, row.settings.retry_hours) else { continue };
                    let id = run_id(&row.system_id, close);
                    let done = {
                        let state = app.state::<AppState>();
                        let db = state.db.lock().map(|db| finished(&db, &id));
                        db.unwrap_or(true)
                    };
                    if done {
                        continue;
                    }
                    if let Some(at) = retry_at.get(&id) {
                        if *at > now {
                            next = next.min(*at);
                            continue;
                        }
                    }
                    let busy = in_flight.lock().map(|mut f| !f.insert(row.system_id.clone())).unwrap_or(true);
                    if busy {
                        continue;
                    }
                    retry_at.insert(id.clone(), now + RETRY_EVERY);
                    next = next.min(now + RETRY_EVERY);
                    let (app2, flight, system_id) = (app.clone(), in_flight.clone(), row.system_id.clone());
                    let run = id.clone();
                    std::thread::spawn(move || {
                        // A run that keeps failing tells the user once, not every five minutes.
                        let before = {
                            let state = app2.state::<AppState>();
                            let status = state.db.lock().ok().and_then(|db| run_status(&db, &run));
                            status
                        };
                        let outcome = attempt(&app2, &system_id, close);
                        if let Ok(mut f) = flight.lock() {
                            f.remove(&system_id);
                        }
                        let name = qs_mod_systems::signals::systems(&app2)
                            .into_iter()
                            .find(|(id, _)| id == &system_id)
                            .map(|(_, n)| n)
                            .unwrap_or_else(|| system_id.clone());
                        match outcome {
                            Ok(Some(record)) => {
                                let orders = {
                                    let state = app2.state::<AppState>();
                                    SqliteStore { conn: &state.db }.orders(&record.id).unwrap_or_default()
                                };
                                if before.as_deref() != Some(record.status.as_str()) {
                                    notify(&app2, &system_id, &record, &summary(&name, &record, &orders));
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                log::warn!(target: "algo", "manual {system_id} {close}: {e}");
                                crate::logs::push_bot_log(&app2, "error", format!("Manual · {name}: {e} (retried in 5 minutes)"));
                            }
                        }
                        wake();
                    });
                }
                let (lock, cv) = wake_signal();
                let Ok(guard) = lock.lock() else { return };
                let seen = *guard;
                let wait = (next - Utc::now()).to_std().unwrap_or(Duration::from_secs(1)).max(Duration::from_secs(1));
                let _ = cv.wait_timeout_while(guard, wait, |n| *n == seen);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| log::error!(target: "algo", "the manual execution scheduler did not start: {e}"));
}

// ── commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ManualView {
    pub system_id: String,
    pub settings: ManualSettings,
    pub armed: bool,
    pub armed_at: Option<String>,
    pub ack_not_robust: bool,
    /// UTC, RFC 3339; `None` while Off.
    pub next_run: Option<String>,
    pub paper_runs_done: i64,
    pub verdict: Option<Verdict>,
}

fn paper_runs_done(conn: &Connection, system_id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM execution_runs WHERE owner = ?1 AND mode = 'paper' AND status = 'done'",
        params![owner(system_id)],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

fn view(conn: &Connection, system_id: &str, app: &AppHandle<Wry>) -> Result<ManualView, String> {
    let row = load_row(conn, system_id)?;
    let portfolio = qs_mod_systems::signals::run_config(app, system_id).ok().as_ref().and_then(portfolio_key);
    let settings = row.as_ref().map(|r| r.settings.clone()).unwrap_or_default();
    let next_run = (settings.mode != ExecMode::Off)
        .then(|| parse_run_time(&settings.run_time).ok())
        .flatten()
        .map(|rt| next_due(Utc::now(), rt).to_rfc3339());
    Ok(ManualView {
        system_id: system_id.to_string(),
        armed: row.as_ref().is_some_and(|r| r.armed),
        armed_at: row.as_ref().and_then(|r| r.armed_at.clone()),
        ack_not_robust: row.as_ref().is_some_and(|r| r.ack_not_robust),
        next_run,
        paper_runs_done: paper_runs_done(conn, system_id),
        verdict: robustness_verdict(&research_dir(), system_id, portfolio.as_ref()),
        settings,
    })
}

#[tauri::command(async, rename_all = "snake_case")]
pub(crate) fn manual_execution_get(system_id: String, state: State<'_, AppState>, app: AppHandle<Wry>) -> Result<ManualView, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    view(&db, &system_id, &app)
}

#[tauri::command(async, rename_all = "snake_case")]
pub(crate) fn manual_execution_save(
    system_id: String,
    settings: ManualSettings,
    state: State<'_, AppState>,
    app: AppHandle<Wry>,
) -> Result<ManualView, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    if let Some(id) = settings.exchange_id.as_deref().filter(|id| !id.is_empty()) {
        let (provider, _) = exchange_provider(&db, id)?;
        if !crate::venues::markets(&provider).contains(&settings.market_type()) {
            return Err(format!("This {provider} account cannot trade {}; perp is on Hyperliquid accounts.", settings.market));
        }
    }
    let cap = state.settings.lock().map(|s| s.perp_max_leverage).unwrap_or(crate::settings::DEFAULT_PERP_MAX_LEVERAGE);
    check_leverage(settings.leverage, cap)?;
    save_settings(&db, &system_id, &settings)?;
    let out = view(&db, &system_id, &app);
    drop(db);
    wake();
    out
}

#[derive(Debug, Serialize)]
pub struct ArmCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct ArmResult {
    pub armed: bool,
    pub checks: Vec<ArmCheck>,
}

/// The arming rule (§2.3): a completed paper run, a working account and the
/// robustness verdict — or the user's own "I know this system is not
/// robust". Pure, so it is tested without a venue.
pub fn arming_checks(
    settings: &ManualSettings,
    paper_runs: i64,
    account: Result<String, String>,
    verdict: Option<&Verdict>,
    ack_not_robust: bool,
    portfolio: bool,
) -> Vec<ArmCheck> {
    let check = |name: &str, ok: bool, detail: String| ArmCheck { name: name.into(), ok, detail };
    // Portfolio mode trades on spot (card 791a9140). Listed only when it
    // refuses, so other systems' check lists stay as they were.
    let refused = (portfolio && settings.market_type() == MarketType::Perp)
        .then(|| check("portfolio", false, format!("cannot arm: {}", crate::execution::engine::PORTFOLIO_SPOT_ONLY)));
    refused.into_iter().chain([
        check("mode", settings.mode == ExecMode::Live, if settings.mode == ExecMode::Live { "Live".into() } else { "set the mode to Live first".into() }),
        check(
            "paper_run",
            paper_runs >= 1,
            if paper_runs >= 1 { format!("{paper_runs} completed paper run(s)") } else { "no completed paper run yet".into() },
        ),
        match account {
            Ok(detail) => check("account", true, detail),
            Err(e) => check("account", false, e),
        },
        match verdict {
            Some(v) if v.robust => check("robustness", true, format!("robust ({})", v.finished_at.get(..10).unwrap_or(""))),
            Some(v) if ack_not_robust => check("robustness", true, format!("{} — armed with \"I know this system is not robust\"", v.status)),
            Some(v) => check("robustness", false, format!("the System Gauntlet says {}; tick \"I know this system is not robust\" to arm anyway", v.status)),
            None if ack_not_robust => check("robustness", true, "no verdict — armed with \"I know this system is not robust\"".into()),
            None => check("robustness", false, "no System Gauntlet verdict yet; run it, or tick \"I know this system is not robust\"".into()),
        },
    ])
    .collect()
}

/// Arm Live. `external`: it lets the system trade real money from the next run.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn manual_execution_arm(
    system_id: String,
    ack_not_robust: Option<bool>,
    app: AppHandle<Wry>,
) -> Result<ArmResult, String> {
    let ack = ack_not_robust.unwrap_or(false);
    // Armed systems run where the data lives: never on a connected client.
    qs_core::server::executors_allowed()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (row, paper_runs) = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            (load_row(&db, &system_id)?.ok_or("Save the Execution settings first.")?, paper_runs_done(&db, &system_id))
        };
        let exchange_id = row.settings.exchange_id.clone().unwrap_or_default();
        let account = live_venue(&state, &exchange_id, row.settings.market_type(), row.settings.leverage).and_then(|venue| {
            venue.ready()?;
            let balances = venue.balances()?;
            let quote = quote_for(&venue.label());
            // A Solana wallet must hold no more than its cap (PLAN-EXECUTION-VENUES §8).
            let creds = {
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                crate::exchanges::load_credentials(&db, &exchange_id)?
            };
            let cap = crate::venues::wallet_cap_check(&creds)?.map(|c| format!(", {c}")).unwrap_or_default();
            Ok(format!("{}: {:.2} {quote} free{cap}", venue.label(), free_balance(&balances, quote)))
        });
        // A portfolio system's verdict is its own portfolio config's (card 791a9140).
        let config = qs_mod_systems::signals::run_config(&app, &system_id).ok();
        let key = config.as_ref().and_then(portfolio_key);
        let verdict = robustness_verdict(&research_dir(), &system_id, key.as_ref());
        // An unreadable config does not block here: a portfolio run on perp refuses as well.
        let portfolio = key.is_some();
        let checks = arming_checks(&row.settings, paper_runs, account, verdict.as_ref(), ack, portfolio);
        let armed = checks.iter().all(|c| c.ok);
        if armed {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            set_armed(&db, &system_id, true, ack)?;
        }
        wake();
        Ok(ArmResult { armed, checks })
    })
    .await
    .map_err(|e| format!("Arming failed: {e}"))?
}

#[tauri::command(async, rename_all = "snake_case")]
pub(crate) fn manual_execution_disarm(system_id: String, state: State<'_, AppState>, app: AppHandle<Wry>) -> Result<ManualView, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    set_armed(&db, &system_id, false, false)?;
    view(&db, &system_id, &app)
}

#[derive(Debug, Serialize)]
pub struct RunView {
    pub run: RunRecord,
    pub orders: Vec<OrderRecord>,
    pub summary: String,
}

/// A system's runs, newest first, with their orders. `read`.
#[tauri::command(async, rename_all = "snake_case")]
pub(crate) fn manual_execution_runs(
    system_id: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<RunView>, String> {
    let ids: Vec<String> = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        let mut stmt = db
            .prepare("SELECT id FROM execution_runs WHERE owner = ?1 ORDER BY started_at DESC LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![owner(&system_id), limit.unwrap_or(60).min(500)], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
    };
    let store = SqliteStore { conn: &state.db };
    ids.into_iter()
        .filter_map(|id| store.load_run(&id).ok().flatten())
        .map(|run| {
            let orders = store.orders(&run.id).unwrap_or_default();
            let summary = summary(&system_id, &run, &orders);
            Ok(RunView { run, orders, summary })
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub venue: String,
    pub as_of: String,
    pub coins: Vec<String>,
    pub tradable: Vec<String>,
    pub missing: Vec<String>,
}

/// Which coins of today's universe the account's venue can trade (§2.3).
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn manual_execution_coverage(system_id: String, exchange_id: String, app: AppHandle<Wry>) -> Result<Coverage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (provider, testnet, market) = {
            let state = app.state::<AppState>();
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            let (provider, testnet) = exchange_provider(&db, &exchange_id)?;
            let market = load_row(&db, &system_id)?.map(|r| r.settings.market_type()).unwrap_or(MarketType::Spot);
            (provider, testnet, market)
        };
        let config = qs_mod_systems::signals::run_config(&app, &system_id)?;
        let today = Utc::now().date_naive().to_string();
        let coins: Vec<String> = if config.get("mode").and_then(|v| v.as_str()) == Some("single_asset") {
            let pair = config.pointer("/singleAsset/pair").and_then(|v| v.as_str()).unwrap_or("");
            pair.split('/').next().filter(|b| !b.is_empty()).map(|b| vec![b.to_string()]).unwrap_or_default()
        } else {
            let universe = qs_mod_systems::signals::universe(&app, &today, config)?;
            universe
                .get("coins")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|c| c.get("symbol").and_then(|s| s.as_str())).map(String::from).collect())
                .unwrap_or_default()
        };
        let venue = crate::venues::public(&provider, testnet, market)?;
        let quote = quote_for(&provider);
        let listed: HashSet<String> = venue
            .instruments()?
            .into_iter()
            .filter(|i| i.quote().eq_ignore_ascii_case(quote))
            .map(|i| i.base().to_uppercase())
            .collect();
        let (tradable, missing): (Vec<String>, Vec<String>) = coins.iter().cloned().partition(|c| listed.contains(&c.to_uppercase()));
        Ok(Coverage { venue: venue.label(), as_of: today, coins, tradable, missing })
    })
    .await
    .map_err(|e| format!("Coverage failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn the_run_is_due_after_the_close_and_waits_inside_its_window() {
        let rt = parse_run_time("00:15").unwrap();
        assert_eq!(due_at(d("2026-11-10"), rt), t("2026-11-10T00:15:00Z"));
        assert_eq!(window_end(d("2026-11-10"), rt, 6), t("2026-11-10T06:00:00Z"));
        assert_eq!(current_close(t("2026-11-10T00:14:59Z"), rt, 6), None, "not before the due time");
        assert_eq!(current_close(t("2026-11-10T00:15:00Z"), rt, 6), Some(d("2026-11-10")));
        assert_eq!(current_close(t("2026-11-10T05:59:00Z"), rt, 6), Some(d("2026-11-10")), "a late start still runs once");
        assert_eq!(current_close(t("2026-11-10T06:00:00Z"), rt, 6), None, "after the window the day is skipped");
        assert_eq!(next_due(t("2026-11-10T00:10:00Z"), rt), t("2026-11-10T00:15:00Z"));
        assert_eq!(next_due(t("2026-11-10T00:15:00Z"), rt), t("2026-11-11T00:15:00Z"));
        // A run time later than the retry hours still gets half an hour.
        let late = parse_run_time("07:00").unwrap();
        assert_eq!(window_end(d("2026-11-10"), late, 6), t("2026-11-10T07:30:00Z"));
        assert!(parse_run_time("25:00").is_err());
        assert_eq!(run_id("lces", d("2026-11-10")), "manual-lces-2026-11-10");
    }

    fn rotation(best: &str) -> Value {
        json!({"asOf": "2026-11-10", "best": best, "symbols": ["BTC", "ETH", "SOL", "USD"], "scores": {}})
    }

    #[test]
    fn the_signal_becomes_a_target() {
        let s = parse_signal(&rotation("ETH")).unwrap();
        assert_eq!(s.hold.as_deref(), Some("ETH"));
        assert_eq!(s.scored, vec!["BTC", "ETH", "SOL"]);
        let all = |_: &str| true;
        assert_eq!(target_for(&s, &all, false).0, Target::Hold("ETH".into()));
        assert_eq!(target_for(&parse_signal(&rotation("USD")).unwrap(), &all, false).0, Target::Flat);
        // Policy hold_usd: a pick the venue cannot trade is held as USD, flagged.
        let no_eth = |c: &str| c != "ETH";
        let (target, notes) = target_for(&s, &no_eth, false);
        assert_eq!(target, Target::Flat);
        assert!(notes[0].contains("backtest ≠ live"), "{notes:?}");
        // Single asset: long holds the base, short on spot holds cash.
        let single = |position: &str| {
            json!({"mode": "single_asset", "asOf": "2026-11-10", "symbols": ["BTC", "USDT"],
                   "singleAssetSignal": {"position": position, "close": 100000.0, "closedAt": "2026-11-10T00:00:00Z"}})
        };
        let s = parse_signal(&single("long")).unwrap();
        assert_eq!(target_for(&s, &all, false).0, Target::Hold("BTC".into()));
        let s = parse_signal(&single("short")).unwrap();
        let (target, notes) = target_for(&s, &all, false);
        assert_eq!(target, Target::Flat);
        assert!(notes[0].contains("long-only"));
        // On perp the same signal is a short (Venues 4/5).
        assert_eq!(target_for(&s, &all, true).0, Target::Short("BTC".into()));
        let none = |_: &str| false;
        assert_eq!(target_for(&s, &none, true).0, Target::Flat, "no perp for the coin: cash");
        assert!(parse_signal(&json!({"asOf": "2026-11-10", "best": null, "symbols": []})).is_err(), "no ranking");
    }

    #[test]
    fn leverage_stays_under_the_cap_and_off_spot() {
        assert!(check_leverage(1.0, 3.0).is_ok());
        assert!(check_leverage(3.0, 3.0).is_ok());
        assert!(check_leverage(4.0, 3.0).unwrap_err().contains("cap of 3x"));
        assert!(check_leverage(5.0, 5.0).is_ok(), "the user raised the cap");
        assert!(check_leverage(1.5, 3.0).is_err());
        let s = ManualSettings::default();
        assert_eq!(s.leverage, 1.0, "1x is the default");
        assert!(ManualSettings { leverage: 2.0, ..s.clone() }.validate().is_err(), "spot is always 1x");
        assert!(ManualSettings { market: "perp".into(), leverage: 2.0, ..s.clone() }.validate().is_ok());
        assert!(ManualSettings { market: "perp".into(), leverage: 0.5, ..s }.validate().is_err());
    }

    #[test]
    fn a_run_never_trades_on_old_data() {
        let s = parse_signal(&rotation("ETH")).unwrap();
        let mut closes: HashMap<String, f64> = [("BTC".to_string(), 1.0), ("ETH".to_string(), 1.0)].into_iter().collect();
        let err = freshness(&s, d("2026-11-10"), &closes).unwrap_err();
        assert!(err.contains("2026-11-09") && err.contains("SOL"), "{err}");
        closes.insert("SOL".into(), 1.0);
        assert!(freshness(&s, d("2026-11-10"), &closes).is_ok());
        assert!(freshness(&s, d("2026-11-11"), &closes).unwrap_err().contains("as of 2026-11-10"), "yesterday's signal");
        let single = parse_signal(&json!({"mode": "single_asset", "asOf": "2026-11-10", "symbols": ["BTC", "USDT"],
            "singleAssetSignal": {"position": "long", "close": 1.0, "closedAt": "2026-11-09T00:00:00Z"}})).unwrap();
        assert!(freshness(&single, d("2026-11-10"), &HashMap::new()).is_err(), "the bar closed a day early");
        let single = parse_signal(&json!({"mode": "single_asset", "asOf": "2026-11-10", "symbols": ["BTC", "USDT"],
            "singleAssetSignal": {"position": "long", "close": 1.0, "closedAt": "2026-11-10T00:00:00+00:00Z"}})).unwrap();
        assert!(freshness(&single, d("2026-11-10"), &HashMap::new()).is_ok());
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn settings_are_checked_and_a_change_disarms_live() {
        let conn = db();
        let mut s = ManualSettings::default();
        save_settings(&conn, "lces", &s).unwrap();
        assert!(active_rows(&conn).unwrap().is_empty(), "Off is not scheduled");
        s.mode = ExecMode::Live;
        assert!(save_settings(&conn, "lces", &s).unwrap_err().contains("account"));
        s.exchange_id = Some("x1".into());
        save_settings(&conn, "lces", &s).unwrap();
        set_armed(&conn, "lces", true, true).unwrap();
        // Same arming fields: still armed.
        s.run_time = "00:30".into();
        assert!(save_settings(&conn, "lces", &s).unwrap().armed);
        // A new capital disarms.
        s.capital.value = 2_000.0;
        let row = save_settings(&conn, "lces", &s).unwrap();
        assert!(!row.armed && !row.ack_not_robust);
        assert_eq!(active_rows(&conn).unwrap().len(), 1);
        for bad in [
            ManualSettings { run_time: "24:61".into(), ..s.clone() },
            ManualSettings { retry_hours: 0, ..s.clone() },
            ManualSettings { capital: Capital { kind: "percent".into(), value: 150.0 }, ..s.clone() },
            ManualSettings { market: "futures".into(), ..s.clone() },
            ManualSettings { coverage_policy: "sometimes".into(), ..s.clone() },
            ManualSettings { restricted_symbols: vec!["SOL/USDT".into()], ..s.clone() },
        ] {
            assert!(bad.validate().is_err());
        }
        set_armed(&conn, "lces", true, false).unwrap();
        assert_eq!(disarm_all(&conn).unwrap(), 1);
        assert!(!load_row(&conn, "lces").unwrap().unwrap().armed);
    }

    #[test]
    fn restricted_coins_are_listed_per_system_for_the_backtest() {
        let conn = db();
        let mut s = ManualSettings { coverage_policy: "restrict".into(), restricted_symbols: vec!["hype".into(), "LEO".into(), "HYPE".into()], ..ManualSettings::default() };
        save_settings(&conn, "lces", &s).unwrap();
        s.coverage_policy = "hold_usd".into();
        save_settings(&conn, "sces", &s).unwrap();
        let map = restrictions(&conn).unwrap();
        assert_eq!(map.len(), 1, "only the restrict policy restricts");
        assert_eq!(map["lces"], vec!["HYPE", "LEO"]);
    }

    #[test]
    fn exactly_one_run_per_close_date() {
        let conn = db();
        assert!(!finished(&conn, "manual-lces-2026-11-10"));
        let rec_json = |status: &str| {
            conn.execute(
                "INSERT INTO execution_runs (id, owner, exchange_id, mode, close_date, status, started_at, record_json)
                 VALUES ('manual-lces-2026-11-10', 'system:lces', 'x', 'paper', '2026-11-10', ?1, 't', '{}')
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status",
                params![status],
            )
            .unwrap();
        };
        rec_json("interrupted");
        assert!(!finished(&conn, "manual-lces-2026-11-10"), "an interrupted run resumes");
        rec_json("done");
        assert!(finished(&conn, "manual-lces-2026-11-10"));
        // The unique index refuses a second run of the same system and close date.
        let second = conn.execute(
            "INSERT INTO execution_runs (id, owner, exchange_id, mode, close_date, status, started_at, record_json)
             VALUES ('other-id', 'system:lces', 'x', 'paper', '2026-11-10', 'done', 't', '{}')",
            [],
        );
        assert!(second.is_err());
    }

    #[test]
    fn arming_needs_a_paper_run_an_account_and_a_verdict_or_the_tick() {
        let live = ManualSettings { mode: ExecMode::Live, exchange_id: Some("x".into()), ..ManualSettings::default() };
        let robust = Verdict { run_id: "r".into(), status: "robust".into(), robust: true, finished_at: "2026-11-04".into() };
        let fragile = Verdict { status: "not_robust".into(), robust: false, ..robust.clone() };
        let ok = |checks: &[ArmCheck]| checks.iter().all(|c| c.ok);
        assert!(ok(&arming_checks(&live, 1, Ok("ok".into()), Some(&robust), false, false)));
        assert!(!ok(&arming_checks(&live, 0, Ok("ok".into()), Some(&robust), false, false)), "no paper run");
        assert!(!ok(&arming_checks(&live, 1, Err("Bybit: Your api key has expired.".into()), Some(&robust), false, false)));
        assert!(!ok(&arming_checks(&live, 1, Ok("ok".into()), Some(&fragile), false, false)));
        assert!(ok(&arming_checks(&live, 1, Ok("ok".into()), Some(&fragile), true, false)), "the user's own tick");
        assert!(!ok(&arming_checks(&live, 1, Ok("ok".into()), None, false, false)));
        assert!(!ok(&arming_checks(&ManualSettings::default(), 1, Ok("ok".into()), Some(&robust), false, false)), "mode is not Live");
        // Portfolio mode arms on spot (card 791a9140) with no extra line; on perp it never arms,
        // not even with the "not robust" tick.
        let portfolio_spot = arming_checks(&live, 1, Ok("ok".into()), Some(&robust), false, true);
        assert!(ok(&portfolio_spot));
        assert!(!portfolio_spot.iter().any(|c| c.name == "portfolio"));
        let perp = ManualSettings { market: "perp".into(), ..live.clone() };
        let refused = arming_checks(&perp, 1, Ok("ok".into()), Some(&robust), true, true);
        assert!(!ok(&refused));
        assert!(refused.iter().any(|c| c.name == "portfolio" && !c.ok && c.detail.contains("spot only")), "{:?}", refused.iter().map(|c| &c.detail).collect::<Vec<_>>());
        assert!(ok(&arming_checks(&perp, 1, Ok("ok".into()), Some(&robust), false, false)), "single pick on perp as before");
        let plain = arming_checks(&live, 1, Ok("ok".into()), Some(&robust), false, false);
        assert_eq!(plain.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["mode", "paper_run", "account", "robustness"]);
    }

    #[test]
    fn portfolio_signals_carry_their_weights() {
        assert!(portfolio_on(&json!({"mode": "rotation", "portfolio": {"enabled": true}})));
        assert!(portfolio_on(&json!({"portfolio": {"enabled": true, "positions": 3}})), "rotation is the default mode");
        assert!(!portfolio_on(&json!({"portfolio": {"enabled": false}})));
        assert!(!portfolio_on(&json!({"mode": "single_asset", "portfolio": {"enabled": true}})), "the engine ignores it there");
        assert!(!portfolio_on(&json!({"mode": "rotation"})));
        let live = json!({"asOf": "2026-11-10", "best": "ETH", "symbols": ["ETH", "BTC", "USD"],
                          "portfolioTarget": {"ETH": 0.5, "BTC": 0.25, "USD": 0.25}});
        let s = parse_signal(&live).unwrap();
        assert_eq!(s.portfolio.unwrap(), [("BTC".to_string(), 0.25), ("ETH".into(), 0.5), ("USD".into(), 0.25)].into_iter().collect());
        assert_eq!(s.hold.as_deref(), Some("ETH"), "best keeps its single-pick meaning");
        assert!(parse_signal(&json!({"asOf": "2026-11-10", "symbols": ["ETH"], "portfolioTarget": {"ETH": 0.5}})).unwrap_err().contains("add up"));
        assert!(parse_signal(&json!({"asOf": "2026-11-10", "symbols": ["ETH"], "portfolioTarget": {"ETH": -1.0, "USD": 2.0}})).is_err());
        assert!(parse_signal(&json!({"asOf": "2026-11-10", "symbols": ["ETH"], "portfolioTarget": ["ETH"]})).is_err());
        assert!(parse_signal(&json!({"asOf": "2026-11-10", "symbols": ["ETH"], "best": "ETH"})).unwrap().portfolio.is_none());
    }

    #[test]
    fn portfolio_weights_trade_in_the_accounts_quote_and_untradable_shares_hold_it() {
        let w: BTreeMap<String, f64> = [("BTC", 0.4), ("HYPE", 0.2), ("USD", 0.4)].into_iter().map(|(a, w)| (a.to_string(), w)).collect();
        let (out, notes) = portfolio_weights(&w, "USDT", &|c: &str| c != "HYPE");
        assert_eq!(out.len(), 2);
        assert!((out["BTC"] - 0.4).abs() < 1e-12 && (out["USDT"] - 0.6).abs() < 1e-12, "{out:?}");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("HYPE") && notes[0].contains("backtest ≠ live"), "{notes:?}");
        let (out, notes) = portfolio_weights(&w, "USDC", &|_| true);
        assert_eq!(out.keys().cloned().collect::<Vec<_>>(), ["BTC", "HYPE", "USDC"]);
        assert!(notes.is_empty());
    }

    #[test]
    fn a_portfolio_trades_when_its_assets_change_or_every_day_and_repairs_an_unfinished_run() {
        let target = |assets: &[(&str, f64)]| PortfolioTarget { weights: assets.iter().map(|(a, w)| (a.to_string(), *w)).collect(), rebalance: true };
        let run = |status: &str, t: Value| -> RunRecord {
            serde_json::from_value(json!({"id": "manual-p-2026-11-09", "owner": "system:p", "exchange_id": "x", "mode": "paper",
                "close_date": "2026-11-09", "status": status, "target": t, "quote": "USDT", "started_at": "t"})).unwrap()
        };
        let today = target(&[("BTC", 0.5), ("ETH", 0.3), ("USDT", 0.2)]);
        // The same assets with other weights: the weights drift, nothing trades.
        let same = run("done", json!({"kind": "portfolio", "asset": {"weights": {"BTC": 0.4, "ETH": 0.4, "USDT": 0.2}, "rebalance": true}}));
        let (go, why) = portfolio_rebalance(false, Some(&same), &today);
        assert!(!go, "{why}");
        assert!(why.contains("drift"), "{why}");
        // Every day under the daily rule.
        assert!(portfolio_rebalance(true, Some(&same), &today).0);
        // A coin joins or leaves, or the cash share goes: trade.
        let other = run("done", json!({"kind": "portfolio", "asset": {"weights": {"BTC": 0.5, "SOL": 0.3, "USDT": 0.2}, "rebalance": false}}));
        let (go, why) = portfolio_rebalance(false, Some(&other), &today);
        assert!(go && why.contains("SOL"), "{why}");
        assert!(portfolio_rebalance(false, Some(&same), &target(&[("BTC", 0.6), ("ETH", 0.4)])).0);
        // The first run, and the run after the single pick or USD.
        assert!(portfolio_rebalance(false, None, &today).0);
        assert!(portfolio_rebalance(false, Some(&run("done", json!({"kind": "hold", "asset": "BTC"}))), &today).0);
        assert!(!portfolio_rebalance(false, Some(&run("done", json!({"kind": "flat"}))), &target(&[("USDT", 1.0)])).0, "all cash stays all cash");
        // A run that did not finish leaves the account off its target: trade back.
        for status in ["partial", "failed", "guard_stopped", "interrupted"] {
            let (go, why) = portfolio_rebalance(false, Some(&run(status, json!({"kind": "portfolio", "asset": {"weights": {"BTC": 0.5, "ETH": 0.3, "USDT": 0.2}, "rebalance": true}}))), &today);
            assert!(go && why.contains(status), "{status}: {why}");
        }
    }

    #[test]
    fn the_portfolio_block_decides_which_verdict_counts() {
        let on = json!({"mode": "rotation", "portfolio": {"enabled": true, "positions": 2, "weighting": "equal", "notTrending": "withUsd", "rebalance": "onChange"}});
        let snake = json!({"portfolio": {"enabled": true, "positions": 2, "not_trending": "with_usd", "rebalance": "on_change"}});
        assert_eq!(portfolio_key(&on), portfolio_key(&snake), "defaults and spellings match");
        assert!(portfolio_key(&json!({"portfolio": {"enabled": true, "positions": 3}})) != portfolio_key(&on));
        assert!(portfolio_key(&json!({"portfolio": {"enabled": false, "positions": 2}})).is_none());
        assert!(!portfolio_rebalances_daily(&on));
        assert!(portfolio_rebalances_daily(&json!({"portfolio": {"enabled": true, "rebalance": "daily"}})));
    }

    #[test]
    fn the_newest_verdict_of_the_system_counts() {
        let dir = std::env::temp_dir().join(format!("qs-manual-verdict-{}", uuid::Uuid::new_v4()));
        let write_with = |id: &str, system: &str, finished: &str, verdict: Value, file: &str, config: Value| {
            let run = dir.join(id);
            std::fs::create_dir_all(&run).unwrap();
            std::fs::write(
                run.join("run.json"),
                json!({"id": id, "kind": "robustness", "status": "done", "system": {"id": system}, "finished_at": finished, "config": config}).to_string(),
            )
            .unwrap();
            let body = if file == "verdict.json" { verdict } else { json!({ "verdict": verdict }) };
            std::fs::write(run.join(file), body.to_string()).unwrap();
        };
        let single = json!({"mode": "rotation", "topN": 5});
        let write = |id: &str, system: &str, finished: &str, verdict: Value, file: &str| write_with(id, system, finished, verdict, file, single.clone());
        write("a", "lces", "2026-11-01T00:00:00Z", json!({"status": "robust", "robust": true}), "verdict.json");
        write("b", "lces", "2026-11-03T00:00:00Z", json!({"status": "not_robust", "robust": false}), "results.json");
        write("c", "other", "2026-11-04T00:00:00Z", json!({"status": "robust", "robust": true}), "verdict.json");
        let v = robustness_verdict(&dir, "lces", None).unwrap();
        assert_eq!((v.run_id.as_str(), v.robust), ("b", false));
        assert!(robustness_verdict(&dir, "none", None).is_none());

        // Portfolio mode (card 791a9140): only a Gauntlet of the same portfolio block counts, both ways.
        let portfolio = json!({"mode": "rotation", "portfolio": {"enabled": true, "positions": 2}});
        let key = portfolio_key(&portfolio);
        assert!(robustness_verdict(&dir, "lces", key.as_ref()).is_none(), "the single pick's verdict is not the portfolio's");
        write_with("d", "lces", "2026-11-05T00:00:00Z", json!({"status": "robust", "robust": true}), "verdict.json", portfolio.clone());
        assert_eq!(robustness_verdict(&dir, "lces", key.as_ref()).unwrap().run_id, "d");
        assert_eq!(robustness_verdict(&dir, "lces", None).unwrap().run_id, "b", "and the portfolio's is not the single pick's");
        let three = portfolio_key(&json!({"portfolio": {"enabled": true, "positions": 3}}));
        assert!(robustness_verdict(&dir, "lces", three.as_ref()).is_none(), "another number of positions is another system");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_reads_as_one_line() {
        let mut rec: RunRecord = serde_json::from_value(json!({
            "id": "manual-lces-2026-11-10", "owner": "system:lces", "exchange_id": "x", "mode": "paper",
            "status": "done", "target": {"kind": "hold", "asset": "ETH"}, "quote": "USDT",
            "holdings_before": {"BTC": 0.1}, "holdings_after": {"ETH": 2.49, "USDT": 3.0},
            "fees": 13.98, "started_at": "t"
        }))
        .unwrap();
        assert_eq!(summary("LCES", &rec, &[]), "LCES: holds ETH, nothing to trade");
        rec.status = RunStatus::GuardStopped;
        rec.errors = vec!["data_fresh: the signal is not from the candle that just closed".into()];
        rec.signal = json!({"freshness": "no 2026-11-09 candle yet for SOL"});
        assert_eq!(summary("LCES", &rec, &[]), "LCES: skipped — no 2026-11-09 candle yet for SOL. Still holding BTC.");
    }

    #[test]
    fn a_portfolio_run_reads_as_its_shares() {
        let rec: RunRecord = serde_json::from_value(json!({
            "id": "manual-p-2026-11-10", "owner": "system:p", "exchange_id": "x", "mode": "paper", "status": "done",
            "target": {"kind": "portfolio", "asset": {"weights": {"BTC": 0.5, "ETH": 0.3, "USDT": 0.2}, "rebalance": false}},
            "quote": "USDT", "reference_prices": {"BTC": 100000.0, "ETH": 4000.0, "USDT": 1.0},
            "holdings_before": {"BTC": 0.05, "ETH": 0.75, "USDT": 2000.0, "SOL": 0.001},
            "holdings_after": {"BTC": 0.05, "ETH": 0.75, "USDT": 2000.0, "SOL": 0.001},
            "started_at": "t"
        }))
        .unwrap();
        // SOL has no price in the run (dust from earlier) and stays out.
        assert_eq!(summary("P", &rec, &[]), "P: holds BTC 50 % · ETH 30 % · USD 20 %, nothing to trade");
        assert_eq!(shares(&Holdings::new(), &rec.reference_prices, "USDT"), "USD");
    }
}
