//! QuantSystems Tauri backend.
//!
//! Owns three things:
//!   1. App settings + per-system run-config persistence (`~/.quantsystems/config.json`).
//!   2. The Python **engine** process lifecycle — spawns `python -m rotation_lab`
//!      and talks to it over newline-delimited JSON-RPC on stdin/stdout. The
//!      process stays this module's: its stdin/stdout carry that protocol, so
//!      nothing outside can own it. The suite only gets told about it, through
//!      `qs_core::processes` (see [`ENGINE_PROCESS_ID`]).
//!   3. Thin command proxies the Nuxt frontend calls via `invoke()`.
//!
//! The heavy quantitative work (universe reconstruction, OHLCV, backtests,
//! metrics) lives in the reused `rotation_lab` Python package. Rust just
//! manages the process and routes requests/notifications.

use chrono::{Datelike, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, State, Wry,
};

// ─── Persisted types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub font_size: i64,
    pub active_system_id: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_size: 14,
            active_system_id: "lces".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMeta {
    pub id: String,
    pub name: String,
    pub short: String,
    pub status: String, // "ready" | "planned"
    pub description: String,
}

fn systems_catalog() -> Vec<SystemMeta> {
    vec![
        SystemMeta {
            id: "lces".into(),
            name: "Large-Cap Evaluation System".into(),
            short: "LCES".into(),
            status: "ready".into(),
            description: "Survivorship-bias-free rotation across the top-N large-cap coins, ranked as they stood on each date.".into(),
        },
        SystemMeta {
            id: "sces".into(),
            name: "Small-Cap Evaluation System".into(),
            short: "SCES".into(),
            status: "ready".into(),
            description: "Same engine, applied to a lower-rank small-cap cohort: excludes the top-ranked coins and rotates the slice beneath them.".into(),
        },
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedStore {
    #[serde(default)]
    settings: Option<AppSettings>,
    #[serde(default, rename = "systemConfigs")]
    system_configs: HashMap<String, Value>,
}

/// Default run-config (camelCase — matches the frontend `RunConfig` shape and
/// the keys `rotation_lab.rpc` understands).
///
/// Per-system defaults: both systems default to a `topN` of 100. LCES draws
/// the top of the ranking (no exclusion); SCES excludes the top-ranked coins
/// and rotates the slice beneath them (`excludeTopN` 5 → ranks 6..100).
fn default_run_config(system_id: &str) -> Value {
    let today = Utc::now().date_naive();
    let start = today - ChronoDuration::days(365 * 3);
    let (top_n, exclude_top_n) = match system_id {
        "sces" => (100, 5),
        _ => (100, 0),
    };
    json!({
        "topN": top_n,
        "excludeTopN": exclude_top_n,
        "cadence": "daily",
        "startDate": format!("{:04}-{:02}-{:02}", start.year(), start.month(), start.day()),
        "endDate": format!("{:04}-{:02}-{:02}", today.year(), today.month(), today.day()),
        "rankingSource": "auto",
        "excludeStablecoins": true,
        "excludeWrapped": true,
        "includeUsd": true,
        "feeRate": 0.001,
        "slippageRate": 0.0,
        "minRequestInterval": 1.2,
        "indicator": { "trend": "ema_cross", "emaCross": { "src": "close", "fastLength": 12, "slowLength": 21 } }
    })
}

// ─── Store state ────────────────────────────────────────────────────────────

struct Store {
    data_dir: PathBuf,
    settings: Mutex<AppSettings>,
    system_configs: Mutex<HashMap<String, Value>>,
}

/// `~/.quantsuite/modules/systems/`. Was `~/.quantsystems` in the standalone
/// app; [`import_legacy_data`] copies that across on first launch.
fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("systems")
}

fn config_path(base: &Path) -> PathBuf {
    base.join("config.json")
}

fn load_store(base: &Path) -> (AppSettings, HashMap<String, Value>) {
    let path = config_path(base);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(parsed) = serde_json::from_str::<PersistedStore>(&raw) {
            return (
                parsed.settings.unwrap_or_default(),
                parsed.system_configs,
            );
        }
    }
    (AppSettings::default(), HashMap::new())
}

fn save_store(store: &Store) -> Result<(), String> {
    let settings = store
        .settings
        .lock()
        .map_err(|e| format!("Lock settings: {e}"))?
        .clone();
    let configs = store
        .system_configs
        .lock()
        .map_err(|e| format!("Lock configs: {e}"))?
        .clone();
    let persisted = PersistedStore {
        settings: Some(settings),
        system_configs: configs,
    };
    let raw = serde_json::to_string_pretty(&persisted).map_err(|e| format!("Serialize store: {e}"))?;
    fs::create_dir_all(&store.data_dir).map_err(|e| format!("Create data dir: {e}"))?;
    qs_core::paths::write_atomic(&config_path(&store.data_dir), raw.as_bytes()).map_err(|e| format!("Write config: {e}"))
}

