//! Jupiter on Solana as an [`ExecutionVenue`], spot only (PLAN-EXECUTION-VENUES
//! §3, §4, §7, §8; card Venues 3/5). Tauri-free.
//!
//! **Why Jupiter, re-checked 2026-11-04** (the card's first step): of the 11
//! coins the systems' Top-5/Top-10 universes held over two years, Jupiter
//! trades 7 with under 0.5 % price impact for 10,000 USDC (BTC, ETH, SOL,
//! TRX, HYPE, ZEC, DOGE; XRP 0.59 %), CoW Protocol on Base 5 (BTC, ETH, SOL,
//! XRP, LINK). CoW does not cover materially more, so Jupiter stays.
//!
//! **The wallet**: QuantSuite generates an ed25519 keypair inside the vault
//! and shows its address; the user funds it with the trading capital only.
//! There is no import (never a seed phrase). The key can do everything, so a
//! "max value on this wallet" cap blocks arming when the wallet holds more.
//!
//! **A swap** (Swap API V2): `GET /order` with this wallet as taker → the
//! limit becomes the minimum output (`slippageBps`, at most 50) and the price
//! impact is capped → our signature goes into the fee payer's slot → the
//! signature (the transaction id) is journaled → `simulateTransaction` on the
//! RPC → `POST /execute`, which lands it with Jupiter's priority-fee strategy
//! and confirms it. An order older than 10 s is fetched again. RFQ routes
//! (`jupiterz`) are excluded: there the market maker pays and signs first,
//! and the transaction id would not be ours. A lost answer is looked up by
//! the journaled signature (`getSignatureStatuses`), never sent again.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::broker::SymbolFilters;
use crate::execution::mapping::{self, MappingTable, VerifiedList};
use crate::execution::venue::{
    Capabilities, ExecutionVenue, FeeSchedule, Instrument, OrderBook, OrderReport, OrderRequest, OrderStatus, Side,
    VenueBalance, VenuePosition,
};

pub const SWAP_API: &str = "https://api.jup.ag/swap/v2";
pub const PRICE_API: &str = "https://api.jup.ag/price/v3";
/// Solana's public RPC: rate-limited, fine for a few calls a day.
pub const PUBLIC_RPC: &str = "https://api.mainnet-beta.solana.com";
/// The plan's default slippage cap (§7) — never more, less when the
/// engine's limit is closer.
pub const MAX_SLIPPAGE_BPS: u32 = 50;
/// A swap whose own price impact is above this does not go out.
pub const MAX_PRICE_IMPACT: f64 = 0.005;
/// An order is signed and sent within this, or fetched again.
pub const ORDER_MAX_AGE: Duration = Duration::from_secs(10);
/// SOL that always stays on the wallet for network fees and token accounts.
pub const SOL_RESERVE_LAMPORTS: u64 = 20_000_000;
/// The swap fee when no order told it yet (Jupiter's platform fee seen on
/// 2026-11-04 was 2 bps; pool fees are inside the price).
pub const DEFAULT_FEE: f64 = 0.0005;
/// The reference size of the synthetic book, in USDC.
const BOOK_SIZE_USD: f64 = 1_000.0;

pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

fn num(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())).unwrap_or(0.0)
}

fn units(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok())).unwrap_or(0)
}

// ── verified tokens ─────────────────────────────────────────────────────────

/// A token QuantSuite trades as a data coin: by mint, with its custodian.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub symbol: &'static str,
    pub name: &'static str,
    pub mint: &'static str,
    pub decimals: u32,
    pub custodian: Option<&'static str>,
}

/// Mints checked on 2026-11-04 (Jupiter's verified list, issuer named on the
/// token). One mint per coin. Not here although Jupiter lists them: HYPE,
/// ZEC and DOGE (issuer not confirmed), ADA and LINK (no real depth).
pub const VERIFIED: &[Token] = &[
    Token { symbol: "USDC", name: "USD Coin", mint: USDC_MINT, decimals: 6, custodian: Some("Circle") },
    Token { symbol: "SOL", name: "SOL", mint: SOL_MINT, decimals: 9, custodian: None },
    Token { symbol: "BTC", name: "cbBTC", mint: "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij", decimals: 8, custodian: Some("Coinbase") },
    Token { symbol: "ETH", name: "ETH (Portal)", mint: "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs", decimals: 8, custodian: Some("Wormhole") },
    Token { symbol: "BNB", name: "BNB (Portal)", mint: "9gP2kCy3wA1ctvYWQk75guqXuHfrEomqydHLtcTCqiLa", decimals: 8, custodian: Some("Wormhole") },
    Token { symbol: "XRP", name: "wXRP", mint: "6UpQcMAb5xMzxc7ZfPaVMgx3KqsvKZdT5U718BzD5We2", decimals: 6, custodian: Some("Hex Trust") },
    Token { symbol: "TRX", name: "TRX", mint: "GbbesPbaYh5uiAZSYNXTc7w9jty1rpg3P9L4JeN4LkKc", decimals: 6, custodian: Some("TRON DAO") },
];

pub fn token_by_symbol(symbol: &str) -> Option<&'static Token> {
    VERIFIED.iter().find(|t| t.symbol.eq_ignore_ascii_case(symbol))
}

pub fn token_by_mint(mint: &str) -> Option<&'static Token> {
    VERIFIED.iter().find(|t| t.mint == mint)
}

pub fn verified_list() -> VerifiedList {
    VerifiedList(VERIFIED.iter().map(|t| (t.symbol, t.mint, t.custodian)).collect())
}

fn instrument_for(t: &Token) -> Instrument {
    Instrument::spot(SymbolFilters {
        symbol: t.mint.to_string(),
        base: t.symbol.to_string(),
        quote: "USDC".into(),
        qty_step: 10f64.powi(-(t.decimals as i32)),
        min_qty: 10f64.powi(-(t.decimals as i32)),
        min_notional: 1.0,
        price_tick: 1e-9,
    })
}

