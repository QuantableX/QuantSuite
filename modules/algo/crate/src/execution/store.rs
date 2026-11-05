//! Where the execution core keeps what it did (PLAN-AUTO-EXECUTION §2.2
//! steps 5, 7 and 8): run records, every order before and after it is sent,
//! what each owner holds on each account, the kill switch, and the journal
//! rows of system runs. SQLite tables in quantalgo.db (created in
//! `db::init_db`); the engine sees them through [`RunStore`] so its tests can
//! use an in-memory database.

use std::collections::BTreeMap;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::guards::GuardResult;
use super::route::RouteChoice;
use super::venue::{Instrument, OrderReport, Side};

/// What an owner holds on an account: asset → quantity (quote included).
pub type Holdings = BTreeMap<String, f64>;

/// What a run should make true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "asset", rename_all = "snake_case")]
pub enum Target {
    /// Everything in the account's quote (USD).
    Flat,
    /// All of the owner's capital in this asset: bought on spot, a long on
    /// perp (at the run's leverage).
    Hold(String),
    /// A short of this asset at the run's leverage — perp only (Venues 4/5).
    Short(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    /// The target holds (with or without orders).
    Done,
    GuardStopped,
    /// Partial fills stayed after the retries; the rest is reported.
    Partial,
    /// The venue refused an order or could not be read.
    Failed,
    /// An order's outcome is unknown (venue unreachable); resume settles it.
    Interrupted,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Done => "done",
            RunStatus::GuardStopped => "guard_stopped",
            RunStatus::Partial => "partial",
            RunStatus::Failed => "failed",
            RunStatus::Interrupted => "interrupted",
        }
    }

    /// Final states are never run again; `Running` and `Interrupted` resume.
    pub fn is_final(self) -> bool {
        // A partial or failed run has not done what the signal said: it goes on
        // while its window is open.
        !matches!(self, RunStatus::Running | RunStatus::Interrupted | RunStatus::Partial | RunStatus::Failed)
    }
}

/// One run: the signal snapshot, plan, route, guard results, orders (in
/// their own table), holdings before and after, errors and timings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub owner: String,
    pub exchange_id: String,
    pub mode: String,
    #[serde(default)]
    pub close_date: Option<String>,
    pub status: RunStatus,
    pub target: Target,
    pub quote: String,
    #[serde(default)]
    pub signal: Value,
    #[serde(default)]
    pub reference_prices: BTreeMap<String, f64>,
    #[serde(default)]
    pub route: Option<RouteChoice>,
    #[serde(default)]
    pub guards: Vec<GuardResult>,
    #[serde(default)]
    pub holdings_before: Option<Holdings>,
    #[serde(default)]
    pub holdings_after: Option<Holdings>,
    /// Amounts too small for the venue, left as they are.
    #[serde(default)]
    pub dust: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    /// Fees plus spread/slippage against the reference prices, in the quote.
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub fees: f64,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    /// Milliseconds per phase.
    #[serde(default)]
    pub timings: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    /// Written before the call: on a crash the venue is asked by client id.
    Sending,
    /// The venue answered; `report` is set.
    Done,
    /// The venue never saw it (rejected, or the call did not get through).
    NotSent,
    /// Sent, outcome unknown (venue unreachable while asking).
    Unknown,
}

/// One order of a run, written before it is sent and updated after.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub client_order_id: String,
    pub run_id: String,
    pub seq: u32,
    /// `sell:BTC`, `buy:ETH`, `direct:BTC>ETH`.
    pub leg: String,
    pub instrument: Instrument,
    pub side: Side,
    pub qty: f64,
    pub limit_price: f64,
    pub state: OrderState,
    #[serde(default)]
    pub report: Option<OrderReport>,
    #[serde(default)]
    pub error: Option<String>,
    /// Value in the account's quote at the time of sending.
    pub value: f64,
    /// USD (account quote) per unit of the instrument's quote.
    pub quote_usd: f64,
    pub sent_at: String,
    pub updated_at: String,
}

