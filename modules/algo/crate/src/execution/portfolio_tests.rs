//! Portfolio mode through the execution core on a mock spot venue (card
//! 791a9140): several assets by weight, sells before buys, only what has to
//! move, minimums never rounded up, a refused or partly filled leg, a restart
//! mid-run, the guards per asset and across legs, "Stop all trading" during a
//! run, the capital modes, perp refused, and the backtest's fees against
//! what the legs pay.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use rusqlite::Connection;
use serde_json::json;

use super::engine::{Engine, RunRequest};
use super::guards::GuardConfig;
use super::paper::PaperVenue;
use super::route::tests::{deep, inst};
use super::store::{self, OrderState, PortfolioTarget, RunStatus, RunStore, SqliteStore, Target};
use super::venue::*;
use crate::runtime::Mode;

const BOOK_HALF_SPREAD: f64 = 0.000_01;
const FEE: f64 = 0.001;

/// A spot venue in memory: real book walks and balances, per-pair partial
/// fills and refusals, a lost answer, and a kill switch thrown mid-run.
struct MockSpot<'a> {
    books: HashMap<String, OrderBook>,
    instruments: Vec<Instrument>,
    balances: Mutex<HashMap<String, f64>>,
    orders: Mutex<HashMap<String, OrderReport>>,
    placed: Mutex<Vec<(String, Side, String)>>,
    /// Fill only this share of each order on these pairs (IOC partial).
    fill_share: Mutex<HashMap<String, f64>>,
    /// The venue rejects every order on these pairs.
    refuse: Mutex<HashSet<String>>,
    /// The n-th `place` fills but its answer is lost and the venue cannot be asked once.
    lose_answer_on: Option<usize>,
    query_failures: Mutex<usize>,
    /// "Stop all trading" is turned on right after the n-th order.
    kill_after: Option<(usize, &'a Mutex<Connection>)>,
    perp: bool,
}

impl<'a> MockSpot<'a> {
    fn new() -> Self {
        let mut v = MockSpot {
            books: HashMap::new(),
            instruments: Vec::new(),
            balances: Mutex::new(HashMap::new()),
            orders: Mutex::new(HashMap::new()),
            placed: Mutex::new(Vec::new()),
            fill_share: Mutex::new(HashMap::new()),
            refuse: Mutex::new(HashSet::new()),
            lose_answer_on: None,
            query_failures: Mutex::new(0),
            kill_after: None,
            perp: false,
        };
        for (base, mid, size) in [("BTC", 100_000.0, 2.0), ("ETH", 4_000.0, 50.0), ("SOL", 200.0, 1_000.0)] {
            let i = inst(base, "USDT", 0.01, 0.00001);
            v.books.insert(i.filters.symbol.clone(), deep(mid, BOOK_HALF_SPREAD, 30, size));
            v.instruments.push(i);
        }
        v
    }

    fn fund(&self, asset: &str, qty: f64) {
        *self.balances.lock().unwrap().entry(asset.into()).or_default() += qty;
    }

    fn calls(&self) -> usize {
        self.placed.lock().unwrap().len()
    }

    /// The legs as sent: "buy BTCUSDT".
    fn sent(&self) -> Vec<String> {
        self.placed.lock().unwrap().iter().map(|(_, side, sym)| format!("{} {sym}", if *side == Side::Buy { "buy" } else { "sell" })).collect()
    }
}

