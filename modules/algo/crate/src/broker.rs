//! Live order routing for bots (PLAN-QUANTALGO §4) — Tauri-free.
//!
//! One [`Broker`] per live bot, built from the exchange's stored credentials:
//! symbol filters (lot step, minimum quantity and notional), the available
//! quote balance, and buys/sells whose fills come back normalised to one
//! [`Fill`] shape whatever the venue's fee convention. Binance spot and
//! Bybit spot (v5, unified account) are the venues with routing; the other
//! providers stay paper-only and the preflight says so.
//!
//! Since the execution core (PLAN-AUTO-EXECUTION §2.2) the venues are
//! [`crate::execution::cex::CexVenue`]s and a bot's "market" order goes out
//! the core's way: an IOC limit at the slippage cap with our client order id,
//! looked up by that id when the answer is lost, a partial fill retried up to
//! three times. The bot runtime keeps this narrow interface.
//!
//! Private calls go to the venue's sandbox when the exchange row says so
//! (Binance spot testnet, Bybit demo trading); public market data always
//! comes from production — the sandboxes carry thin or absent markets, and
//! a bot has to trade real prices (§4.2).

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::time::Duration;

use crate::execution::cex::CexVenue;
use crate::execution::venue::{free_balance, ExecutionVenue, Instrument, MarketType, OrderReport, OrderRequest, OrderStatus, Side};

type HmacSha256 = Hmac<Sha256>;

/// A fee charged in an asset that is neither base nor quote (Binance's BNB
/// discount) cannot be read off the fill; it is valued at Binance's standard
/// BNB-discounted taker rate and flagged as an estimate.
const THIRD_ASSET_FEE_RATE: f64 = 0.00075;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Venue {
    Binance,
    Bybit,
}

impl Venue {
    pub fn parse(provider: &str) -> Option<Self> {
        match provider.to_lowercase().as_str() {
            "binance" => Some(Venue::Binance),
            "bybit" => Some(Venue::Bybit),
            _ => None,
        }
    }

    pub fn supports_live(provider: &str) -> bool {
        Self::parse(provider).is_some()
    }

    pub fn label(self) -> &'static str {
        match self {
            Venue::Binance => "Binance spot",
            Venue::Bybit => "Bybit spot",
        }
    }

    pub fn public_base(self) -> &'static str {
        match self {
            Venue::Binance => "https://api.binance.com",
            Venue::Bybit => "https://api.bybit.com",
        }
    }

    pub fn private_base(self, sandbox: bool) -> &'static str {
        match (self, sandbox) {
            (Venue::Binance, false) => "https://api.binance.com",
            (Venue::Binance, true) => "https://testnet.binance.vision",
            (Venue::Bybit, false) => "https://api.bybit.com",
            (Venue::Bybit, true) => "https://api-demo.bybit.com",
        }
    }

    /// The venue's symbol for the module's `BASE/QUOTE` pair.
    pub fn symbol(self, pair: &str) -> String {
        pair.replace('/', "")
    }
}

/// What the venue allows for a symbol — every order is rounded and checked
/// against this before it is sent.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SymbolFilters {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    /// Base-quantity step (Binance LOT_SIZE.stepSize, Bybit basePrecision).
    pub qty_step: f64,
    pub min_qty: f64,
    /// Minimum order value in quote (Binance NOTIONAL.minNotional, Bybit minOrderAmt).
    pub min_notional: f64,
    pub price_tick: f64,
}

impl SymbolFilters {
    /// Round a base quantity down to the step — never up, an order for more
    /// than is held is rejected by the venue.
    pub fn round_qty_down(&self, qty: f64) -> f64 {
        if self.qty_step <= 0.0 {
            return qty;
        }
        let steps = (qty / self.qty_step + 1e-9).floor();
        let rounded = steps * self.qty_step;
        // Re-quantise through the string form to drop binary noise.
        self.qty_string(rounded).parse::<f64>().unwrap_or(rounded)
    }