// ─── Engine process manager ─────────────────────────────────────────────────

/// Key this module's engine is announced under in the suite's process register
/// (`qs_core::processes`). Module-prefixed and stable — the process page and
/// the register both address the engine by exactly this string.
///
/// The register holds no handle to the engine and cannot see its state: every
/// transition below is *reported*. That is why `mark_stopped` also lives in
/// [`reader_loop`], where an engine that dies on its own is noticed — a crash
/// nobody reports leaves the page claiming "running" forever.
const ENGINE_PROCESS_ID: &str = "systems.engine";

/// How long the engine may sit with no requests before its watchdog stops it.
/// The engine's cache is sqlite on disk (`ROTATION_LAB_CACHE`), so nothing
/// warm is lost — the next request pays only interpreter startup, and an idle
/// Python process stops occupying memory in the tray-resident suite.
const ENGINE_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Watchdog check interval. Coarse on purpose: the timeout is minutes, so the
/// only cost of a big interval is stopping up to this much later than exactly
/// `ENGINE_IDLE_TIMEOUT` after the last request.
const ENGINE_IDLE_POLL: Duration = Duration::from_secs(30);

struct ChildHandle {
    child: Child,
    stdin: ChildStdin,
}

struct EngineInner {
    app: AppHandle,
    /// The interpreter, resolved on the first start (`resolve_python` probes
    /// candidates by spawning them, which is why it is not done at setup).
    python: Mutex<Option<PythonCommand>>,
    engine_dir: PathBuf,
    cache_path: PathBuf,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Sender<Value>>>,
    proc: Mutex<Option<ChildHandle>>,
    /// When the engine last did something on a caller's behalf. Touched by
    /// every request; read only by the idle watchdog.
    last_used: Mutex<Instant>,
    /// Bumped by every `start()`. A watchdog captures the value it was born
    /// under and exits when it no longer matches, so a stop/start cycle never
    /// leaves two watchdogs racing over the same engine.
    generation: AtomicU64,
}

