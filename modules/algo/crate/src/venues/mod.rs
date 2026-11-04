//! Where the orders go (PLAN-EXECUTION-VENUES §9): one place that turns an
//! account into an [`ExecutionVenue`], whatever the venue. Binance and Bybit
//! spot are [`CexVenue`]; Hyperliquid spot and perp are
//! [`hyperliquid::HyperliquidVenue`] (card Venues 2/5).

pub mod hyperliquid;

use chrono::NaiveDate;

use crate::exchanges::StoredCredentials;
use crate::execution::cex::CexVenue;
use crate::execution::mapping::{self, ExchangeCodes, MappingTable};
use crate::execution::venue::{ExecutionVenue, Instrument, MarketType};
use crate::{Balance, ConnectionResult};

use hyperliquid::HyperliquidVenue;

fn is_hyperliquid(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("hyperliquid")
}

/// The execution core can trade on this provider's accounts.
pub fn executes(provider: &str) -> bool {
    crate::broker::Venue::supports_live(provider) || is_hyperliquid(provider)
}

/// The market types an account of this provider trades.
pub fn markets(provider: &str) -> Vec<MarketType> {
    if is_hyperliquid(provider) {
        vec![MarketType::Spot, MarketType::Perp]
    } else if executes(provider) {
        vec![MarketType::Spot]
    } else {
        Vec::new()
    }
}

/// The provider has a test network the account can point at.
pub fn has_testnet(provider: &str) -> bool {
    crate::broker::Venue::supports_live(provider) || is_hyperliquid(provider)
}

fn check_market(provider: &str, market: MarketType) -> Result<(), String> {
    if markets(provider).contains(&market) {
        Ok(())
    } else if executes(provider) {
        Err(format!("{provider} accounts trade spot only; perp is on Hyperliquid."))
    } else {
        Err(format!("Live order routing is implemented for Binance, Bybit and Hyperliquid, not {provider}."))
    }
}

/// An account's venue for `market`, with its keys.
pub fn open(creds: &StoredCredentials, market: MarketType) -> Result<Box<dyn ExecutionVenue>, String> {
    check_market(&creds.provider, market)?;
    if is_hyperliquid(&creds.provider) {
        return Ok(Box::new(HyperliquidVenue::new(
            market,
            creds.sandbox,
            &creds.wallet_address,
            &creds.sub_account,
            &creds.private_key,
        )?));
    }
    Ok(Box::new(CexVenue::new(&creds.provider, &creds.api_key, &creds.api_secret, creds.sandbox)?))
}

/// The public side of a provider's `market`: instruments, books, candles.
/// A CEX's sandbox keeps production market data (its sandboxes carry thin
/// markets); Hyperliquid's testnet has its own markets and token ids, so a
/// testnet account reads the testnet.
pub fn public(provider: &str, testnet: bool, market: MarketType) -> Result<Box<dyn ExecutionVenue>, String> {
    check_market(provider, market)?;
    if is_hyperliquid(provider) {
        return Ok(Box::new(HyperliquidVenue::public(market, testnet)?));
    }
    Ok(Box::new(CexVenue::new(provider, "", "", false)?))
}

/// The coverage of `symbols` on a provider's market, by verified id.
pub fn mapping_table(provider: &str, testnet: bool, market: MarketType, symbols: &[String]) -> Result<MappingTable, String> {
    check_market(provider, market)?;
    if is_hyperliquid(provider) {
        return HyperliquidVenue::public(market, testnet)?.mapping(symbols);
    }
    let venue = CexVenue::new(provider, "", "", false)?;
    let instruments = venue.instruments()?;
    Ok(mapping::map_coins(&venue.label(), symbols, &instruments, &ExchangeCodes, &|i: &Instrument| i.base().to_string()))
}

/// The closed daily candle of `day` (UTC) of `coin` against `quote` on the
/// provider's own market, when it has one there.
pub fn daily_close(provider: &str, testnet: bool, market: MarketType, coin: &str, quote: &str, day: NaiveDate) -> Option<f64> {
    if is_hyperliquid(provider) {
        let venue = HyperliquidVenue::public(market, testnet).ok()?;
        let inst = venue.instrument(&format!("{coin}/{quote}")).ok()?;
        return venue.daily_close_of(&inst, day);
    }
    let prefix = day.to_string();
    let candles = crate::exchanges::fetch_recent_market_candles(provider, &format!("{coin}/{quote}"), "1d", 3).ok()?;
    candles.iter().find(|c| c.time.starts_with(&prefix)).map(|c| c.close).filter(|c| *c > 0.0)
}

