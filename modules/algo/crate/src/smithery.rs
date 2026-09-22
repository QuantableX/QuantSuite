//! The Indicator Smithery inside QuantAlgo (PLAN-QUANTALGO §6): the forge's
//! vault, shelf, reports and roster through `python -m smithery.forge`, and
//! its long jobs — the gauntlet, walk-forward, the shelf refresh — as one
//! child process at a time whose JSON-lines progress the Smithery page polls
//! (`smithery_jobs`). Reports and the ledger keep landing in the vault the
//! package resolves (`smithery.data.ROOT`); nothing here writes them.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

#[cfg(windows)]
use crate::settings::hide_console_window;
use crate::settings::{build_python_command, resolve_python_path};
use crate::AppState;

/// Key of the forge in the suite's process register: one entry, Running
/// while a job runs.
pub(crate) const FORGE_PROCESS_ID: &str = "algo.smithery";

/// Jobs kept for the session (finished ones fall off the front).
const MAX_JOBS: usize = 20;
/// Structured lines kept per job; a gauntlet over the whole registry is a few
/// hundred, so the cap only ever trims a runaway child.
const MAX_EVENTS: usize = 2_000;
const MAX_LOG_LINES: usize = 2_000;
const MAX_INDICATORS_PER_JOB: usize = 64;

/// What the Smithery page asks the forge to run.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ForgeRequest {
    /// `gauntlet` | `walkforward` | `compare` | `refresh`.
    pub kind: String,
    /// Registry keys, or `all` / `certified` (gauntlet and walk-forward).
    #[serde(default)]
    pub indicators: Vec<String>,
    /// Reduced Monte Carlo counts (a smoke test, not a certification).
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub perm: Option<u32>,
    #[serde(default)]
    pub boot: Option<u32>,
    #[serde(default)]
    pub garch: Option<u32>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub folds: Option<u32>,
    /// Certification track: `all` (every track in turn — the score is earned
    /// on every track) | `1d` | `4h` | `1h` | `1m`. The forge's own default is `all`.
    #[serde(default)]
    pub timeframe: Option<String>,
}

const TRACKS: [&str; 5] = ["all", "1d", "4h", "1h", "1m"];

/// One forge run, as the page sees it. `events` are the child's JSON lines
/// in order; `summary` keeps the ones worth showing after the job is gone
/// from view (verdicts, walk-forward results, refreshed series, errors).
#[derive(Debug, Serialize, Clone)]
pub struct ForgeJob {
    pub id: String,
    pub kind: String,
    pub indicators: Vec<String>,
    pub request: ForgeRequest,
    /// `running` | `done` | `failed` | `cancelled`.
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub events: Vec<Value>,
    pub summary: Vec<Value>,
    pub log: Vec<String>,
    pub command: String,
}