impl ExecutionVenue for MockSpot<'_> {
    fn label(&self) -> String {
        "MockSpot".into()
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { spot: !self.perp, perp: self.perp, ioc_limit: true, client_order_ids: true, market_orders: true, ..Default::default() }
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
        Ok(self.balances.lock().unwrap().iter().map(|(a, q)| VenueBalance { asset: a.clone(), free: *q, locked: 0.0 }).collect())
    }
    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        Ok(Vec::new())
    }
    fn order_book(&self, i: &Instrument) -> Result<OrderBook, String> {
        self.books.get(&i.filters.symbol).cloned().ok_or_else(|| "no book".into())
    }
    fn fee_schedule(&self, _i: &Instrument) -> Result<FeeSchedule, String> {
        Ok(FeeSchedule { maker: FEE, taker: FEE })
    }
    fn place(&self, o: &OrderRequest) -> Result<OrderReport, String> {
        let symbol = o.instrument.filters.symbol.clone();
        let n = {
            let mut placed = self.placed.lock().unwrap();
            placed.push((o.client_order_id.clone(), o.side, symbol.clone()));
            placed.len()
        };
        if self.refuse.lock().unwrap().contains(&symbol) {
            return Ok(OrderReport::empty(&o.client_order_id, OrderStatus::Rejected, "this pair is in reduce-only mode"));
        }
        let book = self.order_book(&o.instrument)?;
        let share = self.fill_share.lock().unwrap().get(&symbol).copied().unwrap_or(1.0);
        let want = o.instrument.filters.round_qty_down(o.qty * share);
        let walk = match o.side {
            Side::Buy => book.walk_buy_base(want, None),
            Side::Sell => book.walk_sell_base(want, None),
        };
        let (base, quote) = (o.instrument.base().to_string(), o.instrument.quote().to_string());
        let fee = walk.quote * FEE;
        {
            let mut b = self.balances.lock().unwrap();
            match o.side {
                Side::Buy => {
                    *b.entry(quote.clone()).or_default() -= walk.quote + fee;
                    *b.entry(base.clone()).or_default() += walk.base;
                }
                Side::Sell => {
                    *b.entry(base.clone()).or_default() -= walk.base;
                    *b.entry(quote.clone()).or_default() += walk.quote - fee;
                }
            }
        }
        let report = OrderReport {
            client_order_id: o.client_order_id.clone(),
            venue_order_id: format!("v{n}"),
            status: if walk.base + 1e-12 >= o.qty { OrderStatus::Filled } else { OrderStatus::PartiallyFilled },
            filled_qty: walk.base,
            base_received: walk.base,
            avg_price: walk.avg_price(),
            quote_amount: walk.quote,
            fee_asset: quote,
            fee_amount: fee,
            fee_quote: fee,
            fee_estimated: false,
            message: None,
        };
        self.orders.lock().unwrap().insert(o.client_order_id.clone(), report.clone());
        if let Some((after, conn)) = self.kill_after {
            if n == after {
                store::set_kill_switch(&conn.lock().unwrap(), true).unwrap();
            }
        }
        if self.lose_answer_on == Some(n) {
            *self.query_failures.lock().unwrap() += 1;
            return Err("connection reset after sending".into());
        }
        Ok(report)
    }
    fn cancel(&self, _i: &Instrument, _id: &str) -> Result<(), String> {
        Ok(())
    }
    fn query_by_client_id(&self, _i: &Instrument, id: &str) -> Result<Option<OrderReport>, String> {
        let mut failures = self.query_failures.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return Err("venue unreachable".into());
        }
        Ok(self.orders.lock().unwrap().get(id).cloned())
    }
    fn test_order(&self, _o: &OrderRequest) -> Result<String, String> {
        Ok("ok".into())
    }
}

fn db() -> Mutex<Connection> {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::init_db(&conn).unwrap();
    Mutex::new(conn)
}

fn refs() -> BTreeMap<String, f64> {
    [("BTC", 100_000.0), ("ETH", 4_000.0), ("SOL", 200.0), ("USDT", 1.0)].into_iter().map(|(a, p)| (a.into(), p)).collect()
}

fn weights(w: &[(&str, f64)]) -> PortfolioTarget {
    PortfolioTarget { weights: w.iter().map(|(a, w)| (a.to_string(), *w)).collect(), rebalance: true }
}

fn request(run_id: &str, target: PortfolioTarget, exchange: &str) -> RunRequest {
    RunRequest {
        run_id: run_id.into(),
        owner: "system:p".into(),
        exchange_id: exchange.into(),
        mode: Mode::Live,
        quote: "USDT".into(),
        target: Target::Portfolio(target),
        reference_prices: refs(),
        data_fresh: true,
        armed: true,
        account_belongs: true,
        close_date: None,
        signal: json!({"portfolioTarget": {}}),
        initial_capital: None,
        initial_holdings: None,
        leverage: 1.0,
    }
}

/// The account funded and the owner's books saying the same.
fn seed(conn: &Mutex<Connection>, venue: &MockSpot, exchange: &str, holdings: &[(&str, f64)]) {
    for (a, q) in holdings {
        venue.fund(a, *q);
    }
    SqliteStore { conn }
        .set_holdings("system:p", exchange, &holdings.iter().map(|(a, q)| (a.to_string(), *q)).collect())
        .unwrap();
}

