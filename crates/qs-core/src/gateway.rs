//! The gateway on this installation (PLAN-SERVER-CLIENT §4, card Server 3/7).
//!
//! **On a server**: the [`Hub`] with the server's key (vault entry
//! [`SERVER_IDENTITY`]), the paired devices and the audit log
//! (`gateway.db`), listening on the address chosen under Settings → Server.
//! An authorized remote call runs through the main webview's IPC with the
//! app's own invoke key (§6.3, option B), so the ACL resolves exactly as for
//! the UI's own calls.
//!
//! **On any PC**: this PC's device key (vault entry [`DEVICE_IDENTITY`]) and its
//! connection profiles — the servers it paired with and their pinned keys
//! (core setting [`CONNECTIONS`]).
//!
//! **In client mode** (card Server 5/7; this PC uses one of those servers,
//! core setting [`ACTIVE`], role `client`): the [`Link`] keeps one connection
//! open — reconnecting with backoff and resuming events — re-emits the
//! server's events here so module code keeps its `listen` calls, and carries
//! the calls the frontend seam (`apps/shell/seam`) routes to the server.

use crate::db::{self, Db};
use crate::server::{self, Role};
use crate::vault;
use qs_gateway::authz::Effect;
use qs_gateway::client::{self, CallOptions, ClientEvent, ClientInfo, ConnectError, ConnectOptions, PairingAttempt, Session};
use qs_gateway::files::FileRoots;
use qs_gateway::protocol::{CallError, EventKind, ResumePoint};
use qs_gateway::net::{self, Interface};
use qs_gateway::registry::{AuditEntry, Device, Registry};
use qs_gateway::server::{CallFuture, Dispatch, Hub, HubConfig, Listener, Observer, PairingStatus};
use qs_gateway::{DeviceRole, Identity, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponse};
use tauri::webview::InvokeRequest;
use tauri::{AppHandle, Emitter, Listener as _, Manager, Runtime};

pub const SERVER_IDENTITY: &str = "gateway.server-identity";
pub const DEVICE_IDENTITY: &str = "gateway.device-identity";
const SCOPE: &str = "core";
const ADDRESS: &str = "server.gateway.address";
const EVERY_INTERFACE: &str = "server.gateway.every_interface";
pub const CONNECTIONS: &str = "server.connections";
/// Emitted with `{ what }` when pairing, devices or the audit change.
pub const EVENT: &str = "server:gateway";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Default)]
pub struct GatewayState {
    hub: Mutex<Option<Arc<Hub>>>,
    listener: Mutex<Option<Listener>>,
    error: Mutex<Option<String>>,
    /// This PC pairing with a server (client side), and what its screen shows.
    attempt: tokio::sync::Mutex<Option<(PairBegun, PairingAttempt)>>,
    /// Client mode: the connection to the server this PC uses.
    link: Mutex<Option<Arc<Link>>>,
    /// Bumped by every restart, so an older wait for the address stops.
    generation: std::sync::atomic::AtomicU64,
}

/// The computer's name, as devices and servers see it.
pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "QuantSuite".into())
}

/// The key stored under `id`, made on first use.
fn identity(id: &str) -> Result<Identity, String> {
    vault::with_vault(|v| {
        if let Some(pkcs8) = v.get(id)? {
            return Identity::from_pkcs8(&pkcs8);
        }
        let fresh = Identity::generate()?;
        v.set(id, fresh.pkcs8())?;
        Ok(fresh)
    })
}

fn setting(conn: &rusqlite::Connection, key: &str) -> Option<Value> {
    db::get_setting(conn, SCOPE, key).ok().flatten()
}

/// What other devices may browse, download from and upload to (§6.5): the
/// data root without the vault, the signing key and the device registry, and
/// the user's Documents, Downloads and Desktop — never the whole home folder
/// (it holds SSH keys and the agents' sign-ins).
pub fn file_roots() -> FileRoots {
    let root = crate::paths::root();
    let mut roots = vec![("QuantSuite data".to_string(), root.clone())];
    for (label, dir) in [("Documents", dirs::document_dir()), ("Downloads", dirs::download_dir()), ("Desktop", dirs::desktop_dir())] {
        if let Some(d) = dir {
            roots.push((label.to_string(), d));
        }
    }
    let deny = ["secrets", "signing", "gateway.db", "gateway.db-wal", "gateway.db-shm"].iter().map(|n| root.join(n)).collect();
    FileRoots { roots, deny, transfers: root.join("transfers") }
}