    /// The quantity as the venue wants it written: the step's decimals, no more.
    pub fn qty_string(&self, qty: f64) -> String {
        let decimals = decimals_of(self.qty_step);
        let mut s = format!("{qty:.decimals$}");
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

    pub fn quote_string(&self, amount: f64) -> String {
        let mut s = format!("{amount:.2}");
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

/// Decimal places implied by a step like 0.001 (3) or 1 (0).
pub fn decimals_of(step: f64) -> usize {
    if step <= 0.0 {
        return 8;
    }
    let mut d = 0usize;
    let mut s = step;
    while d < 12 && (s - s.round()).abs() > 1e-9 {
        s *= 10.0;
        d += 1;
    }
    d
}

/// A completed market order, normalised across venues.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub order_id: String,
    pub avg_price: f64,
    /// Base quantity the venue executed (gross).
    pub base_qty: f64,
    /// Base quantity actually credited — gross minus a fee charged in base.
    pub base_received: f64,
    /// Quote value of the execution (gross of any fee).
    pub quote_amount: f64,
    /// The fee valued in quote, whatever asset it was charged in.
    pub fee_quote: f64,
    pub fee_asset: String,
    pub fee_amount: f64,
    /// True when the fee's quote value is a valuation, not a venue figure.
    pub fee_estimated: bool,
}

impl Fill {
    /// The fee charged in the quote asset — the part that leaves the bot's
    /// cash. A fee in base is already inside `base_received`; a fee in a
    /// third asset leaves another balance.
    pub fn fee_from_cash(&self, quote: &str) -> f64 {
        if self.fee_asset.eq_ignore_ascii_case(quote) {
            self.fee_amount
        } else {
            0.0
        }
    }
}

/// The venue behind a live bot. `Send` so the runtime can hold it behind a
/// mutex; a mock implements it in the runtime's tests.
pub trait Broker: Send {
    /// "Binance spot", "Hyperliquid perp (testnet)".
    fn label(&self) -> String;
    fn sandbox(&self) -> bool;
    /// The broker trades perp: a sell may open a short and a buy cover it
    /// (Venues 4/5). Spot brokers are long-only.
    fn perp(&self) -> bool {
        false
    }
    /// Open a short of `base_qty` (perp only).
    fn open_short(&self, _filters: &SymbolFilters, _base_qty: f64) -> Result<Fill, String> {
        Err("spot trading is long-only".into())
    }
    /// Buy back `base_qty` of a short, reduce-only (perp only).
    fn close_short(&self, _filters: &SymbolFilters, _base_qty: f64) -> Result<Fill, String> {
        Err("spot trading is long-only".into())
    }
    fn symbol_filters(&self, pair: &str) -> Result<SymbolFilters, String>;
    fn available_balance(&self, asset: &str) -> Result<f64, String>;
    fn market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<Fill, String>;
    fn market_sell(&self, filters: &SymbolFilters, base_qty: f64) -> Result<Fill, String>;
    /// A dry run the venue validates but never executes (Binance
    /// `order/test`). Venues without one report that instead of pretending.
    fn test_market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

/// Binance: HMAC-SHA256 of the exact query string, hex.
pub fn binance_signature(secret: &str, query: &str) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).expect("HMAC key length");
    mac.update(query.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Bybit v5: HMAC-SHA256 of `timestamp + api_key + recv_window + payload`
/// (the query string for GET, the JSON body for POST), hex.
pub fn bybit_signature(secret: &str, timestamp_ms: &str, api_key: &str, recv_window: &str, payload: &str) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).expect("HMAC key length");
    mac.update(timestamp_ms.as_bytes());
    mac.update(api_key.as_bytes());
    mac.update(recv_window.as_bytes());
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn f(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Fill normalisation
// ---------------------------------------------------------------------------

/// A Binance `newOrderRespType=FULL` response → [`Fill`].
pub fn normalize_binance_fill(resp: &Value, filters: &SymbolFilters) -> Result<Fill, String> {
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let executed = f(&resp["executedQty"]);
    let quote_amount = f(&resp["cummulativeQuoteQty"]);
    if executed <= 0.0 || quote_amount <= 0.0 {
        return Err(format!("Binance order {} executed nothing (status {status})", s(&resp["orderId"])));
    }
    let avg_price = quote_amount / executed;
    let mut base_fee = 0.0;
    let mut quote_fee = 0.0;
    let mut other_fee = 0.0;
    let mut other_asset = String::new();
    for fill in resp.get("fills").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        let amount = f(&fill["commission"]);
        let asset = fill.get("commissionAsset").and_then(|v| v.as_str()).unwrap_or("");
        if asset.eq_ignore_ascii_case(&filters.base) {
            base_fee += amount;
        } else if asset.eq_ignore_ascii_case(&filters.quote) {
            quote_fee += amount;
        } else {
            other_fee += amount;
            other_asset = asset.to_string();
        }
    }
    let (fee_asset, fee_amount, fee_quote, fee_estimated) = if base_fee > 0.0 {
        (filters.base.clone(), base_fee, base_fee * avg_price, false)
    } else if quote_fee > 0.0 {
        (filters.quote.clone(), quote_fee, quote_fee, false)
    } else if other_fee > 0.0 {
        (other_asset, other_fee, quote_amount * THIRD_ASSET_FEE_RATE, true)
    } else {
        (filters.quote.clone(), 0.0, 0.0, false)
    };
    Ok(Fill {
        order_id: s(&resp["orderId"]),
        avg_price,
        base_qty: executed,
        base_received: executed - base_fee,
        quote_amount,
        fee_quote,
        fee_asset,
        fee_amount,
        fee_estimated,
    })
}

/// A Bybit v5 order row (`/v5/order/realtime` or `/v5/order/history`) → [`Fill`].
/// Bybit spot charges buys in the base coin and sells in the quote coin
/// unless `feeCurrency` says otherwise.
pub fn normalize_bybit_order(row: &Value, filters: &SymbolFilters) -> Result<Fill, String> {
    let status = row.get("orderStatus").and_then(|v| v.as_str()).unwrap_or("");
    let executed = f(&row["cumExecQty"]);
    let quote_amount = f(&row["cumExecValue"]);
    if executed <= 0.0 || quote_amount <= 0.0 {
        return Err(format!("Bybit order {} executed nothing (status {status})", s(&row["orderId"])));
    }
    let avg_price = {
        let p = f(&row["avgPrice"]);
        if p > 0.0 { p } else { quote_amount / executed }
    };
    let fee_amount = f(&row["cumExecFee"]);
    let side_buy = row.get("side").and_then(|v| v.as_str()).unwrap_or("").eq_ignore_ascii_case("Buy");
    let fee_asset = match row.get("feeCurrency").and_then(|v| v.as_str()).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None if side_buy => filters.base.clone(),
        None => filters.quote.clone(),
    };
    let in_base = fee_asset.eq_ignore_ascii_case(&filters.base);
    let in_quote = fee_asset.eq_ignore_ascii_case(&filters.quote);
    let (fee_quote, fee_estimated) = if in_base {
        (fee_amount * avg_price, false)
    } else if in_quote {
        (fee_amount, false)
    } else {
        (quote_amount * THIRD_ASSET_FEE_RATE, true)
    };
    Ok(Fill {
        order_id: s(&row["orderId"]),
        avg_price,
        base_qty: executed,
        base_received: if in_base { executed - fee_amount } else { executed },
        quote_amount,
        fee_quote,
        fee_asset,
        fee_amount,
        fee_estimated,
    })
}

// ---------------------------------------------------------------------------
// The live broker
// ---------------------------------------------------------------------------

/// A live bot's [`Broker`] over any execution venue: Binance or Bybit spot
/// ([`CexVenue`]), Hyperliquid spot or perp (Venues 4/5).
pub struct LiveBroker {
    venue: Box<dyn ExecutionVenue>,
    label: String,
    sandbox: bool,
    market: MarketType,
    /// The venue's quote when it is not the bot pair's (a BTC/USDT signal
    /// trades BTC/USDC on Hyperliquid).
    venue_quote: Option<String>,
    /// The IOC cap against the best price (the core's default 0.5 %).
    pub max_slippage: f64,
    tag: String,
    sent: AtomicU64,
}

/// How much an order is for: quote to spend, or base to trade.
#[derive(Debug, Clone, Copy)]
enum Amount {
    Quote(f64),
    Base(f64),
}

impl LiveBroker {
    pub fn new(provider: &str, api_key: &str, api_secret: &str, sandbox: bool) -> Result<Self, String> {
        let venue = CexVenue::new(provider, api_key, api_secret, sandbox)?;
        let label = venue.venue().label().to_string();
        Ok(Self::on(Box::new(venue), label, sandbox, None))
    }

