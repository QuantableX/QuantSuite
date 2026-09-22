//! QuantVoice — dictation for QuantHUD (docs/PLAN-QUANTVOICE.md).
//!
//! Whisper runs in a Python sidecar (`sidecars/python/quantvoice`, driven
//! over JSON-RPC on its stdio like the other engines) inside a venv the
//! module creates on first use. This file is the Rust half: the child, the
//! wire, the one-time setup, the global push-to-talk hotkey, and putting
//! the words where the cursor is.
//!
//! Two ways in, one result. The Transcript module's button records into
//! the HUD's own list (the HUD has focus then, so there is nowhere else to
//! put the words); the hotkey records from anywhere and pastes the result
//! into whatever app is in front, because the HUD never took focus. Held,
//! the hotkey is push-to-talk; tapped, it toggles.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// The process register entry (ARCHITECTURE.md §6).
pub const PROCESS_ID: &str = "hud.voice";
pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+Space";
/// A press shorter than this is a tap (toggle); longer, push-to-talk.
const HOLD: Duration = Duration::from_millis(600);
const LOG_CAP: usize = 400;
const SETUP_LINES_CAP: usize = 200;
const READY_TIMEOUT: Duration = Duration::from_secs(90);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const SIDECAR_MARKER: &str = "quantvoice";

// ── Configuration (the HUD's config.json, the `speech*` keys of useConfig) ──

fn d_language() -> String {
    "auto".into()
}
fn d_model() -> String {
    "small".into()
}
fn d_device() -> String {
    "auto".into()
}
fn d_insert() -> String {
    "paste".into()
}
fn d_hotkey() -> String {
    DEFAULT_HOTKEY.into()
}
fn d_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    /// `auto` or an ISO 639-1 code (`de`, `en`, …).
    #[serde(default = "d_language")]
    pub speech_language: String,
    /// `tiny` | `base` | `small` | `medium` | `large-v3` | `large-v3-turbo` | a local directory.
    #[serde(default = "d_model")]
    pub speech_model: String,
    /// `auto` (CUDA when it works, else CPU) | `cpu` | `cuda`.
    #[serde(default = "d_device")]
    pub speech_device: String,
    /// What happens to hotkey results: `paste` (clipboard + Ctrl+V into the
    /// front app) | `type` (typed as keystrokes) | `clipboard` (copy only).
    #[serde(default = "d_insert")]
    pub speech_insert: String,
    #[serde(default = "d_hotkey")]
    pub speech_hotkey: String,
    /// Preview words while still recording.
    #[serde(default = "d_true")]
    pub speech_live_preview: bool,
    /// Input device by name; empty = the system default.
    #[serde(default)]
    pub speech_input_device: String,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            speech_language: d_language(),
            speech_model: d_model(),
            speech_device: d_device(),
            speech_insert: d_insert(),
            speech_hotkey: d_hotkey(),
            speech_live_preview: true,
            speech_input_device: String::new(),
        }
    }
}

/// `auto`, or the bare ISO 639-1 code the settings page writes (`de`, `en`).
///
/// The standalone app stored BCP-47 tags (`en-US`, `de-DE`) or `system` for
/// the Windows dictation engine. Those were a choice for that engine, not for
/// Whisper — `en-US` was its default and, mapped to `en`, pinned Whisper to
/// English for every take — so every legacy tag means `auto`: Whisper detects
/// the language per take. Only a bare code stays fixed.
fn normalize_language(value: &str) -> String {
    let lang = value.trim().to_lowercase();
    if lang.is_empty() || lang == "system" || lang == "auto" || lang.contains(['-', '_']) {
        "auto".into()
    } else {
        lang
    }
}

impl SpeechConfig {
    fn normalized(mut self) -> Self {
        self.speech_language = normalize_language(&self.speech_language);
        if !matches!(self.speech_device.as_str(), "auto" | "cpu" | "cuda") {
            self.speech_device = "auto".into();
        }
        if !matches!(self.speech_insert.as_str(), "paste" | "type" | "clipboard") {
            self.speech_insert = "paste".into();
        }
        if self.speech_hotkey.trim().is_empty() {
            self.speech_hotkey = DEFAULT_HOTKEY.into();
        }
        self.speech_model = self.speech_model.trim().to_string();
        if self.speech_model.is_empty() {
            self.speech_model = d_model();
        }
        self
    }
}

pub fn read_config() -> SpeechConfig {
    crate::config::load_config()
        .ok()
        .and_then(|s| serde_json::from_str::<SpeechConfig>(&s).ok())
        .unwrap_or_default()
        .normalized()
}

// ── Paths ──

