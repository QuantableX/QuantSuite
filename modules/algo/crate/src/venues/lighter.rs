//! Lighter as an [`ExecutionVenue`], perp and spot (PLAN-EXECUTION-VENUES
//! §1, §3, §12; card Venues 5/5) — the second order-book DEX, so one venue
//! is never the only option. Tauri-free.
//!
//! **Lighter vs dYdX v4, re-checked 2026-11-04** (the card's first step): on
//! the 11 coins of the systems' universes Lighter's open interest is 10–100×
//! dYdX's (BTC 353 M$ vs 16 M$, 24 h volume 1.4 B$ vs 8 M$), it lists every
//! one of them as perp (dYdX lists them too, mostly with thin books) plus a
//! few spot pairs, and standard accounts pay no trading fee. A Lighter API
//! key signs orders, cancels and leverage; it can also sign a withdrawal,
//! but only to the account's own L1 address — moving funds to anyone else
//! needs the L1 wallet's key, which QuantSuite never asks for. dYdX's
//! permissioned keys would be stricter, its liquidity is not enough.
//!
//! **Signing**: a transaction's fields are Goldilocks field elements hashed
//! with Poseidon2 into a quintic-extension element; that hash is signed with
//! Schnorr over ECgFp5 (`goldilocks-crypto`, `poseidon-hash`, MIT/Apache —
//! Lighter's own signer is a Go library behind a DLL). The known-answer tests
//! hold the official signer's public key, transaction hashes and signatures
//! (Schnorr is randomized, so ours are checked by verification).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use chrono::NaiveDate;
use goldilocks_crypto::{Goldilocks, KeyPair, ScalarField};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::broker::SymbolFilters;
use crate::execution::mapping::{self, MappingTable};
use crate::execution::venue::{
    Capabilities, ExecutionVenue, FeeSchedule, Instrument, MarketType, OrderBook, OrderReport, OrderRequest,
    OrderStatus, Side, VenueBalance, VenuePosition,
};
use crate::venues::hyperliquid::FundingPayment;

pub const MAINNET: &str = "https://mainnet.zklighter.elliot.ai";
pub const TESTNET: &str = "https://testnet.zklighter.elliot.ai";
pub const CHAIN_MAINNET: u32 = 304;
pub const CHAIN_TESTNET: u32 = 300;
pub const TX_CREATE_ORDER: u8 = 14;
pub const TX_CANCEL_ORDER: u8 = 15;
pub const TX_UPDATE_LEVERAGE: u8 = 20;
/// Order type limit, time in force immediate-or-cancel, no expiry (IOC).
const ORDER_TYPE_LIMIT: i64 = 0;
const TIF_IOC: i64 = 0;
const IOC_EXPIRY: i64 = 0;
const ISOLATED: i64 = 1;
/// A transaction is valid for this long after it is signed.
const TX_VALIDITY_MS: i64 = 10 * 60_000;
/// QuantSuite's client order indexes carry this in their top byte (48 bits
/// in all), so the kill switch only ever cancels ours.
const CLIENT_MARKER: u64 = 0x71;
const OURS_PREFIX: &str = "qs#";
const MARKETS_TTL: Duration = Duration::from_secs(15 * 60);
/// An order's outcome is read back this long at most after sending.
const SETTLE_WAIT: Duration = Duration::from_secs(8);

fn num(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())).unwrap_or(0.0)
}

// ── hashing and signing ─────────────────────────────────────────────────────

/// A field element from an integer field (Go's `GoldilocksField(int64)`:
/// the bit pattern, reduced).
fn gl(v: i64) -> Goldilocks {
    Goldilocks::from_noncanonical_u64(v as u64)
}

/// Poseidon2 of the fields into a quintic-extension element, 40 bytes LE.
pub fn hash_fields(fields: &[i64]) -> [u8; 40] {
    let elems: Vec<Goldilocks> = fields.iter().map(|v| gl(*v)).collect();
    poseidon_hash::hash_to_quintic_extension(&elems).to_bytes_le()
}

#[allow(clippy::too_many_arguments)]
pub fn create_order_hash(
    chain: u32,
    nonce: i64,
    expired_at: i64,
    account: i64,
    api_key: u8,
    market: i64,
    client_index: i64,
    base_amount: i64,
    price: i64,
    is_ask: bool,
    reduce_only: bool,
) -> [u8; 40] {
    hash_fields(&[
        chain as i64,
        TX_CREATE_ORDER as i64,
        nonce,
        expired_at,
        account,
        api_key as i64,
        market,
        client_index,
        base_amount,
        price,
        is_ask as i64,
        ORDER_TYPE_LIMIT,
        TIF_IOC,
        reduce_only as i64,
        0, // trigger price
        IOC_EXPIRY,
    ])
}

pub fn cancel_order_hash(chain: u32, nonce: i64, expired_at: i64, account: i64, api_key: u8, market: i64, index: i64) -> [u8; 40] {
    hash_fields(&[chain as i64, TX_CANCEL_ORDER as i64, nonce, expired_at, account, api_key as i64, market, index])
}

#[allow(clippy::too_many_arguments)]
pub fn update_leverage_hash(
    chain: u32,
    nonce: i64,
    expired_at: i64,
    account: i64,
    api_key: u8,
    market: i64,
    initial_margin_fraction: i64,
    margin_mode: i64,
) -> [u8; 40] {
    hash_fields(&[
        chain as i64,
        TX_UPDATE_LEVERAGE as i64,
        nonce,
        expired_at,
        account,
        api_key as i64,
        market,
        initial_margin_fraction,
        margin_mode,
    ])
}