    /// A broker over an account's venue (`venues::open_leveraged`).
    pub fn on_venue(venue: Box<dyn ExecutionVenue>, sandbox: bool, venue_quote: &str) -> Self {
        let label = venue.label();
        Self::on(venue, label, sandbox, Some(venue_quote.to_string()))
    }

    fn on(venue: Box<dyn ExecutionVenue>, label: String, sandbox: bool, venue_quote: Option<String>) -> Self {
        let caps = venue.capabilities();
        Self {
            market: if caps.perp && !caps.spot { MarketType::Perp } else { MarketType::Spot },
            venue,
            label,
            sandbox,
            venue_quote,
            max_slippage: crate::execution::guards::GuardConfig::default().max_slippage,
            tag: uuid::Uuid::new_v4().simple().to_string()[..10].to_string(),
            sent: AtomicU64::new(0),
        }
    }

    /// The bot's pair on the venue: BTC/USDT → BTC/USDC where USDC is the quote.
    fn venue_pair(&self, pair: &str) -> String {
        match (&self.venue_quote, pair.split_once('/')) {
            (Some(q), Some((base, _))) => format!("{base}/{q}"),
            _ => pair.to_string(),
        }
    }

    /// A stablecoin asked for is the venue's quote (budgets are in USD).
    fn venue_asset(&self, asset: &str) -> String {
        match &self.venue_quote {
            Some(q) if matches!(asset.to_uppercase().as_str(), "USDT" | "USDC" | "USD") => q.clone(),
            _ => asset.to_string(),
        }
    }

