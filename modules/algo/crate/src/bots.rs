//! The bots table and its commands (PLAN-QUANTALGO §3.1, §3.3).
//!
//! A bot is one strategy on one connected exchange, one pair, one timeframe,
//! in one mode, with its own budget. Rows outlive runs: a stopped bot keeps
//! its name, its journal (`trades.bot_id`) and its equity curve
//! (`equity_snapshots.bot_id`). The process side — start, stop, the runner —
//! is `bot.rs`; this file never spawns anything.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::runtime::Mode;
use crate::{AppState, Trade};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bot {
    pub id: String,
    pub name: String,
    pub strategy_id: String,
    pub exchange_id: String,
    pub pair: String,
    pub timeframe: String,
    pub trading_mode: String,
    /// Quote-currency cash the bot may use.
    pub budget: f64,
    /// `stopped` | `running` | `error`.
    pub status: String,
    pub started_at: Option<String>,
    pub stopped_at: Option<String>,
    pub last_error: Option<String>,
    /// Risk overrides and strategy parameter overrides (`risk_per_trade`,
    /// `max_positions`, `slippage`, `fee`, `strategy_params`).
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// What the create form (or the MCP tool) sends. Everything but the
/// strategy, exchange and pair has a default.
#[derive(Debug, Deserialize, Clone)]
pub struct BotDraft {
    #[serde(default)]
    pub name: Option<String>,
    pub strategy_id: String,
    pub exchange_id: String,
    pub pair: String,
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    #[serde(default = "default_mode")]
    pub trading_mode: String,
    #[serde(default = "default_budget")]
    pub budget: f64,
    #[serde(default)]
    pub config: Option<Value>,
}

fn default_timeframe() -> String {
    "1h".into()
}

fn default_mode() -> String {
    "paper".into()
}

fn default_budget() -> f64 {
    10_000.0
}

/// Fields `update_bot` may change while a bot is stopped.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct BotPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub budget: Option<f64>,
    #[serde(default)]
    pub timeframe: Option<String>,
    #[serde(default)]
    pub trading_mode: Option<String>,
    #[serde(default)]
    pub pair: Option<String>,
    #[serde(default)]
    pub strategy_id: Option<String>,
    #[serde(default)]
    pub exchange_id: Option<String>,
    #[serde(default)]
    pub config: Option<Value>,
}

/// A bot row plus what its runtime says right now (zeros when stopped).
#[derive(Debug, Serialize, Clone)]
pub struct BotSnapshot {
    #[serde(flatten)]
    pub bot: Bot,
    pub equity: f64,
    pub balance: f64,
    pub last_price: f64,
    pub open_positions: usize,
    /// True while this session holds a runner process for the bot.
    pub process_alive: bool,
}

pub(crate) const SELECT_BOT: &str = "SELECT id, name, strategy_id, exchange_id, pair, timeframe, trading_mode, budget, status, started_at, stopped_at, last_error, config_json, created_at, updated_at FROM bots";

