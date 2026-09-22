//! Live order routing (PLAN-QUANTALGO §4) — Tauri-free.
//!
//! One [`Broker`] per live bot, built from the exchange's stored credentials:
//! symbol filters (lot step, minimum quantity and notional), the available
//! quote balance, and market buys/sells whose fills come back normalised to
//! one [`Fill`] shape whatever the venue's fee convention. Binance spot and
//! Bybit spot (v5, unified account) are the venues with routing; the other
//! providers stay paper-only and the preflight says so.
//!
//! Private calls go to the venue's sandbox when the exchange row says so
//! (Binance spot testnet, Bybit demo trading); public market data always
//! comes from production — the sandboxes carry thin or absent markets, and
//! a bot has to trade real prices (§4.2).

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

const RECV_WINDOW_MS: &str = "10000";
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
#[derive(Debug, Clone, PartialEq)]
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
    fn venue(&self) -> Venue;
    fn sandbox(&self) -> bool;
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

pub struct LiveBroker {
    venue: Venue,
    api_key: String,
    api_secret: String,
    sandbox: bool,
    client: reqwest::blocking::Client,
}

impl LiveBroker {
    pub fn new(provider: &str, api_key: &str, api_secret: &str, sandbox: bool) -> Result<Self, String> {
        let venue = Venue::parse(provider)
            .ok_or_else(|| format!("Live order routing is implemented for Binance and Bybit spot, not {provider}."))?;
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("HTTP client: {e}"))?;
        Ok(Self {
            venue,
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
            sandbox,
            client,
        })
    }

    fn has_keys(&self) -> Result<(), String> {
        if self.api_key.is_empty() || self.api_secret.is_empty() {
            return Err("The exchange has no API key and secret stored; live trading needs both.".into());
        }
        Ok(())
    }

    fn server_time_ms(&self) -> i64 {
        let fallback = chrono::Utc::now().timestamp_millis();
        let url = match self.venue {
            Venue::Binance => format!("{}/api/v3/time", self.venue.public_base()),
            Venue::Bybit => format!("{}/v5/market/time", self.venue.public_base()),
        };
        let body: Option<Value> = self.client.get(&url).send().ok().and_then(|r| r.json().ok());
        match (self.venue, body) {
            (Venue::Binance, Some(j)) => j.get("serverTime").and_then(|v| v.as_i64()).unwrap_or(fallback),
            (Venue::Bybit, Some(j)) => j
                .pointer("/result/timeSecond")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|s| (s * 1000.0) as i64)
                .or_else(|| j.get("time").and_then(|v| v.as_i64()))
                .unwrap_or(fallback),
            _ => fallback,
        }
    }

    // ── Binance ──

    fn binance_signed(&self, method: reqwest::Method, path: &str, params: &[(&str, String)]) -> Result<Value, String> {
        self.has_keys()?;
        let ts = self.server_time_ms().to_string();
        let mut query: Vec<String> = params.iter().map(|(k, v)| format!("{k}={v}")).collect();
        query.push(format!("recvWindow={RECV_WINDOW_MS}"));
        query.push(format!("timestamp={ts}"));
        let query = query.join("&");
        let signature = binance_signature(&self.api_secret, &query);
        let url = format!("{}{path}?{query}&signature={signature}", self.venue.private_base(self.sandbox));
        let resp = self
            .client
            .request(method, &url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .map_err(|e| format!("Binance request failed: {e}"))?;
        let status = resp.status();
        let body: Value = resp.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = body.get("msg").and_then(|v| v.as_str()).unwrap_or("request rejected");
            let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            return Err(format!("Binance {status} ({code}): {msg}"));
        }
        Ok(body)
    }

    fn binance_filters(&self, pair: &str) -> Result<SymbolFilters, String> {
        let symbol = self.venue.symbol(pair);
        let url = format!("{}/api/v3/exchangeInfo", self.venue.public_base());
        let body: Value = self
            .client
            .get(&url)
            .query(&[("symbol", symbol.as_str())])
            .send()
            .map_err(|e| format!("Binance exchangeInfo: {e}"))?
            .json()
            .map_err(|e| format!("Binance exchangeInfo parse: {e}"))?;
        let sym = body
            .get("symbols")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| format!("Binance does not list {pair} ({symbol})"))?;
        let mut filters = SymbolFilters {
            symbol: symbol.clone(),
            base: sym.get("baseAsset").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            quote: sym.get("quoteAsset").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            qty_step: 0.0,
            min_qty: 0.0,
            min_notional: 0.0,
            price_tick: 0.0,
        };
        for filter in sym.get("filters").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            match filter.get("filterType").and_then(|v| v.as_str()).unwrap_or("") {
                "LOT_SIZE" => {
                    filters.qty_step = f(&filter["stepSize"]);
                    filters.min_qty = f(&filter["minQty"]);
                }
                "NOTIONAL" | "MIN_NOTIONAL" => {
                    filters.min_notional = filters.min_notional.max(f(&filter["minNotional"]));
                }
                "PRICE_FILTER" => filters.price_tick = f(&filter["tickSize"]),
                _ => {}
            }
        }
        if filters.qty_step <= 0.0 {
            return Err(format!("Binance returned no LOT_SIZE filter for {symbol}"));
        }
        Ok(filters)
    }

    fn binance_balance(&self, asset: &str) -> Result<f64, String> {
        let body = self.binance_signed(reqwest::Method::GET, "/api/v3/account", &[])?;
        Ok(body
            .get("balances")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|b| b.get("asset").and_then(|v| v.as_str()).map(|a| a.eq_ignore_ascii_case(asset)).unwrap_or(false))
            })
            .map(|b| f(&b["free"]))
            .unwrap_or(0.0))
    }

    fn binance_order(&self, filters: &SymbolFilters, side: &str, params: Vec<(&str, String)>, test: bool) -> Result<Value, String> {
        let mut all: Vec<(&str, String)> = vec![
            ("symbol", filters.symbol.clone()),
            ("side", side.to_string()),
            ("type", "MARKET".to_string()),
        ];
        all.extend(params);
        all.push(("newOrderRespType", "FULL".to_string()));
        let path = if test { "/api/v3/order/test" } else { "/api/v3/order" };
        self.binance_signed(reqwest::Method::POST, path, &all)
    }

    // ── Bybit ──

    fn bybit_signed(&self, method: reqwest::Method, path: &str, query: Option<&str>, body: Option<&Value>) -> Result<Value, String> {
        self.has_keys()?;
        let ts = self.server_time_ms().to_string();
        let payload = match (query, body) {
            (Some(q), _) => q.to_string(),
            (None, Some(b)) => b.to_string(),
            (None, None) => String::new(),
        };
        let signature = bybit_signature(&self.api_secret, &ts, &self.api_key, RECV_WINDOW_MS, &payload);
        let url = match query {
            Some(q) => format!("{}{path}?{q}", self.venue.private_base(self.sandbox)),
            None => format!("{}{path}", self.venue.private_base(self.sandbox)),
        };
        let mut req = self
            .client
            .request(method, &url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", &ts)
            .header("X-BAPI-SIGN", &signature)
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW_MS)
            .header("X-BAPI-SIGN-TYPE", "2");
        if let Some(b) = body {
            req = req.header("Content-Type", "application/json").body(b.to_string());
        }
        let resp = req.send().map_err(|e| format!("Bybit request failed: {e}"))?;
        let status = resp.status();
        let json: Value = resp.json().unwrap_or(Value::Null);
        let ret_code = json.get("retCode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if !status.is_success() || ret_code != 0 {
            let msg = json.get("retMsg").and_then(|v| v.as_str()).unwrap_or("request rejected");
            return Err(format!("Bybit {status} ({ret_code}): {msg}"));
        }
        Ok(json)
    }

    fn bybit_filters(&self, pair: &str) -> Result<SymbolFilters, String> {
        let symbol = self.venue.symbol(pair);
        let url = format!("{}/v5/market/instruments-info", self.venue.public_base());
        let body: Value = self
            .client
            .get(&url)
            .query(&[("category", "spot"), ("symbol", symbol.as_str())])
            .send()
            .map_err(|e| format!("Bybit instruments-info: {e}"))?
            .json()
            .map_err(|e| format!("Bybit instruments-info parse: {e}"))?;
        let row = body
            .pointer("/result/list")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| format!("Bybit does not list {pair} ({symbol})"))?;
        let lot = row.get("lotSizeFilter").cloned().unwrap_or(Value::Null);
        let filters = SymbolFilters {
            symbol: symbol.clone(),
            base: row.get("baseCoin").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            quote: row.get("quoteCoin").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            qty_step: f(&lot["basePrecision"]),
            min_qty: f(&lot["minOrderQty"]),
            min_notional: f(&lot["minOrderAmt"]),
            price_tick: f(&row["priceFilter"]["tickSize"]),
        };
        if filters.qty_step <= 0.0 {
            return Err(format!("Bybit returned no lot size for {symbol}"));
        }
        Ok(filters)
    }

    fn bybit_balance(&self, asset: &str) -> Result<f64, String> {
        let query = format!("accountType=UNIFIED&coin={}", asset.to_uppercase());
        let body = self.bybit_signed(reqwest::Method::GET, "/v5/account/wallet-balance", Some(&query), None)?;
        let mut available = 0.0;
        for account in body.pointer("/result/list").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            for coin in account.get("coin").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
                if coin.get("coin").and_then(|v| v.as_str()).map(|c| c.eq_ignore_ascii_case(asset)).unwrap_or(false) {
                    // `availableToWithdraw` is empty on some accounts; the wallet
                    // balance minus locked is the spendable amount.
                    let avail = f(&coin["availableToWithdraw"]);
                    available = if avail > 0.0 {
                        avail
                    } else {
                        (f(&coin["walletBalance"]) - f(&coin["locked"])).max(0.0)
                    };
                }
            }
        }
        Ok(available)
    }

    fn bybit_order_row(&self, symbol: &str, order_id: &str) -> Result<Value, String> {
        // A market order fills at once; the realtime endpoint may still lag a
        // moment behind the create call.
        let query = format!("category=spot&symbol={symbol}&orderId={order_id}");
        let mut last = Value::Null;
        for _ in 0..8 {
            let body = self.bybit_signed(reqwest::Method::GET, "/v5/order/realtime", Some(&query), None)
                .or_else(|_| self.bybit_signed(reqwest::Method::GET, "/v5/order/history", Some(&query), None))?;
            if let Some(row) = body.pointer("/result/list").and_then(|v| v.as_array()).and_then(|a| a.first()) {
                last = row.clone();
                let status = row.get("orderStatus").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(status, "Filled" | "PartiallyFilledCanceled" | "Cancelled" | "Rejected") {
                    return Ok(last);
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if last.is_null() {
            return Err(format!("Bybit order {order_id} not found after placing it"));
        }
        Ok(last)
    }
}

impl Broker for LiveBroker {
    fn venue(&self) -> Venue {
        self.venue
    }

    fn sandbox(&self) -> bool {
        self.sandbox
    }

    fn symbol_filters(&self, pair: &str) -> Result<SymbolFilters, String> {
        match self.venue {
            Venue::Binance => self.binance_filters(pair),
            Venue::Bybit => self.bybit_filters(pair),
        }
    }

    fn available_balance(&self, asset: &str) -> Result<f64, String> {
        match self.venue {
            Venue::Binance => self.binance_balance(asset),
            Venue::Bybit => self.bybit_balance(asset),
        }
    }

    fn market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<Fill, String> {
        if quote_amount < filters.min_notional {
            return Err(format!(
                "{} is below the minimum order of {} {}",
                filters.quote_string(quote_amount), filters.quote_string(filters.min_notional), filters.quote
            ));
        }
        match self.venue {
            Venue::Binance => {
                let resp = self.binance_order(filters, "BUY", vec![("quoteOrderQty", filters.quote_string(quote_amount))], false)?;
                normalize_binance_fill(&resp, filters)
            }
            Venue::Bybit => {
                let body = serde_json::json!({
                    "category": "spot",
                    "symbol": filters.symbol,
                    "side": "Buy",
                    "orderType": "Market",
                    "qty": filters.quote_string(quote_amount),
                    "marketUnit": "quoteCoin",
                });
                let created = self.bybit_signed(reqwest::Method::POST, "/v5/order/create", None, Some(&body))?;
                let order_id = s(&created["result"]["orderId"]);
                let row = self.bybit_order_row(&filters.symbol, &order_id)?;
                normalize_bybit_order(&row, filters)
            }
        }
    }

    fn market_sell(&self, filters: &SymbolFilters, base_qty: f64) -> Result<Fill, String> {
        let qty = filters.round_qty_down(base_qty);
        if qty < filters.min_qty || qty <= 0.0 {
            return Err(format!(
                "{} {} is below the minimum sell quantity of {}",
                filters.qty_string(qty), filters.base, filters.qty_string(filters.min_qty)
            ));
        }
        match self.venue {
            Venue::Binance => {
                let resp = self.binance_order(filters, "SELL", vec![("quantity", filters.qty_string(qty))], false)?;
                normalize_binance_fill(&resp, filters)
            }
            Venue::Bybit => {
                let body = serde_json::json!({
                    "category": "spot",
                    "symbol": filters.symbol,
                    "side": "Sell",
                    "orderType": "Market",
                    "qty": filters.qty_string(qty),
                    "marketUnit": "baseCoin",
                });
                let created = self.bybit_signed(reqwest::Method::POST, "/v5/order/create", None, Some(&body))?;
                let order_id = s(&created["result"]["orderId"]);
                let row = self.bybit_order_row(&filters.symbol, &order_id)?;
                normalize_bybit_order(&row, filters)
            }
        }
    }

    fn test_market_buy(&self, filters: &SymbolFilters, quote_amount: f64) -> Result<String, String> {
        match self.venue {
            Venue::Binance => {
                self.binance_order(filters, "BUY", vec![("quoteOrderQty", filters.quote_string(quote_amount))], true)?;
                Ok(format!(
                    "Binance accepted a test market buy of {} {} on {} (nothing executed)",
                    filters.quote_string(quote_amount), filters.quote, filters.symbol
                ))
            }
            Venue::Bybit => Ok("Bybit offers no order dry run; filters and balance were checked instead".into()),
        }
    }
}

impl std::fmt::Debug for LiveBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveBroker")
            .field("venue", &self.venue)
            .field("sandbox", &self.sandbox)
            .field("api_key", &format!("{}…", &self.api_key[..self.api_key.len().min(4)]))
            .finish()
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
