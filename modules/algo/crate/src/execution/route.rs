//! Route choice for a switch X → Y (PLAN-AUTO-EXECUTION §2.2 "Route choice",
//! user decision 2026-11-03): the cheaper of a direct pair (one trade) and
//! the way through the account's quote asset (sell, then buy), estimated for
//! exactly this size from the fee tier plus spread and slippage of a walk of
//! the live books. Direct only when it saves at least 0.02 % of the switch
//! value; otherwise, and whenever an estimate is unsure, through USD.

use serde::{Deserialize, Serialize};

use super::venue::{Instrument, OrderBook, Side};

/// The direct pair must beat the way through USD by this share of the value.
pub const DIRECT_MIN_SAVING: f64 = 0.0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    ViaQuote,
    Direct,
}

/// One route's estimated cost in USD (the account's quote).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteEstimate {
    pub kind: RouteKind,
    pub trades: usize,
    /// The switch value at mid prices.
    pub value: f64,
    pub fees: f64,
    /// Spread and slippage: the walk's price against the mid.
    pub spread: f64,
    pub total: f64,
    /// The books covered the whole size and every input was there.
    pub complete: bool,
}

/// The direct market for a switch, when the venue lists one.
#[derive(Debug, Clone)]
pub struct DirectMarket<'a> {
    pub instrument: Instrument,
    pub book: &'a OrderBook,
    pub taker_fee: f64,
}

impl DirectMarket<'_> {
    /// The side that turns `from` into the other asset: sell when `from` is
    /// the pair's base (X/Y), buy when it is the quote (Y/X).
    pub fn side_for(&self, from: &str) -> Side {
        if self.instrument.base().eq_ignore_ascii_case(from) {
            Side::Sell
        } else {
            Side::Buy
        }
    }
}

pub struct SwitchInputs<'a> {
    pub from: &'a str,
    pub to: &'a str,
    /// The quantity of `from` the switch moves.
    pub qty_from: f64,
    /// `from/QUOTE` and `to/QUOTE`.
    pub from_book: &'a OrderBook,
    pub to_book: &'a OrderBook,
    pub from_fee: f64,
    pub to_fee: f64,
    pub direct: Option<DirectMarket<'a>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteChoice {
    pub chosen: RouteKind,
    pub via_quote: RouteEstimate,
    pub direct: Option<RouteEstimate>,
    /// `BASE/QUOTE` of the direct pair, when there is one.
    pub direct_pair: Option<String>,
    pub reason: String,
}

fn via_quote(i: &SwitchInputs) -> RouteEstimate {
    let mid_from = i.from_book.mid().unwrap_or(0.0);
    let mid_to = i.to_book.mid().unwrap_or(0.0);
    let value = i.qty_from * mid_from;
    let sell = i.from_book.walk_sell_base(i.qty_from, None);
    let sell_spread = (value - sell.quote).max(0.0);
    let sell_fee = sell.quote * i.from_fee;
    let spend = sell.quote - sell_fee;
    let buy = i.to_book.walk_buy_quote(spend, None);
    let buy_spread = (spend - buy.base * mid_to).max(0.0);
    let buy_fee = spend * i.to_fee;
    let fees = sell_fee + buy_fee;
    let spread = sell_spread + buy_spread;
    RouteEstimate {
        kind: RouteKind::ViaQuote,
        trades: 2,
        value,
        fees,
        spread,
        total: fees + spread,
        complete: sell.complete && buy.complete && mid_from > 0.0 && mid_to > 0.0,
    }
}

