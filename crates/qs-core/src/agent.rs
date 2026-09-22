//! The agent-call broker (PLAN-V2 E4) — the missing transport between the
//! MCP bridge's *policy* and the modules' *commands*.
//!
//! An external agent calls `quantsuite.<module>.<name>` on QuantMCP's MCP
//! server. That server calls [`request`] here. The broker:
//!
//!   1. asks the gate (`qs_mcp_bridge::decide_tool`) under the user's
//!      approval mode (`core / agent.approvalMode` in core.db)
//!   2. **deny** → the error goes straight back to the agent; the frontend
//!      never even sees the call, so a deny cannot be upgraded by UI code
//!   3. **allow / prompt** → emits `agent.call.requested` on the bus; the
//!      shell dispatches allowed calls immediately and puts prompted ones in
//!      the approval queue. Either way the webview performs the actual
//!      `invoke` — modules' commands stay reachable exactly one way, through
//!      Tauri's normal permission-checked path — and reports back via the
//!      `agent_call_complete` command, which resolves the waiting oneshot.
//!
//! `external` capabilities always arrive as `prompt` (the gate guarantees
//! it, with tests); the queue is where the user says yes or no.

use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

/// How long a call may sit in the approval queue before the agent gets told
/// nobody answered. Allowed calls get a shorter leash — they only cover the
/// invoke round-trip.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);
const ALLOW_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCall {
    pub call_id: String,
    pub tool: String,
    /// `plugin:<module>|<command>` — what the shell will invoke.
    pub command: String,
    pub args: serde_json::Value,
    /// `allow` or `prompt` — a denied call never becomes pending.
    pub decision: &'static str,
    pub reason: Option<String>,
    pub requested_at: i64,
    #[serde(skip)]
    claimed: bool,
}

#[derive(Default)]
pub struct AgentBroker {
    pending: Mutex<HashMap<String, PendingCall>>,
    waiters: Mutex<HashMap<String, oneshot::Sender<Result<String, String>>>>,
}

impl AgentBroker {
    pub fn pending(&self) -> Vec<PendingCall> {
        let map = self.pending.lock().expect("agent broker poisoned");
        let mut list: Vec<_> = map.values().filter(|call| !call.claimed).cloned().collect();
        list.sort_by_key(|c| c.requested_at);
        list
    }

    /// Claim under the same lock as completion and expiry. A stale approval
    /// or competing window must never invoke a command a second time.
    pub fn claim(&self, call_id: &str) -> bool {
        let mut pending = self.pending.lock().expect("agent broker poisoned");
        let Some(call) = pending.get_mut(call_id) else {
            return false;
        };
        if call.claimed {
            return false;
        }
        call.claimed = true;
        true
    }

    /// Resolve a call from the frontend. Unknown ids are a no-op (the call
    /// may have timed out a moment earlier) — not an error worth surfacing.
    pub fn complete(&self, call_id: &str, result: Result<String, String>) {
        self.pending.lock().expect("agent broker poisoned").remove(call_id);
        if let Some(tx) = self.waiters.lock().expect("agent broker poisoned").remove(call_id) {
            let _ = tx.send(result);
        }
    }

    fn insert(&self, call: PendingCall) -> oneshot::Receiver<Result<String, String>> {
        let (tx, rx) = oneshot::channel();
        self.waiters
            .lock()
            .expect("agent broker poisoned")
            .insert(call.call_id.clone(), tx);
        self.pending
            .lock()
            .expect("agent broker poisoned")
            .insert(call.call_id.clone(), call);
        rx
    }

    fn forget(&self, call_id: &str) {
        self.pending.lock().expect("agent broker poisoned").remove(call_id);
        self.waiters.lock().expect("agent broker poisoned").remove(call_id);
    }
}

/// Drops the call out of the broker unless [`run`] disarmed it. The MCP
/// streamable-HTTP transport awaits the request inline in its axum handler, so
/// a client that disconnects mid-call takes the whole future with it — without
/// this the `PendingCall` would sit in the approval queue forever, resolvable
/// only by hand.
///
/// Forgetting it in Rust is only half of that. The shell prunes its approval
/// queue on `agent.call.completed` and nothing else, so a card left standing
/// would still invoke the module command for real — on behalf of an agent that
/// is already gone. The guard emits the event too, exactly as the timeout arm
/// does.
struct CallGuard<'a, R: tauri::Runtime> {
    app: &'a tauri::AppHandle<R>,
    broker: &'a AgentBroker,
    call_id: String,
    tool: String,
    armed: bool,
}

impl<R: tauri::Runtime> Drop for CallGuard<'_, R> {
    fn drop(&mut self) {
        if self.armed {
            self.broker.forget(&self.call_id);
            let _ = crate::bus::emit(
                self.app,
                "agent.call.completed",
                serde_json::json!({ "callId": self.call_id, "tool": self.tool, "ok": false }),
            );
        }
    }
}

type Requester = Box<
    dyn Fn(String, serde_json::Value) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>
        + Send
        + Sync,
>;

/// Installed once by qs-core's setup with the concrete `AppHandle` captured;
/// [`request`] is what QuantMCP's server calls, without needing Tauri types.
static REQUESTER: OnceLock<Requester> = OnceLock::new();
static CALL_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn install<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let _ = REQUESTER.set(Box::new(move |tool, args| {
        let app = app.clone();
        Box::pin(async move { run(app, tool, args).await })
    }));
}

