//! Funds: one amount split across several Manual systems. Saved with the systems' own store; the calculation is the
//! engine's (`rotation_lab/funds.py`). No orders: a fund plans and evaluates.

use std::time::Duration;

use serde_json::{json, Map, Value};
use tauri::State;

use crate::{with_restriction, EngineManager, Store};

pub(crate) const METHODS: [&str; 9] = [
    "equal", "fixed", "profit", "sharpe", "sortino", "return_drawdown", "inverse_volatility", "kelly", "combined",
];
const REBALANCES: [&str; 4] = ["never", "weekly", "monthly", "quarterly"];
const LOOKBACKS: [i64; 4] = [30, 90, 180, 365];
const KELLY_FRACTIONS: [f64; 3] = [0.25, 0.5, 1.0];
/// One backtest per member at most; a member whose settings did not change answers from the engine's memory.
const FUND_TIMEOUT: Duration = Duration::from_secs(4 * 3600);

fn text_of<'a>(fund: &'a Value, key: &str) -> Option<&'a str> {
    fund.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// A fund as the store keeps it, or what is wrong with it. `known` says whether a system exists.
pub(crate) fn validate(fund: &Value, known: impl Fn(&str) -> bool, new_id: String, today: String) -> Result<Value, String> {
    let name = text_of(fund, "name").ok_or("A fund needs a name.")?;
    if name.chars().count() > 80 {
        return Err("A fund's name can have at most 80 characters.".into());
    }
    let amount = fund.get("amount").and_then(Value::as_f64).filter(|a| a.is_finite() && *a > 0.0).ok_or("A fund needs an amount above 0.")?;
    let members: Vec<String> = fund
        .get("members")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default();
    if members.is_empty() {
        return Err("A fund needs at least one system.".into());
    }
    let mut seen = std::collections::HashSet::new();
    for id in &members {
        if !seen.insert(id.as_str()) {
            return Err("A system can only be in a fund once.".into());
        }
        if !known(id) {
            return Err(format!("Unknown system {id}."));
        }
    }
    let method = text_of(fund, "method").unwrap_or("equal");
    if !METHODS.contains(&method) {
        return Err(format!("Unknown method {method}."));
    }
    let lookback = fund.get("lookbackDays").and_then(Value::as_i64).unwrap_or(180);
    if !LOOKBACKS.contains(&lookback) {
        return Err("The lookback must be 30, 90, 180 or 365 days.".into());
    }
    let rebalance = text_of(fund, "rebalance").unwrap_or("monthly");
    if !REBALANCES.contains(&rebalance) {
        return Err("Rebalance must be never, weekly, monthly or quarterly.".into());
    }
    let cap = fund.get("capPct").and_then(Value::as_f64).unwrap_or(50.0);
    if !cap.is_finite() || !(1.0..=100.0).contains(&cap) {
        return Err("The cap must be between 1% and 100%.".into());
    }
    let kelly = fund.get("kellyFraction").and_then(Value::as_f64).unwrap_or(0.25);
    if !KELLY_FRACTIONS.contains(&kelly) {
        return Err("The Kelly fraction must be 1/4, 1/2 or full.".into());
    }
    let mut fixed = Map::new();
    let mut fixed_total = 0.0;
    if let Some(shares) = fund.get("fixedPct").and_then(Value::as_object) {
        for (id, share) in shares.iter().filter(|(id, _)| members.iter().any(|m| m == *id)) {
            let share = share.as_f64().filter(|s| s.is_finite() && *s >= 0.0).ok_or("A fixed share must be 0% or more.")?;
            fixed_total += share;
            fixed.insert(id.clone(), json!(share));
        }
    }
    if fixed_total > 100.000001 {
        return Err("The fixed shares add up to more than 100%.".into());
    }
    if method == "fixed" && fixed_total <= 0.0 {
        return Err("Fixed % needs a share for at least one system.".into());
    }
    let live_start = text_of(fund, "liveStartDate").map(String::from).unwrap_or(today);
    if chrono::NaiveDate::parse_from_str(&live_start, "%Y-%m-%d").is_err() {
        return Err("The live start must be a date (YYYY-MM-DD).".into());
    }
    let id = text_of(fund, "id").map(String::from).unwrap_or(new_id);
    Ok(json!({
        "id": id,
        "name": name,
        "amount": amount,
        "members": members,
        "method": method,
        "lookbackDays": lookback,
        "rebalance": rebalance,
        "capPct": cap,
        "kellyFraction": kelly,
        "fixedPct": fixed,
        "liveStartDate": live_start,
    }))
}

#[tauri::command(async)]
pub(crate) fn list_funds(store: State<'_, Store>) -> Result<Vec<Value>, String> {
    store.data.lock().map(|s| s.funds.clone()).map_err(|e| format!("Lock funds: {e}"))
}

#[tauri::command(async)]
pub(crate) fn save_fund(fund: Value, store: State<'_, Store>) -> Result<Value, String> {
    store.update(|data| {
        let saved = validate(
            &fund,
            |id| data.systems.iter().any(|s| s.id == id),
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now().format("%Y-%m-%d").to_string(),
        )?;
        match data.funds.iter_mut().find(|f| f.get("id") == saved.get("id")) {
            Some(existing) => *existing = saved.clone(),
            None => data.funds.push(saved.clone()),
        }
        Ok(saved)
    })
}

