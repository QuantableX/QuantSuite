//! Tauri commands for plugin `core`. Invoked from the frontend as
//! `plugin:core|<name>` — see ARCHITECTURE.md §2.

use crate::{
    agent, autostart,
    bus::{self, Event},
    db::{self, Db, Entity, Link},
    paths,
    processes::{ProcessInfo, ProcessRegistry},
    shutdown, window,
};
use tauri::{AppHandle, Runtime, State};

fn map_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ── Settings ────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn get_settings(db_state: State<'_, Db>, scope: String) -> Result<serde_json::Value, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    let map = db::get_scope(&conn, &scope).map_err(map_err)?;
    Ok(serde_json::Value::Object(map))
}

#[tauri::command(async)]
pub fn get_setting(
    db_state: State<'_, Db>,
    scope: String,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::get_setting(&conn, &scope, &key).map_err(map_err)
}

#[tauri::command(async)]
pub fn set_setting<R: Runtime>(
    app: AppHandle<R>,
    db_state: State<'_, Db>,
    scope: String,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    if scope == "core" {
        qs_mcp_bridge::validate_app_setting(&key, &value)?;
    }
    {
        let conn = db_state.0.lock().map_err(map_err)?;
        db::set_setting(&conn, &scope, &key, &value).map_err(map_err)?;
    }
    let _ = bus::emit(
        &app,
        "core.setting.changed",
        serde_json::json!({ "scope": scope, "key": key, "value": value }),
    );
    Ok(())
}

// ── Bus ─────────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn emit_event<R: Runtime>(
    app: AppHandle<R>,
    topic: String,
    source: Option<String>,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    let event = Event::new(
        topic,
        source.unwrap_or_else(|| "shell".into()),
        payload.unwrap_or(serde_json::Value::Null),
    );
    bus::publish(&app, event)
}

#[tauri::command(async)]
pub fn recent_events(db_state: State<'_, Db>, limit: Option<i64>) -> Result<Vec<serde_json::Value>, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::recent_events(&conn, limit.unwrap_or(100)).map_err(map_err)
}

// ── Entities ────────────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn upsert_entity<R: Runtime>(
    app: AppHandle<R>,
    db_state: State<'_, Db>,
    entity: Entity,
) -> Result<(), String> {
    {
        let conn = db_state.0.lock().map_err(map_err)?;
        db::upsert_entity(&conn, &entity).map_err(map_err)?;
    }
    let _ = bus::emit(
        &app,
        "core.entity.upserted",
        serde_json::json!({ "id": entity.id, "module": entity.module, "kind": entity.kind }),
    );
    Ok(())
}

#[tauri::command(async)]
pub fn delete_entity<R: Runtime>(
    app: AppHandle<R>,
    db_state: State<'_, Db>,
    id: String,
) -> Result<(), String> {
    {
        let conn = db_state.0.lock().map_err(map_err)?;
        db::delete_entity(&conn, &id).map_err(map_err)?;
    }
    // Upserts announce themselves; deletions must too, or every other window
    // keeps showing the entity until its next full reload (E3: the drawer,
    // notes and the HUD overlay all render the same entities live).
    let _ = bus::emit(&app, "core.entity.deleted", serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command(async)]
pub fn list_entities(
    db_state: State<'_, Db>,
    module: Option<String>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Entity>, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::list_entities(&conn, module.as_deref(), kind.as_deref(), limit.unwrap_or(200)).map_err(map_err)
}

/// How many entities match — same filters as [`list_entities`], without
/// loading the rows. For counters (the dashboard's per-module badges).
#[tauri::command(async)]
pub fn count_entities(
    db_state: State<'_, Db>,
    module: Option<String>,
    kind: Option<String>,
) -> Result<i64, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::count_entities(&conn, module.as_deref(), kind.as_deref()).map_err(map_err)
}

#[tauri::command(async)]
pub fn search_entities(
    db_state: State<'_, Db>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Entity>, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::search_entities(&conn, &query, limit.unwrap_or(30)).map_err(map_err)
}

#[tauri::command(async)]
pub fn link_entities(db_state: State<'_, Db>, link: Link) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::link(&conn, &link).map_err(map_err)
}

#[tauri::command(async)]
pub fn unlink_entities(db_state: State<'_, Db>, link: Link) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::unlink(&conn, &link).map_err(map_err)
}

#[tauri::command(async)]
pub fn linked_entities(
    db_state: State<'_, Db>,
    id: String,
    incoming: Option<bool>,
) -> Result<Vec<Entity>, String> {
    let conn = db_state.0.lock().map_err(map_err)?;
    db::linked(&conn, &id, incoming.unwrap_or(false)).map_err(map_err)
}

// ── Processes ───────────────────────────────────────────────────────────

/// Every process a module has announced, running or not (E4: the process
/// center's list).
///
/// The only command the register needs: start, stop and log tail belong to the
/// owning module, and each entry names the commands the shell may invoke for
/// it. Sync, not async — this reads a small in-memory map and never spawns,
/// waits or touches a pipe.
#[tauri::command(async)]
pub fn process_list(reg: State<'_, ProcessRegistry>) -> Vec<ProcessInfo> {
    reg.list()
}

