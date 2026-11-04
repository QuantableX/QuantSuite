//! The client's end (PLAN-SERVER-CLIENT §4.1, §6.4): pair once, then connect
//! with the pinned server key and call.

use crate::authz::DeviceRole;
use crate::identity::{fingerprint, Identity};
use crate::pairing::CODE_VALID;
use crate::protocol::{CallError, Frame, Refusal, PROTOCOL};
use crate::server::ws_config;
use crate::{tls, words};
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(300);

type Ws = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

/// What this PC says about itself in `hello`.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub app: String,
    /// [`crate::Manifest::hash`] of this build.
    pub manifest: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Welcome {
    pub server: String,
    pub app: String,
    pub device_id: String,
    pub role: DeviceRole,
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

async fn hello(ws: &mut Ws, protocol: u32, me: &ClientInfo) -> Result<Frame, ConnectError> {
    let f = Frame::Hello { protocol, app: me.app.clone(), manifest: me.manifest.clone(), name: me.name.clone() };
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
    let server_name = match hello(&mut ws, PROTOCOL, me).await.map_err(|e| e.to_string())? {
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

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Frame>>>>;

pub struct Session {
    out: mpsc::Sender<Message>,
    waiting: Waiting,
    step_up: Arc<Mutex<Option<oneshot::Sender<Frame>>>>,
    next: AtomicU64,
    closed: watch::Receiver<bool>,
    pub welcome: Welcome,
    tasks: Vec<JoinHandle<()>>,
}

pub async fn connect(addr: &str, identity: &Identity, server_key: [u8; 32], me: &ClientInfo) -> Result<Session, ConnectError> {
    connect_speaking(addr, identity, server_key, me, PROTOCOL).await
}

/// [`connect`] with another protocol number (tests of version refusal).
#[doc(hidden)]
pub async fn connect_speaking(addr: &str, identity: &Identity, server_key: [u8; 32], me: &ClientInfo, protocol: u32) -> Result<Session, ConnectError> {
    let (mut ws, _) = open(addr, identity, Some(server_key)).await?;
    match hello(&mut ws, protocol, me).await? {
        Frame::Welcome { server, app, device_id, role, .. } => Ok(Session::start(ws, Welcome { server, app, device_id, role })),
        Frame::Unpaired { .. } => Err(ConnectError::Unpaired),
        _ => Err(ConnectError::Protocol("Unexpected answer from the server.".into())),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Session {
    fn start(ws: Ws, welcome: Welcome) -> Self {
        let (mut sink, mut stream) = ws.split();
        let (out, mut queue) = mpsc::channel::<Message>(64);
        let (closed_tx, closed) = watch::channel(false);
        let waiting: Waiting = Arc::default();
        let step_up: Arc<Mutex<Option<oneshot::Sender<Frame>>>> = Arc::default();
        let writer = tokio::spawn(async move {
            while let Some(m) = queue.recv().await {
                if sink.send(m).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });
        let (w, s) = (waiting.clone(), step_up.clone());
        let reader = tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t,
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                match Frame::decode(text.as_str()) {
                    Ok(f @ Frame::Result { .. }) => {
                        let Frame::Result { id, .. } = &f else { continue };
                        if let Some(tx) = lock(&w).remove(id) {
                            let _ = tx.send(f);
                        }
                    }
                    Ok(f @ Frame::SteppedUp { .. }) => {
                        if let Some(tx) = lock(&s).take() {
                            let _ = tx.send(f);
                        }
                    }
                    _ => {}
                }
            }
            // Closed first, then every waiting call fails (its sender is
            // dropped): a call that registers in between sees `closed`.
            let _ = closed_tx.send(true);
            lock(&w).clear();
            lock(&s).take();
        });
        Self { out, waiting, step_up, next: AtomicU64::new(1), closed, welcome, tasks: vec![writer, reader] }
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

    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value, CallError> {
        let closed = || CallError::new("closed", "The connection to the server is closed.");
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.waiting).insert(id, tx);
        if self.is_closed() {
            lock(&self.waiting).remove(&id);
            return Err(closed());
        }
        let f = Frame::Call { id, cmd: cmd.to_string(), args };
        if self.out.send(Message::text(f.encode())).await.is_err() {
            lock(&self.waiting).remove(&id);
            return Err(closed());
        }
        match tokio::time::timeout(CALL_TIMEOUT, rx).await {
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
}

impl Drop for Session {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}