/// Hands the events of server-placed modules to the hub (§6.3.2): the Tauri
/// events in the manifest's `events` and the bus topics in its `topics`.
pub fn forward_events<R: Runtime>(app: &AppHandle<R>, hub: &Arc<Hub>) {
    for name in hub.manifest().events().to_vec() {
        let h = Arc::downgrade(hub);
        let topic = name.clone();
        app.listen_any(name, move |e| {
            if let Some(h) = h.upgrade() {
                let payload = serde_json::from_str(e.payload()).unwrap_or(Value::Null);
                h.publish(EventKind::Tauri, &topic, payload);
            }
        });
    }
    let topics: HashSet<String> = hub.manifest().topics().iter().cloned().collect();
    let data = hub.manifest().data().clone();
    let h = Arc::downgrade(hub);
    app.listen_any(crate::bus::CHANNEL, move |e| {
        let Some(h) = h.upgrade() else { return };
        let Ok(v) = serde_json::from_str::<Value>(e.payload()) else { return };
        let topic = v.get("topic").and_then(Value::as_str).unwrap_or_default().to_string();
        let p = v.get("payload").cloned().unwrap_or(Value::Null);
        let field = |k: &str| p.get(k).and_then(Value::as_str).map(String::from);
        // The core's own topics only for the server's module data (§6.3.3).
        let core_ok = match topic.as_str() {
            "core.setting.changed" => field("scope").is_some_and(|s| data.scopes.contains(&s)),
            "core.entity.upserted" | "core.entity.deleted" => field("module")
                .or_else(|| field("id").map(|id| id.split(':').next().unwrap_or("").to_string()))
                .is_some_and(|m| data.modules.contains(&m)),
            _ => false,
        };
        if topics.contains(&topic) || core_ok {
            h.publish(EventKind::Bus, &topic, v);
        }
    });
}

/// Remote calls through the main webview's IPC.
pub fn webview_dispatch<R: Runtime>(app: &AppHandle<R>) -> Arc<dyn Dispatch> {
    Arc::new(WebviewDispatch { app: app.clone() })
}

// ── dispatch through the main webview (option B) ────────────────────────────

struct WebviewDispatch<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Dispatch for WebviewDispatch<R> {
    fn call(&self, cmd: String, args: Value) -> CallFuture {
        let app = self.app.clone();
        Box::pin(async move {
            let webview = app.get_webview("main").ok_or_else(|| Value::String("The server's window is not ready.".into()))?;
            let fallback = if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" };
            let url = webview.url().or_else(|_| fallback.parse()).map_err(|e| Value::String(format!("{e}")))?;
            let body = if args.is_null() { Value::Object(Default::default()) } else { args };
            let request = InvokeRequest {
                cmd,
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url,
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: app.invoke_key().to_string(),
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            // A synchronous command runs on the calling thread: keep it off the
            // async workers.
            tauri::async_runtime::spawn_blocking(move || {
                webview.on_message(
                    request,
                    Box::new(move |_w, _cmd, response, _cb, _err| {
                        let _ = tx.send(response);
                    }),
                );
            });
            match tokio::time::timeout(Duration::from_secs(300), rx).await {
                Ok(Ok(InvokeResponse::Ok(body))) => body.deserialize::<Value>().map_err(|e| Value::String(e.to_string())),
                Ok(Ok(InvokeResponse::Err(e))) => Err(e.0),
                Ok(Err(_)) => Err(Value::String("The command was refused by the server's IPC.".into())),
                Err(_) => Err(Value::String("The command did not answer in time.".into())),
            }
        })
    }
}

struct Notify<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Observer for Notify<R> {
    fn changed(&self, what: &'static str) {
        let _ = self.app.emit(EVENT, serde_json::json!({ "what": what }));
    }
}

// ── the server side ─────────────────────────────────────────────────────────

fn hub<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Hub>, String> {
    let state = app.state::<GatewayState>();
    let mut slot = lock(&state.hub);
    if let Some(h) = slot.as_ref() {
        return Ok(h.clone());
    }
    if server::role() != Role::Server {
        return Err("This PC is not a server. Turn on \"Run as server\" and restart QuantSuite.".into());
    }
    let registry = Registry::open(&crate::paths::root().join("gateway.db"))?;
    let manifest = Arc::new(Manifest::bundled().clone());
    let mut config = HubConfig::new(computer_name(), app.package_info().version.to_string());
    config.files = Some(file_roots());
    let h = Hub::new(identity(SERVER_IDENTITY)?, registry, manifest, config)?;
    h.set_dispatch(webview_dispatch(app));
    h.set_observer(Arc::new(Notify { app: app.clone() }));
    forward_events(app, &h);
    *slot = Some(h.clone());
    Ok(h)
}

/// The configured listen address, if any.
fn configured(conn: &rusqlite::Connection) -> (Option<String>, bool) {
    let address = setting(conn, ADDRESS).and_then(|v| v.as_str().map(String::from)).filter(|s| !s.is_empty());
    let every = setting(conn, EVERY_INTERFACE).and_then(|v| v.as_bool()).unwrap_or(false);
    (address, every)
}

fn parse_address(address: &str) -> Result<SocketAddr, String> {
    let a = address.trim();
    a.parse::<SocketAddr>()
        .or_else(|_| format!("{a}:{}", qs_gateway::DEFAULT_PORT).parse::<SocketAddr>())
        .map_err(|_| format!("{a} is not an address like 100.101.102.103:{}.", qs_gateway::DEFAULT_PORT))
}

/// (Re)starts listening on the configured address; stops without one. When
/// the address is not on this machine yet — a tailnet address before
/// Tailscale has connected after a reboot — it keeps trying every 15 s
/// (card Server 6/7).
pub async fn restart<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let generation = app.state::<GatewayState>().generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    match bind(app).await {
        Err(Bind::Absent(ip, message)) => {
            *lock(&app.state::<GatewayState>().error) = Some(message.clone());
            let app = app.clone();
            tauri::async_runtime::spawn(async move { wait_for_address(app, ip, generation).await });
            Err(message)
        }
        Err(Bind::Failed(e)) => Err(e),
        Ok(()) => Ok(()),
    }
}

