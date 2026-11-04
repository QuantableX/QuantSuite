//! Binance spot and Bybit spot (v5, unified account) as [`ExecutionVenue`]s
//! (PLAN-AUTO-EXECUTION §2.2). The HTTP and signing are what `LiveBroker`
//! used before; new are IOC limit orders with our client order id, the
//! order book, the account's fee tier, every instrument, and the lookup of
//! an order by client id that makes a resumed run idempotent.
//!
//! Private calls go to the venue's sandbox when the exchange row says so
//! (Binance spot testnet, Bybit demo trading); public market data always
//! comes from production — the sandboxes carry thin or absent markets.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::venue::{
    Capabilities, ExecutionVenue, FeeSchedule, Instrument, OrderBook, OrderReport, OrderRequest, OrderStatus, Side,
    VenueBalance, VenuePosition,
};
use crate::broker::{
    binance_signature, bybit_signature, normalize_binance_fill, normalize_bybit_order, Fill, SymbolFilters, Venue,
};

const RECV_WINDOW_MS: &str = "10000";
/// A fee tier the venue would not tell: its standard spot taker rate.
pub const DEFAULT_TAKER_FEE: f64 = 0.001;
const INSTRUMENTS_TTL: Duration = Duration::from_secs(15 * 60);

fn f(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())).unwrap_or(0.0)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

type InstrumentCache = Mutex<HashMap<&'static str, (Instant, Vec<Instrument>)>>;

/// Every instrument per venue, shared by all accounts (public data).
fn instrument_cache() -> &'static InstrumentCache {
    static CACHE: OnceLock<InstrumentCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct CexVenue {
    venue: Venue,
    api_key: String,
    api_secret: String,
    sandbox: bool,
    client: reqwest::blocking::Client,
}

impl Drop for CexVenue {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.api_key.zeroize();
        self.api_secret.zeroize();
    }
}

impl std::fmt::Debug for CexVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CexVenue")
            .field("venue", &self.venue)
            .field("sandbox", &self.sandbox)
            .field("api_key", &format!("{}…", &self.api_key[..self.api_key.len().min(4)]))
            .finish()
    }
}

/// Binance's order statuses for an IOC order.
fn binance_status(status: &str, executed: f64) -> OrderStatus {
    match status {
        "FILLED" => OrderStatus::Filled,
        "REJECTED" => OrderStatus::Rejected,
        "NEW" | "PENDING_NEW" => OrderStatus::Open,
        "PARTIALLY_FILLED" if executed > 0.0 => OrderStatus::Open,
        _ if executed > 0.0 => OrderStatus::PartiallyFilled,
        _ => OrderStatus::NotFilled,
    }
}

fn bybit_status(status: &str, executed: f64) -> OrderStatus {
    match status {
        "Filled" => OrderStatus::Filled,
        "Rejected" => OrderStatus::Rejected,
        "New" | "Created" | "Untriggered" => OrderStatus::Open,
        "PartiallyFilled" => OrderStatus::Open,
        _ if executed > 0.0 => OrderStatus::PartiallyFilled,
        _ => OrderStatus::NotFilled,
    }
}

/// A normalised fill as an order report.
pub fn report_from_fill(client_order_id: &str, status: OrderStatus, fill: &Fill) -> OrderReport {
    OrderReport {
        client_order_id: client_order_id.to_string(),
        venue_order_id: fill.order_id.clone(),
        status,
        filled_qty: fill.base_qty,
        base_received: fill.base_received,
        avg_price: fill.avg_price,
        quote_amount: fill.quote_amount,
        fee_asset: fill.fee_asset.clone(),
        fee_amount: fill.fee_amount,
        fee_quote: fill.fee_quote,
        fee_estimated: fill.fee_estimated,
        message: None,
    }
}

/// A Binance order response (FULL, or a queried order plus its trades as
/// `fills`) → report.
pub fn binance_report(resp: &Value, filters: &SymbolFilters, client_order_id: &str) -> OrderReport {
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let executed = f(&resp["executedQty"]);
    let state = binance_status(status, executed);
    match normalize_binance_fill(resp, filters) {
        Ok(fill) => report_from_fill(client_order_id, state, &fill),
        Err(e) => {
            let mut r = OrderReport::empty(client_order_id, state, e);
            r.venue_order_id = s(&resp["orderId"]);
            r
        }
    }
}

