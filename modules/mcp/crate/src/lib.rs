mod clients;
mod commands;
mod config_gen;
pub mod git_helpers;
mod indexing;
pub mod kanban_db;
mod logs;
mod mcp;
mod mcp_server;
mod native_tools;
mod process;
mod scripts;
mod settings;
mod tools;
mod tool_preferences;
mod worktree;
mod worktree_cleanup;

use kanban_db::KanbanDb;
use logs::LogStore;
use mcp::McpRegistry;
use mcp_server::ActiveClientMap;
use native_tools::NativeToolRegistry;
use process::ProcessManager;
use scripts::ScriptRegistry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::Manager;
use tauri::Wry;
use tools::ToolRegistry;

pub struct AppState {
    pub registry: Mutex<McpRegistry>,
    pub process_manager: ProcessManager,
    pub tool_registry: Arc<Mutex<ToolRegistry>>,
    pub native_tool_registry: Arc<Mutex<NativeToolRegistry>>,
    pub mcp_server_port: u16,
    pub log_store: Arc<Mutex<LogStore>>,
    pub script_registry: Arc<Mutex<ScriptRegistry>>,
    pub quantmcp_enabled: Arc<Mutex<bool>>,
    pub kanban_db: Arc<KanbanDb>,
    pub active_clients: ActiveClientMap,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

// â"€â"€ App Entry â"€â"€

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `mcp` plugin. Pinned to `Wry` — see the note on `systems`.
///
/// Note what this module does NOT change: it writes into *other* applications'
/// config files (Claude Desktop, Cursor, VS Code) to install MCP servers. Those
/// paths are external and stay exactly as they were.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("mcp")
        .setup(|app, _api| {
            // Was `app.path().app_data_dir()`, which resolves from the bundle
            // identifier — in the suite that would silently point at a different
            // directory and lose every MCP, tool and project.
            let data_dir = qs_core::paths::module_dir("mcp");
            std::fs::create_dir_all(&data_dir)?;
            // One-time fresh-start cleanup (PLAN-WORKSPACE-UNIFY): sample
            // tools, placeholder MCP entries and the retired projects.json.
            // Runs before the registries below read their files.
            cleanup_sample_data(&data_dir);

            let mcp_config_path = data_dir.join("mcps.json");
            let registry = McpRegistry::new(mcp_config_path);

            let tools_config_path = data_dir.join("tools.json");
            let tool_registry = Arc::new(Mutex::new(ToolRegistry::new(tools_config_path)));

            let native_tools_dir = app
                .path()
                .resource_dir()
                .unwrap_or_default()
                .join("native_tools");
            let native_tools_state_path = data_dir.join("native_tools_state.json");
            let native_tool_registry = Arc::new(Mutex::new(NativeToolRegistry::new(
                native_tools_dir,
                native_tools_state_path,
            )));

            let log_store = Arc::new(Mutex::new(LogStore::new()));

            let scripts_meta_path = data_dir.join("scripts.json");
            let scripts_dir = data_dir.join("scripts");
            let script_registry = Arc::new(Mutex::new(ScriptRegistry::new(
                scripts_meta_path,
                scripts_dir,
            )));

            let quantmcp_enabled = Arc::new(Mutex::new(true));

            // Initialize SQLite kanban database (v2, workspace-keyed; a
            // pre-unify DB is retired to kanban.legacy.db inside open()).
            let kanban_db_path = data_dir.join("kanban.db");
            let kanban_db = Arc::new(
                KanbanDb::open(&kanban_db_path).expect("Failed to open kanban database"),
            );
            // One-time fresh-start (PLAN-KANBAN-UNIFY): the drawer's own
            // `core / kanban.card` entities are gone — kanban.db is the one
            // store for every board — and the mirror is rebuilt in the new
            // shape. qs-core registers before this plugin, so core.db and
            // the runtime hooks are up.
            cleanup_kanban_boards(app, &data_dir, &kanban_db);

            // Persistent Windows sharing failures survive a restart. Work stays
            // off the UI thread; only explicitly queued remnants are retried.
            std::thread::spawn(|| loop {
                worktree_cleanup::retry();
                std::thread::sleep(std::time::Duration::from_secs(60));
            });

            // 3100 belongs to the release build — external MCP clients (Claude
            // Code etc.) are configured against it. A dev instance running
            // beside the installed suite binds 3101 instead of failing the bind.
            let mcp_server_port: u16 = if cfg!(debug_assertions) { 3101 } else { 3100 };

            let active_clients: ActiveClientMap =
                Arc::new(tokio::sync::Mutex::new(HashMap::new()));

            // Ensure the codebase-index Python venv is set up (background,
            // non-blocking; venv under module_dir("mcp")/pyenv).
            let venv_app = app.clone();
            std::thread::spawn(move || indexing::ensure_venv(&venv_app));

            // Start built-in MCP server in background. The handle is the
            // server's door to core.db: workspace registry, approval modes,
            // index settings.
            let server_app = app.clone();
            let tool_reg_clone = tool_registry.clone();
            let native_tool_reg_clone = native_tool_registry.clone();
            let log_store_clone = log_store.clone();
            let quantmcp_enabled_clone = quantmcp_enabled.clone();
            let kanban_db_clone = kanban_db.clone();
            let active_clients_clone = active_clients.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = mcp_server::start_mcp_server(
                    server_app,
                    tool_reg_clone,
                    native_tool_reg_clone,
                    log_store_clone,
                    quantmcp_enabled_clone,
                    kanban_db_clone,
                    active_clients_clone,
                    mcp_server_port,
                )
                .await
                {
                    eprintln!("MCP server error: {}", e);
                }
            });