async fn wait_for_address<R: Runtime>(app: AppHandle<R>, ip: std::net::IpAddr, generation: u64) {
    loop {
        tokio::time::sleep(Duration::from_secs(15)).await;
        if app.state::<GatewayState>().generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return;
        }
        if net::has_address(&ip) && bind(&app).await.is_ok() {
            log::info!("gateway: {ip} appeared, listening");
            return;
        }
    }
}

enum Bind {
    /// The address is not on this machine (yet).
    Absent(std::net::IpAddr, String),
    Failed(String),
}

async fn bind<R: Runtime>(app: &AppHandle<R>) -> Result<(), Bind> {
    let state = app.state::<GatewayState>();
    let old = lock(&state.listener).take();
    if let Some(l) = old {
        // The old socket must be closed before the same address is bound again.
        l.close().await;
    }
    let (address, every) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| Bind::Failed(e.to_string()))?;
        configured(&conn)
    };
    let Some(address) = address else {
        *lock(&state.error) = None;
        return Ok(());
    };
    let addr = parse_address(&address).map_err(Bind::Failed)?;
    if !net::has_address(&addr.ip()) {
        let what = if net::kind_of(&addr.ip()) == "tailnet" { "Tailscale is not connected yet" } else { "this network is not connected" };
        let message = format!("Waiting for {} to appear on this PC ({what}); QuantSuite keeps trying.", addr.ip());
        log::info!("gateway: {message}");
        return Err(Bind::Absent(addr.ip(), message));
    }
    let result = async {
        net::check_listen(&addr, every)?;
        let h = hub(app)?;
        h.listen(addr).await
    }
    .await;
    match result {
        Ok(l) => {
            *lock(&state.listener) = Some(l);
            *lock(&state.error) = None;
            Ok(())
        }
        Err(e) => {
            log::warn!("gateway: {e}");
            *lock(&state.error) = Some(e.clone());
            Err(Bind::Failed(e))
        }
    }
}

/// At setup: a server listens on its configured address.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    if server::role() != Role::Server {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = restart(&app).await;
    });
}

/// "100.101.102.103:47100", for the health page.
pub fn listening<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let state = app.try_state::<GatewayState>()?;
    let addr = lock(&state.listener).as_ref().map(|l| l.local_addr().to_string());
    addr
}

#[derive(Serialize)]
pub struct GatewayStatus {
    pub role: Role,
    pub address: Option<String>,
    pub every_interface: bool,
    pub listening: Option<String>,
    pub error: Option<String>,
    pub fingerprint: Option<String>,
    pub pin_set: bool,
    pub default_port: u16,
    pub interfaces: Vec<Interface>,
    pub computer: String,
    /// The listening address is reachable from the whole internet.
    pub listening_public: bool,
    /// Tailscale on this PC (card Server 6/7).
    pub tailnet: net::Tailnet,
}

