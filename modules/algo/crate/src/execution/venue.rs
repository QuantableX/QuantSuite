//! What the execution core needs from a place that trades — `ExecutionVenue`
//! (PLAN-AUTO-EXECUTION §2.2) — and the shapes that cross it: instruments,
//! balances, order books, orders and their reports.
//!
//! Binance spot and Bybit spot implement it in [`super::cex`]; the
//! [`super::paper::PaperVenue`] implements it over a real venue's public book.
//! Hyperliquid, Jupiter and Lighter plug in later (PLAN-EXECUTION-VENUES).

use serde::{Deserialize, Serialize};

use crate::broker::SymbolFilters;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketType {
    Spot,
    Perp,
}

/// What a venue can do. The engine asks before it relies on a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub spot: bool,
    pub perp: bool,
    /// Immediate-or-cancel limit orders — the engine's order type.
    pub ioc_limit: bool,
    /// Orders carry our id and can be looked up by it (idempotency).
    pub client_order_ids: bool,
    pub sub_accounts: bool,
    /// A dry run the venue validates but never executes (Binance `order/test`).
    pub test_orders: bool,
}

/// One tradable market: the lot and price rules plus the market type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    pub market: MarketType,
    #[serde(flatten)]
    pub filters: SymbolFilters,
}

impl Instrument {
    pub fn spot(filters: SymbolFilters) -> Self {
        Self { market: MarketType::Spot, filters }
    }

    /// `BASE/QUOTE`.
    pub fn pair(&self) -> String {
        format!("{}/{}", self.filters.base, self.filters.quote)
    }

    pub fn base(&self) -> &str {
        &self.filters.base
    }

    pub fn quote(&self) -> &str {
        &self.filters.quote
    }

    /// A limit price rounded to the tick in the direction that keeps it
    /// inside the cap: down for a buy, up for a sell.
    pub fn round_price(&self, price: f64, side: Side) -> f64 {
        let tick = self.filters.price_tick;
        if tick <= 0.0 || price <= 0.0 {
            return price;
        }
        let steps = price / tick;
        let steps = match side {
            Side::Buy => (steps + 1e-9).floor(),
            Side::Sell => (steps - 1e-9).ceil(),
        };
        let decimals = crate::broker::decimals_of(tick);
        format!("{:.decimals$}", steps * tick).parse().unwrap_or(steps * tick)
    }

    pub fn price_string(&self, price: f64) -> String {
        let decimals = crate::broker::decimals_of(self.filters.price_tick);
        let mut s = format!("{price:.decimals$}");
        if decimals > 0 {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }

    /// Whether `qty` at `price` clears the venue's minimums.
    pub fn tradable(&self, qty: f64, price: f64) -> bool {
        qty > 0.0 && qty + 1e-12 >= self.filters.min_qty && qty * price + 1e-9 >= self.filters.min_notional
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueBalance {
    pub asset: String,
    pub free: f64,
    pub locked: f64,
}

/// An open perp position (spot venues report none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenuePosition {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry_price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
}

/// An immediate-or-cancel limit order: fills what the book offers up to the
/// limit price, cancels the rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRequest {
    pub client_order_id: String,
    pub instrument: Instrument,
    pub side: Side,
    /// Base quantity, already on the lot step.
    pub qty: f64,
    /// The slippage cap, on the tick.
    pub limit_price: f64,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Filled,
    /// IOC: part filled, the rest cancelled.
    PartiallyFilled,
    /// IOC: nothing at or inside the limit; cancelled.
    NotFilled,
    Rejected,
    /// Still working on the venue (should not happen with IOC).
    Open,
}

/// What the venue says an order did, normalised across venues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderReport {
    pub client_order_id: String,
    pub venue_order_id: String,
    pub status: OrderStatus,
    /// Base quantity executed (gross).
    pub filled_qty: f64,
    /// Base actually credited on a buy — gross minus a fee charged in base.
    pub base_received: f64,
    pub avg_price: f64,
    /// Quote value of the execution, gross of any fee.
    pub quote_amount: f64,
    /// The fee in the asset the venue charged.
    pub fee_asset: String,
    pub fee_amount: f64,
    /// The fee valued in the instrument's quote.
    pub fee_quote: f64,
    /// True when `fee_quote` is a valuation, not a venue figure.
    pub fee_estimated: bool,
    #[serde(default)]
    pub message: Option<String>,
}

