//! The server's end (PLAN-SERVER-CLIENT §4, §6.4): the [`Hub`] holds the
//! identity, pairing, devices, PIN and live connections; [`Hub::listen`]
//! serves them on one address.
//!
//! Every connection: TLS 1.3 with the device's key → `hello` (protocol and
//! manifest must match) → a paired device gets `welcome` and may call; an
//! unknown key may only pair; a revoked key is refused. Each call is checked
//! against the device's role and the remote manifest, `external` calls need
//! the server PIN, and every call lands in the audit log. Revoking a device
//! drops its connections at once.

use crate::authz::{decide, Decision, DeviceRole, Manifest};
use crate::identity::Identity;
use crate::pairing::{Pairing, CODE_VALID};
use crate::protocol::{CallError, Frame, Refusal, MAX_FRAME, PROTOCOL};
use crate::registry::{AuditEntry, Device, DeviceState, Registry};
use crate::stepup::{self, StepUps};
use crate::{tls, words};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub type CallFuture = Pin<Box<dyn Future<Output = Result<Value, Value>> + Send>>;

/// Performs an authorized call on the server (qs-core: through the main
/// webview's IPC, PLAN §6.3 option B).
pub trait Dispatch: Send + Sync {
    fn call(&self, cmd: String, args: Value) -> CallFuture;
}

