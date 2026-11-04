//! The venue commands (PLAN-EXECUTION-VENUES §2, §4, §5, §10, card Venues
//! 1/5): which connected accounts can execute, how a universe maps onto a
//! venue, what a switch and holding would cost, and where the money is.
//! All read-only: nothing here trades.

use std::collections::BTreeMap;

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::execution::cex::DEFAULT_TAKER_FEE;
use crate::execution::mapping::{self, CostPreview, MappingTable};
use crate::execution::venue::{ExecutionVenue, FeeSchedule, MarketType};
use crate::venues::hyperliquid::{self, AgentStatus};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct VenueAccount {
    pub exchange_id: String,
    pub name: String,
    pub provider: String,
    pub sandbox: bool,
    /// The account can take orders from the execution core.
    pub executes: bool,
    pub markets: Vec<MarketType>,
    pub quote: String,
    /// `cex` or `dex` (a wallet account: Hyperliquid's API wallet).
    pub kind: String,
}

/// Every connected account and what it can execute. `read`.
#[tauri::command(async)]
pub(crate) fn venue_list(state: State<'_, AppState>) -> Result<Vec<VenueAccount>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let mut stmt = db
        .prepare("SELECT id, name, provider, sandbox, exchange_type FROM exchanges ORDER BY name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)? != 0, r.get::<_, String>(4)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, name, provider, sandbox, kind) = row;
        let executes = crate::venues::executes(&provider);
        out.push(VenueAccount {
            exchange_id: id,
            quote: mapping::quote_for(&provider).into(),
            markets: crate::venues::markets(&provider),
            name,
            provider,
            sandbox,
            executes,
            kind,
        });
    }
    Ok(out)
}

/// An account's provider and whether it points at the test network.
fn provider_of(state: &AppState, exchange_id: &str) -> Result<(String, bool), String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    db.query_row("SELECT provider, sandbox FROM exchanges WHERE id = ?1", params![exchange_id], |r| {
        Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))
    })
    .map_err(|_| "That account is not connected.".to_string())
}

/// The venue's instruments for the data coins, verified by id. `read`.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn venue_mapping(
    exchange_id: String,
    symbols: Vec<String>,
    market: Option<MarketType>,
    app: AppHandle,
) -> Result<MappingTable, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (provider, testnet) = provider_of(&app.state::<AppState>(), &exchange_id)?;
        crate::venues::mapping_table(&provider, testnet, market.unwrap_or(MarketType::Spot), &symbols)
    })
    .await
    .map_err(|e| format!("Mapping failed: {e}"))?
}

#[derive(Debug, Serialize)]
pub struct CostPreviewView {
    #[serde(flatten)]
    pub preview: CostPreview,
    pub venue: String,
    pub taker_fee: f64,
    /// Where the fee came from: the account's tier, or the venue's standard rate.
    pub fee_source: String,
}