/// Map data coins onto mints. `candidates` are `(mint, symbol shown)` pairs
/// a token list offers — a token merely called "BTC" is rejected.
pub fn mapping_with(label: &str, symbols: &[String], candidates: &[(String, String)]) -> MappingTable {
    let mut raw: Vec<Instrument> = VERIFIED.iter().filter(|t| t.symbol != "USDC").map(|t| {
        let mut i = instrument_for(t);
        i.filters.base = t.mint.to_string();
        i
    }).collect();
    let mut shown: HashMap<String, String> = VERIFIED.iter().map(|t| (t.mint.to_string(), t.symbol.to_string())).collect();
    for (mint, symbol) in candidates {
        if token_by_mint(mint).is_none() {
            let mut i = instrument_for(&VERIFIED[1]);
            i.filters.symbol = mint.clone();
            i.filters.base = mint.clone();
            raw.push(i);
            shown.insert(mint.clone(), symbol.clone());
        }
    }
    let display = |i: &Instrument| shown.get(i.base()).cloned().unwrap_or_else(|| i.base().to_string());
    let mut table = mapping::map_coins(label, symbols, &raw, &verified_list(), &display);
    for m in &mut table.mapped {
        if let Some(t) = token_by_mint(&m.asset_id) {
            m.instrument = format!("{}/USDC", t.name);
        }
    }
    table
}

// ── amounts ─────────────────────────────────────────────────────────────────

pub fn to_units(amount: f64, decimals: u32) -> u64 {
    if amount <= 0.0 || !amount.is_finite() {
        return 0;
    }
    (amount * 10f64.powi(decimals as i32) + 1e-6).floor() as u64
}

pub fn from_units(amount: u64, decimals: u32) -> f64 {
    amount as f64 / 10f64.powi(decimals as i32)
}

/// The slippage (bps) that keeps the worst fill inside the engine's limit,
/// from the quoted price: never above 50, `None` when the quote itself is
/// beyond the limit.
pub fn slippage_for(side: Side, quoted_price: f64, limit: f64) -> Option<u32> {
    if quoted_price <= 0.0 || limit <= 0.0 {
        return None;
    }
    let room = match side {
        // A buy: worst price = quoted / (1 − s) ≤ limit.
        Side::Buy => 1.0 - quoted_price / limit,
        // A sell: worst price = quoted × (1 − s) ≥ limit.
        Side::Sell => 1.0 - limit / quoted_price,
    };
    let bps = (room * 10_000.0).floor();
    if bps < 1.0 {
        None
    } else {
        Some((bps as u32).min(MAX_SLIPPAGE_BPS))
    }
}

/// `/execute`'s wallet amounts as an order report (raw units in, f64 out).
/// The fee is taken in `fee_mint`: the input (`total_in − in_result`) or
/// the output (`out_result − total_out`). The network fee is added to the
/// fee's quote value.
#[allow(clippy::too_many_arguments)]
pub fn report_from_amounts(
    client_order_id: &str,
    signature: &str,
    inst: &Instrument,
    side: Side,
    base: &Token,
    fee_mint: &str,
    (total_in, in_result, out_result, total_out): (u64, u64, u64, u64),
    network_fee_quote: f64,
) -> OrderReport {
    let usdc = 6;
    let base_dec = base.decimals;
    let fee_in_input = match side {
        Side::Buy => fee_mint != base.mint,
        Side::Sell => fee_mint == base.mint,
    };
    let (filled, base_received, quote_amount, fee_asset, fee_amount) = match (side, fee_in_input) {
        // Buy, fee in USDC (the input).
        (Side::Buy, true) => {
            let got = from_units(total_out, base_dec);
            (got, got, from_units(in_result, usdc), inst.quote().to_string(), from_units(total_in.saturating_sub(in_result), usdc))
        }
        // Buy, fee in the bought token.
        (Side::Buy, false) => (
            from_units(out_result, base_dec),
            from_units(total_out, base_dec),
            from_units(total_in, usdc),
            inst.base().to_string(),
            from_units(out_result.saturating_sub(total_out), base_dec),
        ),
        // Sell, fee in the sold token.
        (Side::Sell, true) => (
            from_units(in_result, base_dec),
            from_units(in_result, base_dec),
            from_units(total_out, usdc),
            inst.base().to_string(),
            from_units(total_in.saturating_sub(in_result), base_dec),
        ),
        // Sell, fee in USDC (the output).
        (Side::Sell, false) => (
            from_units(total_in, base_dec),
            from_units(total_in, base_dec),
            from_units(out_result, usdc),
            inst.quote().to_string(),
            from_units(out_result.saturating_sub(total_out), usdc),
        ),
    };
    let avg = if filled > 0.0 { quote_amount / filled } else { 0.0 };
    let fee_quote = if fee_asset == inst.base() { fee_amount * avg } else { fee_amount } + network_fee_quote;
    OrderReport {
        client_order_id: client_order_id.to_string(),
        venue_order_id: signature.to_string(),
        status: if filled > 0.0 { OrderStatus::Filled } else { OrderStatus::NotFilled },
        filled_qty: filled,
        base_received,
        avg_price: avg,
        quote_amount,
        fee_asset,
        fee_amount,
        fee_quote,
        fee_estimated: network_fee_quote > 0.0,
        message: None,
    }
}

// ── keys and transactions ───────────────────────────────────────────────────

/// A new wallet: its address and its secret (seed ‖ public key, base58 —
/// the format Phantom and the Solana CLI read).
pub fn generate_wallet() -> Result<(String, Zeroizing<String>), String> {
    use ring::rand::SecureRandom;
    let mut seed = Zeroizing::new([0u8; 32]);
    ring::rand::SystemRandom::new().fill(seed.as_mut()).map_err(|_| "no system randomness".to_string())?;
    let pair = Ed25519KeyPair::from_seed_unchecked(seed.as_ref()).map_err(|e| format!("key: {e}"))?;
    let public = pair.public_key().as_ref().to_vec();
    let mut secret = Zeroizing::new(seed.to_vec());
    secret.extend_from_slice(&public);
    Ok((bs58::encode(&public).into_string(), Zeroizing::new(bs58::encode(secret.as_slice()).into_string())))
}