/// Each asset's share of the holdings at the reference prices.
fn achieved(h: &store::Holdings) -> BTreeMap<String, f64> {
    let prices = refs();
    let values: Vec<(String, f64)> = h.iter().map(|(a, q)| (a.clone(), q * prices.get(a).copied().unwrap_or(0.0))).collect();
    let total: f64 = values.iter().map(|(_, v)| v).sum();
    values.into_iter().map(|(a, v)| (a, v / total)).collect()
}

fn legs(conn: &Mutex<Connection>, run: &str) -> Vec<String> {
    SqliteStore { conn }.orders(run).unwrap().iter().map(|o| o.leg.clone()).collect()
}

#[test]
fn from_cash_every_coin_is_bought_to_its_weight_and_the_cash_share_stays() {
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p1", &[("USDT", 10_000.0)]);
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("p1", weights(&[("BTC", 0.5), ("ETH", 0.3), ("USDT", 0.2)]), "p1")).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?} {:?}", rec.errors, rec.notes);
    assert_eq!(legs(&conn, "p1"), ["buy:BTC", "buy:ETH"]);
    let plan = rec.planned.as_ref().unwrap();
    assert!((plan["BTC"] - 0.05).abs() < 1e-12 && (plan["ETH"] - 0.75).abs() < 1e-12 && (plan["USDT"] - 2_000.0).abs() < 1e-9, "{plan:?}");
    let got = achieved(rec.holdings_after.as_ref().unwrap());
    // Each buy pays the 0.1 % fee and keeps 0.1 % room: within 0.25 points of the weights.
    for (asset, w) in [("BTC", 0.5), ("ETH", 0.3), ("USDT", 0.2)] {
        assert!((got[asset] - w).abs() < 0.0025, "{asset}: {got:?}");
    }
    assert!(rec.notes.iter().any(|n| n.starts_with("plan on 10000.00 USDT")), "{:?}", rec.notes);
}

#[test]
fn a_change_sells_first_then_buys_and_only_what_has_to_move() {
    let conn = db();
    let venue = MockSpot::new();
    // 5,000$ BTC and 5,000$ ETH; tomorrow BTC 50 % and SOL 50 %: BTC is right already.
    seed(&conn, &venue, "p2", &[("BTC", 0.05), ("ETH", 1.25)]);
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("p2", weights(&[("BTC", 0.5), ("SOL", 0.5)]), "p2")).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(legs(&conn, "p2"), ["sell:ETH", "buy:SOL"], "no BTC order: it holds its plan");
    let after = rec.holdings_after.unwrap();
    assert_eq!(after["BTC"], 0.05);
    assert!(!after.contains_key("ETH"));
    let got = achieved(&after);
    assert!((got["SOL"] - 0.5).abs() < 0.003, "{got:?}");
}

#[test]
fn the_same_assets_trade_nothing_and_the_weights_drift() {
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p3", &[("BTC", 0.06), ("ETH", 1.0)]);
    let mut target = weights(&[("BTC", 0.5), ("ETH", 0.5)]);
    target.rebalance = false;
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("p3", target, "p3")).unwrap();
    assert_eq!(rec.status, RunStatus::Done);
    assert_eq!(venue.calls(), 0);
    assert!(rec.planned.is_none());
    assert!(rec.notes.iter().any(|n| n.contains("drift")), "{:?}", rec.notes);
}

#[test]
fn an_amount_below_the_venues_minimum_is_skipped_never_rounded_up() {
    let conn = db();
    let venue = MockSpot::new();
    // 1,000$: SOL's 0.4 % is 4$, under the 5$ minimum; BTC's 0.01$ over its plan is too.
    seed(&conn, &venue, "p4", &[("USDT", 500.0), ("BTC", 0.0050001)]);
    let rec = Engine::default()
        .run(&SqliteStore { conn: &conn }, &venue, &request("p4", weights(&[("BTC", 0.5), ("SOL", 0.004), ("USDT", 0.496)]), "p4"))
        .unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(venue.calls(), 0, "{:?}", venue.sent());
    assert!(rec.notes.iter().any(|n| n.starts_with("SOL: buying") && n.contains("never rounded up")), "{:?}", rec.notes);
    assert!(rec.notes.iter().any(|n| n.starts_with("BTC: selling") && n.contains("below the venue's minimum")), "{:?}", rec.notes);
    assert!(!rec.holdings_after.unwrap().contains_key("SOL"));
}

