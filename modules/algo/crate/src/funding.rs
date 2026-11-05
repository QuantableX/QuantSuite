//! Funding of perp positions, booked in the journal (PLAN-EXECUTION-VENUES
//! §6, card Venues 4/5). Hyperliquid pays or charges the holder of a perp
//! every hour. QuantSuite reads an account's payments and books each owner's
//! share as its own journal row (side `funding`) and into the owner's cash —
//! a Manual system's books, a running bot's balance — so the equity curve
//! and the stats show what holding really cost.
//!
//! A payment is split over the owners holding the coin on the same side, by
//! their share of the venue's position; what no QuantSuite owner holds (the
//! user's own trades on the account) is not booked. Rows are idempotent per
//! (account, hour, coin, owner).

use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager, Wry};

use crate::execution::store::book_funding;
use crate::venues::hyperliquid::FundingPayment;
use crate::AppState;

/// How often the accounts are read.
const EVERY: Duration = Duration::from_secs(30 * 60);
/// How far back the first read of an account goes.
const FIRST_LOOKBACK_MS: i64 = 3 * 24 * 3_600_000;

/// Who holds a coin on the account: signed quantity (negative = short).
#[derive(Debug, Clone, PartialEq)]
pub struct Holder {
    pub owner: String,
    pub bot_id: Option<String>,
    pub coin: String,
    pub qty: f64,
}

/// One payment over its holders: `(holder index, USD)`. Only holders on the
/// payment's side count, each by its share of the venue's position.
pub fn attribute(p: &FundingPayment, holders: &[Holder]) -> Vec<(usize, f64)> {
    let same: Vec<usize> = holders
        .iter()
        .enumerate()
        .filter(|(_, h)| h.coin.eq_ignore_ascii_case(&p.coin) && h.qty * p.szi > 0.0)
        .map(|(i, _)| i)
        .collect();
    let total: f64 = same.iter().map(|&i| holders[i].qty.abs()).sum();
    let denom = total.max(p.szi.abs());
    if denom <= 0.0 {
        return Vec::new();
    }
    same.into_iter().map(|i| (i, p.usdc * holders[i].qty.abs() / denom)).collect()
}

