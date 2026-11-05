//! The client's end (PLAN-SERVER-CLIENT §4.1, §5, §6.4, §6.5): pair once,
//! then connect with the pinned server key — resuming events where the last
//! connection stopped — and call, ask after keyed calls, and move files.

use crate::authz::DeviceRole;
use crate::files;
use crate::identity::{fingerprint, Identity};
use crate::pairing::CODE_VALID;
use crate::protocol::{self, CallError, EventKind, Frame, Refusal, ResumePoint, CHUNK, PROTOCOL};
use crate::server::ws_config;
use crate::{tls, words};
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The default wait for a call's result (the server's longest class, compute,
/// is 30 minutes; callers pass their own for those).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Events kept for a consumer that has not taken them yet.
const EVENT_QUEUE: usize = 16_384;

type Ws = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

/// What this PC says about itself in `hello`.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub app: String,
    /// [`crate::Manifest::hash`] of this build.
    pub manifest: String,
}

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Where the last connection to this server stopped.
    pub resume: Option<ResumePoint>,
    /// A ping this often keeps the connection alive (§5: 10 s).
    pub ping_every: Duration,
    /// Nothing heard from the server this long: the connection is dead.
    pub dead_after: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self { resume: None, ping_every: Duration::from_secs(10), dead_after: Duration::from_secs(30) }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Welcome {
    pub server: String,
    pub app: String,
    pub device_id: String,
    pub role: DeviceRole,
    pub epoch: u64,
    pub seq: u64,
    /// The resume point could not be honoured: reload the pages' state.
    pub reload: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Event { seq: u64, kind: EventKind, topic: String, payload: Value },
    /// Events were lost: reload the pages' state.
    Reload { epoch: u64, seq: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectError {
    Network(String),
    /// The handshake failed; `key_changed` when the server showed another key
    /// than the pinned one.
    Tls { message: String, key_changed: bool },
    Refused { reason: Refusal, message: String },
    Unpaired,
    Protocol(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(m) | Self::Protocol(m) => f.write_str(m),
            Self::Tls { key_changed: true, .. } => f.write_str(
                "The server's key is not the one this PC paired with. Someone may be in between, or the server was reinstalled: pair again only if you know why.",
            ),
            Self::Tls { message, .. } => write!(f, "Secure connection failed: {message}"),
            Self::Refused { message, .. } => f.write_str(message),
            Self::Unpaired => f.write_str("This PC is not paired with the server (any more). Pair it again."),
        }
    }
}

async fn open(addr: &str, identity: &Identity, pinned: Option<[u8; 32]>) -> Result<(Ws, [u8; 32]), ConnectError> {
    let (cfg, verifier) = tls::client_config(identity, pinned).map_err(ConnectError::Protocol)?;
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| ConnectError::Network(format!("{addr} does not answer.")))?
        .map_err(|e| ConnectError::Network(format!("{addr}: {e}")))?;
    let _ = tcp.set_nodelay(true);
    let name = ServerName::try_from(tls::SERVER_NAME).map_err(|e| ConnectError::Protocol(e.to_string()))?;
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, TlsConnector::from(cfg).connect(name, tcp))
        .await
        .map_err(|_| ConnectError::Network(format!("{addr} does not complete the handshake.")))?
        .map_err(|e| ConnectError::Tls { message: e.to_string(), key_changed: verifier.mismatch() })?;
    let key = verifier.seen().ok_or_else(|| ConnectError::Protocol("no server key".into()))?;
    let (ws, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::client_async_with_config("wss://quantsuite.server/gateway", tls, Some(ws_config())),
    )
    .await
    .map_err(|_| ConnectError::Network(format!("{addr} does not answer.")))?
    .map_err(|e| ConnectError::Protocol(e.to_string()))?;
    Ok((ws, key))
}

async fn next_frame(ws: &mut Ws) -> Option<Frame> {
    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(t)) => return Frame::decode(t.as_str()).ok(),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => continue,
        }
    }
    None
}

