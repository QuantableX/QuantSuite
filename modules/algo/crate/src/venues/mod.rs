//! Where the orders go (PLAN-EXECUTION-VENUES §9): one place that turns an
//! account into an [`ExecutionVenue`], whatever the venue. Binance and Bybit
//! spot are [`CexVenue`]; Hyperliquid spot and perp are
//! [`hyperliquid::HyperliquidVenue`] (card Venues 2/5); Solana spot through
//! Jupiter is [`jupiter::JupiterVenue`] (card Venues 3/5).

pub mod hyperliquid;
pub mod jupiter;

use chrono::NaiveDate;

use crate::exchanges::StoredCredentials;
use crate::execution::cex::CexVenue;
use crate::execution::mapping::{self, ExchangeCodes, MappingTable};
use crate::execution::venue::{ExecutionVenue, Instrument, MarketType};
use crate::{Balance, ConnectionResult};

use hyperliquid::HyperliquidVenue;
use jupiter::JupiterVenue;

fn is_hyperliquid(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("hyperliquid")
}

fn is_jupiter(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("jupiter")
}

/// The execution core can trade on this provider's accounts.
pub fn executes(provider: &str) -> bool {
    crate::broker::Venue::supports_live(provider) || is_hyperliquid(provider) || is_jupiter(provider)
}

/// Where Jupiter swaps are journaled before they are sent.
fn jupiter_journal() -> jupiter::Journal {
    jupiter::Journal { path: qs_core::paths::module_dir("algo").join("execution").join("jupiter-swaps.jsonl") }
}