/// Entry point for the MCP transport. Errors are agent-facing strings.
pub async fn request(tool: &str, args: serde_json::Value) -> Result<String, String> {
    let Some(requester) = REQUESTER.get() else {
        return Err("agent broker not initialised".into());
    };
    requester(tool.to_string(), args).await
}

async fn run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    tool: String,
    args: serde_json::Value,
) -> Result<String, String> {
    use tauri::Manager;

    // The user's approval mode, straight from core.db — not a cached copy.
    let mode = read_mode(&app);
    let decision = qs_mcp_bridge::decide_tool(&tool, mode);

    let (decision_str, reason) = match decision {
        qs_mcp_bridge::Decision::Deny { reason } => {
            return Err(format!("denied by the QuantSuite gate: {reason}"));
        }
        qs_mcp_bridge::Decision::Allow => ("allow", None),
        qs_mcp_bridge::Decision::Prompt { reason } => ("prompt", Some(reason)),
    };

    let cap = qs_mcp_bridge::find(&tool).ok_or_else(|| format!("unknown tool '{tool}'"))?;

    crate::apps::require_module(&app, &cap.module)?;
    let broker = app.state::<AgentBroker>();
    let call_id = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_millis(),
        CALL_SEQ.fetch_add(1, Ordering::Relaxed)
    );

    let call = PendingCall {
        call_id: call_id.clone(),
        tool: tool.clone(),
        command: cap.command.clone(),
        args,
        decision: decision_str,
        reason,
        requested_at: chrono::Utc::now().timestamp_millis(),
        claimed: false,
    };

    let rx = broker.insert(call.clone());
    let mut guard = CallGuard {
        app: &app,
        broker: &broker,
        call_id: call_id.clone(),
        tool: tool.clone(),
        armed: true,
    };
    let _ = crate::bus::emit(
        &app,
        "agent.call.requested",
        serde_json::to_value(&call).unwrap_or_default(),
    );

    let leash = if decision_str == "prompt" { PROMPT_TIMEOUT } else { ALLOW_TIMEOUT };
    let outcome = match timeout(leash, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("the suite dropped the call".into()),
        Err(_) => {
            broker.forget(&call_id);
            Err(if decision_str == "prompt" {
                format!("'{tool}' needs user approval and nobody answered within {}s", leash.as_secs())
            } else {
                format!("'{tool}' timed out after {}s", leash.as_secs())
            })
        }
    };
    guard.armed = false; // every arm above cleaned up already

    let _ = crate::bus::emit(
        &app,
        "agent.call.completed",
        serde_json::json!({ "callId": call_id, "tool": tool, "ok": outcome.is_ok() }),
    );
    // The documented timeline event (ARCHITECTURE.md §3) — persisted, unlike
    // the two live broker events above, so QuantControl's feed can backfill
    // from `recent_events` after a reload.
    let _ = crate::bus::publish(
        &app,
        crate::bus::Event::new(
            "mcp.tool.invoked",
            "mcp",
            serde_json::json!({ "tool": tool, "caller": "mcp", "approved": outcome.is_ok() }),
        ),
    );
    outcome
}

fn read_mode<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> qs_mcp_bridge::ApprovalMode {
    use tauri::Manager;
    let Some(db) = app.try_state::<crate::db::Db>() else {
        return qs_mcp_bridge::ApprovalMode::Strict;
    };
    let Ok(conn) = db.0.lock() else {
        return qs_mcp_bridge::ApprovalMode::Strict;
    };
    match crate::db::get_setting(&conn, "core", "agent.approvalMode") {
        Ok(Some(v)) if v == serde_json::json!("relaxed") => qs_mcp_bridge::ApprovalMode::Relaxed,
        _ => qs_mcp_bridge::ApprovalMode::Strict, // the safe default, deliberately
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> PendingCall {
        PendingCall {
            call_id: "shared-call".into(),
            tool: "quantsuite.notes.create_note".into(),
            command: "plugin:notes|create_note".into(),
            args: serde_json::json!({}),
            decision: "prompt",
            reason: None,
            requested_at: 1,
            claimed: false,
        }
    }

    #[test]
    fn competing_windows_can_only_claim_a_call_once() {
        let broker = std::sync::Arc::new(AgentBroker::default());
        let _waiting = broker.insert(call());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8).map(|_| {
            let broker = broker.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                broker.claim("shared-call")
            })
        }).collect();
        let claims: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
        assert_eq!(claims.iter().filter(|claimed| **claimed).count(), 1);
    }

    #[test]
    fn claimed_calls_are_not_replayed_by_a_new_window() {
        let broker = AgentBroker::default();
        let mut waiting = broker.insert(call());
        assert_eq!(broker.pending().len(), 1);
        assert!(broker.claim("shared-call"));
        assert!(broker.pending().is_empty());
        assert!(!broker.claim("shared-call"));
        broker.complete("shared-call", Ok("saved".into()));
        assert_eq!(waiting.try_recv().unwrap(), Ok("saved".into()));
        assert!(!broker.claim("shared-call"));
    }

    #[test]
    fn expired_or_cancelled_calls_cannot_be_approved_from_a_stale_window() {
        let broker = AgentBroker::default();
        let _waiting = broker.insert(call());
        broker.forget("shared-call");
        assert!(!broker.claim("shared-call"));
        assert!(!broker.claim("unknown"));
    }
}
