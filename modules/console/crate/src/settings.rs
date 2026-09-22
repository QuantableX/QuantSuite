//! Reading the module's own settings out of `core.db`.
//!
//! One setting is left after the plain-terminal rollback: the agent-execution
//! lock. Values are validated, never trusted.

use tauri::{AppHandle, Manager, Runtime};

/// The settings scope this module owns.
const SCOPE: &str = "console";

/// Whether an agent may run a command through the console.
///
/// The first of the two locks (the second is QuantMCP's approval gate), and it
/// ships **off**: anything other than a literal `true` is a no, so a
/// half-written value cannot open it.
pub fn agent_exec_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(db) = app.try_state::<qs_core::Db>() else {
        return false;
    };
    let Ok(conn) = db.0.lock() else {
        return false;
    };
    qs_core::db::get_setting(&conn, SCOPE, "agent.exec.enabled")
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