impl OrderRecord {
    pub fn filled(&self) -> Option<&OrderReport> {
        match (&self.state, &self.report) {
            (OrderState::Done, Some(r)) if r.filled_qty > 0.0 => Some(r),
            _ => None,
        }
    }
}

/// Persistence for the engine.
pub trait RunStore {
    fn load_run(&self, run_id: &str) -> Result<Option<RunRecord>, String>;
    fn save_run(&self, run: &RunRecord) -> Result<(), String>;
    fn orders(&self, run_id: &str) -> Result<Vec<OrderRecord>, String>;
    fn save_order(&self, order: &OrderRecord, exchange_id: &str) -> Result<(), String>;
    fn holdings(&self, owner: &str, exchange_id: &str) -> Result<Holdings, String>;
    fn set_holdings(&self, owner: &str, exchange_id: &str, holdings: &Holdings) -> Result<(), String>;
    fn kill_switch(&self) -> Result<bool, String>;
    /// Value filled on the account on a UTC day (`YYYY-MM-DD`).
    fn turnover(&self, exchange_id: &str, day: &str) -> Result<f64, String>;
    /// Journal rows for a fill (PLAN-AUTO-EXECUTION §2.2 step 7).
    fn book_fill(&self, booking: &FillBooking) -> Result<(), String>;
}

/// A fill as the journal sees it: per asset, what moved at which USD price.
#[derive(Debug, Clone)]
pub struct FillBooking<'a> {
    pub run_id: &'a str,
    pub owner: &'a str,
    pub exchange_id: &'a str,
    pub mode: &'a str,
    pub account_quote: &'a str,
    pub instrument: &'a Instrument,
    pub side: Side,
    pub report: &'a OrderReport,
    /// USD per unit of the instrument's quote (1 for the account quote).
    pub quote_usd: f64,
    pub at: &'a str,
}

pub struct SqliteStore<'a> {
    pub conn: &'a Mutex<Connection>,
}

fn lock(conn: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
    conn.lock().map_err(|e| format!("Lock: {e}"))
}

fn db_err(e: rusqlite::Error) -> String {
    format!("execution store: {e}")
}

