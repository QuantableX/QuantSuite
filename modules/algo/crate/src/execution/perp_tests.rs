//! Perp through the execution core on a mock venue (card Venues 4/5):
//! shorts open and close, closes are reduce-only, leverage sizes the
//! position, the liquidation guard refuses a position too close to
//! liquidation, and spot stays long-only.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use rusqlite::Connection;
use serde_json::json;

use super::engine::{Engine, RunRequest};
use super::guards::GuardConfig;
use super::paper::PaperVenue;
use super::route::tests::deep;
use super::store::{RunStatus, RunStore, SqliteStore, Target};
use super::venue::*;
use crate::broker::SymbolFilters;
use crate::runtime::Mode;

/// A perp account in memory: signed positions, cash, reduce-only checks.
struct MockPerp {
    books: HashMap<String, OrderBook>,
    instruments: Vec<Instrument>,
    cash: Mutex<f64>,
    positions: Mutex<HashMap<String, f64>>,
    /// Every order as sent: (side, qty, reduce_only).
    sent: Mutex<Vec<(Side, f64, bool)>>,
}

fn perp_inst(base: &str) -> Instrument {
    Instrument {
        market: MarketType::Perp,
        filters: SymbolFilters {
            symbol: base.into(),
            base: base.into(),
            quote: "USDC".into(),
            qty_step: 0.00001,
            min_qty: 0.00001,
            min_notional: 10.0,
            price_tick: 0.1,
        },
    }
}

impl MockPerp {
    fn new(cash: f64) -> Self {
        let mut books = HashMap::new();
        let mut instruments = Vec::new();
        for (base, mid) in [("BTC", 100_000.0), ("ETH", 4_000.0)] {
            books.insert(base.to_string(), deep(mid, 0.000_01, 30, 5.0));
            instruments.push(perp_inst(base));
        }
        Self { books, instruments, cash: Mutex::new(cash), positions: Mutex::new(HashMap::new()), sent: Mutex::new(Vec::new()) }
    }

    fn position(&self, coin: &str) -> f64 {
        self.positions.lock().unwrap().get(coin).copied().unwrap_or(0.0)
    }
}

impl ExecutionVenue for MockPerp {
    fn label(&self) -> String {
        "Mock perp".into()
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { perp: true, ioc_limit: true, client_order_ids: true, ..Default::default() }
    }
    fn ready(&self) -> Result<(), String> {
        Ok(())
    }
    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        Ok(self.instruments.clone())
    }
    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        self.instruments.iter().find(|i| i.pair() == pair).cloned().ok_or_else(|| format!("no {pair}"))
    }
    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        let mut out = vec![VenueBalance { asset: "USDC".into(), free: *self.cash.lock().unwrap(), locked: 0.0 }];
        for (c, q) in self.positions.lock().unwrap().iter().filter(|(_, q)| **q > 0.0) {
            out.push(VenueBalance { asset: c.clone(), free: *q, locked: 0.0 });
        }
        Ok(out)
    }
    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        Ok(self
            .positions
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, q)| q.abs() > 1e-12)
            .map(|(c, q)| VenuePosition {
                symbol: c.clone(),
                side: if *q > 0.0 { "long" } else { "short" }.into(),
                size: q.abs(),
                entry_price: 0.0,
                liquidation_price: None,
                mark_price: None,
                leverage: None,
                margin: None,
            })
            .collect())
    }
    fn maintenance_margin(&self, _i: &Instrument) -> Option<f64> {
        Some(0.0125)
    }
    fn order_book(&self, i: &Instrument) -> Result<OrderBook, String> {
        self.books.get(&i.filters.symbol).cloned().ok_or_else(|| "no book".into())
    }
    fn fee_schedule(&self, _i: &Instrument) -> Result<FeeSchedule, String> {
        Ok(FeeSchedule { maker: 0.00015, taker: 0.00045 })
    }
    fn place(&self, o: &OrderRequest) -> Result<OrderReport, String> {
        self.sent.lock().unwrap().push((o.side, o.qty, o.reduce_only));
        let coin = o.instrument.base().to_string();
        let held = self.position(&coin);
        let signed = if o.side == Side::Buy { o.qty } else { -o.qty };
        // Hyperliquid's reduceOnlyRejected: a close may not grow or flip.
        if o.reduce_only && (held == 0.0 || held.signum() == signed.signum() || o.qty > held.abs() + 1e-12) {
            return Ok(OrderReport::empty(&o.client_order_id, OrderStatus::Rejected, "reduce only would increase the position"));
        }
        let book = self.order_book(&o.instrument)?;
        let walk = match o.side {
            Side::Buy => book.walk_buy_base(o.qty, Some(o.limit_price)),
            Side::Sell => book.walk_sell_base(o.qty, Some(o.limit_price)),
        };
        let fee = walk.quote * 0.00045;
        *self.positions.lock().unwrap().entry(coin).or_default() += if o.side == Side::Buy { walk.base } else { -walk.base };
        *self.cash.lock().unwrap() -= fee;
        Ok(OrderReport {
            client_order_id: o.client_order_id.clone(),
            venue_order_id: "1".into(),
            status: OrderStatus::Filled,
            filled_qty: walk.base,
            base_received: walk.base,
            avg_price: walk.avg_price(),
            quote_amount: walk.quote,
            fee_asset: "USDC".into(),
            fee_amount: fee,
            fee_quote: fee,
            fee_estimated: false,
            message: None,
        })
    }
    fn cancel(&self, _i: &Instrument, _id: &str) -> Result<(), String> {
        Ok(())
    }
    fn query_by_client_id(&self, _i: &Instrument, _id: &str) -> Result<Option<OrderReport>, String> {
        Ok(None)
    }
    fn test_order(&self, _o: &OrderRequest) -> Result<String, String> {
        Err("none".into())
    }
}