/// A Bybit order row → report.
pub fn bybit_report(row: &Value, filters: &SymbolFilters, client_order_id: &str) -> OrderReport {
    let status = row.get("orderStatus").and_then(|v| v.as_str()).unwrap_or("");
    let executed = f(&row["cumExecQty"]);
    let state = bybit_status(status, executed);
    match normalize_bybit_order(row, filters) {
        Ok(fill) => report_from_fill(client_order_id, state, &fill),
        Err(e) => {
            let mut r = OrderReport::empty(client_order_id, state, e);
            r.venue_order_id = s(&row["orderId"]);
            if let Some(reason) = row.get("rejectReason").and_then(|v| v.as_str()).filter(|r| !r.is_empty() && *r != "EC_NoError") {
                r.message = Some(reason.to_string());
            }
            r
        }
    }
}

fn binance_filters_from(sym: &Value) -> Option<SymbolFilters> {
    let mut filters = SymbolFilters {
        symbol: sym.get("symbol")?.as_str()?.to_string(),
        base: sym.get("baseAsset")?.as_str()?.to_string(),
        quote: sym.get("quoteAsset")?.as_str()?.to_string(),
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
            "NOTIONAL" | "MIN_NOTIONAL" => filters.min_notional = filters.min_notional.max(f(&filter["minNotional"])),
            "PRICE_FILTER" => filters.price_tick = f(&filter["tickSize"]),
            _ => {}
        }
    }
    (filters.qty_step > 0.0).then_some(filters)
}

fn bybit_filters_from(row: &Value) -> Option<SymbolFilters> {
    let lot = row.get("lotSizeFilter").cloned().unwrap_or(Value::Null);
    let filters = SymbolFilters {
        symbol: row.get("symbol")?.as_str()?.to_string(),
        base: row.get("baseCoin")?.as_str()?.to_string(),
        quote: row.get("quoteCoin")?.as_str()?.to_string(),
        qty_step: f(&lot["basePrecision"]),
        min_qty: f(&lot["minOrderQty"]),
        min_notional: f(&lot["minOrderAmt"]),
        price_tick: f(&row["priceFilter"]["tickSize"]),
    };
    (filters.qty_step > 0.0).then_some(filters)
}

fn levels(v: &Value) -> Vec<(f64, f64)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    let l = l.as_array()?;
                    Some((f(l.first()?), f(l.get(1)?)))
                })
                .filter(|(p, q)| *p > 0.0 && *q > 0.0)
                .collect()
        })
        .unwrap_or_default()
}

impl CexVenue {
    pub fn new(provider: &str, api_key: &str, api_secret: &str, sandbox: bool) -> Result<Self, String> {
        let venue = Venue::parse(provider)
            .ok_or_else(|| format!("Live order routing is implemented for Binance and Bybit spot, not {provider}."))?;
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION"), " (QuantAlgo)"))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("HTTP client: {e}"))?;
        Ok(Self { venue, api_key: api_key.to_string(), api_secret: api_secret.to_string(), sandbox, client })
    }

    pub fn venue(&self) -> Venue {
        self.venue
    }

    pub fn sandbox(&self) -> bool {
        self.sandbox
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

    fn public_get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let url = format!("{}{path}", self.venue.public_base());
        let resp = self
            .client
            .get(&url)
            .query(query)
            .send()
            .map_err(|e| format!("{} {path}: {e}", self.venue.label()))?;
        resp.json().map_err(|e| format!("{} {path} parse: {e}", self.venue.label()))
    }

    // ── Binance ──