pub fn status<R: Runtime>(app: &AppHandle<R>, conn: &rusqlite::Connection) -> GatewayStatus {
    let (address, every_interface) = configured(conn);
    let state = app.state::<GatewayState>();
    let h = if server::role() == Role::Server { hub(app).ok() } else { None };
    let error = lock(&state.error).clone();
    GatewayStatus {
        role: server::role(),
        address,
        every_interface,
        listening: listening(app),
        error,
        fingerprint: h.as_ref().map(|h| h.fingerprint()),
        pin_set: h.as_ref().is_some_and(|h| h.pin_set()),
        default_port: qs_gateway::DEFAULT_PORT,
        interfaces: net::interfaces(),
        computer: computer_name(),
        listening_public: listening(app)
            .and_then(|a| a.parse::<SocketAddr>().ok())
            .is_some_and(|a| net::is_public(&a.ip())),
        tailnet: net::tailscale_status(),
    }
}

pub fn configure(conn: &rusqlite::Connection, address: Option<&str>, every_interface: bool) -> Result<(), String> {
    match address.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => {
            let addr = parse_address(a)?;
            net::check_listen(&addr, every_interface)?;
            db::set_setting(conn, SCOPE, ADDRESS, &Value::String(addr.to_string())).map_err(|e| e.to_string())?;
        }
        None => {
            db::set_setting(conn, SCOPE, ADDRESS, &Value::String(String::new())).map_err(|e| e.to_string())?;
        }
    }
    db::set_setting(conn, SCOPE, EVERY_INTERFACE, &Value::Bool(every_interface)).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PairingView {
    #[serde(flatten)]
    pub status: PairingStatus,
    /// What a phone app would scan: address, code and the server's key.
    pub qr_svg: Option<String>,
}

pub fn pairing_start<R: Runtime>(app: &AppHandle<R>) -> Result<PairingView, String> {
    let h = hub(app)?;
    if listening(app).is_none() {
        return Err("Choose the address the server listens on first.".into());
    }
    let status = h.start_pairing()?;
    Ok(pairing_view(app, &h, status))
}

fn pairing_view<R: Runtime>(app: &AppHandle<R>, h: &Hub, status: PairingStatus) -> PairingView {
    let qr_svg = match (&status.code, listening(app)) {
        (Some(code), Some(addr)) => {
            let fp = hex::encode(h.public_key());
            qs_gateway::pairing::qr_svg(&format!("quantsuite://pair?addr={addr}&code={code}&key={fp}"))
        }
        _ => None,
    };
    PairingView { status, qr_svg }
}

pub fn pairing_status<R: Runtime>(app: &AppHandle<R>) -> Result<PairingView, String> {
    let h = hub(app)?;
    let status = h.pairing_status();
    Ok(pairing_view(app, &h, status))
}

pub fn pairing_confirm<R: Runtime>(app: &AppHandle<R>, device_id: &str, role: Option<&str>) -> Result<(), String> {
    let role = match role {
        Some(r) => Some(DeviceRole::parse(r).ok_or_else(|| format!("Unknown role {r:?}."))?),
        None => None,
    };
    hub(app)?.confirm_pairing(device_id, role)
}

pub fn pairing_cancel<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    hub(app)?.cancel_pairing();
    Ok(())
}

#[derive(Serialize)]
pub struct DeviceLine {
    #[serde(flatten)]
    pub device: Device,
    pub connected: bool,
}

pub fn devices<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<DeviceLine>, String> {
    let h = hub(app)?;
    let live = h.connected();
    Ok(h.devices()?.into_iter().map(|d| DeviceLine { connected: live.contains(&d.id), device: d }).collect())
}

pub fn device_rename<R: Runtime>(app: &AppHandle<R>, id: &str, name: &str) -> Result<Device, String> {
    hub(app)?.rename_device(id, name)
}

pub fn device_set_role<R: Runtime>(app: &AppHandle<R>, id: &str, role: &str) -> Result<Device, String> {
    let role = DeviceRole::parse(role).ok_or_else(|| format!("Unknown role {role:?}."))?;
    hub(app)?.set_device_role(id, role)
}

pub fn device_revoke<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<Device, String> {
    hub(app)?.revoke_device(id)
}

pub fn device_forget<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    hub(app)?.forget_device(id)
}