fn jupiter_account(creds: &StoredCredentials, journal: bool) -> Result<JupiterVenue, String> {
    JupiterVenue::new(
        &creds.private_key,
        &creds.wallet_address,
        &creds.rpc_endpoint,
        &creds.api_key,
        journal.then(jupiter_journal),
    )
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

/// An account's venue for `market`, with its keys (1x on perp).
pub fn open(creds: &StoredCredentials, market: MarketType) -> Result<Box<dyn ExecutionVenue>, String> {
    open_leveraged(creds, market, 1)
}

/// The same with the leverage an opening perp order sets (isolated).
pub fn open_leveraged(creds: &StoredCredentials, market: MarketType, leverage: u32) -> Result<Box<dyn ExecutionVenue>, String> {
    check_market(&creds.provider, market)?;
    if is_jupiter(&creds.provider) {
        return Ok(Box::new(jupiter_account(creds, true)?));
    }
    if is_hyperliquid(&creds.provider) {
        return Ok(Box::new(
            HyperliquidVenue::new(market, creds.sandbox, &creds.wallet_address, &creds.sub_account, &creds.private_key)?
                .with_leverage(leverage),
        ));
    }
    Ok(Box::new(CexVenue::new(&creds.provider, &creds.api_key, &creds.api_secret, creds.sandbox)?))
}

/// The public side of a provider's `market`: instruments, books, candles.
/// A CEX's sandbox keeps production market data (its sandboxes carry thin
/// markets); Hyperliquid's testnet has its own markets and token ids, so a
/// testnet account reads the testnet.
pub fn public(provider: &str, testnet: bool, market: MarketType) -> Result<Box<dyn ExecutionVenue>, String> {
    check_market(provider, market)?;
    if is_jupiter(provider) {
        return Ok(Box::new(JupiterVenue::public("", "")?));
    }
    if is_hyperliquid(provider) {
        return Ok(Box::new(HyperliquidVenue::public(market, testnet)?));
    }
    Ok(Box::new(CexVenue::new(provider, "", "", false)?))
}

/// The coverage of `symbols` on a provider's market, by verified id.
pub fn mapping_table(provider: &str, testnet: bool, market: MarketType, symbols: &[String]) -> Result<MappingTable, String> {
    check_market(provider, market)?;
    if is_jupiter(provider) {
        return Ok(jupiter::mapping_with("Jupiter (Solana)", symbols, &[]));
    }
    if is_hyperliquid(provider) {
        return HyperliquidVenue::public(market, testnet)?.mapping(symbols);
    }
    let venue = CexVenue::new(provider, "", "", false)?;
    let instruments = venue.instruments()?;
    Ok(mapping::map_coins(&venue.label(), symbols, &instruments, &ExchangeCodes, &|i: &Instrument| i.base().to_string()))
}

/// The closed daily candle of `day` (UTC) of `coin` against `quote` on the
/// provider's own market, when it has one there (Jupiter has no candles).
pub fn daily_close(provider: &str, testnet: bool, market: MarketType, coin: &str, quote: &str, day: NaiveDate) -> Option<f64> {
    if is_jupiter(provider) {
        return None;
    }
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

/// The Exchange page's test of a Solana wallet: the RPC answers, the
/// balance is readable, Jupiter quotes.
pub fn test_jupiter(creds: &StoredCredentials) -> Result<ConnectionResult, String> {
    let fail = |message: String| Ok(ConnectionResult { success: false, message, latency_ms: None });
    let start = std::time::Instant::now();
    let venue = match jupiter_account(creds, false) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let balances = match venue.balances() {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let sol = balances.iter().find(|b| b.asset == "SOL").map(|b| b.free + b.locked).unwrap_or(0.0);
    let usdc = crate::execution::venue::free_balance(&balances, "USDC");
    if let Err(e) = venue.instrument("SOL/USDC").and_then(|i| venue.order_book(&i)) {
        return fail(format!("The wallet reads, but Jupiter does not quote: {e}"));
    }
    let latency = start.elapsed().as_millis() as u64;
    let rpc = if venue.rpc_is_public() { "the public Solana RPC (rate-limited)" } else { "your RPC" };
    let gas = if sol * 1e9 < jupiter::SOL_RESERVE_LAMPORTS as f64 {
        format!(" — add at least {:.2} SOL for network fees", jupiter::SOL_RESERVE_LAMPORTS as f64 / 1e9)
    } else {
        String::new()
    };
    Ok(ConnectionResult {
        success: true,
        message: format!("Wallet {} via {rpc}: {sol:.4} SOL, {usdc:.2} USDC; Jupiter quotes{gas} ({latency}ms)", venue.address()),
        latency_ms: Some(latency),
    })
}

/// A Solana wallet's tokens as the Exchange page's balance list.
pub fn jupiter_balances(creds: &StoredCredentials) -> Result<Vec<Balance>, String> {
    Ok(jupiter_account(creds, false)?
        .balances()?
        .into_iter()
        .filter(|b| b.free + b.locked > 0.0)
        .map(|b| Balance { total: b.free + b.locked, available: b.free, in_positions: b.locked, asset: b.asset })
        .collect())
}

/// Arming a system on a Solana wallet: what the wallet holds must stay under
/// its "max value on this wallet" (§8). `Ok(None)` for other accounts.
pub fn wallet_cap_check(creds: &StoredCredentials) -> Result<Option<String>, String> {
    if !is_jupiter(&creds.provider) {
        return Ok(None);
    }
    let cap = creds.max_value.ok_or("set a \"max value on this wallet\" on the Exchange page before arming")?;
    let value = jupiter_account(creds, false)?.wallet_value_usd()?;
    if value > cap {
        return Err(format!("the wallet holds {value:.2}$, above its cap of {cap:.2}$; move the excess out or raise the cap"));
    }
    Ok(Some(format!("wallet {value:.2}$ of its {cap:.2}$ cap")))
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
        assert!(executes("hyperliquid") && executes("bybit") && executes("jupiter") && !executes("uniswap"));
        assert_eq!(markets("jupiter"), vec![MarketType::Spot]);
        assert!(public("jupiter", false, MarketType::Perp).is_err());
        assert!(has_testnet("hyperliquid"));
        assert!(public("binance", false, MarketType::Perp).is_err(), "no CEX perps");
        assert!(public("hyperliquid", true, MarketType::Perp).is_ok());
    }
}
