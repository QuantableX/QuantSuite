use chrono::Utc;
use rusqlite::params;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use crate::{AppState, Strategy};
use crate::settings::{build_python_command, resolve_python_path};

/// The strategy the Indicators page writes for a forge indicator: hold the
/// regime the indicator declares, flip when it flips, nothing else. The
/// markers are replaced with the chosen indicator's key, name and class.
const REGIME_TREND_TEMPLATE: &str = r#"from quantalgo import Strategy


class __CLASS_NAME__(Strategy):
    """Trades the regime of the __INDICATOR_NAME__ indicator (Indicator Smithery).

    The indicator is a causal binary trend classifier: +1 in an uptrend regime,
    -1 in a downtrend regime, 0 while it warms up. This strategy holds a long
    position through an uptrend and — when ``direction`` is ``"both"`` — a short
    position through a downtrend; it flips on the candle the regime flips.
    Sizing is ``position_size`` of the balance. Indicator parameters are
    copied from the registry when this strategy is created.

    Check the Smithery evidence for the chosen timeframe and run a strategy
    backtest with costs. Registration alone does not certify an indicator.
    """

    params = {
        "indicator": "__INDICATOR_KEY__",
        "indicator_params": __INDICATOR_PARAMS__,
        "warmup_candles": __WARMUP_BARS__,
        "direction": "long",   # "long" | "both" — spot live trading is long-only
        "position_size": 0.95,
    }

    def on_start(self):
        self.log(
            f"RegimeTrend on {self.params['indicator']} "
            f"({self.params['direction']}), {len(self._candle_history)} candles of history"
        )

    def on_candle(self, candle):
        if len(self._candle_history) < self.params["warmup_candles"]:
            return
        regime = self.regime(
            self.params["indicator"], self.params.get("indicator_params") or None
        )
        if regime == 0:
            return  # still warming up

        pair = self._pair
        pos = self.get_position(pair)
        held = getattr(pos, "side", None)
        if regime > 0:
            want = "long"
        elif self.params["direction"] == "both":
            want = "short"
        else:
            want = None

        if want == held:
            return
        if want is None:
            if pos is not None:
                self.close()
            return
        if pos is not None:
            # Close the opposite side and open the new one on post-close cash.
            self.reverse(pair, "buy" if want == "long" else "sell", self.params["position_size"])
            return

        balance = self.get_balance()
        capital = list(balance.values())[0] if isinstance(balance, dict) and balance else 0.0
        qty = (capital * self.params["position_size"]) / candle.close
        if qty <= 0:
            return
        if want == "long":
            self.buy(pair, qty)
        else:
            self.sell(pair, qty)
"#;

const STRATEGY_TEMPLATE: &str = r#"from quantalgo import Strategy


class NewStrategy(Strategy):
    """Strategy description here."""

    params = {
        "fast_period": 12,
        "slow_period": 26,
        "position_size": 0.95,
    }

    def on_candle(self, candle):
        pass

    def on_tick(self, tick):
        pass

    def on_trade(self, trade):
        pass

    def on_start(self):
        pass

    def on_stop(self):
        pass
"#;

