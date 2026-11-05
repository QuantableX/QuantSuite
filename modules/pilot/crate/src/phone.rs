//! Phone access (round 4, card 1, docs/PLAN-QUANTPILOT.md §53): approvals and
//! messages from a phone's browser.
//!
//! A small web page served by this PC, off by default, on an address the user
//! chooses: the Tailscale address is recommended, a LAN address needs an
//! explicit choice (plain HTTP on the Wi-Fi), a public address or "every
//! interface" is refused. There is no relay and no account, and this is the
//! coding PC: the trading server still runs no agents.
//!
//! Three locks: the network (Tailscale), the pairing (an 8-digit one-time code,
//! then a long random token stored only as its hash) and the PIN, which anything
//! that makes an agent act needs (10 minutes, locked after wrong tries). Reading
//! needs the pairing only.
//!
//! The browser's own attacks: every request's Host must be this address (DNS
//! rebinding), a write needs our custom header and a matching Origin (cross-site
//! requests), the cookie is HttpOnly and SameSite=Strict, and the page has a
//! strict Content-Security-Policy and builds its DOM from text only.

use crate::db::{Database, PhoneAudit, PhoneDevice, ThreadMeta};
use crate::driver::Answer;
use crate::events::{Item, ItemKind};
use crate::threads::{self, CreateThread};
use crate::{contexts, PilotState};
use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Request, State as AxumState},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use qs_gateway::{net, pairing, pairing::Pairing, stepup, stepup::StepUps};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast;

pub const DEFAULT_PORT: u16 = 47110;
/// The vault entry holding the PIN's Argon2id hash.
const PIN_SECRET: &str = "pilot.phone-pin";
const COOKIE: &str = "qs_phone";
/// Every write carries this header, which a cross-site form cannot set.
const WRITE_HEADER: &str = "x-qs-phone";
const TOKEN_MAX_AGE: i64 = 365 * 24 * 3600;
/// An item's text on the phone; the whole of it is on the PC.
const TEXT_LIMIT: usize = 6000;
/// Tool calls shown per thread, newest first: the phone is for what matters.
const TOOLS_SHOWN: usize = 6;
const ITEMS_SHOWN: usize = 50;
const ROWS_SHOWN: usize = 30;
const CONTENT_SECURITY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const PAGE: &str = include_str!("phone_page/index.html");
const SCRIPT: &str = include_str!("phone_page/app.js");
const STYLE: &str = include_str!("phone_page/app.css");

/// What is saved in the settings. The commands below are the only way to change it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhoneSettings {
    pub enabled: bool,
    pub address: String,
    pub port: u16,
    /// The user knows a LAN address is plain HTTP on the Wi-Fi.
    pub lan_ok: bool,
}

/// One row of the phone's list.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRow {
    pub id: String,
    pub title: String,
    pub project: String,
    pub provider: String,
    pub running: bool,
    /// What the agent waits for, when it waits for the user.
    pub needs: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadView {
    pub id: String,
    pub title: String,
    pub project: String,
    pub provider: String,
    pub running: bool,
    pub items: Vec<PhoneItem>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneItem {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub title: String,
    pub status: String,
    pub request_id: Option<String>,
    pub allow_session: bool,
    pub questions: Value,
    /// A request the phone cannot answer: a form or web step of an MCP server.
    pub on_pc: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// What the web layer needs from QuantPilot: the tests give it a fake.
pub trait Backend: Send + Sync + 'static {
    fn threads(&self) -> Result<Vec<ThreadRow>, String>;
    fn thread(&self, id: &str) -> Result<ThreadView, String>;
    fn projects(&self) -> Vec<Project>;
    fn send(&self, id: &str, text: String) -> Result<(), String>;
    fn create(&self, project: &str, text: String) -> Result<String, String>;
    fn answer(&self, id: &str, request_id: &str, answer: Answer) -> Result<(), String>;
    fn stop(&self, id: &str) -> Result<(), String>;
    fn pin_hash(&self) -> Result<Option<String>, String>;
}

// ── The web layer ───────────────────────────────────────────────────────────

struct Shared {
    backend: Arc<dyn Backend>,
    db: Arc<Database>,
    pairing: Arc<Mutex<Pairing>>,
    stepups: Arc<Mutex<StepUps>>,
    /// The one Host header this listener answers to.
    authority: String,
    events: broadcast::Sender<String>,
    /// Turned off: open connections are refused too.
    stopped: AtomicBool,
}

struct ApiError(StatusCode, String, bool);

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self(status, message.into(), false)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1, "needPin": self.2 }))).into_response()
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|c| c.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

impl Shared {
    fn audit(&self, device: &str, action: &str, thread: Option<&str>, ok: bool, note: &str) {
        self.db.phone_audit_add(&PhoneAudit {
            at: now(),
            device: device.to_string(),
            action: action.to_string(),
            thread_id: thread.map(str::to_string),
            ok,
            note: note.chars().take(200).collect(),
        });
    }

    fn device(&self, headers: &HeaderMap) -> Result<PhoneDevice, ApiError> {
        let device = cookie_token(headers)
            .and_then(|t| self.db.phone_device_by_token(&sha256_hex(&t)))
            .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "This phone is not paired."))?;
        self.db.phone_device_seen(&device.id, now());
        Ok(device)
    }

    /// A paired phone with the PIN entered in the last ten minutes.
    fn acting_device(&self, headers: &HeaderMap) -> Result<PhoneDevice, ApiError> {
        let device = self.device(headers)?;
        let granted = self.stepups.lock().map(|s| s.granted(&device.id, Instant::now())).unwrap_or(false);
        if !granted {
            return Err(ApiError(StatusCode::FORBIDDEN, "Enter the PIN first.".into(), true));
        }
        Ok(device)
    }

    /// Run a backend call off the async workers, and write it to the audit.
    async fn act<T: Send + 'static>(
        self: &Arc<Self>,
        device: &PhoneDevice,
        action: &str,
        thread: Option<&str>,
        call: impl FnOnce(&dyn Backend) -> Result<T, String> + Send + 'static,
    ) -> Result<T, ApiError> {
        let backend = self.backend.clone();
        let result = tokio::task::spawn_blocking(move || call(backend.as_ref()))
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match result {
            Ok(v) => {
                self.audit(&device.name, action, thread, true, "");
                Ok(v)
            }
            Err(e) => {
                self.audit(&device.name, action, thread, false, &e);
                Err(ApiError::new(StatusCode::BAD_REQUEST, e))
            }
        }
    }
}

