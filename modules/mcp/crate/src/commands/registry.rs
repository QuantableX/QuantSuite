//! The MCP registry (CRUD over `mcps.json`) and the server fleet: per-server
//! start/stop/status plus the whole-fleet commands the suite's process page
//! invokes.

use crate::mcp::{McpEntry, McpTransport};
use crate::process::{McpProcessStatus, ProcessManager};
use crate::{lock, AppState};
use std::collections::HashMap;
use std::sync::Mutex;

// â"€â"€ MCP CRUD â"€â"€

#[tauri::command(async)]
pub(crate) fn list_mcps(state: tauri::State<'_, AppState>) -> Vec<McpEntry> {
    lock(&state.registry).list()
}

#[tauri::command]
pub(crate) async fn toggle_mcp(state: tauri::State<'_, AppState>, id: String, enabled: bool) -> Result<(), String> {
    lock(&state.registry).set_enabled(&id, enabled)
}

#[tauri::command(async)]
pub(crate) fn get_mcp_config(state: tauri::State<'_, AppState>, id: String) -> Result<McpEntry, String> {
    lock(&state.registry)
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("MCP '{}' not found", id))
}

#[tauri::command]
pub(crate) async fn add_mcp(
    state: tauri::State<'_, AppState>,
    name: String,
    description: String,
    transport: McpTransport,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    working_dir: Option<String>,
) -> Result<McpEntry, String> {
    Ok(lock(&state.registry).add(McpEntry {
        id: String::new(),
        name,
        description,
        enabled: true,
        transport,
        command,
        args: args.unwrap_or_default(),
        env: env.unwrap_or_default(),
        working_dir,
    }))
}

#[tauri::command]
pub(crate) async fn update_mcp(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    description: String,
    transport: McpTransport,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    working_dir: Option<String>,
) -> Result<McpEntry, String> {
    let entry = McpEntry {
        id: id.clone(),
        name,
        description,
        enabled: true,
        transport,
        command,
        args: args.unwrap_or_default(),
        env: env.unwrap_or_default(),
        working_dir,
    };
    lock(&state.registry).update(&id, entry)
}

#[tauri::command]
pub(crate) async fn delete_mcp(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    lock(&state.registry).remove(&id)
}

// â"€â"€ Process Management â"€â"€
//
// # Why the register gets one entry, not one per server
//
// This module runs *n* servers, one per configured MCP, and the obvious reading
// of `qs_core::processes` would be one announced entry each. Two things rule
// that out:
//
//   * The register has no `remove`. Entries are announced once and live as long
//     as the app does, while the MCP list is user-editable at runtime — every
//     `delete_mcp` would leave a ghost row on the process page that nothing can
//     ever clear.
//   * The page invokes start and stop **without arguments**, but every per-server
//     command needs the `{ id }`. Wrappers are compile-time commands, so there
//     is no way to mint one per server; per-server rows could only be announced
//     with `start_command: None` and `stop_command: None` — a list with no
//     buttons, duplicating what the module's own MCP screen already shows better.
//
// So the fleet is announced as one entry whose start brings up every enabled
// server and whose stop takes them all down — the two things the process page
// can do that the module screen cannot do from outside itself. Per-server detail
// stays where it belongs, in the module.
//
// There is no `logs_with`: the children are spawned with stdout and stderr on
// `null` (see `process.rs`), so there is no captured output to tail, and a button
// that always shows nothing is worse than no button.

/// Stable, module-prefixed key, as in `systems.engine`.
pub(crate) const SERVERS_PROCESS_ID: &str = "mcp.servers";

/// What was last reported to the register, or `None` before the first report.
///
/// Reporting is edge-triggered on purpose: `mark_running` stamps a fresh `since`
/// for the page's uptime readout, so repeating it on every status poll would pin
/// the fleet at "up 0s".
#[derive(PartialEq)]
enum ReportedState {
    Running,
    Stopped,
    Failed(String),
}

static REPORTED: Mutex<Option<ReportedState>> = Mutex::new(None);

