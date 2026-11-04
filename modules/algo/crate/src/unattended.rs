//! Automated bots run unattended (PLAN-AUTO-EXECUTION §2.6, card 4).
//!
//! - **Resume after restart**, opt-in per bot (`resume_after_restart` in the
//!   bot's config; off by default). On start the bot's books — its cash and
//!   the positions it opened since it was started — are rebuilt from the
//!   journal and, for a live bot, reconciled with the venue: the venue must
//!   hold at least the base the books say is open, and no QuantSuite order
//!   may still rest on it. A mismatch keeps the bot stopped with the reason;
//!   it never trades to fix one.
//! - **Health**: no new candle for two bar lengths, or three order errors in
//!   a row, pause the bot (stopped, with the reason) and notify; every feed
//!   tick is a heartbeat (`bot:heartbeat`, `bots.heartbeat_at`).
//! - **Kill switch** "Stop all trading": sets the flag every run checks,
//!   stops every bot, disarms every Manual system and cancels QuantSuite's
//!   open orders. It never sells; the report says what stays on the exchange.

use std::collections::BTreeMap;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::bots::Bot;
use crate::execution::venue::{ExecutionVenue, Instrument};
use crate::runtime::Position;
use crate::{AppState, ChargedFee};

/// Seconds per bar of a bot timeframe (bots run 1h, 4h or 1d).
pub fn bar_seconds(timeframe: &str) -> u64 {
    match timeframe {
        "1h" => 3_600,
        "4h" => 14_400,
        "1d" => 86_400,
        other => {
            let (n, unit) = other.split_at(other.len().saturating_sub(1));
            let n: u64 = n.parse().unwrap_or(1);
            n * match unit {
                "m" => 60,
                "h" => 3_600,
                "d" => 86_400,
                _ => 3_600,
            }
        }
    }
}

/// No new candle for two bar lengths: the feed is stale.
pub fn feed_stale(seconds_since_new_candle: u64, timeframe: &str) -> bool {
    seconds_since_new_candle > 2 * bar_seconds(timeframe)
}

/// A bot's books as the journal has them: its cash and open positions.
#[derive(Debug, Clone)]
pub struct Books {
    pub balance: f64,
    pub positions: Vec<Position>,
}

impl Books {
    /// Base quantity of open longs.
    pub fn long_qty(&self) -> f64 {
        self.positions.iter().filter(|p| p.side == "long").map(|p| p.quantity).sum()
    }
}

/// Rebuild the books of the session that began at `started_at`: the budget,
/// plus what closed since then, minus what the open positions tied up.
pub fn restore_books(conn: &Connection, bot: &Bot, started_at: &str) -> Result<Books, String> {
    let realized: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(pnl), 0) FROM trades WHERE bot_id = ?1 AND is_backtest = 0
             AND exit_time IS NOT NULL AND entry_time >= ?2",
            params![bot.id, started_at],
            |r| r.get(0),
        )
        .map_err(|e| format!("Restore books: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, pair, side, entry_price, quantity, fee, entry_time, notes, fees_json FROM trades
             WHERE bot_id = ?1 AND is_backtest = 0 AND exit_time IS NULL AND entry_time >= ?2 ORDER BY entry_time",
        )
        .map_err(|e| format!("Restore books: {e}"))?;
    let rows = stmt
        .query_map(params![bot.id, started_at], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| format!("Restore books: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Restore books: {e}"))?;
    let mut balance = bot.budget + realized;
    let mut positions = Vec::new();
    for (id, pair, side, entry_price, quantity, fee, entry_time, notes, fees_json) in rows {
        let notional = entry_price * quantity;
        balance -= notional + fee;
        let entry_order_id = notes
            .as_deref()
            .and_then(|n| n.split("entry order ").nth(1))
            .map(|rest| rest.split(' ').next().unwrap_or("").to_string())
            .filter(|s| !s.is_empty());
        let entry_fees: Vec<ChargedFee> = crate::journal::fees_from_json(fees_json);
        positions.push(Position {
            id,
            pair,
            reserved_margin: if side == "short" { notional } else { 0.0 },
            side,
            entry_price,
            quantity,
            entry_fee: fee,
            entry_time,
            entry_order_id,
            entry_fees,
        });
    }
    Ok(Books { balance, positions })
}

