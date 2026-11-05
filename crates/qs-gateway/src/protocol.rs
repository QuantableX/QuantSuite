//! The gateway's frames (PLAN-SERVER-CLIENT §6.4): JSON text messages over a
//! WebSocket inside TLS 1.3, plus binary messages for file chunks.
//!
//! ```text
//! client                                   server
//! hello {protocol, app, manifest, resume?} →
//!                                          ← welcome {role, epoch, seq, reload}   (paired)
//!                                          ← event {seq, …} …                     (replay, then live)
//!                                          ← unpaired | refused {reason}
//! pair {code, name}                        →                                      (unpaired only)
//!                                          ← pair_pending {words} → paired | pair_failed
//! call {id, cmd, args, key?}               →
//!                                          ← result {id, ok | error}
//! outcome {id, key}                        →  "what happened to that keyed call?"
//!                                          ← result {id, …} | error "unknown"
//! cancel {id}                              →
//! step_up {pin}                            →
//!                                          ← stepped_up {ok, valid_secs}
//! ping                                     →
//!                                          ← pong
//! download {id, path, offset}              →
//!                                          ← transfer {id, size, sha256, offset}, binary chunks, transfer_done {id}
//! upload {id, name, size, sha256}          →
//!                                          ← result {id, ok: {offset}}
//! binary chunks, upload_done {id}          →
//!                                          ← result {id, ok: {path, size, sha256}}
//! ```
//!
//! A binary message is one chunk: 8 bytes transfer id, 8 bytes offset (both
//! big-endian), then the data.

use crate::authz::DeviceRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Raised whenever a frame changes meaning; both sides must agree.
/// 1: card Server 3/7. 2: events, resume, idempotency keys, files (card 4/7).
pub const PROTOCOL: u32 = 2;

/// The largest message either side accepts.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// The size of one file chunk.
pub const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The client speaks a newer protocol.
    UpdateServer,
    /// The client speaks an older protocol.
    UpdateClient,
    /// Same protocol, different remote manifest: different QuantSuite versions.
    VersionMismatch,
    Revoked,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallError {
    /// `denied`, `step_up_required`, `failed`, `unavailable`, `timeout`,
    /// `closed`, `cancelled`, `rate_limited`, `unknown` (no record of a key).
    pub code: String,
    pub message: String,
}

impl CallError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Where a reconnecting client left off: the server's run (`epoch`) and the
/// last event it received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumePoint {
    pub epoch: u64,
    pub last_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A Tauri event of a server-placed module (`bot:log`), re-emitted under
    /// the same name on the client.
    Tauri,
    /// A suite-bus event (`qs://event`) of a server-placed module's topic;
    /// the payload is the whole bus event.
    Bus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Frame {
    Hello {
        protocol: u32,
        app: String,
        manifest: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume: Option<ResumePoint>,
        /// This connection is to pair: a revoked key may pair again (with a
        /// new code from the server's screen), but not connect.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pair: bool,
    },
    Welcome {
        protocol: u32,
        app: String,
        server: String,
        device_id: String,
        role: DeviceRole,
        /// The server's run; sequence numbers restart with a new epoch.
        epoch: u64,
        /// The newest event at this moment.
        seq: u64,
        /// The resume point could not be honoured (another run, or the events
        /// are no longer kept): reload the pages' state.
        #[serde(default)]
        reload: bool,
    },
    Unpaired {
        server: String,
    },
    Refused {
        reason: Refusal,
        message: String,
    },
    Pair {
        code: String,
        name: String,
    },
    PairPending {
        words: Vec<String>,
    },
    Paired {
        device_id: String,
        role: DeviceRole,
    },
    PairFailed {
        message: String,
    },
    Call {
        id: u64,
        cmd: String,
        #[serde(default)]
        args: Value,
        /// Idempotency key of a changing call: the same key runs once.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
    Result {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<CallError>,
    },
    /// What happened to the call with this key? Answered as a `result` for `id`.
    Outcome {
        id: u64,
        key: String,
    },
    Cancel {
        id: u64,
    },
    Event {
        seq: u64,
        kind: EventKind,
        topic: String,
        #[serde(default)]
        payload: Value,
    },
    /// Events were lost for this client (it fell too far behind): reload.
    Reload {
        epoch: u64,
        seq: u64,
    },
    StepUp {
        pin: String,
    },
    SteppedUp {
        ok: bool,
        valid_secs: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Ping,
    Pong,
    Download {
        id: u64,
        path: String,
        #[serde(default)]
        offset: u64,
    },
    Transfer {
        id: u64,
        size: u64,
        sha256: String,
        offset: u64,
    },
    TransferDone {
        id: u64,
    },
    Upload {
        id: u64,
        name: String,
        size: u64,
        sha256: String,
    },
    UploadDone {
        id: u64,
    },
}

impl Frame {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"t":"ping"}"#.into())
    }

    pub fn decode(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("bad frame: {e}"))
    }
}

/// A binary chunk message.
pub fn chunk(id: u64, offset: u64, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + data.len());
    v.extend_from_slice(&id.to_be_bytes());
    v.extend_from_slice(&offset.to_be_bytes());
    v.extend_from_slice(data);
    v
}

/// `(id, offset, data)` of a binary chunk message.
pub fn parse_chunk(b: &[u8]) -> Option<(u64, u64, &[u8])> {
    if b.len() < 16 {
        return None;
    }
    let id = u64::from_be_bytes(b[..8].try_into().ok()?);
    let offset = u64::from_be_bytes(b[8..16].try_into().ok()?);
    Some((id, offset, &b[16..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let f = Frame::Call { id: 3, cmd: "plugin:notes|list_notes".into(), args: serde_json::json!({"a": 1}), key: None };
        assert_eq!(f.encode(), r#"{"t":"call","id":3,"cmd":"plugin:notes|list_notes","args":{"a":1}}"#);
        assert_eq!(Frame::decode(&f.encode()).unwrap(), f);
        let r = Frame::decode(r#"{"t":"result","id":3,"ok":null}"#).unwrap();
        assert_eq!(r, Frame::Result { id: 3, ok: None, error: None }, "a unit result");
        assert!(Frame::decode(r#"{"t":"launch_missiles"}"#).is_err());
        let e = Frame::Event { seq: 9, kind: EventKind::Bus, topic: "notes.note.created".into(), payload: Value::Null };
        assert_eq!(Frame::decode(&e.encode()).unwrap(), e);
    }

    #[test]
    fn chunks_carry_id_and_offset() {
        let c = chunk(7, 1 << 33, b"abc");
        assert_eq!(parse_chunk(&c), Some((7, 1 << 33, &b"abc"[..])));
        assert_eq!(parse_chunk(&c[..15]), None);
    }
}