/// The auth token message (`deadline:account:api key`) as field elements:
/// 8-byte little-endian chunks, the last zero-padded.
pub fn auth_message_hash(message: &str) -> [u8; 40] {
    let elems: Vec<Goldilocks> = message
        .as_bytes()
        .chunks(8)
        .map(|c| {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            Goldilocks::from_canonical_u64(u64::from_le_bytes(b))
        })
        .collect();
    poseidon_hash::hash_to_quintic_extension(&elems).to_bytes_le()
}

/// The API key's 40-byte private key.
pub fn parse_key(hex_key: &str) -> Result<Zeroizing<[u8; 40]>, String> {
    let k = hex_key.trim();
    let bytes = Zeroizing::new(hex::decode(k.strip_prefix("0x").unwrap_or(k)).map_err(|_| "The API key's private key is not hex.".to_string())?);
    if bytes.len() != 40 {
        return Err(format!("The API key's private key has {} bytes; Lighter keys have 40.", bytes.len()));
    }
    let mut key = Zeroizing::new([0u8; 40]);
    key.copy_from_slice(&bytes);
    KeyPair::from_private_key_bytes(key.as_slice()).map_err(|e| format!("The API key's private key is not valid: {e}"))?;
    Ok(key)
}

pub fn public_key(private: &[u8; 40]) -> Result<[u8; 40], String> {
    Ok(KeyPair::from_private_key_bytes(private).map_err(|e| e.to_string())?.public_key_bytes())
}

/// Schnorr over ECgFp5 of an already-hashed message (Go's
/// `SchnorrSignHashedMessage`): `s ‖ e`, 80 bytes.
pub fn sign_hash(private: &[u8; 40], hash: &[u8; 40]) -> Result<Vec<u8>, String> {
    let k = ScalarField::sample_crypto().to_bytes_le();
    goldilocks_crypto::sign_hashed_message(private, hash, &k).map_err(|e| e.to_string())
}

pub fn verify(signature: &[u8], hash: &[u8; 40], public: &[u8; 40]) -> bool {
    goldilocks_crypto::verify_signature(signature, hash, public).unwrap_or(false)
}

/// A 48-bit client order index for one of ours: the marker byte and 40 bits
/// of keccak of our id. The same id always maps to the same index.
pub fn client_index_for(client_order_id: &str) -> i64 {
    use sha3::{Digest, Keccak256};
    let h = Keccak256::digest(client_order_id.as_bytes());
    let mut low = [0u8; 8];
    low[3..].copy_from_slice(&h[..5]);
    ((CLIENT_MARKER << 40) | u64::from_be_bytes(low)) as i64
}

pub fn is_ours(client_index: i64) -> bool {
    (client_index as u64) >> 40 == CLIENT_MARKER
}

// ── markets ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Market {
    pub id: i64,
    /// `BTC` (perp) or `ETH/USDC` (spot).
    pub symbol: String,
    pub market: MarketType,
    pub size_decimals: u32,
    pub price_decimals: u32,
    pub min_base: f64,
    pub min_quote: f64,
    pub taker_fee: f64,
    pub maker_fee: f64,
    /// Share of the notional (1.2 % on BTC).
    pub maintenance_margin: Option<f64>,
}

impl Market {
    pub fn base(&self) -> &str {
        self.symbol.split('/').next().unwrap_or(&self.symbol)
    }

    pub fn instrument(&self) -> Instrument {
        Instrument {
            market: self.market,
            filters: SymbolFilters {
                symbol: self.id.to_string(),
                base: self.base().to_string(),
                quote: "USDC".into(),
                qty_step: 10f64.powi(-(self.size_decimals as i32)),
                min_qty: self.min_base.max(10f64.powi(-(self.size_decimals as i32))),
                min_notional: self.min_quote,
                price_tick: 10f64.powi(-(self.price_decimals as i32)),
            },
        }
    }

    pub fn base_units(&self, qty: f64) -> i64 {
        (qty * 10f64.powi(self.size_decimals as i32) + 1e-6).floor() as i64
    }

    pub fn price_units(&self, price: f64, side: Side) -> i64 {
        let raw = price * 10f64.powi(self.price_decimals as i32);
        match side {
            Side::Buy => (raw + 1e-6).floor() as i64,
            Side::Sell => (raw - 1e-6).ceil() as i64,
        }
    }
}

pub fn parse_markets(list: &Value) -> Vec<Market> {
    list.get("order_books")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|m| m["status"].as_str() == Some("active"))
        .filter_map(|m| {
            let market = match m["market_type"].as_str()? {
                "perp" => MarketType::Perp,
                "spot" => MarketType::Spot,
                _ => return None,
            };
            Some(Market {
                id: m["market_id"].as_i64()?,
                symbol: m["symbol"].as_str()?.to_string(),
                market,
                size_decimals: m["supported_size_decimals"].as_u64()? as u32,
                price_decimals: m["supported_price_decimals"].as_u64()? as u32,
                min_base: num(&m["min_base_amount"]),
                min_quote: num(&m["min_quote_amount"]),
                taker_fee: num(&m["taker_fee"]) / 100.0,
                maker_fee: num(&m["maker_fee"]) / 100.0,
                maintenance_margin: m.get("maintenance_margin_fraction").map(|v| num(v) / 10_000.0),
            })
        })
        .collect()
}

type MarketCache = Mutex<HashMap<bool, (Instant, Vec<Market>)>>;

fn market_cache() -> &'static MarketCache {
    static CACHE: OnceLock<MarketCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn nonces() -> &'static Mutex<HashMap<(i64, u8), i64>> {
    static LAST: OnceLock<Mutex<HashMap<(i64, u8), i64>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Nonces per (account, API key): the venue's next nonce once, then