// ---------------------------------------------------------------------------
// Strategy Commands
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub(crate) fn list_strategies(state: State<'_, AppState>) -> Result<Vec<Strategy>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let mut stmt = db
        .prepare("SELECT id, name, description, file_path, params_json, created_at, updated_at FROM strategies ORDER BY updated_at DESC")
        .map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Strategy {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                file_path: row.get(3)?,
                params_json: row.get(4)?,
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

#[tauri::command(async)]
pub(crate) fn get_strategy(id: String, state: State<'_, AppState>) -> Result<Strategy, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    db.query_row(
        "SELECT id, name, description, file_path, params_json, created_at, updated_at FROM strategies WHERE id = ?1",
        params![id],
        |row| {
            Ok(Strategy {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                file_path: row.get(3)?,
                params_json: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .map_err(|e| format!("Not found: {e}"))
}

/// A Python identifier from a display name: alphanumerics and underscores,
/// never starting with a digit.
fn class_name_for(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if safe.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("S_{safe}")
    } else {
        safe
    }
}

/// Write a strategy file and its row — shared by the blank template and the
/// forge template.
fn write_new_strategy(
    state: &AppState,
    name: &str,
    description: &str,
    code: &str,
    params_json: Option<String>,
) -> Result<Strategy, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
    let strategy_dir = PathBuf::from(&settings.strategy_dir);
    drop(settings);

    std::fs::create_dir_all(&strategy_dir).map_err(|e| format!("Mkdir: {e}"))?;

    let file_name = format!("{}_{}.py", class_name_for(name), &id[..8]);
    let file_path = strategy_dir.join(&file_name);
    std::fs::write(&file_path, code).map_err(|e| format!("Write file: {e}"))?;

    let file_path_str = file_path.to_string_lossy().to_string();
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    db.execute(
        "INSERT INTO strategies (id, name, description, file_path, params_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, name, description, file_path_str, params_json, now, now],
    )
    .map_err(|e| format!("Insert: {e}"))?;

    Ok(Strategy {
        id,
        name: name.to_string(),
        description: description.to_string(),
        file_path: file_path_str,
        params_json,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub(crate) async fn create_strategy(
    name: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<Strategy, String> {
    let code = STRATEGY_TEMPLATE
        .replace("NewStrategy", &class_name_for(&name))
        .replace("Strategy description here.", &description);
    write_new_strategy(&state, &name, &description, &code, None)
}

// ---------------------------------------------------------------------------
// The Indicator Smithery (PLAN-QUANTALGO §6)
// ---------------------------------------------------------------------------

/// The forge's registry, from `python -m smithery.registry --json`, cached
/// for the session. Blocking: runs the interpreter.
pub(crate) fn load_indicator_registry(state: &AppState, refresh: bool) -> Result<Value, String> {
    if !refresh {
        if let Some(cached) = state
            .indicators
            .lock()
            .map_err(|e| format!("Lock: {e}"))?
            .as_ref()
        {
            return Ok(cached.clone());
        }
    }

    let python_path = {
        let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
        resolve_python_path(&settings)
    };
    let output = build_python_command(&python_path)
        .arg("-m")
        .arg("smithery.registry")
        .arg("--json")
        .output()
        .map_err(|e| format!("Could not run the forge's registry with '{python_path}': {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok());

    match parsed {
        Some(registry) if output.status.success() => {
            *state.indicators.lock().map_err(|e| format!("Lock: {e}"))? = Some(registry.clone());
            Ok(registry)
        }
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            Err(format!(
                "The forge's registry could not be read (python -m smithery.registry): {}",
                if detail.is_empty() {
                    "no JSON on stdout — is numpy/pandas installed for this interpreter?"
                } else {
                    detail
                }
            ))
        }
    }
}

#[tauri::command]
pub(crate) async fn list_indicators(
    refresh: Option<bool>,
    app_handle: AppHandle,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        load_indicator_registry(&state, refresh.unwrap_or(false))
    })
    .await
    .map_err(|e| format!("Registry task failed: {e}"))?
}

/// Problems with a parameter override, judged by the registry entry's
/// `schema` — the rules of the forge's `TrendIndicator.check_params`
/// (sidecars/python/smithery/contract.py). Empty when the values can be used.
fn indicator_param_problems(schema: &Value, values: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, value) in values {
        let Some(entry) = schema.get(name) else {
            problems.push(format!("{name}: unknown parameter"));
            continue;
        };
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("float");
        let problem = match kind {
            "bool" => (!value.is_boolean()).then(|| "expected true or false".to_string()),
            "choice" => {
                let choices = entry.get("choices").and_then(Value::as_array).cloned().unwrap_or_default();
                (!choices.contains(value)).then(|| format!("expected one of {}", Value::Array(choices)))
            }
            "list" => {
                let n = entry.get("default").and_then(Value::as_array).map_or(0, Vec::len);
                let ok = value.as_array().is_some_and(|items| items.len() == n && items.iter().all(Value::is_number));
                (!ok).then(|| format!("expected {n} numbers"))
            }
            _ => match value.as_f64() {
                None => Some("expected a finite number".to_string()),
                Some(v) if kind == "int" && v.fract() != 0.0 => Some("expected a whole number".to_string()),
                Some(v) => {
                    let lo = entry.get("min").and_then(Value::as_f64);
                    let hi = entry.get("max").and_then(Value::as_f64);
                    match (lo, hi) {
                        (Some(lo), _) if v < lo => Some(format!("at least {}", entry["min"])),
                        (_, Some(hi)) if v > hi => Some(format!("at most {}", entry["max"])),
                        _ => None,
                    }
                }
            },
        };
        if let Some(problem) = problem {
            problems.push(format!("{name}: {problem}"));
        }
    }
    problems
}

/// The parameters a new strategy freezes: the version's own (`entry.params`)
/// with `overrides` on top, checked against `entry.schema`; whole numbers
/// of an int parameter stay ints.
fn strategy_indicator_params(entry: &Value, overrides: Option<&Value>) -> Result<Value, String> {
    let mut params = entry.get("params").and_then(Value::as_object).cloned().unwrap_or_default();
    let Some(overrides) = overrides.filter(|v| !v.is_null()) else {
        return Ok(Value::Object(params));
    };
    let key = entry.get("key").and_then(Value::as_str).unwrap_or("?");
    let values = overrides
        .as_object()
        .ok_or_else(|| format!("Parameters of '{key}' must be an object of name → value."))?;
    if values.is_empty() {
        return Ok(Value::Object(params));
    }
    let schema = entry
        .get("schema")
        .filter(|s| s.is_object())
        .ok_or_else(|| format!("The registry has no parameter schema for '{key}' — restart QuantSuite after updating the engine."))?;
    let problems = indicator_param_problems(schema, values);
    if !problems.is_empty() {
        return Err(format!("Parameters of '{key}': {}", problems.join("; ")));
    }
    for (name, value) in values {
        let int = schema[name].get("type").and_then(Value::as_str) == Some("int");
        let typed = match value.as_f64() {
            Some(v) if int => Value::from(v as i64),
            _ => value.clone(),
        };
        params.insert(name.clone(), typed);
    }
    Ok(Value::Object(params))
}

/// A RegimeTrend strategy for one forge indicator — the Roster's "New
/// strategy": a version key, an optional name and optional parameter
/// overrides (checked against the registry's schema, frozen into the
/// strategy's `indicator_params`).
#[tauri::command]
pub(crate) async fn create_strategy_from_indicator(
    indicator: String,
    name: Option<String>,
    params: Option<Value>,
    app_handle: AppHandle,
) -> Result<Strategy, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let registry = load_indicator_registry(&state, false)?;
        let entry = registry
            .get("indicators")
            .and_then(|v| v.as_array())
            .and_then(|list| {
                list.iter()
                    .find(|item| item.get("key").and_then(|k| k.as_str()) == Some(indicator.as_str()))
            })
            .ok_or_else(|| format!("The forge has no indicator '{indicator}'."))?;

        let indicator_name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(indicator.as_str())
            .to_string();
        let hypothesis = entry
            .get("hypothesis")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let indicator_params = strategy_indicator_params(entry, params.as_ref())?;
        let custom = indicator_params != entry.get("params").cloned().unwrap_or_else(|| serde_json::json!({}));
        let display_name = name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Regime {indicator_name}{}", if custom { " (custom)" } else { "" }));
        let class_name = class_name_for(&display_name);
        let description = {
            let mut text = format!("RegimeTrend on {indicator_name}: {hypothesis}");
            if text.chars().count() > 240 {
                text = text.chars().take(237).collect::<String>() + "...";
            }
            text
        };
        // JSON is parsed in Python so future boolean/null parameters stay valid.
        let params_literal = serde_json::to_string(&indicator_params.to_string())
            .map_err(|e| format!("Indicator params: {e}"))?;
        let warmup_bars = entry.get("warmup_bars").and_then(Value::as_u64).unwrap_or(400);
        let code = REGIME_TREND_TEMPLATE
            .replace("__CLASS_NAME__", &class_name)
            .replace("__INDICATOR_NAME__", &indicator_name)
            .replace("__INDICATOR_KEY__", &indicator)
            .replace("__INDICATOR_PARAMS__", &format!("__import__('json').loads({params_literal})"))
            .replace("__WARMUP_BARS__", &warmup_bars.to_string());
        let params_json = serde_json::json!({
            "indicator": indicator,
            "indicator_params": indicator_params,
            "warmup_candles": warmup_bars,
            "direction": "long",
            "position_size": 0.95,
        })
        .to_string();

        write_new_strategy(&state, &display_name, &description, &code, Some(params_json))
    })
    .await
    .map_err(|e| format!("Create strategy task failed: {e}"))?
}

#[tauri::command]
pub(crate) async fn save_strategy(
    id: String,
    code: String,
    params: Option<Value>,
    state: State<'_, AppState>,
) -> Result<Strategy, String> {
    let now = Utc::now().to_rfc3339();
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    let strategy: Strategy = db
        .query_row(
            "SELECT id, name, description, file_path, params_json, created_at, updated_at FROM strategies WHERE id = ?1",
            params![id],
            |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    file_path: row.get(3)?,
                    params_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Not found: {e}"))?;

    std::fs::write(&strategy.file_path, &code).map_err(|e| format!("Write file: {e}"))?;

    let params_json = params.map(|v| serde_json::to_string(&v).unwrap_or_default());

    db.execute(
        "UPDATE strategies SET params_json = ?1, updated_at = ?2 WHERE id = ?3",
        params![params_json, now, id],
    )
    .map_err(|e| format!("Update: {e}"))?;

    Ok(Strategy {
        id: strategy.id,
        name: strategy.name,
        description: strategy.description,
        file_path: strategy.file_path,
        params_json,
        created_at: strategy.created_at,
        updated_at: now,
    })
}

#[tauri::command(async)]
pub(crate) fn update_strategy_meta(
    id: String,
    name: Option<String>,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Strategy, String> {
    let now = Utc::now().to_rfc3339();
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    let mut strategy: Strategy = db
        .query_row(
            "SELECT id, name, description, file_path, params_json, created_at, updated_at FROM strategies WHERE id = ?1",
            params![id],
            |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    file_path: row.get(3)?,
                    params_json: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("Not found: {e}"))?;

    if let Some(value) = name {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            strategy.name = trimmed.to_string();
        }
    }

    if let Some(value) = description {
        strategy.description = value.trim().to_string();
    }

    db.execute(
        "UPDATE strategies SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4",
        params![strategy.name, strategy.description, now, strategy.id],
    )
    .map_err(|e| format!("Update: {e}"))?;

    strategy.updated_at = now;
    Ok(strategy)
}

#[tauri::command]
pub(crate) async fn delete_strategy(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;

    let file_path: Option<String> = db
        .query_row(
            "SELECT file_path FROM strategies WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok();

    if let Some(ref path) = file_path {
        let _ = std::fs::remove_file(path);
    }

    let affected = db
        .execute("DELETE FROM strategies WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete: {e}"))?;

    Ok(affected > 0)
}

#[tauri::command]
pub(crate) async fn read_strategy_file(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let file_path: String = db
        .query_row(
            "SELECT file_path FROM strategies WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Not found: {e}"))?;
    std::fs::read_to_string(&file_path).map_err(|e| format!("Read: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry() -> Value {
        json!({
            "key": "alpha_opt",
            "params": { "length": 20, "mult": 2.0, "mode": "fast", "gate": true, "lags": [1, 2, 4] },
            "schema": {
                "length": { "type": "int", "min": 2, "max": 400 },
                "mult": { "type": "float", "min": 0.5 },
                "mode": { "type": "choice", "choices": ["fast", "slow"] },
                "gate": { "type": "bool" },
                "lags": { "type": "list", "default": [1, 2, 4] }
            }
        })
    }

    #[test]
    fn no_overrides_freeze_the_version_params() {
        assert_eq!(strategy_indicator_params(&entry(), None).unwrap(), entry()["params"]);
        assert_eq!(strategy_indicator_params(&entry(), Some(&json!({}))).unwrap(), entry()["params"]);
        assert_eq!(strategy_indicator_params(&entry(), Some(&Value::Null)).unwrap(), entry()["params"]);
    }

    #[test]
    fn overrides_merge_over_the_version_and_keep_ints() {
        let got = strategy_indicator_params(&entry(), Some(&json!({ "length": 34.0, "mode": "slow", "lags": [2, 3, 5] }))).unwrap();
        assert_eq!(got["length"], json!(34));
        assert!(got["length"].is_i64());
        assert_eq!(got["mode"], json!("slow"));
        assert_eq!(got["lags"], json!([2, 3, 5]));
        assert_eq!(got["mult"], json!(2.0));
    }

    #[test]
    fn invalid_overrides_name_the_parameter() {
        let err = strategy_indicator_params(
            &entry(),
            Some(&json!({ "length": 1, "mult": "x", "mode": "medium", "gate": 1, "lags": [1, 2], "nope": 3 })),
        )
        .unwrap_err();
        for part in [
            "Parameters of 'alpha_opt'",
            "length: at least 2",
            "mult: expected a finite number",
            "mode: expected one of",
            "gate: expected true or false",
            "lags: expected 3 numbers",
            "nope: unknown parameter",
        ] {
            assert!(err.contains(part), "{part} missing in {err}");
        }
        assert!(strategy_indicator_params(&entry(), Some(&json!({ "length": 2.5 }))).unwrap_err().contains("whole number"));
        assert!(strategy_indicator_params(&entry(), Some(&json!({ "length": 401 }))).unwrap_err().contains("at most 400"));
        assert!(strategy_indicator_params(&entry(), Some(&json!([1]))).unwrap_err().contains("must be an object"));
    }

    #[test]
    fn overrides_need_a_schema() {
        let bare = json!({ "key": "old", "params": { "length": 20 } });
        assert!(strategy_indicator_params(&bare, Some(&json!({ "length": 30 }))).unwrap_err().contains("no parameter schema"));
        assert_eq!(strategy_indicator_params(&bare, None).unwrap(), json!({ "length": 20 }));
    }
}
