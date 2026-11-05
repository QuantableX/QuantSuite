//! The guards (PLAN-AUTO-EXECUTION §2.2 step 4). Every one must pass, or the
//! run stops with the reason. Run-level guards are checked before anything
//! is read from the venue; order-level guards before each order is sent.

use serde::{Deserialize, Serialize};

/// User limits. `None` = not set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardConfig {
    /// Max value of one order, in the account's quote.
    #[serde(default)]
    pub max_order_value: Option<f64>,
    /// Max traded value per UTC day on the account.
    #[serde(default)]
    pub daily_turnover_cap: Option<f64>,
    /// A perp position must open at least this far from its liquidation
    /// price, as a share of the entry (default 20 %, Venues 4/5).
    #[serde(default = "default_min_liquidation_distance")]
    pub min_liquidation_distance: f64,
}

fn default_min_liquidation_distance() -> f64 {
    0.2
}

/// How far an isolated position at `leverage` opens from its liquidation
/// price, as a share of the entry: the margin (1 / leverage) less the
/// maintenance margin. The same both ways, fees and funding aside.
pub fn liquidation_distance(leverage: f64, maintenance_margin: f64) -> f64 {
    (1.0 / leverage.max(1.0) - maintenance_margin).max(0.0)
}

/// The liquidation price of an isolated position opened at `entry`.
pub fn liquidation_price(entry: f64, leverage: f64, maintenance_margin: f64, short: bool) -> f64 {
    let d = liquidation_distance(leverage, maintenance_margin);
    if short {
        entry * (1.0 + d)
    } else {
        entry * (1.0 - d)
    }
}

