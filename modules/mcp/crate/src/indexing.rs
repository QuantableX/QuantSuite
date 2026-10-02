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
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Kept outside the webview: reloading a page must not forget an active job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexJobStatus {
    pub request_id: String,
    pub started_at: u64,
    pub mode: String,
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

type IndexJobs = HashMap<String, HashMap<String, IndexJobStatus>>;
fn jobs() -> &'static Mutex<IndexJobs> {
    static JOBS: OnceLock<Mutex<IndexJobs>> = OnceLock::new();
    JOBS.get_or_init(Mutex::default)
}

pub fn index_jobs(codebase: &str) -> Vec<IndexJobStatus> {
    jobs().lock().unwrap_or_else(|e| e.into_inner())
        .get(codebase).map(|modes| modes.values().cloned().collect()).unwrap_or_default()
}

struct IndexJob {
    codebase: String,
    request_id: String,
    finished: bool,
}

impl IndexJob {
    fn start(codebase: &str, mode: &str, request_id: Option<String>) -> Result<Self, String> {
        let modes: &[&str] = match mode {
            "both" => &["structural", "semantic"],
            "semantic" => &["semantic"],
            _ => &["structural"],
        };
        let request_id = request_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let started_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_millis() as u64;
        let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
        let workspace = all.entry(codebase.to_string()).or_default();
        if modes.iter().any(|mode| workspace.get(*mode).is_some_and(|job| job.status == "running")) {
            return Err("Indexing is already running for this workspace and mode. Wait for it to finish.".into());
        }
        for mode in modes {
            workspace.insert((*mode).into(), IndexJobStatus {
                request_id: request_id.clone(), started_at, mode: (*mode).into(), status: "running".into(),
                result: None, error: None,
            });
        }
        Ok(Self { codebase: codebase.into(), request_id, finished: false })
    }

    fn finish(&mut self, result: &Result<Value, String>) {
        let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(workspace) = all.get_mut(&self.codebase) {
            for job in workspace.values_mut().filter(|job| job.request_id == self.request_id) {
                job.status = if result.is_ok() { "succeeded" } else { "failed" }.into();
                job.result = result.as_ref().ok().cloned();
                job.error = result.as_ref().err().cloned();
            }
        }
        self.finished = true;
    }
}

impl Drop for IndexJob {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(&Err("Indexing was interrupted. Retry to finish the index.".into()));
        }
    }
}

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
    run_cli_with_request(app, args, None).await
}

pub async fn run_cli_with_request(
    app: &tauri::AppHandle, args: Vec<String>, request_id: Option<String>,
) -> Result<Value, String> {
    let mut job = if matches!(args.first().map(String::as_str), Some("index" | "reindex")) {
        let name = cli_option(&args, "--name").or_else(|| cli_option(&args, "--codebase"));
        if let Some(name) = name {
            let meta = index_metadata(&args, &index_dir());
            let mode = cli_option(&args, "--mode").or_else(|| meta["mode"].as_str()).unwrap_or("structural");
            Some(IndexJob::start(name, mode, request_id)?)
        } else { None }
    } else { None };
    let result = run_cli_inner(app, args).await;
    if let Some(job) = &mut job { job.finish(&result); }
    result
}