    /// A signed call; `Err((code, message))` keeps Binance's error code so
    /// "order does not exist" can be told from a failure.
    fn binance_call(&self, method: reqwest::Method, path: &str, params: &[(&str, String)]) -> Result<Value, (i64, String)> {
        self.has_keys().map_err(|e| (0, e))?;
        let ts = self.server_time_ms().to_string();
        let mut query: Vec<String> = params.iter().map(|(k, v)| format!("{k}={}", urlencode(v))).collect();
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
            .map_err(|e| (-1, format!("Binance request failed: {e}")))?;
        let status = resp.status();
        let body: Value = resp.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = body.get("msg").and_then(|v| v.as_str()).unwrap_or("request rejected");
            let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            return Err((code, format!("Binance {status} ({code}): {msg}")));
        }
        Ok(body)
    }

    fn binance_signed(&self, method: reqwest::Method, path: &str, params: &[(&str, String)]) -> Result<Value, String> {
        self.binance_call(method, path, params).map_err(|(_, e)| e)
    }

    fn binance_order_params(order: &OrderRequest) -> Vec<(&'static str, String)> {
        vec![
            ("symbol", order.instrument.filters.symbol.clone()),
            ("side", if order.side == Side::Buy { "BUY" } else { "SELL" }.to_string()),
            ("type", "LIMIT".to_string()),
            ("timeInForce", "IOC".to_string()),
            ("quantity", order.instrument.filters.qty_string(order.qty)),
            ("price", order.instrument.price_string(order.limit_price)),
            ("newClientOrderId", order.client_order_id.clone()),
            ("newOrderRespType", "FULL".to_string()),
        ]
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

    /// The order row by client id, from the realtime list or the history.
    fn bybit_row_by_link(&self, symbol: &str, link: &str) -> Result<Option<Value>, String> {
        let query = format!("category=spot&symbol={symbol}&orderLinkId={link}");
        for path in ["/v5/order/realtime", "/v5/order/history"] {
            let body = self.bybit_signed(reqwest::Method::GET, path, Some(&query), None)?;
            if let Some(row) = body.pointer("/result/list").and_then(|v| v.as_array()).and_then(|a| a.first()) {
                return Ok(Some(row.clone()));
            }
        }
        Ok(None)
    }

    /// An IOC order settles at once; the realtime endpoint may lag a moment
    /// behind the create call.
    fn bybit_settled_row(&self, symbol: &str, link: &str) -> Result<Value, String> {
        let mut last = None;
        for _ in 0..8 {
            if let Some(row) = self.bybit_row_by_link(symbol, link)? {
                let status = row.get("orderStatus").and_then(|v| v.as_str()).unwrap_or("").to_string();
                last = Some(row);
                if matches!(status.as_str(), "Filled" | "PartiallyFilledCanceled" | "Cancelled" | "Rejected") {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        last.ok_or_else(|| format!("Bybit order {link} not found after placing it"))
    }

    fn fetch_instruments(&self) -> Result<Vec<Instrument>, String> {
        match self.venue {
            Venue::Binance => {
                let body = self.public_get("/api/v3/exchangeInfo", &[("permissions", "SPOT")])?;
                Ok(body
                    .get("symbols")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter(|sym| sym.get("status").and_then(|v| v.as_str()) == Some("TRADING"))
                            .filter_map(binance_filters_from)
                            .map(Instrument::spot)
                            .collect()
                    })
                    .unwrap_or_default())
            }
            Venue::Bybit => {
                let body = self.public_get("/v5/market/instruments-info", &[("category", "spot")])?;
                Ok(body
                    .pointer("/result/list")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter(|row| row.get("status").and_then(|v| v.as_str()) == Some("Trading"))
                            .filter_map(bybit_filters_from)
                            .map(Instrument::spot)
                            .collect()
                    })
                    .unwrap_or_default())
            }
        }
    }
}