#[test]
fn a_partial_leg_stops_the_run_as_partial_and_the_next_attempt_finishes_without_repeating_a_leg() {
    let conn = db();
    let venue = MockSpot::new();
    venue.fill_share.lock().unwrap().insert("ETHUSDT".into(), 0.4);
    seed(&conn, &venue, "p5", &[("USDT", 10_000.0)]);
    let req = request("p5", weights(&[("BTC", 0.5), ("ETH", 0.5)]), "p5");
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Partial, "{:?}", rec.errors);
    assert!(!rec.status.is_final(), "tried again inside its window");
    assert!(rec.errors.last().unwrap().contains("buy:ETH"), "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT", "buy ETHUSDT", "buy ETHUSDT", "buy ETHUSDT", "buy ETHUSDT"], "BTC filled; ETH tried and retried 3 times");
    let btc = rec.holdings_after.as_ref().unwrap()["BTC"];

    venue.fill_share.lock().unwrap().clear();
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(venue.sent().iter().filter(|s| s.contains("BTC")).count(), 1, "BTC is never bought twice");
    let after = rec.holdings_after.unwrap();
    assert_eq!(after["BTC"], btc);
    let got = achieved(&after);
    assert!((got["ETH"] - 0.5).abs() < 0.003, "{got:?}");
}

#[test]
fn a_refused_leg_lets_the_others_go_through_and_the_run_fails_to_be_tried_again() {
    let conn = db();
    let venue = MockSpot::new();
    venue.refuse.lock().unwrap().insert("ETHUSDT".into());
    seed(&conn, &venue, "p6", &[("USDT", 9_000.0)]);
    let req = request("p6", weights(&[("BTC", 1.0 / 3.0), ("ETH", 1.0 / 3.0), ("SOL", 1.0 / 3.0)]), "p6");
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Failed);
    assert!(!rec.status.is_final());
    assert!(rec.errors.last().unwrap().contains("refused buy:ETH"), "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT", "buy ETHUSDT", "buy SOLUSDT"]);
    let after = rec.holdings_after.as_ref().unwrap();
    assert!(after["BTC"] > 0.0299 && after["SOL"] > 14.9, "{after:?}");
    assert!(after["USDT"] > 2_990.0, "ETH's share stays cash: {after:?}");

    // The venue takes it on the next attempt: only ETH is bought.
    venue.refuse.lock().unwrap().clear();
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT", "buy ETHUSDT", "buy SOLUSDT", "buy ETHUSDT"]);
}

#[test]
fn a_restart_mid_run_resumes_with_no_duplicate_order_on_any_leg() {
    let conn = db();
    let mut venue = MockSpot::new();
    // The second order (sell SOL) fills, its answer is lost and the venue cannot be asked.
    venue.lose_answer_on = Some(2);
    seed(&conn, &venue, "p7", &[("ETH", 1.25), ("SOL", 25.0)]);
    let req = request("p7", weights(&[("BTC", 0.6), ("USDT", 0.4)]), "p7");
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Interrupted, "{:?}", rec.errors);
    assert_eq!(venue.calls(), 2);
    let plan = rec.planned.clone().unwrap();

    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(rec.planned.unwrap(), plan, "the resumed run trades to the same plan");
    assert_eq!(venue.sent(), ["sell ETHUSDT", "sell SOLUSDT", "buy BTCUSDT"], "every leg once");
    let orders = SqliteStore { conn: &conn }.orders("p7").unwrap();
    assert!(orders.iter().all(|o| o.state == OrderState::Done));
    let ids: HashSet<&str> = orders.iter().map(|o| o.client_order_id.as_str()).collect();
    assert_eq!(ids.len(), orders.len());
    let got = achieved(&rec.holdings_after.unwrap());
    assert!((got["BTC"] - 0.6).abs() < 0.004, "{got:?}");
}

#[test]
fn the_wrong_coin_check_holds_per_asset() {
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p8", &[("USDT", 10_000.0)]);
    let mut req = request("p8", weights(&[("BTC", 0.5), ("ETH", 0.5)]), "p8");
    req.reference_prices.insert("ETH".into(), 20_000.0); // another coin under the same symbol
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.errors[0].starts_with("wrong_coin") && rec.errors[0].contains("ETH"), "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT"], "BTC passed its own check");
}