#[tauri::command(async)]
pub(crate) fn delete_fund(fund_id: String, store: State<'_, Store>) -> Result<(), String> {
    store.update(|data| {
        data.funds.retain(|f| f.get("id").and_then(Value::as_str) != Some(fund_id.as_str()));
        Ok(())
    })
}

/// What the engine needs for one run: each member's name and settings, and the systems deleted since.
fn run_params(fund: &Value, data: &crate::PersistedStore, live: bool) -> Value {
    let mut members = Vec::new();
    let mut removed = Vec::new();
    for id in fund.get("members").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        match data.systems.iter().find(|s| s.id == id) {
            Some(system) => {
                let config = data.system_configs.get(id).cloned().unwrap_or_else(|| crate::default_run_config(id));
                members.push(json!({ "id": id, "name": system.name, "config": with_restriction(id, config) }));
            }
            None => removed.push(id.to_string()),
        }
    }
    json!({
        "members": members,
        "removed": removed,
        "amount": fund.get("amount"),
        "live": live,
        "liveStartDate": fund.get("liveStartDate"),
        "rule": fund,
    })
}

#[tauri::command]
pub(crate) async fn run_fund(
    fund_id: String,
    live: Option<bool>,
    store: State<'_, Store>,
    engine: State<'_, EngineManager>,
) -> Result<Value, String> {
    let params = {
        let data = store.data.lock().map_err(|e| format!("Lock funds: {e}"))?;
        let fund = data
            .funds
            .iter()
            .find(|f| f.get("id").and_then(Value::as_str) == Some(fund_id.as_str()))
            .ok_or_else(|| format!("Unknown fund {fund_id}."))?;
        run_params(fund, &data, live.unwrap_or(false))
    };
    let mgr = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.request("fund", params, FUND_TIMEOUT))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fund() -> Value {
        json!({ "name": "Core", "amount": 500.0, "members": ["a", "b"], "method": "sharpe", "lookbackDays": 90,
                "rebalance": "weekly", "capPct": 40.0, "kellyFraction": 0.5 })
    }

    fn check(value: &Value) -> Result<Value, String> {
        validate(value, |id| id == "a" || id == "b", "new-id".into(), "2026-11-06".into())
    }

    #[test]
    fn a_valid_fund_is_kept_and_completed() {
        let saved = check(&fund()).unwrap();
        assert_eq!(saved["id"], "new-id");
        assert_eq!(saved["liveStartDate"], "2026-11-06");
        assert_eq!(saved["lookbackDays"], 90);
        assert_eq!(saved["capPct"], 40.0);
        // Saving it again keeps its id and start.
        let again = validate(&saved, |_| true, "other".into(), "2030-01-01".into()).unwrap();
        assert_eq!(again["id"], "new-id");
        assert_eq!(again["liveStartDate"], "2026-11-06");
    }

    #[test]
    fn defaults_fill_what_is_missing() {
        let saved = check(&json!({ "name": "x", "amount": 1, "members": ["a"] })).unwrap();
        assert_eq!(saved["method"], "equal");
        assert_eq!(saved["rebalance"], "monthly");
        assert_eq!(saved["lookbackDays"], 180);
        assert_eq!(saved["capPct"], 50.0);
        assert_eq!(saved["kellyFraction"], 0.25);
    }

    #[test]
    fn a_wrong_fund_is_refused() {
        for (key, value) in [
            ("name", json!("  ")),
            ("amount", json!(0)),
            ("amount", json!(-5)),
            ("members", json!([])),
            ("members", json!(["a", "a"])),
            ("members", json!(["ghost"])),
            ("method", json!("luck")),
            ("lookbackDays", json!(45)),
            ("rebalance", json!("daily")),
            ("capPct", json!(0)),
            ("capPct", json!(150)),
            ("kellyFraction", json!(0.3)),
            ("liveStartDate", json!("soon")),
        ] {
            let mut bad = fund();
            bad[key] = value.clone();
            assert!(check(&bad).is_err(), "{key} = {value} passed");
        }
    }

    #[test]
    fn fixed_shares_are_checked() {
        let mut f = fund();
        f["method"] = json!("fixed");
        assert!(check(&f).is_err(), "no shares");
        f["fixedPct"] = json!({ "a": 60, "b": 50 });
        assert!(check(&f).is_err(), "above 100%");
        f["fixedPct"] = json!({ "a": 60, "b": 30, "gone": 5 });
        let saved = check(&f).unwrap();
        assert_eq!(saved["fixedPct"], json!({ "a": 60.0, "b": 30.0 }));
    }

    #[test]
    fn the_run_uses_each_members_settings_and_names_a_deleted_one() {
        let mut data: crate::PersistedStore = serde_json::from_value(json!({ "systems": [] })).unwrap();
        data.systems = serde_json::from_value(json!([{ "id": "a", "name": "Alpha", "short": "A", "status": "ready", "description": "" }])).unwrap();
        data.system_configs.insert("a".into(), json!({ "topN": 5 }));
        let params = run_params(&json!({ "amount": 500, "members": ["a", "gone"], "liveStartDate": "2026-11-06" }), &data, true);
        assert_eq!(params["members"][0]["name"], "Alpha");
        assert_eq!(params["members"][0]["config"]["topN"], 5);
        assert_eq!(params["removed"], json!(["gone"]));
        assert_eq!(params["live"], true);
    }
}
