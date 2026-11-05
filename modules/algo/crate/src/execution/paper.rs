//! The paper venue (PLAN-AUTO-EXECUTION §2.2 "Paper"): the same engine, the
//! same route decision and guards — only the last step is simulated. An
//! order fills against the real venue's current book (walked for its size,
//! up to its limit), plus the configured slippage and fee; balances are kept
//! per owner in `paper_balances`, orders in `paper_orders` so a resumed run
//! finds them by client id like on a real venue.

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::venue::{
    Capabilities, ExecutionVenue, FeeSchedule, Instrument, OrderBook, OrderReport, OrderRequest, OrderStatus, Side,
    VenueBalance, VenuePosition,
};

pub struct PaperVenue<'a> {
    /// The real venue whose public book and instruments paper trades against.
    market: &'a dyn ExecutionVenue,
    conn: &'a Mutex<Connection>,
    owner: String,
    /// The account the paper balances belong to (an exchange id).
    venue_key: String,
    /// Fee per fill, charged in the quote asset.
    pub fee_rate: f64,
    /// Extra slippage on top of the walked book price.
    pub slippage: f64,
}

fn db_err(e: rusqlite::Error) -> String {
    format!("paper venue: {e}")
}

impl<'a> PaperVenue<'a> {
    pub fn new(
        market: &'a dyn ExecutionVenue,
        conn: &'a Mutex<Connection>,
        owner: &str,
        venue_key: &str,
        fee_rate: f64,
        slippage: f64,
    ) -> Self {
        Self { market, conn, owner: owner.to_string(), venue_key: venue_key.to_string(), fee_rate, slippage }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.conn.lock().map_err(|e| format!("Lock: {e}"))
    }

    fn balance(conn: &Connection, owner: &str, venue: &str, asset: &str) -> Result<f64, String> {
        conn.query_row(
            "SELECT qty FROM paper_balances WHERE owner = ?1 AND venue = ?2 AND asset = ?3",
            params![owner, venue, asset],
            |r| r.get(0),
        )
        .optional()
        .map(|v| v.unwrap_or(0.0))
        .map_err(db_err)
    }

    fn add(conn: &Connection, owner: &str, venue: &str, asset: &str, delta: f64) -> Result<(), String> {
        conn.execute(
            "INSERT INTO paper_balances (owner, venue, asset, qty) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(owner, venue, asset) DO UPDATE SET qty = qty + excluded.qty",
            params![owner, venue, asset, delta],
        )
        .map_err(db_err)?;
        conn.execute(
            "DELETE FROM paper_balances WHERE owner = ?1 AND venue = ?2 AND asset = ?3 AND ABS(qty) < 1e-12",
            params![owner, venue, asset],
        )
        .map(|_| ())
        .map_err(db_err)
    }

    /// Give a new paper account its starting capital; an account that holds
    /// anything already is left alone. Returns whether it was funded.
    pub fn fund_if_empty(&self, asset: &str, amount: f64) -> Result<bool, String> {
        let conn = self.lock()?;
        let any: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM paper_balances WHERE owner = ?1 AND venue = ?2)",
                params![self.owner, self.venue_key],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if any || amount <= 0.0 {
            return Ok(false);
        }
        Self::add(&conn, &self.owner, &self.venue_key, asset, amount)?;
        Ok(true)
    }

    /// The fill a book gives an IOC order, with the paper slippage on top.
    pub fn simulate(&self, order: &OrderRequest, book: &OrderBook) -> OrderReport {
        let limit = (!order.market).then_some(order.limit_price);
        let walk = match order.side {
            Side::Buy => book.walk_buy_base(order.qty, limit),
            Side::Sell => book.walk_sell_base(order.qty, limit),
        };
        let qty = order.instrument.filters.round_qty_down(walk.base);
        if qty <= 0.0 {
            return OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, "nothing in the book inside the limit");
        }
        let price = match order.side {
            Side::Buy => walk.avg_price() * (1.0 + self.slippage),
            Side::Sell => walk.avg_price() * (1.0 - self.slippage),
        };
        let quote_amount = qty * price;
        let fee = quote_amount * self.fee_rate;
        OrderReport {
            client_order_id: order.client_order_id.clone(),
            venue_order_id: format!("paper-{}", order.client_order_id),
            status: if qty + 1e-12 >= order.qty { OrderStatus::Filled } else { OrderStatus::PartiallyFilled },
            filled_qty: qty,
            base_received: qty,
            avg_price: price,
            quote_amount,
            fee_asset: order.instrument.quote().to_string(),
            fee_amount: fee,
            fee_quote: fee,
            fee_estimated: false,
            message: None,
        }
    }
}