/// The keypair from its base58 secret (64 bytes, or a 32-byte seed).
pub fn parse_secret(secret_b58: &str) -> Result<(Ed25519KeyPair, [u8; 32]), String> {
    let bytes = Zeroizing::new(bs58::decode(secret_b58.trim()).into_vec().map_err(|_| "The wallet key is not base58.".to_string())?);
    let pair = match bytes.len() {
        64 => Ed25519KeyPair::from_seed_and_public_key(&bytes[..32], &bytes[32..]).map_err(|_| "The wallet key does not match its address.".to_string())?,
        32 => Ed25519KeyPair::from_seed_unchecked(&bytes).map_err(|_| "The wallet key is not a valid key.".to_string())?,
        _ => return Err("The wallet key has the wrong length.".into()),
    };
    let mut public = [0u8; 32];
    public.copy_from_slice(pair.public_key().as_ref());
    Ok((pair, public))
}

fn compact_u16(b: &[u8], pos: &mut usize) -> Result<usize, String> {
    let mut value = 0usize;
    for i in 0..3 {
        let byte = *b.get(*pos).ok_or("transaction too short")?;
        *pos += 1;
        value |= ((byte & 0x7f) as usize) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("bad length in the transaction".into())
}

/// Sign a serialised transaction (legacy or v0) as `public`: the signature
/// goes into `public`'s slot among the required signers. Refuses a
/// transaction whose fee payer is someone else — the transaction id would
/// not be ours to look up.
pub fn sign_transaction(tx: &[u8], pair: &Ed25519KeyPair, public: &[u8; 32]) -> Result<(Vec<u8>, String), String> {
    let mut pos = 0;
    let n_sigs = compact_u16(tx, &mut pos)?;
    let sigs_at = pos;
    let msg_at = sigs_at + 64 * n_sigs;
    let message = tx.get(msg_at..).ok_or("transaction too short")?;
    let mut m = 0;
    if message.first().ok_or("empty message")? & 0x80 != 0 {
        let version = message[0] & 0x7f;
        if version != 0 {
            return Err(format!("transaction version {version} is not supported"));
        }
        m = 1;
    }
    let required = *message.get(m).ok_or("message too short")? as usize;
    m += 3;
    let n_keys = compact_u16(message, &mut m)?;
    if n_keys < required || required == 0 || n_sigs != required {
        return Err("the transaction's signer list is inconsistent".into());
    }
    let key = |i: usize| message.get(m + 32 * i..m + 32 * (i + 1));
    if key(0) != Some(public.as_slice()) {
        return Err("this wallet is not the transaction's fee payer".into());
    }
    let sig = pair.sign(message);
    let mut out = tx.to_vec();
    out[sigs_at..sigs_at + 64].copy_from_slice(sig.as_ref());
    Ok((out, bs58::encode(sig.as_ref()).into_string()))
}

// ── the journal ─────────────────────────────────────────────────────────────

/// One swap as sent: written before the send, so a lost answer is looked
/// up by its signature instead of being sent again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwapRecord {
    pub client_order_id: String,
    pub signature: String,
    pub request_id: String,
    pub side: Side,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: u64,
    pub quoted_out: u64,
    pub fee_mint: String,
    pub last_valid_block_height: Option<u64>,
    pub network_fee_lamports: u64,
    /// `signed` (before the send), `not_sent` (refused before the send),
    /// `sent` (Jupiter answered).
    pub state: String,
    pub at: String,
}

/// Append-only JSON lines next to the execution store.
#[derive(Debug, Clone)]
pub struct Journal {
    pub path: PathBuf,
}

impl Journal {
    pub fn append(&self, rec: &SwapRecord) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        let line = serde_json::to_string(rec).map_err(|e| e.to_string())?;
        writeln!(f, "{line}").and_then(|_| f.sync_all()).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// The newest record of a client order id.
    pub fn latest(&self, client_order_id: &str) -> Option<SwapRecord> {
        let f = std::fs::File::open(&self.path).ok()?;
        std::io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str::<SwapRecord>(&l).ok())
            .filter(|r| r.client_order_id == client_order_id)
            .last()
    }
}

// ── the venue ───────────────────────────────────────────────────────────────

/// Keyless Jupiter allows 0.5 requests per second; with a key, more.
fn throttle(keyed: bool) {
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let gap = if keyed { Duration::from_millis(300) } else { Duration::from_millis(2_100) };
    let mut last = LAST.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        let wait = gap.saturating_sub(at.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

/// The fee rate the last order showed.
fn last_fee() -> &'static Mutex<Option<f64>> {
    static FEE: OnceLock<Mutex<Option<f64>>> = OnceLock::new();
    FEE.get_or_init(|| Mutex::new(None))
}

pub struct JupiterVenue {
    key: Option<(Ed25519KeyPair, [u8; 32])>,
    address: String,
    rpc: String,
    api_key: Zeroizing<String>,
    journal: Option<Journal>,
    client: reqwest::blocking::Client,
    /// Synthetic books per mint: (read at, bid, ask).
    books: Mutex<HashMap<String, (Instant, f64, f64)>>,
}

impl std::fmt::Debug for JupiterVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JupiterVenue").field("address", &self.address).field("rpc", &self.rpc).finish()
    }
}

