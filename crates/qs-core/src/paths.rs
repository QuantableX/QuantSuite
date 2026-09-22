//! The `~/.quantsuite/` layout. Every path the suite writes to comes from here,
//! so no module invents its own location (ARCHITECTURE.md §5).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// `~/.quantsuite/` in release builds, `~/.quantsuite-dev/` in debug builds —
/// a `tauri dev` instance must be able to run beside the installed suite
/// without contending for core.db and the module dirs (the dev config also
/// uses its own identifier, so the single-instance lock no longer guards
/// across the two). `QUANTSUITE_HOME` overrides both; pointing a dev build at
/// the real `~/.quantsuite` is only safe while the installed suite is closed.
pub fn root() -> PathBuf {
    if let Some(home) = std::env::var_os("QUANTSUITE_HOME") {
        return PathBuf::from(home);
    }
    let dir = if cfg!(debug_assertions) {
        ".quantsuite-dev"
    } else {
        ".quantsuite"
    };
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(dir)
}

/// `~/.quantsuite/core.db`
pub fn core_db() -> PathBuf {
    root().join("core.db")
}

/// `~/.quantsuite/modules/<id>/`
pub fn module_dir(module: &str) -> PathBuf {
    root().join("modules").join(module)
}

/// Private workspace data, keyed by the registry's normalized path identity.
pub fn workspace_dir(folder: &str) -> PathBuf {
    let id = crate::workspaces::workspace_id_for(folder);
    root().join("workspaces").join(id.rsplit(':').next().unwrap())
}

/// Agent screenshots, scratch scripts and logs, outside the checkout.
pub fn workspace_artifacts_dir(folder: &str) -> PathBuf {
    workspace_dir(folder).join("artifacts")
}

/// Private QuantScript sources shared by every Python consumer. Never bundled.
pub fn indicators_dir() -> PathBuf {
    std::env::var_os("QUANTSCRIPT_INDICATORS_DIR")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let software_dir = if cfg!(debug_assertions) {
                // Development uses the checkout, outside distributable sidecars.
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
            } else {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| exe.parent().map(Path::to_path_buf))
                    .unwrap_or_else(root)
            };
            software_dir.join("QuantScript").join("indicators")
        })
}


/// Create an empty library. Installing/updating must never seed private code.
pub fn ensure_indicator_scripts() -> io::Result<PathBuf> {
    let target = indicators_dir();
    fs::create_dir_all(&target)?;
    Ok(target)
}

/// `~/.quantsuite/logs/`
pub fn logs_dir() -> PathBuf {
    root().join("logs")
}

/// `~/.quantsuite/secrets/`
pub fn secrets_dir() -> PathBuf {
    root().join("secrets")
}

/// Create the directory skeleton. Idempotent; safe to call on every launch.
pub fn ensure() -> std::io::Result<()> {
    fs::create_dir_all(root())?;
    fs::create_dir_all(root().join("modules"))?;
    fs::create_dir_all(logs_dir())?;
    fs::create_dir_all(secrets_dir())?;
    Ok(())
}