/// strictly increasing in this process.
fn next_local_nonce(key: (i64, u8), from_venue: impl FnOnce() -> Result<i64, String>) -> Result<i64, String> {
    let mut map = nonces().lock().unwrap_or_else(|e| e.into_inner());
    let n = match map.get(&key) {
        Some(last) => last + 1,
        None => from_venue()?,
    };
    map.insert(key, n);
    Ok(n)
}

/// After a refused transaction the venue's count is the truth again.
fn forget_nonce(key: (i64, u8)) {
    nonces().lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
}

// ── fills ───────────────────────────────────────────────────────────────────

/// An order of `accountInactiveOrders` / `accountActiveOrders` as a report.
pub fn report_from_order(o: &Value, client_order_id: &str, inst: &Instrument, fee_rate: f64) -> OrderReport {
    let filled = num(&o["filled_base_amount"]);
    let quote = num(&o["filled_quote_amount"]);
    let initial = num(&o["initial_base_amount"]);
    let status = match o["status"].as_str().unwrap_or("") {
        "filled" => OrderStatus::Filled,
        "open" | "pending" | "in-progress" => OrderStatus::Open,
        "canceled-reduce-only" | "canceled-position-not-allowed" | "canceled-margin-not-allowed" | "canceled-invalid-balance"
            if filled <= 0.0 =>
        {
            OrderStatus::Rejected
        }
        _ if filled > 0.0 && filled + 1e-12 < initial => OrderStatus::PartiallyFilled,
        _ if filled > 0.0 => OrderStatus::Filled,
        _ => OrderStatus::NotFilled,
    };
    let fee = quote * fee_rate;
    OrderReport {
        client_order_id: client_order_id.to_string(),
        venue_order_id: o["order_index"].as_i64().map(|i| i.to_string()).unwrap_or_default(),
        status,
        filled_qty: filled,
        base_received: filled,
        avg_price: if filled > 0.0 { quote / filled } else { 0.0 },
        quote_amount: quote,
        fee_asset: inst.quote().to_string(),
        fee_amount: fee,
        fee_quote: fee,
        // The fee is the market's schedule applied to the fill (0 for
        // standard accounts), not a per-fill figure.
        fee_estimated: fee > 0.0,
        message: o["status"].as_str().filter(|s| s.starts_with("canceled") && filled <= 0.0).map(String::from),
    }
}

/// `positionFunding` rows as payments (`change` is what the account got).
pub fn parse_funding(rows: &Value, markets: &[Market]) -> Vec<FundingPayment> {
    rows.get("position_fundings")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let market = markets.iter().find(|m| Some(m.id) == r["market_id"].as_i64())?;
            let size = num(&r["position_size"]);
            let szi = if r["position_side"].as_str() == Some("short") { -size } else { size };
            Some(FundingPayment {
                time: r["timestamp"].as_i64().map(|t| if t < 10_000_000_000 { t * 1000 } else { t }).unwrap_or(0),
                coin: market.base().to_string(),
                usdc: num(&r["change"]),
                szi,
                rate: num(&r["rate"]),
                hash: r["funding_id"].to_string(),
            })
        })
        .collect()
}

// ── the venue ───────────────────────────────────────────────────────────────

pub struct LighterVenue {
    market: MarketType,
    testnet: bool,
    account: i64,
    api_key_index: u8,
    key: Option<Zeroizing<[u8; 40]>>,
    /// The key's public key: every signature is verified before it goes out.
    public: [u8; 40],
    leverage: u32,
    leverage_set: Mutex<HashSet<i64>>,
    auth: Mutex<Option<(i64, String)>>,
    client: reqwest::blocking::Client,
}

impl std::fmt::Debug for LighterVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LighterVenue")
            .field("market", &self.market)
            .field("testnet", &self.testnet)
            .field("account", &self.account)
            .field("api_key_index", &self.api_key_index)
            .finish()
    }
}