#[test]
fn the_order_value_limit_and_the_turnover_cap_count_every_leg() {
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p9", &[("USDT", 10_000.0)]);
    let target = || weights(&[("BTC", 0.5), ("ETH", 0.3), ("USDT", 0.2)]);
    // Each leg against the order limit: BTC's 5,000$ is over 4,000$.
    let engine = Engine::new(GuardConfig { max_order_value: Some(4_000.0), ..GuardConfig::default() });
    let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &request("p9a", target(), "p9")).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.errors[0].starts_with("max_order_value") && rec.errors[0].contains("BTC"), "{:?}", rec.errors);
    assert_eq!(venue.calls(), 0);
    // The day's turnover across the legs: 5,000 passes a 6,000$ cap, the next 3,000 does not.
    let engine = Engine::new(GuardConfig { daily_turnover_cap: Some(6_000.0), ..GuardConfig::default() });
    let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &request("p9b", target(), "p9")).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.errors[0].starts_with("daily_turnover"), "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT"]);
}

#[test]
fn stop_all_trading_during_a_run_stops_the_legs_left_and_sells_nothing() {
    let conn = db();
    let mut venue = MockSpot::new();
    venue.kill_after = Some((1, &conn));
    seed(&conn, &venue, "p10", &[("USDT", 9_000.0)]);
    let req = request("p10", weights(&[("BTC", 1.0 / 3.0), ("ETH", 1.0 / 3.0), ("SOL", 1.0 / 3.0)]), "p10");
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.status.is_final());
    assert!(rec.errors[0].starts_with("kill_switch") && rec.errors[0].contains("buy:ETH"), "{:?}", rec.errors);
    assert_eq!(venue.sent(), ["buy BTCUSDT"], "the legs after the switch were not sent, nothing was sold");
    let after = rec.holdings_after.unwrap();
    assert!(after["BTC"] > 0.0299 && after["USDT"] > 5_990.0, "{after:?}");
}

#[test]
fn every_capital_mode_trades_several_assets_and_the_rest_stays_cash() {
    // Fixed (and a share of the balance, which Manual turns into the same amount): the
    // owner trades its 1,000$ on an account that holds 50,000$.
    let conn = db();
    let venue = MockSpot::new();
    venue.fund("USDT", 50_000.0);
    let mut req = request("p11", weights(&[("BTC", 0.5), ("SOL", 0.5)]), "p11");
    req.initial_capital = Some(1_000.0);
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    let after = rec.holdings_after.unwrap();
    // Fee, sizing room and BTC's 0.00001 step (1$ at 100,000$) below the 500$ each.
    assert!((after["BTC"] * 100_000.0 - 500.0).abs() < 3.0 && (after["SOL"] * 200.0 - 500.0).abs() < 3.0, "{after:?}");
    let cash = after["USDT"];
    assert!(cash > 0.0 && cash < 5.0, "rounding and fees stay as cash: {cash}");

    // The whole account: everything on it is the owner's equity.
    let conn = db();
    let venue = MockSpot::new();
    for (a, q) in [("USDT", 3_000.0), ("ETH", 0.25)] {
        venue.fund(a, q);
    }
    let mut req = request("p12", weights(&[("BTC", 0.5), ("USDT", 0.5)]), "p12");
    req.initial_holdings = Some([("USDT".to_string(), 3_000.0), ("ETH".to_string(), 0.25)].into_iter().collect());
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(rec.planned.as_ref().unwrap()["BTC"], 0.02, "half of 4,000$ at 100,000$");
    let got = achieved(&rec.holdings_after.unwrap());
    assert!((got["BTC"] - 0.5).abs() < 0.003 && !got.contains_key("ETH"), "{got:?}");
}

#[test]
fn a_coin_that_left_the_signal_is_valued_at_the_venues_mid_and_sold() {
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p13", &[("SOL", 10.0), ("USDT", 2_000.0)]);
    let mut req = request("p13", weights(&[("BTC", 1.0)]), "p13");
    req.reference_prices.remove("SOL");
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert!(rec.notes.iter().any(|n| n.starts_with("SOL: not in the signal, valued at the venue's mid")), "{:?}", rec.notes);
    assert_eq!(legs(&conn, "p13"), ["sell:SOL", "buy:BTC"]);
    assert!((rec.planned.unwrap()["BTC"] - 0.04).abs() < 1e-9, "2,000$ + 10 SOL at 200$");
}