#[derive(Clone)]
struct EngineManager(Arc<EngineInner>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineStatus {
    status: String, // "running" | "stopped"
    pid: Option<u32>,
}

impl EngineManager {
    fn new(app: AppHandle, engine_dir: PathBuf, cache_path: PathBuf) -> Self {
        EngineManager(Arc::new(EngineInner {
            app,
            python: Mutex::new(None),
            engine_dir,
            cache_path,
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            proc: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
            generation: AtomicU64::new(0),
        }))
    }

    /// Reset the idle clock. Called wherever a caller demonstrates the engine
    /// is still wanted; the watchdog only ever compares against this.
    fn touch(&self) {
        if let Ok(mut t) = self.0.last_used.lock() {
            *t = Instant::now();
        }
    }

    fn is_running(&self) -> bool {
        self.0
            .proc
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    fn status(&self) -> EngineStatus {
        let pid = self
            .0
            .proc
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|h| h.child.id()));
        EngineStatus {
            status: if pid.is_some() { "running".into() } else { "stopped".into() },
            pid,
        }
    }

    /// The interpreter, resolved once. A probe spawns Python, so this runs on
    /// the first `start()` — an async command, off the main thread — never
    /// in `setup`.
    fn python(&self) -> PythonCommand {
        let mut slot = self.0.python.lock().unwrap_or_else(|e| e.into_inner());
        slot.get_or_insert_with(resolve_python).clone()
    }

    fn start(&self) -> Result<EngineStatus, String> {
        let mut guard = self.0.proc.lock().map_err(|e| format!("Lock proc: {e}"))?;
        if guard.is_some() {
            return Ok(self.status_locked(&guard));
        }

        let python = self.python();
        // `command()` also hides the console window: the release binary is a
        // GUI-subsystem process with no console, so a console child would
        // otherwise get a brand-new visible window for the engine's whole
        // lifetime (dev builds keep a console the child inherits).
        let mut cmd = python.command();
        cmd.arg("-m")
            .arg("rotation_lab")
            .current_dir(&self.0.engine_dir)
            .env("ROTATION_LAB_CACHE", &self.0.cache_path)
            .env("QUANTSUITE_HOME", qs_core::paths::root())
            .env("QUANTSCRIPT_INDICATORS_DIR", qs_core::paths::indicators_dir())
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                // Reported, not just returned: the caller may be the process
                // page, which shows the register's entry rather than this
                // Result — a silent failure would read as "still stopped".
                let message = format!(
                    "Failed to spawn engine ({} in {}): {e}",
                    python.display(),
                    self.0.engine_dir.display()
                );
                qs_core::processes::mark_failed(&self.0.app, ENGINE_PROCESS_ID, message.clone());
                return Err(message);
            }
        };

        let stdout = child.stdout.take().ok_or("No engine stdout")?;
        let stderr = child.stderr.take();
        let stdin = child.stdin.take().ok_or("No engine stdin")?;
        let pid = child.id();
        let my_generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;

        // Reader thread: route responses to waiters, notifications to events.
        let inner = self.0.clone();
        thread::spawn(move || reader_loop(inner, stdout, my_generation));

        // Drain stderr so the engine never blocks on a full pipe.
        //
        // This used to emit `eval:note` once per line. Nothing has ever
        // listened for that event — so every warning the Python engine printed
        // bought an `EvaluateScript` hop on the main thread, per line, per
        // webview, and then went nowhere. The lines are worth keeping, so they
        // go to the suite log instead, where they can actually be read
        // (`~/.quantsuite/logs/quantsuite.log`).
        if let Some(stderr) = stderr {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    log::warn!(target: "systems::engine", "{line}");
                }
            });
        }

        *guard = Some(ChildHandle { child, stdin });
        self.touch();

        // Idle watchdog: stop the engine once nothing has needed it for
        // `ENGINE_IDLE_TIMEOUT`. In-flight work is protected twice over — a
        // running request keeps an entry in `pending`, and its completion
        // touches the idle clock. The kill happens under the proc lock, the
        // same lock `request()` holds to write, so the watchdog can never
        // shoot an engine a caller is mid-conversation with.
        let inner = self.0.clone();
        thread::spawn(move || loop {
            thread::sleep(ENGINE_IDLE_POLL);
            if inner.generation.load(Ordering::SeqCst) != my_generation {
                return; // a newer engine has its own watchdog
            }
            let Ok(mut guard) = inner.proc.lock() else { return };
            if inner.generation.load(Ordering::SeqCst) != my_generation {
                return;
            }
            if guard.is_none() {
                return; // engine already stopped or crashed; reader cleaned up
            }
            let busy = inner.pending.lock().map(|p| !p.is_empty()).unwrap_or(true);
            let idle_for = inner
                .last_used
                .lock()
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
            if busy || idle_for < ENGINE_IDLE_TIMEOUT {
                continue;
            }
            if let Some(mut handle) = guard.take() {
                let _ = handle.child.kill();
                let _ = handle.child.wait();
            }
            // The reader thread notices the closed stdout and repeats this
            // cleanup; both paths are idempotent.
            qs_core::processes::mark_stopped(&inner.app, ENGINE_PROCESS_ID);
            let _ = inner.app.emit("engine:status", json!({ "status": "stopped" }));
            return;
        });

        // The suite's process page reads the register, not this event.
        qs_core::processes::mark_running(&self.0.app, ENGINE_PROCESS_ID, Some(pid));
        let _ = self
            .0
            .app
            .emit("engine:status", json!({ "status": "running", "pid": pid }));
        Ok(EngineStatus { status: "running".into(), pid: Some(pid) })
    }

    fn status_locked(&self, guard: &Option<ChildHandle>) -> EngineStatus {
        let pid = guard.as_ref().map(|h| h.child.id());
        EngineStatus {
            status: if pid.is_some() { "running".into() } else { "stopped".into() },
            pid,
        }
    }

    fn stop(&self) -> Result<EngineStatus, String> {
        self.stop_generation(None)
    }

    /// A timed-out request may stop only the process that received it. A
    /// concurrent manual restart must never kill the replacement engine.
    fn stop_generation(&self, expected: Option<u64>) -> Result<EngineStatus, String> {
        let mut guard = self.0.proc.lock().map_err(|e| format!("Lock proc: {e}"))?;
        if expected.is_some_and(|generation| self.0.generation.load(Ordering::SeqCst) != generation) {
            return Ok(self.status_locked(&guard));
        }
        if let Some(mut handle) = guard.take() {
            let _ = handle.child.kill();
            let _ = handle.child.wait();
        }
        // Fail any in-flight requests so callers don't hang.
        if let Ok(mut pending) = self.0.pending.lock() {
            pending.clear();
        }
        qs_core::processes::mark_stopped(&self.0.app, ENGINE_PROCESS_ID);
        let _ = self.0.app.emit("engine:status", json!({ "status": "stopped" }));
        Ok(EngineStatus { status: "stopped".into(), pid: None })
    }

    fn ensure_started(&self) -> Result<(), String> {
        if !self.is_running() {
            self.start()?;
        }
        Ok(())
    }

    /// Send a request and block (on a worker thread) until the matching response.
    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.ensure_started()?;
        self.touch();
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<Value>();
        let line = json!({ "id": id, "method": method, "params": params }).to_string() + "\n";
        // Register and dispatch under the same process lock used by stop/start.
        // Otherwise an old reader can clear a new process's pending request.
        let generation = {
            let mut guard = self.0.proc.lock().map_err(|e| format!("Lock proc: {e}"))?;
            let handle = guard.as_mut().ok_or("Engine is not running")?;
            self.0.pending.lock().map_err(|e| format!("Lock pending: {e}"))?.insert(id, tx);
            if let Err(e) = handle.stdin.write_all(line.as_bytes()).and_then(|()| handle.stdin.flush()) {
                if let Ok(mut pending) = self.0.pending.lock() {
                    pending.remove(&id);
                }
                return Err(format!("Write to engine: {e}"));
            }
            self.0.generation.load(Ordering::SeqCst)
        };

        let resp = rx.recv_timeout(timeout);
        // Completion restarts the idle clock either way: a caller who just got
        // an answer (or a timeout) is the strongest signal the engine is in
        // active use right now.
        self.touch();
        let resp = match resp {
            Ok(resp) => resp,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Python processes RPC synchronously. Merely forgetting the
                // waiter leaves the old calculation blocking every later job.
                self.stop_generation(Some(generation))?;
                return Err(format!(
                    "Engine {method} request timed out after {} seconds; the engine was stopped. Retry the evaluation.",
                    timeout.as_secs()
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Engine stopped before the evaluation completed.".to_string());
            }
        };

        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }
}

