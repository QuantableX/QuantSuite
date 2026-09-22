//! The codebase-index engine: one vendored Python CLI, one venv, one index
//! directory — all suite-owned (PLAN-WORKSPACE-UNIFY).
//!
//! The CLI lives in `sidecars/python/codebase_index/` and is found through
//! `qs_core::paths::python_sidecar_dir` like every other engine; the legacy
//! `QUANTMCP_CODEBASE_INDEX_DIR` env override still wins for development
//! against a checkout elsewhere. Index DBs are one per workspace, named by the
//! workspace id's b36 tail (`indexes/<b36>.db`) — the CLI's basename-derived
//! naming is bypassed with `--name`. The venv sits in the module data dir, not
//! next to the CLI: bundled resources are read-only in an installed suite.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Where the vendored CLI lives. The marker package is `codebase_index`, and
/// the scripts sit inside that package directory.
pub fn cli_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("QUANTMCP_CODEBASE_INDEX_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    qs_core::paths::python_sidecar_dir(app, "codebase_index")
        .map(|d| d.join("codebase_index"))
        .ok_or_else(|| {
            "codebase_index sidecar not found — sidecars/python/codebase_index is missing from \
             the checkout or the installed bundle"
                .into()
        })
}

/// `~/.quantsuite/modules/mcp/indexes/` — one `<b36>.db` per workspace.
pub fn index_dir() -> PathBuf {
    qs_core::paths::module_dir("mcp").join("indexes")
}

fn venv_dir() -> PathBuf {
    qs_core::paths::module_dir("mcp").join("pyenv")
}

fn venv_python() -> PathBuf {
    if cfg!(windows) {
        venv_dir().join("Scripts/python.exe")
    } else {
        venv_dir().join("bin/python")
    }
}

fn python_exe() -> String {
    let venv = venv_python();
    if venv.exists() {
        venv.to_string_lossy().to_string()
    } else {
        "python".to_string()
    }
}

/// Create the venv and install the CLI's requirements once. Runs on a
/// background thread at setup — first launch pulls dependencies from the
/// network, later launches return on the marker check.
pub fn ensure_venv(app: &tauri::AppHandle) {
    let Ok(cli) = cli_dir(app) else { return };
    let requirements = cli.join("requirements.txt");
    if !requirements.exists() {
        return;
    }

    let venv = venv_dir();
    let python = venv_python();
    let pip = if cfg!(windows) {
        venv.join("Scripts/pip.exe")
    } else {
        venv.join("bin/pip")
    };

    let marker = venv.join(".deps_installed");
    if python.exists() && marker.exists() {
        if let (Ok(marker_meta), Ok(req_meta)) =
            (std::fs::metadata(&marker), std::fs::metadata(&requirements))
        {
            if let (Ok(marker_time), Ok(req_time)) = (marker_meta.modified(), req_meta.modified()) {
                if marker_time >= req_time {
                    return;
                }
            }
        }
    }

    eprintln!("mcp: setting up codebase-index Python environment…");

    if !python.exists() {
        let mut cmd = std::process::Command::new("python");
        cmd.args(["-m", "venv", &venv.to_string_lossy()]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        match cmd.output() {
            Ok(output) if output.status.success() => {
                eprintln!("mcp: venv created at {}", venv.display());
            }
            Ok(output) => {
                eprintln!(
                    "mcp: failed to create venv: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            }
            Err(e) => {
                eprintln!("mcp: failed to run python: {e}");
                return;
            }
        }
    }

    let mut cmd = std::process::Command::new(pip.to_string_lossy().to_string());
    cmd.args(["install", "-q", "-r", &requirements.to_string_lossy()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(output) if output.status.success() => {
            let _ = std::fs::write(&marker, "");
            eprintln!("mcp: codebase-index dependencies installed");
        }
        Ok(output) => {
            eprintln!(
                "mcp: pip install failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => eprintln!("mcp: failed to run pip: {e}"),
    }
}

/// Run one CLI subcommand and parse its JSON stdout. `QUANTMCP_INDEX_DIR` is
/// pinned on every invocation so the CLI can never fall back to the legacy
/// `~/.quantmcp/indexes/` — the suite owns its indexes.
pub async fn run_cli(app: &tauri::AppHandle, args: Vec<String>) -> Result<Value, String> {
    let cli_path = cli_dir(app)?.join("cli.py");
    if !cli_path.exists() {
        return Err(format!(
            "Codebase index CLI not found at {}.",
            cli_path.display()
        ));
    }

    let index_dir = index_dir();
    let _ = std::fs::create_dir_all(&index_dir);

    let mut cmd_args = vec![cli_path.to_string_lossy().to_string()];
    cmd_args.extend(args);

    let mut cmd = tokio::process::Command::new(python_exe());
    cmd.args(&cmd_args);
    cmd.env("QUANTMCP_INDEX_DIR", &index_dir);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run codebase index CLI: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        if !stderr.trim().is_empty() {
            return Err(stderr);
        }
        if let Ok(value) = serde_json::from_str::<Value>(&stdout) {
            if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
                return Err(err.to_string());
            }
        }
        return Err(if stdout.trim().is_empty() {
            format!("Codebase index CLI exited with code {:?}", output.status.code())
        } else {
            stdout
        });
    }

    let value: Value = serde_json::from_str(&stdout).map_err(|_| {
        format!("Codebase index CLI returned non-JSON output:\n{}", stdout.trim())
    })?;

    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }

    Ok(value)
}

// ── Result shapes (moved from the deleted projects.rs) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseIndexStatus {
    pub workspace_id: String,
    pub status: String, // "not_indexed" | "indexed"
    pub file_count: u32,
    pub indexed_at: Option<u64>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub fts_entry_count: Option<u32>,
    #[serde(default)]
    pub chunk_count: Option<u32>,
    #[serde(default)]
    pub structural_indexed_at: Option<u64>,
    #[serde(default)]
    pub semantic_indexed_at: Option<u64>,
}

impl CodebaseIndexStatus {
    pub fn not_indexed(workspace_id: String) -> Self {
        Self {
            workspace_id,
            status: "not_indexed".into(),
            file_count: 0,
            indexed_at: None,
            mode: None,
            fts_entry_count: None,
            chunk_count: None,
            structural_indexed_at: None,
            semantic_indexed_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexCodebaseResult {
    pub status: String,
    pub action: Option<String>,
    pub codebase: Option<String>,
    pub mode: Option<String>,
    pub files_indexed: Option<u32>,
    pub files_skipped: Option<u32>,
    pub errors: Option<u32>,
    pub total_entries: Option<u32>,
    #[serde(default)]
    pub structural_entries: Option<u32>,
    #[serde(default)]
    pub semantic_entries: Option<u32>,
    pub db_path: Option<String>,
    pub error: Option<String>,
}