    /// Unique per order: `qsb` + this broker's tag + a counter.
    fn next_id(&self) -> String {
        format!("qsb{}-{:04}", self.tag, self.sent.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// One IOC order. An error from the venue is looked up by client id
    /// before it counts — the order may have gone through.
    fn send(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        match self.venue.place(order) {
            Ok(report) => Ok(report),
            Err(e) => match self.venue.query_by_client_id(&order.instrument, &order.client_order_id) {
                Ok(Some(report)) => Ok(report),
                Ok(None) => Err(e),
                Err(e2) => Err(format!(
                    "{e}; the outcome of order {} is unknown (the lookup failed: {e2}) — check the exchange",
                    order.client_order_id
                )),
            },
        }
    }

    fn priced(&self, inst: &Instrument, side: Side) -> Result<(crate::execution::venue::OrderBook, f64, f64), String> {
        let book = self.venue.order_book(inst)?;
        let best = match side {
            Side::Buy => book.best_ask(),
            Side::Sell => book.best_bid(),
        }
        .ok_or_else(|| format!("{}: the order book is empty", inst.pair()))?;
        let cap = match side {
            Side::Buy => best * (1.0 + self.max_slippage),
            Side::Sell => best * (1.0 - self.max_slippage),
        };
        Ok((book, best, inst.round_price(cap, side)))
    }

    /// Fill `amount` — quote to spend on a buy, or base — in up to four IOC
    /// attempts at the cap; one [`Fill`] for all of them. `reduce_only` on
    /// perp closes.
    fn fill(&self, filters: &SymbolFilters, side: Side, amount: Amount, reduce_only: bool) -> Result<Fill, String> {
        let inst = Instrument { market: self.market, filters: filters.clone() };
        let mut reports: Vec<OrderReport> = Vec::new();
        let mut left = match amount {
            Amount::Quote(q) | Amount::Base(q) => q,
        };
        for _ in 0..=crate::execution::engine::MAX_RETRIES {
            let (book, best, limit) = self.priced(&inst, side)?;
            let qty = match amount {
                // 0.1 % room for a move between reading the book and the order.
                Amount::Quote(_) => filters.round_qty_down(book.walk_buy_quote(left / 1.001, Some(limit)).base),
                Amount::Base(_) => filters.round_qty_down(left),
            };
            if !inst.tradable(qty, best) {
                break;
            }
            let order = OrderRequest {
                client_order_id: self.next_id(),
                instrument: inst.clone(),
                side,
                qty,
                limit_price: limit,
                reduce_only,
            };
            let report = match self.send(&order) {
                Ok(report) => report,
                Err(e) if reports.is_empty() => return Err(e),
                Err(_) => break,
            };
            if report.status == OrderStatus::Rejected {
                if reports.is_empty() {
                    return Err(report.message.unwrap_or_else(|| "rejected".into()));
                }
                break;
            }
            left -= match amount {
                Amount::Quote(_) => report.quote_amount + report.fee_in(&filters.quote),
                Amount::Base(_) => report.filled_qty,
            };
            let done = report.status == OrderStatus::Filled;
            if report.filled_qty > 0.0 {
                reports.push(report);
            }
            if done {
                break;
            }
        }
        merge_reports(&reports).ok_or_else(|| {
            format!("nothing filled within the {:.2} % slippage cap", self.max_slippage * 100.0)
        })
    }
}

/// Several IOC fills of one bot order as one [`Fill`].
pub fn merge_reports(reports: &[OrderReport]) -> Option<Fill> {
    let first = reports.first()?;
    let base_qty: f64 = reports.iter().map(|r| r.filled_qty).sum();
    let quote_amount: f64 = reports.iter().map(|r| r.quote_amount).sum();
    if base_qty <= 0.0 {
        return None;
    }
    Some(Fill {
        order_id: reports.iter().map(|r| r.venue_order_id.as_str()).collect::<Vec<_>>().join(","),
        avg_price: quote_amount / base_qty,
        base_qty,
        base_received: reports.iter().map(|r| r.base_received).sum(),
        quote_amount,
        fee_quote: reports.iter().map(|r| r.fee_quote).sum(),
        fee_asset: first.fee_asset.clone(),
        fee_amount: reports.iter().filter(|r| r.fee_asset.eq_ignore_ascii_case(&first.fee_asset)).map(|r| r.fee_amount).sum(),
        fee_estimated: reports.iter().any(|r| r.fee_estimated),
    })
}

impl Broker for LiveBroker {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn sandbox(&self) -> bool {
        self.sandbox
    }

    fn perp(&self) -> bool {
        self.market == MarketType::Perp
    }

    fn symbol_filters(&self, pair: &str) -> Result<SymbolFilters, String> {
        self.venue.instrument(&self.venue_pair(pair)).map(|i| i.filters)
    }

    fn available_balance(&self, asset: &str) -> Result<f64, String> {
        self.venue.ready()?;
        Ok(free_balance(&self.venue.balances()?, &self.venue_asset(asset)))
    }

    fn open_short(&self, filters: &SymbolFilters, base_qty: f64) -> Result<Fill, String> {
        if !self.perp() {
            return Err(format!("{} is spot: long-only", self.label));
        }
        let qty = filters.round_qty_down(base_qty);
        if qty < filters.min_qty || qty <= 0.0 {
            return Err(format!("{} {} is below the minimum quantity of {}", filters.qty_string(qty), filters.base, filters.qty_string(filters.min_qty)));
        }
        self.venue.ready()?;
        self.fill(filters, Side::Sell, Amount::Base(qty), false)
    }

    fn close_short(&self, filters: &SymbolFilters, base_qty: f64) -> Result<Fill, String> {
        if !self.perp() {
            return Err(format!("{} is spot: there is no short to close", self.label));
        }
        self.venue.ready()?;
        self.fill(filters, Side::Buy, Amount::Base(filters.round_qty_down(base_qty)), true)
    }

    fn market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<Fill, String> {
        if quote_amount < filters.min_notional {
            return Err(format!(
                "{} is below the minimum order of {} {}",
                filters.quote_string(quote_amount), filters.quote_string(filters.min_notional), filters.quote
            ));
        }
        self.venue.ready()?;
        self.fill(filters, Side::Buy, Amount::Quote(quote_amount), false)
    }

    fn market_sell(&self, filters: &SymbolFilters, base_qty: f64) -> Result<Fill, String> {
        let qty = filters.round_qty_down(base_qty);
        if qty < filters.min_qty || qty <= 0.0 {
            return Err(format!(
                "{} {} is below the minimum sell quantity of {}",
                filters.qty_string(qty), filters.base, filters.qty_string(filters.min_qty)
            ));
        }
        self.venue.ready()?;
        // On perp a sell of a long only ever closes it.
        self.fill(filters, Side::Sell, Amount::Base(qty), self.perp())
    }

    fn test_market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<String, String> {
        self.venue.ready()?;
        if !self.venue.capabilities().test_orders {
            return Ok(format!(
                "{} has no test-order endpoint; lot size, minimum and the slippage cap were checked here, the venue checks the rest when the order goes out",
                self.label
            ));
        }
        let inst = Instrument { market: self.market, filters: filters.clone() };
        let (book, _, limit) = self.priced(&inst, Side::Buy)?;
        let qty = filters.round_qty_down(book.walk_buy_quote(quote_amount, Some(limit)).base);
        let order = OrderRequest { client_order_id: self.next_id(), instrument: inst, side: Side::Buy, qty, limit_price: limit, reduce_only: false };
        self.venue.test_order(&order)
    }
}

impl std::fmt::Debug for LiveBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveBroker").field("venue", &self.label).field("max_slippage", &self.max_slippage).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn btc() -> SymbolFilters {
        SymbolFilters {
            symbol: "BTCUSDT".into(),
            base: "BTC".into(),
            quote: "USDT".into(),
            qty_step: 0.00001,
            min_qty: 0.00001,
            min_notional: 5.0,
            price_tick: 0.01,
        }
    }