pub fn audit<R: Runtime>(app: &AppHandle<R>, limit: usize) -> Result<Vec<AuditEntry>, String> {
    hub(app)?.audit_log(limit.clamp(1, 1000))
}

pub fn set_pin<R: Runtime>(app: &AppHandle<R>, pin: Option<&str>) -> Result<(), String> {
    hub(app)?.set_pin(pin)
}

// ── this PC as a client: pairing and connection profiles ───────────────────

/// A server this PC paired with. `server_key` is public (it is what the
/// connection pins); this PC's own key stays in the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub address: String,
    pub server_key: String,
    pub fingerprint: String,
    pub device_id: String,
    pub role: DeviceRole,
    pub paired_at: String,
}

fn manifest_hash() -> String {
    Manifest::bundled().hash().to_string()
}

fn me<R: Runtime>(app: &AppHandle<R>) -> ClientInfo {
    ClientInfo { name: computer_name(), app: app.package_info().version.to_string(), manifest: manifest_hash() }
}

pub fn connections(conn: &rusqlite::Connection) -> Vec<Connection> {
    setting(conn, CONNECTIONS).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn save_connections(conn: &rusqlite::Connection, list: &[Connection]) -> Result<(), String> {
    let v = serde_json::to_value(list).map_err(|e| e.to_string())?;
    db::set_setting(conn, SCOPE, CONNECTIONS, &v).map_err(|e| e.to_string())
}

fn with_default_port(address: &str) -> String {
    let a = address.trim();
    if a.parse::<SocketAddr>().is_ok() || a.rsplit_once(':').is_some_and(|(h, p)| !h.contains(':') && p.parse::<u16>().is_ok()) {
        a.to_string()
    } else {
        format!("{a}:{}", qs_gateway::DEFAULT_PORT)
    }
}

#[derive(Clone, Serialize)]
pub struct PairBegun {
    pub server: String,
    pub address: String,
    pub words: [String; 4],
    pub server_fingerprint: String,
}

pub async fn pair_begin<R: Runtime>(app: &AppHandle<R>, address: &str, code: &str) -> Result<PairBegun, String> {
    let address = with_default_port(address);
    let device = identity(DEVICE_IDENTITY)?;
    let attempt = client::pair(&address, &device, code, &me(app)).await?;
    let begun = PairBegun {
        server: attempt.server_name.clone(),
        address: address.clone(),
        words: attempt.words.clone(),
        server_fingerprint: attempt.server_fingerprint(),
    };
    *app.state::<GatewayState>().attempt.lock().await = Some((begun.clone(), attempt));
    Ok(begun)
}

/// The pairing in progress, if any: the settings page shows its words again
/// after it was closed.
pub async fn pair_current<R: Runtime>(app: &AppHandle<R>) -> Option<PairBegun> {
    app.state::<GatewayState>().attempt.lock().await.as_ref().map(|(b, _)| b.clone())
}

/// This PC's user compared the words: `accept` waits for the server's user and
/// saves the connection; otherwise the pairing ends.
pub async fn pair_finish<R: Runtime>(app: &AppHandle<R>, accept: bool) -> Result<Option<Connection>, String> {
    let taken = app.state::<GatewayState>().attempt.lock().await.take();
    let Some((PairBegun { address, .. }, attempt)) = taken else {
        return Err("No pairing is in progress.".into());
    };
    if !accept {
        return Ok(None);
    }
    let (server_key, name, fingerprint) = (attempt.server_key, attempt.server_name.clone(), attempt.server_fingerprint());
    let (device_id, role) = attempt.finish().await?;
    let connection = Connection {
        id: hex::encode(&server_key[..8]),
        name,
        address,
        server_key: hex::encode(server_key),
        fingerprint,
        device_id,
        role,
        paired_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let mut list = connections(&conn);
    list.retain(|c| c.id != connection.id);
    list.push(connection.clone());
    save_connections(&conn, &list)?;
    Ok(Some(connection))
}

pub fn forget_connection(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    let mut list = connections(conn);
    let before = list.len();
    list.retain(|c| c.id != id);
    if list.len() == before {
        return Err("No such connection.".into());
    }
    save_connections(conn, &list)
}

fn server_key(c: &Connection) -> Result<[u8; 32], String> {
    hex::decode(&c.server_key).ok().and_then(|k| k.try_into().ok()).ok_or_else(|| "The saved server key is damaged. Pair again.".to_string())
}

#[derive(Serialize)]
pub struct ConnectionTest {
    pub server: String,
    pub app: String,
    pub role: DeviceRole,
    pub millis: u64,
    /// The server's health (`server_status`), fetched through the connection.
    pub health: Value,
}

pub async fn test_connection<R: Runtime>(app: &AppHandle<R>, c: &Connection) -> Result<ConnectionTest, String> {
    let device = identity(DEVICE_IDENTITY)?;
    let t = Instant::now();
    let session = client::connect(&c.address, &device, server_key(c)?, &me(app)).await.map_err(|e| e.to_string())?;
    let health = session.call("plugin:qs|server_status", Value::Null).await.map_err(|e| e.to_string())?;
    Ok(ConnectionTest {
        server: session.welcome.server.clone(),
        app: session.welcome.app.clone(),
        role: session.welcome.role,
        millis: t.elapsed().as_millis() as u64,
        health,
    })
}

// ── client mode: the link to the server this PC uses (card Server 5/7) ─────

/// The connection profile this PC uses (its id), or none: local.
pub const ACTIVE: &str = "server.active_connection";
/// Emitted with a [`LinkStatus`] whenever the link changes.
pub const LINK_EVENT: &str = "server:connection";
/// Emitted when the server pages must reload their data (a reconnect, or
/// events that could not be resumed).
pub const REFRESH_EVENT: &str = "server:refresh";

#[derive(Debug, Clone, Serialize)]
pub struct LinkStatus {
    /// `local` or `client`.
    pub mode: &'static str,
    pub connection: Option<Connection>,
    /// `local`, `connecting`, `online`, `offline` or `refused`.
    pub state: &'static str,
    /// Online since, or not reachable since (the last moment it was).
    pub since: Option<String>,
    pub message: Option<String>,
    pub server_app: Option<String>,
    pub role: Option<DeviceRole>,
    /// Modules whose pages come from the server.
    pub server_modules: Vec<String>,
    /// The last ping's round trip (connection quality, card Server 6/7).
    pub rtt_ms: Option<f64>,
    /// Connections made again since QuantSuite started.
    pub reconnects: u32,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub struct Link {
    connection: Connection,
    session: tokio::sync::RwLock<Option<Arc<Session>>>,
    status: Mutex<LinkStatus>,
    resume: Mutex<Option<ResumePoint>>,
    wake: tokio::sync::Notify,
}

impl Link {
    fn new(connection: Connection) -> Self {
        let status = LinkStatus {
            mode: "client",
            connection: Some(connection.clone()),
            state: "connecting",
            since: None,
            message: None,
            server_app: None,
            role: Some(connection.role),
            server_modules: Manifest::bundled().data().modules.clone(),
            rtt_ms: None,
            reconnects: 0,
        };
        Self { connection, session: Default::default(), status: Mutex::new(status), resume: Mutex::new(None), wake: Default::default() }
    }

    fn set<R: Runtime>(&self, app: &AppHandle<R>, f: impl FnOnce(&mut LinkStatus)) {
        let snapshot = {
            let mut s = lock(&self.status);
            f(&mut s);
            s.clone()
        };
        let _ = app.emit(LINK_EVENT, snapshot);
    }

    fn status(&self) -> LinkStatus {
        let mut s = lock(&self.status).clone();
        if let Ok(guard) = self.session.try_read() {
            s.rtt_ms = guard.as_ref().and_then(|x| x.rtt()).map(|d| (d.as_secs_f64() * 1000.0 * 10.0).round() / 10.0);
        }
        s
    }

    /// The open session, waiting up to `wait` for one.
    async fn session(&self, wait: Duration) -> Option<Arc<Session>> {
        let until = Instant::now() + wait;
        loop {
            if let Some(s) = self.session.read().await.as_ref().filter(|s| !s.is_closed()) {
                return Some(s.clone());
            }
            if Instant::now() >= until {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn unreachable(&self) -> Value {
        let s = self.status();
        let message = match (s.state, s.message) {
            ("refused", Some(m)) => m,
            _ => "The server is not reachable right now.".to_string(),
        };
        serde_json::json!({ "code": "offline", "message": message, "since": s.since })
    }
}

fn call_error(e: CallError) -> Value {
    serde_json::json!({ "code": e.code, "message": e.message })
}

/// This launch's link, if this PC is a client.
fn link<R: Runtime>(app: &AppHandle<R>) -> Option<Arc<Link>> {
    let state = app.try_state::<GatewayState>()?;
    let l = lock(&state.link).clone();
    l
}

pub fn link_status<R: Runtime>(app: &AppHandle<R>) -> LinkStatus {
    link(app).map(|l| l.status()).unwrap_or_else(|| LinkStatus {
        mode: "local",
        connection: None,
        state: "local",
        since: None,
        message: None,
        server_app: None,
        role: None,
        server_modules: Manifest::bundled().data().modules.clone(),
        rtt_ms: None,
        reconnects: 0,
    })
}

/// At setup: a client connects to the server it uses.
pub fn start_link<R: Runtime>(app: &AppHandle<R>, conn: &rusqlite::Connection) {
    if server::role() != Role::Client {
        return;
    }
    let active = setting(conn, ACTIVE).and_then(|v| v.as_str().map(String::from));
    let found = active.and_then(|id| connections(conn).into_iter().find(|c| c.id == id));
    let Some(connection) = found else {
        log::warn!("gateway: this PC is a client, but its server connection is gone");
        return;
    };
    let l = Arc::new(Link::new(connection));
    *lock(&app.state::<GatewayState>().link) = Some(l.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run_link(app, l).await });
}

async fn run_link<R: Runtime>(app: AppHandle<R>, link: Arc<Link>) {
    let refuse = |message: String| {
        link.set(&app, |s| {
            s.state = "refused";
            s.message = Some(message);
        })
    };
    let device = match identity(DEVICE_IDENTITY) {
        Ok(d) => d,
        Err(e) => return refuse(e),
    };
    let key = match server_key(&link.connection) {
        Ok(k) => k,
        Err(e) => return refuse(e),
    };
    let me = me(&app);
    // Seconds between attempts: quick at first, then every 30 s.
    const BACKOFF: [u64; 6] = [1, 2, 5, 10, 20, 30];
    let mut failures = 0usize;
    let mut was_online = false;
    loop {
        let resume = *lock(&link.resume);
        let opts = ConnectOptions { resume, ..Default::default() };
        match client::connect_with(&link.connection.address, &device, key, &me, opts).await {
            Ok(s) => {
                failures = 0;
                let s = Arc::new(s);
                *lock(&link.resume) = Some(s.resume_point());
                *link.session.write().await = Some(s.clone());
                let (app_v, role, reload) = (s.welcome.app.clone(), s.welcome.role, s.welcome.reload);
                let again = was_online;
                link.set(&app, |st| {
                    st.state = "online";
                    st.since = Some(now_iso());
                    st.message = None;
                    st.server_app = Some(app_v);
                    st.role = Some(role);
                    if again {
                        st.reconnects += 1;
                    }
                });
                // Back after a gap: the server's pages load their data again.
                if was_online || reload {
                    let _ = app.emit(REFRESH_EVENT, ());
                }
                was_online = true;
                let pump = tauri::async_runtime::spawn(pump_events(app.clone(), link.clone(), s.clone()));
                s.closed().await;
                pump.abort();
                *link.session.write().await = None;
                link.set(&app, |st| {
                    st.state = "offline";
                    st.since = Some(now_iso());
                    st.message = Some("The connection to the server was lost; reconnecting.".into());
                });
            }
            Err(e) => {
                let hard = matches!(e, ConnectError::Refused { .. } | ConnectError::Unpaired | ConnectError::Tls { key_changed: true, .. });
                let message = e.to_string();
                link.set(&app, |st| {
                    st.state = if hard { "refused" } else { "offline" };
                    // Not reachable since the last moment it was (or the first try).
                    if st.since.is_none() || st.state == "refused" {
                        st.since = Some(now_iso());
                    }
                    st.message = Some(message);
                });
                if hard {
                    failures = BACKOFF.len() - 1;
                }
            }
        }
        let wait = Duration::from_secs(BACKOFF[failures.min(BACKOFF.len() - 1)]);
        failures += 1;
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = link.wake.notified() => {}
        }
    }
}

/// The server's events, emitted here under their own names.
async fn pump_events<R: Runtime>(app: AppHandle<R>, link: Arc<Link>, s: Arc<Session>) {
    let Some(mut rx) = s.take_events() else { return };
    let epoch = s.welcome.epoch;
    while let Some(ev) = rx.recv().await {
        match ev {
            ClientEvent::Event { seq, kind, topic, payload } => {
                match kind {
                    EventKind::Tauri => {
                        let _ = app.emit(&topic, payload);
                    }
                    EventKind::Bus => {
                        if let Ok(e) = serde_json::from_value::<crate::bus::Event>(payload) {
                            let _ = crate::bus::publish(&app, e);
                        }
                    }
                }
                *lock(&link.resume) = Some(ResumePoint { epoch, last_seq: seq });
            }
            ClientEvent::Reload { epoch, seq } => {
                *lock(&link.resume) = Some(ResumePoint { epoch, last_seq: seq });
                let _ = app.emit(REFRESH_EVENT, ());
            }
        }
    }
}

/// How long the client waits for a call of each side-effect class (a little
/// longer than the server, so the server's own timeout answers first).
fn client_timeout(effect: Effect) -> Duration {
    qs_gateway::server::timeout_for(effect) + Duration::from_secs(10)
}

/// A call the seam routed to the server. Changing calls carry an idempotency
/// key: after a timeout or a dropped connection the client asks what happened
/// instead of sending them again (§5).
pub async fn remote_call<R: Runtime>(app: &AppHandle<R>, cmd: String, args: Value) -> Result<Value, Value> {
    let link = link(app).ok_or_else(|| serde_json::json!({ "code": "local", "message": "This PC does not use a server." }))?;
    let effect = Manifest::bundled().lookup(&cmd).map(|e| e.1).unwrap_or(Effect::Read);
    let timeout = client_timeout(effect);
    let key = matches!(effect, Effect::Write | Effect::External).then(client::new_key);
    let session = link.session(Duration::from_secs(3)).await.ok_or_else(|| link.unreachable())?;
    match session.call_with(&cmd, args.clone(), CallOptions { key: key.clone(), timeout }).await {
        Ok(v) => Ok(v),
        Err(e) if key.is_some() && (e.code == "timeout" || e.code == "closed") => {
            let key = key.unwrap_or_default();
            let unsure = || {
                serde_json::json!({ "code": "unknown", "message": "The connection dropped during this change; the server could not be asked whether it took effect. Check before trying again." })
            };
            let again = link.session(Duration::from_secs(30)).await.ok_or_else(unsure)?;
            match again.outcome(&key, timeout).await {
                Ok(v) => Ok(v),
                // It never arrived: sending it now is safe (same key).
                Err(o) if o.code == "unknown" => again.call_with(&cmd, args, CallOptions { key: Some(key), timeout }).await.map_err(call_error),
                Err(o) if o.code == "closed" || o.code == "timeout" => Err(unsure()),
                Err(o) => Err(call_error(o)),
            }
        }
        Err(e) => Err(call_error(e)),
    }
}

pub async fn step_up<R: Runtime>(app: &AppHandle<R>, pin: &str) -> Result<u64, String> {
    let link = link(app).ok_or("This PC does not use a server.")?;
    let s = link.session(Duration::from_secs(3)).await.ok_or("The server is not reachable right now.")?;
    s.step_up(pin).await
}

/// Reconnect now instead of waiting for the next attempt.
pub fn retry<R: Runtime>(app: &AppHandle<R>) {
    if let Some(l) = link(app) {
        l.wake.notify_one();
    }
}

/// Use a saved connection (`Some(id)`) or this PC's own data (`None`). Takes
/// effect at the next start: the role decides at setup whether this PC may
/// run executors (one trader, §5).
pub fn use_connection(conn: &rusqlite::Connection, id: Option<&str>) -> Result<(), String> {
    match id {
        Some(id) => {
            if !connections(conn).iter().any(|c| c.id == id) {
                return Err("No such connection.".into());
            }
            if server::stored_role(conn) == Some(Role::Server) {
                return Err("This PC runs as a server. Turn \"Run as server\" off first.".into());
            }
            db::set_setting(conn, SCOPE, ACTIVE, &Value::String(id.to_string())).map_err(|e| e.to_string())?;
            server::set_role(conn, Role::Client)
        }
        None => {
            db::set_setting(conn, SCOPE, ACTIVE, &Value::String(String::new())).map_err(|e| e.to_string())?;
            server::set_role(conn, Role::Local)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_get_the_default_port() {
        assert_eq!(with_default_port("100.101.102.103"), "100.101.102.103:47100");
        assert_eq!(with_default_port(" homeserver "), "homeserver:47100");
        assert_eq!(with_default_port("homeserver:5000"), "homeserver:5000");
        assert_eq!(with_default_port("192.168.1.20:47101"), "192.168.1.20:47101");
        assert_eq!(parse_address("192.168.1.20").unwrap().to_string(), "192.168.1.20:47100");
        assert!(parse_address("homeserver").is_err(), "the server listens on an address of its own");
    }
}