            app.manage(AppState {
                registry: Mutex::new(registry),
                process_manager: ProcessManager::new(),
                tool_registry,
                native_tool_registry,
                mcp_server_port,
                log_store,
                script_registry,
                quantmcp_enabled,
                kanban_db,
                active_clients,
            });

            // Announce the server fleet to the suite's process register,
            // stopped: nothing is spawned at setup, and the page lists stopped
            // processes so they can be started from there. See the note above
            // `commands::registry::SERVERS_PROCESS_ID` for why this is one entry and not one per
            // configured server, and why there is no log tail.
            qs_core::processes::announce(
                app,
                qs_core::processes::ProcessInfo::new(commands::registry::SERVERS_PROCESS_ID, "mcp", "MCP Servers")
                    .start_with("plugin:mcp|process_start_servers")
                    .stop_with("plugin:mcp|process_stop_servers")
                    // Nothing tells this module that a server died — it only
                    // ever finds out by reaping in `status_all`. Without this
                    // the page would keep showing "running" for a server that
                    // exited while the user was watching it.
                    .refresh_with("plugin:mcp|get_all_mcp_statuses"),
            );

            // The standalone app built its own tray icon here and its own
            // close-to-hide handler. Both now belong to qs-core, which owns the
            // single suite tray and the window lifecycle (ARCHITECTURE.md §10).
            // This module only contributes a menu entry.
            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "mcp".into(),
                    label: "QuantMCP".into(),
                    route: "/mcp".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            // MCP CRUD
            commands::registry::list_mcps,
            commands::registry::toggle_mcp,
            commands::registry::get_mcp_config,
            commands::registry::add_mcp,
            commands::registry::update_mcp,
            commands::registry::delete_mcp,
            // Process management
            commands::registry::start_mcp,
            commands::registry::stop_mcp,
            commands::registry::get_mcp_status,
            commands::registry::get_all_mcp_statuses,
            commands::registry::process_start_servers,
            commands::registry::process_stop_servers,
            // Tool CRUD
            commands::tools::list_tools,
            commands::tools::get_tool_preferences,
            commands::tools::set_tool_preference,
            commands::tools::add_tool,
            commands::tools::update_tool,
            commands::tools::delete_tool,
            commands::tools::toggle_tool,
            commands::tools::test_tool,
            // QuantMCP server toggle
            commands::tools::get_quantmcp_enabled,
            commands::tools::set_quantmcp_enabled,
            // File I/O
            commands::tools::read_file,
            commands::tools::write_file,
            // Logs
            commands::tools::list_logs,
            commands::tools::clear_logs,
            // Scripts
            commands::tools::list_scripts,
            commands::tools::add_script,
            commands::tools::update_script,
            commands::tools::delete_script,
            commands::tools::get_script_path,
            // Connect: the AI-client table, one button, the standard snippet
            commands::clients::scan_clients,
            commands::clients::connect_clients,
            commands::clients::connect_mcp_entry,
            commands::clients::get_connect_snippet,
            commands::clients::list_clients,
            commands::clients::forget_client,
            commands::tools::list_codebase_index_tools,
            commands::tools::list_agentos_tools,
            commands::tools::list_kanban_tools,
            commands::tools::list_worktree_tools,
            commands::tools::list_bridge_tools,
            commands::clients::get_mcp_server_port,
            // Per-workspace state (the folder list itself is the core.db
            // workspace registry, read via @quantsuite/core in the webview)
            commands::workspace::get_approval_mode,
            commands::workspace::set_approval_mode,
            commands::workspace::get_workspace_index_settings,
            commands::workspace::set_workspace_index_settings,
            commands::kanban::list_kanban_cards,
            commands::kanban::add_kanban_card,
            commands::kanban::update_kanban_card,
            commands::kanban::move_kanban_card,
            commands::kanban::review_kanban_card,
            commands::kanban::move_kanban_card_to_workspace,
            commands::kanban::delete_kanban_card,
            commands::kanban::list_archived_kanban_cards,
            commands::kanban::archive_done_kanban_cards,
            commands::kanban::archive_kanban_card,
            commands::kanban::unarchive_kanban_card,
            // Codebase indexing
            commands::workspace::index_project_codebase,
            commands::workspace::get_codebase_index_stats,
            // AGENT.md management
            commands::agentos::get_agent_md_paths,
            commands::agentos::read_agent_md,
            commands::agentos::write_agent_md,
            commands::agentos::init_agent_md,
            commands::agentos::reset_agent_md,
        ]))
        .build()
}