    // Known answers computed independently with Python's hmac module.
    #[test]
    fn binance_signs_the_query_string_with_hmac_sha256_hex() {
        let secret = "NhqPtmdSJYdKjVHjA7PZj4Yuo0Ry7rVzHqQ1G2dtCBu2gI9XKHc2H8vfCfq58TW1";
        let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
        assert_eq!(
            binance_signature(secret, query),
            "234184881ab793ed973328b50d0f126b299e100caf222bad1128cca20438a919"
        );
    }

    #[test]
    fn bybit_signs_timestamp_key_window_and_payload_in_that_order() {
        let payload = r#"{"category":"spot","symbol":"BTCUSDT","side":"Buy","orderType":"Market","qty":"25","marketUnit":"quoteCoin"}"#;
        assert_eq!(
            bybit_signature("bybit-secret-for-tests", "1700000000000", "bybit-key", "10000", payload),
            "360acc07257945e7c30f678d4142442d7e4c010353e940946940d4e20dab4966"
        );
        // Order matters: swapping key and timestamp is a different signature.
        assert_ne!(
            bybit_signature("bybit-secret-for-tests", "bybit-key", "1700000000000", "10000", payload),
            "360acc07257945e7c30f678d4142442d7e4c010353e940946940d4e20dab4966"
        );
    }