fn reader_loop(inner: Arc<EngineInner>, stdout: std::process::ChildStdout, generation: u64) {
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Ok(guard) = inner.proc.lock() else { return };
        if guard.is_none() || inner.generation.load(Ordering::SeqCst) != generation {
            return;
        }

        // Response (has a numeric id) → deliver to the waiting request.
        if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
            if let Ok(mut pending) = inner.pending.lock() {
                if let Some(tx) = pending.remove(&id) {
                    let _ = tx.send(value);
                }
            }
            continue;
        }

        // Notification (has a method, no id) → map to a Tauri event.
        if let Some(method) = value.get("method").and_then(|v| v.as_str()) {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            match method {
                "progress" => {
                    let _ = inner.app.emit("eval:progress", params);
                }
                "ready" => {
                    let _ = inner.app.emit("engine:status", json!({ "status": "running" }));
                }
                other => {
                    let _ = inner.app.emit(&format!("engine:{other}"), params);
                }
            }
        }
    }

    // stdout closed → the engine exited. Mark stopped and fail pending requests.
    //
    // This is also the only place a *crash* is noticed. The process register
    // believes what modules report, so an engine that dies without anyone
    // calling `mark_stopped` would leave the suite's process page showing
    // "running" indefinitely. The owner already learns of the exit right here,
    // so this is where the register is told.
    let Ok(mut guard) = inner.proc.lock() else { return };
    if guard.is_none() || inner.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    if let Some(mut handle) = guard.take() {
        let _ = handle.child.kill();
        let _ = handle.child.wait();
    }
    if let Ok(mut pending) = inner.pending.lock() {
        pending.clear();
    }
    qs_core::processes::mark_stopped(&inner.app, ENGINE_PROCESS_ID);
    let _ = inner.app.emit("engine:status", json!({ "status": "stopped" }));
}

// ─── Engine path resolution ─────────────────────────────────────────────────

/// The interpreter the engine runs on: a program plus the arguments that
/// belong to it — the Windows launcher is `py -3`, not `py`.
#[derive(Clone, Debug)]
struct PythonCommand {
    program: String,
    args: Vec<String>,
}

impl PythonCommand {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    /// A `Command` for this interpreter, console window hidden (CREATE_NO_WINDOW).
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd
    }

    fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Probe dependencies and resolve the actual interpreter, including when the