fn direct(i: &SwitchInputs, d: &DirectMarket) -> RouteEstimate {
    let mid_from = i.from_book.mid().unwrap_or(0.0);
    let mid_to = i.to_book.mid().unwrap_or(0.0);
    let value = i.qty_from * mid_from;
    let mid_pair = d.book.mid().unwrap_or(0.0);
    let (fees, spread, complete) = match d.side_for(i.from) {
        // X/Y: sell X for Y; costs are in Y.
        Side::Sell => {
            let w = d.book.walk_sell_base(i.qty_from, None);
            let spread_y = (i.qty_from * mid_pair - w.quote).max(0.0);
            (w.quote * d.taker_fee * mid_to, spread_y * mid_to, w.complete)
        }
        // Y/X: spend X on Y; costs are in X.
        Side::Buy => {
            let w = d.book.walk_buy_quote(i.qty_from, None);
            let spread_x = if mid_pair > 0.0 { (w.quote - w.base * mid_pair).max(0.0) } else { 0.0 };
            (w.quote * d.taker_fee * mid_from, spread_x * mid_from, w.complete)
        }
    };
    RouteEstimate {
        kind: RouteKind::Direct,
        trades: 1,
        value,
        fees,
        spread,
        total: fees + spread,
        complete: complete && mid_pair > 0.0 && mid_from > 0.0 && mid_to > 0.0,
    }
}

