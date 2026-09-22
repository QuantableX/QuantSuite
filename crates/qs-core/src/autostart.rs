//! Start QuantSuite with the OS session (ARCHITECTURE.md §10, "Autostart").
//!
//! Optional, off by default, toggled in suite settings. The OS entry is the
//! single source of truth — on Windows that is the `Run` key under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, written by
//! `tauri-plugin-autostart`. Nothing is mirrored into `core.db`: a settings row
//! and a registry value drift apart the moment the user removes the entry
//! through Task Manager, and then the toggle lies.
//!
//! The entry carries [`FLAG`] as an argument, which is how a boot launch is told
//! apart from the user double-clicking the icon. A boot launch stays in the tray
//! with `main` hidden; the suite is tray-resident anyway, so this is the same
//! end state as closing the window.
//!
//! `main` is declared `"visible": false` in `tauri.conf.json` for that reason —
//! showing it and hiding it again a moment later flashes an empty maximised
//! window across the desktop on every boot. [`apply_launch_visibility`] shows it
//! for every launch that is *not* an autostart one, and the app's `setup` hook
//! is the earliest place that can: plugin `setup` runs before Tauri creates the
//! windows declared in the config, so there is nothing to show yet from here.

use crate::window;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// Passed to the registered autostart entry, so a boot launch is recognisable.
/// Also the argument list handed to `tauri_plugin_autostart::init` in
/// `apps/src-tauri/src/lib.rs` — the two must agree.
pub const FLAG: &str = "--autostart";

/// True when this process was started by the OS autostart entry rather than by
/// the user.
pub fn launched_by_autostart() -> bool {
    std::env::args().any(|arg| arg == FLAG)
}

/// Show `main` unless the OS started us. Call once from the app's `setup` hook.
pub fn apply_launch_visibility<R: Runtime>(app: &AppHandle<R>) {
    if launched_by_autostart() {
        log::info!("started by autostart — staying in the tray");
        return;
    }
    window::show(app);
}

/// Whether the OS autostart entry exists right now.
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Register or remove the OS autostart entry.
///
/// The registered path is this binary's, so enabling it from `tauri dev` points
/// the entry at `target/debug/quantsuite.exe`. Harmless, but that entry keeps
/// working after the debug build is gone — toggling it off in a dev session
/// removes it again.
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| e.to_string())?;
    log::info!("autostart {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}