/// One-time fresh-start cleanup (PLAN-KANBAN-UNIFY): clears every
/// `core / kanban.card` entity — the drawer's pre-unify user cards and the
/// old-shape agent mirrors — and re-mirrors kanban.db's cards in the new
/// shape. No data migration by decision; kanban.db itself is untouched.
fn cleanup_kanban_boards(
    app: &tauri::AppHandle,
    data_dir: &std::path::Path,
    kanban_db: &Arc<KanbanDb>,
) {
    let marker = data_dir.join(".cleanup-kanban-unify");
    if marker.exists() {
        return;
    }
    let cleared = settings::with_core_db(app, |conn| {
        let cards = qs_core::db::list_entities(conn, None, Some("kanban.card"), 100_000)
            .map_err(|e| e.to_string())?;
        for e in &cards {
            qs_core::db::delete_entity(conn, &e.id).map_err(|e| e.to_string())?;
        }
        Ok(cards.len())
    });
    match cleared {
        Ok(n) => {
            eprintln!("mcp: kanban-unify fresh start — {n} old board entities cleared");
            kanban_db.remirror_all();
            let _ = std::fs::write(&marker, "kanban boards unified (PLAN-KANBAN-UNIFY)\n");
        }
        // No marker written — retried on the next start until core.db is up.
        Err(e) => eprintln!("mcp: kanban-unify cleanup deferred ({e})"),
    }
}

/// One-time fresh-start cleanup (PLAN-WORKSPACE-UNIFY). Replaces the old
/// `import_legacy_data` copy from the standalone app: instead of pulling
/// legacy data in, it clears the sample/test entries that copy left behind.
/// Everything it removes is either regenerable or parked with a `.legacy`
/// name — nothing is destroyed.
fn cleanup_sample_data(data_dir: &std::path::Path) {
    let marker = data_dir.join(".cleanup-workspace-unify");
    if marker.exists() {
        return;
    }

    // tools.json: drop the demo tools and anything whose script lives in the
    // legacy standalone checkout — the features (user tools) stay.
    let tools_path = data_dir.join("tools.json");
    if let Ok(raw) = std::fs::read_to_string(&tools_path) {
        if let Ok(serde_json::Value::Array(entries)) = serde_json::from_str(&raw) {
            let kept: Vec<serde_json::Value> = entries
                .into_iter()
                .filter(|t| {
                    let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let script = t
                        .get("script_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .replace('\\', "/")
                        .to_lowercase();
                    !(matches!(id, "tool-hello" | "tool-sysinfo")
                        || script.contains("/projects/quantmcp/"))
                })
                .collect();
            if let Ok(data) = serde_json::to_string_pretty(&kept) {
                let _ = std::fs::write(&tools_path, data);
            }
        }
    }

    // mcps.json: drop the "Test n" placeholders (no command, never real).
    let mcps_path = data_dir.join("mcps.json");
    if let Ok(raw) = std::fs::read_to_string(&mcps_path) {
        if let Ok(serde_json::Value::Array(entries)) = serde_json::from_str(&raw) {
            let kept: Vec<serde_json::Value> = entries
                .into_iter()
                .filter(|m| {
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let no_command = m.get("command").map(|c| c.is_null()).unwrap_or(true);
                    !(no_command && name.to_lowercase().starts_with("test "))
                })
                .collect();
            if let Ok(data) = serde_json::to_string_pretty(&kept) {
                let _ = std::fs::write(&mcps_path, data);
            }
        }
    }

    // projects.json is retired — the workspace registry replaced it. Parked,
    // not deleted, in case anything in it turns out to matter.
    let projects_path = data_dir.join("projects.json");
    if projects_path.exists() {
        let _ = std::fs::rename(&projects_path, data_dir.join("projects.legacy.json"));
    }

    // Stale state for native tools that don't ship with the suite.
    let _ = std::fs::remove_file(data_dir.join("native_tools_state.json"));

    let _ = std::fs::write(&marker, chrono::Utc::now().to_rfc3339());
}