struct RunningJob {
    id: String,
    child: Arc<Mutex<Child>>,
    pid: u32,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct ForgeState {
    jobs: Mutex<Vec<ForgeJob>>,
    running: Mutex<Option<RunningJob>>,
    /// `python -m smithery.forge info`, cached until a job finishes or the
    /// page asks for a refresh.
    info: Mutex<Option<Value>>,
}

// ---------------------------------------------------------------------------
// The forge as a query
// ---------------------------------------------------------------------------

fn python_for(state: &AppState) -> Result<String, String> {
    let settings = state.settings.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(resolve_python_path(&settings))
}

fn forge_command(python: &str, args: &[String]) -> Command {
    let mut command = build_python_command(python);
    command
        .arg("-m")
        .arg("smithery.forge")
        .args(args)
        // The child prints JSON on a pipe; without this a cp1252 console
        // encoding would choke on the vault's paths and the verdict glyphs.
        .env("PYTHONIOENCODING", "utf-8")
            .env("QUANTSUITE_HOME", qs_core::paths::root())
            .env("QUANTSCRIPT_INDICATORS_DIR", qs_core::paths::indicators_dir())
        .stdin(Stdio::null());
    command
}

/// Run one short forge query and return its JSON document (the last JSON
/// line on stdout). An `error` event from the forge becomes the `Err`.
fn query(state: &AppState, args: &[String]) -> Result<Value, String> {
    let python = python_for(state)?;
    let output = forge_command(&python, args)
        .output()
        .map_err(|e| format!("Could not run the forge with '{python}': {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok());
    match parsed {
        Some(doc) if doc.get("event").and_then(Value::as_str) == Some("error") => Err(doc
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the forge reported an error")
            .to_string()),
        Some(doc) if output.status.success() => Ok(doc),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            Err(format!(
                "The forge did not answer (python -m smithery.forge {}): {}",
                args.join(" "),
                if detail.is_empty() {
                    "no JSON on stdout — is numpy/pandas installed for this interpreter?"
                } else {
                    detail
                }
            ))
        }
    }
}

/// The vault, the shelf, the reports and the roster — cached per session.
pub(crate) fn load_info(state: &AppState, refresh: bool) -> Result<Value, String> {
    if !refresh {
        if let Some(cached) = state
            .smithery
            .info
            .lock()
            .map_err(|e| format!("Lock: {e}"))?
            .as_ref()
        {
            return Ok(cached.clone());
        }
    }
    let info = query(state, &["info".to_string()])?;
    *state.smithery.info.lock().map_err(|e| format!("Lock: {e}"))? = Some(info.clone());
    Ok(info)
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A report is addressed by its note name (`2026-09-07 PageTrend Gauntlet`)
/// inside the vault's Output folder — never by a path.
fn valid_report_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.contains(['/', '\\', ':'])
        && !name.contains("..")
        && !name.starts_with('.')
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

fn job_args(request: &ForgeRequest) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = Vec::new();
    match request.kind.as_str() {
        "gauntlet" | "walkforward" | "compare" => {
            if request.indicators.is_empty() {
                return Err("Pick at least one indicator.".into());
            }
            if request.kind == "compare" && request.indicators.len() < 2
                && !request.indicators.iter().any(|k| k == "all" || k == "certified")
            {
                return Err("Comparison needs at least two indicators, including a baseline.".into());
            }
            if request.indicators.len() > MAX_INDICATORS_PER_JOB {
                return Err(format!("At most {MAX_INDICATORS_PER_JOB} indicators per job."));
            }
            if let Some(bad) = request.indicators.iter().find(|k| !valid_key(k)) {
                return Err(format!("'{bad}' is not an indicator key."));
            }
            args.push(request.kind.clone());
            args.push("--indicator".into());
            args.extend(request.indicators.iter().cloned());
            if let Some(tf) = request.timeframe.as_deref().filter(|tf| !tf.is_empty()) {
                if !TRACKS.contains(&tf) || (tf == "all" && request.kind == "walkforward") {
                    return Err(format!("'{tf}' is not a certification track (all, 1d, 4h, 1h, 1m)."));
                }
                args.push("--timeframe".into());
                args.push(tf.to_string());
            }
            if request.kind == "gauntlet" {
                if request.fast {
                    args.push("--fast".into());
                }
                for (flag, value) in [
                    ("--perm", request.perm),
                    ("--boot", request.boot),
                    ("--garch", request.garch),
                ] {
                    if let Some(n) = value {
                        args.push(flag.into());
                        args.push(n.clamp(1, 5_000).to_string());
                    }
                }
                if let Some(seed) = request.seed {
                    args.push("--seed".into());
                    args.push(seed.to_string());
                }
            } else if let Some(folds) = request.folds {
                args.push("--folds".into());
                args.push(folds.clamp(2, 12).to_string());
            }
        }
        "refresh" => {
            args.push("refresh".into());
            if let Some(tf) = request.timeframe.as_deref() {
                if !TRACKS.contains(&tf) {
                    return Err(format!("'{tf}' is not a shelf timeframe."));
                }
                args.extend(["--timeframe".into(), tf.into()]);
            }
        }
        other => return Err(format!("Unknown forge job '{other}'.")),
    }
    Ok(args)
}

fn with_job<T>(state: &AppState, id: &str, f: impl FnOnce(&mut ForgeJob) -> T) -> Option<T> {
    let mut jobs = state.smithery.jobs.lock().ok()?;
    jobs.iter_mut().find(|j| j.id == id).map(f)
}

fn is_summary_event(event: &Value) -> bool {
    matches!(
        event.get("event").and_then(Value::as_str),
        Some("job" | "verdict" | "walkforward" | "comparison" | "comparison_done" | "series" | "error" | "done")
    )
}

fn push_event(state: &AppState, id: &str, event: Value) {
    with_job(state, id, |job| {
        if job.events.len() >= MAX_EVENTS {
            job.events.remove(0);
        }
        if is_summary_event(&event) {
            job.summary.push(event.clone());
        }
        job.events.push(event);
    });
}

fn push_log(state: &AppState, id: &str, line: String) {
    with_job(state, id, |job| {
        if job.log.len() >= MAX_LOG_LINES {
            job.log.remove(0);
        }
        job.log.push(line);
    });
}

/// Kill the child and, on Windows, the process pool it spawned — the workers
/// are the parent's children and would otherwise keep computing for nobody.
fn kill_tree(child: &Arc<Mutex<Child>>, pid: u32) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill.args(["/T", "/F", "/PID", &pid.to_string()]);
        hide_console_window(&mut taskkill);
        let _ = taskkill.output();
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
    }
    if let Ok(mut c) = child.lock() {
        let _ = c.kill();
    }
}