/// A live bot's books against the venue: at least the open base must be
/// there, and none of QuantSuite's orders may still rest on the venue.
pub fn reconcile(books: &Books, base: &str, venue_base_total: f64, our_open_orders: &[String]) -> Result<(), String> {
    if !our_open_orders.is_empty() {
        return Err(format!(
            "{} QuantSuite order(s) still open on the venue ({}); cancel them or let them fill, then start the bot by hand",
            our_open_orders.len(),
            our_open_orders.join(", ")
        ));
    }
    let want = books.long_qty();
    // Lot rounding and a base fee leave a hair less than the books say.
    if venue_base_total + 1e-9 < want * 0.999 {
        return Err(format!(
            "the books say {want} {base} is open, the venue holds {venue_base_total} {base}; check the account, then start the bot by hand"
        ));
    }
    Ok(())
}

/// QuantSuite's own client order ids start with `qs` (the engine's `qs…`,
/// the bots' `qsb…`); anything else on the account is the user's and is
/// never touched.
pub fn ours(client_order_id: &str) -> bool {
    client_order_id.starts_with("qs")
}

/// Cancel QuantSuite's open orders on these instruments. Never places one.
pub fn cancel_open_orders(venue: &dyn ExecutionVenue, instruments: &[Instrument]) -> (Vec<String>, Vec<String>) {
    let mut cancelled = Vec::new();
    let mut errors = Vec::new();
    for inst in instruments {
        match venue.open_orders(inst) {
            Ok(ids) => {
                for id in ids.into_iter().filter(|id| ours(id)) {
                    match venue.cancel(inst, &id) {
                        Ok(()) => cancelled.push(format!("{id} ({})", inst.pair())),
                        Err(e) => errors.push(format!("{id} ({}): {e}", inst.pair())),
                    }
                }
            }
            Err(e) => errors.push(format!("{}: listing open orders failed: {e}", inst.pair())),
        }
    }
    (cancelled, errors)
}

// ── the app side ────────────────────────────────────────────────────────────

/// Stop a bot for a health reason, keep the reason on its row, tell the user.
pub fn pause_bot(app: &AppHandle, bot_id: &str, reason: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let _ = crate::bot::stop_bot_inner(app, &state, bot_id);
    if let Ok(db) = state.db.lock() {
        let _ = db.execute(
            "UPDATE bots SET last_error = ?2, updated_at = ?3 WHERE id = ?1",
            params![bot_id, format!("paused: {reason}"), Utc::now().to_rfc3339()],
        );
    }
    let name = state
        .db
        .lock()
        .ok()
        .and_then(|db| db.query_row("SELECT name FROM bots WHERE id = ?1", params![bot_id], |r| r.get::<_, String>(0)).ok())
        .unwrap_or_else(|| bot_id.to_string());
    let line = format!("{name} paused: {reason}");
    crate::logs::push_bot_log_for(app, Some(bot_id), "error", line.clone());
    let _ = app.emit("bot:paused", json!({ "bot_id": bot_id, "reason": reason }));
    crate::manual::toast_line(app, "QuantAlgo · Bot paused", &line);
}

/// Record a feed tick.
pub fn heartbeat(app: &AppHandle, bot_id: &str) {
    let at = Utc::now().to_rfc3339();
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(db) = state.db.lock() {
            let _ = db.execute("UPDATE bots SET heartbeat_at = ?2 WHERE id = ?1", params![bot_id, at]);
        }
    }
    let _ = app.emit("bot:heartbeat", json!({ "bot_id": bot_id, "at": at }));
}

/// The bots that were running when the suite exited and asked to resume.
pub fn resumable(conn: &Connection) -> Vec<(Bot, String)> {
    crate::bots::load_bots(conn)
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.status == "running")
        .filter(|b| {
            b.config_json
                .as_deref()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
                .and_then(|c| c.get("resume_after_restart").and_then(|v| v.as_bool()))
                .unwrap_or(false)
        })
        .filter_map(|b| b.started_at.clone().map(|s| (b, s)))
        .collect()
}

