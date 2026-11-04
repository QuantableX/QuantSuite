//! The guards (PLAN-AUTO-EXECUTION §2.2 step 4). Every one must pass, or the
//! run stops with the reason. Run-level guards are checked before anything
//! is read from the venue; order-level guards before each order is sent.

use serde::{Deserialize, Serialize};

/// User limits. `None` = not set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardConfig {
    /// Max slippage per order against the best price (default 0.5 %); the
    /// IOC limit sits at this cap.
    pub max_slippage: f64,
    /// The venue's price must be within this share of the signal's
    /// reference price (default 2 %).
    pub price_tolerance: f64,
    /// Max value of one order, in the account's quote.
    #[serde(default)]
    pub max_order_value: Option<f64>,
    /// Max traded value per UTC day on the account.
    #[serde(default)]
    pub daily_turnover_cap: Option<f64>,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self { max_slippage: 0.005, price_tolerance: 0.02, max_order_value: None, daily_turnover_cap: None }
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
    /// Average fill of the walk for the full size against the best price.
    pub expected_slippage: f64,
    /// Value already traded on the account today (UTC), this run included.
    pub turnover_today: f64,
}

pub fn order_guards(cfg: &GuardConfig, o: &OrderCheck) -> Vec<GuardResult> {
    let mut out = Vec::new();
    match o.reference_price {
        Some(reference) if reference > 0.0 && o.venue_price > 0.0 => {
            let gap = (o.venue_price / reference - 1.0).abs();
            out.push(GuardResult::new(
                "price_sanity",
                gap <= cfg.price_tolerance + 1e-12,
                format!(
                    "{}: venue {} vs signal {} ({:.2} %, limit {:.2} %)",
                    o.pair,
                    fmt_price(o.venue_price),
                    fmt_price(reference),
                    gap * 100.0,
                    cfg.price_tolerance * 100.0
                ),
            ));
        }
        _ => out.push(GuardResult::new("price_sanity", false, format!("{}: no reference price to check against", o.pair))),
    }
    out.push(GuardResult::new(
        "max_slippage",
        o.expected_slippage <= cfg.max_slippage + 1e-12,
        format!(
            "{}: the book fills this size {:.3} % from the best price (cap {:.2} %)",
            o.pair,
            o.expected_slippage * 100.0,
            cfg.max_slippage * 100.0
        ),
    ));
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
            expected_slippage: 0.001,
            turnover_today: 0.0,
        }
    }

    #[test]
    fn every_order_guard_refuses_with_its_reason() {
        let cfg = GuardConfig { max_order_value: Some(5_000.0), daily_turnover_cap: Some(20_000.0), ..GuardConfig::default() };
        assert!(all_pass(&order_guards(&cfg, &check())));
        for (o, name) in [
            (OrderCheck { venue_price: 103_000.0, ..check() }, "price_sanity"),
            (OrderCheck { reference_price: None, ..check() }, "price_sanity"),
            (OrderCheck { expected_slippage: 0.006, ..check() }, "max_slippage"),
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
}