/// Told when pairing, the devices or the audit change, so the server's
/// screen refreshes.
pub trait Observer: Send + Sync {
    fn changed(&self, what: &'static str);
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const PAIR_FRAME_TIMEOUT: Duration = Duration::from_secs(60);
const PIN_KEY: &str = "pin_hash";

type Ws = WebSocketStream<tokio_rustls::server::TlsStream<TcpStream>>;
type Tx = SplitSink<Ws, Message>;
type Rx = SplitStream<Ws>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub(crate) fn ws_config() -> WebSocketConfig {
    let mut c = WebSocketConfig::default();
    c.max_message_size = Some(MAX_FRAME);
    c.max_frame_size = Some(MAX_FRAME);
    c
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingDevice {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingStatus {
    /// Shown on the server's screen only, never sent to a device.
    pub code: Option<String>,
    pub secs_left: u64,
    pub locked_secs: u64,
    pub pending: Vec<PendingDevice>,
}

pub struct HubConfig {
    /// What devices see as the server's name (the computer name).
    pub server_name: String,
    pub app_version: String,
}

/// A device that entered the right code, and how the server's answer reaches it.
type Waiting = (PendingDevice, oneshot::Sender<Option<DeviceRole>>);
/// An open connection: its id and the switch that drops it.
type Live = (u64, watch::Sender<bool>);

pub struct Hub {
    identity: Identity,
    tls: Arc<rustls::ServerConfig>,
    registry: Mutex<Registry>,
    pairing: Mutex<Pairing>,
    pending: Mutex<HashMap<String, Waiting>>,
    stepups: Mutex<StepUps>,
    live: Mutex<HashMap<String, Vec<Live>>>,
    next_session: AtomicU64,
    manifest: Arc<Manifest>,
    dispatch: RwLock<Option<Arc<dyn Dispatch>>>,
    observer: RwLock<Option<Arc<dyn Observer>>>,
    config: HubConfig,
}

fn read<T: Clone>(l: &RwLock<Option<T>>) -> Option<T> {
    l.read().unwrap_or_else(|p| p.into_inner()).clone()
}

impl Hub {
    pub fn new(identity: Identity, registry: Registry, manifest: Arc<Manifest>, config: HubConfig) -> Result<Arc<Self>, String> {
        let tls = tls::server_config(&identity)?;
        Ok(Arc::new(Self {
            identity,
            tls,
            registry: Mutex::new(registry),
            pairing: Mutex::new(Pairing::new()),
            pending: Mutex::new(HashMap::new()),
            stepups: Mutex::new(StepUps::default()),
            live: Mutex::new(HashMap::new()),
            next_session: AtomicU64::new(1),
            manifest,
            dispatch: RwLock::new(None),
            observer: RwLock::new(None),
            config,
        }))
    }

    pub fn set_dispatch(&self, d: Arc<dyn Dispatch>) {
        *self.dispatch.write().unwrap_or_else(|p| p.into_inner()) = Some(d);
    }

    pub fn set_observer(&self, o: Arc<dyn Observer>) {
        *self.observer.write().unwrap_or_else(|p| p.into_inner()) = Some(o);
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.identity.public_key()
    }

    pub fn fingerprint(&self) -> String {
        self.identity.fingerprint()
    }

    fn notify(&self, what: &'static str) {
        if let Some(o) = read(&self.observer) {
            o.changed(what);
        }
    }

    fn audit(&self, device: Option<&Device>, command: &str, outcome: &str, detail: Option<&str>) {
        let (id, name) = match device {
            Some(d) => (Some(d.id.as_str()), d.name.as_str()),
            None => (None, "unpaired"),
        };
        lock(&self.registry).audit(id, name, command, outcome, detail);
    }

    /// An action on the server's own screen.
    fn audit_local(&self, command: &str, target: Option<&Device>, detail: Option<&str>) {
        let detail = match (target, detail) {
            (Some(d), Some(x)) => Some(format!("{} — {x}", d.name)),
            (Some(d), None) => Some(d.name.clone()),
            (None, x) => x.map(String::from),
        };
        lock(&self.registry).audit(target.map(|d| d.id.as_str()), "server", command, "ok", detail.as_deref());
        self.notify("audit");
    }

    // ── pairing ──────────────────────────────────────────────────────────

    pub fn start_pairing(&self) -> Result<PairingStatus, String> {
        lock(&self.pairing).start(Instant::now()).map_err(|e| e.to_string())?;
        self.audit_local("pairing_start", None, None);
        self.notify("pairing");
        Ok(self.pairing_status())
    }

    /// Closes the code and turns away every device still waiting.
    pub fn cancel_pairing(&self) {
        lock(&self.pairing).cancel();
        let waiting: Vec<_> = lock(&self.pending).drain().collect();
        for (_, (_, tx)) in waiting {
            let _ = tx.send(None);
        }
        self.notify("pairing");
    }

    pub fn pairing_status(&self) -> PairingStatus {
        let now = Instant::now();
        let mut p = lock(&self.pairing);
        let open = p.open(now);
        let locked = p.locked_for(now).map(|d| d.as_secs().max(1)).unwrap_or(0);
        drop(p);
        let pending = lock(&self.pending).values().map(|(d, _)| d.clone()).collect();
        PairingStatus {
            secs_left: open.as_ref().map(|(_, l)| l.as_secs()).unwrap_or(0),
            code: open.map(|(c, _)| c),
            locked_secs: locked,
            pending,
        }
    }

    /// The server's user compared the words: `Some(role)` lets the device in,
    /// `None` turns it away.
    pub fn confirm_pairing(&self, device_id: &str, role: Option<DeviceRole>) -> Result<(), String> {
        let (_, tx) = lock(&self.pending).remove(device_id).ok_or("That device is no longer waiting. Pair it again.")?;
        tx.send(role).map_err(|_| "The device has disconnected. Pair it again.".to_string())
    }

    // ── devices ──────────────────────────────────────────────────────────

    pub fn devices(&self) -> Result<Vec<Device>, String> {
        lock(&self.registry).list()
    }

    /// Device ids with at least one open connection.
    pub fn connected(&self) -> Vec<String> {
        lock(&self.live).iter().filter(|(_, v)| !v.is_empty()).map(|(k, _)| k.clone()).collect()
    }

    pub fn rename_device(&self, id: &str, name: &str) -> Result<Device, String> {
        let d = lock(&self.registry).rename(id, name)?;
        self.audit_local("device_rename", Some(&d), None);
        self.notify("devices");
        Ok(d)
    }

    pub fn set_device_role(&self, id: &str, role: DeviceRole) -> Result<Device, String> {
        let d = lock(&self.registry).set_role(id, role)?;
        lock(&self.stepups).forget(id);
        self.audit_local("device_role", Some(&d), Some(role.as_str()));
        self.notify("devices");
        Ok(d)
    }

    /// Removes a device's access; its open connections drop at once.
    pub fn revoke_device(&self, id: &str) -> Result<Device, String> {
        let d = lock(&self.registry).revoke(id)?;
        self.drop_connections(id);
        lock(&self.stepups).forget(id);
        self.audit_local("device_revoke", Some(&d), None);
        self.notify("devices");
        Ok(d)
    }

    pub fn forget_device(&self, id: &str) -> Result<(), String> {
        lock(&self.registry).forget(id)?;
        self.notify("devices");
        Ok(())
    }

    fn drop_connections(&self, id: &str) {
        if let Some(sessions) = lock(&self.live).remove(id) {
            for (_, s) in sessions {
                let _ = s.send(true);
            }
        }
    }

    pub fn audit_log(&self, limit: usize) -> Result<Vec<AuditEntry>, String> {
        lock(&self.registry).recent_audit(limit)
    }

    // ── the server PIN ───────────────────────────────────────────────────

    pub fn pin_set(&self) -> bool {
        lock(&self.registry).meta(PIN_KEY).is_some()
    }

    /// Sets (or with `None` removes) the server PIN; every grant ends.
    pub fn set_pin(&self, pin: Option<&str>) -> Result<(), String> {
        let hash = pin.map(stepup::hash_pin).transpose()?;
        lock(&self.registry).set_meta(PIN_KEY, hash.as_deref())?;
        lock(&self.stepups).clear_grants();
        self.audit_local(if pin.is_some() { "pin_set" } else { "pin_remove" }, None, None);
        Ok(())
    }

    // ── the network ──────────────────────────────────────────────────────

    pub async fn listen(self: &Arc<Self>, addr: SocketAddr) -> Result<Listener, String> {
        let tcp = TcpListener::bind(addr).await.map_err(|e| format!("Cannot listen on {addr}: {e}"))?;
        let local = tcp.local_addr().map_err(|e| e.to_string())?;
        let (stop, mut stopped) = watch::channel(false);
        let hub = self.clone();
        let acceptor = TlsAcceptor::from(self.tls.clone());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    r = tcp.accept() => match r {
                        Ok((sock, peer)) => {
                            let (hub, acceptor, stop) = (hub.clone(), acceptor.clone(), stopped.clone());
                            tokio::spawn(async move { hub.session(acceptor, sock, peer, stop).await });
                        }
                        Err(e) => {
                            log::warn!("gateway: accept: {e}");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    }
                }
            }
        });
        log::info!("gateway: listening on {local}");
        Ok(Listener { addr: local, stop })
    }

    async fn session(self: Arc<Self>, acceptor: TlsAcceptor, sock: TcpStream, peer: SocketAddr, stop: watch::Receiver<bool>) {
        let _ = sock.set_nodelay(true);
        let tls = match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(sock)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                log::info!("gateway: TLS handshake from {peer} failed: {e}");
                return;
            }
            Err(_) => return,
        };
        let Some(key) = tls::peer_key(tls.get_ref().1) else { return };
        let ws = match tokio::time::timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::accept_async_with_config(tls, Some(ws_config()))).await {
            Ok(Ok(ws)) => ws,
            _ => return,
        };
        let (mut tx, mut rx) = ws.split();
        let hello = match tokio::time::timeout(HANDSHAKE_TIMEOUT, next_frame(&mut rx)).await {
            Ok(Some(Ok(f))) => f,
            _ => return,
        };
        let Frame::Hello { protocol, app, manifest, name } = hello else {
            let _ = send(&mut tx, &refused(Refusal::Protocol, "The first message must be hello.")).await;
            return;
        };
        if protocol != PROTOCOL {
            let (reason, message) = if protocol < PROTOCOL {
                (Refusal::UpdateClient, format!("This PC runs an older QuantSuite ({app}) than the server ({}). Update this PC.", self.config.app_version))
            } else {
                (Refusal::UpdateServer, format!("The server runs an older QuantSuite ({}) than this PC ({app}). Update the server.", self.config.app_version))
            };
            let _ = send(&mut tx, &refused(reason, &message)).await;
            return;
        }
        if manifest != self.manifest.hash() {
            let message = format!(
                "The server runs QuantSuite {} and this PC {app}. Install the same version on both.",
                self.config.app_version
            );
            let _ = send(&mut tx, &refused(Refusal::VersionMismatch, &message)).await;
            return;
        }
        let device = lock(&self.registry).by_key(&key).ok().flatten();
        match device {
            Some(d) if d.state == DeviceState::Paired => self.serve(d, tx, rx, stop).await,
            Some(d) if d.state == DeviceState::Revoked => {
                self.audit(Some(&d), "connect", "refused", Some("revoked"));
                let _ = send(&mut tx, &refused(Refusal::Revoked, "This device was removed from the server. Pair it again there.")).await;
            }
            _ => self.pair(key, name, tx, rx, stop).await,
        }
    }

    async fn pair(&self, key: [u8; 32], hello_name: String, mut tx: Tx, mut rx: Rx, mut stop: watch::Receiver<bool>) {
        let _ = send(&mut tx, &Frame::Unpaired { server: self.config.server_name.clone() }).await;
        let (code, name) = match tokio::time::timeout(PAIR_FRAME_TIMEOUT, next_frame(&mut rx)).await {
            Ok(Some(Ok(Frame::Pair { code, name }))) => (code, name),
            Ok(Some(Ok(Frame::Call { id, cmd, .. }))) => {
                self.audit(None, &cmd, "denied", Some("unpaired device"));
                let error = CallError::new("denied", "This device is not paired with the server.");
                let _ = send(&mut tx, &Frame::Result { id, ok: None, error: Some(error) }).await;
                let _ = tx.close().await;
                return;
            }
            _ => return,
        };
        let checked = lock(&self.pairing).check(&code, Instant::now());
        if let Err(e) = checked {
            self.audit(None, "pair", "failed", Some(&e.to_string()));
            self.notify("pairing");
            let _ = send(&mut tx, &Frame::PairFailed { message: e.to_string() }).await;
            let _ = tx.close().await;
            return;
        }
        let name = if name.trim().is_empty() { hello_name } else { name };
        let added = lock(&self.registry).add_pending(&key, &name);
        let device = match added {
            Ok(d) => d,
            Err(e) => {
                let _ = send(&mut tx, &Frame::PairFailed { message: e }).await;
                return;
            }
        };
        let words = words::pairing_words(&self.identity.public_key(), &key).to_vec();
        let (otx, orx) = oneshot::channel();
        let shown = PendingDevice { id: device.id.clone(), name: device.name.clone(), fingerprint: device.fingerprint.clone(), words: words.clone() };
        lock(&self.pending).insert(device.id.clone(), (shown, otx));
        self.notify("pairing");
        let _ = send(&mut tx, &Frame::PairPending { words }).await;

        let outcome = tokio::select! {
            r = orx => r.ok().flatten(),
            _ = wait_closed(&mut rx) => None,
            _ = tokio::time::sleep(CODE_VALID) => None,
            _ = stop.changed() => None,
        };
        lock(&self.pending).remove(&device.id);
        let confirmed = outcome.map(|role| lock(&self.registry).confirm(&device.id, role));
        match confirmed {
            Some(Ok(d)) => {
                self.audit(Some(&d), "pair", "paired", Some(d.role.as_str()));
                let _ = send(&mut tx, &Frame::Paired { device_id: d.id.clone(), role: d.role }).await;
            }
            Some(Err(e)) => {
                let _ = send(&mut tx, &Frame::PairFailed { message: e }).await;
            }
            None => {
                let _ = lock(&self.registry).remove_pending(&device.id);
                self.audit(Some(&device), "pair", "rejected", None);
                let message = "The server did not confirm this device. Compare the four words and pair again.".to_string();
                let _ = send(&mut tx, &Frame::PairFailed { message }).await;
            }
        }
        self.notify("devices");
        self.notify("pairing");
        let _ = tx.close().await;
    }

    async fn serve(self: &Arc<Self>, device: Device, tx: Tx, mut rx: Rx, mut stop: watch::Receiver<bool>) {
        let (revoke, mut revoked) = watch::channel(false);
        let sid = self.next_session.fetch_add(1, Ordering::Relaxed);
        lock(&self.live).entry(device.id.clone()).or_default().push((sid, revoke));
        lock(&self.registry).touch(&device.id);
        self.audit(Some(&device), "connect", "ok", None);
        self.notify("devices");

        let (out, mut queue) = mpsc::channel::<Frame>(64);
        let writer = tokio::spawn(async move {
            let mut tx = tx;
            while let Some(f) = queue.recv().await {
                if tx.send(Message::text(f.encode())).await.is_err() {
                    break;
                }
            }
            let _ = tx.close().await;
        });
        let welcome = Frame::Welcome {
            protocol: PROTOCOL,
            app: self.config.app_version.clone(),
            server: self.config.server_name.clone(),
            device_id: device.id.clone(),
            role: device.role,
        };
        let _ = out.send(welcome).await;
        let mut was_revoked = false;
        loop {
            tokio::select! {
                r = revoked.changed() => { was_revoked = r.is_ok(); break }
                _ = stop.changed() => break,
                f = next_frame(&mut rx) => match f {
                    Some(Ok(frame)) => self.handle(&device.id, frame, &out).await,
                    _ => break,
                },
            }
        }
        if let Some(v) = lock(&self.live).get_mut(&device.id) {
            v.retain(|(id, _)| *id != sid);
        }
        drop(out);
        if was_revoked {
            // At once: no goodbye, the socket closes now.
            writer.abort();
        } else {
            let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
        }
        self.notify("devices");
    }

    async fn handle(self: &Arc<Self>, device_id: &str, frame: Frame, out: &mpsc::Sender<Frame>) {
        match frame {
            Frame::Call { id, cmd, args } => {
                // The device as it is now: a new role applies to the next call.
                let current = lock(&self.registry).get(device_id);
                let device = match current {
                    Ok(Some(d)) if d.state == DeviceState::Paired => d,
                    _ => {
                        let error = CallError::new("denied", "This device is no longer paired with the server.");
                        let _ = out.send(Frame::Result { id, ok: None, error: Some(error) }).await;
                        return;
                    }
                };
                let granted = lock(&self.stepups).granted(&device.id, Instant::now());
                let decision = match decide(&self.manifest, device.role, &cmd) {
                    Decision::StepUp if granted => Decision::Allow,
                    d => d,
                };
                match decision {
                    Decision::Deny(why) => {
                        self.audit(Some(&device), &cmd, "denied", Some(&why));
                        let _ = out.send(Frame::Result { id, ok: None, error: Some(CallError::new("denied", why)) }).await;
                    }
                    Decision::StepUp => {
                        self.audit(Some(&device), &cmd, "step_up_required", None);
                        let error = CallError::new("step_up_required", "Enter the server PIN to do this from another device.");
                        let _ = out.send(Frame::Result { id, ok: None, error: Some(error) }).await;
                    }
                    Decision::Allow => {
                        let (hub, out) = (self.clone(), out.clone());
                        tokio::spawn(async move {
                            let result = match read(&hub.dispatch) {
                                Some(d) => d.call(cmd.clone(), args).await,
                                None => Err(Value::String("The server is still starting.".into())),
                            };
                            let frame = match result {
                                Ok(v) => {
                                    hub.audit(Some(&device), &cmd, "ok", None);
                                    Frame::Result { id, ok: Some(v), error: None }
                                }
                                Err(e) => {
                                    let message = match e {
                                        Value::String(s) => s,
                                        other => other.to_string(),
                                    };
                                    hub.audit(Some(&device), &cmd, "error", Some(&message));
                                    Frame::Result { id, ok: None, error: Some(CallError::new("failed", message)) }
                                }
                            };
                            let _ = out.send(frame).await;
                        });
                    }
                }
            }
            Frame::StepUp { pin } => {
                let device = lock(&self.registry).get(device_id).ok().flatten();
                let locked = lock(&self.stepups).check_locked(device_id, Instant::now());
                let hash = lock(&self.registry).meta(PIN_KEY);
                let reply = match (locked, hash) {
                    (Err(e), _) => Err(e),
                    (Ok(()), None) => Err(stepup::StepUpError::NoPin),
                    (Ok(()), Some(hash)) => {
                        // Argon2 takes a moment: off the async workers, and no lock held.
                        let right = tokio::task::spawn_blocking(move || stepup::verify_pin(&hash, &pin)).await.unwrap_or(false);
                        lock(&self.stepups).record(device_id, right, Instant::now())
                    }
                };
                let frame = match reply {
                    Ok(valid) => {
                        self.audit(device.as_ref(), "step_up", "ok", None);
                        Frame::SteppedUp { ok: true, valid_secs: valid.as_secs(), message: None }
                    }
                    Err(e) => {
                        self.audit(device.as_ref(), "step_up", "failed", Some(&e.to_string()));
                        Frame::SteppedUp { ok: false, valid_secs: 0, message: Some(e.to_string()) }
                    }
                };
                let _ = out.send(frame).await;
            }
            Frame::Ping => {
                let _ = out.send(Frame::Pong).await;
            }
            _ => {}
        }
    }
}

/// A bound gateway; dropping it stops listening and ends every connection.
pub struct Listener {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
}

impl Listener {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

fn refused(reason: Refusal, message: &str) -> Frame {
    Frame::Refused { reason, message: message.to_string() }
}

async fn send(tx: &mut Tx, f: &Frame) -> Result<(), ()> {
    tx.send(Message::text(f.encode())).await.map_err(|_| ())
}

async fn next_frame(rx: &mut Rx) -> Option<Result<Frame, String>> {
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(t)) => return Some(Frame::decode(t.as_str())),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => continue,
        }
    }
    None
}

async fn wait_closed(rx: &mut Rx) {
    while let Some(m) = rx.next().await {
        if matches!(m, Err(_) | Ok(Message::Close(_))) {
            return;
        }
    }
}