impl ExecutionVenue for PaperVenue<'_> {
    fn label(&self) -> String {
        format!("Paper ({} book)", self.market.label())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { ioc_limit: true, client_order_ids: true, test_orders: true, ..self.market.capabilities() }
    }

    fn ready(&self) -> Result<(), String> {
        Ok(())
    }

    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        self.market.instruments()
    }

    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        self.market.instrument(pair)
    }

    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        let rows = self.paper_rows()?;
        // On perp a negative row is a short: a position, not a balance.
        Ok(rows.into_iter().filter(|b| !self.perp() || b.free >= 0.0).collect())
    }

    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        if !self.perp() {
            return Ok(Vec::new());
        }
        Ok(self
            .paper_rows()?
            .into_iter()
            .filter(|b| !matches!(b.asset.as_str(), "USDC" | "USDT" | "USD") && b.free.abs() > 1e-12)
            .map(|b| VenuePosition {
                symbol: b.asset,
                side: if b.free > 0.0 { "long" } else { "short" }.into(),
                size: b.free.abs(),
                entry_price: 0.0,
                liquidation_price: None,
                mark_price: None,
                leverage: None,
                margin: None,
            })
            .collect())
    }

    fn maintenance_margin(&self, instrument: &Instrument) -> Option<f64> {
        self.market.maintenance_margin(instrument)
    }

    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String> {
        self.market.order_book(instrument)
    }

    fn fee_schedule(&self, _instrument: &Instrument) -> Result<FeeSchedule, String> {
        Ok(FeeSchedule { maker: self.fee_rate, taker: self.fee_rate })
    }

    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        self.place_paper(order)
    }

    fn cancel(&self, _instrument: &Instrument, _client_order_id: &str) -> Result<(), String> {
        // IOC: nothing ever rests in the paper book.
        Ok(())
    }

    fn query_by_client_id(&self, _instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        self.paper_order(client_order_id)
    }

    fn test_order(&self, order: &OrderRequest) -> Result<String, String> {
        Ok(format!("paper: {} {} {} accepted", order.side.as_str(), order.qty, order.instrument.pair()))
    }
}

impl PaperVenue<'_> {
    /// Paper on a perp book: a sell may open a short (Venues 4/5).
    fn perp(&self) -> bool {
        let caps = self.market.capabilities();
        caps.perp && !caps.spot
    }

    fn paper_rows(&self) -> Result<Vec<VenueBalance>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT asset, qty FROM paper_balances WHERE owner = ?1 AND venue = ?2")
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![self.owner, self.venue_key], |r| {
                Ok(VenueBalance { asset: r.get(0)?, free: r.get(1)?, locked: 0.0 })
            })
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    fn place_paper(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        if let Some(existing) = self.paper_order(&order.client_order_id)? {
            return Ok(existing);
        }
        let book = self.market.order_book(&order.instrument)?;
        let report = self.simulate(order, &book);
        let (base, quote) = (order.instrument.base().to_string(), order.instrument.quote().to_string());
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        let report = if report.filled_qty > 0.0 {
            let (have, need, asset) = match order.side {
                Side::Buy => (Self::balance(&tx, &self.owner, &self.venue_key, &quote)?, report.quote_amount + report.fee_amount, &quote),
                Side::Sell => (Self::balance(&tx, &self.owner, &self.venue_key, &base)?, report.filled_qty, &base),
            };
            // On perp a sell opens or adds to a short; the books carry it.
            let shorting = self.perp() && order.side == Side::Sell;
            if need > have + 1e-9 && !shorting {
                OrderReport::empty(
                    &order.client_order_id,
                    OrderStatus::Rejected,
                    format!("insufficient paper balance: {need} {asset} needed, {have} held"),
                )
            } else {
                match order.side {
                    Side::Buy => {
                        Self::add(&tx, &self.owner, &self.venue_key, &quote, -(report.quote_amount + report.fee_amount))?;
                        Self::add(&tx, &self.owner, &self.venue_key, &base, report.base_received)?;
                    }
                    Side::Sell => {
                        Self::add(&tx, &self.owner, &self.venue_key, &base, -report.filled_qty)?;
                        Self::add(&tx, &self.owner, &self.venue_key, &quote, report.quote_amount - report.fee_amount)?;
                    }
                }
                report
            }
        } else {
            report
        };
        tx.execute(
            "INSERT INTO paper_orders (client_order_id, owner, report_json, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                order.client_order_id,
                self.owner,
                serde_json::to_string(&report).map_err(|e| e.to_string())?,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(report)
    }

    fn paper_order(&self, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        let conn = self.lock()?;
        let json: Option<String> = conn
            .query_row("SELECT report_json FROM paper_orders WHERE client_order_id = ?1", params![client_order_id], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        json.map(|j| serde_json::from_str(&j).map_err(|e| format!("paper order: {e}"))).transpose()
    }
}