/// Stop a running job on Quit; the register is not updated — the app is
/// going away.
pub(crate) fn kill_running(state: &AppState) {
    if let Ok(mut running) = state.smithery.running.lock() {
        if let Some(job) = running.take() {
            job.cancelled.store(true, Ordering::SeqCst);
            kill_tree(&job.child, job.pid);
        }
    }
}

fn finish_job(app: &AppHandle, id: &str, code: Option<i32>, cancelled: bool) {
    let state = app.state::<AppState>();
    // The run changed what the vault holds (a report, the ledger, the shelf).
    // Reload the cached listing BEFORE the job reads as finished, so the
    // page's next poll never pairs a finished job with a stale vault.
    let _ = load_info(&state, true);
    let outcome = with_job(&state, id, |job| {
        let last_error = job
            .events
            .iter()
            .rev()
            .find(|e| e.get("event").and_then(Value::as_str) == Some("error"))
            .and_then(|e| e.get("message").and_then(Value::as_str))
            .map(str::to_string);
        let (status, error) = if cancelled {
            ("cancelled", None)
        } else if code == Some(0) {
            ("done", None)
        } else {
            (
                "failed",
                Some(last_error.unwrap_or_else(|| match code {
                    Some(c) => format!("the forge exited with code {c}"),
                    None => "the forge was killed".to_string(),
                })),
            )
        };
        job.status = status.to_string();
        job.error = error.clone();
        job.exit_code = code;
        job.finished_at = Some(Utc::now().to_rfc3339());
        (status.to_string(), error)
    });

    if let Ok(mut running) = state.smithery.running.lock() {
        if running.as_ref().is_some_and(|r| r.id == id) {
            *running = None;
        }
    }
    match outcome {
        Some((status, Some(message))) if status == "failed" => {
            qs_core::processes::mark_failed(app, FORGE_PROCESS_ID, message);
        }
        _ => qs_core::processes::mark_stopped(app, FORGE_PROCESS_ID),
    }
}

