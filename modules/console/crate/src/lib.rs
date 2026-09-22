//! QuantConsole — the suite-wide terminal console, plain-terminal edition.
//!
//! The block terminal (engine, `console.db`, completions, workflows) was
//! removed on 2026-08-20 at the user's decision — the module is back to the
//! original QuantCode contract: PTY bytes to an xterm pane, nothing between
//! them. See docs/CONSOLE-ROLLBACK.md for what was removed and where the
//! pre-rollback snapshot lives.
//!
//! The PTY mechanics themselves are **not** here — they live in `crates/qs-pty`,
//! shared with QuantCanvas, so the suite runs one implementation of the two
//! threads, the coalescing and the detach buffer.

mod commands;
pub mod settings;
mod state;

// QuantPilot starts its agent CLIs in this registry (one PTY implementation,
// one output channel, one owner rule) — it needs the state and the spawn.
pub use commands::session::{spawn_session, OpenSessionRequest};
pub use state::ConsoleState;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Wry};

/// Build the `console` plugin. Pinned to `Wry` like every other module plugin.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("console")
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            commands::session::open_session,
            commands::session::write_session,
            commands::session::resize_session,
            commands::session::close_session,
            commands::session::detach_session,
            commands::session::attach_session,
            commands::session::session_alive,
            commands::session::list_sessions,
            commands::shells::list_shells,
            commands::paste::save_pasted_image,
            commands::agent::run_command,
        ]))
        .setup(|app, _api| {
            // The module directory still hosts pasted images (`pasted/`).
            let dir = qs_core::paths::module_dir("console");
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("console: failed to create {}: {e}", dir.display());
            }

            app.manage(ConsoleState::new());

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "console".into(),
                    label: "QuantConsole".into(),
                    route: "/console".into(),
                    toggle_window: None,
                },
            );

            // Shells outlive a hidden window, so they are killed on Quit rather
            // than on close (ARCHITECTURE.md §10). A leaked shell in a fused
            // binary is much harder to notice than in a single-purpose app.
            let handle = app.clone();
            qs_core::shutdown::on_shutdown(
                app,
                "console:sessions",
                Box::new(move || {
                    if let Some(state) = handle.try_state::<ConsoleState>() {
                        state.close_all();
                    }
                }),
            );

            Ok(())
        })
        .build()
}