/// The execution tables. Idempotent; part of `db::init_db`.
pub fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS execution_runs (
            id TEXT PRIMARY KEY,
            owner TEXT NOT NULL,
            exchange_id TEXT NOT NULL,
            mode TEXT NOT NULL,
            close_date TEXT,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            record_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS execution_runs_owner ON execution_runs (owner, started_at);
        CREATE UNIQUE INDEX IF NOT EXISTS execution_runs_once
            ON execution_runs (owner, close_date) WHERE close_date IS NOT NULL;

        CREATE TABLE IF NOT EXISTS execution_orders (
            client_order_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            exchange_id TEXT NOT NULL,
            day TEXT NOT NULL,
            filled_value REAL NOT NULL DEFAULT 0,
            record_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS execution_orders_run ON execution_orders (run_id, seq);
        CREATE INDEX IF NOT EXISTS execution_orders_day ON execution_orders (exchange_id, day);

        CREATE TABLE IF NOT EXISTS execution_holdings (
            owner TEXT NOT NULL,
            exchange_id TEXT NOT NULL,
            asset TEXT NOT NULL,
            qty REAL NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (owner, exchange_id, asset)
        );

        CREATE TABLE IF NOT EXISTS execution_flags (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS paper_balances (
            owner TEXT NOT NULL,
            venue TEXT NOT NULL,
            asset TEXT NOT NULL,
            qty REAL NOT NULL,
            PRIMARY KEY (owner, venue, asset)
        );
        CREATE TABLE IF NOT EXISTS paper_orders (
            client_order_id TEXT PRIMARY KEY,
            owner TEXT NOT NULL,
            report_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        ",
    )
}

/// "Stop all trading" (Automation 4/5).
pub fn set_kill_switch(conn: &Connection, on: bool) -> Result<(), String> {
    conn.execute(
        "INSERT INTO execution_flags (key, value) VALUES ('kill_switch', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![if on { "1" } else { "0" }],
    )
    .map(|_| ())
    .map_err(db_err)
}

pub fn kill_switch_on(conn: &Connection) -> Result<bool, String> {
    conn.query_row("SELECT value FROM execution_flags WHERE key = 'kill_switch'", [], |r| r.get::<_, String>(0))
        .optional()
        .map(|v| v.as_deref() == Some("1"))
        .map_err(db_err)
}

impl RunStore for SqliteStore<'_> {
    fn load_run(&self, run_id: &str) -> Result<Option<RunRecord>, String> {
        let conn = lock(self.conn)?;
        let json: Option<String> = conn
            .query_row("SELECT record_json FROM execution_runs WHERE id = ?1", params![run_id], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        json.map(|j| serde_json::from_str(&j).map_err(|e| format!("run record {run_id}: {e}"))).transpose()
    }

    fn save_run(&self, run: &RunRecord) -> Result<(), String> {
        let json = serde_json::to_string(run).map_err(|e| format!("run record: {e}"))?;
        let conn = lock(self.conn)?;
        conn.execute(
            "INSERT INTO execution_runs (id, owner, exchange_id, mode, close_date, status, started_at, finished_at, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET status = excluded.status, finished_at = excluded.finished_at,
               record_json = excluded.record_json",
            params![
                run.id,
                run.owner,
                run.exchange_id,
                run.mode,
                run.close_date,
                run.status.as_str(),
                run.started_at,
                run.finished_at,
                json
            ],
        )
        .map(|_| ())
        .map_err(db_err)
    }

    fn orders(&self, run_id: &str) -> Result<Vec<OrderRecord>, String> {
        let conn = lock(self.conn)?;
        let mut stmt = conn
            .prepare("SELECT record_json FROM execution_orders WHERE run_id = ?1 ORDER BY seq")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![run_id], |r| r.get::<_, String>(0))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        rows.iter()
            .map(|j| serde_json::from_str(j).map_err(|e| format!("order record: {e}")))
            .collect()
    }

    fn save_order(&self, order: &OrderRecord, exchange_id: &str) -> Result<(), String> {
        let json = serde_json::to_string(order).map_err(|e| format!("order record: {e}"))?;
        let day = order.sent_at.get(..10).unwrap_or("").to_string();
        let filled_value = order.filled().map(|r| r.quote_amount * order.quote_usd).unwrap_or(0.0);
        let conn = lock(self.conn)?;
        conn.execute(
            "INSERT INTO execution_orders (client_order_id, run_id, seq, exchange_id, day, filled_value, record_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(client_order_id) DO UPDATE SET filled_value = excluded.filled_value, record_json = excluded.record_json",
            params![order.client_order_id, order.run_id, order.seq, exchange_id, day, filled_value, json],
        )
        .map(|_| ())
        .map_err(db_err)
    }

    fn holdings(&self, owner: &str, exchange_id: &str) -> Result<Holdings, String> {
        let conn = lock(self.conn)?;
        let mut stmt = conn
            .prepare("SELECT asset, qty FROM execution_holdings WHERE owner = ?1 AND exchange_id = ?2")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![owner, exchange_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows.into_iter().filter(|(_, q)| q.abs() > 1e-12).collect())
    }

    fn set_holdings(&self, owner: &str, exchange_id: &str, holdings: &Holdings) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut conn = lock(self.conn)?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("DELETE FROM execution_holdings WHERE owner = ?1 AND exchange_id = ?2", params![owner, exchange_id])
            .map_err(db_err)?;
        for (asset, qty) in holdings {
            if qty.abs() > 1e-12 {
                tx.execute(
                    "INSERT INTO execution_holdings (owner, exchange_id, asset, qty, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![owner, exchange_id, asset, qty, now],
                )
                .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)
    }

    fn kill_switch(&self) -> Result<bool, String> {
        let conn = lock(self.conn)?;
        kill_switch_on(&conn)
    }

    fn turnover(&self, exchange_id: &str, day: &str) -> Result<f64, String> {
        let conn = lock(self.conn)?;
        conn.query_row(
            "SELECT COALESCE(SUM(filled_value), 0) FROM execution_orders WHERE exchange_id = ?1 AND day = ?2",
            params![exchange_id, day],
            |r| r.get(0),
        )
        .map_err(db_err)
    }

    fn book_fill(&self, b: &FillBooking) -> Result<(), String> {
        let conn = lock(self.conn)?;
        book_fill(&conn, b)
    }
}

/// Journal rows: an asset bought opens a long row, an asset sold closes the
/// owner's open row of that asset (and reopens what is left). A direct pair
/// moves two assets in one fill: the quote side is booked at its reference.
pub fn book_fill(conn: &Connection, b: &FillBooking) -> Result<(), String> {
    let r = b.report;
    if r.filled_qty <= 0.0 {
        return Ok(());
    }
    let base = b.instrument.base();
    let quote = b.instrument.quote();
    let base_usd = r.avg_price * b.quote_usd;
    let fee_usd = r.fee_quote * b.quote_usd;
    let fees_json = serde_json::to_string(&serde_json::json!([{ "asset": r.fee_asset, "amount": r.fee_amount }]))
        .unwrap_or_default();
    let note = format!("{} · order {} ({})", b.mode, r.venue_order_id, r.client_order_id);
    let perp = b.instrument.market == super::venue::MarketType::Perp;
    match b.side {
        Side::Buy => {
            // On perp a buy first covers the owner's shorts; the rest is long.
            let left = if perp { close_rows(conn, b, base, "short", r.filled_qty, base_usd, fee_usd, &fees_json, &note)? } else { r.filled_qty };
            if left > 1e-12 {
                let share = if r.filled_qty > 0.0 { left / r.filled_qty } else { 1.0 };
                open_row(conn, b, base, "long", r.base_received * share, base_usd, fee_usd * share, &fees_json, &note)?;
            }
            if !quote.eq_ignore_ascii_case(b.account_quote) {
                close_rows(conn, b, quote, "long", r.quote_amount, b.quote_usd, 0.0, &fees_json, &note)?;
            }
        }
        Side::Sell => {
            let left = close_rows(conn, b, base, "long", r.filled_qty, base_usd, fee_usd, &fees_json, &note)?;
            // On perp what a sell does beyond the longs opens a short; on
            // spot it is lot rounding.
            if perp && left > 1e-12 {
                let share = left / r.filled_qty.max(1e-18);
                open_row(conn, b, base, "short", left, base_usd, fee_usd * share, &fees_json, &note)?;
            }
            if !quote.eq_ignore_ascii_case(b.account_quote) {
                let received = r.quote_amount - r.fee_in(quote);
                open_row(conn, b, quote, "long", received, b.quote_usd, 0.0, &fees_json, &note)?;
            }
        }
    }
    Ok(())
}

/// A funding payment as its own journal row (side `funding`, Venues 4/5):
/// `usd` is what the owner received (negative: paid). Idempotent by `key`.
#[allow(clippy::too_many_arguments)]
pub fn book_funding(
    conn: &Connection,
    owner: &str,
    bot_id: Option<&str>,
    exchange_id: &str,
    pair: &str,
    size: f64,
    usd: f64,
    rate: f64,
    at: &str,
    key: &str,
) -> Result<bool, String> {
    let source = format!("funding:{key}");
    let exists: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM trades WHERE source = ?1)", params![source], |r| r.get(0))
        .map_err(db_err)?;
    if exists {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, exit_price, quantity, entry_time, exit_time, pnl, pnl_pct, fee, is_backtest, notes, created_at, source, mode, bot_id)
         VALUES (?1, ?2, ?3, ?4, 'funding', 0, 0, ?5, ?6, ?6, ?7, ?8, 0, 0, ?9, ?6, ?10, 'live', ?11)",
        params![
            uuid::Uuid::new_v4().to_string(),
            owner,
            exchange_id,
            pair,
            size,
            at,
            usd,
            rate * 100.0,
            format!("funding {:+.4} % per hour on {size}", rate * 100.0),
            source,
            bot_id,
        ],
    )
    .map_err(db_err)?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn open_row(
    conn: &Connection,
    b: &FillBooking,
    asset: &str,
    side: &str,
    qty: f64,
    price_usd: f64,
    fee_usd: f64,
    fees_json: &str,
    note: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, quantity, entry_time, fee, is_backtest, notes, created_at, source, fees_json, mode)
         VALUES (?1, ?2, ?3, ?4, ?13, ?5, ?6, ?7, ?8, 0, ?9, ?7, ?10, ?11, ?12)",
        params![
            uuid::Uuid::new_v4().to_string(),
            b.owner,
            b.exchange_id,
            format!("{asset}/{}", b.account_quote),
            price_usd,
            qty,
            b.at,
            fee_usd,
            note,
            format!("run:{}", b.run_id),
            fees_json,
            b.mode,
            side,
        ],
    )
    .map(|_| ())
    .map_err(db_err)
}