impl OrderReport {
    /// A report for an order that executed nothing.
    pub fn empty(client_order_id: &str, status: OrderStatus, message: impl Into<String>) -> Self {
        Self {
            client_order_id: client_order_id.to_string(),
            venue_order_id: String::new(),
            status,
            filled_qty: 0.0,
            base_received: 0.0,
            avg_price: 0.0,
            quote_amount: 0.0,
            fee_asset: String::new(),
            fee_amount: 0.0,
            fee_quote: 0.0,
            fee_estimated: false,
            message: Some(message.into()),
        }
    }

    /// The fee that leaves the quote balance (a fee in base is already netted
    /// out of `base_received`; a fee in a third asset leaves another balance).
    pub fn fee_in(&self, asset: &str) -> f64 {
        if self.fee_asset.eq_ignore_ascii_case(asset) {
            self.fee_amount
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FeeSchedule {
    pub maker: f64,
    pub taker: f64,
}

/// Price levels, best first: `(price, base quantity)`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrderBook {
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}

/// The result of walking a book for a size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Walk {
    /// Base quantity taken.
    pub base: f64,
    /// Quote spent (buy) or received (sell).
    pub quote: f64,
    /// The book (within the limit) covered the whole size.
    pub complete: bool,
}

impl Walk {
    pub fn avg_price(&self) -> f64 {
        if self.base > 0.0 {
            self.quote / self.base
        } else {
            0.0
        }
    }
}

impl OrderBook {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|l| l.0).filter(|p| *p > 0.0)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|l| l.0).filter(|p| *p > 0.0)
    }

    pub fn mid(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => Some((a + b) / 2.0),
            (Some(p), None) | (None, Some(p)) => Some(p),
            _ => None,
        }
    }

    /// Buy `qty` base from the asks, not above `limit`.
    pub fn walk_buy_base(&self, qty: f64, limit: Option<f64>) -> Walk {
        walk(&self.asks, Amount::Base(qty), |p| limit.map_or(true, |l| p <= l + 1e-12))
    }

    /// Spend `quote` on the asks, not above `limit`.
    pub fn walk_buy_quote(&self, quote: f64, limit: Option<f64>) -> Walk {
        walk(&self.asks, Amount::Quote(quote), |p| limit.map_or(true, |l| p <= l + 1e-12))
    }

    /// Sell `qty` base into the bids, not below `limit`.
    pub fn walk_sell_base(&self, qty: f64, limit: Option<f64>) -> Walk {
        walk(&self.bids, Amount::Base(qty), |p| limit.map_or(true, |l| p + 1e-12 >= l))
    }
}

enum Amount {
    Base(f64),
    Quote(f64),
}

fn walk(levels: &[(f64, f64)], amount: Amount, allowed: impl Fn(f64) -> bool) -> Walk {
    let mut out = Walk::default();
    let target = match amount {
        Amount::Base(q) | Amount::Quote(q) => q,
    };
    if target <= 0.0 {
        out.complete = true;
        return out;
    }
    for &(price, size) in levels {
        if price <= 0.0 || size <= 0.0 || !allowed(price) {
            break;
        }
        match amount {
            Amount::Base(qty) => {
                let take = (qty - out.base).min(size);
                out.base += take;
                out.quote += take * price;
                if out.base + 1e-12 >= qty {
                    out.complete = true;
                    return out;
                }
            }
            Amount::Quote(quote) => {
                let take_quote = (quote - out.quote).min(size * price);
                out.base += take_quote / price;
                out.quote += take_quote;
                if out.quote + 1e-9 >= quote {
                    out.complete = true;
                    return out;
                }
            }
        }
    }
    out
}

