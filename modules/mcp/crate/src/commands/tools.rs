//! Tools as the module's own screens see them: CRUD over native and user
//! script tools, the script editor's file I/O, the QuantMCP server toggle,
//! the request log and the built-in tool catalogues for the tool pages.

use crate::config_gen;
use crate::logs;
use crate::mcp_server;
use crate::scripts::{ScriptEntry, ScriptLanguage};
use crate::tools::ToolDef;
use crate::{lock, AppState};

// â"€â"€ Tool CRUD â"€â"€

#[tauri::command(async)]
pub(crate) fn list_tools(state: tauri::State<'_, AppState>) -> Vec<ToolDef> {
    let mut tools = lock(&state.native_tool_registry).list();
    tools.extend(lock(&state.tool_registry).list());
    tools
}

#[tauri::command]
pub(crate) async fn add_tool(
    state: tauri::State<'_, AppState>,
    name: String,
    description: String,
    input_schema: serde_json::Value,
    script_path: String,
    interpreter: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<ToolDef, String> {
    if lock(&state.native_tool_registry).has_name(&name) {
        return Err(format!("Name '{}' is reserved by a built-in tool", name));
    }
    if config_gen::codebase_index_tools()
        .iter()
        .any(|t| t.name == name.as_str())
    {
        return Err(format!(
            "Name '{}' is reserved by a built-in codebase tool",
            name
        ));
    }
    if config_gen::agentos_tools()
        .iter()
        .chain(config_gen::kanban_tools().iter())
        .chain(config_gen::worktree_tools().iter())
        .any(|t| t.name == name.as_str())
    {
        return Err(format!(
            "Name '{}' is reserved by a built-in tool",
            name
        ));
    }
    Ok(lock(&state.tool_registry).add(ToolDef {
        id: String::new(),
        name,
        description,
        input_schema,
        script_path,
        interpreter,
        timeout_secs,
        enabled: true,
        native: false,
    }))
}

#[tauri::command]
pub(crate) async fn update_tool(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    description: String,
    input_schema: serde_json::Value,
    script_path: String,
    interpreter: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<ToolDef, String> {
    if lock(&state.native_tool_registry).get(&id).is_some() {
        return Err("Cannot edit native tools".to_string());
    }
    if config_gen::codebase_index_tools()
        .iter()
        .any(|t| t.name == name.as_str())
    {
        return Err(format!(
            "Name '{}' is reserved by a built-in codebase tool",
            name
        ));
    }
    if config_gen::agentos_tools()
        .iter()
        .chain(config_gen::kanban_tools().iter())
        .chain(config_gen::worktree_tools().iter())
        .any(|t| t.name == name.as_str())
    {
        return Err(format!(
            "Name '{}' is reserved by a built-in tool",
            name
        ));
    }
    lock(&state.tool_registry).update(
        &id,
        ToolDef {
            id: id.clone(),
            name,
            description,
            input_schema,
            script_path,
            interpreter,
            timeout_secs,
            enabled: true,
            native: false,
        },
    )
}

#[tauri::command]
pub(crate) async fn toggle_tool(state: tauri::State<'_, AppState>, id: String, enabled: bool) -> Result<(), String> {
    let native = lock(&state.native_tool_registry).get(&id).is_some();
    if native {
        lock(&state.native_tool_registry).set_enabled(&id, enabled)?;
    } else {
        lock(&state.tool_registry).set_enabled(&id, enabled)?;
    }
    let clients = state.active_clients.clone();
    tauri::async_runtime::spawn(async move { mcp_server::notify_tools_changed(&clients).await; });
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_tool(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    if lock(&state.native_tool_registry).get(&id).is_some() {
        return Err("Cannot delete native tools".to_string());
    }
    lock(&state.tool_registry).remove(&id)
}

#[tauri::command]
pub(crate) async fn test_tool(
    state: tauri::State<'_, AppState>,
    id: String,
    input: serde_json::Value,
) -> Result<String, String> {
    let tool = {
        let native_reg = lock(&state.native_tool_registry);
        if let Some(t) = native_reg.get(&id) {
            t.clone()
        } else {
            lock(&state.tool_registry)
                .get(&id)
                .cloned()
                .ok_or("Tool not found")?
        }
    };
    mcp_server::execute_tool(&tool, &input).await
}

// â"€â"€ File I/O â"€â"€

#[tauri::command]
pub(crate) async fn read_file(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub(crate) async fn write_file(path: String, content: String) -> Result<(), String> {
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))
}

// ── QuantMCP Server Toggle ──

#[tauri::command(async)]
pub(crate) fn get_quantmcp_enabled(state: tauri::State<'_, AppState>) -> bool {
    *lock(&state.quantmcp_enabled)
}

#[tauri::command(async)]
pub(crate) fn set_quantmcp_enabled(state: tauri::State<'_, AppState>, enabled: bool) {
    *lock(&state.quantmcp_enabled) = enabled;
}

// â"€â"€ Logs â"€â"€

#[tauri::command(async)]
pub(crate) fn list_logs(state: tauri::State<'_, AppState>, since_id: Option<u64>) -> Vec<logs::LogEntry> {
    let store = lock(&state.log_store);
    match since_id {
        Some(id) => store.list_since(id),
        None => store.list(),
    }
}

#[tauri::command(async)]
pub(crate) fn clear_logs(state: tauri::State<'_, AppState>) {
    lock(&state.log_store).clear()
}

// â"€â"€ Scripts â"€â"€

#[tauri::command(async)]
pub(crate) fn list_scripts(state: tauri::State<'_, AppState>) -> Vec<ScriptEntry> {
    lock(&state.script_registry).list()
}

#[tauri::command]
pub(crate) async fn add_script(
    state: tauri::State<'_, AppState>,
    name: String,
    language: ScriptLanguage,
    content: String,
) -> Result<ScriptEntry, String> {
    Ok(lock(&state.script_registry).add(name, language, content))
}

#[tauri::command]
pub(crate) async fn update_script(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
    language: ScriptLanguage,
    content: String,
) -> Result<ScriptEntry, String> {
    lock(&state.script_registry)
        .update(&id, name, language, content)
}

#[tauri::command]
pub(crate) async fn delete_script(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    lock(&state.script_registry).remove(&id)
}

#[tauri::command(async)]
pub(crate) fn get_script_path(state: tauri::State<'_, AppState>, id: String) -> Result<String, String> {
    lock(&state.script_registry)
        .get_script_path(&id)
        .ok_or_else(|| format!("Script '{}' not found", id))
}

#[tauri::command(async)]
pub(crate) fn get_tool_preferences(app: tauri::AppHandle) -> Result<crate::tool_preferences::ToolPreferences, String> {
    crate::tool_preferences::read(&app)
}

#[tauri::command(async)]
pub(crate) fn set_tool_preference(app: tauri::AppHandle, kind: String, name: String, enabled: bool) -> Result<crate::tool_preferences::ToolPreferences, String> {
    crate::tool_preferences::set(&app, &kind, &name, enabled)
}

// Built-in tool catalogues, for the tool pages

#[tauri::command(async)]
pub(crate) fn list_codebase_index_tools() -> Vec<config_gen::CodebaseIndexToolInfo> {
    config_gen::codebase_index_tools()
}

#[tauri::command(async)]
pub(crate) fn list_agentos_tools() -> Vec<config_gen::CodebaseIndexToolInfo> {
    config_gen::agentos_tools()
}

#[tauri::command(async)]
pub(crate) fn list_kanban_tools() -> Vec<config_gen::CodebaseIndexToolInfo> {
    config_gen::kanban_tools()
}

/// The agent worktree tools (docs/PLAN-WORKTREES.md), for the tool pages.
#[tauri::command(async)]
pub(crate) fn list_worktree_tools() -> Vec<config_gen::CodebaseIndexToolInfo> {
    config_gen::worktree_tools()
}

/// The suite's bridge catalogue (`quantsuite.<module>.<name>`), read-only:
/// Full metadata stays available here even when tools are hidden from clients.
#[tauri::command(async)]
pub(crate) fn list_bridge_tools() -> Vec<qs_mcp_bridge::Capability> {
    qs_mcp_bridge::capabilities().to_vec()
}