/// Close the owner's open `side` rows of `asset` with `qty`; returns what
/// was left over (no open row to close).
#[allow(clippy::too_many_arguments)]
fn close_rows(
    conn: &Connection,
    b: &FillBooking,
    asset: &str,
    side: &str,
    mut qty: f64,
    price_usd: f64,
    fee_usd: f64,
    fees_json: &str,
    note: &str,
) -> Result<f64, String> {
    let pair = format!("{asset}/{}", b.account_quote);
    let open: Vec<(String, f64, f64, f64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, entry_price, quantity, fee FROM trades
                 WHERE strategy_id = ?1 AND exchange = ?2 AND pair = ?3 AND side = ?4 AND exit_time IS NULL AND is_backtest = 0
                 ORDER BY entry_time",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![b.owner, b.exchange_id, pair, side], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)?
    };
    let total = qty;
    for (id, entry, row_qty, entry_fee) in open {
        if qty <= 1e-12 {
            break;
        }
        let take = qty.min(row_qty);
        let share_fee = if total > 0.0 { fee_usd * take / total } else { 0.0 };
        let gross = if side == "short" { (entry - price_usd) * take } else { (price_usd - entry) * take };
        let pnl = gross - share_fee - entry_fee * (take / row_qty.max(1e-18));
        let pnl_pct = pnl / (entry * take).max(1e-9) * 100.0;
        conn.execute(
            "UPDATE trades SET exit_price = ?1, exit_time = ?2, quantity = ?3, pnl = ?4, pnl_pct = ?5,
               fee = fee * (?3 / quantity) + ?6, notes = COALESCE(notes, '') || ' → ' || ?7,
               fees_json = ?8 WHERE id = ?9",
            params![price_usd, b.at, take, pnl, pnl_pct, share_fee, note, fees_json, id],
        )
        .map_err(db_err)?;
        if row_qty - take > 1e-12 {
            // What is left of the row stays open at its entry price.
            conn.execute(
                "INSERT INTO trades (id, strategy_id, exchange, pair, side, entry_price, quantity, entry_time, fee, is_backtest, notes, created_at, source, fees_json, mode)
                 SELECT ?1, strategy_id, exchange, pair, side, entry_price, ?2, entry_time, 0, 0, notes, created_at, source, NULL, mode
                 FROM trades WHERE id = ?3",
                params![uuid::Uuid::new_v4().to_string(), row_qty - take, id],
            )
            .map_err(db_err)?;
        }
        qty -= take;
    }
    Ok(qty.max(0.0))
}