struct Paths {
    sidecar: PathBuf,
    venv: PathBuf,
    python: PathBuf,
    models: PathBuf,
    marker: PathBuf,
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

fn paths(app: &AppHandle) -> Result<Paths, String> {
    let sidecar = qs_core::paths::python_sidecar_dir(app, SIDECAR_MARKER).ok_or_else(|| {
        "the quantvoice sidecar was not found — sidecars/python/quantvoice is missing from the checkout or the installed bundle".to_string()
    })?;
    let module = qs_core::paths::module_dir("hud");
    let venv = module.join("pyenv");
    Ok(Paths { python: venv_python(&venv), models: module.join("models"), marker: venv.join(".quantvoice-installed"), sidecar, venv })
}

/// FNV-1a over the requirement files: the marker stores it, so a changed
/// requirements.txt triggers a reinstall.
fn requirements_hash(p: &Paths, cuda: bool) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(&std::fs::read(p.sidecar.join(SIDECAR_MARKER).join("requirements.txt")).unwrap_or_default());
    if cuda {
        feed(b"+cuda");
        feed(&std::fs::read(p.sidecar.join(SIDECAR_MARKER).join("requirements-cuda.txt")).unwrap_or_default());
    }
    format!("{h:016x}")
}

fn no_window(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(0x0800_0000);
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) { vec![format!("{name}.exe"), name.to_string()] } else { vec![name.to_string()] };
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let c = dir.join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

// ── State ──

struct Proc {
    child: Child,
    stdin: ChildStdin,
}

#[derive(Default)]
struct Inner {
    proc: Option<Proc>,
    next_id: u64,
    pending: HashMap<u64, mpsc::Sender<Value>>,
    logs: VecDeque<String>,
    ready: bool,
    configured: bool,
    phase: String,
    detail: Option<String>,
    model: Option<String>,
    device: Option<String>,
    compute: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub busy: bool,
    /// `venv` | `install` | `cuda` | `done` | `error` | empty
    pub phase: String,
    pub error: Option<String>,
    pub lines: Vec<String>,
}

pub struct Speech {
    inner: Mutex<Inner>,
    setup: Mutex<SetupState>,
    recording: AtomicBool,
    via_hotkey: AtomicBool,
    /// The next result is pasted where the cursor is.
    paste_next: AtomicBool,
    pressed_at: Mutex<Option<Instant>>,
    hotkey: Mutex<Option<String>>,
    hotkey_error: Mutex<Option<String>>,
}

impl Default for Speech {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner { phase: "offline".into(), ..Default::default() }),
            setup: Mutex::new(SetupState::default()),
            recording: AtomicBool::new(false),
            via_hotkey: AtomicBool::new(false),
            paste_next: AtomicBool::new(false),
            pressed_at: Mutex::new(None),
            hotkey: Mutex::new(None),
            hotkey_error: Mutex::new(None),
        }
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn push_log(inner: &mut Inner, line: String) {
    if inner.logs.len() >= LOG_CAP {
        inner.logs.pop_front();
    }
    inner.logs.push_back(line);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    pub sidecar_dir: Option<String>,
    pub python: Option<String>,
    pub installed: bool,
    /// The pip CUDA runtime (cuBLAS, cuDNN 9) is in the venv — what makes
    /// the GPU path work; a cuDNN of the wrong major on the system PATH
    /// hangs the first pass instead.
    pub cuda_extras: bool,
    /// `nvidia-smi` is on PATH: the setup card ticks the CUDA box then.
    pub nvidia: bool,
    pub uv: bool,
    pub running: bool,
    pub ready: bool,
    /// The engine's phase: `offline` | `starting` | `unloaded` | `loading` | `downloading` | `ready` | `recording` | `transcribing` | `error`
    pub phase: String,
    pub detail: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub compute: Option<String>,
    pub recording: bool,
    pub hotkey: String,
    pub hotkey_error: Option<String>,
    pub setup: SetupState,
    pub model_dir: String,
    pub config: SpeechConfig,
}

fn proc_alive(inner: &mut Inner) -> bool {
    match inner.proc.as_mut() {
        Some(p) => matches!(p.child.try_wait(), Ok(None)),
        None => false,
    }
}