async fn hello(ws: &mut Ws, protocol: u32, me: &ClientInfo, resume: Option<ResumePoint>, pair: bool) -> Result<Frame, ConnectError> {
    let f = Frame::Hello { protocol, app: me.app.clone(), manifest: me.manifest.clone(), name: me.name.clone(), resume, pair };
    ws.send(Message::text(f.encode())).await.map_err(|e| ConnectError::Network(e.to_string()))?;
    match tokio::time::timeout(CONNECT_TIMEOUT, next_frame(ws)).await {
        Ok(Some(Frame::Refused { reason, message })) => Err(ConnectError::Refused { reason, message }),
        Ok(Some(f)) => Ok(f),
        _ => Err(ConnectError::Protocol("The server closed the connection.".into())),
    }
}

// ── pairing ────────────────────────────────────────────────────────────────

/// A pairing in progress: the code was right, the server's user now compares
/// the words. Dropping it cancels the pairing.
pub struct PairingAttempt {
    ws: Ws,
    pub server_key: [u8; 32],
    pub server_name: String,
    /// Computed here from the key this PC saw — never taken from the server.
    pub words: [String; 4],
}

impl PairingAttempt {
    pub fn server_fingerprint(&self) -> String {
        fingerprint(&self.server_key)
    }

    /// Waits for the server's user; on success this PC may pin `server_key` —
    /// once its own user has confirmed the same words.
    pub async fn finish(mut self) -> Result<(String, DeviceRole), String> {
        match tokio::time::timeout(CODE_VALID, next_frame(&mut self.ws)).await {
            Ok(Some(Frame::Paired { device_id, role })) => {
                let _ = self.ws.close(None).await;
                Ok((device_id, role))
            }
            Ok(Some(Frame::PairFailed { message })) => Err(message),
            Ok(_) => Err("The server closed the connection.".into()),
            Err(_) => Err("The server did not confirm in time.".into()),
        }
    }
}

pub async fn pair(addr: &str, identity: &Identity, code: &str, me: &ClientInfo) -> Result<PairingAttempt, String> {
    let (mut ws, server_key) = open(addr, identity, None).await.map_err(|e| e.to_string())?;
    let server_name = match hello(&mut ws, PROTOCOL, me, None, true).await.map_err(|e| e.to_string())? {
        Frame::Unpaired { server } => server,
        Frame::Welcome { .. } => {
            return Err("This PC is already paired with that server. Revoke it under Devices on the server, then pair again.".into())
        }
        _ => return Err("Unexpected answer from the server.".into()),
    };
    let f = Frame::Pair { code: code.to_string(), name: me.name.clone() };
    ws.send(Message::text(f.encode())).await.map_err(|e| e.to_string())?;
    match tokio::time::timeout(CONNECT_TIMEOUT, next_frame(&mut ws)).await {
        Ok(Some(Frame::PairPending { .. })) => {}
        Ok(Some(Frame::PairFailed { message })) => return Err(message),
        _ => return Err("The server closed the connection.".into()),
    }
    let words = words::pairing_words(&server_key, &identity.public_key());
    Ok(PairingAttempt { ws, server_key, server_name, words })
}

// ── a connection ──────────────────────────────────────────────────────────

enum TransferMsg {
    Begin { size: u64, sha256: String, offset: u64 },
    Chunk { offset: u64, data: Vec<u8> },
    Done,
    Failed(CallError),
}

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Frame>>>>;
type Transfers = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<TransferMsg>>>>;