pub fn liquidation_guard(pair: &str, distance: f64, min: f64) -> GuardResult {
    GuardResult::new(
        "liquidation_distance",
        distance + 1e-12 >= min,
        format!("{pair}: liquidation {:.1} % from the entry (limit {:.1} %)", distance * 100.0, min * 100.0),
    )
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            max_order_value: None,
            daily_turnover_cap: None,
            min_liquidation_distance: default_min_liquidation_distance(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardResult {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

impl GuardResult {
    fn new(name: &str, ok: bool, detail: impl Into<String>) -> Self {
        Self { name: name.to_string(), ok, detail: detail.into() }
    }
}

/// What the run-level guards look at.
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    /// The signal is from the candle that just closed (the caller checks).
    pub data_fresh: bool,
    pub kill_switch: bool,
    /// The account is the one this system / bot is bound to.
    pub account_belongs: bool,
    pub live: bool,
    pub armed: bool,
}

pub fn run_guards(ctx: &RunContext) -> Vec<GuardResult> {
    vec![
        GuardResult::new(
            "data_fresh",
            ctx.data_fresh,
            if ctx.data_fresh { "signal from the candle that just closed" } else { "the signal is not from the candle that just closed" },
        ),
        GuardResult::new(
            "kill_switch",
            !ctx.kill_switch,
            if ctx.kill_switch { "\"Stop all trading\" is on" } else { "off" },
        ),
        GuardResult::new(
            "account",
            ctx.account_belongs,
            if ctx.account_belongs { "the account belongs to this system" } else { "the account is not the one this system is bound to" },
        ),
        GuardResult::new(
            "armed",
            !ctx.live || ctx.armed,
            match (ctx.live, ctx.armed) {
                (false, _) => "paper",
                (true, true) => "live is armed",
                (true, false) => "live is not armed",
            },
        ),
    ]
}

/// One order, as the order-level guards see it.
#[derive(Debug, Clone)]
pub struct OrderCheck<'a> {
    pub pair: &'a str,
    /// Value in the account's quote.
    pub value: f64,
    /// The venue's mid for the instrument.
    pub venue_price: f64,
    /// The signal's reference for the same instrument (a cross price for a
    /// direct pair: X in USD ÷ Y in USD); `None` = no reference given.
    pub reference_price: Option<f64>,
    /// A buy: the one order where a far-off price can mean the wrong coin.
    pub buying: bool,
    /// Value already traded on the account today (UTC), this run included.
    pub turnover_today: f64,
}

/// A buy stops when the venue's price is this many times away from the signal's:
/// no market moves that far between the close and the order, so it is another
/// coin under the same symbol or a broken book.
pub const WRONG_COIN_FACTOR: f64 = 3.0;

/// Orders follow the system's signal. How far the price moved since the close and
/// how deep the book is stop nothing (user rule, 2026-11-05): if it drops a lot,
/// that happens. What is left is the wrong-coin check and the user's own limits.
pub fn order_guards(cfg: &GuardConfig, o: &OrderCheck) -> Vec<GuardResult> {
    let mut out = Vec::new();
    if let (true, Some(reference)) = (o.buying && o.venue_price > 0.0, o.reference_price.filter(|r| *r > 0.0)) {
        let apart = (o.venue_price / reference).max(reference / o.venue_price);
        out.push(GuardResult::new(
            "wrong_coin",
            apart <= WRONG_COIN_FACTOR,
            format!(
                "{}: venue {} vs signal {} is {apart:.1} times apart, so this is not the same coin (limit {WRONG_COIN_FACTOR} times)",
                o.pair,
                fmt_price(o.venue_price),
                fmt_price(reference),
            ),
        ));
    }
    if let Some(max) = cfg.max_order_value {
        out.push(GuardResult::new(
            "max_order_value",
            o.value <= max + 1e-9,
            format!("{}: order {:.2} (limit {:.2})", o.pair, o.value, max),
        ));
    }
    if let Some(cap) = cfg.daily_turnover_cap {
        out.push(GuardResult::new(
            "daily_turnover",
            o.turnover_today + o.value <= cap + 1e-9,
            format!("{:.2} traded today + {:.2} (cap {:.2})", o.turnover_today, o.value, cap),
        ));
    }
    out
}

fn fmt_price(p: f64) -> String {
    if p >= 100.0 {
        format!("{p:.2}")
    } else if p >= 1.0 {
        format!("{p:.4}")
    } else {
        format!("{p:.8}")
    }
}

pub fn all_pass(results: &[GuardResult]) -> bool {
    results.iter().all(|g| g.ok)
}

pub fn first_failure(results: &[GuardResult]) -> Option<String> {
    results.iter().find(|g| !g.ok).map(|g| format!("{}: {}", g.name, g.detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_run() -> RunContext {
        RunContext { data_fresh: true, kill_switch: false, account_belongs: true, live: true, armed: true }
    }

    #[test]
    fn every_run_guard_refuses_with_its_reason() {
        assert!(all_pass(&run_guards(&ok_run())));
        for (ctx, name) in [
            (RunContext { data_fresh: false, ..ok_run() }, "data_fresh"),
            (RunContext { kill_switch: true, ..ok_run() }, "kill_switch"),
            (RunContext { account_belongs: false, ..ok_run() }, "account"),
            (RunContext { armed: false, ..ok_run() }, "armed"),
        ] {
            let results = run_guards(&ctx);
            assert!(!all_pass(&results));
            assert!(first_failure(&results).unwrap().starts_with(name), "{name}");
        }
        // Paper needs no arming.
        assert!(all_pass(&run_guards(&RunContext { live: false, armed: false, ..ok_run() })));
    }

    fn check() -> OrderCheck<'static> {
        OrderCheck {
            pair: "BTC/USDT",
            value: 1_000.0,
            venue_price: 100_000.0,
            reference_price: Some(100_500.0),
            buying: true,
            turnover_today: 0.0,
        }
    }

    #[test]
    fn every_order_guard_refuses_with_its_reason() {
        let cfg = GuardConfig { max_order_value: Some(5_000.0), daily_turnover_cap: Some(20_000.0), ..GuardConfig::default() };
        assert!(all_pass(&order_guards(&cfg, &check())));
        for (o, name) in [
            (OrderCheck { venue_price: 400_000.0, ..check() }, "wrong_coin"),
            (OrderCheck { venue_price: 30_000.0, ..check() }, "wrong_coin"),
            (OrderCheck { value: 6_000.0, ..check() }, "max_order_value"),
            (OrderCheck { turnover_today: 19_500.0, ..check() }, "daily_turnover"),
        ] {
            let results = order_guards(&cfg, &o);
            assert_eq!(first_failure(&results).unwrap().split(':').next().unwrap(), name);
        }
        // Unset limits are not checked.
        let results = order_guards(&GuardConfig::default(), &OrderCheck { value: 1e9, ..check() });
        assert!(all_pass(&results));
        assert!(!results.iter().any(|g| g.name == "max_order_value"));
    }

    /// If it drops a lot, that happens: no market move stops an order.
    #[test]
    fn a_price_move_stops_no_order() {
        let cfg = GuardConfig::default();
        for venue_price in [50_000.0, 90_000.0, 110_000.0, 200_000.0] {
            assert!(all_pass(&order_guards(&cfg, &OrderCheck { venue_price, ..check() })), "a buy at {venue_price}");
        }
        // A sell is never stopped by the price at all: it sells what the account holds.
        for venue_price in [1.0, 1_000_000.0] {
            let o = OrderCheck { buying: false, venue_price, ..check() };
            assert!(all_pass(&order_guards(&cfg, &o)), "a sell at {venue_price}");
        }
        // No reference is no reason to stop either.
        assert!(all_pass(&order_guards(&cfg, &OrderCheck { reference_price: None, ..check() })));
    }
}