impl LighterVenue {
    pub fn public(market: MarketType, testnet: bool) -> Result<Self, String> {
        Ok(Self {
            market,
            testnet,
            account: -1,
            api_key_index: 0,
            key: None,
            public: [0u8; 40],
            leverage: 1,
            leverage_set: Mutex::new(HashSet::new()),
            auth: Mutex::new(None),
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|e| format!("HTTP client: {e}"))?,
        })
    }

    /// An account: its index, the API key's index (2–254) and private key.
    pub fn new(market: MarketType, testnet: bool, account: &str, api_key_index: &str, private_key: &str) -> Result<Self, String> {
        let mut v = Self::public(market, testnet)?;
        if !account.trim().is_empty() {
            v.account = account.trim().parse().map_err(|_| "The account index is a number (Lighter → Settings → API).".to_string())?;
        }
        if !api_key_index.trim().is_empty() {
            let i: u8 = api_key_index.trim().parse().map_err(|_| "The API key index is a number from 2 to 254.".to_string())?;
            if !(2..=254).contains(&i) {
                return Err("The API key index is from 2 to 254 (0 and 1 belong to Lighter's own front ends).".into());
            }
            v.api_key_index = i;
        }
        if !private_key.trim().is_empty() {
            let key = parse_key(private_key)?;
            v.public = public_key(&key)?;
            v.key = Some(key);
        }
        Ok(v)
    }

    pub fn with_leverage(mut self, leverage: u32) -> Self {
        self.leverage = leverage.max(1);
        self
    }

    fn base_url(&self) -> &'static str {
        if self.testnet {
            TESTNET
        } else {
            MAINNET
        }
    }

    fn chain(&self) -> u32 {
        if self.testnet {
            CHAIN_TESTNET
        } else {
            CHAIN_MAINNET
        }
    }

    fn get(&self, path: &str, query: &[(&str, String)], auth: bool) -> Result<Value, String> {
        let mut req = self.client.get(format!("{}/api/v1/{path}", self.base_url())).query(query);
        if auth {
            req = req.header("authorization", self.auth_token()?);
        }
        let resp = req.send().map_err(|e| format!("Lighter {path}: {e}"))?;
        let status = resp.status();
        let body: Value = resp.json().map_err(|e| format!("Lighter {path} parse: {e}"))?;
        if !status.is_success() || body["code"].as_i64().is_some_and(|c| c != 200 && c != 0) {
            return Err(format!("Lighter {path}: {}", body.get("message").and_then(|m| m.as_str()).unwrap_or(&body.to_string())));
        }
        Ok(body)
    }

    pub fn markets(&self) -> Result<Vec<Market>, String> {
        if let Ok(cache) = market_cache().lock() {
            if let Some((at, m)) = cache.get(&self.testnet) {
                if at.elapsed() < MARKETS_TTL {
                    return Ok(m.clone());
                }
            }
        }
        let mut markets = parse_markets(&self.get("orderBooks", &[], false)?);
        if let Ok(details) = self.get("orderBookDetails", &[], false) {
            for d in details["order_book_details"].as_array().into_iter().flatten() {
                if let Some(m) = markets.iter_mut().find(|m| Some(m.id) == d["market_id"].as_i64()) {
                    m.maintenance_margin = d.get("maintenance_margin_fraction").map(|v| num(v) / 10_000.0).filter(|v| *v > 0.0);
                }
            }
        }
        if markets.is_empty() {
            return Err("Lighter returned no markets.".into());
        }
        if let Ok(mut cache) = market_cache().lock() {
            cache.insert(self.testnet, (Instant::now(), markets.clone()));
        }
        Ok(markets)
    }

    fn market_of(&self, inst: &Instrument) -> Result<Market, String> {
        let id: i64 = inst.filters.symbol.parse().map_err(|_| format!("{} is not a Lighter market", inst.pair()))?;
        self.markets()?.into_iter().find(|m| m.id == id).ok_or_else(|| format!("Lighter has no market {id}"))
    }

    fn signer(&self) -> Result<&[u8; 40], String> {
        self.key.as_deref().ok_or_else(|| "The Lighter account has no API key stored; trading needs it.".to_string())
    }

    fn check_account(&self) -> Result<(), String> {
        if self.account < 0 {
            return Err("The Lighter account has no account index stored.".into());
        }
        Ok(())
    }

    /// The token for private reads: `deadline:account:key:signature`, valid
    /// for an hour, reused until ten minutes before it ends.
    pub fn auth_token(&self) -> Result<String, String> {
        self.check_account()?;
        let now = chrono::Utc::now().timestamp();
        if let Ok(cached) = self.auth.lock() {
            if let Some((deadline, token)) = cached.as_ref() {
                if *deadline - now > 600 {
                    return Ok(token.clone());
                }
            }
        }
        let deadline = now + 3600;
        let message = format!("{deadline}:{}:{}", self.account, self.api_key_index);
        let sig = sign_hash(self.signer()?, &auth_message_hash(&message))?;
        let token = format!("{message}:{}", hex::encode(sig));
        if let Ok(mut cached) = self.auth.lock() {
            *cached = Some((deadline, token.clone()));
        }
        Ok(token)
    }

    fn nonce(&self) -> Result<i64, String> {
        let key = (self.account, self.api_key_index);
        next_local_nonce(key, || {
            let r = self.get("nextNonce", &[("account_index", self.account.to_string()), ("api_key_index", self.api_key_index.to_string())], false)?;
            r["nonce"].as_i64().ok_or_else(|| "Lighter nextNonce: no nonce".to_string())
        })
    }

    /// Sign and send a transaction. `Err` only when the answer is unknown.
    fn send(&self, tx_type: u8, mut info: serde_json::Map<String, Value>, hash: [u8; 40]) -> Result<Value, String> {
        let sig = sign_hash(self.signer()?, &hash)?;
        // Like the official client: never send a signature that does not verify.
        if !verify(&sig, &hash, &self.public) {
            return Err("the transaction signature did not verify; nothing was sent".into());
        }
        info.insert("Sig".into(), json!(base64::engine::general_purpose::STANDARD.encode(sig)));
        info.insert("L2TxAttributes".into(), Value::Null);
        let form = [("tx_type", tx_type.to_string()), ("tx_info", Value::Object(info).to_string())];
        let resp = self
            .client
            .post(format!("{}/api/v1/sendTx", self.base_url()))
            .form(&form)
            .send()
            .map_err(|e| format!("Lighter sendTx: {e}"))?;
        let body: Value = resp.json().map_err(|e| format!("Lighter sendTx parse: {e}"))?;
        if body["code"].as_i64() != Some(200) {
            forget_nonce((self.account, self.api_key_index));
        }
        Ok(body)
    }

    fn tx_header(&self) -> Result<(i64, i64, serde_json::Map<String, Value>), String> {
        self.check_account()?;
        let nonce = self.nonce()?;
        let expired_at = chrono::Utc::now().timestamp_millis() + TX_VALIDITY_MS;
        let mut m = serde_json::Map::new();
        m.insert("AccountIndex".into(), json!(self.account));
        m.insert("ApiKeyIndex".into(), json!(self.api_key_index));
        Ok((nonce, expired_at, m))
    }

    fn ensure_isolated(&self, market: &Market) -> Result<(), String> {
        if self.leverage_set.lock().map(|s| s.contains(&market.id)).unwrap_or(false) {
            return Ok(());
        }
        let (nonce, expired_at, mut info) = self.tx_header()?;
        let fraction = (10_000 / self.leverage.max(1)) as i64;
        info.insert("MarketIndex".into(), json!(market.id));
        info.insert("InitialMarginFraction".into(), json!(fraction));
        info.insert("MarginMode".into(), json!(ISOLATED));
        info.insert("ExpiredAt".into(), json!(expired_at));
        info.insert("Nonce".into(), json!(nonce));
        let hash = update_leverage_hash(self.chain(), nonce, expired_at, self.account, self.api_key_index, market.id, fraction, ISOLATED);
        let r = self.send(TX_UPDATE_LEVERAGE, info, hash)?;
        if r["code"].as_i64() != Some(200) {
            return Err(format!("setting {}x isolated failed: {}", self.leverage, r["message"]));
        }
        if let Ok(mut s) = self.leverage_set.lock() {
            s.insert(market.id);
        }
        Ok(())
    }

    /// The order of one of our client indexes, active or done.
    fn find_order(&self, market: &Market, client_index: i64) -> Result<Option<Value>, String> {
        let q = [("account_index", self.account.to_string()), ("market_id", market.id.to_string())];
        let active = self.get("accountActiveOrders", &q, true)?;
        let pick = |list: &Value| {
            list["orders"].as_array().and_then(|a| a.iter().find(|o| o["client_order_index"].as_i64() == Some(client_index)).cloned())
        };
        if let Some(o) = pick(&active) {
            return Ok(Some(o));
        }
        let done = self.get(
            "accountInactiveOrders",
            &[("account_index", self.account.to_string()), ("market_id", market.id.to_string()), ("limit", "100".into())],
            true,
        )?;
        Ok(pick(&done))
    }

    /// The account's funding payments since `start_ms`.
    pub fn funding_since(&self, start_ms: i64) -> Result<Vec<FundingPayment>, String> {
        let markets = self.markets()?;
        let rows = self.get(
            "positionFunding",
            &[("account_index", self.account.to_string()), ("limit", "100".into()), ("start_timestamp", (start_ms / 1000).to_string())],
            true,
        )?;
        Ok(parse_funding(&rows, &markets).into_iter().filter(|p| p.time >= start_ms).collect())
    }

    /// The closed daily candle of `day` (UTC).
    pub fn daily_close_of(&self, inst: &Instrument, day: NaiveDate) -> Option<f64> {
        let start = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis();
        let r = self
            .get(
                "candles",
                &[
                    ("market_id", inst.filters.symbol.clone()),
                    ("resolution", "1d".into()),
                    ("start_timestamp", start.to_string()),
                    ("end_timestamp", (start + 86_400_000).to_string()),
                    ("count_back", "2".into()),
                ],
                false,
            )
            .ok()?;
        r["c"].as_array()?.iter().find(|c| c["t"].as_i64() == Some(start)).map(|c| num(&c["c"])).filter(|c| *c > 0.0)
    }

    /// The coverage of `symbols` on this venue's market (by the protocol's
    /// own listing).
    pub fn mapping(&self, symbols: &[String]) -> Result<MappingTable, String> {
        let spot_like: Vec<Instrument> = self
            .instruments()?
            .into_iter()
            .map(|i| Instrument { market: MarketType::Spot, ..i })
            .collect();
        Ok(mapping::map_coins(&self.label(), symbols, &spot_like, &mapping::ExchangeCodes, &|i: &Instrument| i.base().to_string()))
    }
}