impl JupiterVenue {
    /// Quotes and books only.
    pub fn public(rpc: &str, api_key: &str) -> Result<Self, String> {
        Ok(Self {
            key: None,
            address: String::new(),
            rpc: if rpc.trim().is_empty() { PUBLIC_RPC.to_string() } else { rpc.trim().to_string() },
            api_key: Zeroizing::new(api_key.trim().to_string()),
            journal: None,
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| format!("HTTP client: {e}"))?,
            books: Mutex::new(HashMap::new()),
        })
    }

    /// The account: its generated wallet, the RPC and an optional API key.
    pub fn new(secret_b58: &str, address: &str, rpc: &str, api_key: &str, journal: Option<Journal>) -> Result<Self, String> {
        let mut v = Self::public(rpc, api_key)?;
        if !secret_b58.trim().is_empty() {
            let (pair, public) = parse_secret(secret_b58)?;
            let derived = bs58::encode(public).into_string();
            if !address.trim().is_empty() && derived != address.trim() {
                return Err("The stored wallet key does not belong to the stored address.".into());
            }
            v.address = derived;
            v.key = Some((pair, public));
        } else {
            v.address = address.trim().to_string();
        }
        v.journal = journal;
        Ok(v)
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn rpc_is_public(&self) -> bool {
        self.rpc == PUBLIC_RPC
    }

    fn api_get(&self, url: &str, query: &[(&str, String)]) -> Result<Value, String> {
        throttle(!self.api_key.is_empty());
        let mut req = self.client.get(url).query(query);
        if !self.api_key.is_empty() {
            req = req.header("x-api-key", self.api_key.as_str());
        }
        let resp = req.send().map_err(|e| format!("Jupiter: {e}"))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("Jupiter: {e}"))?;
        if !status.is_success() {
            return Err(format!("Jupiter: HTTP {status}: {}", text.chars().take(200).collect::<String>()));
        }
        serde_json::from_str(&text).map_err(|e| format!("Jupiter parse: {e}"))
    }

    pub fn rpc_call(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp: Value = self
            .client
            .post(&self.rpc)
            .json(&body)
            .send()
            .map_err(|e| format!("Solana RPC {method}: {e}"))?
            .json()
            .map_err(|e| format!("Solana RPC {method} parse: {e}"))?;
        if let Some(err) = resp.get("error") {
            return Err(format!("Solana RPC {method}: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or(&err.to_string())));
        }
        Ok(resp["result"].clone())
    }

    /// `/order`: with `taker` an assembled transaction, without a quote only.
    fn order(&self, input: &str, output: &str, amount: u64, taker: bool, slippage_bps: Option<u32>) -> Result<Value, String> {
        let mut q = vec![
            ("inputMint", input.to_string()),
            ("outputMint", output.to_string()),
            ("amount", amount.to_string()),
            // The transaction id must be our own signature (see the module doc).
            ("excludeRouters", "jupiterz".to_string()),
        ];
        if taker {
            q.push(("taker", self.address.clone()));
        }
        if let Some(bps) = slippage_bps {
            q.push(("slippageBps", bps.to_string()));
        }
        let o = self.api_get(&format!("{SWAP_API}/order"), &q)?;
        if let Some(bps) = o.get("feeBps").and_then(|v| v.as_f64()) {
            if let Ok(mut f) = last_fee().lock() {
                *f = Some(bps / 10_000.0);
            }
        }
        Ok(o)
    }

    fn base_token(&self, inst: &Instrument) -> Result<&'static Token, String> {
        token_by_symbol(inst.base())
            .filter(|t| t.symbol != "USDC" && t.mint == inst.filters.symbol)
            .ok_or_else(|| format!("{} is not on Jupiter's verified list here", inst.pair()))
    }

    /// The price to buy and to sell about 1,000 USDC of a token.
    fn synthetic_book(&self, t: &Token) -> Result<(f64, f64), String> {
        if let Ok(books) = self.books.lock() {
            if let Some((at, bid, ask)) = books.get(t.mint) {
                if at.elapsed() < ORDER_MAX_AGE {
                    return Ok((*bid, *ask));
                }
            }
        }
        let buy = self.order(USDC_MINT, t.mint, to_units(BOOK_SIZE_USD, 6), false, None)?;
        let got = from_units(units(&buy["outAmount"]), t.decimals);
        if got <= 0.0 {
            return Err(format!("Jupiter has no route USDC → {}", t.symbol));
        }
        let ask = BOOK_SIZE_USD / got;
        let sell = self.order(t.mint, USDC_MINT, to_units(BOOK_SIZE_USD / ask, t.decimals), false, None)?;
        let back = from_units(units(&sell["outAmount"]), 6);
        if back <= 0.0 {
            return Err(format!("Jupiter has no route {} → USDC", t.symbol));
        }
        let bid = back / (BOOK_SIZE_USD / ask);
        if let Ok(mut books) = self.books.lock() {
            books.insert(t.mint.to_string(), (Instant::now(), bid, ask));
        }
        Ok((bid, ask))
    }

    fn journal(&self, rec: &SwapRecord) -> Result<(), String> {
        match &self.journal {
            Some(j) => j.append(rec),
            None => Err("no swap journal: refusing to send a swap that could not be looked up".into()),
        }
    }

    fn simulate(&self, tx: &[u8]) -> Result<(), String> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(tx);
        let r = self.rpc_call(
            "simulateTransaction",
            json!([b64, {"encoding": "base64", "sigVerify": true, "replaceRecentBlockhash": false, "commitment": "processed"}]),
        )?;
        match r.pointer("/value/err") {
            Some(Value::Null) | None => Ok(()),
            Some(err) => {
                let logs: Vec<String> = r
                    .pointer("/value/logs")
                    .and_then(|l| l.as_array())
                    .map(|l| l.iter().rev().take(2).filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                Err(format!("simulation failed: {err} {}", logs.join(" | ")))
            }
        }
    }

    fn sol_price(&self) -> Option<f64> {
        let p = self.api_get(PRICE_API, &[("ids", SOL_MINT.to_string())]).ok()?;
        p.get(SOL_MINT).map(|x| num(&x["usdPrice"])).filter(|p| *p > 0.0)
    }

    /// Every verified token's USD price (Jupiter's price API).
    pub fn prices(&self) -> Result<HashMap<String, f64>, String> {
        let ids: Vec<&str> = VERIFIED.iter().map(|t| t.mint).collect();
        let p = self.api_get(PRICE_API, &[("ids", ids.join(","))])?;
        Ok(VERIFIED
            .iter()
            .filter_map(|t| p.get(t.mint).map(|x| (t.symbol.to_string(), num(&x["usdPrice"]))))
            .collect())
    }

    /// What the wallet holds, valued in USD (SOL reserve included).
    pub fn wallet_value_usd(&self) -> Result<f64, String> {
        let prices = self.prices()?;
        Ok(self
            .balances()?
            .iter()
            .map(|b| (b.free + b.locked) * if b.asset == "USDC" { 1.0 } else { prices.get(&b.asset).copied().unwrap_or(0.0) })
            .sum())
    }

    /// A journaled swap from the chain: its status, and its fill.
    fn read_back(&self, rec: &SwapRecord, inst: &Instrument) -> Result<Option<OrderReport>, String> {
        let st = self.rpc_call("getSignatureStatuses", json!([[rec.signature], {"searchTransactionHistory": true}]))?;
        let status = st.pointer("/value/0").cloned().unwrap_or(Value::Null);
        if status.is_null() {
            let height = self.rpc_call("getBlockHeight", json!([{"commitment": "confirmed"}])).ok().and_then(|h| h.as_u64());
            return Ok(Some(match (height, rec.last_valid_block_height) {
                (Some(h), Some(last)) if h > last => OrderReport::empty(&rec.client_order_id, OrderStatus::NotFilled, "the swap expired before it landed"),
                _ => OrderReport::empty(&rec.client_order_id, OrderStatus::Open, "the swap is not confirmed yet"),
            }));
        }
        if !status["err"].is_null() {
            return Ok(Some(OrderReport::empty(&rec.client_order_id, OrderStatus::NotFilled, format!("the swap failed on chain: {}", status["err"]))));
        }
        let tx = self.rpc_call(
            "getTransaction",
            json!([rec.signature, {"encoding": "jsonParsed", "maxSupportedTransactionVersion": 0, "commitment": "confirmed"}]),
        )?;
        let base = self.base_token(inst)?;
        let delta = |mint: &str| -> f64 {
            let sum = |key: &str| -> f64 {
                tx.pointer(&format!("/meta/{key}"))
                    .and_then(|v| v.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter(|r| r["owner"].as_str() == Some(self.address.as_str()) && r["mint"].as_str() == Some(mint))
                            .map(|r| num(&r.pointer("/uiTokenAmount/uiAmountString").cloned().unwrap_or_default()))
                            .sum()
                    })
                    .unwrap_or(0.0)
            };
            let mut d = sum("postTokenBalances") - sum("preTokenBalances");
            if mint == SOL_MINT {
                // Native SOL of the fee payer (index 0), without the fee.
                let pre = tx.pointer("/meta/preBalances/0").map(num).unwrap_or(0.0);
                let post = tx.pointer("/meta/postBalances/0").map(num).unwrap_or(0.0);
                let fee = tx.pointer("/meta/fee").map(num).unwrap_or(0.0);
                d += (post - pre + fee) / 1e9;
            }
            d
        };
        let (b, q) = (delta(base.mint), delta(USDC_MINT));
        let (qty, quote) = match rec.side {
            Side::Buy => (b.max(0.0), (-q).max(0.0)),
            Side::Sell => ((-b).max(0.0), q.max(0.0)),
        };
        let network = tx.pointer("/meta/fee").map(num).unwrap_or(0.0) / 1e9 * self.sol_price().unwrap_or(0.0);
        Ok(Some(OrderReport {
            client_order_id: rec.client_order_id.clone(),
            venue_order_id: rec.signature.clone(),
            status: if qty > 0.0 { OrderStatus::Filled } else { OrderStatus::NotFilled },
            filled_qty: qty,
            base_received: qty,
            avg_price: if qty > 0.0 { quote / qty } else { 0.0 },
            quote_amount: quote,
            fee_asset: inst.quote().to_string(),
            fee_amount: network,
            fee_quote: network,
            // The swap fee is inside the wallet deltas; the network fee valued.
            fee_estimated: true,
            message: Some("read back from the chain".into()),
        }))
    }
}