    #[test]
    fn quantities_round_down_to_the_lot_step_and_print_without_noise() {
        let f = btc();
        assert_eq!(f.round_qty_down(0.0015609375), 0.00156);
        assert_eq!(f.qty_string(0.00156), "0.00156");
        assert_eq!(f.qty_string(1.0), "1");
        let whole = SymbolFilters { qty_step: 1.0, ..btc() };
        assert_eq!(whole.round_qty_down(12.9), 12.0);
        assert_eq!(whole.qty_string(12.0), "12");
        assert_eq!(decimals_of(0.001), 3);
        assert_eq!(decimals_of(1.0), 0);
        assert_eq!(f.quote_string(25.0), "25");
        assert_eq!(f.quote_string(25.5), "25.5");
        assert_eq!(f.quote_string(99.999), "100");
    }

    #[test]
    fn a_binance_fill_with_a_base_fee_nets_the_quantity_and_values_the_fee_in_quote() {
        let resp = json!({
            "symbol": "BTCUSDT", "orderId": 28, "status": "FILLED",
            "executedQty": "0.00156000", "cummulativeQuoteQty": "99.84000000",
            "fills": [
                { "price": "64000.00", "qty": "0.00100000", "commission": "0.00000100", "commissionAsset": "BTC" },
                { "price": "64000.00", "qty": "0.00056000", "commission": "0.00000056", "commissionAsset": "BTC" }
            ]
        });
        let fill = normalize_binance_fill(&resp, &btc()).unwrap();
        assert_eq!(fill.order_id, "28");
        assert!((fill.avg_price - 64_000.0).abs() < 1e-9);
        assert!((fill.base_qty - 0.00156).abs() < 1e-12);
        assert!((fill.base_received - (0.00156 - 0.00000156)).abs() < 1e-12);
        assert!((fill.quote_amount - 99.84).abs() < 1e-9);
        assert_eq!(fill.fee_asset, "BTC");
        assert!((fill.fee_quote - 0.00000156 * 64_000.0).abs() < 1e-9);
        assert!(!fill.fee_estimated);
        assert_eq!(fill.fee_from_cash("USDT"), 0.0, "a base fee never leaves the quote cash");
    }

