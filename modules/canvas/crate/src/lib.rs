mod commands;

use std::path::Path;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::Wry;

/// Build the `code` plugin. Pinned to `Wry` — see the note on `systems`.
///
/// The dialog and shell plugins the standalone app registered are now
/// registered once by the suite binary.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("canvas")
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            // File system
            commands::fs::read_dir_tree,
            commands::fs::read_file,
            commands::fs::write_file,
            commands::fs::create_file,
            commands::fs::delete_file,
            commands::fs::rename_file,
            commands::fs::search_files,
            commands::fs::read_file_binary,
            // Git
            commands::git::git_status,
            commands::git::git_diff,
            commands::git::git_stage,
            commands::git::git_branch_list,
            commands::git::git_log,
            commands::git::git_commit_files,
            // Timeline: local save history (QuantCode right panel)
            commands::timeline::timeline_snapshot,
            commands::timeline::timeline_list,
            commands::timeline::timeline_read,
            // Per-project canvas state
            commands::workspace::workspace_storage_dir,
            commands::workspace::load_canvas_state,
            commands::workspace::save_canvas_state,
            // Browser
            commands::browser::navigate_browser,
            commands::browser::eval_browser,
            commands::browser::get_browser_url,
            commands::browser::set_browser_clip_region,
            commands::browser::find_in_browser,
            commands::browser::browser_request_new_tab,
            commands::browser::browser_disable_fullscreen,
            // Browser data
            commands::browser_data::load_browser_data,
            commands::browser_data::save_browser_data,
        ]))
        .setup(|app, _api| {
            let dir = qs_core::paths::module_dir("canvas");
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("code: failed to create {}: {e}", dir.display());
            }
            import_legacy_data(&dir);

            // No managed state left: the only thing this plugin held was the PTY
            // registry, and the terminals moved to `plugin:console` with P4.5.

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "canvas".into(),
                    label: "QuantCode".into(),
                    route: "/canvas".into(),
                    toggle_window: None,
                },
            );

            // The terminals this module used to own moved to `plugin:console`
            // with P4.5 (docs/PLAN-CONSOLE.md): one emulator for the suite. Their
            // shutdown hook moved with them, so there is nothing to tear down
            // here — the embedded browser's webviews die with the window.

            Ok(())
        })
        .build()
}

/// One-time import of `~/.quantcode/` — the standalone app's browser data.
///
/// It used to carry the workspace registry too; that moved to `core.db` on
/// 2026-08-26 (docs/PLAN-WORKSPACES.md) and any `workspaces.json` copied here
/// is inert.
///
/// Imports standalone browser data. Per-project .quantcode data is migrated
/// separately by commands::workspace when that workspace is first used.
fn import_legacy_data(target: &Path) {
    let marker = target.join(".migrated");
    if marker.exists() {
        return;
    }

    let legacy = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".quantcode");

    if legacy.exists() && legacy != target {
        if let Err(e) = copy_dir_recursive(&legacy, target) {
            eprintln!("code: legacy import failed: {e}");
            return;
        }
    }

    let _ = std::fs::write(&marker, chrono::Utc::now().to_rfc3339());
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else if !dst.exists() {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