fn urlencode(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl ExecutionVenue for CexVenue {
    fn label(&self) -> String {
        format!("{}{}", self.venue.label(), if self.sandbox { " (sandbox)" } else { "" })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            spot: true,
            perp: false,
            ioc_limit: true,
            client_order_ids: true,
            sub_accounts: true,
            test_orders: self.venue == Venue::Binance,
        }
    }

    fn ready(&self) -> Result<(), String> {
        self.has_keys()
    }

    fn instruments(&self) -> Result<Vec<Instrument>, String> {
        let key = self.venue.label();
        if let Ok(cache) = instrument_cache().lock() {
            if let Some((at, list)) = cache.get(key) {
                if at.elapsed() < INSTRUMENTS_TTL {
                    return Ok(list.clone());
                }
            }
        }
        let list = self.fetch_instruments()?;
        if let Ok(mut cache) = instrument_cache().lock() {
            cache.insert(key, (Instant::now(), list.clone()));
        }
        Ok(list)
    }

    fn instrument(&self, pair: &str) -> Result<Instrument, String> {
        let symbol = self.venue.symbol(pair);
        let filters = match self.venue {
            Venue::Binance => {
                let body = self.public_get("/api/v3/exchangeInfo", &[("symbol", symbol.as_str())])?;
                body.get("symbols")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .ok_or_else(|| format!("Binance does not list {pair} ({symbol})"))
                    .and_then(|sym| binance_filters_from(sym).ok_or_else(|| format!("Binance returned no LOT_SIZE filter for {symbol}")))?
            }
            Venue::Bybit => {
                let body = self.public_get("/v5/market/instruments-info", &[("category", "spot"), ("symbol", symbol.as_str())])?;
                body.pointer("/result/list")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .ok_or_else(|| format!("Bybit does not list {pair} ({symbol})"))
                    .and_then(|row| bybit_filters_from(row).ok_or_else(|| format!("Bybit returned no lot size for {symbol}")))?
            }
        };
        Ok(Instrument::spot(filters))
    }

    fn balances(&self) -> Result<Vec<VenueBalance>, String> {
        match self.venue {
            Venue::Binance => {
                let body = self.binance_signed(reqwest::Method::GET, "/api/v3/account", &[])?;
                Ok(body
                    .get("balances")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .map(|b| VenueBalance { asset: s(&b["asset"]), free: f(&b["free"]), locked: f(&b["locked"]) })
                            .filter(|b| b.free > 0.0 || b.locked > 0.0)
                            .collect()
                    })
                    .unwrap_or_default())
            }
            Venue::Bybit => {
                let body = self.bybit_signed(reqwest::Method::GET, "/v5/account/wallet-balance", Some("accountType=UNIFIED"), None)?;
                let mut out = Vec::new();
                for account in body.pointer("/result/list").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
                    for coin in account.get("coin").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
                        let wallet = f(&coin["walletBalance"]);
                        let locked = f(&coin["locked"]);
                        // `availableToWithdraw` is empty on some accounts; the
                        // wallet balance minus locked is the spendable amount.
                        let avail = f(&coin["availableToWithdraw"]);
                        let free = if avail > 0.0 { avail } else { (wallet - locked).max(0.0) };
                        if wallet > 0.0 {
                            out.push(VenueBalance { asset: s(&coin["coin"]), free, locked });
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    fn positions(&self) -> Result<Vec<VenuePosition>, String> {
        Ok(Vec::new())
    }

    fn order_book(&self, instrument: &Instrument) -> Result<OrderBook, String> {
        let symbol = instrument.filters.symbol.as_str();
        match self.venue {
            Venue::Binance => {
                let body = self.public_get("/api/v3/depth", &[("symbol", symbol), ("limit", "100")])?;
                Ok(OrderBook { bids: levels(&body["bids"]), asks: levels(&body["asks"]) })
            }
            Venue::Bybit => {
                let body = self.public_get("/v5/market/orderbook", &[("category", "spot"), ("symbol", symbol), ("limit", "200")])?;
                Ok(OrderBook { bids: levels(&body["result"]["b"]), asks: levels(&body["result"]["a"]) })
            }
        }
    }

    fn fee_schedule(&self, instrument: &Instrument) -> Result<FeeSchedule, String> {
        let symbol = instrument.filters.symbol.clone();
        match self.venue {
            Venue::Binance => {
                let body = self.binance_signed(reqwest::Method::GET, "/api/v3/account/commission", &[("symbol", symbol)])?;
                let c = &body["standardCommission"];
                Ok(FeeSchedule { maker: f(&c["maker"]), taker: f(&c["taker"]) })
            }
            Venue::Bybit => {
                let query = format!("category=spot&symbol={symbol}");
                let body = self.bybit_signed(reqwest::Method::GET, "/v5/account/fee-rate", Some(&query), None)?;
                let row = body.pointer("/result/list/0").cloned().unwrap_or(Value::Null);
                Ok(FeeSchedule { maker: f(&row["makerFeeRate"]), taker: f(&row["takerFeeRate"]) })
            }
        }
    }

    fn place(&self, order: &OrderRequest) -> Result<OrderReport, String> {
        let filters = &order.instrument.filters;
        match self.venue {
            Venue::Binance => {
                let resp = self.binance_signed(reqwest::Method::POST, "/api/v3/order", &Self::binance_order_params(order))?;
                Ok(binance_report(&resp, filters, &order.client_order_id))
            }
            Venue::Bybit => {
                let body = json!({
                    "category": "spot",
                    "symbol": filters.symbol,
                    "side": if order.side == Side::Buy { "Buy" } else { "Sell" },
                    "orderType": "Limit",
                    "timeInForce": "IOC",
                    "qty": filters.qty_string(order.qty),
                    "price": order.instrument.price_string(order.limit_price),
                    "orderLinkId": order.client_order_id,
                });
                self.bybit_signed(reqwest::Method::POST, "/v5/order/create", None, Some(&body))?;
                let row = self.bybit_settled_row(&filters.symbol, &order.client_order_id)?;
                Ok(bybit_report(&row, filters, &order.client_order_id))
            }
        }
    }

    fn cancel(&self, instrument: &Instrument, client_order_id: &str) -> Result<(), String> {
        let symbol = instrument.filters.symbol.clone();
        match self.venue {
            Venue::Binance => self
                .binance_signed(
                    reqwest::Method::DELETE,
                    "/api/v3/order",
                    &[("symbol", symbol), ("origClientOrderId", client_order_id.to_string())],
                )
                .map(|_| ()),
            Venue::Bybit => self
                .bybit_signed(
                    reqwest::Method::POST,
                    "/v5/order/cancel",
                    None,
                    Some(&json!({ "category": "spot", "symbol": symbol, "orderLinkId": client_order_id })),
                )
                .map(|_| ()),
        }
    }

    fn open_orders(&self, instrument: &Instrument) -> Result<Vec<String>, String> {
        let symbol = instrument.filters.symbol.clone();
        match self.venue {
            Venue::Binance => {
                let body = self.binance_signed(reqwest::Method::GET, "/api/v3/openOrders", &[("symbol", symbol)])?;
                Ok(body
                    .as_array()
                    .map(|a| a.iter().map(|o| s(&o["clientOrderId"])).filter(|id| !id.is_empty()).collect())
                    .unwrap_or_default())
            }
            Venue::Bybit => {
                let query = format!("category=spot&symbol={symbol}&openOnly=0");
                let body = self.bybit_signed(reqwest::Method::GET, "/v5/order/realtime", Some(&query), None)?;
                Ok(body
                    .pointer("/result/list")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter(|o| matches!(o.get("orderStatus").and_then(|v| v.as_str()), Some("New" | "PartiallyFilled" | "Untriggered")))
                            .map(|o| s(&o["orderLinkId"]))
                            .filter(|id| !id.is_empty())
                            .collect()
                    })
                    .unwrap_or_default())
            }
        }
    }

    fn query_by_client_id(&self, instrument: &Instrument, client_order_id: &str) -> Result<Option<OrderReport>, String> {
        let filters = &instrument.filters;
        match self.venue {
            Venue::Binance => {
                let order = match self.binance_call(
                    reqwest::Method::GET,
                    "/api/v3/order",
                    &[("symbol", filters.symbol.clone()), ("origClientOrderId", client_order_id.to_string())],
                ) {
                    Ok(order) => order,
                    // -2013: "Order does not exist."
                    Err((-2013, _)) => return Ok(None),
                    Err((_, e)) => return Err(e),
                };
                let mut full = order.clone();
                if f(&order["executedQty"]) > 0.0 {
                    // The fees are on the trades, not on the order.
                    let trades = self.binance_signed(
                        reqwest::Method::GET,
                        "/api/v3/myTrades",
                        &[("symbol", filters.symbol.clone()), ("orderId", s(&order["orderId"]))],
                    )?;
                    let fills: Vec<Value> = trades
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|t| json!({ "commission": t["commission"], "commissionAsset": t["commissionAsset"] }))
                        .collect();
                    full["fills"] = Value::Array(fills);
                }
                Ok(Some(binance_report(&full, filters, client_order_id)))
            }
            Venue::Bybit => Ok(self
                .bybit_row_by_link(&filters.symbol, client_order_id)?
                .map(|row| bybit_report(&row, filters, client_order_id))),
        }
    }

    fn test_order(&self, order: &OrderRequest) -> Result<String, String> {
        match self.venue {
            Venue::Binance => {
                let mut params = Self::binance_order_params(order);
                params.retain(|(k, _)| *k != "newOrderRespType");
                self.binance_signed(reqwest::Method::POST, "/api/v3/order/test", &params)?;
                Ok(format!(
                    "Binance accepted a test IOC {} of {} {} at {} on {} (nothing executed)",
                    order.side.as_str(),
                    order.instrument.filters.qty_string(order.qty),
                    order.instrument.base(),
                    order.instrument.price_string(order.limit_price),
                    order.instrument.filters.symbol
                ))
            }
            Venue::Bybit => Ok("Bybit offers no order dry run; filters and balance were checked instead".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn binance_ioc_responses_map_to_states() {
        let filled = json!({
            "orderId": 7, "status": "FILLED", "executedQty": "0.00100", "cummulativeQuoteQty": "64.0",
            "fills": [{"commission": "0.000001", "commissionAsset": "BTC"}]
        });
        let r = binance_report(&filled, &btc(), "qs-1");
        assert_eq!(r.status, OrderStatus::Filled);
        assert_eq!(r.client_order_id, "qs-1");
        assert!((r.base_received - 0.000999).abs() < 1e-12);
        let partial = json!({"orderId": 8, "status": "EXPIRED", "executedQty": "0.0005", "cummulativeQuoteQty": "32.0", "fills": []});
        assert_eq!(binance_report(&partial, &btc(), "qs-2").status, OrderStatus::PartiallyFilled);
        let none = json!({"orderId": 9, "status": "EXPIRED", "executedQty": "0", "cummulativeQuoteQty": "0"});
        let r = binance_report(&none, &btc(), "qs-3");
        assert_eq!(r.status, OrderStatus::NotFilled);
        assert_eq!(r.venue_order_id, "9");
        assert_eq!(r.filled_qty, 0.0);
    }

    #[test]
    fn bybit_ioc_rows_map_to_states() {
        let row = json!({"orderId": "b1", "orderStatus": "PartiallyFilledCanceled", "side": "Sell", "cumExecQty": "0.5",
            "cumExecValue": "50", "avgPrice": "100", "cumExecFee": "0.05"});
        let r = bybit_report(&row, &btc(), "qs-4");
        assert_eq!(r.status, OrderStatus::PartiallyFilled);
        assert_eq!(r.fee_asset, "USDT");
        let row = json!({"orderId": "b2", "orderStatus": "Cancelled", "cumExecQty": "0", "cumExecValue": "0"});
        assert_eq!(bybit_report(&row, &btc(), "qs-5").status, OrderStatus::NotFilled);
        let row = json!({"orderId": "b3", "orderStatus": "Rejected", "cumExecQty": "0", "cumExecValue": "0", "rejectReason": "EC_InsufficientBalance"});
        let r = bybit_report(&row, &btc(), "qs-6");
        assert_eq!(r.status, OrderStatus::Rejected);
        assert_eq!(r.message.as_deref(), Some("EC_InsufficientBalance"));
    }

    #[test]
    fn instrument_lists_parse_both_venues() {
        let sym = json!({"symbol": "ETHBTC", "status": "TRADING", "baseAsset": "ETH", "quoteAsset": "BTC", "filters": [
            {"filterType": "LOT_SIZE", "stepSize": "0.0001", "minQty": "0.0001"},
            {"filterType": "NOTIONAL", "minNotional": "0.0001"},
            {"filterType": "PRICE_FILTER", "tickSize": "0.00001"}]});
        let f = binance_filters_from(&sym).unwrap();
        assert_eq!((f.base.as_str(), f.quote.as_str(), f.qty_step, f.price_tick), ("ETH", "BTC", 0.0001, 0.00001));
        let row = json!({"symbol": "ETHUSDT", "baseCoin": "ETH", "quoteCoin": "USDT",
            "lotSizeFilter": {"basePrecision": "0.00001", "minOrderQty": "0.0001", "minOrderAmt": "5"}, "priceFilter": {"tickSize": "0.01"}});
        let f = bybit_filters_from(&row).unwrap();
        assert_eq!((f.min_notional, f.price_tick), (5.0, 0.01));
        assert_eq!(levels(&json!([["100.5", "2"], ["0", "1"], ["99", "0"]])), vec![(100.5, 2.0)]);
        assert_eq!(urlencode("qs3f-01"), "qs3f-01");
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn without_keys_nothing_private_is_sent() {
        let v = CexVenue::new("binance", "", "", true).unwrap();
        assert!(v.ready().unwrap_err().contains("no API key"));
        assert!(v.balances().unwrap_err().contains("no API key"));
        assert!(CexVenue::new("kraken", "k", "s", false).is_err());
        assert!(format!("{v:?}").contains("sandbox: true"));
    }

    // Live public halves — `cargo test -p tauri-plugin-algo -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_books_and_instruments_of_both_venues() {
        for provider in ["binance", "bybit"] {
            let v = CexVenue::new(provider, "", "", false).unwrap();
            let inst = v.instrument("BTC/USDT").unwrap();
            let book = v.order_book(&inst).unwrap();
            assert!(book.best_bid().unwrap() < book.best_ask().unwrap(), "{provider}");
            let all = v.instruments().unwrap();
            assert!(all.iter().any(|i| i.base() == "ETH" && i.quote() == "BTC"), "{provider}: ETH/BTC listed");
            println!("{provider}: {} instruments, BTC mid {:?}", all.len(), book.mid());
        }
    }
}