    #[test]
    fn a_binance_sell_with_a_quote_fee_and_a_bnb_fee_are_told_apart() {
        let sell = json!({
            "orderId": "29", "status": "FILLED", "executedQty": "0.00156", "cummulativeQuoteQty": "101.40",
            "fills": [{ "price": "65000", "qty": "0.00156", "commission": "0.1014", "commissionAsset": "USDT" }]
        });
        let fill = normalize_binance_fill(&sell, &btc()).unwrap();
        assert_eq!(fill.fee_asset, "USDT");
        assert!((fill.fee_from_cash("USDT") - 0.1014).abs() < 1e-12);
        assert!((fill.base_received - 0.00156).abs() < 1e-12);

        let bnb = json!({
            "orderId": "30", "status": "FILLED", "executedQty": "0.001", "cummulativeQuoteQty": "64",
            "fills": [{ "price": "64000", "qty": "0.001", "commission": "0.00005", "commissionAsset": "BNB" }]
        });
        let fill = normalize_binance_fill(&bnb, &btc()).unwrap();
        assert_eq!(fill.fee_asset, "BNB");
        assert!(fill.fee_estimated);
        assert!((fill.fee_quote - 64.0 * THIRD_ASSET_FEE_RATE).abs() < 1e-12);
        assert_eq!(fill.fee_from_cash("USDT"), 0.0);

        let nothing = json!({ "orderId": "31", "status": "EXPIRED", "executedQty": "0", "cummulativeQuoteQty": "0", "fills": [] });
        assert!(normalize_binance_fill(&nothing, &btc()).unwrap_err().contains("executed nothing"));
    }

