//! The server's end (PLAN-SERVER-CLIENT §4–§6.5): the [`Hub`] holds the
//! identity, pairing, devices, PIN, events and live connections;
//! [`Hub::listen`] serves them on one address.
//!
//! Every connection: TLS 1.3 with the device's key → `hello` (protocol and
//! manifest must match) → a paired device gets `welcome` and its missed
//! events, then may call; an unknown key may only pair; a revoked key is
//! refused. Each call is checked against the device's role and the remote
//! manifest, `external` calls need the server PIN, every call lands in the
//! audit log, keyed calls run once, and each side-effect class has its
//! timeout. Revoking a device drops its connections at once.

use crate::authz::{decide, Decision, DeviceRole, Effect, Manifest};
use crate::events::{EventItem, EventLog};
use crate::files::{self, FileRoots};
use crate::identity::Identity;
use crate::pairing::{Pairing, CODE_VALID};
use crate::protocol::{self, CallError, EventKind, Frame, Refusal, ResumePoint, CHUNK, MAX_FRAME, PROTOCOL};
use crate::registry::{AuditEntry, Device, DeviceState, Registry};
use crate::stepup::{self, StepUps};
use crate::{tls, words};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::AbortHandle;
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
/// Results of keyed calls are remembered this long.
const KEYED_FOR: Duration = Duration::from_secs(15 * 60);
const KEYED_MAX: usize = 10_000;

/// How long the server waits for a call of each side-effect class.
pub fn timeout_for(effect: Effect) -> Duration {
    match effect {
        Effect::Read => Duration::from_secs(60),
        Effect::Compute => Duration::from_secs(30 * 60),
        Effect::Write | Effect::External => Duration::from_secs(120),
    }
}

type Ws = WebSocketStream<tokio_rustls::server::TlsStream<TcpStream>>;
type Tx = SplitSink<Ws, Message>;
type Rx = SplitStream<Ws>;
type Out = mpsc::Sender<Message>;
type Outcome = Result<Value, CallError>;
type Running = Arc<Mutex<HashMap<u64, AbortHandle>>>;

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
    /// The file browser and transfers; `None` offers no files.
    pub files: Option<FileRoots>,
    /// A connection that sends nothing (not even a ping) this long is closed.
    pub idle_timeout: Duration,
    /// Calls per second a device may make, and the burst on top.
    pub calls_per_sec: f64,
    pub burst: f64,
}

impl HubConfig {
    pub fn new(server_name: impl Into<String>, app_version: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            app_version: app_version.into(),
            files: None,
            idle_timeout: Duration::from_secs(35),
            calls_per_sec: 100.0,
            burst: 400.0,
        }
    }
}

/// A file's size and modification time with its SHA-256, so a resumed
/// download of an unchanged file is not hashed again.
type Checksum = (u64, Option<SystemTime>, String);
/// A device that entered the right code, and how the server's answer reaches it.
type Waiting = (PendingDevice, oneshot::Sender<Option<DeviceRole>>);
/// An open connection: its id and the switch that drops it.
type Live = (u64, watch::Sender<bool>);

enum Slot {
    Running(Vec<oneshot::Sender<Outcome>>),
    Done(Outcome, Instant),
}

struct Upload {
    file: tokio::fs::File,
    part: PathBuf,
    meta: PathBuf,
    target: PathBuf,
    size: u64,
    sha256: String,
    written: u64,
}

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
    events: EventLog,
    keyed: Mutex<HashMap<(String, String), Slot>>,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
    sums: Mutex<HashMap<PathBuf, Checksum>>,
    config: HubConfig,
}

fn read<T: Clone>(l: &RwLock<Option<T>>) -> Option<T> {
    l.read().unwrap_or_else(|p| p.into_inner()).clone()
}

