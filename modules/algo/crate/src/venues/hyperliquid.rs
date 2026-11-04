//! Hyperliquid as an [`ExecutionVenue`], spot and perp (PLAN-EXECUTION-VENUES
//! §3, §7, §8, §9; card Venues 2/5). Tauri-free.
//!
//! An **API wallet** signs. Hyperliquid lets such a wallet trade for the main
//! address (or a sub-account of it) but never withdraw or transfer: those are
//! user-signed actions that need the main wallet's key, which QuantSuite never
//! asks for. The account stores the API wallet's private key (vault), the
//! main address and optionally a sub-account address.
//!
//! Signing follows the official Python SDK (`hyperliquid/utils/signing.py`):
//! the action is msgpack-encoded, followed by the nonce and the vault flag,
//! hashed with keccak-256 into a "phantom agent" `connectionId`, which is
//! signed as the EIP-712 struct `Agent(string source,bytes32 connectionId)`
//! (source `a` on mainnet, `b` on testnet). The known-answer tests below
//! hold signatures the SDK produced. Own code over the official Rust SDK:
//! that has had no release since May 2025 and builds on the deprecated
//! ethers-rs.
//!
//! The venue speaks the data source's terms: `BTC/USDC` is the verified Unit
//! token UBTC on spot (by token id, never by name — anyone can create a token
//! called "BTC") and the `BTC` contract on perp. Balances come back the same
//! way.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use k256::ecdsa::SigningKey;
use serde::Serialize;
use serde_json::{json, Value};
use sha3::{Digest, Keccak256};

use crate::broker::SymbolFilters;
use crate::execution::mapping::{self, MappingTable, VerifiedList};
use crate::execution::venue::{
    Capabilities, ExecutionVenue, FeeSchedule, Instrument, MarketType, OrderBook, OrderReport, OrderRequest,
    OrderStatus, Side, VenueBalance, VenuePosition,
};

pub const MAINNET: &str = "https://api.hyperliquid.xyz";
pub const TESTNET: &str = "https://api.hyperliquid-testnet.xyz";
/// Hyperliquid's minimum order value, in USDC.
pub const MIN_ORDER_VALUE: f64 = 10.0;
/// The entry-tier taker fees (Sources in the plan, checked 2026-11-03).
pub const SPOT_TAKER: f64 = 0.0007;
pub const SPOT_MAKER: f64 = 0.0004;
pub const PERP_TAKER: f64 = 0.00045;
pub const PERP_MAKER: f64 = 0.00015;
/// Spot prices carry at most 8 decimals minus the token's size decimals,
/// perp prices 6.
const SPOT_MAX_DECIMALS: i32 = 8;
const PERP_MAX_DECIMALS: i32 = 6;
/// The first two bytes of every client order id QuantSuite sends: "qs".
pub const CLOID_MARKER: &str = "0x7173";
/// How an open order of ours reads in `open_orders` — the prefix the kill
/// switch looks for (`unattended::ours`).
const OURS_PREFIX: &str = "qs#";
const META_TTL: Duration = Duration::from_secs(15 * 60);
/// Warn this many days before an API wallet expires.
pub const EXPIRY_WARNING_DAYS: i64 = 7;

fn num(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())).unwrap_or(0.0)
}

