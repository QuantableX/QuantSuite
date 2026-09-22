//! Which shells this machine actually has.
//!
//! QuantCanvas hardcodes its shell list in the frontend (`SHELL_OPTIONS`), which
//! offers shells the machine may not have — the spawn then fails with a raw OS
//! error. Console asks the OS instead: the picker only ever shows shells that
//! exist.
//!
//! On Windows that list is deliberately **PowerShell and cmd only**. Git Bash and
//! WSL were detected here and are not any more: both are MSYS/Linux environments
//! wearing a Windows path, and everything path-shaped this module does is
//! written for Windows paths. Offering them made the picker longer and its
//! results wrong.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOption {
    /// Stable key for settings and session metadata.
    pub id: String,
    pub label: String,
    pub path: String,
    /// The one preselected in the picker.
    pub is_default: bool,
}

/// Resolve an executable name against `PATH`.
///
/// No `which` crate for one lookup — and on Windows the extension matters, so
/// the candidate is expected to carry it (`pwsh.exe`, not `pwsh`).
fn in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

fn absolute(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    p.is_file().then(|| p.to_path_buf())
}

#[cfg(windows)]
fn candidates() -> Vec<(&'static str, &'static str, Option<PathBuf>)> {
    vec![
        ("pwsh", "PowerShell 7", in_path("pwsh.exe")),
        (
            "powershell",
            "Windows PowerShell",
            absolute("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
        ),
        ("cmd", "Command Prompt", absolute("C:\\Windows\\System32\\cmd.exe")),
    ]
}

#[cfg(not(windows))]
fn candidates() -> Vec<(&'static str, &'static str, Option<PathBuf>)> {
    vec![
        ("zsh", "zsh", absolute("/bin/zsh").or_else(|| in_path("zsh"))),
        ("bash", "bash", absolute("/bin/bash").or_else(|| in_path("bash"))),
        ("fish", "fish", in_path("fish")),
        ("sh", "sh", absolute("/bin/sh")),
    ]
}

/// Every shell present on this machine, best first. The default is the one
/// `qs-pty` would pick on its own, so the picker and a `shell: null` spawn never
/// disagree; if that one is somehow absent, the first available wins.
#[tauri::command]
pub async fn list_shells() -> Result<Vec<ShellOption>, String> {
    let fallback = qs_pty::default_shell();

    let mut shells: Vec<ShellOption> = candidates()
        .into_iter()
        .filter_map(|(id, label, found)| {
            let path = found?;
            let path = path.to_string_lossy().into_owned();
            Some(ShellOption {
                id: id.to_string(),
                label: label.to_string(),
                path,
                is_default: false,
            })
        })
        .collect();

    // `default_shell()` may be a bare name resolved via PATH ("powershell.exe")
    // or an absolute path ("/bin/zsh") — compare the whole string, then the
    // file name, case-insensitively for Windows.
    let file_name = |p: &str| {
        Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
    };
    let fallback_name = file_name(fallback);

    let default_index = shells
        .iter()
        .position(|s| s.path == fallback || file_name(&s.path) == fallback_name)
        .unwrap_or(0);

    if let Some(shell) = shells.get_mut(default_index) {
        shell.is_default = true;
    }

    Ok(shells)
}