    #[test]
    fn a_bybit_order_row_defaults_the_fee_coin_by_side() {
        let buy = json!({
            "orderId": "1321003749386647552", "side": "Buy", "orderStatus": "Filled",
            "avgPrice": "64000.5", "cumExecQty": "0.001", "cumExecValue": "64.0005", "cumExecFee": "0.000001"
        });
        let fill = normalize_bybit_order(&buy, &btc()).unwrap();
        assert_eq!(fill.fee_asset, "BTC", "spot buys are charged in base");
        assert!((fill.base_received - 0.000999).abs() < 1e-12);
        assert!((fill.fee_quote - 0.000001 * 64_000.5).abs() < 1e-9);
        assert_eq!(fill.avg_price, 64_000.5);

        let sell = json!({
            "orderId": "1321003749386647553", "side": "Sell", "orderStatus": "Filled",
            "avgPrice": "", "cumExecQty": "0.001", "cumExecValue": "64.2", "cumExecFee": "0.0642", "feeCurrency": "USDT"
        });
        let fill = normalize_bybit_order(&sell, &btc()).unwrap();
        assert_eq!(fill.fee_asset, "USDT");
        assert!((fill.avg_price - 64_200.0).abs() < 1e-9, "average from value / qty when avgPrice is blank");
        assert!((fill.fee_from_cash("USDT") - 0.0642).abs() < 1e-12);
        assert!((fill.base_received - 0.001).abs() < 1e-12);
    }

    #[test]
    fn venues_and_their_environments() {
        assert_eq!(Venue::parse("Binance"), Some(Venue::Binance));
        assert_eq!(Venue::parse("kraken"), None);
        assert!(Venue::supports_live("bybit"));
        assert!(!Venue::supports_live("okx"));
        assert_eq!(Venue::Binance.private_base(true), "https://testnet.binance.vision");
        assert_eq!(Venue::Bybit.private_base(true), "https://api-demo.bybit.com");
        assert_eq!(Venue::Bybit.private_base(false), Venue::Bybit.public_base());
        assert_eq!(Venue::Binance.symbol("BTC/USDT"), "BTCUSDT");
        assert!(LiveBroker::new("kraken", "k", "s", false).is_err());
    }

    #[test]
    fn a_broker_without_keys_refuses_private_calls_before_sending_anything() {
        let broker = LiveBroker::new("binance", "", "", true).unwrap();
        assert!(broker.available_balance("USDT").unwrap_err().contains("no API key"));
        assert!(broker.market_buy(&btc(), 1.0).unwrap_err().contains("below the minimum order"));
        assert!(broker.market_sell(&btc(), 0.000001).unwrap_err().contains("below the minimum sell"));
        assert!(broker.market_buy(&btc(), 10.0).unwrap_err().contains("no API key"));
    }

    // Live public halves — `cargo test -p tauri-plugin-algo -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_binance_filters_for_btc_usdt() {
        let broker = LiveBroker::new("binance", "", "", false).unwrap();
        let f = broker.symbol_filters("BTC/USDT").unwrap();
        assert_eq!(f.symbol, "BTCUSDT");
        assert_eq!((f.base.as_str(), f.quote.as_str()), ("BTC", "USDT"));
        assert!(f.qty_step > 0.0 && f.min_qty > 0.0 && f.min_notional > 0.0, "{f:?}");
        println!("binance {f:?}");
    }

    #[test]
    #[ignore]
    fn live_bybit_filters_for_btc_usdt() {
        let broker = LiveBroker::new("bybit", "", "", false).unwrap();
        let f = broker.symbol_filters("BTC/USDT").unwrap();
        assert_eq!(f.symbol, "BTCUSDT");
        assert!(f.qty_step > 0.0 && f.min_qty > 0.0 && f.min_notional > 0.0, "{f:?}");
        println!("bybit {f:?}");
    }

    #[test]
    #[ignore]
    fn live_sandbox_hosts_answer() {
        // The sandboxes must be reachable for a sandbox exchange to work at all;
        // an unsigned private call is expected to be rejected, not to time out.
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build().unwrap();
        let binance = client.get(format!("{}/api/v3/time", Venue::Binance.private_base(true))).send().unwrap();
        assert!(binance.status().is_success(), "binance testnet time: {}", binance.status());
        let bybit = client.get(format!("{}/v5/market/time", Venue::Bybit.private_base(true))).send().unwrap();
        assert!(bybit.status().is_success(), "bybit demo time: {}", bybit.status());
    }
}