pub fn status(app: &AppHandle, speech: &Speech) -> SpeechStatus {
    let config = read_config();
    let p = paths(app).ok();
    let marker = p.as_ref().and_then(|p| std::fs::read_to_string(&p.marker).ok()).unwrap_or_default();
    let installed = p
        .as_ref()
        .map(|p| p.python.is_file() && (marker.trim() == requirements_hash(p, false) || marker.trim() == requirements_hash(p, true)))
        .unwrap_or(false);
    let cuda_extras = p.as_ref().map(|p| marker.trim() == requirements_hash(p, true)).unwrap_or(false);
    let mut inner = lock(&speech.inner);
    let running = proc_alive(&mut inner);
    SpeechStatus {
        sidecar_dir: p.as_ref().map(|p| p.sidecar.to_string_lossy().into_owned()),
        python: p.as_ref().filter(|p| p.python.is_file()).map(|p| p.python.to_string_lossy().into_owned()),
        installed,
        cuda_extras,
        nvidia: which("nvidia-smi").is_some(),
        uv: which("uv").is_some(),
        running,
        ready: running && inner.ready,
        phase: if running { inner.phase.clone() } else { "offline".into() },
        detail: inner.detail.clone(),
        model: inner.model.clone(),
        device: inner.device.clone(),
        compute: inner.compute.clone(),
        recording: speech.recording.load(Ordering::SeqCst),
        hotkey: config.speech_hotkey.clone(),
        hotkey_error: lock(&speech.hotkey_error).clone(),
        setup: lock(&speech.setup).clone(),
        model_dir: p.as_ref().map(|p| p.models.to_string_lossy().into_owned()).unwrap_or_default(),
        config,
    }
}

// ── The child and the wire ──

/// Start the sidecar unless it runs, and wait for its `ready`.
fn ensure_running(app: &AppHandle, speech: &Speech) -> Result<(), String> {
    let p = paths(app)?;
    let (rx_ready, pid) = {
        let mut inner = lock(&speech.inner);
        if proc_alive(&mut inner) {
            return Ok(());
        }
        if !p.python.is_file() {
            return Err("QuantVoice is not set up yet — run the setup in the Transcript module".into());
        }
        let _ = std::fs::create_dir_all(&p.models);
        let mut cmd = Command::new(&p.python);
        cmd.args(["-u", "-m", SIDECAR_MARKER])
            .current_dir(&p.sidecar)
            .env("QUANTVOICE_MODEL_DIR", &p.models)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        no_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("could not start the QuantVoice engine ({}): {e}", p.python.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin on the engine")?;
        let stdout = child.stdout.take().ok_or("no stdout on the engine")?;
        let stderr = child.stderr.take().ok_or("no stderr on the engine")?;
        let pid = child.id();
        inner.proc = Some(Proc { child, stdin });
        inner.ready = false;
        inner.configured = false;
        inner.phase = "starting".into();
        inner.detail = None;
        inner.pending.clear();
        push_log(&mut inner, format!("[quantvoice] started pid {pid}"));

        let (tx_ready, rx_ready) = mpsc::channel::<()>();
        let app_out = app.clone();
        std::thread::Builder::new()
            .name("quantvoice-stdout".into())
            .spawn(move || read_stdout(app_out, stdout, tx_ready))
            .map_err(|e| e.to_string())?;
        let app_err = app.clone();
        std::thread::Builder::new()
            .name("quantvoice-stderr".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Some(s) = app_err.try_state::<Speech>() {
                        push_log(&mut lock(&s.inner), line);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        (rx_ready, pid)
    };
    let _ = app.emit("speech-state", json!({ "phase": "starting" }));
    qs_core::processes::mark_running(app, PROCESS_ID, Some(pid));
    match rx_ready.recv_timeout(READY_TIMEOUT) {
        Ok(()) => Ok(()),
        Err(_) => {
            let tail: Vec<String> = lock(&speech.inner).logs.iter().rev().take(6).cloned().collect();
            kill(app, speech);
            Err(format!("the QuantVoice engine did not report ready: {}", tail.join(" | ")))
        }
    }
}

fn kill(app: &AppHandle, speech: &Speech) {
    let mut inner = lock(&speech.inner);
    if let Some(mut p) = inner.proc.take() {
        let _ = p.child.kill();
        let _ = p.child.wait();
    }
    inner.ready = false;
    inner.configured = false;
    inner.phase = "offline".into();
    inner.pending.clear();
    speech.recording.store(false, Ordering::SeqCst);
    drop(inner);
    qs_core::processes::mark_stopped(app, PROCESS_ID);
    let _ = app.emit("speech-state", json!({ "phase": "offline" }));
}

/// Whether a take is in flight according to a `state` notification. The
/// engine's own `recording`/`transcribing` flags count first — a model that
/// finished loading mid-take used to announce `ready`, this side dropped the
/// take, and the stop failed with "not recording" — the phase is the
/// fallback. `transcribing` keeps the flag up until the result arrives: a
/// new take must not clear the pending paste.
fn take_in_flight(params: &Value) -> bool {
    let flag = |key: &str| params.get(key).and_then(Value::as_bool).unwrap_or(false);
    let phase = params.get("phase").and_then(Value::as_str).unwrap_or("");
    flag("recording") || flag("transcribing") || phase == "recording" || phase == "transcribing"
}

/// Every line the engine prints: responses go to their waiters,
/// notifications become `speech-*` events (and the state the status
/// command reports).
fn read_stdout(app: AppHandle, stdout: std::process::ChildStdout, tx_ready: mpsc::Sender<()>) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            if let Some(s) = app.try_state::<Speech>() {
                push_log(&mut lock(&s.inner), line);
            }
            continue;
        };
        let Some(speech) = app.try_state::<Speech>() else { break };
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            let waiter = lock(&speech.inner).pending.remove(&id);
            if let Some(tx) = waiter {
                let _ = tx.send(msg);
            }
            continue;
        }
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match method.as_str() {
            "ready" => {
                let mut inner = lock(&speech.inner);
                inner.ready = true;
                inner.phase = "unloaded".into();
                if let Some(info) = params.get("info") {
                    inner.model = info.get("model").and_then(Value::as_str).map(str::to_string);
                }
                drop(inner);
                let _ = tx_ready.send(());
                let _ = app.emit("speech-state", json!({ "phase": "unloaded", "engine": params }));
            }
            "state" => {
                let mut inner = lock(&speech.inner);
                inner.phase = params.get("phase").and_then(Value::as_str).unwrap_or("ready").to_string();
                inner.detail = params.get("detail").and_then(Value::as_str).map(str::to_string);
                inner.model = params.get("model").and_then(Value::as_str).map(str::to_string);
                inner.device = params.get("device").and_then(Value::as_str).map(str::to_string);
                inner.compute = params.get("compute").and_then(Value::as_str).map(str::to_string);
                speech.recording.store(take_in_flight(&params), Ordering::SeqCst);
                drop(inner);
                let _ = app.emit("speech-state", params);
            }
            "level" => {
                let _ = app.emit("speech-level", params);
            }
            "interim" => {
                let _ = app.emit("speech-interim", params);
            }
            "download" => {
                let _ = app.emit("speech-download", params);
            }
            "result" => {
                speech.recording.store(false, Ordering::SeqCst);
                let text = params.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
                let via_hotkey = speech.via_hotkey.swap(false, Ordering::SeqCst);
                let paste = speech.paste_next.swap(false, Ordering::SeqCst) && !text.is_empty();
                let config = read_config();
                let inserted = paste && config.speech_insert != "clipboard";
                if paste {
                    let app2 = app.clone();
                    let mode = config.speech_insert.clone();
                    let text2 = text.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = insert_text(&app2, &text2, &mode) {
                            let _ = app2.emit("speech-error", json!({ "message": format!("could not insert the text: {e}") }));
                        }
                    });
                }
                let mut payload = params.clone();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("text".into(), Value::String(text));
                    obj.insert("inserted".into(), Value::Bool(inserted));
                    obj.insert("viaHotkey".into(), Value::Bool(via_hotkey));
                    obj.insert("insertMode".into(), Value::String(config.speech_insert));
                }
                let _ = app.emit("speech-result", payload);
            }
            "error" => {
                speech.recording.store(false, Ordering::SeqCst);
                speech.paste_next.store(false, Ordering::SeqCst);
                let _ = app.emit("speech-error", params);
            }
            other => {
                push_log(&mut lock(&speech.inner), format!("[quantvoice] {other}: {params}"));
            }
        }
    }
    // EOF: the engine is gone.
    if let Some(speech) = app.try_state::<Speech>() {
        let mut inner = lock(&speech.inner);
        if let Some(mut p) = inner.proc.take() {
            let _ = p.child.wait();
        }
        inner.ready = false;
        inner.configured = false;
        inner.phase = "offline".into();
        inner.pending.clear();
        push_log(&mut inner, "[quantvoice] exited".into());
        drop(inner);
        speech.recording.store(false, Ordering::SeqCst);
    }
    qs_core::processes::mark_stopped(&app, PROCESS_ID);
    let _ = app.emit("speech-state", json!({ "phase": "offline" }));
}