/// candidate is the Windows `py` launcher. The managed child must be Python
/// itself: killing only the launcher leaves its worker and stdout pipe alive.
fn engine_python(python: &PythonCommand) -> Option<PythonCommand> {
    let output = python
        .command()
        .args(["-c", "import numpy, pandas, sys, json; print(json.dumps(sys.executable))"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let executable: String = serde_json::from_slice(&output.stdout).ok()?;
    if executable.trim().is_empty() {
        return None;
    }
    Some(PythonCommand::new(&executable, &[]))
}

/// `QUANTSYSTEMS_PYTHON` when set; else the first candidate that has the
/// engine's dependencies. A bare `python` on PATH is whatever venv happens to
/// be active — on the operator's machine one without pandas, which is how the
/// engine came to fail to start on 2026-09-08 — so on Windows the launcher
/// `py -3` and `python3` are tried before it. Called lazily, on the first
/// start, because the probe spawns Python.
fn resolve_python() -> PythonCommand {
    if let Ok(p) = std::env::var("QUANTSYSTEMS_PYTHON") {
        if !p.trim().is_empty() {
            let candidate = PythonCommand::new(p.trim(), &[]);
            return engine_python(&candidate).unwrap_or(candidate);
        }
    }
    let candidates: Vec<PythonCommand> = if cfg!(windows) {
        vec![
            PythonCommand::new("py", &["-3"]),
            PythonCommand::new("python3", &[]),
            PythonCommand::new("python", &[]),
        ]
    } else {
        vec![PythonCommand::new("python3", &[]), PythonCommand::new("python", &[])]
    };
    if let Some(found) = candidates.iter().find_map(engine_python) {
        log::info!(target: "systems", "engine interpreter: {}", found.display());
        return found;
    }
    // Nothing on this machine can import numpy and pandas: keep the last
    // candidate so the spawn error names a real interpreter and the operator
    // installs the requirements there.
    log::warn!(target: "systems", "no interpreter with numpy and pandas found (py -3, python3, python)");
    candidates
        .last()
        .cloned()
        .unwrap_or_else(|| PythonCommand::new("python", &[]))
}

fn resolve_engine_dir(app: &AppHandle) -> PathBuf {
    if let Ok(dir) = std::env::var("QUANTSYSTEMS_ENGINE_DIR") {
        let p = PathBuf::from(dir);
        if p.join("rotation_lab").exists() {
            return p;
        }
    }

    // Repo checkout AND installed bundle both resolve through qs-core, which
    // also checks the Tauri resource dir — the installed suite ships the
    // engine as a bundled resource, not as files beside the exe.
    if let Some(dir) = qs_core::paths::python_sidecar_dir(app, "rotation_lab") {
        return dir;
    }

    // Standalone checkout of the pre-suite layout: `engine/` beside the app.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("engine"));
        candidates.push(cwd.join("..").join("engine"));
        candidates.push(cwd.join("../..").join("engine"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("engine"));
            candidates.push(dir.join("..").join("engine"));
            candidates.push(dir.join("../..").join("engine"));
            candidates.push(dir.join("../../..").join("engine"));
        }
    }
    for c in candidates {
        if c.join("rotation_lab").exists() {
            return c;
        }
    }
    // Fall back to ./engine; start() will report a clear error if missing.
    PathBuf::from("engine")
}

// ─── Settings & systems commands ────────────────────────────────────────────

#[tauri::command(async)]
fn get_app_settings(store: State<'_, Store>) -> Result<AppSettings, String> {
    store
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| format!("Lock settings: {e}"))
}

#[tauri::command]
async fn update_app_settings(settings: AppSettings, store: State<'_, Store>) -> Result<AppSettings, String> {
    {
        let mut current = store
            .settings
            .lock()
            .map_err(|e| format!("Lock settings: {e}"))?;
        *current = settings;
    }
    save_store(&store)?;
    get_app_settings(store)
}

#[tauri::command(async)]
fn list_systems() -> Vec<SystemMeta> {
    systems_catalog()
}

#[tauri::command(async)]
fn get_system_config(system_id: String, store: State<'_, Store>) -> Result<Value, String> {
    let configs = store
        .system_configs
        .lock()
        .map_err(|e| format!("Lock configs: {e}"))?;
    Ok(configs
        .get(&system_id)
        .cloned()
        .unwrap_or_else(|| default_run_config(&system_id)))
}

#[tauri::command]
async fn save_system_config(
    system_id: String,
    config: Value,
    store: State<'_, Store>,
) -> Result<Value, String> {
    {
        let mut configs = store
            .system_configs
            .lock()
            .map_err(|e| format!("Lock configs: {e}"))?;
        configs.insert(system_id.clone(), config.clone());
    }
    save_store(&store)?;
    Ok(config)
}

// ─── Engine lifecycle commands ──────────────────────────────────────────────

#[tauri::command]
async fn start_engine(engine: State<'_, EngineManager>) -> Result<EngineStatus, String> {
    let mgr = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.start())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_engine(engine: State<'_, EngineManager>) -> Result<EngineStatus, String> {
    let mgr = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.stop())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn engine_status(engine: State<'_, EngineManager>) -> EngineStatus {
    engine.status()
}

// ─── Evaluation commands (proxy to the Python engine) ───────────────────────

const LIVE_TIMEOUT: Duration = Duration::from_secs(600);
const BACKTEST_TIMEOUT: Duration = Duration::from_secs(1800);
const QUICK_TIMEOUT: Duration = Duration::from_secs(120);