/// Resume each bot after reconciling its books; a mismatch keeps it stopped.
pub fn resume_bots(app: AppHandle, bots: Vec<(Bot, String)>) {
    for (bot, started_at) in bots {
        let state = app.state::<AppState>();
        let outcome = (|| -> Result<(), String> {
            let books = {
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                restore_books(&db, &bot, &started_at)?
            };
            if bot.trading_mode == "live" {
                let venue = {
                    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                    let creds = crate::exchanges::load_credentials(&db, &bot.exchange_id)?;
                    crate::execution::cex::CexVenue::new(&creds.provider, &creds.api_key, &creds.api_secret, creds.sandbox)?
                };
                let inst = venue.instrument(&bot.pair)?;
                let total: f64 = venue
                    .balances()?
                    .iter()
                    .filter(|b| b.asset.eq_ignore_ascii_case(inst.base()))
                    .map(|b| b.free + b.locked)
                    .sum();
                let open: Vec<String> = venue.open_orders(&inst)?.into_iter().filter(|id| ours(id)).collect();
                reconcile(&books, inst.base(), total, &open)?;
            }
            crate::bot::resume_bot(&app, &state, bot.clone(), books, &started_at)
        })();
        match outcome {
            Ok(()) => crate::logs::push_bot_log_for(&app, Some(&bot.id), "warn", format!("{} resumed after the restart", bot.name)),
            Err(e) => {
                if let Ok(db) = state.db.lock() {
                    let _ = db.execute(
                        "UPDATE bots SET status = 'stopped', last_error = ?2, updated_at = ?3 WHERE id = ?1",
                        params![bot.id, format!("not resumed: {e}"), Utc::now().to_rfc3339()],
                    );
                }
                let line = format!("{} not resumed: {e}", bot.name);
                crate::logs::push_bot_log_for(&app, Some(&bot.id), "error", line.clone());
                crate::manual::toast_line(&app, "QuantAlgo · Bot not resumed", &line);
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct KillReport {
    pub on: bool,
    pub bots_stopped: usize,
    pub systems_disarmed: usize,
    pub orders_cancelled: Vec<String>,
    pub errors: Vec<String>,
    /// What stays on the exchanges, per bot or system — nothing is sold.
    pub left_on_exchange: Vec<String>,
}

/// Account and pair of every live Manual order of the last two days: where a
/// QuantSuite order could still rest.
fn live_run_pairs(conn: &Connection) -> Vec<(String, String)> {
    let since = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT r.exchange_id, json_extract(o.record_json, '$.instrument.base') || '/' || json_extract(o.record_json, '$.instrument.quote')
         FROM execution_orders o JOIN execution_runs r ON r.id = o.run_id WHERE r.mode = 'live' AND r.started_at >= ?1",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

fn holdings_report(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT b.name, t.pair, SUM(t.quantity) FROM trades t JOIN bots b ON b.id = t.bot_id
         WHERE t.exit_time IS NULL AND t.is_backtest = 0 AND b.trading_mode = 'live' GROUP BY b.name, t.pair",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))) {
            for (name, pair, qty) in rows.flatten() {
                out.push(format!("{} {qty} (bot {name})", pair.split('/').next().unwrap_or(&pair)));
            }
        }
    }
    if let Ok(mut stmt) = conn.prepare(
        "SELECT h.owner, h.asset, h.qty FROM execution_holdings h
         JOIN manual_execution m ON 'system:' || m.system_id = h.owner
         WHERE json_extract(m.settings_json, '$.mode') = 'live' AND h.asset != 'USDT'",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))) {
            for (owner, asset, qty) in rows.flatten() {
                out.push(format!("{asset} {qty} ({owner})"));
            }
        }
    }
    out
}

/// "Stop all trading" on or off. `external`: it stops live bots and Manual
/// execution. Turning it off re-arms nothing and starts nothing.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn trading_kill_switch(on: bool, app: AppHandle) -> Result<KillReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::execution::store::set_kill_switch(&db, on)?;
        }
        if !on {
            crate::logs::push_bot_log(&app, "warn", "Stop all trading is off; nothing was re-armed or restarted.");
            return Ok(KillReport { on, bots_stopped: 0, systems_disarmed: 0, orders_cancelled: Vec::new(), errors: Vec::new(), left_on_exchange: Vec::new() });
        }
        // Every bot stops; the live ones' pairs are where orders could rest.
        let running: Vec<Bot> = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::bots::load_bots(&db)?.into_iter().filter(|b| b.status == "running").collect()
        };
        let mut errors = Vec::new();
        for bot in &running {
            if let Err(e) = crate::bot::stop_bot_inner(&app, &state, &bot.id) {
                errors.push(format!("stop {}: {e}", bot.name));
            }
        }
        let systems_disarmed = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::manual::disarm_all(&db)?
        };
        // Cancel QuantSuite's open orders per live account and pair.
        let mut by_account: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for bot in running.iter().filter(|b| b.trading_mode == "live") {
            by_account.entry(bot.exchange_id.clone()).or_default().push(bot.pair.clone());
        }
        let recent = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            live_run_pairs(&db)
        };
        for (ex, pair) in recent {
            by_account.entry(ex).or_default().push(pair);
        }
        let mut cancelled = Vec::new();
        for (exchange_id, mut pairs) in by_account {
            pairs.sort();
            pairs.dedup();
            let venue = {
                let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
                crate::exchanges::load_credentials(&db, &exchange_id).and_then(|c| {
                    crate::execution::cex::CexVenue::new(&c.provider, &c.api_key, &c.api_secret, c.sandbox)
                })
            };
            let venue = match venue {
                Ok(v) => v,
                Err(e) => {
                    errors.push(format!("account {exchange_id}: {e}"));
                    continue;
                }
            };
            let instruments: Vec<Instrument> = pairs.iter().filter_map(|p| venue.instrument(p).ok()).collect();
            let (c, e) = cancel_open_orders(&venue, &instruments);
            cancelled.extend(c);
            errors.extend(e);
        }
        let left_on_exchange = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            holdings_report(&db)
        };
        let line = format!(
            "Stop all trading: {} bot(s) stopped, {systems_disarmed} system(s) disarmed, {} order(s) cancelled. Nothing was sold; still held: {}",
            running.len(),
            cancelled.len(),
            if left_on_exchange.is_empty() { "nothing QuantSuite bought".to_string() } else { left_on_exchange.join(", ") }
        );
        crate::logs::push_bot_log(&app, "error", line.clone());
        crate::manual::toast_line(&app, "QuantAlgo · Stop all trading", &line);
        crate::manual::wake();
        let _ = app.emit("algo:kill-switch", json!({ "on": true }));
        Ok(KillReport {
            on,
            bots_stopped: running.len(),
            systems_disarmed,
            orders_cancelled: cancelled,
            errors,
            left_on_exchange,
        })
    })
    .await
    .map_err(|e| format!("Stop all trading failed: {e}"))?
}