async fn run_cli_inner(app: &tauri::AppHandle, args: Vec<String>) -> Result<Value, String> {
    let cli_path = cli_dir(app)?.join("cli.py");
    if !cli_path.exists() {
        return Err(format!(
            "Codebase index CLI not found at {}.",
            cli_path.display()
        ));
    }

    let index_dir = index_dir();
    let _ = std::fs::create_dir_all(&index_dir);

    // Acquire per invocation: the port/key change on every engine start. The
    // lease stays alive until the subprocess exits, including auto-refresh.
    let meta = index_metadata(&args, &index_dir);
    let request = embedding_request(&args, &meta);
    let mut engine_error = None;
    let lease = if request != EmbeddingRequest::None {
        match qs_core::embeddings::acquire().await {
            Ok(lease) => Some(lease),
            Err(e) if request == EmbeddingRequest::Optional => {
                engine_error = Some(e);
                None
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    let mut cmd_args = vec![cli_path.to_string_lossy().to_string()];
    cmd_args.extend(args);

    let mut cmd = tokio::process::Command::new(python_exe());
    cmd.args(&cmd_args);
    cmd.env("QUANTMCP_INDEX_DIR", &index_dir);
    cmd.kill_on_drop(true);
    // Never inherit a stale endpoint, even for a structural operation.
    for key in ["QUANTMCP_EMBEDDING_ENDPOINT", "QUANTMCP_EMBEDDING_KEY", "QUANTMCP_EMBEDDING_ERROR"] {
        cmd.env_remove(key);
    }
    if let Some(lease) = &lease {
        let e = lease.endpoint();
        cmd.env("QUANTMCP_EMBEDDING_ENDPOINT", serde_json::json!({
            "url": e.url, "model": e.model, "dims": e.dims, "queryPrefix": e.query_prefix,
        }).to_string());
        cmd.env("QUANTMCP_EMBEDDING_KEY", &e.api_key);
    }
    if let Some(error) = engine_error {
        cmd.env("QUANTMCP_EMBEDDING_ERROR", error);
    }
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
    pub structural_pending_count: Option<u32>,
    pub semantic_pending_count: Option<u32>,
    pub jobs: Vec<IndexJobStatus>,
    pub stats_error: Option<String>,
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
            structural_pending_count: None,
            semantic_pending_count: None,
            jobs: Vec::new(),
            stats_error: None,
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

#[derive(Debug, PartialEq)]
enum EmbeddingRequest { None, Optional, Required }

fn cli_option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    // Positional search text may itself equal "--mode"; skip it.
    let skip = if matches!(args.first().map(String::as_str), Some("index" | "search" | "lookup")) { 2 } else { 1 };
    args.get(skip..).unwrap_or_default().windows(2)
        .find(|pair| pair[0] == name).map(|pair| pair[1].as_str())
}

fn index_metadata(args: &[String], directory: &std::path::Path) -> Value {
    let Some(name) = cli_option(args, "--codebase") else { return Value::Null };
    let Ok(db) = rusqlite::Connection::open_with_flags(
        directory.join(format!("{name}.db")), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else { return Value::Null };
    let Ok(mut statement) = db.prepare("SELECT key, value FROM meta") else { return Value::Null };
    let Ok(rows) = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) else { return Value::Null };
    let mut meta = serde_json::Map::new();
    for (key, value) in rows.flatten() { meta.insert(key, Value::String(value)); }
    Value::Object(meta)
}

/// The mode a workspace's index was built with (`structural`, `semantic` or
/// `both`), `None` before its first index run.
pub fn stored_index_mode(b36: &str) -> Option<String> {
    let args = ["stats".to_string(), "--codebase".into(), b36.into()];
    index_metadata(&args, &index_dir())["mode"].as_str().map(str::to_string)
}

fn embedding_request(args: &[String], meta: &Value) -> EmbeddingRequest {
    let mode = cli_option(args, "--mode");
    match args.first().map(String::as_str) {
        Some("index") if matches!(mode, Some("semantic" | "both")) => EmbeddingRequest::Required,
        Some("reindex") if matches!(mode.or_else(|| meta["mode"].as_str()), Some("semantic" | "both")) => EmbeddingRequest::Required,
        Some("search") if mode == Some("semantic") => EmbeddingRequest::Required,
        // Auto prefers the existing structural index, so it need not load a model.
        Some("search") if mode.unwrap_or("auto") == "auto"
            && !matches!(meta["mode"].as_str(), Some("structural" | "both"))
            && meta["mode"] == "semantic" => EmbeddingRequest::Optional,
        _ => EmbeddingRequest::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_snapshot_survives_the_caller_and_records_success() {
        let workspace = uuid::Uuid::new_v4().to_string();
        let mut job = IndexJob::start(&workspace, "semantic", Some("request-one".into())).unwrap();
        let snapshot = index_jobs(&workspace);
        assert_eq!(snapshot[0].status, "running");
        assert_eq!(snapshot[0].request_id, "request-one");
        let result = serde_json::json!({"status": "ok", "files_indexed": 12});
        job.finish(&Ok(result.clone()));
        drop(job);
        let snapshot = index_jobs(&workspace);
        assert_eq!(snapshot[0].status, "succeeded");
        assert_eq!(snapshot[0].result, Some(result));
        assert!(snapshot[0].error.is_none());
    }

    #[test]
    fn job_errors_and_cancelled_futures_are_terminal() {
        let workspace = uuid::Uuid::new_v4().to_string();
        let mut job = IndexJob::start(&workspace, "both", None).unwrap();
        job.finish(&Err("Engine unavailable".into()));
        assert!(index_jobs(&workspace).iter().all(|job| job.status == "failed"
            && job.error.as_deref() == Some("Engine unavailable")));
        let interrupted = IndexJob::start(&workspace, "semantic", None).unwrap();
        drop(interrupted);
        let snapshot = index_jobs(&workspace);
        let semantic = snapshot.iter().find(|job| job.mode == "semantic").unwrap();
        assert_eq!(semantic.status, "failed");
        assert!(semantic.error.as_ref().unwrap().contains("interrupted"));
    }

    #[test]
    fn jobs_reject_overlapping_modes_without_replacing_the_active_request() {
        let workspace = uuid::Uuid::new_v4().to_string();
        let mut semantic = IndexJob::start(&workspace, "semantic", Some("active".into())).unwrap();
        assert!(IndexJob::start(&workspace, "semantic", None).is_err());
        assert!(IndexJob::start(&workspace, "both", None).is_err());
        assert_eq!(index_jobs(&workspace)[0].request_id, "active");
        let structural = IndexJob::start(&workspace, "structural", None).unwrap();
        let independent = IndexJob::start(&uuid::Uuid::new_v4().to_string(), "both", None).unwrap();
        assert_eq!(index_jobs(&workspace).len(), 2);
        semantic.finish(&Ok(serde_json::json!({"status": "ok"})));
        let retry = IndexJob::start(&workspace, "semantic", Some("retry".into())).unwrap();
        drop(semantic);
        assert!(index_jobs(&workspace).iter().any(|job| job.request_id == "retry" && job.status == "running"));
        drop((structural, independent, retry));
    }

    #[test]
    fn only_semantic_operations_acquire_the_shared_engine() {
        let request = |args: &[&str], mode: &str| embedding_request(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &serde_json::json!({"mode": mode}));
        assert_eq!(request(&["index", "repo", "--mode", "structural"], ""), EmbeddingRequest::None);
        assert_eq!(request(&["index", "repo", "--mode", "both"], ""), EmbeddingRequest::Required);
        assert_eq!(request(&["reindex", "--codebase", "id"], "both"), EmbeddingRequest::Required);
        assert_eq!(request(&["reindex", "--codebase", "id", "--mode", "structural"], "both"), EmbeddingRequest::None);
        assert_eq!(request(&["search", "--mode", "--mode", "semantic"], "both"), EmbeddingRequest::Required);
        assert_eq!(request(&["search", "term", "--mode", "auto"], "both"), EmbeddingRequest::None);
        assert_eq!(request(&["search", "term", "--mode", "auto"], "semantic"), EmbeddingRequest::Optional);
        assert_eq!(request(&["lookup", "term", "--codebase", "id"], "semantic"), EmbeddingRequest::None);
    }
}