/// What switching `value_usd` of `from` into `to` and holding it `days`
/// costs on an account (§5.1, live books). `read`.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn venue_cost_preview(
    exchange_id: String,
    from: String,
    to: String,
    value_usd: f64,
    market: Option<MarketType>,
    days: Option<u32>,
    app: AppHandle,
) -> Result<CostPreviewView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (provider, testnet) = provider_of(&state, &exchange_id)?;
        let market = market.unwrap_or(MarketType::Spot);
        let quote = mapping::quote_for(&provider);
        let public = crate::venues::public(&provider, testnet, market)?;
        let from_inst = public.instrument(&format!("{from}/{quote}"))?;
        let to_inst = public.instrument(&format!("{to}/{quote}"))?;
        let from_book = public.order_book(&from_inst)?;
        let to_book = public.order_book(&to_inst)?;
        // The account's own tier when its keys can read it.
        let keyed = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::exchanges::load_credentials(&db, &exchange_id)
                .ok()
                .filter(|c| !c.api_key.is_empty() || !c.wallet_address.is_empty())
                .and_then(|c| crate::venues::open(&c, market).ok())
        };
        let standard = standard_taker(&provider, market);
        let (taker, fee_source) = match keyed.as_ref().map(|v| v.fee_schedule(&from_inst)) {
            Some(Ok(f)) if f.taker > 0.0 && f.taker != standard => (f.taker, "the account's fee tier".to_string()),
            _ => (standard, "the venue's standard taker rate (no readable fee tier)".to_string()),
        };
        // Perp: the live funding of the coin held, per 8 hours.
        let funding = (market == MarketType::Perp)
            .then(|| crate::venues::funding_rate_hourly(&provider, testnet, &to).map(|h| h * 8.0).unwrap_or(BASE_FUNDING_PER_8H));
        let direct_inst = public.instruments().ok().and_then(|all| {
            all.into_iter().find(|i| {
                (i.base().eq_ignore_ascii_case(&from) && i.quote().eq_ignore_ascii_case(&to))
                    || (i.base().eq_ignore_ascii_case(&to) && i.quote().eq_ignore_ascii_case(&from))
            })
        });
        let direct_book = direct_inst.as_ref().and_then(|i| public.order_book(i).ok());
        let mid = from_book.mid().ok_or("the order book is empty")?;
        let qty_from = value_usd / mid;
        let preview = mapping::preview(
            &from,
            &to,
            qty_from,
            &from_book,
            &to_book,
            taker,
            match (&direct_inst, &direct_book) {
                (Some(i), Some(b)) => Some((i.clone(), b, taker)),
                _ => None,
            },
            market,
            funding,
            days.unwrap_or(30),
        );
        Ok(CostPreviewView { preview, venue: public.label(), taker_fee: taker, fee_source })
    })
    .await
    .map_err(|e| format!("Cost preview failed: {e}"))?
}

/// Hyperliquid's base funding when the live rate is unreadable: the 0.01 %
/// interest part per 8 hours (PLAN-EXECUTION-VENUES §5.1).
const BASE_FUNDING_PER_8H: f64 = 0.0001;

fn standard_taker(provider: &str, market: MarketType) -> f64 {
    if provider.eq_ignore_ascii_case("hyperliquid") {
        let FeeSchedule { taker, .. } = match market {
            MarketType::Spot => FeeSchedule { maker: hyperliquid::SPOT_MAKER, taker: hyperliquid::SPOT_TAKER },
            MarketType::Perp => FeeSchedule { maker: hyperliquid::PERP_MAKER, taker: hyperliquid::PERP_TAKER },
        };
        taker
    } else {
        DEFAULT_TAKER_FEE
    }
}

/// A wallet account's API wallet: approved by the main address, and its
/// expiry (Hyperliquid warns 7 days ahead). `read`.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn venue_wallet_status(exchange_id: String, app: AppHandle) -> Result<AgentStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let creds = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::exchanges::load_credentials(&db, &exchange_id)?
        };
        if !creds.provider.eq_ignore_ascii_case("hyperliquid") {
            return Err("Only Hyperliquid accounts have an API wallet.".into());
        }
        crate::venues::hyperliquid_agent(&creds)
    })
    .await
    .map_err(|e| format!("Wallet status failed: {e}"))?
}

#[derive(Debug, Serialize)]
pub struct SolanaWallet {
    pub address: String,
    pub sol: f64,
    pub usdc: f64,
    pub value_usd: Option<f64>,
    pub cap: Option<f64>,
    pub rpc_public: bool,
}

/// A generated Solana wallet: its address, what it holds and its cap. `read`.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn solana_wallet_status(exchange_id: String, app: AppHandle) -> Result<SolanaWallet, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let creds = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::exchanges::load_credentials(&db, &exchange_id)?
        };
        if !creds.provider.eq_ignore_ascii_case("jupiter") {
            return Err("Only Jupiter accounts have a Solana wallet.".into());
        }
        let venue = crate::venues::jupiter::JupiterVenue::new("", &creds.wallet_address, &creds.rpc_endpoint, &creds.api_key, None)?;
        let balances = venue.balances()?;
        let held = |a: &str| balances.iter().filter(|b| b.asset == a).map(|b| b.free + b.locked).sum::<f64>();
        Ok(SolanaWallet {
            address: creds.wallet_address.clone(),
            sol: held("SOL"),
            usdc: held("USDC"),
            value_usd: venue.wallet_value_usd().ok(),
            cap: creds.max_value,
            rpc_public: venue.rpc_is_public(),
        })
    })
    .await
    .map_err(|e| format!("Wallet status failed: {e}"))?
}

