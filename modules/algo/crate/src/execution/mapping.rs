//! Data source vs execution venue (PLAN-EXECUTION-VENUES §2, §4, §5, card
//! Venues 1/5): which venue instrument a data coin trades as, verified by id
//! and never by name; the direct pairs between mapped coins (the switch route
//! choice); the coverage of a universe; and the cost preview of a switch and
//! of holding.
//!
//! On a CEX the asset id is the exchange's own asset code: the exchange lists
//! one asset per code. On-chain venues (Hyperliquid token index, Solana mint
//! address) are only mapped through their verified list — anyone can create
//! a token called "BTC".

use serde::Serialize;

use super::route::{self, DirectMarket, RouteChoice, SwitchInputs};
use super::venue::{Instrument, MarketType, OrderBook};

/// One coin of the data source on the venue.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Mapping {
    /// The data source's symbol (`BTC`).
    pub symbol: String,
    /// The venue's instrument against its quote (`BTCUSDT`, `UBTC/USDC`).
    pub instrument: String,
    /// The venue's id of the asset (asset code, token index, mint address),
    /// shown next to the name.
    pub asset_id: String,
    pub quote: String,
    /// Who holds the real asset behind a tokenised one (Coinbase, BitGo,
    /// Unit), when it is one.
    pub custodian: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MappingTable {
    pub venue: String,
    pub quote: String,
    pub mapped: Vec<Mapping>,
    /// Data coins the venue does not list against its quote.
    pub missing: Vec<String>,
    /// Instruments named like a data coin whose id is not the verified one.
    pub rejected: Vec<(String, String)>,
    /// Direct pairs between mapped coins: `(base, quote, instrument)`.
    pub direct_pairs: Vec<(String, String, String)>,
    /// "17 / 20 coins tradable on Binance spot".
    pub summary: String,
}

/// The ids a venue's assets must carry to count as a data coin. A CEX
/// accepts its own asset code; an on-chain venue its verified list.
pub trait Verified {
    /// Whether `asset_id` on the venue is the real `symbol`, and its custodian.
    fn check(&self, symbol: &str, asset_id: &str) -> Option<Option<String>>;
}

/// A CEX: the exchange's asset code is the asset (`BTC` is BTC).
pub struct ExchangeCodes;

impl Verified for ExchangeCodes {
    fn check(&self, symbol: &str, asset_id: &str) -> Option<Option<String>> {
        symbol.eq_ignore_ascii_case(asset_id).then_some(None)
    }
}