impl ExecutionVenue for JupiterVenue {
    fn label(&self) -> String {
        "Jupiter (Solana)".into()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { spot: true, perp: false, ioc_limit: true, client_order_ids: true, sub_accounts: false, test_orders: false, market_orders: false }
    }

    fn ready(&self) -> Result<(), String> {
        if self.key.is_none() {
            return Err("The Solana wallet has no key stored.".into());
        }
        if self.journal.is_none() {
            return Err("The swap journal is not available.".into());
        }
        Ok(())
    }

    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        Ok(VERIFIED.iter().filter(|t| t.symbol != "USDC").map(instrument_for).collect())
    }

    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        let (base, quote) = pair.split_once('/').ok_or_else(|| format!("{pair} is not BASE/QUOTE"))?;
        if !quote.eq_ignore_ascii_case("USDC") {
            return Err(format!("Jupiter trades against USDC here, not {quote}"));
        }
        token_by_symbol(base)
            .filter(|t| t.symbol != "USDC")
            .map(instrument_for)
            .ok_or_else(|| format!("{base} has no verified mint on Jupiter"))
    }

    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        if self.address.is_empty() {
            return Err("The Solana wallet has no address stored.".into());
        }
        let lamports = self.rpc_call("getBalance", json!([self.address]))?.pointer("/value").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut by: HashMap<String, f64> = HashMap::new();
        for program in [TOKEN_PROGRAM, TOKEN_2022_PROGRAM] {
            let r = self.rpc_call("getTokenAccountsByOwner", json!([self.address, {"programId": program}, {"encoding": "jsonParsed"}]))?;
            for acc in r.pointer("/value").and_then(|v| v.as_array()).into_iter().flatten() {
                let info = acc.pointer("/account/data/parsed/info").cloned().unwrap_or_default();
                let mint = info["mint"].as_str().unwrap_or("");
                let amount = num(&info.pointer("/tokenAmount/uiAmountString").cloned().unwrap_or_default());
                let asset = token_by_mint(mint).map(|t| t.symbol.to_string()).unwrap_or_else(|| format!("mint {mint}"));
                *by.entry(asset).or_insert(0.0) += amount;
            }
        }
        let reserve = lamports.min(SOL_RESERVE_LAMPORTS);
        let mut out = vec![VenueBalance {
            asset: "SOL".into(),
            free: from_units(lamports - reserve, 9) + by.remove("SOL").unwrap_or(0.0),
            locked: from_units(reserve, 9),
        }];
        out.extend(by.into_iter().filter(|(_, v)| *v > 0.0).map(|(asset, free)| VenueBalance { asset, free, locked: 0.0 }));
        Ok(out)
    }

    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        Ok(Vec::new())
    }

    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String> {
        let t = self.base_token(instrument)?;
        let (bid, ask) = self.synthetic_book(t)?;
        // One deep level at the quoted price: the real price impact of the
        // size is checked against MAX_PRICE_IMPACT when the order is built.
        let depth = 1e12;
        Ok(OrderBook { bids: vec![(bid, depth / bid)], asks: vec![(ask, depth / ask)] })
    }

    fn fee_schedule(&self, _instrument: &Instrument) -> Result<FeeSchedule, String> {
        let fee = last_fee().lock().ok().and_then(|f| *f).unwrap_or(DEFAULT_FEE);
        Ok(FeeSchedule { maker: fee, taker: fee })
    }

    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        self.ready()?;
        let inst = &order.instrument;
        let base = self.base_token(inst)?;
        let (pair, public) = self.key.as_ref().ok_or("no key")?;
        let (input, output, amount_in) = match order.side {
            Side::Buy => {
                let (_, ask) = self.synthetic_book(base)?;
                (USDC_MINT, base.mint, to_units(order.qty * ask, 6))
            }
            Side::Sell => (base.mint, USDC_MINT, to_units(order.qty, base.decimals)),
        };
        if amount_in == 0 {
            return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, "nothing to swap"));
        }
        let (in_dec, out_dec) = match order.side {
            Side::Buy => (6, base.decimals),
            Side::Sell => (base.decimals, 6),
        };
        let price_of = |out: u64| -> f64 {
            let (i, o) = (from_units(amount_in, in_dec), from_units(out, out_dec));
            match order.side {
                Side::Buy if o > 0.0 => i / o,
                Side::Sell if i > 0.0 => o / i,
                _ => 0.0,
            }
        };
        let mut attempt = 0;
        loop {
            attempt += 1;
            // The slippage that keeps the worst fill inside the limit, from a
            // first look at the price; tightened once if the order moved.
            let (bid, ask) = self.synthetic_book(base)?;
            let estimate = if order.side == Side::Buy { ask } else { bid };
            let Some(bps) = slippage_for(order.side, estimate, order.limit_price) else {
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, "the price is beyond the limit"));
            };
            let fetched = Instant::now();
            let o = self.order(input, output, amount_in, true, Some(bps))?;
            let tx_b64 = o["transaction"].as_str().unwrap_or("");
            if tx_b64.is_empty() {
                let why = o["errorMessage"].as_str().unwrap_or("no transaction in the order");
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, format!("Jupiter: {why}")));
            }
            let quoted = price_of(units(&o["outAmount"]));
            let worst = price_of(units(&o["otherAmountThreshold"]));
            let inside = match order.side {
                Side::Buy => worst <= order.limit_price * (1.0 + 1e-9),
                Side::Sell => worst + 1e-12 >= order.limit_price,
            };
            if !inside {
                if attempt < 2 {
                    // The order's own price is the better estimate for the retry.
                    let (bid, ask) = if order.side == Side::Buy { (bid, quoted) } else { (quoted, ask) };
                    if let Ok(mut books) = self.books.lock() {
                        books.insert(base.mint.to_string(), (Instant::now(), bid, ask));
                    }
                    continue;
                }
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, format!("worst price {worst} is beyond the limit {}", order.limit_price)));
            }
            let impact = num(&o["priceImpact"]).abs() / 100.0;
            if impact > MAX_PRICE_IMPACT {
                return Ok(OrderReport::empty(
                    &order.client_order_id,
                    OrderStatus::NotFilled,
                    format!("price impact {:.2} % is above the {:.2} % cap", impact * 100.0, MAX_PRICE_IMPACT * 100.0),
                ));
            }
            let tx = base64::engine::general_purpose::STANDARD.decode(tx_b64).map_err(|e| format!("transaction: {e}"))?;
            let (signed, signature) = match sign_transaction(&tx, pair, public) {
                Ok(s) => s,
                Err(e) => return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, e)),
            };
            let network = o["signatureFeeLamports"].as_u64().unwrap_or(0)
                + o["prioritizationFeeLamports"].as_u64().unwrap_or(0)
                + o["rentFeeLamports"].as_u64().unwrap_or(0);
            let mut rec = SwapRecord {
                client_order_id: order.client_order_id.clone(),
                signature: signature.clone(),
                request_id: o["requestId"].as_str().unwrap_or("").to_string(),
                side: order.side,
                input_mint: input.into(),
                output_mint: output.into(),
                in_amount: amount_in,
                quoted_out: units(&o["outAmount"]),
                fee_mint: o["feeMint"].as_str().unwrap_or("").to_string(),
                last_valid_block_height: o["lastValidBlockHeight"].as_u64(),
                network_fee_lamports: network,
                state: "signed".into(),
                at: chrono::Utc::now().to_rfc3339(),
            };
            self.journal(&rec)?;
            if let Err(e) = self.simulate(&signed) {
                rec.state = "not_sent".into();
                self.journal(&rec)?;
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, e));
            }
            if fetched.elapsed() > ORDER_MAX_AGE {
                rec.state = "not_sent".into();
                self.journal(&rec)?;
                if attempt < 3 {
                    continue;
                }
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, "the order went stale twice"));
            }
            throttle(!self.api_key.is_empty());
            let mut req = self.client.post(format!("{SWAP_API}/execute")).json(&json!({
                "signedTransaction": base64::engine::general_purpose::STANDARD.encode(&signed),
                "requestId": rec.request_id,
            }));
            if !self.api_key.is_empty() {
                req = req.header("x-api-key", self.api_key.as_str());
            }
            // A lost answer is an Err: the engine looks the order up by its
            // id, which finds this signature in the journal.
            let resp: Value = req.send().and_then(|r| r.json()).map_err(|e| format!("Jupiter execute: {e}"))?;
            rec.state = "sent".into();
            let _ = self.journal(&rec);
            if resp["status"].as_str() != Some("Success") {
                let code = resp["code"].as_i64().unwrap_or(0);
                let why = resp["error"].as_str().unwrap_or("failed").to_string();
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::NotFilled, format!("Jupiter execute {code}: {why}")));
            }
            let network_quote = from_units(network, 9) * self.sol_price().unwrap_or(0.0);
            let mut r = report_from_amounts(
                &order.client_order_id,
                resp["signature"].as_str().unwrap_or(&signature),
                inst,
                order.side,
                base,
                &rec.fee_mint,
                (units(&resp["totalInputAmount"]), units(&resp["inputAmountResult"]), units(&resp["outputAmountResult"]), units(&resp["totalOutputAmount"])),
                network_quote,
            );
            r.message = Some(format!("{} bps slippage cap, impact {:.3} %", bps, impact * 100.0));
            return Ok(r);
        }
    }

    fn cancel(&self, _instrument: &Instrument, _client_order_id: &str) -> Result<(), String> {
        Err("a Jupiter swap never rests; there is nothing to cancel".into())
    }

    fn query_by_client_id(&self, instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        let Some(rec) = self.journal.as_ref().and_then(|j| j.latest(client_order_id)) else { return Ok(None) };
        if rec.state == "not_sent" {
            return Ok(None);
        }
        self.read_back(&rec, instrument)
    }

    fn test_order(&self, _order: &OrderRequest) -> Result<String, String> {
        Err("Jupiter has no test order; a small canary swap is the test.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kat() -> Value {
        serde_json::from_str(include_str!("jupiter_kat.json")).unwrap()
    }

    #[test]
    fn transactions_are_signed_like_solders() {
        let k = kat();
        let (pair, public) = parse_secret(k["secret_b58"].as_str().unwrap()).unwrap();
        assert_eq!(bs58::encode(public).into_string(), k["address"].as_str().unwrap());
        // The 32-byte seed gives the same key.
        let seed = hex::decode(k["seed_hex"].as_str().unwrap()).unwrap();
        let (_, from_seed) = parse_secret(&bs58::encode(&seed).into_string()).unwrap();
        assert_eq!(from_seed, public);
        for case in k["cases"].as_array().unwrap() {
            let unsigned = base64::engine::general_purpose::STANDARD.decode(case["unsigned_b64"].as_str().unwrap()).unwrap();
            let (signed, sig) = sign_transaction(&unsigned, &pair, &public).unwrap();
            assert_eq!(sig, case["signature"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(base64::engine::general_purpose::STANDARD.encode(&signed), case["signed_b64"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn someone_elses_transaction_is_not_signed() {
        let k = kat();
        let (pair, _) = parse_secret(k["secret_b58"].as_str().unwrap()).unwrap();
        let unsigned = base64::engine::general_purpose::STANDARD.decode(k["cases"][0]["unsigned_b64"].as_str().unwrap()).unwrap();
        let other = [7u8; 32];
        assert!(sign_transaction(&unsigned, &pair, &other).unwrap_err().contains("fee payer"));
        assert!(sign_transaction(&unsigned[..20], &pair, &other).is_err());
    }

    #[test]
    fn generated_wallets_are_new_and_round_trip() {
        let (a, sa) = generate_wallet().unwrap();
        let (b, _) = generate_wallet().unwrap();
        assert_ne!(a, b);
        let (_, public) = parse_secret(&sa).unwrap();
        assert_eq!(bs58::encode(public).into_string(), a);
        assert_eq!(bs58::decode(sa.as_str()).into_vec().unwrap().len(), 64);
        assert!(JupiterVenue::new(&sa, &b, "", "", None).is_err(), "a key for another address is refused");
    }

    #[test]
    fn mints_map_by_address_and_a_fake_btc_is_rejected() {
        let fake = ("FakeBtc1111111111111111111111111111111111111".to_string(), "BTC".to_string());
        let t = mapping_with("Jupiter (Solana)", &["BTC".into(), "SOL".into(), "ADA".into()], std::slice::from_ref(&fake));
        assert_eq!(t.mapped.len(), 2);
        assert_eq!(t.mapped[0].asset_id, "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij");
        assert_eq!(t.mapped[0].custodian.as_deref(), Some("Coinbase"));
        assert_eq!(t.mapped[0].instrument, "cbBTC/USDC");
        assert_eq!(t.missing, vec!["ADA"]);
        assert_eq!(t.rejected, vec![("BTC".to_string(), fake.0)]);
    }

    #[test]
    fn slippage_keeps_the_worst_fill_inside_the_limit() {
        // Limit 0.5 % above the quote: 49 bps (floored), capped at 50.
        assert_eq!(slippage_for(Side::Buy, 100.0, 100.5), Some(49));
        assert_eq!(slippage_for(Side::Buy, 100.0, 110.0), Some(50));
        assert_eq!(slippage_for(Side::Buy, 100.0, 100.0), None, "the quote is at the limit");
        assert_eq!(slippage_for(Side::Sell, 100.0, 99.5), Some(50));
        assert_eq!(slippage_for(Side::Sell, 100.0, 99.8), Some(20));
        assert_eq!(slippage_for(Side::Sell, 99.0, 99.5), None);
        // The worst buy price at 49 bps stays under the limit.
        let worst = 100.0 / (1.0 - 0.0049);
        assert!(worst <= 100.5);
    }

    fn btc() -> (Instrument, &'static Token) {
        let t = token_by_symbol("BTC").unwrap();
        (instrument_for(t), t)
    }

    #[test]
    fn fills_normalise_from_wallet_amounts() {
        let (inst, t) = btc();
        // Buy 10,000 USDC of BTC, 2 bps fee in USDC: 2 USDC on top.
        let r = report_from_amounts("qs1", "sig", &inst, Side::Buy, t, USDC_MINT, (10_002_000_000, 10_000_000_000, 12_500_000, 12_500_000), 0.01);
        assert!((r.filled_qty - 0.125).abs() < 1e-12);
        assert_eq!(r.base_received, 0.125);
        assert!((r.quote_amount - 10_000.0).abs() < 1e-9);
        assert_eq!(r.fee_asset, "USDC");
        assert!((r.fee_amount - 2.0).abs() < 1e-9);
        assert!((r.fee_quote - 2.01).abs() < 1e-9);
        assert!((r.avg_price - 80_000.0).abs() < 1e-6);
        // What the engine books: quote_amount + fee = what left the wallet.
        let mut h = crate::execution::store::Holdings::new();
        h.insert("USDC".into(), 10_002.0);
        crate::execution::engine::apply_fill(&mut h, &inst, Side::Buy, &r);
        assert!(h.get("USDC").copied().unwrap_or(0.0).abs() < 1e-9);
        assert!((h["BTC"] - 0.125).abs() < 1e-12);
        // Sell 0.125 BTC, fee in BTC (the input).
        let r = report_from_amounts("qs2", "sig", &inst, Side::Sell, t, t.mint, (12_500_000, 12_497_500, 9_998_000_000, 9_998_000_000), 0.0);
        assert!((r.filled_qty - 0.124975).abs() < 1e-12);
        assert_eq!(r.fee_asset, "BTC");
        assert!((r.fee_amount - 0.000025).abs() < 1e-12);
        assert!(!r.fee_estimated);
        let mut h = crate::execution::store::Holdings::new();
        h.insert("BTC".into(), 0.125);
        crate::execution::engine::apply_fill(&mut h, &inst, Side::Sell, &r);
        assert!(h.get("BTC").copied().unwrap_or(0.0).abs() < 1e-12, "{h:?}");
        assert!((h["USDC"] - 9_998.0).abs() < 1e-9);
        // Sell with the fee in USDC (the output).
        let r = report_from_amounts("qs3", "sig", &inst, Side::Sell, t, USDC_MINT, (12_500_000, 12_500_000, 10_000_000_000, 9_998_000_000), 0.0);
        assert_eq!(r.fee_asset, "USDC");
        assert!((r.quote_amount - 10_000.0).abs() < 1e-9);
        assert!((r.fee_amount - 2.0).abs() < 1e-9);
        // Nothing came out: not filled.
        let r = report_from_amounts("qs4", "sig", &inst, Side::Buy, t, USDC_MINT, (0, 0, 0, 0), 0.0);
        assert_eq!(r.status, OrderStatus::NotFilled);
    }

    #[test]
    fn the_journal_keeps_the_newest_state_per_order() {
        let dir = std::env::temp_dir().join(format!("qs-jup-journal-{}", uuid::Uuid::new_v4().simple()));
        let j = Journal { path: dir.join("journal.jsonl") };
        let mut rec = SwapRecord {
            client_order_id: "qsabc-01".into(),
            signature: "sig1".into(),
            request_id: "r".into(),
            side: Side::Buy,
            input_mint: USDC_MINT.into(),
            output_mint: SOL_MINT.into(),
            in_amount: 1,
            quoted_out: 1,
            fee_mint: USDC_MINT.into(),
            last_valid_block_height: Some(10),
            network_fee_lamports: 5000,
            state: "signed".into(),
            at: "t".into(),
        };
        j.append(&rec).unwrap();
        rec.state = "sent".into();
        j.append(&rec).unwrap();
        assert_eq!(j.latest("qsabc-01").unwrap().state, "sent");
        assert!(j.latest("qsabc-02").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn units_round_down() {
        assert_eq!(to_units(1.234567891, 6), 1_234_567);
        assert_eq!(to_units(0.1 + 0.2, 1), 3);
        assert_eq!(to_units(-1.0, 6), 0);
        assert_eq!(from_units(1_500_000, 6), 1.5);
    }

    #[test]
    fn a_venue_without_a_key_or_journal_does_not_trade() {
        let v = JupiterVenue::public("", "").unwrap();
        assert!(v.rpc_is_public());
        assert!(v.ready().is_err());
        assert!(v.instrument("BTC/USDC").is_ok());
        assert!(v.instrument("ADA/USDC").is_err());
        assert!(v.instrument("BTC/USDT").is_err());
    }

    // ── live, public quotes only (`cargo test -- --ignored jupiter_live`) ──

    #[test]
    #[ignore]
    fn jupiter_live_quotes() {
        let v = JupiterVenue::public("", "").unwrap();
        for sym in ["BTC", "ETH", "SOL", "XRP"] {
            let inst = v.instrument(&format!("{sym}/USDC")).unwrap();
            let book = v.order_book(&inst).unwrap();
            println!("{sym}: bid {:?} ask {:?}", book.best_bid(), book.best_ask());
            assert!(book.best_bid().unwrap() <= book.best_ask().unwrap() * 1.001);
        }
        println!("fee {:?}", v.fee_schedule(&v.instrument("SOL/USDC").unwrap()).unwrap());
        let prices = v.prices().unwrap();
        println!("prices {prices:?}");
    }
}