#[test]
fn a_portfolio_never_runs_on_perp() {
    let conn = db();
    let mut venue = MockSpot::new();
    venue.perp = true;
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("p14", weights(&[("BTC", 1.0)]), "p14")).unwrap();
    assert_eq!(rec.status, RunStatus::GuardStopped);
    assert!(rec.status.is_final(), "not tried again every 5 minutes");
    assert!(rec.errors[0].contains("spot only"), "{:?}", rec.errors);
    assert_eq!(venue.calls(), 0);
}

#[test]
fn paper_trades_a_portfolio_at_the_real_book_without_touching_the_venue() {
    let conn = db();
    let market = MockSpot::new();
    let paper = PaperVenue::new(&market, &conn, "system:p", "p15", FEE, 0.0005);
    paper.fund_if_empty("USDT", 10_000.0).unwrap();
    let mut req = request("p15", weights(&[("BTC", 0.4), ("ETH", 0.4), ("USDT", 0.2)]), "p15");
    req.mode = Mode::Paper;
    req.armed = false;
    req.initial_capital = Some(10_000.0);
    let rec = Engine::default().run(&SqliteStore { conn: &conn }, &paper, &req).unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(market.calls(), 0, "the real venue saw no order");
    let got = achieved(rec.holdings_after.as_ref().unwrap());
    for (asset, w) in [("BTC", 0.4), ("ETH", 0.4), ("USDT", 0.2)] {
        assert!((got[asset] - w).abs() < 0.003, "{asset}: {got:?}");
    }
}

/// The rotation backtest's `_rebalance` (rotation_lab/backtest/engine.py): sells pay
/// `c` of the value sold, buys `c / (1 - c)` of the value bought, after-fee equity by
/// fixed-point iteration. Returns the fees.
fn backtest_fees(values: &BTreeMap<&str, f64>, target: &BTreeMap<&str, f64>, c: f64) -> f64 {
    let equity: f64 = values.values().sum();
    let coins: Vec<&str> = values.keys().chain(target.keys()).copied().filter(|s| *s != "USD").collect::<HashSet<_>>().into_iter().collect();
    let buy_cost = c / (1.0 - c);
    let moves = |after: f64| coins.iter().map(|s| target.get(s).copied().unwrap_or(0.0) * after - values.get(s).copied().unwrap_or(0.0)).collect::<Vec<_>>();
    let mut after = equity;
    for _ in 0..12 {
        let m = moves(after);
        after = equity - c * m.iter().filter(|x| **x < 0.0).map(|x| -x).sum::<f64>() - buy_cost * m.iter().filter(|x| **x > 0.0).sum::<f64>();
    }
    equity - after
}

#[test]
fn the_backtest_charges_a_several_leg_move_no_less_fee_than_the_legs_pay() {
    // 5,000$ BTC + 5,000$ ETH → BTC 20 %, SOL 40 %, cash 40 %: three legs.
    let conn = db();
    let venue = MockSpot::new();
    seed(&conn, &venue, "p16", &[("BTC", 0.05), ("ETH", 1.25)]);
    let rec = Engine::default()
        .run(&SqliteStore { conn: &conn }, &venue, &request("p16", weights(&[("BTC", 0.2), ("SOL", 0.4), ("USDT", 0.4)]), "p16"))
        .unwrap();
    assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
    assert_eq!(legs(&conn, "p16"), ["sell:BTC", "sell:ETH", "buy:SOL"]);
    let values: BTreeMap<&str, f64> = [("BTC", 5_000.0), ("ETH", 5_000.0)].into_iter().collect();
    let target: BTreeMap<&str, f64> = [("BTC", 0.2), ("SOL", 0.4), ("USD", 0.4)].into_iter().collect();
    // The system's fee_rate at the venue's taker fee, no slippage (the engine's defaults).
    let backtest = backtest_fees(&values, &target, FEE);
    assert!(rec.fees <= backtest + 1e-6, "live fees {} vs backtest {backtest}", rec.fees);
    // What the backtest leaves out is the spread: the run's cost minus its fees is the
    // book's half-spread on the 10,000$ moved, nothing more.
    let spread = rec.cost - rec.fees;
    assert!(spread > 0.0 && spread < 10_000.0 * BOOK_HALF_SPREAD * 2.0, "spread cost {spread}");
}