#[tauri::command]
async fn live_eval(
    system_id: String,
    config: Value,
    engine: State<'_, EngineManager>,
) -> Result<Value, String> {
    let mgr = engine.inner().clone();
    let _ = system_id; // LCES today; SCES (planned) reuses the same engine path.
    tauri::async_runtime::spawn_blocking(move || {
        mgr.request("live", json!({ "config": config }), LIVE_TIMEOUT)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn run_backtest(
    system_id: String,
    config: Value,
    engine: State<'_, EngineManager>,
    app: AppHandle,
) -> Result<Value, String> {
    let mgr = engine.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        mgr.request("backtest", json!({ "config": config }), BACKTEST_TIMEOUT)
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("backtest:complete", json!({ "systemId": system_id }));
    Ok(result)
}

#[tauri::command]
async fn browse_universe(
    system_id: String,
    on_date: String,
    config: Value,
    engine: State<'_, EngineManager>,
) -> Result<Value, String> {
    let mgr = engine.inner().clone();
    let _ = system_id;
    tauri::async_runtime::spawn_blocking(move || {
        mgr.request(
            "universe",
            json!({ "onDate": on_date, "config": config }),
            LIVE_TIMEOUT,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cache_stats(engine: State<'_, EngineManager>) -> Result<Value, String> {
    let mgr = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        mgr.request("cache_stats", json!({}), QUICK_TIMEOUT)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clear_cache(
    scope: String,
    engine: State<'_, EngineManager>,
    app: AppHandle,
) -> Result<Value, String> {
    let mgr = engine.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        mgr.request("clear_cache", json!({ "scope": scope }), QUICK_TIMEOUT)
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("cache:updated", json!({}));
    Ok(result)
}

// ─── Screenshot command ─────────────────────────────────────────────────────

/// Capture the application window and place the PNG on the system clipboard.
///
/// We capture the whole monitor (the DWM-composited desktop, which correctly
/// includes GPU-accelerated WebView2 content) and crop to the window rect.
/// Capturing the WebView2 window directly via PrintWindow tends to yield black
/// frames. The window has `decorations: false`, so the rect is exactly the app.
#[tauri::command]
async fn screenshot_to_clipboard(window: tauri::Window) -> Result<(), String> {
    let app = window.app_handle().clone();
    let pos = window
        .outer_position()
        .map_err(|e| format!("Window position: {e}"))?;
    let size = window.outer_size().map_err(|e| format!("Window size: {e}"))?;
    let (mut wx, mut wy, mut ww, mut wh) = (pos.x, pos.y, size.width as i32, size.height as i32);

    // Real window handle so the clipboard is owned by our GUI thread on Windows.
    #[cfg(windows)]
    let hwnd_isize: isize = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);

    // `outer_*` mirrors GetWindowRect, which on Windows includes the invisible
    // resize border (~7px) — that overhang bleeds the taskbar/desktop into the
    // shot. The DWM extended frame bounds are the true visible window rect.
    #[cfg(windows)]
    if let Some((dx, dy, dw, dh)) = dwm_frame_bounds(hwnd_isize) {
        wx = dx;
        wy = dy;
        ww = dw;
        wh = dh;
    }

    // 1. Capture the monitor + crop to the window rect (heavy work, off-thread).
    let (bytes, png_bytes, rw, rh) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(Vec<u8>, Vec<u8>, usize, usize), String> {
            let monitors = xcap::Monitor::all().map_err(|e| format!("Enumerate monitors: {e}"))?;

            let cx = wx + ww / 2;
            let cy = wy + wh / 2;
            let monitor = monitors
                .into_iter()
                .find(|m| {
                    let mx = m.x().unwrap_or(0);
                    let my = m.y().unwrap_or(0);
                    let mw = m.width().unwrap_or(0) as i32;
                    let mh = m.height().unwrap_or(0) as i32;
                    cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
                })
                .ok_or("No monitor contains the window")?;

            let mx = monitor.x().map_err(|e| format!("Monitor x: {e}"))?;
            let my = monitor.y().map_err(|e| format!("Monitor y: {e}"))?;
            let image = monitor
                .capture_image()
                .map_err(|e| format!("Capture monitor: {e}"))?;
            let img_w = image.width() as i32;
            let img_h = image.height() as i32;
            let raw = image.into_raw(); // RGBA, row-major

            let rx = (wx - mx).clamp(0, img_w.max(1) - 1);
            let ry = (wy - my).clamp(0, img_h.max(1) - 1);
            let rw = ww.min(img_w - rx).max(0);
            let rh = wh.min(img_h - ry).max(0);
            if rw == 0 || rh == 0 {
                return Err("Window is outside the captured monitor area".into());
            }

            let stride = (img_w * 4) as usize;
            let mut cropped = Vec::with_capacity((rw * rh * 4) as usize);
            for row in 0..rh {
                let src_y = (ry + row) as usize;
                let start = src_y * stride + (rx as usize) * 4;
                let end = start + (rw as usize) * 4;
                cropped.extend_from_slice(&raw[start..end]);
            }

            // PNG copy for Chromium/Electron apps (Discord, Cursor, browsers),
            // which read the registered "PNG" clipboard format, not CF_DIB.
            let buf = image::RgbaImage::from_raw(rw as u32, rh as u32, cropped.clone())
                .ok_or("Build image buffer")?;
            let mut png_bytes = Vec::new();
            buf.write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .map_err(|e| format!("Encode PNG: {e}"))?;

            Ok((cropped, png_bytes, rw as usize, rh as usize))
        })
        .await
        .map_err(|e| e.to_string())??;

    // 2. Write to the clipboard on the MAIN thread. On Windows the clipboard must
    //    be opened from a thread with a message queue, otherwise SetClipboardData
    //    fails with ERROR_CLIPBOARD_NOT_OPEN (os error 1418).
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        #[cfg(windows)]
        let res = set_clipboard_image(hwnd_isize, &bytes, &png_bytes, rw, rh);
        #[cfg(not(windows))]
        let res = (|| -> Result<(), String> {
            let _ = &png_bytes;
            let mut clipboard =
                arboard::Clipboard::new().map_err(|e| format!("Open clipboard: {e}"))?;
            clipboard
                .set_image(arboard::ImageData {
                    width: rw,
                    height: rh,
                    bytes: std::borrow::Cow::Owned(bytes),
                })
                .map_err(|e| format!("Write image to clipboard: {e}"))
        })();
        let _ = tx.send(res);
    })
    .map_err(|e| format!("Dispatch to main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Clipboard task dropped: {e}"))?
}

/// Write an image to the Windows clipboard in two formats:
///   * `CF_DIB`  — top-down 32-bpp, for Paint / classic Win32 apps.
///   * `"PNG"`   — registered format read by Chromium/Electron (Discord, Cursor).
///
/// We bypass `arboard`: its Windows DIB path closes the clipboard before
/// `SetClipboardData` on some builds (→ os error 1418) and only offers a DIB,
/// which Chromium apps ignore. Doing the Win32 calls ourselves lets us publish
/// both formats under a single Open/Close with correctly-ordered writes.
#[cfg(windows)]
fn set_clipboard_image(
    hwnd_isize: isize,
    rgba: &[u8],
    png: &[u8],
    w: usize,
    h: usize,
) -> Result<(), String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW,
        SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };

    const CF_DIB: u32 = 8;

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    // Copy `data` into a moveable global block and hand it to the clipboard.
    unsafe fn publish(format: u32, data: &[u8]) -> Result<(), String> {
        let hmem = GlobalAlloc(GMEM_MOVEABLE, data.len());
        if hmem.is_null() {
            return Err("GlobalAlloc failed".into());
        }
        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        GlobalUnlock(hmem);
        if SetClipboardData(format, hmem).is_null() {
            return Err(format!(
                "SetClipboardData failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Clipboard now owns hmem; do not free it.
        Ok(())
    }

    // Build the CF_DIB blob: BITMAPINFOHEADER + BGRA pixels (top-down).
    let header_size = std::mem::size_of::<BitmapInfoHeader>();
    let mut dib = Vec::with_capacity(header_size + w * h * 4);
    let header = BitmapInfoHeader {
        bi_size: header_size as u32,
        bi_width: w as i32,
        bi_height: -(h as i32), // negative → top-down rows
        bi_planes: 1,
        bi_bit_count: 32,
        bi_compression: 0, // BI_RGB
        bi_size_image: 0,
        bi_x_pels_per_meter: 0,
        bi_y_pels_per_meter: 0,
        bi_clr_used: 0,
        bi_clr_important: 0,
    };
    dib.extend_from_slice(unsafe {
        std::slice::from_raw_parts(&header as *const _ as *const u8, header_size)
    });
    for i in 0..(w * h) {
        dib.push(rgba[i * 4 + 2]); // B
        dib.push(rgba[i * 4 + 1]); // G
        dib.push(rgba[i * 4]); // R
        dib.push(rgba[i * 4 + 3]); // A
    }

    let png_format_name: Vec<u16> = "PNG".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let hwnd = hwnd_isize as *mut c_void as HWND;
        if OpenClipboard(hwnd) == 0 {
            return Err(format!(
                "OpenClipboard failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let result = (|| -> Result<(), String> {
            if EmptyClipboard() == 0 {
                return Err(format!(
                    "EmptyClipboard failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            publish(CF_DIB, &dib)?;

            let png_format = RegisterClipboardFormatW(png_format_name.as_ptr());
            if png_format != 0 {
                publish(png_format, png)?;
            }
            Ok(())
        })();

        CloseClipboard();
        result
    }
}

/// True visible window rectangle (x, y, w, h) in physical pixels, excluding the
/// invisible resize border that `GetWindowRect`/`outer_*` would include.
#[cfg(windows)]
fn dwm_frame_bounds(hwnd_isize: isize) -> Option<(i32, i32, i32, i32)> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;

    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

    if hwnd_isize == 0 {
        return None;
    }
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let hr = DwmGetWindowAttribute(
            hwnd_isize as *mut c_void as HWND,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr != 0 {
            return None;
        }
        Some((
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        ))
    }
}

// ─── Entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `systems` plugin.
///
/// Pinned to the `Wry` runtime rather than generic over `R: Runtime`. Bare
/// `tauri::AppHandle` already means `AppHandle<Wry>`, so every type in this file
/// stays exactly as it was in the standalone app — the alternative was
/// threading `<R>` through `EngineManager`, `EngineInner` and every command for
/// no benefit in a desktop-only binary.
///
/// The `dialog`, `fs` and `shell` plugins the standalone app registered are now
/// registered once by the suite binary, not here.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("systems")
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            get_app_settings,
            update_app_settings,
            list_systems,
            get_system_config,
            save_system_config,
            start_engine,
            stop_engine,
            engine_status,
            live_eval,
            run_backtest,
            browse_universe,
            cache_stats,
            clear_cache,
            screenshot_to_clipboard,
        ]))
        .setup(|app, _api| {
            let dir = data_dir();
            fs::create_dir_all(&dir).ok();
            import_legacy_data(&dir);

            let cache_dir = dir.join("cache");
            fs::create_dir_all(&cache_dir).ok();
            let cache_path = cache_dir.join("rotation_lab.sqlite");

            let (settings, configs) = load_store(&dir);
            app.manage(Store {
                data_dir: dir,
                settings: Mutex::new(settings),
                system_configs: Mutex::new(configs),
            });

            // Lazy by contract: this only builds the manager. No Python process
            // is spawned until a command asks for one (ARCHITECTURE.md §11).
            let engine = EngineManager::new(app.clone(), resolve_engine_dir(app), cache_path);
            // Nothing the suite starts may outlive it, and only this module
            // holds the engine's `Child` — so the kill is registered here.
            // qs-core cannot do it: its register owns no handles.
            let engine_at_shutdown = engine.clone();
            qs_core::shutdown::on_shutdown(
                app,
                "systems.engine",
                Box::new(move || {
                    let _ = engine_at_shutdown.stop();
                }),
            );

            app.manage(engine);

            // Announce the engine to the suite's process register, stopped:
            // the process page lists processes that are NOT running too, or
            // there would be no way to start one from it. Start/stop point at
            // the module's own commands — both already take no arguments, which
            // is the contract the page invokes them under. There is no
            // `logs_with`: the engine's stderr goes to the suite log, this
            // module keeps no ring buffer, and a button that shows nothing is
            // worse than no button (the page hides it when the field is None).
            qs_core::processes::announce(
                app,
                qs_core::processes::ProcessInfo::new(
                    ENGINE_PROCESS_ID,
                    "systems",
                    "Rotation Lab Engine",
                )
                .start_with("plugin:systems|start_engine")
                .stop_with("plugin:systems|stop_engine"),
            );

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "systems".into(),
                    label: "QuantSystems".into(),
                    route: "/systems".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .build()
}

/// One-time import of the standalone app's `~/.quantsystems` directory.
///
/// Copies, never moves: the standalone QuantSystems stays installable and
/// working throughout the migration (MIGRATION.md, governing rule). A marker
/// file makes it idempotent.
fn import_legacy_data(target: &Path) {
    let marker = target.join(".migrated");
    if marker.exists() {
        return;
    }

    let legacy = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".quantsystems");

    if legacy.exists() && legacy != target {
        if let Err(e) = copy_dir_recursive(&legacy, target) {
            eprintln!("systems: legacy import failed: {e}");
            return;
        }
    }

    let _ = fs::write(&marker, chrono::Utc::now().to_rfc3339());
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else if !dst.exists() {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_python_command_keeps_the_launcher_flag() {
        let py = PythonCommand::new("py", &["-3"]);
        assert_eq!(py.display(), "py -3");
        let cmd = py.command();
        assert_eq!(cmd.get_program(), "py");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), vec!["-3"]);
        assert_eq!(PythonCommand::new("python3", &[]).display(), "python3");
    }

    /// Needs an interpreter with numpy and pandas on this machine — the
    /// operator's, not CI's.
    #[test]
    #[ignore]
    fn live_resolve_python_finds_an_interpreter_with_the_engine_deps() {
        let python = resolve_python();
        assert!(
            engine_python(&python).is_some(),
            "resolved {} but it cannot import numpy and pandas",
            python.display()
        );
    }

    #[test]
    #[ignore] // Requires the locally installed Python engine dependencies.
    fn live_engine_handle_owns_python_instead_of_its_launcher() {
        let mut child = resolve_python().command()
            .args(["-c", "import os; print(os.getpid(), flush=True); input()"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn().expect("spawn engine interpreter");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let python_pid = line.trim().parse::<u32>().unwrap();
        let managed_pid = child.id();
        child.kill().expect("stop Python worker");
        child.wait().expect("reap Python worker");
        assert_eq!(managed_pid, python_pid, "the managed child must be the interpreter");
    }
}