/// Collapse the per-server statuses into the one state the page shows, and
/// report it if it changed.
///
/// Precedence is running → failed → stopped: while any server is up the fleet is
/// up, and only once nothing is running does a server's exit message become the
/// thing worth showing.
fn report_servers_state(app: &tauri::AppHandle, statuses: &HashMap<String, McpProcessStatus>) {
    let next = if statuses
        .values()
        .any(|s| matches!(s, McpProcessStatus::Running))
    {
        ReportedState::Running
    } else if let Some(message) = statuses.values().find_map(|s| match s {
        McpProcessStatus::Error { message } => Some(message.clone()),
        _ => None,
    }) {
        ReportedState::Failed(message)
    } else {
        ReportedState::Stopped
    };

    let Ok(mut last) = REPORTED.lock() else { return };
    if last.as_ref() == Some(&next) {
        return;
    }
    match &next {
        // No pid: a fleet has as many as it has servers, and the register asks
        // for one. `None` is the honest answer rather than an arbitrary child's.
        ReportedState::Running => qs_core::processes::mark_running(app, SERVERS_PROCESS_ID, None),
        ReportedState::Stopped => qs_core::processes::mark_stopped(app, SERVERS_PROCESS_ID),
        ReportedState::Failed(message) => {
            qs_core::processes::mark_failed(app, SERVERS_PROCESS_ID, message.clone())
        }
    }
    *last = Some(next);
}

/// The mandatory place, reached from every command below that touches the
/// manager: `ProcessManager::status_all` runs `try_wait` over the children, so
/// this is the moment a server that died on its own is noticed. The register
/// believes what it is told and cannot look for itself, so it is told here.
async fn sync_servers_state(app: &tauri::AppHandle, manager: &ProcessManager) {
    let statuses = manager.status_all().await;
    report_servers_state(app, &statuses);
}

#[tauri::command]
pub(crate) async fn start_mcp(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let entry = {
        let reg = lock(&state.registry);
        reg.get(&id).cloned().ok_or("MCP not found")?
    };
    let command = entry
        .command
        .as_deref()
        .ok_or("No command configured for this MCP")?;
    let result = state
        .process_manager
        .start(
            &id,
            command,
            &entry.args,
            &entry.env,
            entry.working_dir.as_deref(),
        )
        .await;
    sync_servers_state(&app, &state.process_manager).await;
    result
}

#[tauri::command]
pub(crate) async fn stop_mcp(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let result = state.process_manager.stop(&id).await;
    sync_servers_state(&app, &state.process_manager).await;
    result
}

#[tauri::command]
pub(crate) async fn get_mcp_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<McpProcessStatus, String> {
    let status = state.process_manager.status(&id).await;
    sync_servers_state(&app, &state.process_manager).await;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn get_all_mcp_statuses(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, McpProcessStatus>, String> {
    let statuses = state.process_manager.status_all().await;
    report_servers_state(&app, &statuses);
    Ok(statuses)
}

/// Bring up every enabled MCP server that has a command — the no-argument start
/// the suite's process page invokes.
///
/// A wrapper rather than a change to [`start_mcp`], which the module's own screen
/// calls per server with `{ id }`. Servers already running are skipped rather
/// than reported as failures; a server the user disabled stays down, because
/// "enabled" is the module's own word for "should be running".
#[tauri::command]
pub(crate) async fn process_start_servers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let entries: Vec<McpEntry> = {
        let reg = lock(&state.registry);
        reg.list()
            .into_iter()
            .filter(|entry| entry.enabled && entry.command.is_some())
            .collect()
    };
    if entries.is_empty() {
        return Err("No enabled MCP server has a command to start.".into());
    }

    let mut failures: Vec<String> = Vec::new();
    for entry in &entries {
        if matches!(
            state.process_manager.status(&entry.id).await,
            McpProcessStatus::Running
        ) {
            continue;
        }
        let Some(command) = entry.command.as_deref() else {
            continue;
        };
        if let Err(e) = state
            .process_manager
            .start(
                &entry.id,
                command,
                &entry.args,
                &entry.env,
                entry.working_dir.as_deref(),
            )
            .await
        {
            failures.push(format!("{}: {e}", entry.name));
        }
    }

    sync_servers_state(&app, &state.process_manager).await;
    if failures.is_empty() {
        Ok(())
    } else {
        // Partial success is still worth surfacing: the register shows the fleet
        // as running, and this names the servers that are missing from it.
        Err(failures.join("; "))
    }
}

/// Take every MCP server down — the no-argument stop the process page invokes.
#[tauri::command]
pub(crate) async fn process_stop_servers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.process_manager.stop_all().await;
    sync_servers_state(&app, &state.process_manager).await;
    Ok(())
}