fn request(speech: &Speech, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
    let rx = {
        let mut inner = lock(&speech.inner);
        if !proc_alive(&mut inner) {
            return Err("the QuantVoice engine is not running".into());
        }
        inner.next_id += 1;
        let id = inner.next_id;
        let (tx, rx) = mpsc::channel();
        inner.pending.insert(id, tx);
        let line = json!({ "id": id, "method": method, "params": params }).to_string();
        let proc = inner.proc.as_mut().ok_or("no engine")?;
        proc.stdin
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|()| proc.stdin.flush())
            .map_err(|e| format!("could not talk to the QuantVoice engine: {e}"))?;
        rx
    };
    let reply = rx.recv_timeout(timeout).map_err(|_| format!("the QuantVoice engine did not answer `{method}` in time"))?;
    if let Some(err) = reply.get("error").and_then(Value::as_str) {
        return Err(err.to_string());
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// Hand the engine the current settings; it loads the model in the background.
fn configure(speech: &Speech) -> Result<Value, String> {
    let c = read_config();
    let params = json!({
        "model": c.speech_model,
        "device": c.speech_device,
        "language": c.speech_language,
        "live": c.speech_live_preview,
        "inputDevice": c.speech_input_device,
    });
    let r = request(speech, "configure", params, REQUEST_TIMEOUT)?;
    lock(&speech.inner).configured = true;
    Ok(r)
}

fn ensure_configured(app: &AppHandle, speech: &Speech) -> Result<(), String> {
    ensure_running(app, speech)?;
    if !lock(&speech.inner).configured {
        configure(speech)?;
    }
    Ok(())
}

pub fn start_recording(app: &AppHandle, speech: &Speech, via_hotkey: bool) -> Result<(), String> {
    ensure_configured(app, speech)?;
    if speech.recording.swap(true, Ordering::SeqCst) {
        return Err("already recording".into());
    }
    speech.via_hotkey.store(via_hotkey, Ordering::SeqCst);
    speech.paste_next.store(false, Ordering::SeqCst);
    if let Err(e) = request(speech, "start", json!({}), REQUEST_TIMEOUT) {
        speech.recording.store(false, Ordering::SeqCst);
        return Err(e);
    }
    Ok(())
}

/// Stop and transcribe; the words arrive as a `speech-result` event. A
/// hotkey take is pasted where the cursor is.
pub fn stop_recording(speech: &Speech) -> Result<(), String> {
    if !speech.recording.load(Ordering::SeqCst) {
        return Err("not recording".into());
    }
    let via_hotkey = speech.via_hotkey.load(Ordering::SeqCst);
    speech.paste_next.store(via_hotkey, Ordering::SeqCst);
    request(speech, "stop", json!({}), REQUEST_TIMEOUT)?;
    Ok(())
}

pub fn cancel_recording(speech: &Speech) -> Result<(), String> {
    speech.paste_next.store(false, Ordering::SeqCst);
    speech.recording.store(false, Ordering::SeqCst);
    request(speech, "cancel", json!({}), REQUEST_TIMEOUT)?;
    Ok(())
}

// ── Setup: the venv and its packages ──

fn setup_line(app: &AppHandle, speech: &Speech, phase: &str, line: impl Into<String>) {
    let line = line.into();
    {
        let mut s = lock(&speech.setup);
        s.phase = phase.into();
        if s.lines.len() >= SETUP_LINES_CAP {
            s.lines.remove(0);
        }
        s.lines.push(line.clone());
    }
    let _ = app.emit("speech-setup", json!({ "phase": phase, "line": line }));
}

/// Run a command, streaming its output lines into the setup log.
fn run_streaming(app: &AppHandle, speech: &Speech, phase: &str, mut cmd: Command) -> Result<(), String> {
    no_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("{e}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel::<String>();
    let mut readers = Vec::new();
    for stream in [stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        }));
    }
    drop(tx);
    for line in rx {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            setup_line(app, speech, phase, trimmed.to_string());
        }
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}