/// An on-chain venue's verified list: data symbol → (asset id, custodian).
/// Filled by the on-chain venues (Venues 2/5 Hyperliquid, 3/5 Jupiter).
#[allow(dead_code)]
pub struct VerifiedList(pub Vec<(&'static str, &'static str, Option<&'static str>)>);

impl Verified for VerifiedList {
    fn check(&self, symbol: &str, asset_id: &str) -> Option<Option<String>> {
        self.0
            .iter()
            .find(|(s, id, _)| s.eq_ignore_ascii_case(symbol) && id.eq_ignore_ascii_case(asset_id))
            .map(|(_, _, c)| c.map(String::from))
    }
}

/// The venue's quote for "USD".
pub fn quote_for(venue_label: &str) -> &'static str {
    let v = venue_label.to_lowercase();
    if v.contains("hyperliquid") || v.contains("jupiter") || v.contains("lighter") {
        "USDC"
    } else {
        "USDT"
    }
}

/// Map `symbols` onto the venue's spot `instruments`. The instrument's base
/// is its asset id here; a display name that matches but whose id is not the
/// verified one is rejected. `display` gives an instrument's display name
/// where it differs from its id (on-chain venues).
pub fn map_coins(
    venue: &str,
    symbols: &[String],
    instruments: &[Instrument],
    verified: &dyn Verified,
    display: &dyn Fn(&Instrument) -> String,
) -> MappingTable {
    let quote = quote_for(venue);
    let mut table = MappingTable { venue: venue.into(), quote: quote.into(), ..Default::default() };
    let spot: Vec<&Instrument> = instruments.iter().filter(|i| i.market == MarketType::Spot).collect();
    for symbol in symbols {
        let mut found = None;
        for inst in spot.iter().filter(|i| i.quote().eq_ignore_ascii_case(quote)) {
            let name = display(inst);
            let named = name.eq_ignore_ascii_case(symbol) || inst.base().eq_ignore_ascii_case(symbol);
            match verified.check(symbol, inst.base()) {
                // The verified id, whatever the instrument is called (UBTC is BTC).
                Some(custodian) => {
                    found.get_or_insert(Mapping {
                        symbol: symbol.to_uppercase(),
                        instrument: inst.filters.symbol.clone(),
                        asset_id: inst.base().to_string(),
                        quote: quote.into(),
                        custodian,
                    });
                }
                // Named like the coin, but not its id: a look-alike.
                None if named => table.rejected.push((symbol.to_uppercase(), inst.base().to_string())),
                None => {}
            }
        }
        match found {
            Some(m) => table.mapped.push(m),
            None => table.missing.push(symbol.to_uppercase()),
        }
    }
    // Direct pairs only between two mapped coins (their verified ids).
    let ids: Vec<&str> = table.mapped.iter().map(|m| m.asset_id.as_str()).collect();
    for inst in &spot {
        let (b, q) = (inst.base(), inst.quote());
        if !q.eq_ignore_ascii_case(quote) && ids.iter().any(|i| i.eq_ignore_ascii_case(b)) && ids.iter().any(|i| i.eq_ignore_ascii_case(q)) {
            table.direct_pairs.push((b.to_string(), q.to_string(), inst.filters.symbol.clone()));
        }
    }
    table.summary = coverage_line(&table);
    table
}

/// "17 / 20 coins tradable".
pub fn coverage_line(table: &MappingTable) -> String {
    let total = table.mapped.len() + table.missing.len();
    format!("{} / {total} coins tradable on {}", table.mapped.len(), table.venue)
}

/// What one switch and holding cost (§5.1), in USD.
#[derive(Debug, Clone, Serialize)]
pub struct CostPreview {
    pub value: f64,
    pub market: MarketType,
    /// The switch: both routes and the one the engine would take.
    pub switch: RouteChoice,
    /// Holding for `days`: 0 on spot; perp pays funding.
    pub holding: f64,
    pub days: u32,
    /// Perp: funding per 8 hours used (the live rate, or the base rate).
    pub funding_per_8h: Option<f64>,
    pub total: f64,
}

/// Funding a long pays for holding `value` for `days` at `rate_per_8h`.
pub fn funding_cost(value: f64, rate_per_8h: f64, days: u32) -> f64 {
    value * rate_per_8h * 3.0 * days as f64
}

/// The cost of switching `qty_from` of `from` into `to`, then holding.
#[allow(clippy::too_many_arguments)]
pub fn preview(
    from: &str,
    to: &str,
    qty_from: f64,
    from_book: &OrderBook,
    to_book: &OrderBook,
    taker: f64,
    direct: Option<(Instrument, &OrderBook, f64)>,
    market: MarketType,
    funding_per_8h: Option<f64>,
    days: u32,
) -> CostPreview {
    let switch = route::choose(&SwitchInputs {
        from,
        to,
        qty_from,
        from_book,
        to_book,
        from_fee: taker,
        to_fee: taker,
        direct: direct.as_ref().map(|(inst, book, fee)| DirectMarket { instrument: inst.clone(), book, taker_fee: *fee }),
    });
    let value = switch.via_quote.value;
    let switch_cost = match switch.chosen {
        route::RouteKind::Direct => switch.direct.as_ref().map(|d| d.total).unwrap_or(switch.via_quote.total),
        route::RouteKind::ViaQuote => switch.via_quote.total,
    };
    let holding = match (market, funding_per_8h) {
        (MarketType::Perp, Some(rate)) => funding_cost(value, rate, days),
        _ => 0.0,
    };
    CostPreview { value, market, switch, holding, days, funding_per_8h, total: switch_cost + holding }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::route::tests::{deep, inst};

    fn named(i: &Instrument) -> String {
        i.base().to_string()
    }

    #[test]
    fn coins_map_by_id_and_a_same_named_fake_is_rejected() {
        let instruments = vec![
            inst("BTC", "USDT", 0.01, 0.00001),
            inst("ETH", "USDT", 0.01, 0.0001),
            inst("ETH", "BTC", 0.00001, 0.0001),
            inst("SOL", "BTC", 0.0000001, 0.01),
        ];
        let table = map_coins("Binance spot", &["btc".into(), "ETH".into(), "SOL".into()], &instruments, &ExchangeCodes, &named);
        assert_eq!(table.quote, "USDT");
        assert_eq!(table.mapped.iter().map(|m| m.instrument.as_str()).collect::<Vec<_>>(), vec!["BTCUSDT", "ETHUSDT"]);
        assert_eq!(table.missing, vec!["SOL"], "SOL is listed only against BTC, not the quote");
        assert_eq!(table.direct_pairs, vec![("ETH".into(), "BTC".into(), "ETHBTC".into())], "SOL/BTC is no direct pair: SOL is not mapped");
        assert_eq!(coverage_line(&table), "2 / 3 coins tradable on Binance spot");

        // On-chain: a token *named* BTC with another id is not BTC.
        let chain = vec![inst("FAKEBTC1", "USDC", 0.01, 0.00001), inst("UBTC", "USDC", 0.01, 0.00001)];
        let display = |i: &Instrument| if i.base() == "FAKEBTC1" { "BTC".to_string() } else { i.base().to_string() };
        let verified = VerifiedList(vec![("BTC", "UBTC", Some("Unit (2-of-3 MPC)"))]);
        let table = map_coins("Hyperliquid spot", &["BTC".into()], &chain, &verified, &display);
        assert_eq!(table.quote, "USDC");
        assert_eq!(table.rejected, vec![("BTC".into(), "FAKEBTC1".into())]);
        assert_eq!(table.mapped[0].asset_id, "UBTC");
        assert_eq!(table.mapped[0].custodian.as_deref(), Some("Unit (2-of-3 MPC)"));
    }

    #[test]
    fn the_cost_preview_matches_the_worked_example() {
        // §5.1: 10,000$ BTC → ETH, then 10 days.
        let btc = deep(100_000.0, 0.000_001, 20, 1.0);
        let eth = deep(4_000.0, 0.000_001, 20, 25.0);
        let ethbtc = deep(0.04, 0.000_01, 20, 50.0);
        // Binance spot through USDT: 20$; with ETH/BTC: about 10$ plus the spread.
        let spot = preview("BTC", "ETH", 0.1, &btc, &eth, 0.001, None, MarketType::Spot, None, 10);
        assert!((spot.total - 20.0).abs() < 0.2, "{}", spot.total);
        assert_eq!(spot.holding, 0.0);
        let direct = preview(
            "BTC", "ETH", 0.1, &btc, &eth, 0.001,
            Some((inst("ETH", "BTC", 0.00001, 0.0001), &ethbtc, 0.001)),
            MarketType::Spot, None, 10,
        );
        assert_eq!(direct.switch.chosen, route::RouteKind::Direct);
        assert!((direct.total - 10.0).abs() < 1.0, "{}", direct.total);
        // Hyperliquid spot 0.070 % × 2 = 14$.
        let hl = preview("BTC", "ETH", 0.1, &btc, &eth, 0.0007, None, MarketType::Spot, None, 10);
        assert!((hl.total - 14.0).abs() < 0.2, "{}", hl.total);
        // Perp 1x: 0.045 % × 2 = 9$ plus funding at the base rate ≈ 30$.
        let perp = preview("BTC", "ETH", 0.1, &btc, &eth, 0.00045, None, MarketType::Perp, Some(0.0001), 10);
        assert!((perp.holding - 30.0).abs() < 0.2, "{}", perp.holding);
        assert!((perp.total - 39.0).abs() < 0.5, "{}", perp.total);
    }
}

/// Live values for both CEX venues: a Top-10-like universe mapped by id, and
/// the §5.1 switch on the real books at the standard taker rate.
/// `cargo test -p tauri-plugin-algo --lib -- --ignored live_venue_values --nocapture`
#[cfg(test)]
mod live {
    use super::*;
    use crate::execution::cex::{CexVenue, DEFAULT_TAKER_FEE};
    use crate::execution::venue::ExecutionVenue;

    #[test]
    #[ignore = "reads Binance's and Bybit's public data"]
    fn live_venue_values() {
        let universe: Vec<String> = ["BTC", "ETH", "XRP", "BNB", "SOL", "DOGE", "TRX", "ADA", "HYPE", "LINK"].iter().map(|s| s.to_string()).collect();
        for provider in ["binance", "bybit"] {
            let v = CexVenue::new(provider, "", "", false).unwrap();
            let instruments = v.instruments().unwrap();
            let table = map_coins(&v.label(), &universe, &instruments, &ExchangeCodes, &|i: &Instrument| i.base().to_string());
            println!("{}: {} — missing {:?}; direct pairs {}", provider, table.summary, table.missing, table.direct_pairs.len());
            let btc = v.instrument("BTC/USDT").unwrap();
            let eth = v.instrument("ETH/USDT").unwrap();
            let (bb, eb) = (v.order_book(&btc).unwrap(), v.order_book(&eth).unwrap());
            let qty = 10_000.0 / bb.mid().unwrap();
            let direct = instruments.iter().find(|i| i.base() == "ETH" && i.quote() == "BTC").cloned();
            let db = direct.as_ref().and_then(|i| v.order_book(i).ok());
            let p = preview("BTC", "ETH", qty, &bb, &eb, DEFAULT_TAKER_FEE,
                match (&direct, &db) { (Some(i), Some(b)) => Some((i.clone(), b, DEFAULT_TAKER_FEE)), _ => None },
                MarketType::Spot, None, 30);
            println!("  10,000$ BTC→ETH: via USDT {:.2}$, direct {:?}, chosen {:?} — {}", p.switch.via_quote.total, p.switch.direct.as_ref().map(|d| (d.total * 100.0).round() / 100.0), p.switch.chosen, p.switch.reason);
        }
    }
}