/// Estimate both routes and pick one.
pub fn choose(i: &SwitchInputs) -> RouteChoice {
    let via = via_quote(i);
    let Some(d) = i.direct.as_ref() else {
        return RouteChoice {
            chosen: RouteKind::ViaQuote,
            via_quote: via,
            direct: None,
            direct_pair: None,
            reason: format!("the venue lists no {}/{} pair", i.from, i.to),
        };
    };
    let est = direct(i, d);
    let saving = via.total - est.total;
    let needed = DIRECT_MIN_SAVING * via.value;
    let (chosen, reason) = if !est.complete {
        (RouteKind::ViaQuote, format!("the {} book is too thin for this size", d.instrument.pair()))
    } else if !via.complete {
        (RouteKind::ViaQuote, "an estimate is unsure (a book is too thin or missing)".to_string())
    } else if saving + 1e-9 >= needed {
        (
            RouteKind::Direct,
            format!("direct saves {:.2} ({:.3} %) against {:.2} through the quote asset", saving, saving / via.value.max(1e-9) * 100.0, via.total),
        )
    } else {
        (
            RouteKind::ViaQuote,
            format!("direct would save {:.2}, below the 0.02 % ({:.2}) it must save", saving.max(0.0), needed),
        )
    };
    RouteChoice {
        chosen,
        via_quote: via,
        direct: Some(est),
        direct_pair: Some(d.instrument.pair()),
        reason,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::broker::SymbolFilters;

    pub fn inst(base: &str, quote: &str, tick: f64, step: f64) -> Instrument {
        Instrument::spot(SymbolFilters {
            symbol: format!("{base}{quote}"),
            base: base.into(),
            quote: quote.into(),
            qty_step: step,
            min_qty: step,
            // 5$ on a USD pair; Binance's 0.0001 BTC on a BTC pair.
            min_notional: if quote == "BTC" { 0.0001 } else { 5.0 },
            price_tick: tick,
        })
    }

    /// A deep book around `mid` with a relative half-spread.
    pub fn deep(mid: f64, half_spread: f64, levels: usize, size: f64) -> OrderBook {
        let mut book = OrderBook::default();
        for k in 0..levels {
            let step = mid * half_spread * (1.0 + k as f64);
            book.bids.push((mid - step, size));
            book.asks.push((mid + step, size));
        }
        book
    }

    #[test]
    fn a_liquid_direct_pair_wins_10_dollars_against_20() {
        // 10,000$ of BTC → ETH, Binance-like 0.1 % taker everywhere.
        let btc = deep(100_000.0, 0.000_001, 20, 1.0);
        let eth = deep(4_000.0, 0.000_001, 20, 25.0);
        let ethbtc = deep(0.04, 0.000_01, 20, 50.0);
        let i = SwitchInputs {
            from: "BTC",
            to: "ETH",
            qty_from: 0.1,
            from_book: &btc,
            to_book: &eth,
            from_fee: 0.001,
            to_fee: 0.001,
            direct: Some(DirectMarket { instrument: inst("ETH", "BTC", 0.00001, 0.0001), book: &ethbtc, taker_fee: 0.001 }),
        };
        let c = choose(&i);
        assert_eq!(c.chosen, RouteKind::Direct, "{}", c.reason);
        assert!((c.via_quote.fees - 19.99).abs() < 0.05, "two legs ≈ 20$: {:?}", c.via_quote);
        let d = c.direct.unwrap();
        assert!((d.fees - 10.0).abs() < 0.05, "one leg ≈ 10$: {d:?}");
        assert!(d.spread < 1.0);
        assert_eq!(d.trades, 1);
    }

    #[test]
    fn a_thin_cross_pair_goes_through_usd() {
        let btc = deep(100_000.0, 0.000_001, 20, 1.0);
        let eth = deep(4_000.0, 0.000_001, 20, 25.0);
        // The cross book is 2 % wide per level: walking it costs far more than 10$.
        let ethbtc = deep(0.04, 0.02, 20, 50.0);
        let i = SwitchInputs {
            from: "BTC",
            to: "ETH",
            qty_from: 0.1,
            from_book: &btc,
            to_book: &eth,
            from_fee: 0.001,
            to_fee: 0.001,
            direct: Some(DirectMarket { instrument: inst("ETH", "BTC", 0.00001, 0.0001), book: &ethbtc, taker_fee: 0.001 }),
        };
        let c = choose(&i);
        assert_eq!(c.chosen, RouteKind::ViaQuote, "{}", c.reason);
        assert!(c.direct.unwrap().total > c.via_quote.total);
    }

    #[test]
    fn a_saving_under_two_basis_points_goes_through_usd() {
        // Fees: through USD 0.01 % + 0.01 %, direct 0.01 % → saves 1 bp < 2 bp.
        let btc = deep(100_000.0, 0.000_000_1, 20, 1.0);
        let eth = deep(4_000.0, 0.000_000_1, 20, 25.0);
        let ethbtc = deep(0.04, 0.000_000_1, 20, 50.0);
        let i = SwitchInputs {
            from: "BTC",
            to: "ETH",
            qty_from: 0.1,
            from_book: &btc,
            to_book: &eth,
            from_fee: 0.0001,
            to_fee: 0.0001,
            direct: Some(DirectMarket { instrument: inst("ETH", "BTC", 0.00001, 0.0001), book: &ethbtc, taker_fee: 0.0001 }),
        };
        let c = choose(&i);
        assert_eq!(c.chosen, RouteKind::ViaQuote, "{}", c.reason);
        assert!(c.reason.contains("below the 0.02 %"), "{}", c.reason);
    }

    #[test]
    fn a_venue_without_the_pair_goes_through_usd() {
        let btc = deep(100_000.0, 0.000_001, 20, 1.0);
        let eth = deep(4_000.0, 0.000_001, 20, 25.0);
        let i = SwitchInputs {
            from: "BTC",
            to: "ETH",
            qty_from: 0.1,
            from_book: &btc,
            to_book: &eth,
            from_fee: 0.001,
            to_fee: 0.001,
            direct: None,
        };
        let c = choose(&i);
        assert_eq!(c.chosen, RouteKind::ViaQuote);
        assert!(c.direct.is_none());
    }

    #[test]
    fn selling_the_base_of_a_direct_pair_is_estimated_too() {
        // ETH → BTC on ETH/BTC: sell the base.
        let btc = deep(100_000.0, 0.000_001, 20, 1.0);
        let eth = deep(4_000.0, 0.000_001, 20, 25.0);
        let ethbtc = deep(0.04, 0.000_01, 20, 50.0);
        let i = SwitchInputs {
            from: "ETH",
            to: "BTC",
            qty_from: 2.5,
            from_book: &eth,
            to_book: &btc,
            from_fee: 0.001,
            to_fee: 0.001,
            direct: Some(DirectMarket { instrument: inst("ETH", "BTC", 0.00001, 0.0001), book: &ethbtc, taker_fee: 0.001 }),
        };
        let c = choose(&i);
        assert_eq!(c.chosen, RouteKind::Direct, "{}", c.reason);
        assert!((c.direct.unwrap().fees - 10.0).abs() < 0.05);
    }
}