/// A CPython 3.9–3.13 to build the venv from, when `uv` is not around.
fn base_python() -> Option<Vec<String>> {
    let mut candidates: Vec<Vec<String>> = Vec::new();
    if let Ok(p) = std::env::var("QUANTSUITE_PYTHON") {
        if !p.trim().is_empty() {
            candidates.push(vec![p]);
        }
    }
    if cfg!(windows) {
        for v in ["-3.12", "-3.11", "-3.13", "-3.10"] {
            candidates.push(vec!["py".into(), v.into()]);
        }
    }
    candidates.push(vec!["python3".into()]);
    candidates.push(vec!["python".into()]);
    for c in candidates {
        let mut cmd = Command::new(&c[0]);
        cmd.args(&c[1..]).args(["-c", "import sys; print(sys.version_info[0], sys.version_info[1])"]);
        no_window(&mut cmd);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let mut it = s.split_whitespace();
                let major: u32 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let minor: u32 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                if major == 3 && (9..=13).contains(&minor) {
                    return Some(c);
                }
            }
        }
    }
    None
}

fn run_setup(app: &AppHandle, speech: &Speech, cuda: bool) -> Result<(), String> {
    let p = paths(app)?;
    let req = p.sidecar.join(SIDECAR_MARKER).join("requirements.txt");
    let req_cuda = p.sidecar.join(SIDECAR_MARKER).join("requirements-cuda.txt");
    if !req.is_file() {
        return Err(format!("{} is missing", req.display()));
    }
    let uv = which("uv");
    if !p.python.is_file() {
        let _ = std::fs::create_dir_all(p.venv.parent().unwrap_or(&p.venv));
        setup_line(app, speech, "venv", format!("creating the Python environment at {}", p.venv.display()));
        let mut done = false;
        if let Some(uv) = &uv {
            let mut cmd = Command::new(uv);
            cmd.args(["venv", "--python", "3.12", "--seed"]).arg(&p.venv);
            match run_streaming(app, speech, "venv", cmd) {
                Ok(()) => done = true,
                Err(e) => setup_line(app, speech, "venv", format!("uv could not create the venv ({e}); trying python -m venv")),
            }
        }
        if !done {
            let base = base_python().ok_or("no Python 3.9–3.13 found (install Python 3.12 or uv, or set QUANTSUITE_PYTHON)")?;
            setup_line(app, speech, "venv", format!("using {}", base.join(" ")));
            let mut cmd = Command::new(&base[0]);
            cmd.args(&base[1..]).args(["-m", "venv"]).arg(&p.venv);
            run_streaming(app, speech, "venv", cmd).map_err(|e| format!("python -m venv failed: {e}"))?;
        }
        if !p.python.is_file() {
            return Err(format!("the venv was created but {} is missing", p.python.display()));
        }
    }
    setup_line(app, speech, "install", "installing faster-whisper, sounddevice, numpy (this downloads a few hundred MB once)");
    let install = |phase: &str, file: &Path| -> Result<(), String> {
        let mut cmd = if let Some(uv) = &uv {
            let mut c = Command::new(uv);
            c.args(["pip", "install", "--python"]).arg(&p.python).arg("-r").arg(file);
            c
        } else {
            let mut c = Command::new(&p.python);
            c.args(["-m", "pip", "install", "--progress-bar", "off", "-r"]).arg(file);
            c
        };
        cmd.env("PYTHONIOENCODING", "utf-8");
        run_streaming(app, speech, phase, cmd).map_err(|e| format!("installing {} failed: {e}", file.display()))
    };
    install("install", &req)?;
    if cuda {
        if req_cuda.is_file() {
            setup_line(app, speech, "cuda", "installing the CUDA runtime wheels (cuBLAS, cuDNN — about 700 MB)");
            install("cuda", &req_cuda)?;
        } else {
            setup_line(app, speech, "cuda", "requirements-cuda.txt is missing; skipping the CUDA wheels");
        }
    }
    std::fs::write(&p.marker, requirements_hash(&p, cuda)).map_err(|e| format!("marker: {e}"))?;
    setup_line(app, speech, "done", "QuantVoice is installed");
    Ok(())
}