// ── Window / lifecycle ──────────────────────────────────────────────────

/// Clip the window to a circle of `diameter` logical pixels, or pass `null` to
/// restore the rectangle. See [`window::set_circular`] — CSS rounding alone
/// leaves the corners clickable.
#[tauri::command]
pub fn set_circular_window<R: Runtime>(app: AppHandle<R>, diameter: Option<u32>) -> Result<(), String> {
    window::set_circular(&app, diameter)
}

#[tauri::command(async)]
pub fn window_new<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    window::open(&app)
}

#[tauri::command(async)]
pub fn window_show<R: Runtime>(app: AppHandle<R>) {
    window::show(&app);
}

#[tauri::command(async)]
pub fn window_hide<R: Runtime>(app: AppHandle<R>) {
    window::hide(&app);
}

#[tauri::command(async)]
pub fn window_toggle<R: Runtime>(app: AppHandle<R>) {
    window::toggle(&app);
}

/// The only real exit. Runs the ordered teardown first.
#[tauri::command(async)]
pub fn quit<R: Runtime>(app: AppHandle<R>) {
    shutdown::quit(&app);
}

// ── Autostart ───────────────────────────────────────────────────────────

/// Read straight from the OS entry, never from a mirrored setting — the user
/// can remove it outside the app (see `autostart`).
#[tauri::command(async)]
pub fn autostart_enabled<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    autostart::is_enabled(&app)
}

#[tauri::command(async)]
pub fn set_autostart<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    autostart::set_enabled(&app, enabled)
}

// ── Agent tool catalogue (MCP bridge) ───────────────────────────────────

/// Every module capability an agent could call, as `quantsuite.<module>.<name>`.
///
/// This is the catalogue only. Serving it over the MCP protocol still needs a
/// hook in QuantMCP's server — see `qs-mcp-bridge` (ARCHITECTURE.md §7).
#[tauri::command(async)]
pub fn agent_tools<R: Runtime>(app: AppHandle<R>) -> Result<Vec<qs_mcp_bridge::Capability>, String> {
    let availability = crate::apps::read(&app)?;
    Ok(qs_mcp_bridge::capabilities().iter().filter(|cap| availability.tool_enabled(&cap.tool)).cloned().collect())
}

/// What the gate would decide for a tool under the given approval mode.
/// Surfaced so the policy is inspectable rather than folklore.
#[tauri::command(async)]
pub fn agent_tool_decision<R: Runtime>(
    app: AppHandle<R>,
    tool: String,
    mode: Option<qs_mcp_bridge::ApprovalMode>,
) -> qs_mcp_bridge::Decision {
    match crate::apps::read(&app) {
        Ok(availability) if availability.tool_enabled(&tool) => qs_mcp_bridge::decide_tool(&tool, mode.unwrap_or_default()),
        Ok(_) => qs_mcp_bridge::Decision::Deny { reason: "This app is deactivated in Settings > Apps".into() },
        Err(reason) => qs_mcp_bridge::Decision::Deny { reason },
    }
}

/// Unclaimed calls waiting on the shell. Startup recovery must not replay
/// calls another window has already claimed for dispatch or rejection.
#[tauri::command(async)]
pub fn agent_pending_calls(broker: State<'_, agent::AgentBroker>) -> Vec<agent::PendingCall> {
    broker.pending()
}

/// Only one window may dispatch or reject a pending call, even if several
/// windows receive the event or recover the same startup snapshot.
#[tauri::command(async)]
pub fn agent_call_claim<R: Runtime>(
    app: AppHandle<R>,
    broker: State<'_, agent::AgentBroker>,
    call_id: String,
) -> bool {
    let claimed = broker.claim(&call_id);
    if claimed {
        let _ = bus::emit(&app, "agent.call.claimed", serde_json::json!({ "callId": call_id }));
    }
    claimed
}

/// The shell reporting a call's outcome — the dispatched command's result,
/// its error, or the user's rejection. Resolves the agent's waiting request.
#[tauri::command(async)]
pub fn agent_call_complete(
    broker: State<'_, agent::AgentBroker>,
    call_id: String,
    result: Option<String>,
    error: Option<String>,
) {
    match error {
        Some(e) => broker.complete(&call_id, Err(e)),
        None => broker.complete(&call_id, Ok(result.unwrap_or_default())),
    }
}

// ── Introspection ───────────────────────────────────────────────────────

#[tauri::command(async)]
pub fn app_version<R: Runtime>(app: AppHandle<R>) -> String {
    app.package_info().version.to_string()
}

/// Where the suite keeps its data. Surfaced in settings so it is discoverable
/// rather than folklore.
#[tauri::command(async)]
pub fn suite_paths() -> serde_json::Value {
    serde_json::json!({
        "root": paths::root(),
        "coreDb": paths::core_db(),
        "logs": paths::logs_dir(),
        "secrets": paths::secrets_dir(),
    })
}