#[tauri::command(async)]
pub(crate) fn trading_kill_switch_status(state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    crate::execution::store::kill_switch_on(&db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::venue::*;
    use std::sync::Mutex;

    fn bot() -> Bot {
        Bot {
            id: "b1".into(),
            name: "Bot".into(),
            strategy_id: "s".into(),
            exchange_id: "x".into(),
            pair: "BTC/USDT".into(),
            timeframe: "1h".into(),
            trading_mode: "live".into(),
            budget: 1_000.0,
            status: "running".into(),
            started_at: Some("2026-11-04T00:00:00+00:00".into()),
            stopped_at: None,
            last_error: None,
            config_json: Some(r#"{"resume_after_restart": true}"#.into()),
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn the_books_come_back_from_the_journal() {
        let conn = db();
        // Before the session (ignored), one closed (+10), one open long.
        for (id, entry, exit, pnl, qty, price, fee, notes) in [
            ("old", "2026-11-03T10:00:00+00:00", None, None, 0.01, 50_000.0, 0.5, None),
            ("c1", "2026-11-04T01:00:00+00:00", Some("2026-11-04T02:00:00+00:00"), Some(10.0), 0.001, 60_000.0, 0.06, None),
            ("o1", "2026-11-04T03:00:00+00:00", None, None, 0.002, 64_000.0, 0.128, Some("live · entry order 42")),
        ] {
            conn.execute(
                "INSERT INTO trades (id, bot_id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, fee, is_backtest, notes, created_at, fees_json)
                 VALUES (?1, 'b1', 's', 'x', 'BTC/USDT', 'long', ?2, NULL, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?4, '[{\"asset\":\"BTC\",\"amount\":0.000002}]')",
                params![id, price, qty, entry, exit, pnl, fee, notes],
            )
            .unwrap();
        }
        let books = restore_books(&conn, &bot(), "2026-11-04T00:00:00+00:00").unwrap();
        assert_eq!(books.positions.len(), 1);
        let p = &books.positions[0];
        assert_eq!((p.id.as_str(), p.quantity, p.entry_order_id.as_deref()), ("o1", 0.002, Some("42")));
        assert_eq!(p.entry_fees, vec![ChargedFee { asset: "BTC".into(), amount: 0.000002 }]);
        assert!((books.balance - (1_000.0 + 10.0 - 128.0 - 0.128)).abs() < 1e-9, "{}", books.balance);
        assert!((books.long_qty() - 0.002).abs() < 1e-12);
    }

    #[test]
    fn reconcile_matches_or_keeps_the_bot_stopped() {
        let books = Books {
            balance: 0.0,
            positions: vec![Position {
                id: "p".into(),
                pair: "BTC/USDT".into(),
                side: "long".into(),
                entry_price: 1.0,
                quantity: 0.002,
                entry_fee: 0.0,
                reserved_margin: 0.0,
                entry_time: "t".into(),
                entry_order_id: None,
                entry_fees: Vec::new(),
            }],
        };
        assert!(reconcile(&books, "BTC", 0.002, &[]).is_ok());
        assert!(reconcile(&books, "BTC", 0.5, &[]).is_ok(), "other holdings on the account are fine");
        assert!(reconcile(&books, "BTC", 0.0019985, &[]).is_ok(), "lot rounding");
        let err = reconcile(&books, "BTC", 0.001, &[]).unwrap_err();
        assert!(err.contains("the venue holds 0.001 BTC"), "{err}");
        let err = reconcile(&books, "BTC", 0.002, &["qsb1234-0007".into()]).unwrap_err();
        assert!(err.contains("still open"), "{err}");
    }

    #[test]
    fn a_feed_without_a_new_candle_for_two_bars_is_stale() {
        assert!(!feed_stale(2 * 3_600, "1h"));
        assert!(feed_stale(2 * 3_600 + 1, "1h"));
        assert!(!feed_stale(30 * 3_600, "1d"));
        assert!(feed_stale(49 * 3_600, "1d"));
        assert_eq!(bar_seconds("4h"), 14_400);
    }

    /// Open orders on a venue; records every cancel and every place.
    struct Venue {
        open: Vec<String>,
        cancelled: Mutex<Vec<String>>,
        placed: Mutex<usize>,
    }

    impl ExecutionVenue for Venue {
        fn label(&self) -> String { "mock".into() }
        fn capabilities(&self) -> Capabilities { Capabilities::default() }
        fn ready(&self) -> Result<(), String> { Ok(()) }
        fn instruments(&self) -> Result<Vec<Instrument>, String> { Ok(Vec::new()) }
        fn instrument(&self, _p: &str) -> Result<Instrument, String> { Err("n/a".into()) }
        fn balances(&self) -> Result<Vec<VenueBalance>, String> { Ok(Vec::new()) }
        fn positions(&self) -> Result<Vec<VenuePosition>, String> { Ok(Vec::new()) }
        fn order_book(&self, _i: &Instrument) -> Result<OrderBook, String> { Ok(OrderBook::default()) }
        fn fee_schedule(&self, _i: &Instrument) -> Result<FeeSchedule, String> { Ok(FeeSchedule { maker: 0.0, taker: 0.0 }) }
        fn place(&self, _o: &OrderRequest) -> Result<OrderReport, String> {
            *self.placed.lock().unwrap() += 1;
            Err("the kill switch must never place an order".into())
        }
        fn cancel(&self, _i: &Instrument, id: &str) -> Result<(), String> {
            self.cancelled.lock().unwrap().push(id.into());
            Ok(())
        }
        fn query_by_client_id(&self, _i: &Instrument, _id: &str) -> Result<Option<OrderReport>, String> { Ok(None) }
        fn test_order(&self, _o: &OrderRequest) -> Result<String, String> { Ok("ok".into()) }
        fn open_orders(&self, _i: &Instrument) -> Result<Vec<String>, String> { Ok(self.open.clone()) }
    }

    #[test]
    fn the_kill_switch_cancels_our_orders_and_never_sells() {
        let venue = Venue {
            open: vec!["qsb1234-0001".into(), "users-own-order".into(), "qs3f9a-02".into()],
            cancelled: Mutex::new(Vec::new()),
            placed: Mutex::new(0),
        };
        let inst = crate::execution::route::tests::inst("BTC", "USDT", 0.01, 0.00001);
        let (cancelled, errors) = cancel_open_orders(&venue, &[inst]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(cancelled.len(), 2);
        assert_eq!(*venue.cancelled.lock().unwrap(), vec!["qsb1234-0001", "qs3f9a-02"], "the user's own order stays");
        assert_eq!(*venue.placed.lock().unwrap(), 0, "nothing was sold");
    }

    #[test]
    fn only_bots_that_asked_resume() {
        let conn = db();
        for (id, config) in [("b1", Some(r#"{"resume_after_restart": true}"#)), ("b2", None), ("b3", Some(r#"{"resume_after_restart": false}"#))] {
            conn.execute(
                "INSERT INTO bots (id, name, strategy_id, exchange_id, pair, timeframe, trading_mode, budget, status, started_at, config_json, created_at, updated_at)
                 VALUES (?1, ?1, 's', 'x', 'BTC/USDT', '1h', 'live', 1000, 'running', '2026-11-04T00:00:00+00:00', ?2, 't', 't')",
                params![id, config],
            )
            .unwrap();
        }
        let ids: Vec<String> = resumable(&conn).into_iter().map(|(b, _)| b.id).collect();
        assert_eq!(ids, vec!["b1"]);
    }
}