/// The place orders go. `Send + Sync`: a bot runtime owns one, the paper venue borrows one.
pub trait ExecutionVenue: Send + Sync {
    /// "Binance spot", "Paper (Bybit spot book)", …
    fn label(&self) -> String;
    fn capabilities(&self) -> Capabilities;
    /// Fails before anything is sent when the venue cannot trade (no keys).
    fn ready(&self) -> Result<(), String>;
    /// Every spot instrument (direct pairs for the route choice).
    fn instruments(&self) -> Result<Vec<Instrument>, String>;
    /// One instrument by `BASE/QUOTE`.
    fn instrument(&self, pair: &str) -> Result<Instrument, String>;
    fn balances(&self) -> Result<Vec<VenueBalance>, String>;
    fn positions(&self) -> Result<Vec<VenuePosition>, String>;
    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String>;
    /// The account's fee tier for the instrument.
    fn fee_schedule(&self, instrument: &Instrument) -> Result<FeeSchedule, String>;
    /// Send an IOC limit order. `Err` means the outcome is unknown or the
    /// venue refused it — the engine looks the order up by its client id.
    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String>;
    fn cancel(&self, instrument: &Instrument, client_order_id: &str) -> Result<(), String>;
    /// `Ok(None)` when the venue never saw an order with this id.
    fn query_by_client_id(&self, instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String>;
    /// A dry run that is validated but never executed.
    fn test_order(&self, order: &OrderRequest) -> Result<String, String>;
}

pub fn free_balance(balances: &[VenueBalance], asset: &str) -> f64 {
    balances
        .iter()
        .filter(|b| b.asset.eq_ignore_ascii_case(asset))
        .map(|b| b.free)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> OrderBook {
        OrderBook {
            bids: vec![(99.0, 1.0), (98.0, 2.0)],
            asks: vec![(101.0, 1.0), (102.0, 2.0)],
        }
    }

    #[test]
    fn walks_take_levels_in_order_and_stop_at_the_limit() {
        let b = book();
        assert_eq!(b.mid(), Some(100.0));
        let w = b.walk_buy_base(2.0, None);
        assert!(w.complete);
        assert!((w.quote - 203.0).abs() < 1e-9);
        let w = b.walk_buy_base(2.0, Some(101.5));
        assert!(!w.complete);
        assert!((w.base - 1.0).abs() < 1e-12);
        let w = b.walk_buy_quote(152.0, None);
        assert!(w.complete);
        assert!((w.base - 1.5).abs() < 1e-9, "{w:?}");
        let w = b.walk_sell_base(2.5, None);
        assert!(w.complete);
        assert!((w.quote - (99.0 + 98.0 * 1.5)).abs() < 1e-9);
        let w = b.walk_sell_base(10.0, None);
        assert!(!w.complete, "the book is only 3 deep");
    }

    #[test]
    fn limit_prices_round_inside_the_cap() {
        let inst = Instrument::spot(SymbolFilters {
            symbol: "BTCUSDT".into(),
            base: "BTC".into(),
            quote: "USDT".into(),
            qty_step: 0.00001,
            min_qty: 0.00001,
            min_notional: 5.0,
            price_tick: 0.01,
        });
        assert_eq!(inst.round_price(100.019, Side::Buy), 100.01);
        assert_eq!(inst.round_price(100.011, Side::Sell), 100.02);
        assert_eq!(inst.round_price(100.01, Side::Sell), 100.01);
        assert_eq!(inst.price_string(100.10), "100.1");
        assert!(inst.tradable(0.001, 64_000.0));
        assert!(!inst.tradable(0.00005, 64_000.0), "3.2$ is below the 5$ minimum");
    }
}
