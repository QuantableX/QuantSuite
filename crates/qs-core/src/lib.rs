//! QuantSuite core — the infrastructure every module shares.
//!
//! Registered as Tauri plugin `core`, so its commands are invoked as
//! `plugin:core|<name>` and cannot collide with a module's (ARCHITECTURE.md §2).
//!
//! What lives here: the event bus (§3), `core.db` with entities/links/settings
//! (§5), the process register (§6), the tray and window lifecycle (§10) and the
//! ordered shutdown. Modules own their own data and commands; they never
//! duplicate any of this.

pub mod agent;
pub mod apps;
pub mod autostart;
pub mod bus;
pub mod commands;
pub mod diagnostics;
pub mod runtime;
pub mod db;
pub mod migrations;
pub mod paths;
pub mod processes;
pub mod shutdown;
pub mod tray;
pub mod window;
pub mod workspaces;

pub use bus::{Bus, Event};
pub use db::{Db, Entity, Link};
pub use processes::{ProcessInfo, ProcessRegistry, ProcessState};

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

/// Build the `qs` plugin. Registered first in the Tauri builder, before any
/// module plugin — modules assume its state exists.
///
/// The name is `qs`, not `core`: Tauri reserves `core` for its own built-in
/// commands and panics with `ReservedName("core")` at startup otherwise.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("qs")
        .invoke_handler(crate::diagnostics::traced(tauri::generate_handler![
            commands::get_settings,
            commands::get_setting,
            commands::set_setting,
            commands::emit_event,
            commands::recent_events,
            commands::upsert_entity,
            commands::delete_entity,
            commands::list_entities,
            commands::count_entities,
            commands::search_entities,
            commands::link_entities,
            commands::unlink_entities,
            commands::linked_entities,
            commands::process_list,
            commands::set_circular_window,
            commands::window_new,
            commands::window_show,
            commands::window_hide,
            commands::window_toggle,
            commands::quit,
            commands::autostart_enabled,
            commands::set_autostart,
            commands::app_version,
            commands::suite_paths,
            commands::agent_tools,
            commands::agent_tool_decision,
            commands::agent_pending_calls,
            commands::agent_call_claim,
            commands::agent_call_complete,
        ]))
        .setup(|app, _api| {
            paths::ensure()?;

            // First, so every line below actually lands somewhere, and so the
            // main-thread watchdog covers startup too — the phase where a
            // blocking command is most tempting (ARCHITECTURE.md §11).
            diagnostics::install(app.clone());

            let conn = db::open()?;
            // One-time renames (dirs + rows) must land before any module
            // plugin's setup reads its own scope or data directory.
            migrations::run(&conn)?;
            migrations::run_zen_to_notes(&conn)?;
            app.manage(Db(std::sync::Mutex::new(conn)));
            app.manage(Bus::new());
            // A map of what the modules run, not an owner of it — module
            // plugins announce into this during their own setup, which runs
            // after ours (see `processes.rs` for why nothing is owned here).
            app.manage(ProcessRegistry::new());
            app.manage(tray::TrayRegistry::default());
            app.manage(shutdown::Teardown::new());
            app.manage(agent::AgentBroker::default());
            // The broker's entry point for QuantMCP's MCP server (E4). Must
            // come after the states above — a call reads settings and state.
            agent::install(app.clone());
            // Erased data-plane writers for sibling crates (kanban mirror).
            runtime::install(app.clone());

            tray::init(app)?;
            // No geometry to restore: the suite window is always maximised
            // (launcher and app differ only by the circular region), and
            // QuantHUD's overlays place themselves in `tauri-plugin-hud`. The
            // save/restore pair that used to live in `window.rs` had no reader
            // at all, so it wrote a settings row per hide and nothing else.

            install_panic_hook(app.clone());

            log::info!("qs-core ready — data at {}", paths::root().display());
            Ok(())
        })
        .build()
}

/// Write panics to `~/.quantsuite/logs/` before unwinding. A crash in a fused
/// binary is much harder to diagnose than in eight separate apps, so this is
/// not optional (ARCHITECTURE.md §11).
fn install_panic_hook<R: Runtime>(app: tauri::AppHandle<R>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info.to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".into());

        let line = format!(
            "[{}] panic at {location}: {message}\n",
            chrono::Utc::now().to_rfc3339()
        );
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::logs_dir().join("panic.log"))
        {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }

        let _ = bus::emit(
            &app,
            "core.process.panicked",
            serde_json::json!({ "location": location, "message": message }),
        );

        previous(info);
    }));
}