#[derive(Debug, Serialize)]
pub struct ExportedKey {
    pub address: String,
    /// base58 of seed ‖ public key — Phantom and the Solana CLI read it.
    pub secret: String,
}

/// Export a generated Solana wallet's key (§8). The UI asks first; `confirm`
/// must be the wallet's address, so no stray call reveals it.
#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn wallet_export_key(exchange_id: String, confirm: String, app: AppHandle) -> Result<ExportedKey, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let creds = {
            let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            crate::exchanges::load_credentials(&db, &exchange_id)?
        };
        if !creds.provider.eq_ignore_ascii_case("jupiter") {
            return Err("Only a generated Solana wallet can be exported.".into());
        }
        if confirm.trim() != creds.wallet_address {
            return Err("Confirm with the wallet's address.".into());
        }
        log::warn!(target: "algo", "the key of Solana wallet {} was exported", creds.wallet_address);
        Ok(ExportedKey { address: creds.wallet_address.clone(), secret: creds.private_key.clone() })
    })
    .await
    .map_err(|e| format!("Export failed: {e}"))?
}

#[derive(Debug, Serialize, Default)]
pub struct Exposure {
    pub exchange_id: String,
    pub name: String,
    /// `(owner, asset, quantity, cost basis in USD where known)`.
    pub holdings: Vec<(String, String, f64, Option<f64>)>,
    pub cost_basis: f64,
}

/// What every bot and Manual system holds, per account. `read`.
#[tauri::command(async)]
pub(crate) fn venue_exposure(state: State<'_, AppState>) -> Result<Vec<Exposure>, String> {
    let db = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
    let names: BTreeMap<String, String> = db
        .prepare("SELECT id, name FROM exchanges")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .unwrap_or_default();
    // (account, owner, asset, quantity, cost basis)
    let mut rows: Vec<(String, String, String, f64, Option<f64>)> = Vec::new();
    // Bots: their open journal rows.
    if let Ok(mut stmt) = db.prepare(
        "SELECT b.exchange_id, b.name, t.pair, SUM(t.quantity), SUM(t.quantity * t.entry_price) FROM trades t JOIN bots b ON b.id = t.bot_id
         WHERE t.exit_time IS NULL AND t.is_backtest = 0 GROUP BY b.exchange_id, b.name, t.pair",
    ) {
        if let Ok(found) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, f64>(3)?, r.get::<_, f64>(4)?))) {
            for (ex, bot, pair, qty, cost) in found.flatten() {
                let asset = pair.split('/').next().unwrap_or(&pair).to_string();
                rows.push((ex, format!("bot {bot}"), asset, qty, Some(cost)));
            }
        }
    }
    // Manual systems: what the execution core says each owns.
    if let Ok(mut stmt) = db.prepare("SELECT exchange_id, owner, asset, qty FROM execution_holdings ORDER BY exchange_id, owner") {
        if let Ok(found) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, f64>(3)?))) {
            for (ex, owner, asset, qty) in found.flatten() {
                let usd = (asset == "USDT" || asset == "USDC").then_some(qty);
                rows.push((ex, owner, asset, qty, usd));
            }
        }
    }
    let mut by: BTreeMap<String, Exposure> = BTreeMap::new();
    for (ex, owner, asset, qty, cost) in rows {
        let e = by.entry(ex.clone()).or_insert_with(|| Exposure {
            exchange_id: ex.clone(),
            name: names.get(&ex).cloned().unwrap_or_else(|| ex.clone()),
            ..Default::default()
        });
        e.cost_basis += cost.unwrap_or(0.0);
        e.holdings.push((owner, asset, qty, cost));
    }
    Ok(by.into_values().collect())
}