fn db() -> Mutex<Connection> {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::init_db(&conn).unwrap();
    Mutex::new(conn)
}

fn req(run_id: &str, target: Target, leverage: f64) -> RunRequest {
    let refs: BTreeMap<String, f64> = [("BTC", 100_000.0), ("ETH", 4_000.0), ("USDC", 1.0)].into_iter().map(|(a, p)| (a.into(), p)).collect();
    RunRequest {
        run_id: run_id.into(),
        owner: "system:single".into(),
        exchange_id: format!("hl-{run_id}"),
        mode: Mode::Live,
        quote: "USDC".into(),
        target,
        reference_prices: refs,
        data_fresh: true,
        armed: true,
        account_belongs: true,
        close_date: None,
        signal: json!({}),
        initial_capital: Some(10_000.0),
        initial_holdings: None,
        leverage,
    }
}

fn journal(conn: &Mutex<Connection>, side: &str) -> Vec<(f64, Option<f64>)> {
    let c = conn.lock().unwrap();
    let mut stmt = c.prepare("SELECT quantity, pnl FROM trades WHERE side = ?1 ORDER BY entry_time, created_at").unwrap();
    stmt.query_map([side], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn a_short_opens_then_closes_reduce_only_into_a_long() {
    let conn = db();
    let venue = MockPerp::new(10_000.0);
    let store = SqliteStore { conn: &conn };
    let mut r = req("p1", Target::Short("BTC".into()), 1.0);
    let rec = Engine::default().run(&store, &venue, &r).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let short = venue.position("BTC");
    assert!(short < -0.099 && short > -0.1, "1x: about 10,000 USDC of BTC short, {short}");
    assert_eq!(venue.sent.lock().unwrap()[0], (Side::Sell, -short, false), "an opening order is not reduce-only");
    assert!(rec.guards.iter().any(|g| g.name == "liquidation_distance" && g.ok));
    let books = store.holdings("system:single", &r.exchange_id).unwrap();
    assert!((books["BTC"] - short).abs() < 1e-12, "the books carry the short: {books:?}");
    assert_eq!(journal(&conn, "short").len(), 1);

    // The signal turns long: the short is covered reduce-only, then a long opens.
    r.run_id = "p1b".into();
    r.target = Target::Hold("BTC".into());
    let rec = Engine::default().run(&store, &venue, &r).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let sent = venue.sent.lock().unwrap().clone();
    assert_eq!((sent[1].0, sent[1].2), (Side::Buy, true), "the cover is reduce-only");
    assert_eq!((sent[2].0, sent[2].2), (Side::Buy, false), "the long is not");
    let long = venue.position("BTC");
    assert!(long > 0.099, "{long}");
    let closed = journal(&conn, "short");
    assert!(closed[0].1.is_some(), "the short row is closed with its PnL");
    assert_eq!(journal(&conn, "long").len(), 1);

    // Flat: the long closes reduce-only, nothing opens.
    r.run_id = "p1c".into();
    r.target = Target::Flat;
    let rec = Engine::default().run(&store, &venue, &r).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let sent = venue.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 4);
    assert_eq!((sent[3].0, sent[3].2), (Side::Sell, true));
    assert!(venue.position("BTC").abs() < 1e-9);
    let cash = store.holdings("system:single", &r.exchange_id).unwrap()["USDC"];
    assert!(cash > 9_900.0 && cash < 10_000.0, "fees only (same prices): {cash}");
}

#[test]
fn leverage_sizes_the_position() {
    let conn = db();
    let venue = MockPerp::new(10_000.0);
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req("p2", Target::Short("ETH".into()), 3.0)).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let eth = venue.position("ETH");
    assert!(eth < -7.4 && eth > -7.5, "3x of 10,000 USDC at 4,000: about 7.45 ETH short, {eth}");
}

