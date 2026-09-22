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

/// A RegimeTrend strategy for one forge indicator — the Indicators page's
/// "New strategy from this indicator".
#[tauri::command]
pub(crate) async fn create_strategy_from_indicator(
    indicator: String,
    name: Option<String>,
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
        let display_name = name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Regime {indicator_name}"));
        let class_name = class_name_for(&display_name);
        let description = {
            let mut text = format!("RegimeTrend on {indicator_name}: {hypothesis}");
            if text.chars().count() > 240 {
                text = text.chars().take(237).collect::<String>() + "...";
            }
            text
        };
        let indicator_params = entry.get("params").cloned().unwrap_or_else(|| serde_json::json!({}));
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