/// Host on every request, our header and the Origin on every write, and the
/// page's security headers on every answer.
async fn web_guard(AxumState(s): AxumState<Arc<Shared>>, req: Request, next: Next) -> Response {
    if s.stopped.load(Ordering::SeqCst) {
        return ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Phone access is off.").into_response();
    }
    let headers = req.headers();
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
    if !host.eq_ignore_ascii_case(&s.authority) {
        return ApiError::new(StatusCode::FORBIDDEN, "Wrong address.").into_response();
    }
    if req.method() != Method::GET {
        if !headers.contains_key(WRITE_HEADER) {
            return ApiError::new(StatusCode::FORBIDDEN, "Refused.").into_response();
        }
        let expected = format!("http://{}", s.authority);
        if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
            if !origin.eq_ignore_ascii_case(&expected) {
                return ApiError::new(StatusCode::FORBIDDEN, "Refused.").into_response();
            }
        }
    }
    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CONTENT_SECURITY));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn asset(mime: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, mime)], body).into_response()
}

async fn page() -> Response {
    asset("text/html; charset=utf-8", PAGE)
}
async fn script() -> Response {
    asset("text/javascript; charset=utf-8", SCRIPT)
}
async fn style() -> Response {
    asset("text/css; charset=utf-8", STYLE)
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    #[serde(default)]
    name: String,
}

async fn pair(AxumState(s): AxumState<Arc<Shared>>, Json(body): Json<PairBody>) -> Result<Response, ApiError> {
    let checked = s
        .pairing
        .lock()
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "pairing lock"))?
        .check(&body.code, Instant::now());
    if let Err(e) = checked {
        let (status, message) = match e {
            pairing::CodeError::NoCode | pairing::CodeError::Expired => {
                (StatusCode::FORBIDDEN, "No pairing code is open. Start pairing in QuantSuite on the PC.".to_string())
            }
            pairing::CodeError::Locked { .. } => (StatusCode::TOO_MANY_REQUESTS, e.to_string()),
            other => (StatusCode::FORBIDDEN, other.to_string()),
        };
        s.audit("?", "pair", None, false, &message);
        return Err(ApiError::new(status, message));
    }
    let name: String = body.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
    let name = if name.is_empty() { "Phone".to_string() } else { name };
    let at = now();
    let device = PhoneDevice { id: uuid::Uuid::new_v4().to_string(), name, created_at: at, last_seen: at };
    // Two random UUIDs: 244 random bits from the OS.
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    s.db.phone_device_add(&device, &sha256_hex(&token))
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    s.audit(&device.name, "pair", None, true, "");
    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={TOKEN_MAX_AGE}");
    let mut response = Json(json!({ "name": device.name })).into_response();
    let value = HeaderValue::from_str(&cookie).map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    response.headers_mut().insert(header::SET_COOKIE, value);
    Ok(response)
}

async fn me(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let device = s.device(&headers)?;
    let pin_ok = s.stepups.lock().map(|g| g.granted(&device.id, Instant::now())).unwrap_or(false);
    Ok(Json(json!({ "name": device.name, "pinOk": pin_ok })))
}

#[derive(Deserialize)]
struct PinBody {
    pin: String,
}

async fn pin(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Json(body): Json<PinBody>) -> Result<Json<Value>, ApiError> {
    let device = s.device(&headers)?;
    let start = Instant::now();
    let locked = s.stepups.lock().map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "pin lock"))?.check_locked(&device.id, start);
    if let Err(e) = locked {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, e.to_string()));
    }
    let backend = s.backend.clone();
    let right = tokio::task::spawn_blocking(move || match backend.pin_hash() {
        Ok(Some(hash)) => Ok(stepup::verify_pin(&hash, &body.pin)),
        Ok(None) => Err("No PIN is set. Set one in QuantSuite on the PC.".to_string()),
        Err(e) => Err(e),
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let recorded = s.stepups.lock().map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "pin lock"))?.record(&device.id, right, Instant::now());
    match recorded {
        Ok(valid) => {
            s.audit(&device.name, "pin", None, true, "");
            Ok(Json(json!({ "validSeconds": valid.as_secs() })))
        }
        Err(e) => {
            s.audit(&device.name, "pin", None, false, "wrong PIN");
            let status = if matches!(e, stepup::StepUpError::Locked { .. }) { StatusCode::TOO_MANY_REQUESTS } else { StatusCode::FORBIDDEN };
            Err(ApiError::new(status, e.to_string()))
        }
    }
}

async fn list_threads(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap) -> Result<Json<Vec<ThreadRow>>, ApiError> {
    s.device(&headers)?;
    let backend = s.backend.clone();
    let rows = tokio::task::spawn_blocking(move || backend.threads())
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(rows))
}

async fn view_thread(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<ThreadView>, ApiError> {
    s.device(&headers)?;
    let backend = s.backend.clone();
    let view = tokio::task::spawn_blocking(move || backend.thread(&id))
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| ApiError::new(StatusCode::NOT_FOUND, e))?;
    Ok(Json(view))
}

async fn list_projects(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap) -> Result<Json<Vec<Project>>, ApiError> {
    s.device(&headers)?;
    Ok(Json(s.backend.projects()))
}

#[derive(Deserialize)]
struct MessageBody {
    text: String,
}

fn text_of(text: String) -> Result<String, ApiError> {
    if text.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "The message is empty."));
    }
    Ok(text)
}