/// The whole setup on a background thread; progress arrives as
/// `speech-setup` events, the outcome in the status. Starts the engine and
/// loads the model afterwards, so the first recording finds it warm.
pub fn setup_in_background(app: &AppHandle, cuda: bool) -> Result<(), String> {
    let speech = app.try_state::<Speech>().ok_or("speech state missing")?;
    {
        let mut s = lock(&speech.setup);
        if s.busy {
            return Err("setup is already running".into());
        }
        *s = SetupState { busy: true, phase: "venv".into(), error: None, lines: Vec::new() };
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("quantvoice-setup".into())
        .spawn(move || {
            let speech = app.state::<Speech>();
            let outcome = run_setup(&app, &speech, cuda);
            {
                let mut s = lock(&speech.setup);
                s.busy = false;
                match &outcome {
                    Ok(()) => s.phase = "done".into(),
                    Err(e) => {
                        s.phase = "error".into();
                        s.error = Some(e.clone());
                    }
                }
            }
            match outcome {
                Ok(()) => {
                    let _ = app.emit("speech-setup", json!({ "phase": "done", "line": "installed" }));
                    // A fresh venv, a fresh engine: whatever ran before is stale.
                    kill(&app, &speech);
                    if let Err(e) = ensure_configured(&app, &speech) {
                        let _ = app.emit("speech-error", json!({ "message": e }));
                    }
                }
                Err(e) => {
                    let _ = app.emit("speech-setup", json!({ "phase": "error", "line": e }));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── The hotkey ──

/// Register the configured hotkey, replacing the previous one. A hotkey
/// another app owns fails here and shows up as `hotkeyError` in the status.
pub fn register_hotkey(app: &AppHandle, speech: &Speech, hotkey: &str) {
    let hotkey = hotkey.trim().to_string();
    let previous = lock(&speech.hotkey).take();
    if let Some(prev) = previous {
        let _ = app.global_shortcut().unregister(prev.as_str());
    }
    let result = app.global_shortcut().on_shortcut(hotkey.as_str(), |app, _shortcut, event| {
        let app = app.clone();
        match event.state() {
            ShortcutState::Pressed => {
                std::thread::spawn(move || on_press(&app));
            }
            ShortcutState::Released => {
                std::thread::spawn(move || on_release(&app));
            }
        }
    });
    match result {
        Ok(()) => {
            *lock(&speech.hotkey) = Some(hotkey);
            *lock(&speech.hotkey_error) = None;
        }
        Err(e) => {
            *lock(&speech.hotkey_error) = Some(format!("{hotkey}: {e}"));
        }
    }
}

fn on_press(app: &AppHandle) {
    let Some(speech) = app.try_state::<Speech>() else { return };
    if speech.recording.load(Ordering::SeqCst) {
        // A second press ends a toggled take.
        if let Err(e) = stop_recording(&speech) {
            let _ = app.emit("speech-error", json!({ "message": e }));
        }
        return;
    }
    *lock(&speech.pressed_at) = Some(Instant::now());
    if let Err(e) = start_recording(app, &speech, true) {
        let _ = app.emit("speech-error", json!({ "message": e }));
    }
}

fn on_release(app: &AppHandle) {
    let Some(speech) = app.try_state::<Speech>() else { return };
    if !speech.recording.load(Ordering::SeqCst) || !speech.via_hotkey.load(Ordering::SeqCst) {
        return;
    }
    let held = lock(&speech.pressed_at).map(|t| t.elapsed()).unwrap_or(Duration::ZERO);
    if held >= HOLD {
        if let Err(e) = stop_recording(&speech) {
            let _ = app.emit("speech-error", json!({ "message": e }));
        }
    }
    // A tap keeps recording until the next press.
}

// ── Putting the words at the cursor ──

/// `paste`: clipboard, then Ctrl+V into the front window. `type`: typed as
/// keystrokes (for the odd app where Ctrl+V is not paste). `clipboard`:
/// copy only. Modifier keys still held from the hotkey are waited out
/// first, so the paste is not a Ctrl+Shift+V.
pub fn insert_text(app: &AppHandle, text: &str, mode: &str) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    if mode != "type" {
        app.clipboard().write_text(text.to_string()).map_err(|e| e.to_string())?;
    }
    if mode == "clipboard" {
        return Ok(());
    }
    #[cfg(windows)]
    {
        wait_modifiers_released();
        std::thread::sleep(Duration::from_millis(40));
        if mode == "type" {
            type_unicode(text);
        } else {
            send_ctrl_v();
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = mode;
        Ok(())
    }
}

#[cfg(windows)]
fn wait_modifiers_released() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
    let deadline = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < deadline {
        let down = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            .any(|vk| unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16 & 0x8000 != 0);
        if !down {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
fn key(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY, up: bool) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP};
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
        },
    }
}

#[cfg(windows)]
fn send_ctrl_v() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, VK_CONTROL, VK_V};
    let inputs = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(windows)]
fn type_unicode(text: &str) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY};
    let unit = |code: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code,
                dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let mut batch: Vec<INPUT> = Vec::with_capacity(64);
    for code in text.encode_utf16() {
        batch.push(unit(code, false));
        batch.push(unit(code, true));
        if batch.len() >= 64 {
            unsafe {
                SendInput(&batch, std::mem::size_of::<INPUT>() as i32);
            }
            batch.clear();
            std::thread::sleep(Duration::from_millis(4));
        }
    }
    if !batch.is_empty() {
        unsafe {
            SendInput(&batch, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

// ── Plugin glue ──

/// Called from the plugin's setup: the state, the process-register entry,
/// the hotkey, and the shutdown hook that kills the engine.
pub fn init(app: &AppHandle) {
    app.manage(Speech::default());
    qs_core::processes::announce(
        app,
        qs_core::processes::ProcessInfo::new(PROCESS_ID, "hud", "QuantVoice — Whisper engine")
            .start_with("plugin:hud|speech_engine_start")
            .stop_with("plugin:hud|speech_engine_stop")
            .logs_with("plugin:hud|speech_engine_logs"),
    );
    let config = read_config();
    if let Some(speech) = app.try_state::<Speech>() {
        register_hotkey(app, &speech, &config.speech_hotkey);
    }
    let handle = app.clone();
    qs_core::shutdown::on_shutdown(
        app,
        "hud:voice",
        Box::new(move || {
            if let Some(speech) = handle.try_state::<Speech>() {
                let mut inner = lock(&speech.inner);
                if let Some(mut p) = inner.proc.take() {
                    let _ = p.child.kill();
                }
            }
        }),
    );
}

// ── Commands ──

#[tauri::command(async)]
pub fn speech_status(app: AppHandle, state: tauri::State<'_, Speech>) -> SpeechStatus {
    status(&app, &state)
}

/// Create the venv, install the packages, start the engine, load the model.
/// Returns at once; progress comes as `speech-setup` events.
#[tauri::command(async)]
pub fn speech_setup(app: AppHandle, cuda: Option<bool>) -> Result<(), String> {
    setup_in_background(&app, cuda.unwrap_or(false))
}

#[tauri::command(async)]
pub fn speech_start(app: AppHandle, state: tauri::State<'_, Speech>) -> Result<(), String> {
    start_recording(&app, &state, false)
}

#[tauri::command(async)]
pub fn speech_stop(state: tauri::State<'_, Speech>) -> Result<(), String> {
    stop_recording(&state)
}

#[tauri::command(async)]
pub fn speech_cancel(state: tauri::State<'_, Speech>) -> Result<(), String> {
    cancel_recording(&state)
}

#[tauri::command(async)]
pub fn speech_devices(app: AppHandle, state: tauri::State<'_, Speech>) -> Result<Value, String> {
    ensure_running(&app, &state)?;
    request(&state, "devices", json!({}), REQUEST_TIMEOUT)
}

/// The settings changed: re-register the hotkey and hand the engine the
/// new model/device/language if it runs.
#[tauri::command(async)]
pub fn speech_apply_settings(app: AppHandle, state: tauri::State<'_, Speech>) -> Result<SpeechStatus, String> {
    let config = read_config();
    let current = lock(&state.hotkey).clone();
    if current.as_deref() != Some(config.speech_hotkey.as_str()) || lock(&state.hotkey_error).is_some() {
        register_hotkey(&app, &state, &config.speech_hotkey);
    }
    let running = proc_alive(&mut lock(&state.inner));
    if running {
        configure(&state)?;
    }
    Ok(status(&app, &state))
}

#[tauri::command(async)]
pub fn speech_engine_start(app: AppHandle, state: tauri::State<'_, Speech>) -> Result<(), String> {
    ensure_configured(&app, &state)
}

#[tauri::command(async)]
pub fn speech_engine_stop(app: AppHandle, state: tauri::State<'_, Speech>) -> Result<(), String> {
    kill(&app, &state);
    Ok(())
}

#[tauri::command(async)]
pub fn speech_engine_logs(state: tauri::State<'_, Speech>, limit: Option<usize>) -> Vec<String> {
    let inner = lock(&state.inner);
    let n = limit.unwrap_or(200).min(inner.logs.len());
    inner.logs.iter().skip(inner.logs.len() - n).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_normalizes_legacy_values() {
        // The Windows engine's `en-US` (its default) must not pin Whisper to English.
        let c: SpeechConfig = serde_json::from_str(r#"{ "speechLanguage": "en-US", "speechDevice": "gpu", "speechInsert": "" }"#).unwrap();
        let c = c.normalized();
        assert_eq!(c.speech_language, "auto");
        assert_eq!(c.speech_device, "auto");
        assert_eq!(c.speech_insert, "paste");
        assert_eq!(c.speech_hotkey, DEFAULT_HOTKEY);
        assert_eq!(c.speech_model, "small");
        let s: SpeechConfig = serde_json::from_str(r#"{ "speechLanguage": "system", "windowPosition": "left" }"#).unwrap();
        assert_eq!(s.normalized().speech_language, "auto");
        for (raw, want) in [("de-DE", "auto"), ("de_DE", "auto"), ("", "auto"), ("Auto", "auto"), ("DE", "de"), (" en ", "en")] {
            assert_eq!(normalize_language(raw), want, "{raw:?}");
        }
        let plain: SpeechConfig = serde_json::from_str("{}").unwrap();
        assert!(plain.normalized().speech_live_preview);
    }

    #[test]
    fn take_in_flight_follows_the_engine_flags_then_the_phase() {
        // The model finished loading while recording: the flag counts, whatever the phase says.
        assert!(take_in_flight(&json!({ "phase": "ready", "recording": true })));
        assert!(take_in_flight(&json!({ "phase": "error", "transcribing": true })));
        assert!(take_in_flight(&json!({ "phase": "recording" })));
        assert!(take_in_flight(&json!({ "phase": "transcribing", "recording": false })));
        assert!(!take_in_flight(&json!({ "phase": "ready", "recording": false, "transcribing": false })));
        assert!(!take_in_flight(&json!({ "phase": "loading" })));
    }

    #[test]
    fn requirements_hash_is_stable_and_distinguishes_cuda() {
        let dir = std::env::temp_dir().join(format!("qs-voice-hash-{}", std::process::id()));
        let pkg = dir.join(SIDECAR_MARKER);
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("requirements.txt"), "faster-whisper>=1.2\n").unwrap();
        std::fs::write(pkg.join("requirements-cuda.txt"), "nvidia-cudnn-cu12\n").unwrap();
        let p = Paths { sidecar: dir.clone(), venv: dir.join("v"), python: dir.join("v/p"), models: dir.join("m"), marker: dir.join("v/.m") };
        let a = requirements_hash(&p, false);
        let b = requirements_hash(&p, true);
        assert_eq!(a, requirements_hash(&p, false));
        assert_ne!(a, b);
        assert_eq!(a.len(), 16);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