/// The Manual systems holding coins on an account (their books).
pub fn system_holders(conn: &Connection, exchange_id: &str, quote: &str) -> Result<Vec<Holder>, String> {
    let mut stmt = conn
        .prepare("SELECT owner, asset, qty FROM execution_holdings WHERE exchange_id = ?1 AND asset != ?2")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![exchange_id, quote], |r| {
            Ok(Holder { owner: r.get(0)?, bot_id: None, coin: r.get(1)?, qty: r.get(2)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Book `payments` for `holders`: journal rows, and a Manual owner's books.
/// Returns what each bot received, for its running balance.
pub fn book(
    conn: &Connection,
    exchange_id: &str,
    quote: &str,
    payments: &[FundingPayment],
    holders: &[Holder],
) -> Result<Vec<(String, f64)>, String> {
    let mut to_bots = Vec::new();
    for p in payments {
        let at = chrono::DateTime::from_timestamp_millis(p.time).map(|t| t.to_rfc3339()).unwrap_or_default();
        for (i, usd) in attribute(p, holders) {
            let h = &holders[i];
            let key = format!("{exchange_id}:{}:{}:{}", p.time, p.coin, h.owner);
            let pair = format!("{}/{quote}", p.coin);
            if !book_funding(conn, &h.owner, h.bot_id.as_deref(), exchange_id, &pair, h.qty.abs(), usd, p.rate, &at, &key)? {
                continue;
            }
            match &h.bot_id {
                Some(bot) => to_bots.push((bot.clone(), usd)),
                None => {
                    // The system's cash on the account moves with the payment.
                    let now = chrono::Utc::now().to_rfc3339();
                    let changed = conn
                        .execute(
                            "UPDATE execution_holdings SET qty = qty + ?1, updated_at = ?5 WHERE owner = ?2 AND exchange_id = ?3 AND asset = ?4",
                            params![usd, h.owner, exchange_id, quote, now],
                        )
                        .map_err(|e| e.to_string())?;
                    if changed == 0 {
                        conn.execute(
                            "INSERT INTO execution_holdings (owner, exchange_id, asset, qty, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![h.owner, exchange_id, quote, usd, now],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    Ok(to_bots)
}

fn flag_key(exchange_id: &str) -> String {
    format!("funding_since:{exchange_id}")
}

/// Read and book one perp account's new payments (Hyperliquid, Lighter).
pub fn sync_account(app: &AppHandle<Wry>, exchange_id: &str) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let (creds, since) = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        let since: Option<i64> = db
            .query_row("SELECT value FROM execution_flags WHERE key = ?1", params![flag_key(exchange_id)], |r| r.get::<_, String>(0))
            .optional()
            .map_err(|e| e.to_string())?
            .and_then(|v| v.parse().ok());
        (crate::exchanges::load_credentials(&db, exchange_id)?, since)
    };
    if !crate::venues::is_perp_dex(&creds.provider) {
        return Ok(0);
    }
    let start = since.unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - FIRST_LOOKBACK_MS) - 3_600_000;
    let payments = crate::venues::funding_since(&creds, start)?;
    if payments.is_empty() {
        return Ok(0);
    }
    // Running live perp bots on this account, by their open positions.
    let mut holders = Vec::new();
    if let Ok(bots) = state.bots.lock() {
        for handle in bots.values() {
            let Ok(rt) = handle.runtime.lock() else { continue };
            if !rt.live_perp() || rt.execution_account.as_deref() != Some(exchange_id) {
                continue;
            }
            let coin = rt.base_asset();
            for p in &rt.open_positions {
                let qty = if p.side == "short" { -p.quantity } else { p.quantity };
                holders.push(Holder { owner: rt.strategy_id.clone(), bot_id: Some(rt.bot_id.clone()), coin: coin.clone(), qty });
            }
        }
    }
    let (booked, to_bots) = {
        let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        holders.extend(system_holders(&db, exchange_id, "USDC")?);
        let to_bots = book(&db, exchange_id, "USDC", &payments, &holders)?;
        let newest = payments.iter().map(|p| p.time).max().unwrap_or(start);
        db.execute(
            "INSERT INTO execution_flags (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![flag_key(exchange_id), newest.to_string()],
        )
        .map_err(|e| e.to_string())?;
        (to_bots.len(), to_bots)
    };
    if let Ok(bots) = state.bots.lock() {
        for (bot_id, usd) in &to_bots {
            if let Some(handle) = bots.get(bot_id) {
                if let Ok(mut rt) = handle.runtime.lock() {
                    rt.book_funding(*usd);
                }
            }
        }
    }
    Ok(booked)
}

/// The perp accounts something of QuantSuite holds a position on.
fn accounts_to_read(app: &AppHandle<Wry>) -> Vec<String> {
    let state = app.state::<AppState>();
    let mut ids: Vec<String> = state
        .db
        .lock()
        .ok()
        .and_then(|db| {
            db.prepare(
                "SELECT DISTINCT e.id FROM exchanges e JOIN execution_holdings h ON h.exchange_id = e.id
                 WHERE lower(e.provider) IN ('hyperliquid', 'lighter') AND h.asset != 'USDC'",
            )
            .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
            .ok()
        })
        .unwrap_or_default();
    if let Ok(bots) = state.bots.lock() {
        for handle in bots.values() {
            if let Ok(rt) = handle.runtime.lock() {
                if rt.live_perp() {
                    ids.extend(rt.execution_account.clone());
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Read every account with perp holdings every 30 minutes.
pub fn start(app: AppHandle<Wry>) {
    std::thread::Builder::new()
        .name("algo-funding".into())
        .spawn(move || loop {
            // A PC that became a client stops reading (one trader).
            if qs_core::server::executors_allowed().is_err() {
                std::thread::sleep(EVERY);
                continue;
            }
            for id in accounts_to_read(&app) {
                match sync_account(&app, &id) {
                    Ok(0) => {}
                    Ok(n) => log::info!(target: "algo", "funding: {n} payment share(s) booked on {id}"),
                    Err(e) => log::warn!(target: "algo", "funding on {id}: {e}"),
                }
            }
            std::thread::sleep(EVERY);
        })
        .map(|_| ())
        .unwrap_or_else(|e| log::warn!(target: "algo", "funding thread: {e}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pay(coin: &str, usdc: f64, szi: f64, time: i64) -> FundingPayment {
        FundingPayment { time, coin: coin.into(), usdc, szi, rate: 0.0000125, hash: "0x0".into() }
    }

    fn holder(owner: &str, coin: &str, qty: f64) -> Holder {
        Holder { owner: owner.into(), bot_id: None, coin: coin.into(), qty }
    }

    #[test]
    fn a_payment_splits_over_the_holders_on_its_side() {
        let holders = vec![holder("system:a", "BTC", 0.3), holder("system:b", "BTC", 0.1), holder("system:c", "BTC", -0.2), holder("system:d", "ETH", 1.0)];
        // The account is long 0.5 BTC: 0.1 of it belongs to nobody here.
        let split = attribute(&pay("BTC", -5.0, 0.5, 1), &holders);
        assert_eq!(split.len(), 2);
        assert!((split[0].1 + 3.0).abs() < 1e-12 && (split[1].1 + 1.0).abs() < 1e-12, "{split:?}");
        // A short receives when the rate is positive; only the short holder.
        let split = attribute(&pay("BTC", 2.0, -0.2, 1), &holders);
        assert_eq!(split, vec![(2, 2.0)]);
        assert!(attribute(&pay("SOL", -1.0, 3.0, 1), &holders).is_empty());
    }

    #[test]
    fn funding_is_booked_once_into_the_journal_and_the_books() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO execution_holdings (owner, exchange_id, asset, qty, updated_at) VALUES ('system:lces', 'hl', 'BTC', -0.1, 't'), ('system:lces', 'hl', 'USDC', 20000.0, 't')",
            [],
        )
        .unwrap();
        let holders = system_holders(&conn, "hl", "USDC").unwrap();
        assert_eq!(holders, vec![holder("system:lces", "BTC", -0.1)]);
        let payments = vec![pay("BTC", 1.25, -0.1, 1_800_000_000_000), pay("BTC", -0.5, -0.1, 1_800_003_600_000)];
        book(&conn, "hl", "USDC", &payments, &holders).unwrap();
        book(&conn, "hl", "USDC", &payments, &holders).unwrap();
        let (rows, total): (i64, f64) = conn
            .query_row("SELECT COUNT(*), SUM(pnl) FROM trades WHERE side = 'funding' AND strategy_id = 'system:lces'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(rows, 2, "idempotent per hour");
        assert!((total - 0.75).abs() < 1e-12);
        let cash: f64 = conn
            .query_row("SELECT qty FROM execution_holdings WHERE owner = 'system:lces' AND asset = 'USDC'", [], |r| r.get(0))
            .unwrap();
        assert!((cash - 20_000.75).abs() < 1e-9, "{cash}");
        // Bots get theirs back for the running balance.
        let bot = Holder { owner: "strat".into(), bot_id: Some("bot-1".into()), coin: "BTC".into(), qty: -0.1 };
        let to_bots = book(&conn, "hl", "USDC", &[pay("BTC", 0.4, -0.2, 1_800_007_200_000)], &[holders[0].clone(), bot]).unwrap();
        assert_eq!(to_bots.len(), 1);
        assert_eq!(to_bots[0].0, "bot-1");
        assert!((to_bots[0].1 - 0.2).abs() < 1e-12, "{to_bots:?}");
    }

    #[test]
    fn funding_rows_count_in_pnl_but_are_not_trades() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        book(&conn, "hl", "USDC", &[pay("BTC", -2.0, 0.1, 1_800_000_000_000)], &[holder("system:x", "BTC", 0.1)]).unwrap();
        let trades = crate::journal::query_trades(&conn, &crate::TradeFilters::default()).unwrap();
        let stats = crate::journal::compute_stats(&trades);
        assert_eq!(stats.total_trades, 0.0, "{stats:?}");
        assert!((stats.total_pnl + 2.0).abs() < 1e-12);
        assert!((stats.funding + 2.0).abs() < 1e-12);
    }
}
