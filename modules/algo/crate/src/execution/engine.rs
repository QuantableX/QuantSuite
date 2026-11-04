//! The engine (PLAN-AUTO-EXECUTION §2.2): a target and an account in, the
//! target made true on the venue, everything recorded.
//!
//! 1. Lock the account (one run per account at a time).
//! 2. Load or start the run record; a finished run is never run again.
//! 3. Run-level guards (data freshness, kill switch, account, armed).
//! 4. Settle orders a crash left open: ask the venue by client id.
//! 5. Holdings: what the owner held when the run started plus every fill
//!    of this run, capped at what the venue really holds.
//! 6. Step until the target holds: the route for a switch (direct pair or
//!    through the quote asset), sells first, then the buy. Every order is an
//!    IOC limit at the slippage cap, passes the order guards, is written
//!    before it is sent, and is booked after. A partial fill is retried up to
//!    three times, then the run stops and reports. A direct order gets one
//!    attempt; what it leaves goes through the quote asset.
//! 7. Save holdings and the run record.
//!
//! Resuming is re-planning: the plan is "the difference between now and the
//! target", and "now" is rebuilt from the recorded fills, so a run that
//! crashed between two legs continues with the second leg — and an order
//! whose answer was lost is looked up, never sent twice.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::Value;

use super::guards::{self, GuardConfig, OrderCheck, RunContext};
use super::route::{self, DirectMarket, RouteKind, SwitchInputs};
use super::store::{FillBooking, Holdings, OrderRecord, OrderState, RunRecord, RunStatus, RunStore, Target};
use super::venue::{free_balance, ExecutionVenue, Instrument, OrderReport, OrderRequest, OrderStatus, Side};
use crate::runtime::Mode;

/// Partial fills: retries after the first attempt.
pub const MAX_RETRIES: usize = 3;
const MAX_STEPS: usize = 32;

/// What a run is asked to do.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// The caller's id: Manual uses one per (system, close date), so a
    /// second start the same day resumes or returns the finished run.
    pub run_id: String,
    /// `system:<id>` or `bot:<id>`.
    pub owner: String,
    pub exchange_id: String,
    pub mode: Mode,
    /// The account's quote asset (USDT on Binance / Bybit).
    pub quote: String,
    pub target: Target,
    /// USD per unit of every asset the run may touch (the signal's prices);
    /// the account quote counts as 1 when missing.
    pub reference_prices: BTreeMap<String, f64>,
    pub data_fresh: bool,
    pub armed: bool,
    pub account_belongs: bool,
    pub close_date: Option<String>,
    pub signal: Value,
    /// The quote capital an owner starts with when it holds nothing yet.
    pub initial_capital: Option<f64>,
    /// Or everything it starts with (a whole sub-account); wins over the capital.
    pub initial_holdings: Option<Holdings>,
}

/// One run per account at a time, across every caller in the process.
pub struct AccountLock(String);

fn locks() -> &'static Mutex<HashSet<String>> {
    static LOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

impl AccountLock {
    pub fn acquire(account: &str) -> Result<Self, String> {
        let mut held = locks().lock().map_err(|_| "account locks poisoned".to_string())?;
        if !held.insert(account.to_string()) {
            return Err(format!("Another run is trading on account {account} right now."));
        }
        Ok(Self(account.to_string()))
    }
}