/// Create a module's own directory on first use.
pub fn ensure_module(module: &str) -> std::io::Result<PathBuf> {
    let dir = module_dir(module);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The Python sidecar tree: `sidecars/python/` in a repo checkout, bundled to
/// `<resource_dir>/sidecars/python/` in an installed suite (tauri.conf.json
/// `bundle.resources`). Modules spawn their engines from here — QuantSystems
/// runs `rotation_lab`, QuantAlgo imports `quantalgo` — so the search lives in
/// qs-core rather than each module walking directories its own way.
///
/// `marker` is the package the caller is about to run. A candidate only counts
/// when `<candidate>/<marker>/` exists: a stale `QUANTSUITE_PYTHON_DIR`, a
/// half-copied bundle or an unrelated `sidecars/` next to the exe falls
/// through to the next candidate instead of handing Python a directory it
/// cannot import from. `None` means no candidate had the package — callers
/// decide how loudly that fails.
pub fn python_sidecar_dir(app: &tauri::AppHandle, marker: &str) -> Option<PathBuf> {
    use tauri::Manager;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("QUANTSUITE_PYTHON_DIR") {
        if !dir.is_empty() {
            candidates.push(PathBuf::from(dir));
        }
    }
    // Installed bundle: the fixed spot tauri.conf.json copies the tree to.
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("sidecars").join("python"));
    }
    // Repo checkout: `target/debug` and `target/release` sit two levels below
    // the workspace root, `tauri dev` may run with cwd anywhere between the
    // root and `apps/src-tauri` — walk a few levels up from both anchors.
    let rel = PathBuf::from("sidecars").join("python");
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(&rel));
        candidates.push(cwd.join("..").join(&rel));
        candidates.push(cwd.join("../..").join(&rel));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(&rel));
            candidates.push(dir.join("..").join(&rel));
            candidates.push(dir.join("../..").join(&rel));
            candidates.push(dir.join("../../..").join(&rel));
        }
    }
    candidates.into_iter().find(|c| c.join(marker).is_dir())
}

/// Replace `path` with `bytes` so a reader only ever sees the old file or the
/// new one. `fs::write` truncates first and fills second; a crash, power loss,
/// AV scan or a second writer in that window leaves an empty or partial file,
/// and for a JSON config that means the next load fails to parse and the
/// module runs on defaults. Every config / settings file the suite writes goes
/// through here (ARCHITECTURE.md §5).
///
/// The bytes go to `<name>.tmp` in the target's own directory (same volume, so
/// the rename is never a copy), are flushed to disk, and the temp file is then
/// renamed over the target — one atomic replace on NTFS and POSIX. The flush
/// matters: without it the rename can reach the disk before the data does. A
/// failure at any step leaves the target untouched and removes the temp file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = sibling(path, "tmp");
    let result = fs::File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// [`write_atomic`] that first copies the current file to `<name>.bak`, for
/// files whose content the suite cannot recreate: QuantHUD's config.json holds
/// the user's clipboard, transcript and todo history, and QuantMCP edits other
/// applications' config files (Claude Desktop, Cursor, …). The copy runs
/// before the replace, so the backup is the last complete state the target
/// had; a loader that cannot parse the target falls back to it via
/// [`backup_path`]. A first write has nothing to back up.
pub fn write_atomic_with_backup(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.is_file() {
        fs::copy(path, backup_path(path))?;
    }
    write_atomic(path, bytes)
}

/// `<name>.bak` next to `path`: where [`write_atomic_with_backup`] keeps the
/// previous content.
pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, "bak")
}

/// `<file name>.<suffix>` in the target's own directory. The suffix is
/// appended, not swapped in for the extension, so `config.json` and
/// `config.yaml` in one directory never share a temp file and a dotfile such
/// as `.aider.conf.yml` keeps its name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of our own under the OS temp dir; tests run in parallel
    /// inside one process, so each takes a distinct name.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qs-core-paths-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_atomic_creates_then_replaces_and_leaves_no_temp_file() {
        let dir = scratch("atomic");
        let path = dir.join("config.json");
        write_atomic(&path, b"{\"a\":1}").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"a\":1}");
        write_atomic(&path, b"{\"a\":2}").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"a\":2}");
        assert!(!dir.join("config.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_with_backup_keeps_the_previous_content() {
        let dir = scratch("backup");
        let path = dir.join("config.json");
        write_atomic_with_backup(&path, b"first").unwrap();
        assert!(!backup_path(&path).exists(), "a first write has nothing to back up");
        write_atomic_with_backup(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read(backup_path(&path)).unwrap(), b"first");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sibling_appends_to_the_whole_file_name() {
        assert_eq!(sibling(Path::new("x/config.json"), "tmp"), Path::new("x/config.json.tmp"));
        assert_eq!(backup_path(Path::new("x/.aider.conf.yml")), Path::new("x/.aider.conf.yml.bak"));
    }
}