impl ExecutionVenue for LighterVenue {
    fn label(&self) -> String {
        let m = match self.market {
            MarketType::Spot => "spot",
            MarketType::Perp => "perp",
        };
        format!("Lighter {m}{}", if self.testnet { " (testnet)" } else { "" })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            spot: self.market == MarketType::Spot,
            perp: self.market == MarketType::Perp,
            ioc_limit: true,
            client_order_ids: true,
            sub_accounts: true,
            test_orders: false,
            // An IOC order at a far price stands in for one, see LiveBroker::exit.
            market_orders: false,
        }
    }

    fn ready(&self) -> Result<(), String> {
        self.check_account()?;
        if self.api_key_index < 2 {
            return Err("The Lighter account has no API key index stored.".into());
        }
        self.signer().map(|_| ())
    }

    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        Ok(self.markets()?.into_iter().filter(|m| m.market == self.market).map(|m| m.instrument()).collect())
    }

    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        let (base, quote) = pair.split_once('/').ok_or_else(|| format!("{pair} is not BASE/QUOTE"))?;
        if !quote.eq_ignore_ascii_case("USDC") {
            return Err(format!("Lighter trades against USDC, not {quote}"));
        }
        self.instruments()?
            .into_iter()
            .find(|i| i.base().eq_ignore_ascii_case(base))
            .ok_or_else(|| format!("{} has no {base} market", self.label()))
    }

    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        self.check_account()?;
        let r = self.get("account", &[("by", "index".into()), ("value", self.account.to_string())], false)?;
        let acc = r.pointer("/accounts/0").cloned().ok_or("Lighter: account not found")?;
        let mut out = Vec::new();
        match self.market {
            MarketType::Spot => {
                for a in acc["assets"].as_array().into_iter().flatten() {
                    let (bal, locked) = (num(&a["balance"]), num(&a["locked_balance"]));
                    out.push(VenueBalance { asset: a["symbol"].as_str().unwrap_or("").to_string(), free: (bal - locked).max(0.0), locked });
                }
            }
            MarketType::Perp => {
                out.push(VenueBalance { asset: "USDC".into(), free: num(&acc["available_balance"]), locked: 0.0 });
                for p in acc["positions"].as_array().into_iter().flatten() {
                    let size = num(&p["position"]) * p["sign"].as_i64().unwrap_or(1) as f64;
                    if size > 0.0 {
                        out.push(VenueBalance { asset: p["symbol"].as_str().unwrap_or("").to_string(), free: size, locked: 0.0 });
                    }
                }
            }
        }
        Ok(out)
    }

    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        if self.market == MarketType::Spot {
            return Ok(Vec::new());
        }
        self.check_account()?;
        let r = self.get("account", &[("by", "index".into()), ("value", self.account.to_string())], false)?;
        Ok(r.pointer("/accounts/0/positions")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let size = num(&p["position"]);
                let value = num(&p["position_value"]);
                let imf = num(&p["initial_margin_fraction"]);
                (size > 0.0).then(|| VenuePosition {
                    symbol: p["symbol"].as_str().unwrap_or("").to_string(),
                    side: if p["sign"].as_i64().unwrap_or(1) < 0 { "short" } else { "long" }.into(),
                    size,
                    entry_price: num(&p["avg_entry_price"]),
                    liquidation_price: Some(num(&p["liquidation_price"])).filter(|l| *l > 0.0),
                    mark_price: (value > 0.0).then(|| value / size),
                    leverage: (imf > 0.0).then(|| 100.0 / imf),
                    margin: Some(num(&p["allocated_margin"])),
                })
            })
            .collect())
    }

    fn maintenance_margin(&self, instrument: &Instrument) -> Option<f64> {
        self.market_of(instrument).ok()?.maintenance_margin
    }

    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String> {
        let r = self.get("orderBookOrders", &[("market_id", instrument.filters.symbol.clone()), ("limit", "100".into())], false)?;
        // Individual orders; the book's levels are their sums per price.
        let side = |key: &str, descending: bool| -> Vec<(f64, f64)> {
            let mut levels: Vec<(f64, f64)> = Vec::new();
            for o in r[key].as_array().into_iter().flatten() {
                let (p, q) = (num(&o["price"]), num(&o["remaining_base_amount"]));
                match levels.iter_mut().find(|l| (l.0 - p).abs() < 1e-12) {
                    Some(l) => l.1 += q,
                    None => levels.push((p, q)),
                }
            }
            levels.retain(|l| l.0 > 0.0 && l.1 > 0.0);
            levels.sort_by(|a, b| if descending { b.0.total_cmp(&a.0) } else { a.0.total_cmp(&b.0) });
            levels
        };
        Ok(OrderBook { bids: side("bids", true), asks: side("asks", false) })
    }

    fn fee_schedule(&self, instrument: &Instrument) -> Result<FeeSchedule, String> {
        let m = self.market_of(instrument)?;
        Ok(FeeSchedule { maker: m.maker_fee, taker: m.taker_fee })
    }

    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        self.ready()?;
        let inst = &order.instrument;
        if inst.market != self.market {
            return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, format!("{} cannot trade a {:?} instrument", self.label(), inst.market)));
        }
        let market = self.market_of(inst)?;
        if self.market == MarketType::Perp && !order.reduce_only {
            if let Err(e) = self.ensure_isolated(&market) {
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, e));
            }
        }
        let client_index = client_index_for(&order.client_order_id);
        let base = market.base_units(order.qty);
        let price = market.price_units(order.limit_price, order.side);
        let is_ask = order.side == Side::Sell;
        let (nonce, expired_at, mut info) = self.tx_header()?;
        for (k, v) in [
            ("MarketIndex", json!(market.id)),
            ("ClientOrderIndex", json!(client_index)),
            ("BaseAmount", json!(base)),
            ("Price", json!(price)),
            ("IsAsk", json!(is_ask as i64)),
            ("Type", json!(ORDER_TYPE_LIMIT)),
            ("TimeInForce", json!(TIF_IOC)),
            ("ReduceOnly", json!(order.reduce_only as i64)),
            ("TriggerPrice", json!(0)),
            ("OrderExpiry", json!(IOC_EXPIRY)),
            ("ExpiredAt", json!(expired_at)),
            ("Nonce", json!(nonce)),
        ] {
            info.insert(k.into(), v);
        }
        let hash = create_order_hash(self.chain(), nonce, expired_at, self.account, self.api_key_index, market.id, client_index, base, price, is_ask, order.reduce_only);
        let r = self.send(TX_CREATE_ORDER, info, hash)?;
        if r["code"].as_i64() != Some(200) {
            let why = r.get("message").and_then(|m| m.as_str()).unwrap_or("refused").to_string();
            return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, format!("Lighter: {why}")));
        }
        // The order executes in the next block: read its outcome back.
        let rate = market.taker_fee;
        let started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(400));
            if let Some(o) = self.find_order(&market, client_index)? {
                let report = report_from_order(&o, &order.client_order_id, inst, rate);
                if report.status != OrderStatus::Open || started.elapsed() > SETTLE_WAIT {
                    return Ok(report);
                }
            } else if started.elapsed() > SETTLE_WAIT {
                // Sent, but not seen yet: the engine looks it up again.
                return Err(format!("Lighter accepted order {client_index} but has not shown it yet"));
            }
        }
    }

    fn cancel(&self, instrument: &Instrument, client_order_id: &str) -> Result<(), String> {
        let market = self.market_of(instrument)?;
        let client_index = client_order_id
            .strip_prefix(OURS_PREFIX)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_else(|| client_index_for(client_order_id));
        let Some(o) = self.find_order(&market, client_index)? else { return Ok(()) };
        let index = o["order_index"].as_i64().ok_or("the order has no index")?;
        let (nonce, expired_at, mut info) = self.tx_header()?;
        info.insert("MarketIndex".into(), json!(market.id));
        info.insert("Index".into(), json!(index));
        info.insert("ExpiredAt".into(), json!(expired_at));
        info.insert("Nonce".into(), json!(nonce));
        let hash = cancel_order_hash(self.chain(), nonce, expired_at, self.account, self.api_key_index, market.id, index);
        let r = self.send(TX_CANCEL_ORDER, info, hash)?;
        if r["code"].as_i64() != Some(200) {
            return Err(format!("cancel refused: {}", r["message"]));
        }
        Ok(())
    }

    fn open_orders(&self, instrument: &Instrument) -> Result<Vec<String>, String> {
        let r = self.get(
            "accountActiveOrders",
            &[("account_index", self.account.to_string()), ("market_id", instrument.filters.symbol.clone())],
            true,
        )?;
        Ok(r["orders"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| match o["client_order_index"].as_i64() {
                Some(c) if is_ours(c) => format!("{OURS_PREFIX}{c}"),
                Some(c) => format!("client:{c}"),
                None => format!("order:{}", o["order_index"]),
            })
            .collect())
    }

    fn query_by_client_id(&self, instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        let market = self.market_of(instrument)?;
        let Some(o) = self.find_order(&market, client_index_for(client_order_id))? else { return Ok(None) };
        Ok(Some(report_from_order(&o, client_order_id, instrument, market.taker_fee)))
    }

    fn test_order(&self, _order: &OrderRequest) -> Result<String, String> {
        Err("Lighter has no test order; its testnet is the dry run.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kat() -> Value {
        serde_json::from_str(include_str!("lighter_kat.json")).unwrap()
    }

    fn hex40(s: &str) -> [u8; 40] {
        hex::decode(s.trim_start_matches("0x")).unwrap().try_into().unwrap()
    }

    #[test]
    fn the_public_key_matches_the_official_signer() {
        let k = kat();
        let private = hex40(k["generated_example"]["private"].as_str().unwrap());
        let public = hex40(k["generated_example"]["public"].as_str().unwrap());
        assert_eq!(public_key(&private).unwrap(), public);
    }

    /// The official signer's transactions: our hash of their fields equals
    /// their hash, and our verifier accepts their signature.
    #[test]
    fn transaction_hashes_and_signatures_match_the_official_signer() {
        let k = kat();
        let private = parse_key(k["private_key"].as_str().unwrap()).unwrap();
        let public = public_key(&private).unwrap();
        let chain = k["chain_id"].as_u64().unwrap() as u32;
        let mut checked = 0;
        for case in k["cases"].as_array().unwrap() {
            let t = &case["tx_info"];
            let i = |f: &str| t[f].as_i64().unwrap();
            let api = i("ApiKeyIndex") as u8;
            let hash = match case["name"].as_str().unwrap() {
                n if n.starts_with("create_order") => create_order_hash(
                    chain, i("Nonce"), i("ExpiredAt"), i("AccountIndex"), api, i("MarketIndex"), i("ClientOrderIndex"),
                    i("BaseAmount"), i("Price"), i("IsAsk") == 1, i("ReduceOnly") == 1,
                ),
                "cancel_order" => cancel_order_hash(chain, i("Nonce"), i("ExpiredAt"), i("AccountIndex"), api, i("MarketIndex"), i("Index")),
                "update_leverage_1x_isolated" => update_leverage_hash(
                    chain, i("Nonce"), i("ExpiredAt"), i("AccountIndex"), api, i("MarketIndex"), i("InitialMarginFraction"), i("MarginMode"),
                ),
                other => panic!("unknown case {other}"),
            };
            assert_eq!(hex::encode(hash), case["tx_hash"].as_str().unwrap(), "{}", case["name"]);
            let sig = base64::engine::general_purpose::STANDARD.decode(t["Sig"].as_str().unwrap()).unwrap();
            assert!(verify(&sig, &hash, &public), "the official signature verifies: {}", case["name"]);
            // Ours verifies too (randomized, so checked by verification).
            let ours = sign_hash(&private, &hash).unwrap();
            assert!(verify(&ours, &hash, &public));
            assert_ne!(ours, sig);
            checked += 1;
        }
        assert_eq!(checked, 4);
        // The auth token: our message hash verifies the official signature.
        let token = k["auth_token"].as_str().unwrap();
        let (message, sig) = token.rsplit_once(':').unwrap();
        assert_eq!(message, "1793800000:12345:3");
        assert!(verify(&hex::decode(sig).unwrap(), &auth_message_hash(message), &public));
    }

    #[test]
    fn client_indexes_are_ours_stable_and_48_bits() {
        let a = client_index_for("qs0123456789ab-01");
        assert_eq!(a, client_index_for("qs0123456789ab-01"));
        assert_ne!(a, client_index_for("qs0123456789ab-02"));
        assert!(a > 0 && a < 1 << 48);
        assert!(is_ours(a));
        assert!(!is_ours(7));
    }

    fn markets() -> Vec<Market> {
        parse_markets(&json!({"order_books": [
            {"symbol": "BTC", "market_id": 1, "market_type": "perp", "status": "active", "taker_fee": "0.0000", "maker_fee": "0.0000",
             "min_base_amount": "0.00007", "min_quote_amount": "10.000000", "supported_size_decimals": 5, "supported_price_decimals": 1,
             "maintenance_margin_fraction": 120},
            {"symbol": "ETH/USDC", "market_id": 2048, "market_type": "spot", "status": "active", "taker_fee": "0.0200", "maker_fee": "0.0000",
             "min_base_amount": "0.005", "min_quote_amount": "10.000000", "supported_size_decimals": 4, "supported_price_decimals": 2},
            {"symbol": "OLD", "market_id": 99, "market_type": "perp", "status": "inactive", "taker_fee": "0", "maker_fee": "0",
             "min_base_amount": "1", "min_quote_amount": "1", "supported_size_decimals": 0, "supported_price_decimals": 0}
        ]}))
    }

    #[test]
    fn markets_parse_with_units_and_margins() {
        let m = markets();
        assert_eq!(m.len(), 2, "inactive markets drop out");
        let btc = &m[0];
        assert_eq!(btc.maintenance_margin, Some(0.012));
        assert_eq!(btc.base_units(0.012345678), 1234);
        assert_eq!(btc.price_units(80_123.456, Side::Buy), 801_234);
        assert_eq!(btc.price_units(80_123.456, Side::Sell), 801_235, "a sell limit rounds up, inside the cap");
        let i = btc.instrument();
        assert_eq!((i.pair().as_str(), i.filters.symbol.as_str(), i.market), ("BTC/USDC", "1", MarketType::Perp));
        let eth = &m[1];
        assert_eq!(eth.base(), "ETH");
        assert!((eth.taker_fee - 0.0002).abs() < 1e-12, "fees are given in percent");
    }

    #[test]
    fn orders_and_funding_normalise() {
        let btc = markets()[0].instrument();
        let filled = json!({"order_index": 5, "client_order_index": 9, "initial_base_amount": "0.01", "filled_base_amount": "0.01",
                            "filled_quote_amount": "800.5", "status": "filled"});
        let r = report_from_order(&filled, "qs1", &btc, 0.0);
        assert_eq!(r.status, OrderStatus::Filled);
        assert!((r.avg_price - 80_050.0).abs() < 1e-9);
        assert!(!r.fee_estimated);
        let partial = json!({"order_index": 6, "initial_base_amount": "0.01", "filled_base_amount": "0.004", "filled_quote_amount": "320", "status": "canceled"});
        assert_eq!(report_from_order(&partial, "qs2", &btc, 0.0).status, OrderStatus::PartiallyFilled);
        let none = json!({"order_index": 7, "initial_base_amount": "0.01", "filled_base_amount": "0", "filled_quote_amount": "0", "status": "canceled-too-much-slippage"});
        assert_eq!(report_from_order(&none, "qs3", &btc, 0.0).status, OrderStatus::NotFilled);
        let refused = json!({"order_index": 8, "initial_base_amount": "0.01", "filled_base_amount": "0", "filled_quote_amount": "0", "status": "canceled-reduce-only"});
        assert_eq!(report_from_order(&refused, "qs4", &btc, 0.0).status, OrderStatus::Rejected);
        let rows = json!({"position_fundings": [
            {"timestamp": 1800000000, "market_id": 1, "funding_id": 3, "change": "-0.42", "rate": "0.0012", "position_size": "0.1", "position_side": "long"},
            {"timestamp": 1800003600, "market_id": 1, "funding_id": 4, "change": "0.30", "rate": "0.0010", "position_size": "0.1", "position_side": "short"}
        ]});
        let f = parse_funding(&rows, &markets());
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].coin.as_str(), f[0].szi, f[0].usdc, f[0].time), ("BTC", 0.1, -0.42, 1_800_000_000_000));
        assert_eq!(f[1].szi, -0.1);
    }

    #[test]
    fn an_account_needs_valid_indexes_and_key() {
        assert!(LighterVenue::new(MarketType::Perp, true, "abc", "3", "").is_err());
        assert!(LighterVenue::new(MarketType::Perp, true, "12345", "1", "").is_err(), "API key 0 and 1 are Lighter's");
        assert!(LighterVenue::new(MarketType::Perp, true, "12345", "3", "0x1234").is_err());
        let k = kat();
        let v = LighterVenue::new(MarketType::Perp, true, "12345", "3", k["private_key"].as_str().unwrap()).unwrap();
        assert!(v.ready().is_ok());
        assert!(!format!("{v:?}").contains("1111111111"), "no key in Debug");
        assert!(LighterVenue::public(MarketType::Spot, false).unwrap().ready().is_err());
    }

    // ── live, public calls only (`cargo test -- --ignored lighter_live`) ──

    #[test]
    #[ignore]
    fn lighter_live_public() {
        for testnet in [false, true] {
            let perp = LighterVenue::public(MarketType::Perp, testnet).unwrap();
            let spot = LighterVenue::public(MarketType::Spot, testnet).unwrap();
            let p = perp.instruments().unwrap();
            let s = spot.instruments().unwrap();
            println!("{}: {} perps, {} spot {:?}", perp.label(), p.len(), s.len(), s.iter().map(|i| i.pair()).collect::<Vec<_>>());
            let btc = perp.instrument("BTC/USDC").unwrap();
            let book = perp.order_book(&btc).unwrap();
            println!("BTC bid {:?} ask {:?}, mmr {:?}", book.best_bid(), book.best_ask(), perp.maintenance_margin(&btc));
            assert!(book.best_bid().unwrap() < book.best_ask().unwrap());
            let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
            println!("BTC close {yesterday}: {:?}", perp.daily_close_of(&btc, yesterday));
            println!("{}", perp.mapping(&["BTC".into(), "ETH".into(), "XRP".into(), "LEO".into()]).unwrap().summary);
        }
    }
}