#[test]
fn the_liquidation_guard_refuses_a_position_too_close() {
    let conn = db();
    let venue = MockPerp::new(10_000.0);
    // 3x isolated with 1.25 % maintenance: liquidation 32.1 % away, the user wants 40 %.
    let engine = Engine::new(GuardConfig { min_liquidation_distance: 0.4, ..GuardConfig::default() });
    let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &req("p3", Target::Hold("BTC".into()), 3.0)).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.errors[0].starts_with("liquidation_distance"), "{:?}", rec.errors);
    assert!(venue.sent.lock().unwrap().is_empty(), "nothing was sent");
    // 1x passes the same limit.
    let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &req("p3b", Target::Hold("BTC".into()), 1.0)).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
}

#[test]
fn a_short_needs_a_perp_market() {
    let conn = db();
    let market = MockPerp::new(0.0);
    // Paper over a spot book: no shorts.
    struct Spot<'a>(&'a MockPerp);
    impl ExecutionVenue for Spot<'_> {
        fn label(&self) -> String { "spot".into() }
        fn capabilities(&self) -> Capabilities { Capabilities { spot: true, ioc_limit: true, client_order_ids: true, ..Default::default() } }
        fn ready(&self) -> Result<(), String> { Ok(()) }
        fn instruments(&self) -> Result<Vec<Instrument>, String> { self.0.instruments() }
        fn instrument(&self, p: &str) -> Result<Instrument, String> { self.0.instrument(p).map(|i| Instrument { market: MarketType::Spot, ..i }) }
        fn balances(&self) -> Result<Vec<VenueBalance>, String> { Ok(vec![VenueBalance { asset: "USDC".into(), free: 10_000.0, locked: 0.0 }]) }
        fn positions(&self) -> Result<Vec<VenuePosition>, String> { Ok(Vec::new()) }
        fn order_book(&self, i: &Instrument) -> Result<OrderBook, String> { self.0.order_book(i) }
        fn fee_schedule(&self, i: &Instrument) -> Result<FeeSchedule, String> { self.0.fee_schedule(i) }
        fn place(&self, o: &OrderRequest) -> Result<OrderReport, String> { self.0.place(o) }
        fn cancel(&self, i: &Instrument, id: &str) -> Result<(), String> { self.0.cancel(i, id) }
        fn query_by_client_id(&self, i: &Instrument, id: &str) -> Result<Option<OrderReport>, String> { self.0.query_by_client_id(i, id) }
        fn test_order(&self, o: &OrderRequest) -> Result<String, String> { self.0.test_order(o) }
    }
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &Spot(&market), &req("p4", Target::Short("BTC".into()), 1.0)).unwrap();
    assert_eq!(rec.status, RunStatus::Failed);
    assert!(rec.errors[0].contains("perp"), "{:?}", rec.errors);
    assert!(market.sent.lock().unwrap().is_empty());
}

#[test]
fn paper_shorts_on_a_perp_book() {
    let conn = db();
    let market = MockPerp::new(0.0);
    let paper = PaperVenue::new(&market, &conn, "system:single", "hl-paper", 0.00045, 0.0);
    paper.fund_if_empty("USDC", 10_000.0).unwrap();
    let store = SqliteStore { conn: &conn };
    let mut r = req("p5", Target::Short("BTC".into()), 1.0);
    r.exchange_id = "hl-paper".into();
    r.mode = Mode::Paper;
    let rec = Engine::default().run(&store, &paper, &r).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let short = paper.positions().unwrap();
    assert_eq!(short.len(), 1);
    assert_eq!(short[0].side, "short");
    r.run_id = "p5b".into();
    r.target = Target::Flat;
    let rec = Engine::default().run(&store, &paper, &r).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert!(paper.positions().unwrap().is_empty());
    assert!(market.sent.lock().unwrap().is_empty(), "paper never reaches the venue");
}