/// The hourly funding rate of a perp, when the venue has one.
pub fn funding_rate_hourly(provider: &str, testnet: bool, coin: &str) -> Option<f64> {
    if is_hyperliquid(provider) {
        return HyperliquidVenue::public(MarketType::Perp, testnet).ok()?.funding_rate_hourly(coin).ok();
    }
    None
}

fn hyperliquid_account(creds_main: &str, sub: &str, key: &str, testnet: bool, market: MarketType) -> Result<HyperliquidVenue, String> {
    HyperliquidVenue::new(market, testnet, creds_main, sub, key)
}

/// The Exchange page's connection test of a Hyperliquid account: the API
/// wallet must be approved by the main address, and both balances readable.
pub fn test_hyperliquid(main: &str, sub: &str, key: &str, testnet: bool) -> Result<ConnectionResult, String> {
    let fail = |message: String| Ok(ConnectionResult { success: false, message, latency_ms: None });
    if main.trim().is_empty() || key.trim().is_empty() {
        return fail("Enter your main wallet address and the API wallet's private key.".into());
    }
    let start = std::time::Instant::now();
    let spot = match hyperliquid_account(main, sub, key, testnet, MarketType::Spot) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let agent = match spot.agent_status() {
        Ok(a) => a,
        Err(e) => return fail(e),
    };
    if !agent.approved {
        return fail(format!(
            "The API wallet {} is not approved by {}{}. Approve it in Hyperliquid's app (More → API), then test again.",
            agent.address,
            main.trim(),
            if testnet { " on testnet" } else { "" }
        ));
    }
    let perp = hyperliquid_account(main, sub, key, testnet, MarketType::Perp)?;
    let usdc = |v: &HyperliquidVenue| v.balances().map(|b| crate::execution::venue::free_balance(&b, "USDC"));
    let (spot_usdc, perp_usdc) = match (usdc(&spot), usdc(&perp)) {
        (Ok(s), Ok(p)) => (s, p),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let latency = start.elapsed().as_millis() as u64;
    let expiry = match (agent.days_left, agent.expiring) {
        (Some(d), true) => format!(" — the API wallet expires in {d} day(s); approve a new one soon"),
        (Some(d), false) => format!(", API wallet valid for {d} more days"),
        (None, _) => String::new(),
    };
    Ok(ConnectionResult {
        success: true,
        message: format!(
            "API wallet approved on Hyperliquid{}: {spot_usdc:.2} USDC on spot, {perp_usdc:.2} USDC on perp{expiry} ({latency}ms)",
            if testnet { " testnet" } else { "" }
        ),
        latency_ms: Some(latency),
    })
}

/// Spot tokens and perp positions as the Exchange page's balance list.
pub fn hyperliquid_balances(creds: &StoredCredentials) -> Result<Vec<Balance>, String> {
    let mut out = Vec::new();
    let spot = hyperliquid_account(&creds.wallet_address, &creds.sub_account, &creds.private_key, creds.sandbox, MarketType::Spot)?;
    for b in spot.balances()? {
        if b.free + b.locked > 0.0 {
            out.push(Balance { asset: b.asset, total: b.free + b.locked, available: b.free, in_positions: b.locked });
        }
    }
    let perp = hyperliquid_account(&creds.wallet_address, &creds.sub_account, &creds.private_key, creds.sandbox, MarketType::Perp)?;
    for b in perp.balances()? {
        if b.asset == "USDC" {
            out.push(Balance { asset: "USDC (perp)".into(), total: b.free + b.locked, available: b.free, in_positions: b.locked });
        }
    }
    for p in perp.positions()? {
        out.push(Balance { asset: format!("{} perp {}", p.symbol, p.side), total: p.size, available: 0.0, in_positions: p.size });
    }
    Ok(out)
}

/// The API wallet's approval and expiry for an account (Exchange page).
pub fn hyperliquid_agent(creds: &StoredCredentials) -> Result<hyperliquid::AgentStatus, String> {
    hyperliquid_account(&creds.wallet_address, &creds.sub_account, &creds.private_key, creds.sandbox, MarketType::Spot)?.agent_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markets_per_provider() {
        assert_eq!(markets("Hyperliquid"), vec![MarketType::Spot, MarketType::Perp]);
        assert_eq!(markets("binance"), vec![MarketType::Spot]);
        assert!(markets("uniswap").is_empty());
        assert!(executes("hyperliquid") && executes("bybit") && !executes("jupiter"));
        assert!(has_testnet("hyperliquid"));
        assert!(public("binance", false, MarketType::Perp).is_err(), "no CEX perps");
        assert!(public("hyperliquid", true, MarketType::Perp).is_ok());
    }
}