async fn message(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Path(id): Path<String>, Json(body): Json<MessageBody>) -> Result<Json<Value>, ApiError> {
    let device = s.acting_device(&headers)?;
    let text = text_of(body.text)?;
    let thread = id.clone();
    s.act(&device, "message", Some(&id), move |b| b.send(&thread, text)).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct NewThreadBody {
    project: String,
    text: String,
}

async fn new_thread(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Json(body): Json<NewThreadBody>) -> Result<Json<Value>, ApiError> {
    let device = s.acting_device(&headers)?;
    let text = text_of(body.text)?;
    let id = s.act(&device, "new thread", None, move |b| b.create(&body.project, text)).await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnswerBody {
    request_id: String,
    answer: Value,
}

/// What a phone may say: not files, not the form of an MCP server.
fn phone_answer(raw: Value) -> Result<Answer, String> {
    let answer: Answer = serde_json::from_value(raw).map_err(|e| format!("Not an answer: {e}"))?;
    match &answer {
        Answer::Answers { files, .. } if !files.is_empty() => Err("Files can only be attached on the PC.".into()),
        Answer::Elicitation { .. } => Err("This one has to be answered on the PC.".into()),
        _ => Ok(answer),
    }
}

async fn answer(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Path(id): Path<String>, Json(body): Json<AnswerBody>) -> Result<Json<Value>, ApiError> {
    let device = s.acting_device(&headers)?;
    let parsed = phone_answer(body.answer).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let action = match parsed {
        Answer::Allow => "allow",
        Answer::AllowSession => "allow for the session",
        Answer::Deny { .. } => "deny",
        Answer::Dismiss => "dismiss",
        _ => "answer",
    };
    let thread = id.clone();
    let request = body.request_id;
    s.act(&device, action, Some(&id), move |b| b.answer(&thread, &request, parsed)).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn stop(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let device = s.acting_device(&headers)?;
    let thread = id.clone();
    s.act(&device, "stop", Some(&id), move |b| b.stop(&thread)).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Server-sent events: a thread changed, look again. No content travels here,
/// and a revoked phone or a stopped listener ends the stream.
async fn events(AxumState(s): AxumState<Arc<Shared>>, headers: HeaderMap) -> Result<Response, ApiError> {
    let device = s.device(&headers)?;
    let rx = s.events.subscribe();
    let stream = futures_util::stream::unfold((rx, s, device.id), |(mut rx, s, id)| async move {
        let line = match tokio::time::timeout(Duration::from_secs(20), rx.recv()).await {
            Ok(Ok(thread)) => format!("data: {thread}\n\n"),
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => "data: *\n\n".to_string(),
            Ok(Err(broadcast::error::RecvError::Closed)) => return None,
            Err(_) => ": still here\n\n".to_string(),
        };
        if s.stopped.load(Ordering::SeqCst) || !s.db.phone_device_live(&id) {
            return None;
        }
        Some((Ok::<Bytes, Infallible>(Bytes::from(line)), (rx, s, id)))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(stream))
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/app.js", get(script))
        .route("/app.css", get(style))
        .route("/api/pair", post(pair))
        .route("/api/me", get(me))
        .route("/api/pin", post(pin))
        .route("/api/projects", get(list_projects))
        .route("/api/threads", get(list_threads).post(new_thread))
        .route("/api/threads/{id}", get(view_thread))
        .route("/api/threads/{id}/messages", post(message))
        .route("/api/threads/{id}/answer", post(answer))
        .route("/api/threads/{id}/stop", post(stop))
        .route("/api/events", get(events))
        .layer(middleware::from_fn_with_state(shared.clone(), web_guard))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .with_state(shared)
}

// ── From QuantPilot's items to the phone's ──────────────────────────────────

fn clipped(text: &str) -> String {
    if text.chars().count() <= TEXT_LIMIT {
        return text.to_string();
    }
    let head: String = text.chars().take(TEXT_LIMIT).collect();
    format!("{head}\n\n… the rest is on the PC.")
}

/// What a phone shows of a thread: every message and request, and only the
/// newest tool calls. Never the reasoning or the attachments.
pub fn phone_items(items: &[Item]) -> Vec<PhoneItem> {
    let tools: Vec<&str> = items.iter().filter(|i| i.kind == ItemKind::Tool).map(|i| i.id.as_str()).collect();
    let kept_tools = &tools[tools.len().saturating_sub(TOOLS_SHOWN)..];
    let s = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out: Vec<PhoneItem> = Vec::new();
    for item in items {
        let d = &item.data;
        let base = PhoneItem {
            id: item.id.clone(),
            kind: item.kind.as_str().to_string(),
            status: s(d, "status"),
            request_id: d.get("requestId").and_then(Value::as_str).map(str::to_string),
            questions: Value::Null,
            ..Default::default()
        };
        let row = match item.kind {
            ItemKind::User | ItemKind::Assistant | ItemKind::Notice | ItemKind::Error => {
                let text = s(d, "text");
                if text.trim().is_empty() {
                    continue;
                }
                PhoneItem { text: clipped(&text), ..base }
            }
            ItemKind::Tool => {
                if !kept_tools.contains(&item.id.as_str()) {
                    continue;
                }
                PhoneItem { title: s(d, "title"), ..base }
            }
            ItemKind::Approval => {
                let title = s(d, "title");
                PhoneItem {
                    title: if title.is_empty() { s(d, "toolName") } else { title },
                    text: clipped(&s(d, "input")),
                    allow_session: d.get("allowSession").and_then(Value::as_bool).unwrap_or(false),
                    ..base
                }
            }
            ItemKind::Question => {
                if d.get("elicitation").is_some() {
                    PhoneItem { on_pc: true, title: "A form from a connected tool".into(), ..base }
                } else {
                    PhoneItem { questions: d.get("questions").cloned().unwrap_or(json!([])), ..base }
                }
            }
            ItemKind::Plan => PhoneItem { text: clipped(&s(d, "plan")), ..base },
            ItemKind::Changes => PhoneItem { text: "Files changed. The diff is on the PC.".into(), ..base },
            ItemKind::Page => PhoneItem { text: "The agent rendered a page. It is on the PC.".into(), ..base },
            ItemKind::Reasoning | ItemKind::Todo => continue,
        };
        out.push(row);
    }
    let from = out.len().saturating_sub(ITEMS_SHOWN);
    out.split_off(from)
}

/// The phone's list: what waits for the user first, then what runs, then the
/// most recent. Side chats and settled or snoozed threads stay on the PC,
/// unless they wait for the user.
pub fn phone_rows(metas: Vec<ThreadMeta>, running: &dyn Fn(&str) -> bool, needs: &dyn Fn(&str) -> Option<String>, at: i64) -> Vec<ThreadRow> {
    let mut rows: Vec<ThreadRow> = metas
        .into_iter()
        .filter(|m| m.kind == "chat" && m.parent_id.is_none())
        .filter_map(|m| {
            let needs = needs(&m.id);
            let is_running = running(&m.id);
            let hidden = m.settled_at.is_some() || m.snoozed_until.is_some_and(|until| until > at);
            if hidden && needs.is_none() && !is_running {
                return None;
            }
            Some(ThreadRow {
                title: if m.title.trim().is_empty() { "New thread".into() } else { m.title },
                project: m.context_name,
                provider: m.provider,
                running: is_running,
                needs,
                updated_at: m.last_activity_at.max(m.updated_at),
                id: m.id,
            })
        })
        .collect();
    let rank = |r: &ThreadRow| if r.needs.is_some() { 0 } else if r.running { 1 } else { 2 };
    rows.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.updated_at.cmp(&a.updated_at)));
    rows.truncate(ROWS_SHOWN);
    rows
}

/// The address rules: refuses what would open the PC to more than the user chose.
pub fn check_address(address: &str, port: u16, lan_ok: bool) -> Result<SocketAddr, String> {
    let ip: IpAddr = address.trim().parse().map_err(|_| "Choose an address.".to_string())?;
    let addr = SocketAddr::new(ip, port);
    net::check_listen(&addr, false)?;
    if ip.is_unspecified() {
        return Err("Phone access never listens on every network. Choose one address.".into());
    }
    match net::kind_of(&ip) {
        "tailnet" | "loopback" => Ok(addr),
        "lan" if lan_ok => Ok(addr),
        "lan" => Err("A local network address is plain HTTP: anyone on the same Wi-Fi could read the PIN. Tick the box to accept that, or use Tailscale.".into()),
        _ => Err("This looks like a public address, and phone access never listens on one. Use your Tailscale address.".into()),
    }
}

fn authority_of(addr: &SocketAddr) -> String {
    addr.to_string()
}

// ── In the app ──────────────────────────────────────────────────────────────

struct AppBackend {
    app: AppHandle,
}

impl AppBackend {
    fn state(&self) -> State<'_, PilotState> {
        self.app.state::<PilotState>()
    }
}

impl Backend for AppBackend {
    fn threads(&self) -> Result<Vec<ThreadRow>, String> {
        let state = self.state();
        let metas = state.db.list_sessions().map_err(|e| e.to_string())?;
        Ok(phone_rows(
            metas,
            &|id| state.runtime.active_turn(id).is_some(),
            &|id| state.db.pending_request_line(id),
            now(),
        ))
    }

    fn thread(&self, id: &str) -> Result<ThreadView, String> {
        let state = self.state();
        let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
        let (items, _) = state.runtime.snapshot(id, None, 160).map_err(|e| e.to_string())?;
        Ok(ThreadView {
            id: meta.id,
            title: if meta.title.trim().is_empty() { "New thread".into() } else { meta.title },
            project: meta.context_name,
            provider: meta.provider,
            running: state.runtime.active_turn(id).is_some(),
            items: phone_items(&items),
        })
    }

    fn projects(&self) -> Vec<Project> {
        contexts::list(&self.app).into_iter().map(|c| Project { id: c.id, name: c.name }).collect()
    }

    fn send(&self, id: &str, text: String) -> Result<(), String> {
        let state = self.state();
        // While the agent works, a message waits its turn, as on the PC.
        if state.runtime.active_turn(id).is_some() {
            return threads::pilot_queue_add(self.app.clone(), state, id.to_string(), text, None, None).map(|_| ());
        }
        threads::pilot_thread_send(self.app.clone(), state, id.to_string(), text, None, None, None).map(|_| ())
    }

    fn create(&self, project: &str, text: String) -> Result<String, String> {
        let state = self.state();
        let meta = threads::create(
            &self.app,
            state.inner(),
            CreateThread { context_id: Some(project.to_string()), ..Default::default() },
        )?;
        let _ = self.app.emit("pilot-thread-created", &meta);
        threads::start_turn(&self.app, state.inner(), &meta.id, text, Vec::new())?;
        Ok(meta.id)
    }

    fn answer(&self, id: &str, request_id: &str, answer: Answer) -> Result<(), String> {
        threads::pilot_thread_answer(self.app.clone(), self.state(), id.to_string(), request_id.to_string(), answer)
    }

    fn stop(&self, id: &str) -> Result<(), String> {
        threads::pilot_thread_interrupt(self.state(), id.to_string())
    }

    fn pin_hash(&self) -> Result<Option<String>, String> {
        read_pin_hash()
    }
}

fn read_pin_hash() -> Result<Option<String>, String> {
    qs_core::vault::with_vault(|v| Ok(v.get_string(PIN_SECRET)?.map(|s| s.to_string())))
}

struct Running {
    shared: Arc<Shared>,
    task: tauri::async_runtime::JoinHandle<()>,
    url: String,
}

/// Managed by the app: the listener and what outlives it.
pub struct PhoneState {
    events: broadcast::Sender<String>,
    pairing: Arc<Mutex<Pairing>>,
    stepups: Arc<Mutex<StepUps>>,
    running: Mutex<Option<Running>>,
    error: Mutex<Option<String>>,
}

impl Default for PhoneState {
    fn default() -> Self {
        Self {
            events: broadcast::channel(64).0,
            pairing: Arc::new(Mutex::new(Pairing::new())),
            stepups: Arc::new(Mutex::new(StepUps::default())),
            running: Mutex::new(None),
            error: Mutex::new(None),
        }
    }
}

impl PhoneState {
    /// A thread changed: an open page looks again. Nothing is sent when no page is open.
    pub fn ping(&self, thread_id: &str) {
        let _ = self.events.send(thread_id.to_string());
    }

    fn stop(&self) {
        if let Ok(mut running) = self.running.lock() {
            if let Some(r) = running.take() {
                r.shared.stopped.store(true, Ordering::SeqCst);
                let _ = self.events.send("*".to_string());
                r.task.abort();
            }
        }
    }

    fn start(&self, app: &AppHandle, db: Arc<Database>, addr: SocketAddr) -> Result<(), String> {
        self.stop();
        if !net::has_address(&addr.ip()) {
            return Err(format!("{} is not an address of this PC right now.", addr.ip()));
        }
        if read_pin_hash()?.is_none() {
            return Err("Set a PIN first.".into());
        }
        let listener = std::net::TcpListener::bind(addr).map_err(|e| format!("Could not listen on {addr}: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let shared = Arc::new(Shared {
            backend: Arc::new(AppBackend { app: app.clone() }),
            db,
            pairing: self.pairing.clone(),
            stepups: self.stepups.clone(),
            authority: authority_of(&addr),
            events: self.events.clone(),
            stopped: AtomicBool::new(false),
        });
        let service = router(shared.clone());
        let task = tauri::async_runtime::spawn(async move {
            match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => {
                    if let Err(e) = axum::serve(listener, service).await {
                        eprintln!("pilot: phone access stopped: {e}");
                    }
                }
                Err(e) => eprintln!("pilot: phone access could not start: {e}"),
            }
        });
        let url = format!("http://{}/", authority_of(&addr));
        *self.running.lock().map_err(|_| "phone lock")? = Some(Running { shared, task, url });
        Ok(())
    }

    fn url(&self) -> Option<String> {
        self.running.lock().ok().and_then(|r| r.as_ref().map(|r| r.url.clone()))
    }

    fn set_error(&self, error: Option<String>) {
        if let Ok(mut e) = self.error.lock() {
            *e = error;
        }
    }
}

/// At the app's start: on again if the user left it on. A failure is shown in
/// Settings, and never falls back to another address.
pub fn start_saved(app: &AppHandle) {
    let (Some(phone), Some(pilot)) = (app.try_state::<PhoneState>(), app.try_state::<PilotState>()) else { return };
    let saved = lock_settings(&pilot).phone.clone();
    if !saved.enabled {
        return;
    }
    let started = check_address(&saved.address, port_of(&saved), saved.lan_ok).and_then(|addr| phone.start(app, pilot.db.clone(), addr));
    phone.set_error(started.err());
}

fn lock_settings<'a>(state: &'a PilotState) -> std::sync::MutexGuard<'a, crate::settings::Settings> {
    state.settings.lock().unwrap_or_else(|p| p.into_inner())
}