fn start_job(app: &AppHandle, request: ForgeRequest) -> Result<ForgeJob, String> {
    let state = app.state::<AppState>();
    let args = job_args(&request)?;
    let python = python_for(&state)?;

    let mut running = state.smithery.running.lock().map_err(|e| format!("Lock: {e}"))?;
    if running.is_some() {
        return Err("A forge job is already running — wait for it to finish or cancel it.".into());
    }

    let mut command = forge_command(&python, &args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start the forge with '{python}': {e}"))?;
    let stdout = child.stdout.take().ok_or("forge stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("forge stderr unavailable")?;
    let pid = child.id();
    let child = Arc::new(Mutex::new(child));
    let cancelled = Arc::new(AtomicBool::new(false));

    let job = ForgeJob {
        id: Uuid::new_v4().to_string(),
        kind: request.kind.clone(),
        indicators: request.indicators.clone(),
        request,
        status: "running".into(),
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        exit_code: None,
        error: None,
        events: Vec::new(),
        summary: Vec::new(),
        log: Vec::new(),
        command: format!("{python} -m smithery.forge {}", args.join(" ")),
    };
    let id = job.id.clone();
    {
        let mut jobs = state.smithery.jobs.lock().map_err(|e| format!("Lock: {e}"))?;
        while jobs.len() >= MAX_JOBS {
            match jobs.iter().position(|j| j.status != "running") {
                Some(i) => {
                    jobs.remove(i);
                }
                None => break,
            }
        }
        jobs.push(job.clone());
    }
    *running = Some(RunningJob {
        id: id.clone(),
        child: Arc::clone(&child),
        pid,
        cancelled: Arc::clone(&cancelled),
    });
    drop(running);
    qs_core::processes::mark_running(app, FORGE_PROCESS_ID, Some(pid));

    // stderr: the forge's warnings and tracebacks, kept as log lines.
    {
        let app = app.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let state = app.state::<AppState>();
                push_log(&state, &id, format!("! {line}"));
            }
        });
    }
    // stdout: JSON lines are events, everything else is log. Reading to EOF
    // happens without the child lock, so a cancel can take it to kill.
    {
        let app = app.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let state = app.state::<AppState>();
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(event) if event.get("event").is_some() => push_event(&state, &id, event),
                    _ => push_log(&state, &id, trimmed.to_string()),
                }
            }
            let code = match child.lock() {
                Ok(mut c) => c.wait().ok().and_then(|status| status.code()),
                Err(_) => None,
            };
            finish_job(&app, &id, code, cancelled.load(Ordering::SeqCst));
        });
    }

    Ok(job)
}

fn snapshot(job: &ForgeJob, detail: bool) -> ForgeJob {
    if detail {
        return job.clone();
    }
    ForgeJob {
        events: Vec::new(),
        log: Vec::new(),
        ..job.clone()
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The vault, shelf, reports and roster (`python -m smithery.forge info`).
#[tauri::command]
pub(crate) async fn smithery_info(refresh: Option<bool>, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        load_info(&state, refresh.unwrap_or(false))
    })
    .await
    .map_err(|e| format!("Forge task failed: {e}"))?
}

/// One gauntlet report from the vault's Output folder, as markdown.
#[tauri::command]
pub(crate) async fn smithery_report(name: String, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !valid_report_name(&name) {
            return Err("That is not a report name.".to_string());
        }
        let state = app_handle.state::<AppState>();
        let info = load_info(&state, false)?;
        let output_dir = info
            .get("output_dir")
            .and_then(Value::as_str)
            .ok_or("The forge did not report its Output folder.")?;
        let path = PathBuf::from(output_dir).join(format!("{name}.md"));
        let markdown = std::fs::read_to_string(&path)
            .map_err(|e| format!("Could not read the report '{name}': {e}"))?;
        Ok(serde_json::json!({
            "name": name,
            "file": path.to_string_lossy(),
            "markdown": markdown,
        }))
    })
    .await
    .map_err(|e| format!("Report task failed: {e}"))?
}

/// An indicator's module source, read-only.
#[tauri::command]
pub(crate) async fn smithery_source(indicator: String, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !valid_key(&indicator) {
            return Err("That is not an indicator key.".to_string());
        }
        let state = app_handle.state::<AppState>();
        query(&state, &["source".to_string(), "--indicator".to_string(), indicator])
    })
    .await
    .map_err(|e| format!("Source task failed: {e}"))?
}