#[derive(Debug, Clone)]
pub struct CallOptions {
    /// An idempotency key: the server runs the call once per key, and
    /// [`Session::outcome`] can ask for its result later.
    pub key: Option<String>,
    pub timeout: Duration,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self { key: None, timeout: CALL_TIMEOUT }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferReport {
    pub bytes: u64,
    /// Where this attempt started (0, or the part already there).
    pub resumed_from: u64,
    pub sha256: String,
    pub path: String,
}

pub struct Session {
    out: mpsc::Sender<Message>,
    waiting: Waiting,
    transfers: Transfers,
    step_up: Arc<Mutex<Option<oneshot::Sender<Frame>>>>,
    events: Mutex<Option<mpsc::Receiver<ClientEvent>>>,
    last_seq: Arc<AtomicU64>,
    next: AtomicU64,
    closed: watch::Receiver<bool>,
    /// The last ping's round trip, in microseconds (0 before the first pong).
    rtt_us: Arc<AtomicU64>,
    pub welcome: Welcome,
    tasks: Vec<JoinHandle<()>>,
}

pub async fn connect(addr: &str, identity: &Identity, server_key: [u8; 32], me: &ClientInfo) -> Result<Session, ConnectError> {
    connect_with(addr, identity, server_key, me, ConnectOptions::default()).await
}

pub async fn connect_with(addr: &str, identity: &Identity, server_key: [u8; 32], me: &ClientInfo, opts: ConnectOptions) -> Result<Session, ConnectError> {
    connect_speaking(addr, identity, server_key, me, opts, PROTOCOL).await
}

/// [`connect_with`] with another protocol number (tests of version refusal).
#[doc(hidden)]
pub async fn connect_speaking(
    addr: &str,
    identity: &Identity,
    server_key: [u8; 32],
    me: &ClientInfo,
    opts: ConnectOptions,
    protocol: u32,
) -> Result<Session, ConnectError> {
    let (mut ws, _) = open(addr, identity, Some(server_key)).await?;
    match hello(&mut ws, protocol, me, opts.resume, false).await? {
        Frame::Welcome { server, app, device_id, role, epoch, seq, reload, .. } => {
            // Replayed events follow the resume point; otherwise live ones
            // follow the welcome.
            let from = match opts.resume {
                Some(r) if !reload && r.epoch == epoch => r.last_seq,
                _ => seq,
            };
            Ok(Session::start(ws, Welcome { server, app, device_id, role, epoch, seq, reload }, from, &opts))
        }
        Frame::Unpaired { .. } => Err(ConnectError::Unpaired),
        _ => Err(ConnectError::Protocol("Unexpected answer from the server.".into())),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A fresh idempotency key.
pub fn new_key() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    let _ = ring::rand::SystemRandom::new().fill(&mut b);
    hex::encode(b)
}

impl Session {
    fn start(ws: Ws, welcome: Welcome, from: u64, opts: &ConnectOptions) -> Self {
        let (mut sink, mut stream) = ws.split();
        let (out, mut queue) = mpsc::channel::<Message>(32);
        let (closed_tx, closed) = watch::channel(false);
        let waiting: Waiting = Arc::default();
        let transfers: Transfers = Arc::default();
        let step_up: Arc<Mutex<Option<oneshot::Sender<Frame>>>> = Arc::default();
        let (ev_tx, ev_rx) = mpsc::channel(EVENT_QUEUE);
        let last_seq = Arc::new(AtomicU64::new(from));
        let ping_every = opts.ping_every;
        // The round trip of each ping (the connection-quality line, §5).
        let t0 = std::time::Instant::now();
        let ping_at = Arc::new(AtomicU64::new(0));
        let rtt_us = Arc::new(AtomicU64::new(0));
        let sent = ping_at.clone();
        let writer = tokio::spawn(async move {
            // The first ping at once: the round trip is known right away.
            let mut ping = tokio::time::interval(ping_every);
            loop {
                tokio::select! {
                    m = queue.recv() => match m {
                        Some(m) => if sink.send(m).await.is_err() { break },
                        None => break,
                    },
                    _ = ping.tick() => {
                        sent.store(t0.elapsed().as_micros() as u64 + 1, Ordering::SeqCst);
                        if sink.send(Message::text(Frame::Ping.encode())).await.is_err() { break }
                    }
                }
            }
            let _ = sink.close().await;
        });
        let (pinged, rtt) = (ping_at.clone(), rtt_us.clone());
        let (w, t, s, seq) = (waiting.clone(), transfers.clone(), step_up.clone(), last_seq.clone());
        let dead_after = opts.dead_after;
        let reader = tokio::spawn(async move {
            loop {
                let msg = match tokio::time::timeout(dead_after, stream.next()).await {
                    Ok(Some(Ok(m))) => m,
                    _ => break,
                };
                match msg {
                    Message::Text(text) => match Frame::decode(text.as_str()) {
                        Ok(Frame::Result { id, ok, error }) => {
                            if let Some(tx) = lock(&w).remove(&id) {
                                let _ = tx.send(Frame::Result { id, ok, error });
                            } else if let Some(tx) = lock(&t).get(&id) {
                                let _ = tx.send(TransferMsg::Failed(error.unwrap_or_else(|| CallError::new("failed", "The transfer failed."))));
                            }
                        }
                        Ok(Frame::Event { seq: n, kind, topic, payload }) => {
                            // Never twice: the server sends each event once, this guards a resume.
                            if n > seq.load(Ordering::SeqCst) {
                                seq.store(n, Ordering::SeqCst);
                                let _ = ev_tx.try_send(ClientEvent::Event { seq: n, kind, topic, payload });
                            }
                        }
                        Ok(Frame::Reload { epoch, seq: n }) => {
                            seq.store(n, Ordering::SeqCst);
                            let _ = ev_tx.try_send(ClientEvent::Reload { epoch, seq: n });
                        }
                        Ok(Frame::Transfer { id, size, sha256, offset }) => {
                            if let Some(tx) = lock(&t).get(&id) {
                                let _ = tx.send(TransferMsg::Begin { size, sha256, offset });
                            }
                        }
                        Ok(Frame::TransferDone { id }) => {
                            if let Some(tx) = lock(&t).get(&id) {
                                let _ = tx.send(TransferMsg::Done);
                            }
                        }
                        Ok(Frame::Pong) => {
                            let at = pinged.load(Ordering::SeqCst);
                            if at > 0 {
                                rtt.store((t0.elapsed().as_micros() as u64 + 1).saturating_sub(at).max(1), Ordering::SeqCst);
                            }
                        }
                        Ok(f @ Frame::SteppedUp { .. }) => {
                            if let Some(tx) = lock(&s).take() {
                                let _ = tx.send(f);
                            }
                        }
                        _ => {}
                    },
                    Message::Binary(b) => {
                        if let Some((id, offset, data)) = protocol::parse_chunk(&b) {
                            if let Some(tx) = lock(&t).get(&id) {
                                let _ = tx.send(TransferMsg::Chunk { offset, data: data.to_vec() });
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            // Closed first, then every waiting call and transfer fails (its
            // sender is dropped): one that registers in between sees `closed`.
            let _ = closed_tx.send(true);
            lock(&w).clear();
            lock(&t).clear();
            lock(&s).take();
        });
        Self {
            out,
            waiting,
            transfers,
            step_up,
            events: Mutex::new(Some(ev_rx)),
            last_seq,
            next: AtomicU64::new(1),
            closed,
            rtt_us,
            welcome,
            tasks: vec![writer, reader],
        }
    }

    /// The round trip of the last ping, once one came back.
    pub fn rtt(&self) -> Option<Duration> {
        match self.rtt_us.load(Ordering::SeqCst) {
            0 => None,
            us => Some(Duration::from_micros(us)),
        }
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Resolves when the server closes the connection (or drops it).
    pub async fn closed(&self) {
        let mut c = self.closed.clone();
        while !*c.borrow() {
            if c.changed().await.is_err() {
                return;
            }
        }
    }

    /// The events of this connection, in order — taken once.
    pub fn take_events(&self) -> Option<mpsc::Receiver<ClientEvent>> {
        lock(&self.events).take()
    }

    /// Where the next connection should resume.
    pub fn resume_point(&self) -> ResumePoint {
        ResumePoint { epoch: self.welcome.epoch, last_seq: self.last_seq.load(Ordering::SeqCst) }
    }

    fn id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    async fn request(&self, id: u64, f: Frame, timeout: Duration) -> Result<Value, CallError> {
        let closed = || CallError::new("closed", "The connection to the server is closed.");
        let (tx, rx) = oneshot::channel();
        lock(&self.waiting).insert(id, tx);
        if self.is_closed() {
            lock(&self.waiting).remove(&id);
            return Err(closed());
        }
        if self.out.send(Message::text(f.encode())).await.is_err() {
            lock(&self.waiting).remove(&id);
            return Err(closed());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Frame::Result { ok, error, .. })) => match error {
                Some(e) => Err(e),
                None => Ok(ok.unwrap_or(Value::Null)),
            },
            Ok(_) => Err(closed()),
            Err(_) => {
                lock(&self.waiting).remove(&id);
                Err(CallError::new("timeout", "The server did not answer in time."))
            }
        }
    }

    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value, CallError> {
        self.call_with(cmd, args, CallOptions::default()).await
    }

    pub async fn call_with(&self, cmd: &str, args: Value, opts: CallOptions) -> Result<Value, CallError> {
        let id = self.id();
        self.request(id, Frame::Call { id, cmd: cmd.to_string(), args, key: opts.key }, opts.timeout).await
    }

    /// The result of the keyed call — waiting while it still runs. `unknown`
    /// means it never reached the server, so sending it again is safe.
    pub async fn outcome(&self, key: &str, timeout: Duration) -> Result<Value, CallError> {
        let id = self.id();
        self.request(id, Frame::Outcome { id, key: key.to_string() }, timeout).await
    }

    /// Stops a call that is still running (keyed calls always finish).
    pub async fn cancel(&self, id: u64) {
        let _ = self.out.send(Message::text(Frame::Cancel { id }.encode())).await;
    }

    /// Enters the server PIN; returns how many seconds it stays valid.
    pub async fn step_up(&self, pin: &str) -> Result<u64, String> {
        if self.is_closed() {
            return Err("The connection to the server is closed.".into());
        }
        let (tx, rx) = oneshot::channel();
        *lock(&self.step_up) = Some(tx);
        let f = Frame::StepUp { pin: pin.to_string() };
        self.out.send(Message::text(f.encode())).await.map_err(|_| "The connection to the server is closed.".to_string())?;
        match tokio::time::timeout(CONNECT_TIMEOUT, rx).await {
            Ok(Ok(Frame::SteppedUp { ok: true, valid_secs, .. })) => Ok(valid_secs),
            Ok(Ok(Frame::SteppedUp { message, .. })) => Err(message.unwrap_or_else(|| "Wrong PIN.".into())),
            _ => Err("The server did not answer.".into()),
        }
    }

    /// The server's file browser: the roots, or one folder.
    pub async fn list_files(&self, path: Option<&str>) -> Result<files::Listing, CallError> {
        let args = match path {
            Some(p) => serde_json::json!({ "path": p }),
            None => serde_json::json!({}),
        };
        let v = self.call("gateway|fs_list", args).await?;
        serde_json::from_value(v).map_err(|e| CallError::new("failed", e.to_string()))
    }

    /// Downloads a server file to `local`, continuing a `local.part` left by an
    /// interrupted attempt; the whole file's SHA-256 must match.
    pub async fn download(&self, remote: &str, local: &Path) -> Result<TransferReport, String> {
        self.download_until(remote, local, None).await
    }

    /// [`Session::download`] that gives up after `stop_after` bytes (tests of
    /// an interrupted transfer).
    #[doc(hidden)]
    pub async fn download_until(&self, remote: &str, local: &Path, stop_after: Option<u64>) -> Result<TransferReport, String> {
        let (part, _) = files::part_paths(local);
        let offset = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
        let id = self.id();
        let (tx, mut rx) = mpsc::unbounded_channel();
        lock(&self.transfers).insert(id, tx);
        let result = async {
            if self.is_closed() {
                return Err("The connection to the server is closed.".to_string());
            }
            let f = Frame::Download { id, path: remote.to_string(), offset };
            self.out.send(Message::text(f.encode())).await.map_err(|_| "The connection to the server is closed.".to_string())?;
            let (size, sha256) = match rx.recv().await {
                Some(TransferMsg::Begin { size, sha256, offset: o }) if o == offset => (size, sha256),
                Some(TransferMsg::Begin { .. }) => return Err("The server started at another byte.".into()),
                Some(TransferMsg::Failed(e)) => return Err(e.message),
                _ => return Err("The connection to the server is closed.".into()),
            };
            let mut file = tokio::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&part).await.map_err(|e| e.to_string())?;
            file.set_len(offset).await.map_err(|e| e.to_string())?;
            file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| e.to_string())?;
            let mut pos = offset;
            loop {
                match rx.recv().await {
                    Some(TransferMsg::Chunk { offset: o, data }) => {
                        if o != pos {
                            return Err(format!("Expected byte {pos} but got {o}."));
                        }
                        file.write_all(&data).await.map_err(|e| e.to_string())?;
                        pos += data.len() as u64;
                        if stop_after.is_some_and(|n| pos - offset >= n) {
                            file.flush().await.map_err(|e| e.to_string())?;
                            return Err("stopped".into());
                        }
                    }
                    Some(TransferMsg::Done) => break,
                    Some(TransferMsg::Failed(e)) => return Err(e.message),
                    Some(TransferMsg::Begin { .. }) => {}
                    None => {
                        let _ = file.flush().await;
                        return Err("The connection to the server is closed; the download continues next time.".into());
                    }
                }
            }
            file.flush().await.map_err(|e| e.to_string())?;
            file.sync_all().await.map_err(|e| e.to_string())?;
            drop(file);
            if pos != size {
                return Err(format!("Only {pos} of {size} bytes arrived."));
            }
            let p = part.clone();
            let got = tokio::task::spawn_blocking(move || files::sha256_file(&p)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
            if got != sha256 {
                let _ = tokio::fs::remove_file(&part).await;
                return Err("The checksum does not match; the download was discarded.".into());
            }
            tokio::fs::rename(&part, local).await.map_err(|e| e.to_string())?;
            Ok(TransferReport { bytes: size, resumed_from: offset, sha256, path: local.display().to_string() })
        }
        .await;
        lock(&self.transfers).remove(&id);
        result
    }

    /// Uploads `local` into the server's transfers folder as `name`, continuing
    /// a part the server kept from an interrupted attempt.
    pub async fn upload(&self, local: &Path, name: &str) -> Result<TransferReport, String> {
        self.upload_until(local, name, None).await
    }

    #[doc(hidden)]
    pub async fn upload_until(&self, local: &Path, name: &str, stop_after: Option<u64>) -> Result<TransferReport, String> {
        let size = tokio::fs::metadata(local).await.map_err(|e| e.to_string())?.len();
        let l = local.to_path_buf();
        let sha256 = tokio::task::spawn_blocking(move || files::sha256_file(&l)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let id = self.id();
        let begun = self
            .request(id, Frame::Upload { id, name: name.to_string(), size, sha256: sha256.clone() }, CONNECT_TIMEOUT)
            .await
            .map_err(|e| e.message)?;
        let offset = begun.get("offset").and_then(Value::as_u64).unwrap_or(0);
        let mut file = tokio::fs::File::open(local).await.map_err(|e| e.to_string())?;
        file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| e.to_string())?;
        let mut pos = offset;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = file.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            self.out
                .send(Message::binary(protocol::chunk(id, pos, &buf[..n])))
                .await
                .map_err(|_| "The connection to the server is closed; the upload continues next time.".to_string())?;
            pos += n as u64;
            if stop_after.is_some_and(|s| pos - offset >= s) {
                return Err("stopped".into());
            }
        }
        let done = self.request(id, Frame::UploadDone { id }, Duration::from_secs(600)).await.map_err(|e| e.message)?;
        let path = done.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(TransferReport { bytes: size, resumed_from: offset, sha256, path })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}