fn port_of(saved: &PhoneSettings) -> u16 {
    if saved.port == 0 {
        DEFAULT_PORT
    } else {
        saved.port
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingView {
    code: String,
    seconds_left: u64,
    url: String,
    qr: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStatus {
    settings: PhoneSettings,
    default_port: u16,
    running: bool,
    url: Option<String>,
    error: Option<String>,
    has_pin: bool,
    addresses: Vec<net::Interface>,
    tailnet: net::Tailnet,
    pairing: Option<PairingView>,
    devices: Vec<PhoneDevice>,
    audit: Vec<PhoneAudit>,
}

fn status_of(pilot: &PilotState, phone: &PhoneState) -> Result<PhoneStatus, String> {
    let settings = lock_settings(pilot).phone.clone();
    let url = phone.url();
    let pairing = {
        let open = phone.pairing.lock().map_err(|_| "pairing lock")?.open(Instant::now());
        match (open, url.as_ref()) {
            (Some((code, left)), Some(base)) => {
                let link = format!("{base}#pair={code}");
                Some(PairingView { qr: pairing::qr_svg(&link), url: link, code, seconds_left: left.as_secs() })
            }
            _ => None,
        }
    };
    Ok(PhoneStatus {
        default_port: DEFAULT_PORT,
        running: url.is_some(),
        url,
        error: phone.error.lock().ok().and_then(|e| e.clone()),
        has_pin: read_pin_hash().unwrap_or(None).is_some(),
        addresses: net::interfaces().into_iter().filter(|i| i.kind != "loopback").collect(),
        tailnet: net::tailscale_status(),
        pairing,
        devices: pilot.db.phone_devices().map_err(|e| e.to_string())?,
        audit: pilot.db.phone_audit_list(30).map_err(|e| e.to_string())?,
        settings,
    })
}

#[tauri::command(async)]
pub fn pilot_phone_status(pilot: State<'_, PilotState>, phone: State<'_, PhoneState>) -> Result<PhoneStatus, String> {
    status_of(&pilot, &phone)
}

/// Turn phone access on or off, on the chosen address.
#[tauri::command(async)]
pub fn pilot_phone_set(app: AppHandle, pilot: State<'_, PilotState>, phone: State<'_, PhoneState>, settings: PhoneSettings) -> Result<PhoneStatus, String> {
    let mut wanted = settings;
    if wanted.port == 0 {
        wanted.port = DEFAULT_PORT;
    }
    if wanted.enabled {
        let addr = check_address(&wanted.address, wanted.port, wanted.lan_ok)?;
        phone.start(&app, pilot.db.clone(), addr)?;
    } else {
        phone.stop();
        if let Ok(mut p) = phone.pairing.lock() {
            p.cancel();
        }
    }
    phone.set_error(None);
    {
        let mut s = lock_settings(&pilot);
        s.phone = wanted;
        crate::settings::write(&pilot.data_dir, &s)?;
    }
    status_of(&pilot, &phone)
}

/// The PIN: only its hash is kept, in the vault. A new PIN ends every phone's grant.
#[tauri::command(async)]
pub fn pilot_phone_set_pin(pilot: State<'_, PilotState>, phone: State<'_, PhoneState>, pin: String) -> Result<PhoneStatus, String> {
    stepup::check_pin_format(&pin).map_err(|_| "The PIN has 6 to 12 digits.".to_string())?;
    let hash = stepup::hash_pin(&pin)?;
    qs_core::vault::with_vault(|v| v.set(PIN_SECRET, hash.as_bytes()).map(|_| ()))?;
    if let Ok(mut s) = phone.stepups.lock() {
        s.clear_grants();
    }
    status_of(&pilot, &phone)
}

#[tauri::command(async)]
pub fn pilot_phone_pair(pilot: State<'_, PilotState>, phone: State<'_, PhoneState>) -> Result<PhoneStatus, String> {
    if phone.url().is_none() {
        return Err("Turn phone access on first.".into());
    }
    phone.pairing.lock().map_err(|_| "pairing lock")?.start(Instant::now()).map_err(|e| e.to_string())?;
    status_of(&pilot, &phone)
}

#[tauri::command(async)]
pub fn pilot_phone_pair_cancel(pilot: State<'_, PilotState>, phone: State<'_, PhoneState>) -> Result<PhoneStatus, String> {
    phone.pairing.lock().map_err(|_| "pairing lock")?.cancel();
    status_of(&pilot, &phone)
}

/// Cuts the phone off at once, an open page included.
#[tauri::command(async)]
pub fn pilot_phone_revoke(pilot: State<'_, PilotState>, phone: State<'_, PhoneState>, id: String) -> Result<PhoneStatus, String> {
    pilot.db.phone_device_revoke(&id, now()).map_err(|e| e.to_string())?;
    if let Ok(mut s) = phone.stepups.lock() {
        s.forget(&id);
    }
    phone.ping("*");
    status_of(&pilot, &phone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[derive(Default)]
    struct Fake {
        sent: Mutex<Vec<(String, String)>>,
        answers: Mutex<Vec<(String, String, String)>>,
        stops: AtomicUsize,
        created: Mutex<Vec<(String, String)>>,
        pin: Mutex<Option<String>>,
    }

    impl Backend for Fake {
        fn threads(&self) -> Result<Vec<ThreadRow>, String> {
            Ok(vec![ThreadRow {
                id: "t1".into(),
                title: "A thread".into(),
                project: "General".into(),
                provider: "claude".into(),
                running: false,
                needs: Some("Run a command".into()),
                updated_at: 1,
            }])
        }
        fn thread(&self, id: &str) -> Result<ThreadView, String> {
            if id != "t1" {
                return Err("unknown thread".into());
            }
            Ok(ThreadView { id: id.into(), title: "A thread".into(), project: "General".into(), provider: "claude".into(), running: false, items: Vec::new() })
        }
        fn projects(&self) -> Vec<Project> {
            vec![Project { id: "general".into(), name: "General".into() }]
        }
        fn send(&self, id: &str, text: String) -> Result<(), String> {
            self.sent.lock().unwrap().push((id.into(), text));
            Ok(())
        }
        fn create(&self, project: &str, text: String) -> Result<String, String> {
            self.created.lock().unwrap().push((project.into(), text));
            Ok("t2".into())
        }
        fn answer(&self, id: &str, request_id: &str, answer: Answer) -> Result<(), String> {
            self.answers.lock().unwrap().push((id.into(), request_id.into(), format!("{answer:?}")));
            Ok(())
        }
        fn stop(&self, _id: &str) -> Result<(), String> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn pin_hash(&self) -> Result<Option<String>, String> {
            Ok(self.pin.lock().unwrap().clone())
        }
    }

    struct Server {
        base: String,
        shared: Arc<Shared>,
        fake: Arc<Fake>,
        client: reqwest::Client,
    }

    async fn server() -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fake = Arc::new(Fake { pin: Mutex::new(Some(stepup::hash_pin("123456").unwrap())), ..Default::default() });
        let shared = Arc::new(Shared {
            backend: fake.clone(),
            db: Arc::new(Database::open_in_memory().unwrap()),
            pairing: Arc::new(Mutex::new(Pairing::new())),
            stepups: Arc::new(Mutex::new(StepUps::default())),
            authority: authority_of(&addr),
            events: broadcast::channel(16).0,
            stopped: AtomicBool::new(false),
        });
        let service = router(shared.clone());
        tokio::spawn(async move {
            axum::serve(listener, service).await.unwrap();
        });
        Server { base: format!("http://{addr}"), shared, fake, client: reqwest::Client::new() }
    }

    impl Server {
        fn open_code(&self) -> String {
            self.shared.pairing.lock().unwrap().start(Instant::now()).unwrap()
        }

        async fn post(&self, path: &str, cookie: Option<&str>, body: Value) -> reqwest::Response {
            let mut req = self.client.post(format!("{}{path}", self.base)).header(WRITE_HEADER, "1").json(&body);
            if let Some(c) = cookie {
                req = req.header(header::COOKIE, c);
            }
            req.send().await.unwrap()
        }

        async fn get(&self, path: &str, cookie: Option<&str>) -> reqwest::Response {
            let mut req = self.client.get(format!("{}{path}", self.base));
            if let Some(c) = cookie {
                req = req.header(header::COOKIE, c);
            }
            req.send().await.unwrap()
        }

        /// A paired phone's cookie.
        async fn paired(&self) -> String {
            let code = self.open_code();
            let r = self.post("/api/pair", None, json!({ "code": code, "name": "My phone" })).await;
            assert_eq!(r.status(), 200);
            let set = r.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
            assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict"), "{set}");
            set.split(';').next().unwrap().to_string()
        }

        async fn unlocked(&self) -> String {
            let cookie = self.paired().await;
            assert_eq!(self.post("/api/pin", Some(&cookie), json!({ "pin": "123456" })).await.status(), 200);
            cookie
        }
    }

    #[tokio::test]
    async fn nothing_without_a_pairing() {
        let s = server().await;
        // The page itself is public: it has nothing in it.
        let page = s.get("/", None).await;
        assert_eq!(page.status(), 200);
        assert!(page.headers().get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().contains("script-src 'self'"));
        assert_eq!(page.headers().get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
        for path in ["/api/me", "/api/threads", "/api/threads/t1", "/api/projects", "/api/events"] {
            assert_eq!(s.get(path, None).await.status(), 401, "{path}");
            assert_eq!(s.get(path, Some("qs_phone=guess")).await.status(), 401, "{path} with a guessed token");
        }
        for path in ["/api/threads/t1/messages", "/api/threads/t1/stop", "/api/threads/t1/answer", "/api/pin", "/api/threads"] {
            let r = s.post(path, None, json!({ "text": "x", "pin": "123456", "project": "general", "requestId": "r", "answer": { "decision": "allow" } })).await;
            assert_eq!(r.status(), 401, "{path}");
        }
        assert!(s.fake.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_wrong_host_and_a_foreign_site_are_refused() {
        let s = server().await;
        let cookie = s.unlocked().await;
        // DNS rebinding: another name pointing at this address.
        let r = s.client.get(format!("{}/api/threads", s.base)).header(header::HOST, "evil.example").header(header::COOKIE, &cookie).send().await.unwrap();
        assert_eq!(r.status(), 403);
        let r = s.client.get(format!("{}/", s.base)).header(header::HOST, "evil.example").send().await.unwrap();
        assert_eq!(r.status(), 403, "not even the page");
        // A write from another site.
        let r = s.client.post(format!("{}/api/threads/t1/messages", s.base)).header(WRITE_HEADER, "1").header(header::ORIGIN, "https://evil.example").header(header::COOKIE, &cookie).json(&json!({ "text": "x" })).send().await.unwrap();
        assert_eq!(r.status(), 403);
        // A plain form cannot set our header.
        let r = s.client.post(format!("{}/api/threads/t1/messages", s.base)).header(header::COOKIE, &cookie).json(&json!({ "text": "x" })).send().await.unwrap();
        assert_eq!(r.status(), 403);
        assert!(s.fake.sent.lock().unwrap().is_empty());
        // The real page's write goes through.
        let r = s.client.post(format!("{}/api/threads/t1/messages", s.base)).header(WRITE_HEADER, "1").header(header::ORIGIN, &s.base).header(header::COOKIE, &cookie).json(&json!({ "text": "x" })).send().await.unwrap();
        assert_eq!(r.status(), 200);
    }

    #[tokio::test]
    async fn pairing_is_one_time_and_locks_after_wrong_codes() {
        let s = server().await;
        let r = s.post("/api/pair", None, json!({ "code": "00000000" })).await;
        assert_eq!(r.status(), 403, "no code open");

        let code = s.open_code();
        let wrong = if code == "00000000" { "11111111" } else { "00000000" };
        for _ in 0..4 {
            assert_eq!(s.post("/api/pair", None, json!({ "code": wrong })).await.status(), 403);
        }
        let locked = s.post("/api/pair", None, json!({ "code": wrong })).await;
        assert_eq!(locked.status(), 429);
        // The lock holds for the right code too.
        assert_eq!(s.post("/api/pair", None, json!({ "code": code })).await.status(), 429);

        // A fresh listener's code works once.
        let s = server().await;
        let code = s.open_code();
        assert_eq!(s.post("/api/pair", None, json!({ "code": code })).await.status(), 200);
        assert_eq!(s.post("/api/pair", None, json!({ "code": code })).await.status(), 403, "used up");
        assert_eq!(s.shared.db.phone_devices().unwrap()[0].name, "Phone", "a name is optional");
    }

    #[tokio::test]
    async fn the_token_is_stored_only_as_its_hash() {
        let s = server().await;
        let cookie = s.paired().await;
        let token = cookie.split('=').nth(1).unwrap();
        assert!(token.len() >= 64);
        assert!(s.shared.db.phone_device_by_token(token).is_none());
        assert!(s.shared.db.phone_device_by_token(&sha256_hex(token)).is_some());
    }

    #[tokio::test]
    async fn acting_needs_the_pin_and_reading_does_not() {
        let s = server().await;
        let cookie = s.paired().await;
        assert_eq!(s.get("/api/threads", Some(&cookie)).await.status(), 200);
        let view = s.get("/api/threads/t1", Some(&cookie)).await;
        assert_eq!(view.status(), 200);

        for (path, body) in [
            ("/api/threads/t1/messages", json!({ "text": "go" })),
            ("/api/threads/t1/answer", json!({ "requestId": "r1", "answer": { "decision": "allow" } })),
            ("/api/threads/t1/stop", json!({})),
            ("/api/threads", json!({ "project": "general", "text": "go" })),
        ] {
            let r = s.post(path, Some(&cookie), body).await;
            assert_eq!(r.status(), 403, "{path}");
            let v: Value = r.json().await.unwrap();
            assert_eq!(v["needPin"], true, "{path}");
        }
        assert!(s.fake.sent.lock().unwrap().is_empty());
        assert_eq!(s.fake.stops.load(Ordering::SeqCst), 0);

        // Four wrong PINs, then the right one: it works and the count resets.
        for left in [4, 3, 2, 1] {
            let r = s.post("/api/pin", Some(&cookie), json!({ "pin": "000000" })).await;
            assert_eq!(r.status(), 403);
            let v: Value = r.json().await.unwrap();
            assert!(v["error"].as_str().unwrap().contains(&format!("{left} attempts left")), "{v}");
        }
        assert_eq!(s.post("/api/pin", Some(&cookie), json!({ "pin": "123456" })).await.status(), 200);

        assert_eq!(s.post("/api/threads/t1/messages", Some(&cookie), json!({ "text": "go" })).await.status(), 200);
        assert_eq!(s.fake.sent.lock().unwrap()[0], ("t1".to_string(), "go".to_string()));
        assert_eq!(s.post("/api/threads/t1/answer", Some(&cookie), json!({ "requestId": "r1", "answer": { "decision": "allow" } })).await.status(), 200);
        assert_eq!(s.fake.answers.lock().unwrap()[0].2, "Allow");
        assert_eq!(s.post("/api/threads/t1/stop", Some(&cookie), json!({})).await.status(), 200);
        assert_eq!(s.fake.stops.load(Ordering::SeqCst), 1);
        let created: Value = s.post("/api/threads", Some(&cookie), json!({ "project": "general", "text": "go" })).await.json().await.unwrap();
        assert_eq!(created["id"], "t2");

        let audit = s.shared.db.phone_audit_list(20).unwrap();
        let actions: Vec<&str> = audit.iter().map(|a| a.action.as_str()).collect();
        for a in ["pair", "pin", "message", "allow", "stop", "new thread"] {
            assert!(actions.contains(&a), "{a} in {actions:?}");
        }
        assert!(audit.iter().all(|a| !a.note.contains("123456")));
    }

    #[tokio::test]
    async fn five_wrong_pins_lock_the_phone() {
        let s = server().await;
        let cookie = s.paired().await;
        for _ in 0..4 {
            s.post("/api/pin", Some(&cookie), json!({ "pin": "000000" })).await;
        }
        assert_eq!(s.post("/api/pin", Some(&cookie), json!({ "pin": "000000" })).await.status(), 429);
        assert_eq!(s.post("/api/pin", Some(&cookie), json!({ "pin": "123456" })).await.status(), 429, "the right PIN waits too");
        assert_eq!(s.post("/api/threads/t1/messages", Some(&cookie), json!({ "text": "go" })).await.status(), 403);
    }

    #[tokio::test]
    async fn revoking_cuts_the_phone_off_at_once_and_ends_its_events() {
        let s = server().await;
        let cookie = s.unlocked().await;
        assert_eq!(s.post("/api/threads/t1/messages", Some(&cookie), json!({ "text": "go" })).await.status(), 200);

        let mut stream = s.get("/api/events", Some(&cookie)).await;
        assert_eq!(stream.status(), 200);
        assert_eq!(stream.headers().get(header::CONTENT_TYPE).unwrap(), "text/event-stream");
        s.shared.events.send("t1".into()).unwrap();
        let chunk = stream.chunk().await.unwrap().unwrap();
        assert_eq!(&chunk[..], b"data: t1\n\n");

        let id = s.shared.db.phone_devices().unwrap()[0].id.clone();
        assert!(s.shared.db.phone_device_revoke(&id, 1).unwrap());
        s.shared.events.send("*".into()).unwrap();
        assert!(stream.chunk().await.unwrap().is_none(), "the open stream ended");

        assert_eq!(s.get("/api/threads", Some(&cookie)).await.status(), 401);
        assert_eq!(s.post("/api/threads/t1/messages", Some(&cookie), json!({ "text": "again" })).await.status(), 401);
        assert_eq!(s.fake.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn turning_it_off_refuses_open_connections_too() {
        let s = server().await;
        let cookie = s.paired().await;
        assert_eq!(s.get("/api/threads", Some(&cookie)).await.status(), 200);
        s.shared.stopped.store(true, Ordering::SeqCst);
        assert_eq!(s.get("/api/threads", Some(&cookie)).await.status(), 503);
        assert_eq!(s.get("/", None).await.status(), 503);
    }

    #[tokio::test]
    async fn a_phone_cannot_send_files_or_answer_a_tool_form() {
        let s = server().await;
        let cookie = s.unlocked().await;
        let files = json!({ "requestId": "r", "answer": { "decision": "answers", "answers": { "Q": "A" }, "files": { "Q": [{ "kind": "file", "name": "x", "path": "C:/x" }] } } });
        assert_eq!(s.post("/api/threads/t1/answer", Some(&cookie), files).await.status(), 400);
        let form = json!({ "requestId": "r", "answer": { "decision": "elicitation", "action": "accept" } });
        assert_eq!(s.post("/api/threads/t1/answer", Some(&cookie), form).await.status(), 400);
        assert!(s.fake.answers.lock().unwrap().is_empty());
        let plain = json!({ "requestId": "r", "answer": { "decision": "answers", "answers": { "Q": "A" } } });
        assert_eq!(s.post("/api/threads/t1/answer", Some(&cookie), plain).await.status(), 200);
    }

    #[test]
    fn only_the_addresses_the_user_can_mean() {
        assert!(check_address("100.101.102.103", 47110, false).is_ok(), "Tailscale");
        assert!(check_address("127.0.0.1", 47110, false).is_ok(), "this PC only");
        assert!(check_address("192.168.1.20", 47110, false).unwrap_err().contains("same Wi-Fi"));
        assert!(check_address("192.168.1.20", 47110, true).is_ok(), "the user accepted it");
        assert!(check_address("8.8.8.8", 47110, true).unwrap_err().contains("public"));
        assert!(check_address("0.0.0.0", 47110, true).is_err());
        assert!(check_address("::", 47110, true).is_err());
        assert!(check_address("100.101.102.103", 3100, false).is_err(), "QuantMCP's port");
        assert!(check_address("not an address", 47110, false).is_err());
    }

    fn item(id: &str, kind: ItemKind, data: Value) -> Item {
        Item { id: id.into(), thread_id: "t".into(), turn_id: "u".into(), seq: 1, kind, parent: None, data, created_at: 1, updated_at: 1 }
    }

    #[test]
    fn a_phone_sees_messages_and_requests_and_only_the_newest_tools() {
        let mut items = vec![
            item("u", ItemKind::User, json!({ "text": "fix it" })),
            item("r", ItemKind::Reasoning, json!({ "text": "secret thoughts" })),
            item("todo", ItemKind::Todo, json!({ "items": [] })),
            item("empty", ItemKind::Assistant, json!({ "text": "  ", "streaming": true })),
        ];
        for n in 0..10 {
            items.push(item(&format!("tool{n}"), ItemKind::Tool, json!({ "title": format!("ls {n}"), "status": "completed" })));
        }
        items.push(item("a", ItemKind::Approval, json!({ "requestId": "r1", "toolName": "Bash", "title": "rm -rf x", "input": "rm -rf x", "allowSession": true, "status": "pending" })));
        items.push(item("q", ItemKind::Question, json!({ "requestId": "r2", "questions": [{ "question": "Which?" }], "status": "pending" })));
        items.push(item("f", ItemKind::Question, json!({ "requestId": "r3", "questions": [], "elicitation": { "x": 1 }, "status": "pending" })));
        items.push(item("p", ItemKind::Plan, json!({ "requestId": "r4", "plan": "Step 1", "status": "pending" })));
        items.push(item("c", ItemKind::Changes, json!({ "files": [] })));
        let out = phone_items(&items);
        assert!(!out.iter().any(|i| i.kind == "reasoning" || i.kind == "todo" || i.id == "empty"));
        assert_eq!(out.iter().filter(|i| i.kind == "tool").count(), TOOLS_SHOWN);
        assert!(!out.iter().any(|i| i.id == "tool3"), "older tools stay on the PC");
        assert!(out.iter().any(|i| i.id == "tool9"));
        let a = out.iter().find(|i| i.id == "a").unwrap();
        assert_eq!((a.request_id.as_deref(), a.title.as_str(), a.status.as_str(), a.allow_session), (Some("r1"), "rm -rf x", "pending", true));
        assert_eq!(out.iter().find(|i| i.id == "q").unwrap().questions[0]["question"], "Which?");
        assert!(out.iter().find(|i| i.id == "f").unwrap().on_pc);
        assert_eq!(out.iter().find(|i| i.id == "p").unwrap().text, "Step 1");

        let long = "x".repeat(TEXT_LIMIT + 5);
        let out = phone_items(&[item("l", ItemKind::Assistant, json!({ "text": long }))]);
        assert!(out[0].text.ends_with("the rest is on the PC."));
    }

    fn meta(id: &str, activity: i64) -> ThreadMeta {
        let mut m: ThreadMeta = serde_json::from_value(json!({
            "id": id, "provider": "claude", "providerSessionId": null, "title": id, "contextId": "general", "contextName": "General",
            "cwd": "", "model": "", "mode": "ask", "createdAt": 1, "updatedAt": 1, "costUsd": 0.0, "inputTokens": 0, "outputTokens": 0,
            "lastError": null, "kind": "chat", "effort": "", "titleSource": "auto", "state": "idle", "contextTokens": null, "contextWindow": null,
            "lastActivityAt": activity, "pinned": false, "sortKey": 0.0, "settledAt": null, "snoozedUntil": null, "autoSettle": true,
            "workspaceMode": "local", "baseBranch": "", "branch": "", "worktreePath": null
        }))
        .unwrap();
        m.last_activity_at = activity;
        m
    }

    #[test]
    fn what_needs_you_comes_first() {
        let mut settled = meta("settled", 90);
        settled.settled_at = Some(1);
        let mut settled_waiting = meta("settled-waiting", 1);
        settled_waiting.settled_at = Some(1);
        let mut snoozed = meta("snoozed", 95);
        snoozed.snoozed_until = Some(2_000);
        let mut side = meta("side", 99);
        side.parent_id = Some("main".into());
        let mut terminal = meta("terminal", 98);
        terminal.kind = "terminal".into();
        let rows = phone_rows(
            vec![meta("recent", 50), meta("older", 10), meta("running", 5), settled, settled_waiting, snoozed, side, terminal],
            &|id| id == "running",
            &|id| (id == "settled-waiting").then(|| "Run a command".to_string()),
            1_000,
        );
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["settled-waiting", "running", "recent", "older"]);
    }

    #[test]
    fn the_page_loads_nothing_from_elsewhere_and_builds_from_text() {
        for asset in [PAGE, SCRIPT, STYLE] {
            assert!(!asset.contains("https://") && !asset.contains("http://"), "no outside address");
        }
        assert!(!SCRIPT.contains("innerHTML"), "agent text can never become markup");
        assert!(!PAGE.contains("<script>"), "the script is a file, as the CSP wants");
        assert!(!PAGE.contains("style="));
    }
}