/// Start a forge job — one at a time.
#[tauri::command]
pub(crate) async fn smithery_run(request: ForgeRequest, app_handle: AppHandle) -> Result<ForgeJob, String> {
    tauri::async_runtime::spawn_blocking(move || start_job(&app_handle, request))
        .await
        .map_err(|e| format!("Forge task failed: {e}"))?
}

/// Stop the running job. The job finishes as `cancelled` once the child is
/// gone; the page sees that on its next poll.
#[tauri::command]
pub(crate) async fn smithery_cancel(app_handle: AppHandle) -> Result<Option<ForgeJob>, String> {
    let state = app_handle.state::<AppState>();
    let running = {
        let mut running = state.smithery.running.lock().map_err(|e| format!("Lock: {e}"))?;
        running.take()
    };
    let Some(running) = running else {
        return Ok(None);
    };
    running.cancelled.store(true, Ordering::SeqCst);
    kill_tree(&running.child, running.pid);
    let jobs = state.smithery.jobs.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(jobs.iter().find(|j| j.id == running.id).map(|j| snapshot(j, true)))
}

/// The session's jobs, oldest first. Events and log lines come only with
/// the running job and the one named by `detail`; the rest carry their
/// summary.
#[tauri::command]
pub(crate) async fn smithery_jobs(detail: Option<String>, app_handle: AppHandle) -> Result<Vec<ForgeJob>, String> {
    let state = app_handle.state::<AppState>();
    let jobs = state.smithery.jobs.lock().map_err(|e| format!("Lock: {e}"))?;
    Ok(jobs
        .iter()
        .map(|j| snapshot(j, j.status == "running" || detail.as_deref() == Some(j.id.as_str())))
        .collect())
}