fn error_text(e: Value) -> String {
    match e {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

async fn reply(out: &Out, f: Frame) {
    let _ = out.send(Message::text(f.encode())).await;
}

async fn reply_result(out: &Out, id: u64, r: Outcome) {
    let f = match r {
        Ok(v) => Frame::Result { id, ok: Some(v), error: None },
        Err(e) => Frame::Result { id, ok: None, error: Some(e) },
    };
    reply(out, f).await;
}

fn event_frame(i: &EventItem) -> Message {
    Message::text(Frame::Event { seq: i.seq, kind: i.kind, topic: i.topic.clone(), payload: i.payload.clone() }.encode())
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
            events: EventLog::default(),
            keyed: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
            sums: Mutex::new(HashMap::new()),
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

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Hands an event of a server-placed module to every connected device.
    pub fn publish(&self, kind: EventKind, topic: &str, payload: Value) -> u64 {
        self.events.publish(kind, topic, payload)
    }

    pub fn epoch(&self) -> u64 {
        self.events.epoch()
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
        let task = tokio::spawn(async move {
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
        Ok(Listener { addr: local, stop, task: Some(task) })
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
        let Frame::Hello { protocol, app, manifest, name, resume } = hello else {
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
            Some(d) if d.state == DeviceState::Paired => self.serve(d, resume, tx, rx, stop).await,
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

    async fn serve(self: &Arc<Self>, device: Device, resume: Option<ResumePoint>, tx: Tx, mut rx: Rx, mut stop: watch::Receiver<bool>) {
        let (revoke, mut revoked) = watch::channel(false);
        let sid = self.next_session.fetch_add(1, Ordering::Relaxed);
        lock(&self.live).entry(device.id.clone()).or_default().push((sid, revoke));
        lock(&self.registry).touch(&device.id);
        self.audit(Some(&device), "connect", "ok", None);
        self.notify("devices");

        let (out, mut queue) = mpsc::channel::<Message>(32);
        let writer = tokio::spawn(async move {
            let mut tx = tx;
            while let Some(m) = queue.recv().await {
                if tx.send(m).await.is_err() {
                    break;
                }
            }
            let _ = tx.close().await;
        });

        // Welcome, then what the device missed, then live events.
        let attach = self.events.attach(resume);
        let welcome = Frame::Welcome {
            protocol: PROTOCOL,
            app: self.config.app_version.clone(),
            server: self.config.server_name.clone(),
            device_id: device.id.clone(),
            role: device.role,
            epoch: self.events.epoch(),
            seq: attach.seq,
            reload: attach.reload,
        };
        reply(&out, welcome).await;
        let mut sent_upto = resume.filter(|_| !attach.reload).map(|r| r.last_seq).unwrap_or(attach.seq);
        for item in &attach.replay {
            if item.seq > sent_upto {
                let _ = out.send(event_frame(item)).await;
                sent_upto = item.seq;
            }
        }
        sent_upto = sent_upto.max(attach.seq);
        let mut events = attach.rx;

        let running: Running = Arc::default();
        let mut uploads: HashMap<u64, Upload> = HashMap::new();
        let mut heard = Instant::now();
        let idle = self.config.idle_timeout;
        let mut tick = tokio::time::interval((idle / 4).max(Duration::from_millis(50)));
        let mut was_revoked = false;
        loop {
            tokio::select! {
                r = revoked.changed() => { was_revoked = r.is_ok(); break }
                _ = stop.changed() => break,
                _ = tick.tick() => if heard.elapsed() > idle { break },
                ev = events.recv() => match ev {
                    Ok(item) => if item.seq > sent_upto {
                        sent_upto = item.seq;
                        let _ = out.send(event_frame(&item)).await;
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        sent_upto = self.events.current();
                        reply(&out, Frame::Reload { epoch: self.events.epoch(), seq: sent_upto }).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                m = rx.next() => {
                    heard = Instant::now();
                    match m {
                        Some(Ok(Message::Text(t))) => match Frame::decode(t.as_str()) {
                            Ok(frame) => self.handle(&device.id, frame, &out, &running, &mut uploads).await,
                            Err(_) => break,
                        },
                        Some(Ok(Message::Binary(b))) => self.upload_chunk(&b, &out, &mut uploads).await,
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    }
                }
            }
        }
        for (_, h) in lock(&running).drain() {
            h.abort();
        }
        drop(uploads);
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

    /// The device as it is now (a new role applies to the next call), if
    /// still paired.
    fn current(&self, device_id: &str) -> Option<Device> {
        let d = lock(&self.registry).get(device_id).ok().flatten();
        d.filter(|d| d.state == DeviceState::Paired)
    }

    fn take_token(&self, device_id: &str) -> bool {
        let (rate, burst) = (self.config.calls_per_sec, self.config.burst);
        let mut b = lock(&self.buckets);
        let (tokens, at) = b.entry(device_id.to_string()).or_insert((burst, Instant::now()));
        *tokens = (*tokens + at.elapsed().as_secs_f64() * rate).min(burst);
        *at = Instant::now();
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    async fn handle(self: &Arc<Self>, device_id: &str, frame: Frame, out: &Out, running: &Running, uploads: &mut HashMap<u64, Upload>) {
        match frame {
            Frame::Call { id, cmd, args, key } => self.call(device_id, id, cmd, args, key, out, running).await,
            Frame::Outcome { id, key } => self.outcome(device_id, id, key, out).await,
            Frame::Cancel { id } => {
                let h = lock(running).remove(&id);
                if let Some(h) = h {
                    h.abort();
                    reply_result(out, id, Err(CallError::new("cancelled", "Cancelled."))).await;
                }
            }
            Frame::StepUp { pin } => {
                let device = lock(&self.registry).get(device_id).ok().flatten();
                let locked = lock(&self.stepups).check_locked(device_id, Instant::now());
                let hash = lock(&self.registry).meta(PIN_KEY);
                let result = match (locked, hash) {
                    (Err(e), _) => Err(e),
                    (Ok(()), None) => Err(stepup::StepUpError::NoPin),
                    (Ok(()), Some(hash)) => {
                        // Argon2 takes a moment: off the async workers, and no lock held.
                        let right = tokio::task::spawn_blocking(move || stepup::verify_pin(&hash, &pin)).await.unwrap_or(false);
                        lock(&self.stepups).record(device_id, right, Instant::now())
                    }
                };
                let frame = match result {
                    Ok(valid) => {
                        self.audit(device.as_ref(), "step_up", "ok", None);
                        Frame::SteppedUp { ok: true, valid_secs: valid.as_secs(), message: None }
                    }
                    Err(e) => {
                        self.audit(device.as_ref(), "step_up", "failed", Some(&e.to_string()));
                        Frame::SteppedUp { ok: false, valid_secs: 0, message: Some(e.to_string()) }
                    }
                };
                reply(out, frame).await;
            }
            Frame::Ping => reply(out, Frame::Pong).await,
            Frame::Download { id, path, offset } => self.download(device_id, id, path, offset, out, running).await,
            Frame::Upload { id, name, size, sha256 } => self.upload_begin(device_id, id, name, size, sha256, out, uploads).await,
            Frame::UploadDone { id } => self.upload_done(device_id, id, out, uploads).await,
            _ => {}
        }
    }

    async fn run(&self, cmd: String, args: Value, effect: Effect) -> Outcome {
        let Some(d) = read(&self.dispatch) else {
            return Err(CallError::new("unavailable", "The server is still starting."));
        };
        match tokio::time::timeout(timeout_for(effect), d.call(cmd, args)).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(CallError::new("failed", error_text(e))),
            Err(_) => Err(CallError::new("timeout", "The server gave up waiting for this call. Check whether it took effect.")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn call(self: &Arc<Self>, device_id: &str, id: u64, cmd: String, args: Value, key: Option<String>, out: &Out, running: &Running) {
        let Some(device) = self.current(device_id) else {
            reply_result(out, id, Err(CallError::new("denied", "This device is no longer paired with the server."))).await;
            return;
        };
        if !self.take_token(&device.id) {
            self.audit(Some(&device), &cmd, "rate_limited", None);
            reply_result(out, id, Err(CallError::new("rate_limited", "Too many calls from this device; slow down."))).await;
            return;
        }
        if let Some(builtin) = cmd.strip_prefix("gateway|") {
            let r = self.builtin(builtin, &args);
            self.audit(Some(&device), &cmd, if r.is_ok() { "ok" } else { "error" }, r.as_ref().err().map(|e| e.message.as_str()));
            reply_result(out, id, r).await;
            return;
        }
        let granted = lock(&self.stepups).granted(&device.id, Instant::now());
        let decision = match decide(&self.manifest, device.role, &cmd) {
            Decision::StepUp if granted => Decision::Allow,
            d => d,
        };
        match decision {
            Decision::Deny(why) => {
                self.audit(Some(&device), &cmd, "denied", Some(&why));
                reply_result(out, id, Err(CallError::new("denied", why))).await;
            }
            Decision::StepUp => {
                self.audit(Some(&device), &cmd, "step_up_required", None);
                reply_result(out, id, Err(CallError::new("step_up_required", "Enter the server PIN to do this from another device."))).await;
            }
            Decision::Allow => {
                let effect = self.manifest.lookup(&cmd).map(|e| e.1).unwrap_or(Effect::Read);
                match key {
                    Some(key) => self.keyed_call(device, id, cmd, args, key, effect, out).await,
                    None => {
                        let (hub, out2, running2) = (self.clone(), out.clone(), running.clone());
                        let task = tokio::spawn(async move {
                            let r = hub.run(cmd.clone(), args, effect).await;
                            hub.audit(Some(&device), &cmd, if r.is_ok() { "ok" } else { "error" }, r.as_ref().err().map(|e| e.message.as_str()));
                            lock(&running2).remove(&id);
                            reply_result(&out2, id, r).await;
                        });
                        lock(running).insert(id, task.abort_handle());
                    }
                }
            }
        }
    }

    /// A call with an idempotency key runs once per device and key; a repeat
    /// (or an `outcome` question) gets the same result, waiting while it runs.
    /// Keyed calls are not cancelled: their outcome must stay knowable.
    #[allow(clippy::too_many_arguments)]
    async fn keyed_call(self: &Arc<Self>, device: Device, id: u64, cmd: String, args: Value, key: String, effect: Effect, out: &Out) {
        let slot = (device.id.clone(), key);
        let waiter = {
            let mut keyed = lock(&self.keyed);
            prune_keyed(&mut keyed);
            match keyed.get_mut(&slot) {
                Some(Slot::Done(r, _)) => Err(r.clone()),
                Some(Slot::Running(waiters)) => {
                    let (t, r) = oneshot::channel();
                    waiters.push(t);
                    Ok(Some(r))
                }
                None => {
                    keyed.insert(slot.clone(), Slot::Running(Vec::new()));
                    Ok(None)
                }
            }
        };
        match waiter {
            Err(done) => reply_result(out, id, done).await,
            Ok(Some(wait)) => {
                let out = out.clone();
                tokio::spawn(async move {
                    let r = wait.await.unwrap_or_else(|_| Err(CallError::new("failed", "The call ended without a result.")));
                    reply_result(&out, id, r).await;
                });
            }
            Ok(None) => {
                let (hub, out) = (self.clone(), out.clone());
                tokio::spawn(async move {
                    let r = hub.run(cmd.clone(), args, effect).await;
                    hub.audit(Some(&device), &cmd, if r.is_ok() { "ok" } else { "error" }, r.as_ref().err().map(|e| e.message.as_str()));
                    let waiters = {
                        let mut keyed = lock(&hub.keyed);
                        match keyed.insert(slot, Slot::Done(r.clone(), Instant::now())) {
                            Some(Slot::Running(w)) => w,
                            _ => Vec::new(),
                        }
                    };
                    for w in waiters {
                        let _ = w.send(r.clone());
                    }
                    reply_result(&out, id, r).await;
                });
            }
        }
    }

    async fn outcome(&self, device_id: &str, id: u64, key: String, out: &Out) {
        let slot = (device_id.to_string(), key);
        let waiter = {
            let mut keyed = lock(&self.keyed);
            match keyed.get_mut(&slot) {
                Some(Slot::Done(r, _)) => Err(Some(r.clone())),
                Some(Slot::Running(waiters)) => {
                    let (t, r) = oneshot::channel();
                    waiters.push(t);
                    Ok(r)
                }
                None => Err(None),
            }
        };
        match waiter {
            Err(Some(done)) => reply_result(out, id, done).await,
            Err(None) => {
                let e = CallError::new("unknown", "The server has no record of that call: it never arrived, so sending it again is safe.");
                reply_result(out, id, Err(e)).await;
            }
            Ok(wait) => {
                let out = out.clone();
                tokio::spawn(async move {
                    let r = wait.await.unwrap_or_else(|_| Err(CallError::new("failed", "The call ended without a result.")));
                    reply_result(&out, id, r).await;
                });
            }
        }
    }

    // ── files ────────────────────────────────────────────────────────────

    fn files(&self) -> Result<&FileRoots, CallError> {
        self.config.files.as_ref().ok_or_else(|| CallError::new("unavailable", "This server offers no files."))
    }

    /// `gateway|fs_list {path?}` — readable by every role.
    fn builtin(&self, name: &str, args: &Value) -> Outcome {
        match name {
            "fs_list" => {
                let path = args.get("path").and_then(Value::as_str);
                let listing = self.files()?.list(path).map_err(|e| CallError::new("failed", e))?;
                serde_json::to_value(listing).map_err(|e| CallError::new("failed", e.to_string()))
            }
            _ => Err(CallError::new("denied", format!("gateway|{name} is not offered."))),
        }
    }

    async fn checksum(&self, path: &std::path::Path, size: u64, modified: Option<SystemTime>) -> Result<String, String> {
        if let Some((s, m, sum)) = lock(&self.sums).get(path) {
            if *s == size && *m == modified {
                return Ok(sum.clone());
            }
        }
        let p = path.to_path_buf();
        let sum = tokio::task::spawn_blocking(move || files::sha256_file(&p)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        lock(&self.sums).insert(path.to_path_buf(), (size, modified, sum.clone()));
        Ok(sum)
    }

    async fn download(self: &Arc<Self>, device_id: &str, id: u64, path: String, offset: u64, out: &Out, running: &Running) {
        let Some(device) = self.current(device_id) else { return };
        let checked = self.files().and_then(|f| f.check(&path).map_err(|e| CallError::new("denied", e)));
        let p = match checked {
            Ok(p) if p.is_file() => p,
            Ok(_) => return reply_result(out, id, Err(CallError::new("failed", format!("{path} is not a file.")))).await,
            Err(e) => {
                self.audit(Some(&device), "gateway|download", "denied", Some(&path));
                return reply_result(out, id, Err(e)).await;
            }
        };
        self.audit(Some(&device), "gateway|download", "ok", Some(&files::display(&p)));
        let (hub, out2, running2) = (self.clone(), out.clone(), running.clone());
        let task = tokio::spawn(async move {
            let r = hub.send_file(id, &p, offset, &out2).await;
            lock(&running2).remove(&id);
            if let Err(e) = r {
                reply_result(&out2, id, Err(CallError::new("failed", e))).await;
            }
        });
        lock(running).insert(id, task.abort_handle());
    }

    async fn send_file(&self, id: u64, p: &std::path::Path, offset: u64, out: &Out) -> Result<(), String> {
        let meta = tokio::fs::metadata(p).await.map_err(|e| e.to_string())?;
        let size = meta.len();
        if offset > size {
            return Err("The file on the server is shorter than the part already downloaded.".into());
        }
        let sha256 = self.checksum(p, size, meta.modified().ok()).await?;
        reply(out, Frame::Transfer { id, size, sha256, offset }).await;
        let mut f = tokio::fs::File::open(p).await.map_err(|e| e.to_string())?;
        f.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| e.to_string())?;
        let mut pos = offset;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = f.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            out.send(Message::binary(protocol::chunk(id, pos, &buf[..n]))).await.map_err(|_| "closed".to_string())?;
            pos += n as u64;
        }
        reply(out, Frame::TransferDone { id }).await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_begin(&self, device_id: &str, id: u64, name: String, size: u64, sha256: String, out: &Out, uploads: &mut HashMap<u64, Upload>) {
        let Some(device) = self.current(device_id) else { return };
        if device.role == DeviceRole::ReadOnly {
            self.audit(Some(&device), "gateway|upload", "denied", Some(&name));
            return reply_result(out, id, Err(CallError::new("denied", "This device may only read."))).await;
        }
        let begun = async {
            let target = self.files().map_err(|e| e.message)?.upload_target(&name)?;
            let (part, meta) = files::part_paths(&target);
            let tag = format!("{size} {sha256}");
            let resumable = tokio::fs::read_to_string(&meta).await.ok().is_some_and(|m| m == tag);
            let have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
            let offset = if resumable && have <= size { have } else { 0 };
            if offset == 0 {
                tokio::fs::write(&meta, tag.as_bytes()).await.map_err(|e| e.to_string())?;
            }
            let mut file = tokio::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&part).await.map_err(|e| e.to_string())?;
            file.set_len(offset).await.map_err(|e| e.to_string())?;
            file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| e.to_string())?;
            Ok::<_, String>(Upload { file, part, meta, target, size, sha256, written: offset })
        }
        .await;
        match begun {
            Ok(u) => {
                self.audit(Some(&device), "gateway|upload", "ok", Some(&format!("{name}, from byte {}", u.written)));
                let offset = u.written;
                uploads.insert(id, u);
                reply_result(out, id, Ok(json!({ "offset": offset }))).await;
            }
            Err(e) => reply_result(out, id, Err(CallError::new("failed", e))).await,
        }
    }

    async fn upload_chunk(&self, bytes: &[u8], out: &Out, uploads: &mut HashMap<u64, Upload>) {
        let Some((id, offset, data)) = protocol::parse_chunk(bytes) else { return };
        let Some(u) = uploads.get_mut(&id) else { return };
        let r = if offset != u.written {
            Err(format!("Expected byte {} but got {offset}.", u.written))
        } else if u.written + data.len() as u64 > u.size {
            Err("More data than announced.".to_string())
        } else {
            u.file.write_all(data).await.map_err(|e| e.to_string()).map(|_| u.written += data.len() as u64)
        };
        if let Err(e) = r {
            uploads.remove(&id);
            reply_result(out, id, Err(CallError::new("failed", e))).await;
        }
    }

    async fn upload_done(&self, device_id: &str, id: u64, out: &Out, uploads: &mut HashMap<u64, Upload>) {
        let Some(mut u) = uploads.remove(&id) else {
            return reply_result(out, id, Err(CallError::new("failed", "No such upload."))).await;
        };
        let device = self.current(device_id);
        let finished = async {
            u.file.flush().await.map_err(|e| e.to_string())?;
            u.file.sync_all().await.map_err(|e| e.to_string())?;
            drop(u.file);
            if u.written != u.size {
                return Err(format!("Only {} of {} bytes arrived; send the rest.", u.written, u.size));
            }
            let part = u.part.clone();
            let sum = tokio::task::spawn_blocking(move || files::sha256_file(&part)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
            if sum != u.sha256 {
                let _ = tokio::fs::remove_file(&u.part).await;
                let _ = tokio::fs::remove_file(&u.meta).await;
                return Err("The checksum does not match; the upload was discarded.".into());
            }
            tokio::fs::rename(&u.part, &u.target).await.map_err(|e| e.to_string())?;
            let _ = tokio::fs::remove_file(&u.meta).await;
            Ok(json!({ "path": files::display(&u.target), "size": u.size, "sha256": u.sha256 }))
        }
        .await;
        self.audit(device.as_ref(), "gateway|upload_done", if finished.is_ok() { "ok" } else { "error" }, finished.as_ref().err().map(String::as_str));
        reply_result(out, id, finished.map_err(|e| CallError::new("failed", e))).await;
    }
}

fn prune_keyed(keyed: &mut HashMap<(String, String), Slot>) {
    keyed.retain(|_, s| !matches!(s, Slot::Done(_, at) if at.elapsed() > KEYED_FOR));
    if keyed.len() > KEYED_MAX {
        let mut done: Vec<(Instant, (String, String))> =
            keyed.iter().filter_map(|(k, s)| if let Slot::Done(_, at) = s { Some((*at, k.clone())) } else { None }).collect();
        done.sort();
        for (_, k) in done.into_iter().take(keyed.len() - KEYED_MAX) {
            keyed.remove(&k);
        }
    }
}

/// A bound gateway; dropping it stops listening and ends every connection.
pub struct Listener {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Listener {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops and waits until the socket is closed, so the same address can be
    /// bound again at once.
    pub async fn close(mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.task.take() {
            let _ = t.await;
        }
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