fn keccak(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

// ── msgpack ─────────────────────────────────────────────────────────────────

/// The part of msgpack an action needs, encoded the way Python's
/// `msgpack.packb` does (smallest integer and string forms, `str8` for
/// 32–255 bytes). Map keys keep their order: the hash depends on it.
#[derive(Debug, Clone, PartialEq)]
pub enum Mp {
    Bool(bool),
    UInt(u64),
    Str(String),
    Arr(Vec<Mp>),
    Map(Vec<(&'static str, Mp)>),
}

impl Mp {
    pub fn pack(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Mp::Bool(b) => out.push(if *b { 0xc3 } else { 0xc2 }),
            Mp::UInt(n) => write_uint(*n, out),
            Mp::Str(s) => write_str(s, out),
            Mp::Arr(items) => {
                write_len(items.len(), 0x90, 0xdc, 0xdd, out);
                for item in items {
                    item.write(out);
                }
            }
            Mp::Map(entries) => {
                write_len(entries.len(), 0x80, 0xde, 0xdf, out);
                for (k, v) in entries {
                    write_str(k, out);
                    v.write(out);
                }
            }
        }
    }

    /// The same tree as the JSON the exchange endpoint receives.
    pub fn to_json(&self) -> Value {
        match self {
            Mp::Bool(b) => json!(b),
            Mp::UInt(n) => json!(n),
            Mp::Str(s) => json!(s),
            Mp::Arr(items) => Value::Array(items.iter().map(Mp::to_json).collect()),
            Mp::Map(entries) => Value::Object(entries.iter().map(|(k, v)| (k.to_string(), v.to_json())).collect()),
        }
    }
}

fn write_uint(n: u64, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n <= 0xff {
        out.extend([0xcc, n as u8]);
    } else if n <= 0xffff {
        out.push(0xcd);
        out.extend((n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        out.push(0xce);
        out.extend((n as u32).to_be_bytes());
    } else {
        out.push(0xcf);
        out.extend(n.to_be_bytes());
    }
}

fn write_str(s: &str, out: &mut Vec<u8>) {
    let b = s.as_bytes();
    let n = b.len();
    if n < 32 {
        out.push(0xa0 | n as u8);
    } else if n <= 0xff {
        out.extend([0xd9, n as u8]);
    } else if n <= 0xffff {
        out.push(0xda);
        out.extend((n as u16).to_be_bytes());
    } else {
        out.push(0xdb);
        out.extend((n as u32).to_be_bytes());
    }
    out.extend_from_slice(b);
}

fn write_len(n: usize, fix: u8, l16: u8, l32: u8, out: &mut Vec<u8>) {
    if n < 16 {
        out.push(fix | n as u8);
    } else if n <= 0xffff {
        out.push(l16);
        out.extend((n as u16).to_be_bytes());
    } else {
        out.push(l32);
        out.extend((n as u32).to_be_bytes());
    }
}

// ── wire numbers and ids ────────────────────────────────────────────────────

/// A price or size as the SDK's `float_to_wire` writes it: at most 8
/// decimals, trailing zeros dropped. A value that 8 decimals would change is
/// refused, never silently rounded.
pub fn float_to_wire(x: f64) -> Result<String, String> {
    let rounded = format!("{x:.8}");
    let back: f64 = rounded.parse().map_err(|_| format!("not a number: {x}"))?;
    if (back - x).abs() >= 1e-12 {
        return Err(format!("{x} has more than 8 decimals"));
    }
    let mut s = rounded;
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    Ok(s)
}

/// A limit price Hyperliquid accepts: at most 5 significant figures (an
/// integer always passes) and at most `max decimals − szDecimals` decimals,
/// rounded inside the cap — down for a buy, up for a sell.
pub fn round_px(px: f64, sz_decimals: u32, market: MarketType, side: Side) -> f64 {
    if px <= 0.0 || !px.is_finite() {
        return px;
    }
    let max_dec = match market {
        MarketType::Spot => SPOT_MAX_DECIMALS,
        MarketType::Perp => PERP_MAX_DECIMALS,
    } - sz_decimals as i32;
    let magnitude = px.log10().floor() as i32;
    let decimals = (4 - magnitude).min(max_dec).max(0);
    let step = 10f64.powi(-decimals);
    let steps = px / step;
    let steps = match side {
        Side::Buy => (steps + 1e-9).floor(),
        Side::Sell => (steps - 1e-9).ceil(),
    };
    let d = decimals as usize;
    format!("{:.d$}", steps * step).parse().unwrap_or(steps * step)
}

/// The 16-byte client order id Hyperliquid carries for one of ours:
/// `0x7173` ("qs") and 14 bytes of keccak of our id. The same id always maps
/// to the same cloid, so a resumed run finds the order it sent.
pub fn cloid_for(client_order_id: &str) -> String {
    if is_cloid(client_order_id) {
        return client_order_id.to_lowercase();
    }
    let h = keccak(client_order_id.as_bytes());
    format!("{CLOID_MARKER}{}", hex::encode(&h[..14]))
}

fn is_cloid(s: &str) -> bool {
    s.len() == 34 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// A cloid QuantSuite sent (its marker), as opposed to the user's own orders.
pub fn is_ours(cloid: &str) -> bool {
    is_cloid(cloid) && cloid.to_lowercase().starts_with(CLOID_MARKER)
}

fn address_bytes(address: &str) -> Result<[u8; 20], String> {
    let hex_part = address.strip_prefix("0x").unwrap_or(address);
    let bytes = hex::decode(hex_part).map_err(|_| format!("{address} is not an address"))?;
    bytes.try_into().map_err(|_| format!("{address} is not a 20-byte address"))
}

/// A checked, lower-case `0x…` address.
pub fn normalize_address(address: &str) -> Result<String, String> {
    let a = address.trim();
    address_bytes(a)?;
    Ok(format!("0x{}", a.strip_prefix("0x").unwrap_or(a).to_lowercase()))
}

// ── actions ─────────────────────────────────────────────────────────────────

/// One order of an `order` action, in the SDK's field order.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderWire {
    pub asset: u32,
    pub is_buy: bool,
    pub px: String,
    pub sz: String,
    pub reduce_only: bool,
    /// `Ioc` — the engine's order type.
    pub tif: &'static str,
    pub cloid: Option<String>,
}

impl OrderWire {
    fn to_mp(&self) -> Mp {
        let mut m = vec![
            ("a", Mp::UInt(self.asset as u64)),
            ("b", Mp::Bool(self.is_buy)),
            ("p", Mp::Str(self.px.clone())),
            ("s", Mp::Str(self.sz.clone())),
            ("r", Mp::Bool(self.reduce_only)),
            ("t", Mp::Map(vec![("limit", Mp::Map(vec![("tif", Mp::Str(self.tif.into()))]))])),
        ];
        if let Some(c) = &self.cloid {
            m.push(("c", Mp::Str(c.clone())));
        }
        Mp::Map(m)
    }
}

pub fn order_action(orders: &[OrderWire]) -> Mp {
    Mp::Map(vec![
        ("type", Mp::Str("order".into())),
        ("orders", Mp::Arr(orders.iter().map(OrderWire::to_mp).collect())),
        ("grouping", Mp::Str("na".into())),
    ])
}

pub fn cancel_by_cloid_action(asset: u32, cloid: &str) -> Mp {
    Mp::Map(vec![
        ("type", Mp::Str("cancelByCloid".into())),
        ("cancels", Mp::Arr(vec![Mp::Map(vec![("asset", Mp::UInt(asset as u64)), ("cloid", Mp::Str(cloid.into()))])])),
    ])
}

pub fn update_leverage_action(asset: u32, is_cross: bool, leverage: u32) -> Mp {
    Mp::Map(vec![
        ("type", Mp::Str("updateLeverage".into())),
        ("asset", Mp::UInt(asset as u64)),
        ("isCross", Mp::Bool(is_cross)),
        ("leverage", Mp::UInt(leverage as u64)),
    ])
}

// ── signing ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Signature {
    pub r: String,
    pub s: String,
    pub v: u8,
}

/// keccak(msgpack(action) ‖ nonce ‖ vault flag [‖ vault] [‖ 0 ‖ expiresAfter]).
pub fn action_hash(action: &Mp, vault: Option<&str>, nonce: u64, expires_after: Option<u64>) -> Result<[u8; 32], String> {
    let mut data = action.pack();
    data.extend(nonce.to_be_bytes());
    match vault {
        None => data.push(0),
        Some(a) => {
            data.push(1);
            data.extend(address_bytes(a)?);
        }
    }
    if let Some(e) = expires_after {
        data.push(0);
        data.extend(e.to_be_bytes());
    }
    Ok(keccak(&data))
}

/// The EIP-712 digest of `Agent{source, connectionId}` in Hyperliquid's
/// `Exchange` domain (chain id 1337, zero verifying contract).
pub fn agent_digest(connection_id: &[u8; 32], mainnet: bool) -> [u8; 32] {
    let mut domain = Vec::with_capacity(160);
    domain.extend(keccak(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"));
    domain.extend(keccak(b"Exchange"));
    domain.extend(keccak(b"1"));
    let mut chain = [0u8; 32];
    chain[24..].copy_from_slice(&1337u64.to_be_bytes());
    domain.extend(chain);
    domain.extend([0u8; 32]);
    let mut agent = Vec::with_capacity(96);
    agent.extend(keccak(b"Agent(string source,bytes32 connectionId)"));
    agent.extend(keccak(if mainnet { b"a" } else { b"b" }));
    agent.extend(connection_id);
    let mut msg = vec![0x19, 0x01];
    msg.extend(keccak(&domain));
    msg.extend(keccak(&agent));
    keccak(&msg)
}

/// `0x` and the value without leading zeros — what the SDK's `to_hex` sends.
fn hex_value(bytes: &[u8]) -> String {
    let h = hex::encode(bytes);
    let t = h.trim_start_matches('0');
    format!("0x{}", if t.is_empty() { "0" } else { t })
}

/// An L1 action signed by the API wallet.
pub fn sign_l1(
    key: &SigningKey,
    action: &Mp,
    vault: Option<&str>,
    nonce: u64,
    expires_after: Option<u64>,
    mainnet: bool,
) -> Result<Signature, String> {
    let hash = action_hash(action, vault, nonce, expires_after)?;
    let digest = agent_digest(&hash, mainnet);
    let (sig, recid) = key.sign_prehash_recoverable(&digest);
    let (r, s) = sig.split_bytes();
    Ok(Signature { r: hex_value(&r), s: hex_value(&s), v: 27 + recid.to_byte() })
}

/// The API wallet's key from its hex form.
pub fn parse_key(hex_key: &str) -> Result<SigningKey, String> {
    let k = hex_key.trim();
    let mut bytes = hex::decode(k.strip_prefix("0x").unwrap_or(k))
        .map_err(|_| "The API wallet's private key is not hex.".to_string())?;
    let key = SigningKey::from_slice(&bytes).map_err(|_| "The API wallet's private key is not a valid key.".to_string());
    zeroize::Zeroize::zeroize(&mut bytes);
    key
}

/// The address a key signs as.
pub fn address_of(key: &SigningKey) -> String {
    let point = key.verifying_key().to_sec1_point(false);
    let h = keccak(&point.as_bytes()[1..]);
    format!("0x{}", hex::encode(&h[12..]))
}

/// Hyperliquid keeps the 100 highest nonces per signer; a new one must be
/// larger than the smallest of them and within (now − 2 days, now + 1 day).
/// Milliseconds, strictly increasing per API wallet in this process — two
/// orders in the same millisecond never share one.
pub fn next_nonce(signer: &str, now_ms: u64) -> u64 {
    static LAST: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let mut map = LAST.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    let last = map.entry(signer.to_lowercase()).or_insert(0);
    let n = now_ms.max(*last + 1);
    *last = n;
    n
}

// ── verified tokens ─────────────────────────────────────────────────────────

/// Spot tokens QuantSuite trades as a data coin, by token id per network:
/// (data symbol, token name, token id, custodian). Checked against
/// `spotMeta` on 2026-11-04. Anything else — a look-alike called "BTC"
/// included — is not mapped.
const MAINNET_SPOT: &[(&str, &str, &str, Option<&str>)] = &[
    ("BTC", "UBTC", "0x8f254b963e8468305d409b33aa137c67", Some("Unit")),
    ("ETH", "UETH", "0xe1edd30daaf5caac3fe63569e24748da", Some("Unit")),
    ("SOL", "USOL", "0x49b67c39f5566535de22b29b0e51e685", Some("Unit")),
    ("HYPE", "HYPE", "0x0d01dc56dcaaca66ad901c959b4011ec", None),
    ("DOGE", "UDOGE", "0xd5e0a2d2aa8a05e73e4ef1caeea2f62a", Some("Unit")),
    ("AVAX", "UAVAX", "0x730fc3855fb77d2aa5a19dd7891dbe80", Some("Unit")),
    ("ENA", "UENA", "0x593494b6af79172fa983a0cf1c88e0e0", Some("Unit")),
    ("PUMP", "UPUMP", "0x544e60f98a36d7b22c0fb5824b84f795", Some("Unit")),
    ("FARTCOIN", "UFART", "0x7650808198966e4285687d3deb556ccc", Some("Unit")),
    ("BONK", "UBONK", "0xb113d34e351cf195733c98442530c099", Some("Unit")),
    ("WLD", "UWLD", "0x571aa6d5261fb8f28e21a9cdb2e1c646", Some("Unit")),
    ("XPL", "UXPL", "0x2c54c60600e1d786b2dfc139a38a5a99", Some("Unit")),
    ("MON", "UMON", "0x58dae745c8c5fed4012f35ef39829c2d", Some("Unit")),
    ("ZEC", "UZEC", "0x1c994ad3381d31c86c8c2d74ed89a365", Some("Unit")),
    ("VIRTUAL", "UVIRT", "0x85b8124314ae77b78b4b6f20ecd93149", Some("Unit")),
];

/// The testnet's own token ids (testnet tokens are play money; several carry
/// copied names, so the ids matter there too). No Unit Bitcoin on testnet.
const TESTNET_SPOT: &[(&str, &str, &str, Option<&str>)] = &[
    ("ETH", "UETH", "0xe4371d8166f362d6578725f11e0a14f3", Some("Unit (testnet)")),
    ("SOL", "USOL", "0x57ead23624b114018cc0e49d01cc7b6b", Some("Unit (testnet)")),
    ("HYPE", "HYPE", "0x7317beb7cceed72ef0b346074cc8e7ab", None),
    ("FARTCOIN", "UFART", "0x5c1a98b4df03401e19acb16bcf2ffabf", Some("Unit (testnet)")),
    ("PUMP", "UPUMP", "0xdc348378290f167692e50bfb49c60696", Some("Unit (testnet)")),
    ("ENA", "UENA", "0xb54d72e8a850b91b4250a1c92c1d19b5", Some("Unit (testnet)")),
];

fn spot_list(testnet: bool) -> &'static [(&'static str, &'static str, &'static str, Option<&'static str>)] {
    if testnet {
        TESTNET_SPOT
    } else {
        MAINNET_SPOT
    }
}

/// The verified list in the mapping's form: (symbol, token id, custodian).
pub fn verified_spot(testnet: bool) -> VerifiedList {
    VerifiedList(spot_list(testnet).iter().map(|(s, _, id, c)| (*s, *id, *c)).collect())
}

// ── metadata ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PerpAsset {
    pub index: u32,
    pub name: String,
    pub sz_decimals: u32,
    pub max_leverage: u32,
    pub only_isolated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub index: u32,
    pub name: String,
    pub token_id: String,
    pub sz_decimals: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpotPair {
    pub index: u32,
    /// `@142`, or `PURR/USDC` for the canonical pairs — the `coin` of the
    /// info calls.
    pub name: String,
    pub base: u32,
    pub quote: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub perps: Vec<PerpAsset>,
    pub tokens: Vec<Token>,
    pub spot: Vec<SpotPair>,
}

impl Meta {
    pub fn parse(meta: &Value, spot_meta: &Value) -> Meta {
        let perps = meta
            .get("universe")
            .and_then(|v| v.as_array())
            .map(|u| {
                u.iter()
                    .enumerate()
                    .filter(|(_, a)| !a.get("isDelisted").and_then(|v| v.as_bool()).unwrap_or(false))
                    .map(|(i, a)| PerpAsset {
                        index: i as u32,
                        name: a["name"].as_str().unwrap_or("").to_string(),
                        sz_decimals: a["szDecimals"].as_u64().unwrap_or(0) as u32,
                        max_leverage: a["maxLeverage"].as_u64().unwrap_or(1) as u32,
                        only_isolated: a.get("onlyIsolated").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let tokens = spot_meta
            .get("tokens")
            .and_then(|v| v.as_array())
            .map(|t| {
                t.iter()
                    .map(|t| Token {
                        index: t["index"].as_u64().unwrap_or(0) as u32,
                        name: t["name"].as_str().unwrap_or("").to_string(),
                        token_id: t["tokenId"].as_str().unwrap_or("").to_lowercase(),
                        sz_decimals: t["szDecimals"].as_u64().unwrap_or(0) as u32,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let spot = spot_meta
            .get("universe")
            .and_then(|v| v.as_array())
            .map(|u| {
                u.iter()
                    .filter_map(|p| {
                        let toks = p.get("tokens")?.as_array()?;
                        Some(SpotPair {
                            index: p["index"].as_u64()? as u32,
                            name: p["name"].as_str()?.to_string(),
                            base: toks.first()?.as_u64()? as u32,
                            quote: toks.get(1)?.as_u64()? as u32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Meta { perps, tokens, spot }
    }

    fn token(&self, index: u32) -> Option<&Token> {
        self.tokens.iter().find(|t| t.index == index)
    }

    /// USDC, the quote of every spot pair QuantSuite trades (token 0).
    fn usdc(&self) -> Option<&Token> {
        self.tokens.iter().find(|t| t.index == 0 && t.name == "USDC")
    }

    /// The data symbol a token stands for, when it is on the verified list.
    fn symbol_of(&self, token: &Token, testnet: bool) -> Option<&'static str> {
        if Some(token) == self.usdc() {
            return Some("USDC");
        }
        spot_list(testnet).iter().find(|(_, _, id, _)| id.eq_ignore_ascii_case(&token.token_id)).map(|(s, ..)| *s)
    }

    /// Every spot pair with its ids as base and quote (USDC as `USDC`), for
    /// the mapping table and its look-alike check.
    pub fn raw_spot_instruments(&self) -> Vec<(Instrument, String)> {
        let usdc = self.usdc().map(|t| t.index);
        self.spot
            .iter()
            .filter_map(|p| {
                let base = self.token(p.base)?;
                let quote = self.token(p.quote)?;
                let quote_id = if Some(quote.index) == usdc { "USDC".to_string() } else { quote.token_id.clone() };
                let inst = spot_instrument(p, base, &base.token_id, &quote_id);
                Some((inst, base.name.clone()))
            })
            .collect()
    }

    /// The verified spot pairs in the data source's terms (`BTC/USDC`).
    pub fn spot_instruments(&self, testnet: bool) -> Vec<Instrument> {
        self.spot
            .iter()
            .filter_map(|p| {
                let base = self.token(p.base)?;
                let quote = self.token(p.quote)?;
                let b = self.symbol_of(base, testnet).filter(|s| *s != "USDC")?;
                let q = self.symbol_of(quote, testnet)?;
                Some(spot_instrument(p, base, b, q))
            })
            .collect()
    }

    pub fn perp_instruments(&self) -> Vec<Instrument> {
        self.perps
            .iter()
            .map(|a| Instrument {
                market: MarketType::Perp,
                filters: SymbolFilters {
                    symbol: a.name.clone(),
                    base: a.name.clone(),
                    quote: "USDC".into(),
                    qty_step: 10f64.powi(-(a.sz_decimals as i32)),
                    min_qty: 10f64.powi(-(a.sz_decimals as i32)),
                    min_notional: MIN_ORDER_VALUE,
                    price_tick: 10f64.powi(-(PERP_MAX_DECIMALS - a.sz_decimals as i32).max(0)),
                },
            })
            .collect()
    }

    /// The asset number an order carries: the perp's index, or 10000 + the
    /// spot pair's index.
    pub fn asset_of(&self, inst: &Instrument) -> Result<(u32, u32), String> {
        match inst.market {
            MarketType::Perp => self
                .perps
                .iter()
                .find(|a| a.name == inst.filters.symbol)
                .map(|a| (a.index, a.sz_decimals))
                .ok_or_else(|| format!("Hyperliquid has no perp {}", inst.filters.symbol)),
            MarketType::Spot => self
                .spot
                .iter()
                .find(|p| p.name == inst.filters.symbol)
                .map(|p| (10_000 + p.index, self.token(p.base).map(|t| t.sz_decimals).unwrap_or(0)))
                .ok_or_else(|| format!("Hyperliquid has no spot pair {}", inst.filters.symbol)),
        }
    }

    /// The token name behind a spot instrument's base (`UBTC`).
    fn base_token_name(&self, inst: &Instrument) -> Option<String> {
        let p = self.spot.iter().find(|p| p.name == inst.filters.symbol)?;
        self.token(p.base).map(|t| t.name.clone())
    }
}

fn spot_instrument(p: &SpotPair, base: &Token, base_name: &str, quote_name: &str) -> Instrument {
    Instrument::spot(SymbolFilters {
        symbol: p.name.clone(),
        base: base_name.to_string(),
        quote: quote_name.to_string(),
        qty_step: 10f64.powi(-(base.sz_decimals as i32)),
        min_qty: 10f64.powi(-(base.sz_decimals as i32)),
        min_notional: MIN_ORDER_VALUE,
        price_tick: 10f64.powi(-(SPOT_MAX_DECIMALS - base.sz_decimals as i32).max(0)),
    })
}

type MetaCache = Mutex<HashMap<bool, (Instant, Meta)>>;

fn meta_cache() -> &'static MetaCache {
    static CACHE: OnceLock<MetaCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── fills ───────────────────────────────────────────────────────────────────

/// What the fills of one order add up to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FillSum {
    pub qty: f64,
    pub quote: f64,
    pub fee: f64,
    pub fee_token: String,
}

/// Sum the `userFills` rows of order `oid`.
pub fn sum_fills(fills: &Value, oid: u64) -> Option<FillSum> {
    let mut sum = FillSum::default();
    for f in fills.as_array()?.iter().filter(|f| f["oid"].as_u64() == Some(oid)) {
        let (px, sz) = (num(&f["px"]), num(&f["sz"]));
        sum.qty += sz;
        sum.quote += px * sz;
        sum.fee += num(&f["fee"]);
        if sum.fee_token.is_empty() {
            sum.fee_token = f["feeToken"].as_str().unwrap_or("").trim().to_string();
        }
    }
    (sum.qty > 0.0).then_some(sum)
}

/// An order's report from what it executed. A fee in the bought token (a
/// spot buy) is netted out of the base received.
#[allow(clippy::too_many_arguments)]
pub fn report(
    client_order_id: &str,
    oid: u64,
    status: OrderStatus,
    inst: &Instrument,
    base_token: Option<&str>,
    side: Side,
    fills: Option<FillSum>,
    fallback: (f64, f64),
    fee_rate: f64,
    message: Option<String>,
) -> OrderReport {
    let (qty, avg) = match &fills {
        Some(f) => (f.qty, f.quote / f.qty),
        None => fallback,
    };
    let quote_amount = qty * avg;
    let (fee_asset, fee_amount, fee_estimated) = match &fills {
        Some(f) => {
            let token = f.fee_token.as_str();
            let asset = if token.eq_ignore_ascii_case("USDC") {
                inst.quote().to_string()
            } else if base_token.is_some_and(|b| b.eq_ignore_ascii_case(token)) || token.eq_ignore_ascii_case(inst.base()) {
                inst.base().to_string()
            } else {
                token.to_string()
            };
            (asset, f.fee, false)
        }
        None => (inst.quote().to_string(), quote_amount * fee_rate, true),
    };
    let fee_in_base = fee_asset.eq_ignore_ascii_case(inst.base()) && inst.market == MarketType::Spot;
    let base_received = if side == Side::Buy && fee_in_base { (qty - fee_amount).max(0.0) } else { qty };
    let fee_quote = if fee_in_base { fee_amount * avg } else { fee_amount };
    OrderReport {
        client_order_id: client_order_id.to_string(),
        venue_order_id: if oid > 0 { oid.to_string() } else { String::new() },
        status,
        filled_qty: qty,
        base_received,
        avg_price: avg,
        quote_amount,
        fee_asset,
        fee_amount,
        fee_quote,
        fee_estimated,
        message,
    }
}

/// An `orderStatus` status string, given what executed.
pub fn status_of(status: &str, filled: f64, orig: f64) -> OrderStatus {
    match status {
        "filled" => OrderStatus::Filled,
        "open" | "triggered" => OrderStatus::Open,
        _ if filled > 0.0 && filled + 1e-12 < orig => OrderStatus::PartiallyFilled,
        _ if filled > 0.0 => OrderStatus::Filled,
        "iocCancelRejected" | "canceled" | "marginCanceled" | "reduceOnlyCanceled" | "selfTradeCanceled"
        | "scheduledCancel" | "liquidatedCanceled" | "delistedCanceled" | "openInterestCapCanceled" => OrderStatus::NotFilled,
        s if s.ends_with("Rejected") => OrderStatus::Rejected,
        _ => OrderStatus::NotFilled,
    }
}

// ── the venue ───────────────────────────────────────────────────────────────

/// An API wallet as Hyperliquid knows it (`extraAgents`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentStatus {
    pub address: String,
    /// The main account has approved this API wallet.
    pub approved: bool,
    pub name: Option<String>,
    /// Milliseconds since the epoch, when the approval carries an expiry.
    pub valid_until: Option<i64>,
    pub days_left: Option<i64>,
    /// Expires within [`EXPIRY_WARNING_DAYS`].
    pub expiring: bool,
}

pub struct HyperliquidVenue {
    market: MarketType,
    testnet: bool,
    /// The main address.
    main: String,
    /// A sub-account the orders act on (`vaultAddress`), when set.
    sub_account: Option<String>,
    key: Option<SigningKey>,
    agent: String,
    client: reqwest::blocking::Client,
    /// Assets whose leverage this venue already set (isolated).
    leverage_set: Mutex<HashSet<u32>>,
    /// The leverage an opening perp order sets first (1x default; the cap
    /// is checked by the caller, Venues 4/5).
    leverage: u32,
}

/// One funding payment of the account (`userFunding`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FundingPayment {
    pub time: i64,
    pub coin: String,
    /// What the account received (negative: paid).
    pub usdc: f64,
    /// The signed position size it was paid on.
    pub szi: f64,
    /// The hourly rate.
    pub rate: f64,
    pub hash: String,
}

pub fn parse_funding(rows: &Value) -> Vec<FundingPayment> {
    rows.as_array()
        .into_iter()
        .flatten()
        .filter(|r| r.pointer("/delta/type").and_then(|t| t.as_str()) == Some("funding"))
        .map(|r| FundingPayment {
            time: r["time"].as_i64().unwrap_or(0),
            coin: r.pointer("/delta/coin").and_then(|c| c.as_str()).unwrap_or("").to_string(),
            usdc: num(&r.pointer("/delta/usdc").cloned().unwrap_or_default()),
            szi: num(&r.pointer("/delta/szi").cloned().unwrap_or_default()),
            rate: num(&r.pointer("/delta/fundingRate").cloned().unwrap_or_default()),
            hash: r["hash"].as_str().unwrap_or("").to_string(),
        })
        .collect()
}

impl std::fmt::Debug for HyperliquidVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HyperliquidVenue")
            .field("market", &self.market)
            .field("testnet", &self.testnet)
            .field("main", &self.main)
            .field("sub_account", &self.sub_account)
            .field("agent", &self.agent)
            .finish()
    }
}

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))
}

impl HyperliquidVenue {
    /// The public side only: instruments, books, candles — no account.
    pub fn public(market: MarketType, testnet: bool) -> Result<Self, String> {
        Ok(Self {
            market,
            testnet,
            main: String::new(),
            sub_account: None,
            key: None,
            agent: String::new(),
            client: http()?,
            leverage_set: Mutex::new(HashSet::new()),
            leverage: 1,
        })
    }

    /// An account: the main address, an optional sub-account and the API
    /// wallet's key. Any of them may be blank for a read-only use; trading
    /// needs all but the sub-account.
    pub fn new(market: MarketType, testnet: bool, main: &str, sub_account: &str, api_wallet_key: &str) -> Result<Self, String> {
        let mut v = Self::public(market, testnet)?;
        if !main.trim().is_empty() {
            v.main = normalize_address(main).map_err(|e| format!("Main address: {e}"))?;
        }
        if !sub_account.trim().is_empty() {
            v.sub_account = Some(normalize_address(sub_account).map_err(|e| format!("Sub-account: {e}"))?);
        }
        if !api_wallet_key.trim().is_empty() {
            let key = parse_key(api_wallet_key)?;
            v.agent = address_of(&key);
            v.key = Some(key);
        }
        Ok(v)
    }

    /// The leverage opening perp orders set (isolated).
    pub fn with_leverage(mut self, leverage: u32) -> Self {
        self.leverage = leverage.max(1);
        self
    }

    /// The account's funding payments since `start_ms`.
    pub fn funding_since(&self, start_ms: i64) -> Result<Vec<FundingPayment>, String> {
        Ok(parse_funding(&self.info(&json!({"type": "userFunding", "user": self.user()?, "startTime": start_ms}))?))
    }

    fn base(&self) -> &'static str {
        if self.testnet {
            TESTNET
        } else {
            MAINNET
        }
    }

    /// Whose balances and orders: the sub-account when set, else the main.
    fn user(&self) -> Result<&str, String> {
        let u = self.sub_account.as_deref().unwrap_or(&self.main);
        if u.is_empty() {
            return Err("The Hyperliquid account has no main address stored.".into());
        }
        Ok(u)
    }

    pub fn info(&self, body: &Value) -> Result<Value, String> {
        let resp = self
            .client
            .post(format!("{}/info", self.base()))
            .json(body)
            .send()
            .map_err(|e| format!("Hyperliquid info {}: {e}", body["type"]))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("Hyperliquid info {}: {e}", body["type"]))?;
        if !status.is_success() {
            return Err(format!("Hyperliquid info {}: HTTP {status}: {}", body["type"], text.chars().take(200).collect::<String>()));
        }
        serde_json::from_str(&text).map_err(|e| format!("Hyperliquid info {} parse: {e}", body["type"]))
    }

    pub fn meta(&self) -> Result<Meta, String> {
        if let Ok(cache) = meta_cache().lock() {
            if let Some((at, m)) = cache.get(&self.testnet) {
                if at.elapsed() < META_TTL {
                    return Ok(m.clone());
                }
            }
        }
        let meta = Meta::parse(&self.info(&json!({"type": "meta"}))?, &self.info(&json!({"type": "spotMeta"}))?);
        if meta.perps.is_empty() && meta.spot.is_empty() {
            return Err("Hyperliquid returned no markets.".into());
        }
        if let Ok(mut cache) = meta_cache().lock() {
            cache.insert(self.testnet, (Instant::now(), meta.clone()));
        }
        Ok(meta)
    }

    /// The coverage of `symbols` on this venue's market, by verified id.
    pub fn mapping(&self, symbols: &[String]) -> Result<MappingTable, String> {
        let meta = self.meta()?;
        let label = self.label();
        Ok(match self.market {
            MarketType::Spot => {
                let raw = meta.raw_spot_instruments();
                let names: HashMap<String, String> = raw.iter().map(|(i, n)| (i.filters.symbol.clone(), n.clone())).collect();
                let instruments: Vec<Instrument> = raw.into_iter().map(|(i, _)| i).collect();
                let display = |i: &Instrument| names.get(&i.filters.symbol).cloned().unwrap_or_else(|| i.base().to_string());
                let mut table = mapping::map_coins(&label, symbols, &instruments, &verified_spot(self.testnet), &display);
                // Show the token's name next to its id.
                for m in &mut table.mapped {
                    if let Some(name) = names.get(&m.instrument) {
                        m.instrument = format!("{name}/USDC ({})", m.instrument);
                    }
                }
                table
            }
            MarketType::Perp => {
                // Perp contracts are listed by the protocol itself (the first
                // perp dex), one per name — like a CEX's asset code.
                let spot_like: Vec<Instrument> =
                    meta.perp_instruments().into_iter().map(|i| Instrument { market: MarketType::Spot, ..i }).collect();
                mapping::map_coins(&label, symbols, &spot_like, &mapping::ExchangeCodes, &|i: &Instrument| i.base().to_string())
            }
        })
    }

    fn signer(&self) -> Result<&SigningKey, String> {
        self.key.as_ref().ok_or_else(|| "The Hyperliquid account has no API wallet key stored; trading needs it.".to_string())
    }

    /// Sign and send an action. `Err` only when the answer is unknown (the
    /// network); a refusal comes back as `Ok` with `status: err`.
    fn exchange(&self, action: &Mp) -> Result<Value, String> {
        let key = self.signer()?;
        let nonce = next_nonce(&self.agent, chrono::Utc::now().timestamp_millis().max(0) as u64);
        let sig = sign_l1(key, action, self.sub_account.as_deref(), nonce, None, !self.testnet)?;
        let body = json!({
            "action": action.to_json(),
            "nonce": nonce,
            "signature": sig,
            "vaultAddress": self.sub_account,
            "expiresAfter": Value::Null,
        });
        let resp = self
            .client
            .post(format!("{}/exchange", self.base()))
            .json(&body)
            .send()
            .map_err(|e| format!("Hyperliquid exchange: {e}"))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| format!("Hyperliquid exchange: {e}"))?;
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => Ok(v),
            Err(_) if status.is_server_error() => Err(format!("Hyperliquid exchange: HTTP {status}")),
            Err(_) => Ok(json!({"status": "err", "response": format!("HTTP {status}: {}", text.chars().take(200).collect::<String>())})),
        }
    }

    /// Isolated margin at the venue's leverage before the first opening
    /// order on a perp asset (1x by default).
    fn ensure_isolated(&self, asset: u32) -> Result<(), String> {
        if self.leverage_set.lock().map(|s| s.contains(&asset)).unwrap_or(false) {
            return Ok(());
        }
        let resp = self.exchange(&update_leverage_action(asset, false, self.leverage))?;
        if resp["status"].as_str() != Some("ok") {
            return Err(format!("setting {}x isolated failed: {}", self.leverage, resp["response"]));
        }
        if let Ok(mut s) = self.leverage_set.lock() {
            s.insert(asset);
        }
        Ok(())
    }

    fn fills_since(&self, start_ms: i64) -> Result<Value, String> {
        self.info(&json!({"type": "userFillsByTime", "user": self.user()?, "startTime": start_ms, "aggregateByTime": false}))
    }

    fn fee_rate(&self, inst: &Instrument) -> f64 {
        self.fee_schedule(inst).map(|f| f.taker).unwrap_or(match inst.market {
            MarketType::Spot => SPOT_TAKER,
            MarketType::Perp => PERP_TAKER,
        })
    }

    /// The API wallet as the main account approved it (`extraAgents`).
    pub fn agent_status(&self) -> Result<AgentStatus, String> {
        if self.agent.is_empty() || self.main.is_empty() {
            return Err("Store the main address and the API wallet key first.".into());
        }
        let agents = self.info(&json!({"type": "extraAgents", "user": self.main}))?;
        let found = agents
            .as_array()
            .and_then(|a| a.iter().find(|x| x["address"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&self.agent))));
        let valid_until = found.and_then(|x| x["validUntil"].as_i64()).filter(|v| *v > 0);
        Ok(agent_status_from(&self.agent, found.is_some(), found.and_then(|x| x["name"].as_str()), valid_until, chrono::Utc::now().timestamp_millis()))
    }

    /// The closed daily candle of `day` (UTC) for an instrument.
    pub fn daily_close_of(&self, inst: &Instrument, day: NaiveDate) -> Option<f64> {
        let start = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis();
        let rows = self
            .info(&json!({"type": "candleSnapshot", "req": {"coin": inst.filters.symbol, "interval": "1d", "startTime": start, "endTime": start + 86_400_000 - 1}}))
            .ok()?;
        rows.as_array()?
            .iter()
            .find(|c| c["t"].as_i64() == Some(start))
            .map(|c| num(&c["c"]))
            .filter(|c| *c > 0.0)
    }

    /// The hourly funding rate of a perp (`metaAndAssetCtxs`).
    pub fn funding_rate_hourly(&self, coin: &str) -> Result<f64, String> {
        let resp = self.info(&json!({"type": "metaAndAssetCtxs"}))?;
        let universe = resp.pointer("/0/universe").and_then(|v| v.as_array()).ok_or("no perp universe")?;
        let ctxs = resp.get(1).and_then(|v| v.as_array()).ok_or("no asset contexts")?;
        let i = universe.iter().position(|a| a["name"].as_str() == Some(coin)).ok_or_else(|| format!("no perp {coin}"))?;
        Ok(num(&ctxs.get(i).ok_or("no context")?["funding"]))
    }
}

pub fn agent_status_from(address: &str, approved: bool, name: Option<&str>, valid_until: Option<i64>, now_ms: i64) -> AgentStatus {
    let days_left = valid_until.map(|v| (v - now_ms).div_euclid(86_400_000));
    AgentStatus {
        address: address.to_string(),
        approved,
        name: name.map(String::from),
        valid_until,
        days_left,
        expiring: days_left.is_some_and(|d| d < EXPIRY_WARNING_DAYS),
    }
}

impl ExecutionVenue for HyperliquidVenue {
    fn label(&self) -> String {
        let market = match self.market {
            MarketType::Spot => "spot",
            MarketType::Perp => "perp",
        };
        format!("Hyperliquid {market}{}", if self.testnet { " (testnet)" } else { "" })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            spot: self.market == MarketType::Spot,
            perp: self.market == MarketType::Perp,
            ioc_limit: true,
            client_order_ids: true,
            sub_accounts: true,
            test_orders: false,
        }
    }

    fn ready(&self) -> Result<(), String> {
        self.user()?;
        self.signer().map(|_| ())
    }

    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        let meta = self.meta()?;
        Ok(match self.market {
            MarketType::Spot => meta.spot_instruments(self.testnet),
            MarketType::Perp => meta.perp_instruments(),
        })
    }

    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        let (base, quote) = pair.split_once('/').ok_or_else(|| format!("{pair} is not BASE/QUOTE"))?;
        self.instruments()?
            .into_iter()
            .find(|i| i.base().eq_ignore_ascii_case(base) && i.quote().eq_ignore_ascii_case(quote))
            .ok_or_else(|| match self.market {
                MarketType::Spot => format!("{} has no verified {pair} pair", self.label()),
                MarketType::Perp => format!("{} has no {base} contract", self.label()),
            })
    }

    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        let user = self.user()?;
        match self.market {
            MarketType::Spot => {
                let meta = self.meta()?;
                let state = self.info(&json!({"type": "spotClearinghouseState", "user": user}))?;
                Ok(state["balances"]
                    .as_array()
                    .map(|rows| {
                        rows.iter()
                            .map(|b| {
                                let token = b["token"].as_u64().and_then(|t| meta.token(t as u32));
                                let asset = token
                                    .and_then(|t| meta.symbol_of(t, self.testnet))
                                    .map(String::from)
                                    .unwrap_or_else(|| b["coin"].as_str().unwrap_or("").to_string());
                                let (total, hold) = (num(&b["total"]), num(&b["hold"]));
                                VenueBalance { asset, free: (total - hold).max(0.0), locked: hold }
                            })
                            .collect()
                    })
                    .unwrap_or_default())
            }
            MarketType::Perp => {
                let state = self.info(&json!({"type": "clearinghouseState", "user": user}))?;
                let mut out = vec![VenueBalance {
                    asset: "USDC".into(),
                    free: num(&state["withdrawable"]),
                    locked: num(&state.pointer("/marginSummary/totalMarginUsed").cloned().unwrap_or_default()),
                }];
                // A long holds its coin in the engine's books (1x, long-only
                // until Venues 4/5); shorts are positions only.
                for p in state["assetPositions"].as_array().into_iter().flatten() {
                    let pos = &p["position"];
                    let szi = num(&pos["szi"]);
                    if szi > 0.0 {
                        out.push(VenueBalance { asset: pos["coin"].as_str().unwrap_or("").to_string(), free: szi, locked: 0.0 });
                    }
                }
                Ok(out)
            }
        }
    }

    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        if self.market == MarketType::Spot {
            return Ok(Vec::new());
        }
        let state = self.info(&json!({"type": "clearinghouseState", "user": self.user()?}))?;
        Ok(state["assetPositions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let pos = &p["position"];
                let szi = num(&pos["szi"]);
                let value = num(&pos["positionValue"]);
                (szi != 0.0).then(|| VenuePosition {
                    symbol: pos["coin"].as_str().unwrap_or("").to_string(),
                    side: if szi > 0.0 { "long" } else { "short" }.into(),
                    size: szi.abs(),
                    entry_price: num(&pos["entryPx"]),
                    liquidation_price: Some(num(&pos["liquidationPx"])).filter(|p| *p > 0.0),
                    mark_price: (value > 0.0).then(|| value / szi.abs()),
                    leverage: pos.pointer("/leverage/value").map(num).filter(|l| *l > 0.0),
                    margin: Some(num(&pos["marginUsed"])),
                })
            })
            .collect())
    }

    fn maintenance_margin(&self, instrument: &Instrument) -> Option<f64> {
        if instrument.market != MarketType::Perp {
            return None;
        }
        let meta = self.meta().ok()?;
        let a = meta.perps.iter().find(|a| a.name == instrument.filters.symbol)?;
        Some(1.0 / (2.0 * a.max_leverage.max(1) as f64))
    }

    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String> {
        let book = self.info(&json!({"type": "l2Book", "coin": instrument.filters.symbol}))?;
        let side = |i: usize| -> Vec<(f64, f64)> {
            book.pointer(&format!("/levels/{i}"))
                .and_then(|v| v.as_array())
                .map(|lv| lv.iter().map(|l| (num(&l["px"]), num(&l["sz"]))).filter(|(p, q)| *p > 0.0 && *q > 0.0).collect())
                .unwrap_or_default()
        };
        Ok(OrderBook { bids: side(0), asks: side(1) })
    }

    fn fee_schedule(&self, instrument: &Instrument) -> Result<FeeSchedule, String> {
        let standard = match instrument.market {
            MarketType::Spot => FeeSchedule { maker: SPOT_MAKER, taker: SPOT_TAKER },
            MarketType::Perp => FeeSchedule { maker: PERP_MAKER, taker: PERP_TAKER },
        };
        let Ok(user) = self.user() else { return Ok(standard) };
        let fees = self.info(&json!({"type": "userFees", "user": user}))?;
        let (t, m) = match instrument.market {
            MarketType::Spot => ("userSpotCrossRate", "userSpotAddRate"),
            MarketType::Perp => ("userCrossRate", "userAddRate"),
        };
        match (fees.get(t).map(num), fees.get(m).map(num)) {
            (Some(taker), Some(maker)) if taker > 0.0 => Ok(FeeSchedule { maker, taker }),
            _ => Ok(standard),
        }
    }

    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        let inst = &order.instrument;
        if inst.market != self.market {
            return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, format!("{} cannot trade a {:?} instrument", self.label(), inst.market)));
        }
        self.ready()?;
        let meta = self.meta()?;
        let (asset, sz_decimals) = meta.asset_of(inst)?;
        // Closes are reduce-only (the engine says so); an opening order sets
        // isolated margin at the venue's leverage first.
        let reduce_only = order.reduce_only;
        if self.market == MarketType::Perp && !reduce_only {
            if let Err(e) = self.ensure_isolated(asset) {
                return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, e));
            }
        }
        let px = round_px(order.limit_price, sz_decimals, self.market, order.side);
        let qty = inst.filters.round_qty_down(order.qty);
        let wire = OrderWire {
            asset,
            is_buy: order.side == Side::Buy,
            px: float_to_wire(px)?,
            sz: float_to_wire(qty)?,
            reduce_only,
            tif: "Ioc",
            cloid: Some(cloid_for(&order.client_order_id)),
        };
        let sent_at = chrono::Utc::now().timestamp_millis();
        let resp = self.exchange(&order_action(&[wire]))?;
        if resp["status"].as_str() != Some("ok") {
            let why = resp["response"].as_str().map(String::from).unwrap_or_else(|| resp["response"].to_string());
            return Ok(OrderReport::empty(&order.client_order_id, OrderStatus::Rejected, why));
        }
        let st = resp.pointer("/response/data/statuses/0").cloned().unwrap_or(Value::Null);
        let base_token = meta.base_token_name(inst);
        let rate = self.fee_rate(inst);
        if let Some(filled) = st.get("filled") {
            let oid = filled["oid"].as_u64().unwrap_or(0);
            let total = num(&filled["totalSz"]);
            let avg = num(&filled["avgPx"]);
            let status = if total + 1e-12 >= qty { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
            let fills = self.fills_since(sent_at - 5_000).ok().and_then(|f| sum_fills(&f, oid));
            return Ok(report(&order.client_order_id, oid, status, inst, base_token.as_deref(), order.side, fills, (total, avg), rate, None));
        }
        if let Some(resting) = st.get("resting") {
            let oid = resting["oid"].as_u64().unwrap_or(0);
            return Ok(report(&order.client_order_id, oid, OrderStatus::Open, inst, None, order.side, None, (0.0, 0.0), rate, Some("resting on the book".into())));
        }
        let why = st.get("error").and_then(|e| e.as_str()).unwrap_or("no status in the answer").to_string();
        let status = if why.contains("could not immediately match") { OrderStatus::NotFilled } else { OrderStatus::Rejected };
        Ok(OrderReport::empty(&order.client_order_id, status, why))
    }

    fn cancel(&self, instrument: &Instrument, client_order_id: &str) -> Result<(), String> {
        let meta = self.meta()?;
        let (asset, _) = meta.asset_of(instrument)?;
        let cloid = cloid_for(client_order_id.strip_prefix(OURS_PREFIX).unwrap_or(client_order_id));
        let resp = self.exchange(&cancel_by_cloid_action(asset, &cloid))?;
        if resp["status"].as_str() != Some("ok") {
            return Err(format!("cancel refused: {}", resp["response"]));
        }
        match resp.pointer("/response/data/statuses/0") {
            Some(Value::String(s)) if s == "success" => Ok(()),
            Some(other) => Err(format!("cancel: {}", other.get("error").and_then(|e| e.as_str()).unwrap_or(&other.to_string()))),
            None => Ok(()),
        }
    }

    fn open_orders(&self, instrument: &Instrument) -> Result<Vec<String>, String> {
        let rows = self.info(&json!({"type": "openOrders", "user": self.user()?}))?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["coin"].as_str() == Some(instrument.filters.symbol.as_str()))
            .map(|o| match o["cloid"].as_str() {
                Some(c) if is_ours(c) => format!("{OURS_PREFIX}{c}"),
                Some(c) => c.to_string(),
                None => format!("oid:{}", o["oid"]),
            })
            .collect())
    }

    fn query_by_client_id(&self, instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        let cloid = cloid_for(client_order_id);
        let resp = self.info(&json!({"type": "orderStatus", "user": self.user()?, "oid": cloid}))?;
        if resp["status"].as_str() != Some("order") {
            return Ok(None);
        }
        let order = &resp["order"]["order"];
        let status = resp["order"]["status"].as_str().unwrap_or("");
        let orig = num(&order["origSz"]);
        let filled = (orig - num(&order["sz"])).max(0.0);
        let oid = order["oid"].as_u64().unwrap_or(0);
        let side = if order["side"].as_str() == Some("B") { Side::Buy } else { Side::Sell };
        let fills = if filled > 0.0 {
            let since = order["timestamp"].as_i64().unwrap_or(0) - 5_000;
            self.fills_since(since).ok().and_then(|f| sum_fills(&f, oid))
        } else {
            None
        };
        let base_token = self.meta().ok().and_then(|m| m.base_token_name(instrument));
        let st = status_of(status, filled, orig);
        let price = num(&order["limitPx"]);
        let message = (st == OrderStatus::Rejected).then(|| status.to_string());
        Ok(Some(report(client_order_id, oid, st, instrument, base_token.as_deref(), side, fills, (filled, price), self.fee_rate(instrument), message)))
    }

    fn test_order(&self, _order: &OrderRequest) -> Result<String, String> {
        Err("Hyperliquid has no test order; its testnet is the dry run.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
    const VAULT: &str = "0x1719884eb866cb12b2287399b15f7db5e7d775ea";

    fn key() -> SigningKey {
        parse_key(KEY).unwrap()
    }

    fn dummy() -> Mp {
        // float_to_int_for_hashing(1000) = 1000 * 1e8.
        Mp::Map(vec![("type", Mp::Str("dummy".into())), ("num", Mp::UInt(100_000_000_000))])
    }

    fn eth_gtc(cloid: Option<&str>) -> Mp {
        // The SDK's `t` for Gtc; our orders are always Ioc.
        order_action(&[OrderWire {
            asset: 1,
            is_buy: true,
            px: "100".into(),
            sz: "100".into(),
            reduce_only: false,
            tif: "Gtc",
            cloid: cloid.map(String::from),
        }])
    }

    fn check(sig: Signature, r: &str, s: &str, v: u8) {
        assert_eq!(sig, Signature { r: r.into(), s: s.into(), v });
    }

    #[test]
    fn the_key_signs_as_the_sdks_address() {
        assert_eq!(address_of(&key()), "0x14791697260e4c9a71f18484c9f997b308e59325");
    }

    /// `tests/signing_test.py` of hyperliquid-python-sdk.
    #[test]
    fn signatures_match_the_official_sdk_tests() {
        let k = key();
        check(sign_l1(&k, &dummy(), None, 0, None, true).unwrap(),
            "0x53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298",
            "0x755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8", 27);
        check(sign_l1(&k, &dummy(), None, 0, None, false).unwrap(),
            "0x542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510",
            "0x17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613", 28);
        check(sign_l1(&k, &dummy(), Some(VAULT), 0, None, true).unwrap(),
            "0x3c548db75e479f8012acf3000ca3a6b05606bc2ec0c29c50c515066a326239",
            "0x4d402be7396ce74fbba3795769cda45aec00dc3125a984f2a9f23177b190da2c", 28);
        check(sign_l1(&k, &eth_gtc(None), None, 0, None, true).unwrap(),
            "0xd65369825a9df5d80099e513cce430311d7d26ddf477f5b3a33d2806b100d78e",
            "0x2b54116ff64054968aa237c20ca9ff68000f977c93289157748a3162b6ea940e", 28);
        check(sign_l1(&k, &eth_gtc(None), None, 0, None, false).unwrap(),
            "0x82b2ba28e76b3d761093aaded1b1cdad4960b3af30212b343fb2e6cdfa4e3d54",
            "0x6b53878fc99d26047f4d7e8c90eb98955a109f44209163f52d8dc4278cbbd9f5", 27);
        let with_cloid = eth_gtc(Some("0x00000000000000000000000000000001"));
        check(sign_l1(&k, &with_cloid, None, 0, None, true).unwrap(),
            "0x41ae18e8239a56cacbc5dad94d45d0b747e5da11ad564077fcac71277a946e3",
            "0x3c61f667e747404fe7eea8f90ab0e76cc12ce60270438b2058324681a00116da", 27);
        check(sign_l1(&k, &with_cloid, None, 0, None, false).unwrap(),
            "0xeba0664bed2676fc4e5a743bf89e5c7501aa6d870bdb9446e122c9466c5cd16d",
            "0x7f3e74825c9114bc59086f1eebea2928c190fdfbfde144827cb02b85bbe90988", 28);
    }

    #[test]
    fn the_phantom_agent_matches_production() {
        let action = order_action(&[OrderWire {
            asset: 4,
            is_buy: true,
            px: float_to_wire(1670.1).unwrap(),
            sz: float_to_wire(0.0147).unwrap(),
            reduce_only: false,
            tif: "Ioc",
            cloid: None,
        }]);
        let hash = action_hash(&action, None, 1_677_777_606_040, None).unwrap();
        assert_eq!(hex::encode(hash), "0fcbeda5ae3c4950a548021552a4fea2226858c4453571bf3f24ba017eac2908");
    }

    /// Generated with hyperliquid-python-sdk 0.24.0 (artifacts
    /// hyperliquid-kat/gen_kat.py): the three actions this venue sends, on
    /// both networks, with and without a sub-account, at nonce 1793800000000.
    #[test]
    fn order_cancel_and_leverage_match_the_sdk() {
        let k = key();
        let n = 1_793_800_000_000;
        let spot_ioc = order_action(&[OrderWire {
            asset: 10_142,
            is_buy: true,
            px: float_to_wire(101_234.0).unwrap(),
            sz: float_to_wire(0.00123).unwrap(),
            reduce_only: false,
            tif: "Ioc",
            cloid: Some("0x71730000000000000000000000000abc".into()),
        }]);
        assert_eq!(
            hex::encode(spot_ioc.pack()),
            "83a474797065a56f72646572a66f72646572739187a161cd279ea162c3a170a6313031323334a173a7302e3030313233a172c2a17481a56c696d697481a3746966a3496f63a163d92230783731373330303030303030303030303030303030303030303030303030616263a867726f7570696e67a26e61"
        );
        let perp_reduce = order_action(&[OrderWire {
            asset: 1,
            is_buy: false,
            px: float_to_wire(3456.7).unwrap(),
            sz: float_to_wire(0.5).unwrap(),
            reduce_only: true,
            tif: "Ioc",
            cloid: None,
        }]);
        let cancel = cancel_by_cloid_action(10_142, "0x71730000000000000000000000000abc");
        let leverage = update_leverage_action(0, false, 1);
        let kat = include_str!("hyperliquid_kat.json");
        let kat: Value = serde_json::from_str(kat).unwrap();
        let mut checked = 0;
        for case in kat["cases"].as_array().unwrap() {
            let action = match case["name"].as_str().unwrap() {
                "order_ioc_spot" => &spot_ioc,
                "order_ioc_perp_reduce" => &perp_reduce,
                "cancel_by_cloid" => &cancel,
                "update_leverage_isolated_1x" => &leverage,
                other => panic!("unknown case {other}"),
            };
            assert_eq!(hex::encode(action.pack()), case["msgpack"].as_str().unwrap(), "{}", case["name"]);
            let vault = case["vault"].as_str();
            let mainnet = case["mainnet"].as_bool().unwrap();
            let sig = sign_l1(&k, action, vault, n, None, mainnet).unwrap();
            let want = Signature {
                r: case["r"].as_str().unwrap().into(),
                s: case["s"].as_str().unwrap().into(),
                v: case["v"].as_u64().unwrap() as u8,
            };
            assert_eq!(sig, want, "{} vault={vault:?} mainnet={mainnet}", case["name"]);
            checked += 1;
        }
        assert_eq!(checked, 16);
    }

    #[test]
    fn wire_numbers_follow_the_sdk() {
        for (x, s) in [(0.00123, "0.00123"), (101234.0, "101234"), (3456.7, "3456.7"), (0.5, "0.5"), (1e-8, "0.00000001"), (12.0, "12"), (100.10, "100.1"), (0.0, "0")] {
            assert_eq!(float_to_wire(x).unwrap(), s);
        }
        assert!(float_to_wire(0.123456789).is_err(), "never rounds silently");
    }

    #[test]
    fn prices_keep_five_significant_figures_inside_the_cap() {
        // BTC perp (szDecimals 5): integers above 10,000.
        assert_eq!(round_px(101_234.56, 5, MarketType::Perp, Side::Buy), 101_234.0);
        assert_eq!(round_px(101_234.56, 5, MarketType::Perp, Side::Sell), 101_235.0);
        // ETH perp (4): 3456.78 → 5 significant figures, 1 decimal.
        assert_eq!(round_px(3456.78, 4, MarketType::Perp, Side::Buy), 3456.7);
        assert_eq!(round_px(3456.78, 4, MarketType::Perp, Side::Sell), 3456.8);
        // A cheap spot token (szDecimals 0): 0.0123456 → 0.012345 (5 sig).
        assert_eq!(round_px(0.0123456, 0, MarketType::Spot, Side::Buy), 0.012345);
        // Max decimals bind before significant figures: perp szDecimals 5 → 1 decimal.
        assert_eq!(round_px(1.23456, 5, MarketType::Perp, Side::Buy), 1.2);
        assert_eq!(round_px(1.23456, 5, MarketType::Perp, Side::Sell), 1.3);
    }

    #[test]
    fn cloids_are_ours_stable_and_sixteen_bytes() {
        let a = cloid_for("qs0123456789ab-01");
        assert_eq!(a.len(), 34);
        assert!(a.starts_with("0x7173"));
        assert_eq!(a, cloid_for("qs0123456789ab-01"), "a resumed run finds the same order");
        assert_ne!(a, cloid_for("qs0123456789ab-02"));
        assert!(is_ours(&a));
        assert!(!is_ours("0x00000000000000000000000000000001"));
        assert_eq!(cloid_for(&a), a, "a cloid passes through");
    }

    #[test]
    fn nonces_rise_strictly_per_api_wallet() {
        let w = "0xnonce-test-wallet";
        let a = next_nonce(w, 1_000);
        let b = next_nonce(w, 1_000);
        let c = next_nonce(w, 999);
        assert!(a < b && b < c, "{a} {b} {c}");
        assert_eq!(next_nonce(w, 5_000), 5_000, "the clock wins once it is ahead");
        assert_eq!(next_nonce("0xother-test-wallet", 1_000), 1_000, "per wallet");
    }

    fn meta() -> Meta {
        let meta = json!({"universe": [
            {"name": "BTC", "szDecimals": 5, "maxLeverage": 40},
            {"name": "ETH", "szDecimals": 4, "maxLeverage": 25},
            {"name": "OLD", "szDecimals": 0, "maxLeverage": 3, "isDelisted": true},
        ]});
        let spot = json!({
            "tokens": [
                {"index": 0, "name": "USDC", "tokenId": "0x6d1e7cde53ba9467b783cb7c530ce054", "szDecimals": 8},
                {"index": 197, "name": "UBTC", "tokenId": "0x8f254b963e8468305d409b33aa137c67", "szDecimals": 5},
                {"index": 221, "name": "UETH", "tokenId": "0xe1edd30daaf5caac3fe63569e24748da", "szDecimals": 4},
                {"index": 900, "name": "BTC", "tokenId": "0x00000000000000000000000000000bad", "szDecimals": 2},
            ],
            "universe": [
                {"index": 142, "name": "@142", "tokens": [197, 0]},
                {"index": 151, "name": "@151", "tokens": [221, 0]},
                {"index": 777, "name": "@777", "tokens": [900, 0]},
                {"index": 778, "name": "@778", "tokens": [221, 197]},
            ]
        });
        Meta::parse(&meta, &spot)
    }

    #[test]
    fn spot_maps_by_token_id_and_rejects_a_look_alike() {
        let m = meta();
        let inst = m.spot_instruments(false);
        let pairs: Vec<String> = inst.iter().map(|i| i.pair()).collect();
        assert_eq!(pairs, vec!["BTC/USDC", "ETH/USDC", "ETH/BTC"], "the fake BTC is not a market");
        let btc = &inst[0];
        assert_eq!(btc.filters.symbol, "@142");
        assert_eq!(m.asset_of(btc).unwrap(), (10_142, 5));
        assert_eq!(btc.filters.qty_step, 0.00001);
        let raw: Vec<Instrument> = m.raw_spot_instruments().into_iter().map(|(i, _)| i).collect();
        let table = mapping::map_coins("Hyperliquid spot", &["BTC".into(), "ETH".into(), "XRP".into()], &raw, &verified_spot(false), &|i| {
            if i.base() == "0x00000000000000000000000000000bad" { "BTC".into() } else { i.base().to_string() }
        });
        assert_eq!(table.mapped.len(), 2);
        assert_eq!(table.mapped[0].custodian.as_deref(), Some("Unit"));
        assert_eq!(table.missing, vec!["XRP"]);
        assert_eq!(table.rejected, vec![("BTC".to_string(), "0x00000000000000000000000000000bad".to_string())]);
        assert_eq!(table.direct_pairs.len(), 1, "UETH/UBTC between two verified ids");
        // Testnet ids differ: mainnet's UBTC is not verified there.
        assert!(m.spot_instruments(true).is_empty());
    }

    #[test]
    fn perps_list_by_name_without_delisted_ones() {
        let m = meta();
        let perps = m.perp_instruments();
        assert_eq!(perps.iter().map(|i| i.pair()).collect::<Vec<_>>(), vec!["BTC/USDC", "ETH/USDC"]);
        assert_eq!(m.asset_of(&perps[1]).unwrap(), (1, 4));
        assert_eq!(perps[0].filters.price_tick, 0.1);
    }

    #[test]
    fn fills_normalise_with_the_fee_where_it_was_charged() {
        let m = meta();
        let btc = m.spot_instruments(false).remove(0);
        // A spot buy split over two fills; the fee comes in UBTC.
        let fills = json!([
            {"coin": "@142", "px": "100000.0", "sz": "0.001", "side": "B", "oid": 7, "fee": "0.0000007", "feeToken": "UBTC"},
            {"coin": "@142", "px": "100100.0", "sz": "0.001", "side": "B", "oid": 7, "fee": "0.0000007", "feeToken": "UBTC"},
            {"coin": "@142", "px": "1.0", "sz": "5", "side": "B", "oid": 8, "fee": "1", "feeToken": "USDC"},
        ]);
        let sum = sum_fills(&fills, 7).unwrap();
        let r = report("qs1", 7, OrderStatus::Filled, &btc, Some("UBTC"), Side::Buy, Some(sum), (0.0, 0.0), SPOT_TAKER, None);
        assert!((r.filled_qty - 0.002).abs() < 1e-12);
        assert!((r.avg_price - 100_050.0).abs() < 1e-6);
        assert_eq!(r.fee_asset, "BTC");
        assert!((r.base_received - 0.0019986).abs() < 1e-12, "{}", r.base_received);
        assert!((r.fee_quote - 0.0000014 * 100_050.0).abs() < 1e-9);
        assert!(!r.fee_estimated);
        // A sell pays in USDC; nothing comes off the base.
        let sell = json!([{"coin": "@142", "px": "100000.0", "sz": "0.002", "side": "A", "oid": 9, "fee": "0.14", "feeToken": "USDC"}]);
        let r = report("qs2", 9, OrderStatus::Filled, &btc, Some("UBTC"), Side::Sell, sum_fills(&sell, 9), (0.0, 0.0), SPOT_TAKER, None);
        assert_eq!(r.fee_asset, "USDC");
        assert_eq!(r.base_received, 0.002);
        assert!((r.fee_quote - 0.14).abs() < 1e-12);
        // No fills readable: the venue's numbers and an estimated fee.
        let r = report("qs3", 10, OrderStatus::PartiallyFilled, &btc, Some("UBTC"), Side::Sell, None, (0.001, 100_000.0), SPOT_TAKER, None);
        assert!(r.fee_estimated);
        assert!((r.fee_amount - 0.07).abs() < 1e-9);
    }

    #[test]
    fn order_statuses_normalise() {
        assert_eq!(status_of("filled", 1.0, 1.0), OrderStatus::Filled);
        assert_eq!(status_of("canceled", 0.4, 1.0), OrderStatus::PartiallyFilled);
        assert_eq!(status_of("iocCancelRejected", 0.0, 1.0), OrderStatus::NotFilled);
        assert_eq!(status_of("perpMarginRejected", 0.0, 1.0), OrderStatus::Rejected);
        assert_eq!(status_of("open", 0.0, 1.0), OrderStatus::Open);
    }

    #[test]
    fn api_wallet_expiry_warns_a_week_ahead() {
        let day = 86_400_000;
        let now = 1_800_000_000_000;
        let s = agent_status_from("0xa", true, Some("qs"), Some(now + 30 * day), now);
        assert_eq!(s.days_left, Some(30));
        assert!(!s.expiring);
        let s = agent_status_from("0xa", true, None, Some(now + 6 * day + 1), now);
        assert!(s.expiring);
        let s = agent_status_from("0xa", true, None, None, now);
        assert!(!s.expiring && s.days_left.is_none());
    }

    #[test]
    fn an_account_needs_a_valid_address_and_key() {
        assert!(HyperliquidVenue::new(MarketType::Spot, true, "0x123", "", "").is_err());
        assert!(HyperliquidVenue::new(MarketType::Spot, true, VAULT, "", "0xzz").is_err());
        let v = HyperliquidVenue::new(MarketType::Perp, true, VAULT, "", KEY).unwrap();
        assert_eq!(v.agent, "0x14791697260e4c9a71f18484c9f997b308e59325");
        assert!(v.ready().is_ok());
        assert!(HyperliquidVenue::public(MarketType::Spot, false).unwrap().ready().is_err());
        let dbg = format!("{v:?}");
        assert!(!dbg.contains("0123456789012345678901234567890123456789012345678901234567890123"), "no key in Debug");
    }

    // ── live, public calls only (`cargo test -- --ignored hyperliquid_live`) ──

    #[test]
    #[ignore]
    fn hyperliquid_live_public_info() {
        for testnet in [false, true] {
            let spot = HyperliquidVenue::public(MarketType::Spot, testnet).unwrap();
            let perp = HyperliquidVenue::public(MarketType::Perp, testnet).unwrap();
            let s = spot.instruments().unwrap();
            let p = perp.instruments().unwrap();
            println!("{}: {} verified spot pairs {:?}", spot.label(), s.len(), s.iter().map(|i| i.pair()).collect::<Vec<_>>());
            println!("{}: {} perps", perp.label(), p.len());
            let eth = spot.instrument("ETH/USDC").unwrap();
            let book = spot.order_book(&eth).unwrap();
            assert!(book.best_bid().unwrap() < book.best_ask().unwrap());
            let btc = perp.instrument("BTC/USDC").unwrap();
            let book = perp.order_book(&btc).unwrap();
            println!("BTC perp mid {:?}, ETH spot mid {:?}", book.mid(), spot.order_book(&eth).unwrap().mid());
            let table = spot.mapping(&["BTC".into(), "ETH".into(), "SOL".into(), "XRP".into()]).unwrap();
            println!("{}", table.summary);
            println!("funding BTC/h {}", perp.funding_rate_hourly("BTC").unwrap());
            let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
            println!("ETH spot close {yesterday}: {:?}", spot.daily_close_of(&eth, yesterday));
        }
    }
}