/// `{ limit }` → the newest job's log tail, for the suite's process page.
#[tauri::command]
pub(crate) async fn smithery_process_logs(limit: Option<usize>, app_handle: AppHandle) -> Result<Vec<String>, String> {
    let state = app_handle.state::<AppState>();
    let jobs = state.smithery.jobs.lock().map_err(|e| format!("Lock: {e}"))?;
    let limit = limit.unwrap_or(200).max(1);
    let Some(job) = jobs.iter().rev().find(|j| j.status == "running").or_else(|| jobs.last()) else {
        return Ok(Vec::new());
    };
    let mut lines: Vec<String> = job
        .events
        .iter()
        .filter_map(|e| {
            let kind = e.get("event").and_then(Value::as_str)?;
            let indicator = e.get("indicator").and_then(Value::as_str).unwrap_or("");
            Some(match kind {
                "axis" => format!(
                    "{indicator} axis {} {}{}",
                    e.get("axis").and_then(Value::as_str).unwrap_or(""),
                    e.get("status").and_then(Value::as_str).unwrap_or(""),
                    e.get("score").and_then(Value::as_f64).map(|s| format!(" {s:.0}")).unwrap_or_default()
                ),
                "verdict" => format!(
                    "{indicator} score {} grade {} perm p {}",
                    e.get("score").and_then(Value::as_f64).unwrap_or(0.0),
                    e.get("grade").and_then(Value::as_str).unwrap_or(""),
                    e.get("perm_p").and_then(Value::as_f64).unwrap_or(f64::NAN)
                ),
                "error" => format!("{indicator} error: {}", e.get("message").and_then(Value::as_str).unwrap_or("")),
                "log" => e.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
                other => format!("{indicator} {other}"),
            })
        })
        .chain(job.log.iter().cloned())
        .collect();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauntlet_args_carry_every_option() {
        let args = job_args(&ForgeRequest {
            kind: "gauntlet".into(),
            indicators: vec!["hilbert".into(), "dc".into()],
            fast: true,
            perm: Some(50),
            boot: Some(9_999),
            garch: Some(10),
            seed: Some(7),
            folds: Some(3),
            timeframe: Some("4h".into()),
        })
        .unwrap();
        assert_eq!(
            args,
            vec![
                "gauntlet", "--indicator", "hilbert", "dc", "--timeframe", "4h", "--fast", "--perm", "50", "--boot",
                "5000", "--garch", "10", "--seed", "7"
            ]
        );
    }

    #[test]
    fn an_unsupported_track_is_refused() {
        assert!(job_args(&ForgeRequest {
            kind: "gauntlet".into(),
            indicators: vec!["dc".into()],
            timeframe: Some("15m".into()),
            ..Default::default()
        })
        .is_err());
        let args = job_args(&ForgeRequest {
            kind: "walkforward".into(),
            indicators: vec!["dc".into()],
            timeframe: Some("1h".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(args, vec!["walkforward", "--indicator", "dc", "--timeframe", "1h"]);
        // every track at once is a gauntlet thing; a walk-forward re-selects on one series
        assert!(job_args(&ForgeRequest {
            kind: "walkforward".into(),
            indicators: vec!["dc".into()],
            timeframe: Some("all".into()),
            ..Default::default()
        })
        .is_err());
        let args = job_args(&ForgeRequest {
            kind: "gauntlet".into(),
            indicators: vec!["dc".into()],
            timeframe: Some("all".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(args, vec!["gauntlet", "--indicator", "dc", "--timeframe", "all"]);
    }

    #[test]
    fn walkforward_ignores_monte_carlo_options() {
        let args = job_args(&ForgeRequest {
            kind: "walkforward".into(),
            indicators: vec!["mk".into()],
            perm: Some(50),
            folds: Some(1),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(args, vec!["walkforward", "--indicator", "mk", "--folds", "2"]);
    }

    #[test]
    fn refresh_takes_no_indicators_and_bad_input_is_refused() {
        assert_eq!(
            job_args(&ForgeRequest { kind: "refresh".into(), ..Default::default() }).unwrap(),
            vec!["refresh"]
        );
        assert!(job_args(&ForgeRequest { kind: "gauntlet".into(), ..Default::default() }).is_err());
        assert!(job_args(&ForgeRequest {
            kind: "gauntlet".into(),
            indicators: vec!["../evil".into()],
            ..Default::default()
        })
        .is_err());
        assert!(job_args(&ForgeRequest { kind: "format".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn report_names_stay_inside_the_output_folder() {
        assert!(valid_report_name("2026-09-07 PageTrend Gauntlet"));
        assert!(!valid_report_name("../Docs/04 Indicator Registry"));
        assert!(!valid_report_name("C:\\Windows\\win.ini"));
        assert!(!valid_report_name(""));
    }

    #[test]
    fn minute_track_routes_all_jobs_and_refresh_rejects_invalid_tracks() {
        for kind in ["gauntlet", "walkforward", "compare", "refresh"] {
            let args = job_args(&ForgeRequest {
                kind: kind.into(), indicators: vec!["robust".into(), "extremes".into()],
                timeframe: Some("1m".into()), ..Default::default()
            }).unwrap();
            assert!(args.windows(2).any(|pair| pair == ["--timeframe", "1m"]));
        }
        assert!(job_args(&ForgeRequest {
            kind: "refresh".into(), timeframe: Some("1M".into()), ..Default::default()
        }).is_err());
    }

    #[test]
    fn comparison_routes_tracks_and_folds_but_requires_a_baseline() {
        assert!(job_args(&ForgeRequest {
            kind: "compare".into(), indicators: vec!["robust".into()], ..Default::default()
        }).is_err());
        let args = job_args(&ForgeRequest {
            kind: "compare".into(), indicators: vec!["robust".into(), "scale_original".into()],
            timeframe: Some("all".into()), folds: Some(3), fast: true, perm: Some(50),
            ..Default::default()
        }).unwrap();
        assert_eq!(args, vec!["compare", "--indicator", "robust", "scale_original", "--timeframe", "all", "--folds", "3"]);
    }
}
