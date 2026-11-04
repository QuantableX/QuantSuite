//! The execution core (PLAN-AUTO-EXECUTION §2.2) — Tauri-free, like
//! `runtime.rs`. One engine for Manual systems and Automated bots: a target
//! ("hold 100 % ETH", "flat") and an account in; the target made true on the
//! venue with guards, idempotent IOC limit orders, a route choice for every
//! switch and a full run record.
//!
//! - [`venue`]: the `ExecutionVenue` trait and its shapes.
//! - [`cex`]: Binance spot and Bybit spot.
//! - [`paper`]: the paper venue over a real venue's book.
//! - [`route`]: direct pair or through the quote asset.
//! - [`guards`]: the limits every run and order must pass.
//! - [`store`]: run records, orders, holdings, kill switch, journal rows.
//! - [`engine`]: the run itself.

pub mod cex;
pub mod engine;
pub mod guards;
pub mod paper;
pub mod route;
pub mod store;
pub mod venue;

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;

    use rusqlite::Connection;
    use serde_json::json;

    use super::engine::{AccountLock, Engine, RunRequest};
    use super::guards::GuardConfig;
    use super::paper::PaperVenue;
    use super::route::tests::{deep, inst};
    use super::route::RouteKind;
    use super::store::{self, OrderState, RunStatus, RunStore, SqliteStore, Target};
    use super::venue::*;
    use crate::runtime::Mode;

    /// A venue in memory: real book walks, balances, scripted failures.
    #[derive(Default)]
    struct MockVenue {
        books: HashMap<String, OrderBook>,
        instruments: Vec<Instrument>,
        balances: Mutex<HashMap<String, f64>>,
        orders: Mutex<HashMap<String, OrderReport>>,
        /// Every call to `place`, by client id.
        placed: Mutex<Vec<String>>,
        /// Fill only this share of each order (IOC partial).
        fill_share: f64,
        /// The n-th `place` fills but its answer is lost (1-based).
        lose_answer_on: Option<usize>,
        /// The n-th `place` never reaches the venue.
        drop_on: Option<usize>,
        /// Lookups fail this many times.
        query_failures: Mutex<usize>,
    }

    impl MockVenue {
        fn new(direct: bool) -> Self {
            let mut v = MockVenue { fill_share: 1.0, ..Default::default() };
            for (base, mid, size) in [("BTC", 100_000.0, 2.0), ("ETH", 4_000.0, 50.0), ("SOL", 200.0, 1_000.0)] {
                let i = inst(base, "USDT", 0.01, 0.00001);
                v.books.insert(i.filters.symbol.clone(), deep(mid, 0.000_01, 30, size));
                v.instruments.push(i);
            }
            if direct {
                let i = inst("ETH", "BTC", 0.00001, 0.0001);
                v.books.insert(i.filters.symbol.clone(), deep(0.04, 0.000_01, 30, 100.0));
                v.instruments.push(i);
            }
            v
        }

        fn fund(&self, asset: &str, qty: f64) {
            *self.balances.lock().unwrap().entry(asset.into()).or_default() += qty;
        }

        fn held(&self, asset: &str) -> f64 {
            self.balances.lock().unwrap().get(asset).copied().unwrap_or(0.0)
        }

        fn calls(&self) -> usize {
            self.placed.lock().unwrap().len()
        }
    }

    impl ExecutionVenue for MockVenue {
        fn label(&self) -> String {
            "Mock".into()
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities { spot: true, ioc_limit: true, client_order_ids: true, ..Default::default() }
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
            Ok(self
                .balances
                .lock()
                .unwrap()
                .iter()
                .map(|(a, q)| VenueBalance { asset: a.clone(), free: *q, locked: 0.0 })
                .collect())
        }
        fn positions(&self) -> Result<Vec<VenuePosition>, String> {
            Ok(Vec::new())
        }
        fn order_book(&self, i: &Instrument) -> Result<OrderBook, String> {
            self.books.get(&i.filters.symbol).cloned().ok_or_else(|| "no book".into())
        }
        fn fee_schedule(&self, _i: &Instrument) -> Result<FeeSchedule, String> {
            Ok(FeeSchedule { maker: 0.001, taker: 0.001 })
        }
        fn place(&self, o: &OrderRequest) -> Result<OrderReport, String> {
            let n = {
                let mut placed = self.placed.lock().unwrap();
                placed.push(o.client_order_id.clone());
                placed.len()
            };
            if self.drop_on == Some(n) {
                *self.query_failures.lock().unwrap() += 1;
                return Err("connection reset before the order arrived".into());
            }
            let book = self.order_book(&o.instrument)?;
            let want = o.instrument.filters.round_qty_down(o.qty * self.fill_share);
            let walk = match o.side {
                Side::Buy => book.walk_buy_base(want, Some(o.limit_price)),
                Side::Sell => book.walk_sell_base(want, Some(o.limit_price)),
            };
            let (base, quote) = (o.instrument.base().to_string(), o.instrument.quote().to_string());
            let fee = walk.quote * 0.001;
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

    fn request(run_id: &str, target: Target, exchange: &str) -> RunRequest {
        RunRequest {
            run_id: run_id.into(),
            owner: "system:lces".into(),
            exchange_id: exchange.into(),
            mode: Mode::Live,
            quote: "USDT".into(),
            target,
            reference_prices: refs(),
            data_fresh: true,
            armed: true,
            account_belongs: true,
            close_date: None,
            signal: json!({"best": "ETH"}),
            initial_capital: None,
        }
    }

    fn seed(conn: &Mutex<Connection>, exchange: &str, holdings: &[(&str, f64)]) {
        let store = SqliteStore { conn };
        store
            .set_holdings("system:lces", exchange, &holdings.iter().map(|(a, q)| (a.to_string(), *q)).collect())
            .unwrap();
    }

    #[test]
    fn a_rotation_switch_without_a_direct_pair_sells_then_buys() {
        let conn = db();
        let venue = MockVenue::new(false);
        venue.fund("BTC", 0.1);
        seed(&conn, "x1", &[("BTC", 0.1)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r1", Target::Hold("ETH".into()), "x1")).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        let route = rec.route.as_ref().unwrap();
        assert_eq!(route.chosen, RouteKind::ViaQuote);
        let orders = SqliteStore { conn: &conn }.orders("r1").unwrap();
        assert_eq!(orders.iter().map(|o| o.leg.as_str()).collect::<Vec<_>>(), vec!["sell:BTC", "buy:ETH"]);
        assert!(orders.iter().all(|o| o.state == OrderState::Done));
        let after = rec.holdings_after.unwrap();
        // 10,000$ − 0.1 % − 0.1 % − the 0.1 % sizing room ≈ 2.4925 ETH.
        let eth = after["ETH"];
        assert!((eth - 2.4925).abs() < 0.003, "{after:?}");
        assert!(after.get("USDT").copied().unwrap_or(0.0) < 15.0, "cash left is small: {after:?}");
        assert!(!after.contains_key("BTC"));
        assert!((rec.fees - 19.98).abs() < 0.1, "two legs of fees: {}", rec.fees);
        // The holdings are kept for the next run; the journal has the ETH row.
        assert_eq!(SqliteStore { conn: &conn }.holdings("system:lces", "x1").unwrap(), after);
        let open: i64 = conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM trades WHERE strategy_id = 'system:lces' AND pair = 'ETH/USDT' AND exit_time IS NULL AND source = 'run:r1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(open, 1);
    }

    #[test]
    fn a_liquid_direct_pair_is_one_order() {
        let conn = db();
        let venue = MockVenue::new(true);
        venue.fund("BTC", 0.1);
        seed(&conn, "x2", &[("BTC", 0.1)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r2", Target::Hold("ETH".into()), "x2")).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?} {:?}", rec.errors, rec.notes);
        assert_eq!(rec.route.as_ref().unwrap().chosen, RouteKind::Direct, "{:?}", rec.route);
        let orders = SqliteStore { conn: &conn }.orders("r2").unwrap();
        assert_eq!(orders.len(), 1, "{orders:?}");
        assert_eq!(orders[0].leg, "direct:BTC>ETH");
        assert_eq!(orders[0].side, Side::Buy, "ETH/BTC: buy ETH with BTC");
        assert!(rec.holdings_after.unwrap()["ETH"] > 2.49);
        assert!((rec.fees - 10.0).abs() < 0.1, "one leg of fees: {}", rec.fees);
    }

    #[test]
    fn flat_sells_everything_and_dust_stays() {
        let conn = db();
        let venue = MockVenue::new(false);
        venue.fund("ETH", 1.0);
        venue.fund("SOL", 0.01);
        seed(&conn, "x3", &[("ETH", 1.0), ("SOL", 0.01)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r3", Target::Flat, "x3")).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        assert_eq!(venue.calls(), 1, "only ETH is sold; 2$ of SOL is below the 5$ minimum");
        assert!(rec.dust.iter().any(|d| d.starts_with("SOL")), "{:?}", rec.dust);
        let after = rec.holdings_after.unwrap();
        assert!((after["USDT"] - 3_996.0).abs() < 1.0, "{after:?}");
        assert_eq!(after["SOL"], 0.01);
    }

    #[test]
    fn cash_below_the_minimum_buys_nothing_and_holding_the_target_is_done() {
        let conn = db();
        let venue = MockVenue::new(false);
        venue.fund("USDT", 3.0);
        seed(&conn, "x4", &[("USDT", 3.0)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r4", Target::Hold("ETH".into()), "x4")).unwrap();
        assert_eq!(rec.status, RunStatus::Done);
        assert_eq!(venue.calls(), 0);

        venue.fund("ETH", 2.0);
        seed(&conn, "x4", &[("ETH", 2.0), ("USDT", 3.0)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r4b", Target::Hold("ETH".into()), "x4")).unwrap();
        assert_eq!(rec.status, RunStatus::Done);
        assert_eq!(venue.calls(), 0, "already holding the target");
    }

    #[test]
    fn a_new_owner_starts_with_its_capital_and_never_more_than_the_venue_holds() {
        let conn = db();
        let venue = MockVenue::new(false);
        venue.fund("USDT", 1_000.0);
        let mut req = request("r5", Target::Hold("SOL".into()), "x5");
        req.initial_capital = Some(5_000.0);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        assert_eq!(rec.holdings_before.as_ref().unwrap()["USDT"], 5_000.0);
        assert!(rec.notes.iter().any(|n| n.contains("the venue holds 1000")), "{:?}", rec.notes);
        let sol = rec.holdings_after.unwrap()["SOL"];
        assert!(sol > 4.9 && sol < 5.0, "about 1,000$ of SOL: {sol}");
    }

    #[test]
    fn guards_stop_a_run_with_the_reason() {
        let conn = db();
        let venue = MockVenue::new(false);
        venue.fund("BTC", 0.1);
        seed(&conn, "x6", &[("BTC", 0.1)]);
        store::set_kill_switch(&conn.lock().unwrap(), true).unwrap();
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r6", Target::Flat, "x6")).unwrap();
        assert_eq!(rec.status, RunStatus::GuardStopped);
        assert!(rec.errors[0].starts_with("kill_switch"), "{:?}", rec.errors);
        store::set_kill_switch(&conn.lock().unwrap(), false).unwrap();

        // The signal says BTC is worth 110,000$; the venue says 100,000$: a wrong instrument or a broken book.
        let mut req = request("r7", Target::Flat, "x6");
        req.reference_prices.insert("BTC".into(), 110_000.0);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::GuardStopped);
        assert!(rec.errors[0].starts_with("price_sanity"), "{:?}", rec.errors);

        let engine = Engine::new(GuardConfig { max_order_value: Some(5_000.0), ..GuardConfig::default() });
        let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &request("r8", Target::Flat, "x6")).unwrap();
        assert_eq!(rec.status, RunStatus::GuardStopped);
        assert!(rec.errors[0].starts_with("max_order_value"), "{:?}", rec.errors);

        let engine = Engine::new(GuardConfig { daily_turnover_cap: Some(1_000.0), ..GuardConfig::default() });
        let rec = engine.run(&SqliteStore { conn: &conn }, &venue, &request("r9", Target::Flat, "x6")).unwrap();
        assert!(rec.errors[0].starts_with("daily_turnover"), "{:?}", rec.errors);

        for (field, name) in [("fresh", "data_fresh"), ("armed", "armed"), ("account", "account")] {
            let mut req = request(&format!("r10-{field}"), Target::Flat, "x6");
            match field {
                "fresh" => req.data_fresh = false,
                "armed" => req.armed = false,
                _ => req.account_belongs = false,
            }
            let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
            assert_eq!(rec.status, RunStatus::GuardStopped);
            assert!(rec.errors[0].starts_with(name), "{:?}", rec.errors);
        }
        assert_eq!(venue.calls(), 0, "no guard stop sent anything");
    }

    #[test]
    fn a_crash_between_two_legs_resumes_without_a_duplicate_order() {
        let conn = db();
        let mut venue = MockVenue::new(false);
        // The buy goes through but its answer is lost, and the venue cannot be asked.
        venue.lose_answer_on = Some(2);
        venue.fund("BTC", 0.1);
        seed(&conn, "x11", &[("BTC", 0.1)]);
        let req = request("r11", Target::Hold("ETH".into()), "x11");
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Interrupted, "{:?}", rec.errors);
        assert_eq!(venue.calls(), 2);
        let orders = SqliteStore { conn: &conn }.orders("r11").unwrap();
        assert_eq!(orders[1].state, OrderState::Unknown);

        // The restart: the same run id resumes, finds the buy by its client id.
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        assert_eq!(venue.calls(), 2, "nothing was sent twice");
        assert!(rec.holdings_after.unwrap()["ETH"] > 2.49);
        assert!((venue.held("ETH") - 2.4925).abs() < 0.003);

        // Exactly once: a finished run is returned as it is.
        let again = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(again.status, RunStatus::Done);
        assert_eq!(venue.calls(), 2);
    }

    #[test]
    fn an_order_that_never_arrived_is_planned_again() {
        let conn = db();
        let mut venue = MockVenue::new(false);
        venue.drop_on = Some(1);
        venue.fund("ETH", 1.0);
        seed(&conn, "x12", &[("ETH", 1.0)]);
        let req = request("r12", Target::Flat, "x12");
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Interrupted);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        let orders = SqliteStore { conn: &conn }.orders("r12").unwrap();
        assert_eq!(orders.iter().map(|o| o.state).collect::<Vec<_>>(), vec![OrderState::NotSent, OrderState::Done]);
        assert_ne!(orders[0].client_order_id, orders[1].client_order_id);
        assert!((venue.held("ETH")).abs() < 1e-9, "sold once");
    }

    #[test]
    fn partial_fills_are_retried_three_times_then_reported() {
        let conn = db();
        let mut venue = MockVenue::new(false);
        venue.fill_share = 0.4;
        venue.fund("BTC", 1.0);
        seed(&conn, "x13", &[("BTC", 1.0)]);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &venue, &request("r13", Target::Flat, "x13")).unwrap();
        assert_eq!(rec.status, RunStatus::Partial, "{:?}", rec.errors);
        assert_eq!(venue.calls(), 4, "the first attempt and three retries");
        assert!(rec.errors[0].contains("after 3 retries"), "{:?}", rec.errors);
        let left = rec.holdings_after.unwrap()["BTC"];
        assert!((left - 0.6f64.powi(4)).abs() < 1e-4, "{left}");
    }

    #[test]
    fn paper_fills_at_the_real_book_with_fee_and_slippage_and_keeps_its_balances() {
        let conn = db();
        let market = MockVenue::new(true);
        let paper = PaperVenue::new(&market, &conn, "system:lces", "x14", 0.001, 0.0005);
        assert!(paper.fund_if_empty("USDT", 10_000.0).unwrap());
        assert!(!paper.fund_if_empty("USDT", 10_000.0).unwrap(), "funded once");
        let mut req = request("r14", Target::Hold("ETH".into()), "x14");
        req.mode = Mode::Paper;
        req.armed = false;
        req.initial_capital = Some(10_000.0);
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &paper, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        assert_eq!(market.calls(), 0, "the real venue saw no order");
        let orders = SqliteStore { conn: &conn }.orders("r14").unwrap();
        let r = orders[0].report.as_ref().unwrap();
        // Walked ask ≈ 4,000.04 plus 0.05 % paper slippage.
        assert!((r.avg_price - 4_000.04 * 1.0005).abs() < 0.5, "{r:?}");
        assert!((r.fee_amount - r.quote_amount * 0.001).abs() < 1e-9);
        // A second venue object over the same database sees the same balances.
        let again = PaperVenue::new(&market, &conn, "system:lces", "x14", 0.001, 0.0005);
        let eth = free_balance(&again.balances().unwrap(), "ETH");
        assert!((eth - rec.holdings_after.as_ref().unwrap()["ETH"]).abs() < 1e-9);
        // The paper venue takes the same route decision: ETH → BTC goes direct on ETH/BTC.
        let mut req = request("r15", Target::Hold("BTC".into()), "x14");
        req.mode = Mode::Paper;
        let rec = Engine::default().run(&SqliteStore { conn: &conn }, &paper, &req).unwrap();
        assert_eq!(rec.status, RunStatus::Done, "{:?}", rec.errors);
        assert_eq!(rec.route.unwrap().chosen, RouteKind::Direct);
    }

    #[test]
    fn one_run_per_account_at_a_time() {
        let lock = AccountLock::acquire("acct-lock-test").unwrap();
        assert!(AccountLock::acquire("acct-lock-test").is_err());
        drop(lock);
        assert!(AccountLock::acquire("acct-lock-test").is_ok());
    }
}