impl Drop for AccountLock {
    fn drop(&mut self) {
        if let Ok(mut held) = locks().lock() {
            held.remove(&self.0);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Engine {
    pub guards: GuardConfig,
    pub max_retries: usize,
}

impl Default for Engine {
    fn default() -> Self {
        Self { guards: GuardConfig::default(), max_retries: MAX_RETRIES }
    }
}

/// Why a run stopped before the target held.
struct Stop(RunStatus, String);

enum OrderOutcome {
    Report(OrderReport),
    /// The venue refused it; nothing executed.
    Refused(String),
    /// The size is below the venue's minimums.
    TooSmall,
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn compact(run_id: &str) -> String {
    run_id.chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect()
}

/// The fill applied to holdings: what left and what came in.
pub fn apply_fill(holdings: &mut Holdings, inst: &Instrument, side: Side, r: &OrderReport) {
    if r.filled_qty <= 0.0 {
        return;
    }
    let base = inst.base().to_string();
    let quote = inst.quote().to_string();
    let add = |h: &mut Holdings, asset: &str, delta: f64| {
        let entry = h.entry(asset.to_string()).or_insert(0.0);
        *entry += delta;
        if entry.abs() < 1e-12 {
            h.remove(asset);
        }
    };
    match side {
        Side::Buy => {
            add(holdings, &base, r.base_received);
            add(holdings, &quote, -(r.quote_amount + r.fee_in(&quote)));
        }
        Side::Sell => {
            add(holdings, &base, -(r.filled_qty + r.fee_in(&base)));
            add(holdings, &quote, r.quote_amount - r.fee_in(&quote));
        }
    }
}

struct Exec<'a> {
    engine: &'a Engine,
    store: &'a dyn RunStore,
    venue: &'a dyn ExecutionVenue,
    req: &'a RunRequest,
    rec: RunRecord,
    orders: Vec<OrderRecord>,
    holdings: Holdings,
    fees: BTreeMap<String, f64>,
    /// Assets found too small to sell during this run.
    dusted: HashSet<String>,
    /// The direct order was refused or too small without reaching the venue.
    direct_skipped: bool,
}

impl Engine {
    pub fn new(guards: GuardConfig) -> Self {
        Self { guards, max_retries: MAX_RETRIES }
    }

    /// Make `req.target` true. Returns the run record (also on a guard stop or
    /// a venue failure); `Err` only when the record itself cannot be kept.
    pub fn run(&self, store: &dyn RunStore, venue: &dyn ExecutionVenue, req: &RunRequest) -> Result<RunRecord, String> {
        let _lock = AccountLock::acquire(&req.exchange_id)?;
        let t0 = Instant::now();
        let mut rec = match store.load_run(&req.run_id)? {
            Some(rec) if rec.status.is_final() => return Ok(rec),
            Some(mut rec) => {
                rec.notes.push(format!("resumed at {}", now_rfc3339()));
                rec
            }
            None => RunRecord {
                id: req.run_id.clone(),
                owner: req.owner.clone(),
                exchange_id: req.exchange_id.clone(),
                mode: req.mode.as_str().to_string(),
                close_date: req.close_date.clone(),
                status: RunStatus::Running,
                target: req.target.clone(),
                quote: req.quote.clone(),
                signal: req.signal.clone(),
                reference_prices: req.reference_prices.clone(),
                route: None,
                guards: Vec::new(),
                holdings_before: None,
                holdings_after: None,
                dust: Vec::new(),
                notes: Vec::new(),
                errors: Vec::new(),
                cost: 0.0,
                fees: 0.0,
                started_at: now_rfc3339(),
                finished_at: None,
                timings: BTreeMap::new(),
            },
        };
        rec.status = RunStatus::Running;
        store.save_run(&rec)?;

        rec.guards = guards::run_guards(&RunContext {
            data_fresh: req.data_fresh,
            kill_switch: store.kill_switch()?,
            account_belongs: req.account_belongs,
            live: req.mode == Mode::Live,
            armed: req.armed,
        });
        if !guards::all_pass(&rec.guards) {
            let why = guards::first_failure(&rec.guards).unwrap_or_default();
            return Self::finish(store, rec, RunStatus::GuardStopped, Some(why), t0);
        }
        if let Err(e) = venue.ready() {
            return Self::finish(store, rec, RunStatus::Failed, Some(e), t0);
        }
        let caps = venue.capabilities();
        if !caps.spot || !caps.ioc_limit {
            let why = format!("{} cannot take spot IOC limit orders, which this engine sends", venue.label());
            return Self::finish(store, rec, RunStatus::Failed, Some(why), t0);
        }
        if !caps.client_order_ids {
            rec.notes.push(format!("{} has no client order ids: a lost answer cannot be looked up", venue.label()));
        }

        let orders = store.orders(&req.run_id)?;
        let mut x = Exec {
            engine: self,
            store,
            venue,
            req,
            rec,
            orders,
            holdings: Holdings::new(),
            fees: BTreeMap::new(),
            dusted: HashSet::new(),
            direct_skipped: false,
        };
        let outcome = x.settle_open_orders().and_then(|()| x.init_holdings()).and_then(|()| x.drive());
        let Exec { mut rec, holdings, .. } = x;
        rec.timings.insert("total".into(), t0.elapsed().as_millis() as u64);
        let (status, why) = match outcome {
            Ok(()) => (RunStatus::Done, None),
            Err(Stop(status, why)) => (status, Some(why)),
        };
        if status != RunStatus::Interrupted {
            store.set_holdings(&req.owner, &req.exchange_id, &holdings)?;
            rec.holdings_after = Some(holdings);
        }
        Self::finish(store, rec, status, why, t0)
    }

    fn finish(
        store: &dyn RunStore,
        mut rec: RunRecord,
        status: RunStatus,
        why: Option<String>,
        t0: Instant,
    ) -> Result<RunRecord, String> {
        rec.status = status;
        if let Some(why) = why {
            rec.errors.push(why);
        }
        if status != RunStatus::Interrupted {
            rec.finished_at = Some(now_rfc3339());
        }
        rec.timings.insert("total".into(), t0.elapsed().as_millis() as u64);
        store.save_run(&rec)?;
        Ok(rec)
    }
}

impl Exec<'_> {
    fn save(&self) -> Result<(), Stop> {
        self.store.save_run(&self.rec).map_err(|e| Stop(RunStatus::Failed, e))
    }

    fn reference(&self, asset: &str) -> Option<f64> {
        self.req
            .reference_prices
            .get(asset)
            .copied()
            .or_else(|| asset.eq_ignore_ascii_case(&self.req.quote).then_some(1.0))
            .filter(|p| *p > 0.0)
    }

    /// The signal's price of the instrument's base in its quote.
    fn reference_for(&self, inst: &Instrument) -> Option<f64> {
        Some(self.reference(inst.base())? / self.reference(inst.quote())?)
    }

    fn quote_usd(&self, inst: &Instrument) -> Result<f64, Stop> {
        self.reference(inst.quote())
            .ok_or_else(|| Stop(RunStatus::Failed, format!("no reference price for {}", inst.quote())))
    }

    fn taker_fee(&mut self, inst: &Instrument) -> f64 {
        if let Some(fee) = self.fees.get(&inst.filters.symbol) {
            return *fee;
        }
        let fee = match self.venue.fee_schedule(inst) {
            Ok(schedule) if schedule.taker >= 0.0 => schedule.taker,
            Ok(_) | Err(_) => {
                self.rec.notes.push(format!(
                    "{}: fee tier unavailable, estimated at {:.2} %",
                    inst.pair(),
                    super::cex::DEFAULT_TAKER_FEE * 100.0
                ));
                super::cex::DEFAULT_TAKER_FEE
            }
        };
        self.fees.insert(inst.filters.symbol.clone(), fee);
        fee
    }

    fn instrument(&self, base: &str, quote: &str) -> Result<Instrument, Stop> {
        self.venue
            .instrument(&format!("{base}/{quote}"))
            .map_err(|e| Stop(RunStatus::Failed, format!("{base}/{quote}: {e}")))
    }

    fn settle_open_orders(&mut self) -> Result<(), Stop> {
        let mut changed = false;
        for i in 0..self.orders.len() {
            if !matches!(self.orders[i].state, OrderState::Sending | OrderState::Unknown) {
                continue;
            }
            let (inst, id) = (self.orders[i].instrument.clone(), self.orders[i].client_order_id.clone());
            match self.venue.query_by_client_id(&inst, &id) {
                Ok(Some(report)) => {
                    self.rec.notes.push(format!("{id}: found on the venue after a restart ({:?})", report.status));
                    self.orders[i].state = OrderState::Done;
                    self.orders[i].report = Some(report);
                    self.book(i);
                }
                Ok(None) => {
                    self.rec.notes.push(format!("{id}: never reached the venue; the step is planned again"));
                    self.orders[i].state = OrderState::NotSent;
                }
                Err(e) => {
                    return Err(Stop(
                        RunStatus::Interrupted,
                        format!("order {id} has an unknown outcome and the venue cannot be asked: {e}"),
                    ))
                }
            }
            self.orders[i].updated_at = now_rfc3339();
            self.store.save_order(&self.orders[i], &self.req.exchange_id).map_err(|e| Stop(RunStatus::Failed, e))?;
            changed = true;
        }
        if changed {
            self.save()?;
        }
        Ok(())
    }

    fn init_holdings(&mut self) -> Result<(), Stop> {
        let before = match self.rec.holdings_before.clone() {
            Some(before) => before,
            None => {
                let mut h = self
                    .store
                    .holdings(&self.req.owner, &self.req.exchange_id)
                    .map_err(|e| Stop(RunStatus::Failed, e))?;
                if h.is_empty() {
                    if let Some(start) = self.req.initial_holdings.clone() {
                        h = start;
                    } else if let Some(capital) = self.req.initial_capital.filter(|c| *c > 0.0) {
                        h.insert(self.req.quote.clone(), capital);
                    }
                }
                self.rec.holdings_before = Some(h.clone());
                self.save()?;
                h
            }
        };
        let mut holdings = before;
        for o in &self.orders {
            if let Some(r) = o.filled() {
                apply_fill(&mut holdings, &o.instrument, o.side, r);
            }
        }
        // Never more than the venue holds: other owners share the account.
        let t = Instant::now();
        let balances = self
            .venue
            .balances()
            .map_err(|e| Stop(RunStatus::Failed, format!("reading the balances: {e}")))?;
        self.rec.timings.insert("balances".into(), t.elapsed().as_millis() as u64);
        for (asset, qty) in holdings.iter_mut() {
            let on_venue = free_balance(&balances, asset);
            if *qty > on_venue + 1e-9 {
                self.rec.notes.push(format!(
                    "{asset}: the system's books say {qty}, the venue holds {on_venue}; using what the venue holds"
                ));
                *qty = on_venue.max(0.0);
            }
        }
        holdings.retain(|_, q| *q > 1e-12);
        self.holdings = holdings;
        Ok(())
    }

    fn attempts(&self, leg: &str) -> usize {
        self.orders.iter().filter(|o| o.leg == leg && o.state != OrderState::NotSent).count()
    }

    /// A leg whose last order filled completely is done: what is left of it
    /// is lot rounding or the sizing room, not worth another order.
    fn leg_filled(&self, leg: &str) -> bool {
        self.orders
            .iter()
            .rev()
            .find(|o| o.leg == leg && o.state != OrderState::NotSent)
            .and_then(|o| o.report.as_ref())
            .is_some_and(|r| r.status == OrderStatus::Filled)
    }

    fn direct_attempted(&self) -> bool {
        self.direct_skipped || self.orders.iter().any(|o| o.leg.starts_with("direct:"))
    }

    fn drive(&mut self) -> Result<(), Stop> {
        let quote = self.req.quote.clone();
        let target = match &self.req.target {
            Target::Flat => None,
            Target::Hold(asset) if asset.eq_ignore_ascii_case(&quote) => None,
            Target::Hold(asset) => Some(asset.clone()),
        };
        for _ in 0..MAX_STEPS {
            // What to sell: every asset the owner holds that is not the target.
            let mut sells: Vec<(String, f64, Instrument)> = Vec::new();
            let holdings: Vec<(String, f64)> = self.holdings.iter().map(|(a, q)| (a.clone(), *q)).collect();
            for (asset, qty) in holdings {
                if asset.eq_ignore_ascii_case(&quote)
                    || target.as_deref() == Some(asset.as_str())
                    || qty <= 0.0
                    || self.dusted.contains(&asset)
                    || self.leg_filled(&format!("sell:{asset}"))
                {
                    continue;
                }
                let inst = self.instrument(&asset, &quote)?;
                let q = inst.filters.round_qty_down(qty);
                let price = self.reference(&asset).unwrap_or(0.0);
                if inst.tradable(q, price) {
                    sells.push((asset, q, inst));
                } else {
                    let line = format!("{asset}: {qty} is below the venue's minimum and stays as dust");
                    if !self.rec.dust.contains(&line) {
                        self.rec.dust.push(line);
                    }
                }
            }
            // What to buy: the target with the owner's quote, when it clears the minimums.
            let buy = match &target {
                Some(t) if self.leg_filled(&format!("buy:{t}")) => None,
                Some(t) => {
                    let inst = self.instrument(t, &quote)?;
                    let cash = self.holdings.get(&quote).copied().unwrap_or(0.0);
                    let price = self.reference(t).unwrap_or(0.0);
                    let fee = self.taker_fee(&inst);
                    let qty = if price > 0.0 { inst.filters.round_qty_down(cash * (1.0 - fee) / (price * 1.006)) } else { 0.0 };
                    if inst.tradable(qty, price) {
                        Some((t.clone(), cash, inst))
                    } else {
                        None
                    }
                }
                None => None,
            };
            if sells.is_empty() && buy.is_none() {
                return Ok(());
            }

            // A switch X → T: the route is chosen once per run.
            if sells.len() == 1 && target.is_some() && !self.direct_attempted() {
                let (from, qty, from_inst) = sells[0].clone();
                let to = target.clone().unwrap_or_default();
                if self.rec.route.is_none() {
                    self.choose_route(&from, &to, qty, &from_inst)?;
                }
                let direct = self.rec.route.as_ref().filter(|r| r.chosen == RouteKind::Direct).and_then(|r| r.direct_pair.clone());
                if let Some(pair) = direct {
                    self.direct_leg(&from, &to, qty, &pair)?;
                    continue;
                }
            }

            if let Some((asset, qty, inst)) = sells.into_iter().next() {
                let leg = format!("sell:{asset}");
                self.leg_guard(&leg, &inst)?;
                let reference = self.reference_for(&inst);
                match self.execute(&leg, &inst, Side::Sell, qty, None, reference)? {
                    OrderOutcome::Refused(e) => return Err(Stop(RunStatus::Failed, format!("{leg}: the venue refused the order: {e}"))),
                    OrderOutcome::TooSmall => {
                        self.rec.dust.push(format!("{asset}: {qty} is below the venue's minimum at the current price and stays as dust"));
                        self.dusted.insert(asset);
                    }
                    OrderOutcome::Report(_) => {}
                }
                continue;
            }
            if let Some((asset, cash, inst)) = buy {
                let leg = format!("buy:{asset}");
                self.leg_guard(&leg, &inst)?;
                let reference = self.reference_for(&inst);
                match self.execute(&leg, &inst, Side::Buy, 0.0, Some(cash), reference)? {
                    OrderOutcome::Refused(e) => return Err(Stop(RunStatus::Failed, format!("{leg}: the venue refused the order: {e}"))),
                    OrderOutcome::TooSmall => return Ok(()),
                    OrderOutcome::Report(_) => {}
                }
                continue;
            }
        }
        Err(Stop(RunStatus::Failed, format!("the run did not settle after {MAX_STEPS} steps")))
    }

    /// Stop after the first attempt plus `max_retries` retries of a leg.
    fn leg_guard(&self, leg: &str, inst: &Instrument) -> Result<(), Stop> {
        if self.attempts(leg) > self.engine.max_retries {
            let left = self.holdings.get(inst.base()).copied().unwrap_or(0.0);
            return Err(Stop(
                RunStatus::Partial,
                format!(
                    "{leg}: still partly filled after {} retries ({} {} left; {} {} cash); stopped",
                    self.engine.max_retries,
                    left,
                    inst.base(),
                    self.holdings.get(inst.quote()).copied().unwrap_or(0.0),
                    inst.quote()
                ),
            ));
        }
        Ok(())
    }

    fn choose_route(&mut self, from: &str, to: &str, qty: f64, from_inst: &Instrument) -> Result<(), Stop> {
        let t = Instant::now();
        let to_inst = self.instrument(to, &self.req.quote.clone())?;
        let book = |inst: &Instrument| self.venue.order_book(inst).map_err(|e| Stop(RunStatus::Failed, format!("{}: {e}", inst.pair())));
        let from_book = book(from_inst)?;
        let to_book = book(&to_inst)?;
        let from_fee = self.taker_fee(from_inst);
        let to_fee = self.taker_fee(&to_inst);
        // A direct pair between the two, in either direction.
        let direct_inst = match self.venue.instruments() {
            Ok(all) => all.into_iter().find(|i| {
                (i.base().eq_ignore_ascii_case(from) && i.quote().eq_ignore_ascii_case(to))
                    || (i.base().eq_ignore_ascii_case(to) && i.quote().eq_ignore_ascii_case(from))
            }),
            Err(e) => {
                self.rec.notes.push(format!("instrument list unavailable ({e}); through the quote asset"));
                None
            }
        };
        let direct_book = match &direct_inst {
            Some(inst) => self.venue.order_book(inst).ok(),
            None => None,
        };
        let direct_fee = direct_inst.clone().map(|i| self.taker_fee(&i)).unwrap_or(0.0);
        let direct = match (&direct_inst, &direct_book) {
            (Some(inst), Some(b)) => Some(DirectMarket { instrument: inst.clone(), book: b, taker_fee: direct_fee }),
            _ => None,
        };
        let choice = route::choose(&SwitchInputs {
            from,
            to,
            qty_from: qty,
            from_book: &from_book,
            to_book: &to_book,
            from_fee,
            to_fee,
            direct,
        });
        self.rec.timings.insert("route".into(), t.elapsed().as_millis() as u64);
        self.rec.notes.push(format!("route {from} → {to}: {:?} ({})", choice.chosen, choice.reason));
        self.rec.route = Some(choice);
        self.save()
    }

    fn direct_leg(&mut self, from: &str, to: &str, qty_from: f64, pair: &str) -> Result<(), Stop> {
        let inst = self
            .venue
            .instrument(pair)
            .map_err(|e| Stop(RunStatus::Failed, format!("{pair}: {e}")))?;
        let leg = format!("direct:{from}>{to}");
        let reference = self.reference_for(&inst);
        let outcome = if inst.base().eq_ignore_ascii_case(from) {
            self.execute(&leg, &inst, Side::Sell, qty_from, None, reference)?
        } else {
            self.execute(&leg, &inst, Side::Buy, 0.0, Some(qty_from), reference)?
        };
        match outcome {
            OrderOutcome::Report(r) if r.status != OrderStatus::Filled => self
                .rec
                .notes
                .push(format!("{leg}: {:?}; the rest goes through {}", r.status, self.req.quote)),
            OrderOutcome::Refused(e) => {
                self.direct_skipped = true;
                self.rec.notes.push(format!("{leg}: refused ({e}); through {} instead", self.req.quote));
            }
            OrderOutcome::TooSmall => {
                self.direct_skipped = true;
                self.rec.notes.push(format!("{leg}: below the pair's minimum; through {}", self.req.quote));
            }
            OrderOutcome::Report(_) => {
                // Filled: what is left of `from` is the sizing room.
                let left = self.holdings.get(from).copied().unwrap_or(0.0);
                if left > 0.0 {
                    self.rec.dust.push(format!("{from}: {left} left from the direct order's sizing room"));
                }
                self.dusted.insert(from.to_string());
            }
        }
        Ok(())
    }

    /// One IOC limit order at the slippage cap: guards, write-ahead, send,
    /// settle, book. `spend` sizes a buy in the instrument's quote.
    fn execute(
        &mut self,
        leg: &str,
        inst: &Instrument,
        side: Side,
        qty: f64,
        spend: Option<f64>,
        reference: Option<f64>,
    ) -> Result<OrderOutcome, Stop> {
        let t = Instant::now();
        let book = self
            .venue
            .order_book(inst)
            .map_err(|e| Stop(RunStatus::Failed, format!("{}: order book: {e}", inst.pair())))?;
        let slip = self.engine.guards.max_slippage;
        let (best, limit) = match side {
            Side::Buy => {
                let best = book.best_ask().ok_or_else(|| Stop(RunStatus::Failed, format!("{}: no asks", inst.pair())))?;
                (best, inst.round_price(best * (1.0 + slip), Side::Buy))
            }
            Side::Sell => {
                let best = book.best_bid().ok_or_else(|| Stop(RunStatus::Failed, format!("{}: no bids", inst.pair())))?;
                (best, inst.round_price(best * (1.0 - slip), Side::Sell))
            }
        };
        let fee = self.taker_fee(inst);
        // A buy is sized from the book, with 0.1 % room for a price move
        // between reading the book and the order arriving.
        let qty = match spend {
            Some(cash) => inst.filters.round_qty_down(book.walk_buy_quote(cash * (1.0 - fee) / 1.001, Some(limit)).base),
            None => inst.filters.round_qty_down(qty),
        };
        if !inst.tradable(qty, best) {
            return Ok(OrderOutcome::TooSmall);
        }
        let walk = match side {
            Side::Buy => book.walk_buy_base(qty, None),
            Side::Sell => book.walk_sell_base(qty, None),
        };
        let expected_slippage = if walk.complete && walk.base > 0.0 {
            (walk.avg_price() / best - 1.0).abs()
        } else {
            f64::INFINITY
        };
        let quote_usd = self.quote_usd(inst)?;
        let mid = book.mid().unwrap_or(best);
        let value = qty * mid * quote_usd;
        let now = now_rfc3339();
        let day = now.get(..10).unwrap_or("").to_string();
        let turnover = self.store.turnover(&self.req.exchange_id, &day).map_err(|e| Stop(RunStatus::Failed, e))?;
        let pair = inst.pair();
        let checks = guards::order_guards(
            &self.engine.guards,
            &OrderCheck {
                pair: &pair,
                value,
                venue_price: mid,
                reference_price: reference,
                expected_slippage,
                turnover_today: turnover,
            },
        );
        let passed = guards::all_pass(&checks);
        let why = guards::first_failure(&checks);
        self.rec.guards.extend(checks);
        if !passed {
            self.save()?;
            return Err(Stop(RunStatus::GuardStopped, why.unwrap_or_default()));
        }

        // Write-ahead: the order exists in our books before the venue sees it.
        let seq = self.orders.len() as u32 + 1;
        let client_order_id = format!("qs{}-{:02}", compact(&self.req.run_id), seq);
        let request = OrderRequest {
            client_order_id: client_order_id.clone(),
            instrument: inst.clone(),
            side,
            qty,
            limit_price: limit,
            reduce_only: false,
        };
        self.orders.push(OrderRecord {
            client_order_id: client_order_id.clone(),
            run_id: self.req.run_id.clone(),
            seq,
            leg: leg.to_string(),
            instrument: inst.clone(),
            side,
            qty,
            limit_price: limit,
            state: OrderState::Sending,
            report: None,
            error: None,
            value,
            quote_usd,
            sent_at: now.clone(),
            updated_at: now,
        });
        let i = self.orders.len() - 1;
        self.store.save_order(&self.orders[i], &self.req.exchange_id).map_err(|e| Stop(RunStatus::Failed, e))?;

        let outcome = match self.venue.place(&request) {
            Ok(report) => {
                self.orders[i].state = OrderState::Done;
                self.orders[i].report = Some(report.clone());
                if report.status == OrderStatus::Rejected {
                    OrderOutcome::Refused(report.message.clone().unwrap_or_else(|| "rejected".into()))
                } else {
                    OrderOutcome::Report(report)
                }
            }
            // An error may still mean the order went through: ask.
            Err(e) => match self.venue.query_by_client_id(inst, &client_order_id) {
                Ok(Some(report)) => {
                    self.rec.notes.push(format!("{client_order_id}: the venue answered \"{e}\" but has the order"));
                    self.orders[i].state = OrderState::Done;
                    self.orders[i].report = Some(report.clone());
                    OrderOutcome::Report(report)
                }
                Ok(None) => {
                    self.orders[i].state = OrderState::NotSent;
                    self.orders[i].error = Some(e.clone());
                    OrderOutcome::Refused(e)
                }
                Err(e2) => {
                    self.orders[i].state = OrderState::Unknown;
                    self.orders[i].error = Some(format!("{e}; asking failed too: {e2}"));
                    self.orders[i].updated_at = now_rfc3339();
                    let _ = self.store.save_order(&self.orders[i], &self.req.exchange_id);
                    return Err(Stop(
                        RunStatus::Interrupted,
                        format!("{client_order_id}: outcome unknown ({e}); the venue cannot be asked ({e2})"),
                    ));
                }
            },
        };
        self.orders[i].updated_at = now_rfc3339();
        self.store.save_order(&self.orders[i], &self.req.exchange_id).map_err(|e| Stop(RunStatus::Failed, e))?;
        if let OrderOutcome::Report(_) = outcome {
            self.book(i);
        }
        *self.rec.timings.entry("orders".into()).or_insert(0) += t.elapsed().as_millis() as u64;
        self.save()?;
        Ok(outcome)
    }

    /// Apply a fill to holdings, cost and the journal.
    fn book(&mut self, i: usize) {
        let o = self.orders[i].clone();
        let Some(r) = o.filled() else { return };
        apply_fill(&mut self.holdings, &o.instrument, o.side, r);
        let fee_usd = r.fee_quote * o.quote_usd;
        self.rec.fees += fee_usd;
        // Cost against the signal: fees plus the fill's distance from the reference.
        let slip_usd = match self.reference_for(&o.instrument) {
            Some(reference) => match o.side {
                Side::Buy => (r.avg_price - reference) * r.filled_qty * o.quote_usd,
                Side::Sell => (reference - r.avg_price) * r.filled_qty * o.quote_usd,
            },
            None => 0.0,
        };
        self.rec.cost += fee_usd + slip_usd;
        let at = now_rfc3339();
        if let Err(e) = self.store.book_fill(&FillBooking {
            run_id: &self.req.run_id,
            owner: &self.req.owner,
            exchange_id: &self.req.exchange_id,
            mode: self.req.mode.as_str(),
            account_quote: &self.req.quote,
            instrument: &o.instrument,
            side: o.side,
            report: r,
            quote_usd: o.quote_usd,
            at: &at,
        }) {
            self.rec.errors.push(format!("journal: {e}"));
        }
    }
}
