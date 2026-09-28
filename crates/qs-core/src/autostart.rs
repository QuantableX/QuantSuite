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
//!
//! "Update & restart" relaunches the new version with this process's own
//! arguments — the Windows installer gets them through `/ARGS`, `restart()`
//! reuses them elsewhere — so a suite started at boot comes back with [`FLAG`]
//! too. [`mark_update_relaunch`] leaves a short-lived marker right before that
//! hand-over, and a launch that finds it shows `main` despite the flag.

use crate::{paths, window};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// Older than this, the marker is a leftover of an update that never
/// relaunched, not the launch that is starting now.
const UPDATE_MARKER_TTL: Duration = Duration::from_secs(15 * 60);

fn update_marker() -> PathBuf {
    paths::root().join("update-relaunch")
}

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
    // Taken on every launch, so a leftover never reaches a later boot.
    let updated = take_marker(&update_marker(), UPDATE_MARKER_TTL);
    if launched_by_autostart() && !updated {
        log::info!("started by autostart — staying in the tray");
        return;
    }
    window::show(app);
}

/// Call right before an update relaunches the suite: the new version then
/// shows `main` even when it inherits [`FLAG`].
pub fn mark_update_relaunch() {
    let path = update_marker();
    if let Err(e) = fs::write(&path, b"") {
        log::warn!("cannot write {}: {e}", path.display());
    }
}

/// Whether a marker younger than `ttl` was there. It is removed either way.
fn take_marker(path: &Path, ttl: Duration) -> bool {
    let fresh = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|written| written.elapsed().ok())
        .is_some_and(|age| age < ttl);
    let _ = fs::remove_file(path);
    fresh
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn only_a_fresh_marker_counts_and_every_marker_is_taken() {
        let dir = std::env::temp_dir().join(format!("qs-core-autostart-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("update-relaunch");

        assert!(!take_marker(&marker, UPDATE_MARKER_TTL), "no update, a boot launch");

        fs::write(&marker, b"").unwrap();
        assert!(take_marker(&marker, UPDATE_MARKER_TTL), "the relaunch right after an update");
        assert!(!marker.exists());
        assert!(!take_marker(&marker, UPDATE_MARKER_TTL), "the next boot is a boot again");

        fs::write(&marker, b"").unwrap();
        let old = SystemTime::now() - UPDATE_MARKER_TTL - Duration::from_secs(60);
        fs::File::options().write(true).open(&marker).unwrap().set_modified(old).unwrap();
        assert!(!take_marker(&marker, UPDATE_MARKER_TTL), "a leftover from an update that never relaunched");
        assert!(!marker.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