pub(crate) fn row_to_bot(row: &rusqlite::Row) -> rusqlite::Result<Bot> {
    Ok(Bot {
        id: row.get(0)?,
        name: row.get(1)?,
        strategy_id: row.get(2)?,
        exchange_id: row.get(3)?,
        pair: row.get(4)?,
        timeframe: row.get(5)?,
        trading_mode: row.get(6)?,
        budget: row.get(7)?,
        status: row.get(8)?,
        started_at: row.get(9)?,
        stopped_at: row.get(10)?,
        last_error: row.get(11)?,
        config_json: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

pub(crate) fn load_bot(conn: &Connection, id: &str) -> Result<Bot, String> {
    conn.query_row(&format!("{SELECT_BOT} WHERE id = ?1"), params![id], row_to_bot)
        .optional()
        .map_err(|e| format!("Query bot: {e}"))?
        .ok_or_else(|| format!("Bot {id} not found."))
}

pub(crate) fn load_bots(conn: &Connection) -> Result<Vec<Bot>, String> {
    let mut stmt = conn
        .prepare(&format!("{SELECT_BOT} ORDER BY created_at ASC"))
        .map_err(|e| format!("Prepare bots: {e}"))?;
    let rows = stmt
        .query_map([], row_to_bot)
        .map_err(|e| format!("Query bots: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row: {e}"))
}

pub(crate) fn validate_timeframe(timeframe: &str) -> Result<(), String> {
    if matches!(timeframe, "1m" | "5m" | "15m" | "1h" | "4h" | "1d" | "1w") {
        Ok(())
    } else {
        Err(format!("Unsupported timeframe '{timeframe}'."))
    }
}

pub(crate) fn validate_mode(mode: &str) -> Result<Mode, String> {
    Mode::parse(mode).ok_or_else(|| format!("Unsupported trading mode '{mode}'."))
}

pub(crate) fn insert_bot(conn: &Connection, draft: &BotDraft) -> Result<Bot, String> {
    validate_timeframe(&draft.timeframe)?;
    validate_mode(&draft.trading_mode)?;
    if draft.pair.trim().is_empty() {
        return Err("A trading pair is required.".into());
    }
    if draft.budget < 100.0 {
        return Err("The budget must be at least 100.".into());
    }
    let strategy_name: Option<String> = conn
        .query_row(
            "SELECT name FROM strategies WHERE id = ?1",
            params![draft.strategy_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Query strategy: {e}"))?;
    let Some(strategy_name) = strategy_name else {
        return Err("The strategy does not exist.".into());
    };
    let exchange_name: Option<String> = conn
        .query_row(
            "SELECT name FROM exchanges WHERE id = ?1",
            params![draft.exchange_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Query exchange: {e}"))?;
    let Some(exchange_name) = exchange_name else {
        return Err("The exchange is not connected.".into());
    };

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let name = draft
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!("{strategy_name} · {} · {} {}", draft.pair, exchange_name, draft.timeframe)
        });
    let config_json = draft.config.as_ref().map(|v| v.to_string());
    conn.execute(
        "INSERT INTO bots (id, name, strategy_id, exchange_id, pair, timeframe, trading_mode, budget, status, config_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'stopped', ?9, ?10, ?10)",
        params![
            id,
            name,
            draft.strategy_id,
            draft.exchange_id,
            draft.pair,
            draft.timeframe,
            draft.trading_mode,
            draft.budget,
            config_json,
            now,
        ],
    )
    .map_err(|e| format!("Insert bot: {e}"))?;
    load_bot(conn, &id)
}

/// Record a status transition on the row. `None` fields are left alone
/// except that a `running` bot gets a fresh `started_at` and a stopped one a
/// `stopped_at`.
pub(crate) fn set_bot_status(
    conn: &Connection,
    id: &str,
    status: &str,
    last_error: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    match status {
        "running" => conn.execute(
            "UPDATE bots SET status = 'running', started_at = ?1, stopped_at = NULL, last_error = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        ),
        _ => conn.execute(
            "UPDATE bots SET status = ?1, stopped_at = ?2, last_error = ?3, updated_at = ?2 WHERE id = ?4",
            params![status, now, last_error, id],
        ),
    }
    .map(|_| ())
    .map_err(|e| format!("Update bot status: {e}"))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub(crate) fn create_bot(draft: BotDraft, state: State<'_, AppState>) -> Result<Bot, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    insert_bot(&db, &draft)
}

#[tauri::command(async)]
pub(crate) fn update_bot(
    bot_id: String,
    patch: BotPatch,
    state: State<'_, AppState>,
) -> Result<Bot, String> {
    {
        let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        if bots.contains_key(&bot_id) {
            return Err("Stop the bot before changing it.".into());
        }
    }
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let mut bot = load_bot(&db, &bot_id)?;
    if let Some(name) = patch.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        bot.name = name.to_string();
    }
    if let Some(budget) = patch.budget {
        if budget < 100.0 {
            return Err("The budget must be at least 100.".into());
        }
        bot.budget = budget;
    }
    if let Some(timeframe) = patch.timeframe {
        validate_timeframe(&timeframe)?;
        bot.timeframe = timeframe;
    }
    if let Some(mode) = patch.trading_mode {
        validate_mode(&mode)?;
        bot.trading_mode = mode;
    }
    if let Some(pair) = patch.pair.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        bot.pair = pair;
    }
    if let Some(strategy_id) = patch.strategy_id {
        bot.strategy_id = strategy_id;
    }
    if let Some(exchange_id) = patch.exchange_id {
        bot.exchange_id = exchange_id;
    }
    if let Some(config) = patch.config {
        bot.config_json = Some(config.to_string());
    }
    let now = Utc::now().to_rfc3339();
    db.execute(
        "UPDATE bots SET name = ?1, budget = ?2, timeframe = ?3, trading_mode = ?4, pair = ?5, strategy_id = ?6, exchange_id = ?7, config_json = ?8, updated_at = ?9 WHERE id = ?10",
        params![
            bot.name,
            bot.budget,
            bot.timeframe,
            bot.trading_mode,
            bot.pair,
            bot.strategy_id,
            bot.exchange_id,
            bot.config_json,
            now,
            bot.id,
        ],
    )
    .map_err(|e| format!("Update bot: {e}"))?;
    load_bot(&db, &bot_id)
}

#[tauri::command(async)]
pub(crate) fn delete_bot(bot_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    {
        let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
        if bots.contains_key(&bot_id) {
            return Err("Stop the bot before deleting it.".into());
        }
    }
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    // The journal keeps its rows: they carry the bot id and are the record
    // of what the bot did. Only the row that could start it again goes.
    let affected = db
        .execute("DELETE FROM bots WHERE id = ?1", params![bot_id])
        .map_err(|e| format!("Delete bot: {e}"))?;
    Ok(affected > 0)
}

/// Every bot with its live snapshot — what the Bots page and the dashboard
/// render, and what `quantsuite.algo.list_bots` returns.
#[tauri::command(async)]
pub(crate) fn list_bots(state: State<'_, AppState>) -> Result<Vec<BotSnapshot>, String> {
    let rows = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        load_bots(&db)?
    };
    let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|bot| {
            let live = bots
                .get(&bot.id)
                .and_then(|handle| handle.runtime.lock().ok().map(|rt| (rt.equity(), rt.balance, rt.last_price, rt.open_positions.len())));
            match live {
                Some((equity, balance, last_price, open_positions)) => BotSnapshot {
                    bot,
                    equity,
                    balance,
                    last_price,
                    open_positions,
                    process_alive: true,
                },
                None => BotSnapshot {
                    bot,
                    equity: 0.0,
                    balance: 0.0,
                    last_price: 0.0,
                    open_positions: 0,
                    process_alive: false,
                },
            }
        })
        .collect())
}

#[tauri::command(async)]
pub(crate) fn get_bot(bot_id: String, state: State<'_, AppState>) -> Result<BotSnapshot, String> {
    list_bots(state)?
        .into_iter()
        .find(|s| s.bot.id == bot_id)
        .ok_or_else(|| format!("Bot {bot_id} not found."))
}

/// The open positions of a running bot as journal rows; empty when stopped.
#[tauri::command(async)]
pub(crate) fn get_bot_positions(bot_id: String, state: State<'_, AppState>) -> Result<Vec<Trade>, String> {
    let bots = state.bots.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(bots
        .get(&bot_id)
        .and_then(|handle| handle.runtime.lock().ok().map(|rt| rt.open_trades()))
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn db_with_refs() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO strategies (id, name, description, file_path, params_json, created_at, updated_at) VALUES ('s1', 'EMA Cross', '', 'x.py', NULL, 't', 't')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exchanges (id, name, exchange_type, provider, config_encrypted, is_active, created_at, updated_at) VALUES ('e1', 'Bybit', 'cex', 'bybit', NULL, 1, 't', 't')",
            [],
        )
        .unwrap();
        conn
    }

    fn draft() -> BotDraft {
        BotDraft {
            name: None,
            strategy_id: "s1".into(),
            exchange_id: "e1".into(),
            pair: "BTC/USDT".into(),
            timeframe: "1h".into(),
            trading_mode: "paper".into(),
            budget: 5_000.0,
            config: None,
        }
    }

    #[test]
    fn a_bot_is_named_after_its_parts_and_starts_stopped() {
        let conn = db_with_refs();
        let bot = insert_bot(&conn, &draft()).unwrap();
        assert_eq!(bot.name, "EMA Cross · BTC/USDT · Bybit 1h");
        assert_eq!(bot.status, "stopped");
        assert_eq!(bot.budget, 5_000.0);
        assert_eq!(load_bots(&conn).unwrap().len(), 1);
        assert_eq!(load_bot(&conn, &bot.id).unwrap().pair, "BTC/USDT");
    }

    #[test]
    fn a_bot_needs_a_real_strategy_exchange_timeframe_and_budget() {
        let conn = db_with_refs();
        let mut d = draft();
        d.strategy_id = "nope".into();
        assert!(insert_bot(&conn, &d).unwrap_err().contains("strategy"));
        let mut d = draft();
        d.exchange_id = "nope".into();
        assert!(insert_bot(&conn, &d).unwrap_err().contains("exchange"));
        let mut d = draft();
        d.timeframe = "7m".into();
        assert!(insert_bot(&conn, &d).unwrap_err().contains("timeframe"));
        let mut d = draft();
        d.budget = 10.0;
        assert!(insert_bot(&conn, &d).unwrap_err().contains("budget"));
        let mut d = draft();
        d.trading_mode = "demo".into();
        assert!(insert_bot(&conn, &d).unwrap_err().contains("mode"));
    }

    #[test]
    fn status_transitions_keep_their_timestamps_and_error() {
        let conn = db_with_refs();
        let bot = insert_bot(&conn, &draft()).unwrap();
        set_bot_status(&conn, &bot.id, "running", None).unwrap();
        let running = load_bot(&conn, &bot.id).unwrap();
        assert_eq!(running.status, "running");
        assert!(running.started_at.is_some());
        set_bot_status(&conn, &bot.id, "error", Some("feed died")).unwrap();
        let errored = load_bot(&conn, &bot.id).unwrap();
        assert_eq!(errored.status, "error");
        assert_eq!(errored.last_error.as_deref(), Some("feed died"));
        assert!(errored.stopped_at.is_some());
    }
}
