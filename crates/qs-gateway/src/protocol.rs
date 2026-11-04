//! The gateway's frames (PLAN-SERVER-CLIENT §6.4): JSON text messages over a
//! WebSocket inside TLS 1.3.
//!
//! ```text
//! client                          server
//! hello {protocol, app, manifest} →
//!                                 ← welcome {role}            (paired)
//!                                 ← unpaired                  (unknown key)
//!                                 ← refused {reason}          (revoked, versions)
//! pair {code, name}               →                           (unpaired only)
//!                                 ← pair_pending {words}      (the right code)
//!                                 ← paired {role} | pair_failed
//! call {id, cmd, args}            →
//!                                 ← result {id, ok | error}
//! step_up {pin}                   →
//!                                 ← stepped_up {ok, valid_secs}
//! ```

use crate::authz::DeviceRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Raised whenever a frame changes meaning; both sides must agree.
pub const PROTOCOL: u32 = 1;

/// The largest message either side accepts.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

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
    /// `denied`, `step_up_required`, `failed`, `unavailable`, `timeout`, `closed`.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Frame {
    Hello { protocol: u32, app: String, manifest: String, name: String },
    Welcome { protocol: u32, app: String, server: String, device_id: String, role: DeviceRole },
    Unpaired { server: String },
    Refused { reason: Refusal, message: String },
    Pair { code: String, name: String },
    PairPending { words: Vec<String> },
    Paired { device_id: String, role: DeviceRole },
    PairFailed { message: String },
    Call {
        id: u64,
        cmd: String,
        #[serde(default)]
        args: Value,
    },
    Result {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<CallError>,
    },
    StepUp { pin: String },
    SteppedUp {
        ok: bool,
        valid_secs: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Ping,
    Pong,
}

impl Frame {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"t":"ping"}"#.into())
    }

    pub fn decode(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("bad frame: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let f = Frame::Call { id: 3, cmd: "plugin:notes|list_notes".into(), args: serde_json::json!({"a": 1}) };
        assert_eq!(f.encode(), r#"{"t":"call","id":3,"cmd":"plugin:notes|list_notes","args":{"a":1}}"#);
        assert_eq!(Frame::decode(&f.encode()).unwrap(), f);
        let r = Frame::decode(r#"{"t":"result","id":3,"ok":null}"#).unwrap();
        assert_eq!(r, Frame::Result { id: 3, ok: None, error: None }, "a unit result");
        assert!(Frame::decode(r#"{"t":"launch_missiles"}"#).is_err());
    }
}
