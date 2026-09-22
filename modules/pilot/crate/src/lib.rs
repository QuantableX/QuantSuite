//! QuantPilot — the suite's agent harness (docs/PLAN-QUANTPILOT.md, V2).
//!
//! A session is a terminal in the general memory vault or in a registered
//! workspace, from the console's PTY registry. Inside it, `claude`,
//! `codex`, `pi`, `omp`, `opencode`, `gemini` and any custom adapter are
//! wrapper commands that start the real CLI on this row's session — the
//! id, the permission mode, QuantMCP where the CLI takes it, the hooks,
//! the resume flag — and the user decides what runs by typing it.
//!
//! What the face and the panel know comes back as [`signals::Signal`]s:
//! from Claude's hooks through the relay, from a Codex or pi session file
//! through the watcher, or not at all — then the UI reads output activity
//! and the bell. This file is the plugin glue: commands, the session list,
//! the sink that forwards signals to the webview.

mod adapters;
mod contexts;
mod db;
mod prompt;
mod providers;
mod relay;
mod settings;
mod shell;
mod signals;
mod watch;

use adapters::{LaunchInput, SessionFiles};
use contexts::Context;
use db::{Database, SessionMeta};
use providers::AdapterStatus;
use qs_mod_console::{ConsoleState, OpenSessionRequest};
use serde::{Deserialize, Serialize};
use serde_json::json;
use settings::Settings;
use signals::{Signal, Sink};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, State, Wry,
};

/// The Tauri event every signal rides on: `{ sessionId, event }`.
const EVENT_NAME: &str = "pilot-event";
const PTY_OWNER: &str = "pilot";

/// A launched session: its PTY in the console registry and its watcher.
struct Live {
    pty_id: String,
    watch: watch::Watch,
    display: String,
}

pub struct PilotState {
    data_dir: PathBuf,
    db: Arc<Database>,
    settings: Mutex<Settings>,
    relay_port: u16,
    live: Mutex<HashMap<String, Live>>,
    sink: Sink,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// QuantMCP's port: 3100 in release, 3101 in dev builds — the mcp module's
/// own rule, so a dev suite talks to its own server.
fn mcp_url() -> String {
    let port: u16 = if cfg!(debug_assertions) { 3101 } else { 3100 };
    format!("http://localhost:{port}/mcp")
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    session_id: &'a str,
    event: &'a Signal,
}

fn context_of(meta: &SessionMeta) -> Context {
    Context {
        id: meta.context_id.clone(),
        name: meta.context_name.clone(),
        path: meta.cwd.clone(),
        kind: if meta.context_id == contexts::GENERAL_ID { "general".into() } else { "workspace".into() },
    }
}

fn pty_alive(console: &ConsoleState, pty_id: &str) -> bool {
    console.sessions().iter().any(|m| m.id == pty_id && !m.exited)
}

// ── Persistence: what the panel shows when nothing is running ──

fn persist(db: &Database, sid: &str, signal: &Signal) {
    let result: Result<(), String> = match signal {
        Signal::Bound { provider_session_id, .. } => {
            db.set_provider_session(sid, provider_session_id).map_err(|e| e.to_string())
        }
        // The model a CLI reports is a display name, not a launch value —
        // the row's own `model` stays what the user set.
        Signal::Vitals { cost_usd, input_tokens, output_tokens, .. } => {
            db.set_usage(sid, *input_tokens, *output_tokens, *cost_usd).map_err(|e| e.to_string())
        }
        Signal::Title { title } => match db.get_session(sid) {
            Ok(Some(meta)) if meta.title.trim().is_empty() => db.set_title(sid, title).map_err(|e| e.to_string()),
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
        Signal::State { state, detail } if state == "error" => db.set_error(sid, detail.as_deref()).map_err(|e| e.to_string()),
        Signal::Turn { status } if status == "completed" => db.touch(sid).map_err(|e| e.to_string()),
        _ => Ok(()),
    };
    if let Err(e) = result {
        eprintln!("pilot: persisting a signal failed: {e}");
    }
}

/// The user typed an adapter's command in the session's terminal: the row
/// belongs to that adapter now, and the watcher tails its session file.
fn on_launched(state: &PilotState, session_id: &str, adapter_id: &str) {
    let settings = lock(&state.settings).clone();
    let Some(adapter) = adapters::find(&settings, adapter_id) else {
        return;
    };
    let changed = state.db.set_provider(session_id, adapter_id).unwrap_or(false);
    let known = state
        .db
        .get_session(session_id)
        .ok()
        .flatten()
        .and_then(|m| m.provider_session_id)
        .filter(|s| !s.is_empty());
    let known_id = if changed { None } else { known };
    if known_id.is_none() {
        if let Some(binding) = adapter.immediate_binding(session_id) {
            let _ = state.db.set_provider_session(session_id, &binding);
        }
    }
    let _ = state.db.set_error(session_id, None);
    let _ = state.db.touch(session_id);
    if let Some(l) = lock(&state.live).get(session_id) {
        l.watch.set_target(watch::Target { store: adapter.store(), known_id, launched_at: SystemTime::now() });
    }
}

// ── Commands ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PilotStatus {
    adapters: Vec<AdapterStatus>,
    shell: String,
    mcp_url: String,
    general_path: String,
    data_dir: String,
    relay_port: u16,
}

#[tauri::command(async)]
fn pilot_status(app: AppHandle, state: State<'_, PilotState>) -> PilotStatus {
    let settings = lock(&state.settings).clone();
    let adapters = adapters::all(&settings);
    PilotStatus {
        adapters: providers::status_all(&adapters, &settings),
        shell: shell::resolve(&settings.shell),
        mcp_url: mcp_url(),
        general_path: contexts::general_vault(&app).to_string_lossy().into_owned(),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        relay_port: state.relay_port,
    }
}

#[tauri::command(async)]
fn pilot_contexts(app: AppHandle) -> Vec<Context> {
    contexts::list(&app)
}

#[tauri::command(async)]
fn pilot_settings_get(state: State<'_, PilotState>) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command(async)]
fn pilot_settings_set(state: State<'_, PilotState>, settings: Settings) -> Result<Settings, String> {
    let settings = settings.normalized();
    settings::write(&state.data_dir, &settings)?;
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

#[tauri::command(async)]
fn pilot_sessions_list(state: State<'_, PilotState>) -> Result<Vec<SessionMeta>, String> {
    state.db.list_sessions().map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSession {
    context_id: Option<String>,
    mode: Option<String>,
    title: Option<String>,
}

/// A new row: a terminal in a context. Which agent runs in it is decided
/// in the terminal.
#[tauri::command(async)]
fn pilot_session_create(app: AppHandle, state: State<'_, PilotState>, req: CreateSession) -> Result<SessionMeta, String> {
    let settings = lock(&state.settings).clone();
    let context_id = req.context_id.unwrap_or_else(|| contexts::GENERAL_ID.into());
    let ctx = contexts::resolve(&app, &context_id).ok_or_else(|| format!("unknown context '{context_id}'"))?;
    let mode = req.mode.filter(|m| settings::valid_mode(m)).unwrap_or(settings.default_mode);
    let stamp = now();
    let meta = SessionMeta {
        id: uuid::Uuid::new_v4().to_string(),
        provider: String::new(),
        provider_session_id: None,
        title: req.title.unwrap_or_default().trim().to_string(),
        context_id: ctx.id,
        context_name: ctx.name,
        cwd: ctx.path,
        model: String::new(),
        mode,
        created_at: stamp,
        updated_at: stamp,
        cost_usd: 0.0,
        input_tokens: 0,
        output_tokens: 0,
        last_error: None,
    };
    state.db.insert_session(&meta).map_err(|e| e.to_string())?;
    Ok(meta)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionDetail {
    meta: SessionMeta,
    /// The console PTY to attach to, while the process lives.
    pty_id: Option<String>,
    alive: bool,
    display: Option<String>,
}

fn live_of(app: &AppHandle, state: &PilotState, id: &str) -> (Option<String>, bool, Option<String>) {
    let live = lock(&state.live);
    let Some(l) = live.get(id) else {
        return (None, false, None);
    };
    let alive = app.try_state::<ConsoleState>().map(|c| pty_alive(&c, &l.pty_id)).unwrap_or(false);
    (Some(l.pty_id.clone()), alive, Some(l.display.clone()))
}

#[tauri::command(async)]
fn pilot_session_open(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<SessionDetail, String> {
    let meta = state
        .db
        .get_session(&id)
        .map_err(|e| e.to_string())?
        .ok_or("unknown session")?;
    let (pty_id, alive, display) = live_of(&app, &state, &id);
    Ok(SessionDetail { meta, pty_id, alive, display })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    pty_id: String,
    /// The bound adapter's wrapper resumes its session when typed.
    resumed: bool,
    /// The shell and the commands defined in it, for the header.
    display: String,
    /// `true` when the PTY was already running and was handed back as is.
    attached: bool,
}

/// The files a session's wrappers point at, rewritten at every launch.
fn write_session_files(dir: &Path, system_append: &str, mcp_url: Option<&str>, relay_port: u16) -> Result<SessionFiles, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("session dir: {e}"))?;
    let context = dir.join("context.txt");
    std::fs::write(&context, system_append).map_err(|e| format!("context.txt: {e}"))?;
    let mcp = match mcp_url {
        Some(url) => {
            let p = dir.join("mcp.json");
            let cfg = json!({ "mcpServers": { "quantsuite": { "type": "http", "url": url } } });
            qs_core::paths::write_atomic(&p, cfg.to_string().as_bytes()).map_err(|e| format!("mcp.json: {e}"))?;
            Some(p)
        }
        None => None,
    };
    let claude_settings = dir.join("claude-settings.json");
    qs_core::paths::write_atomic(&claude_settings, adapters::claude_settings_json(relay_port).as_bytes())
        .map_err(|e| format!("claude-settings.json: {e}"))?;
    Ok(SessionFiles { context, mcp, claude_settings })
}

/// Open the session's terminal — or hand back the PTY it already runs in.
/// A dead PTY (the shell exited, the app did not) is replaced.
#[tauri::command(async)]
fn pilot_session_launch(
    app: AppHandle,
    state: State<'_, PilotState>,
    webview: tauri::Webview,
    id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<LaunchResult, String> {
    let console = app.try_state::<ConsoleState>().ok_or("the console module is not loaded")?;
    let meta = state
        .db
        .get_session(&id)
        .map_err(|e| e.to_string())?
        .ok_or("unknown session")?;

    if let Some(l) = lock(&state.live).get(&id) {
        if pty_alive(&console, &l.pty_id) {
            return Ok(LaunchResult { pty_id: l.pty_id.clone(), resumed: false, display: l.display.clone(), attached: true });
        }
    }
    if let Some(old) = lock(&state.live).remove(&id) {
        old.watch.stop();
        let _ = console.ptys.close(&old.pty_id);
        console.remove_meta(&old.pty_id);
    }

    let settings = lock(&state.settings).clone();
    let ctx = context_of(&meta);
    if ctx.kind == "general" {
        std::fs::create_dir_all(&ctx.path).map_err(|e| format!("general vault: {e}"))?;
    }
    let general = contexts::general_vault(&app);
    let mcp = settings.attach_quantmcp.then(mcp_url);
    let system_append = prompt::system_append(&ctx, &general, mcp.as_deref());
    let dir = shell::session_dir(&state.data_dir, &id);
    let files = write_session_files(&dir, &system_append, mcp.as_deref(), state.relay_port)?;
    let input = LaunchInput {
        meta: &meta,
        ctx: &ctx,
        general_vault: &general,
        mcp_url: mcp.as_deref(),
        settings: &settings,
        files: &files,
    };

    // One wrapper per adapter this machine has; the bound one resumes.
    let mut wrappers = Vec::new();
    let mut resumed = false;
    for adapter in adapters::all(&settings) {
        let Some(exe) = providers::resolve(&adapter, &settings) else {
            continue;
        };
        let (w, r) = adapters::wrapper(&adapter, &exe, &input);
        if adapter.id == meta.provider {
            resumed = r;
        }
        wrappers.push(w);
    }
    let names: Vec<&str> = wrappers.iter().map(|w| w.command.as_str()).collect();
    let commands = if names.is_empty() { "no agent CLI found".to_string() } else { names.join(", ") };

    let shell_path = shell::resolve(&settings.shell);
    let kind = shell::kind_of(&shell_path);
    let shell_name = Path::new(&shell_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| shell_path.clone());
    let hint = format!("QuantPilot · this terminal is one session (permissions: {}) · {commands} start here as it", meta.mode);
    let args = match shell::init_file_name(kind) {
        Some(name) => {
            let init = dir.join(name);
            std::fs::write(&init, shell::init_script(kind, &id, state.relay_port, &wrappers, &hint))
                .map_err(|e| format!("{name}: {e}"))?;
            shell::shell_args(kind, &init, &hint)
        }
        None => Vec::new(),
    };
    let env = vec![
        ("QS_PILOT_SESSION".to_string(), id.clone()),
        ("QS_PILOT_PORT".to_string(), state.relay_port.to_string()),
        ("QS_PILOT_DIR".to_string(), dir.to_string_lossy().into_owned()),
        ("QS_PILOT_VAULT".to_string(), general.to_string_lossy().replace('\\', "/")),
    ];

    let pty_id = qs_mod_console::spawn_session(
        &app,
        &console,
        webview.label(),
        OpenSessionRequest {
            cwd: meta.cwd.clone(),
            shell: Some(shell_path.clone()),
            cols,
            rows,
            owner: Some(PTY_OWNER.into()),
            args,
            env,
        },
    )?;
    let _ = state.db.set_error(&id, None);
    let _ = state.db.touch(&id);

    let watch = watch::start(
        app.clone(),
        state.sink.clone(),
        watch::WatchSpec { session_id: id.clone(), pty_id: pty_id.clone(), cwd: meta.cwd.clone() },
    );
    let display = format!("{shell_name} · {commands}");
    lock(&state.live).insert(id.clone(), Live { pty_id: pty_id.clone(), watch, display: display.clone() });

    (state.sink)(&id, Signal::State { state: "idle".into(), detail: None });
    let _ = qs_core::bus::publish(
        &app,
        qs_core::bus::Event::new("pilot.session.launched", "pilot", json!({ "sessionId": id, "shell": shell_name })),
    );
    Ok(LaunchResult { pty_id, resumed, display, attached: false })
}

fn stop_live(app: &AppHandle, state: &PilotState, id: &str) {
    let Some(l) = lock(&state.live).remove(id) else {
        return;
    };
    l.watch.stop();
    if let Some(console) = app.try_state::<ConsoleState>() {
        let _ = console.ptys.close(&l.pty_id);
        console.remove_meta(&l.pty_id);
    }
    (state.sink)(id, Signal::State { state: "offline".into(), detail: None });
    (state.sink)(id, Signal::Exited);
}

/// End the terminal; the session stays in the list and resumes on the next open.
#[tauri::command(async)]
fn pilot_session_stop(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<(), String> {
    stop_live(&app, &state, &id);
    Ok(())
}

#[tauri::command(async)]
fn pilot_session_delete(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<(), String> {
    stop_live(&app, &state, &id);
    state.db.delete_session(&id).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(shell::session_dir(&state.data_dir, &id));
    Ok(())
}

/// Title now; model and mode at the next launch — the wrappers bake both in.
#[tauri::command(async)]
fn pilot_session_update(
    state: State<'_, PilotState>,
    id: String,
    title: Option<String>,
    model: Option<String>,
    mode: Option<String>,
) -> Result<SessionMeta, String> {
    if let Some(t) = title.as_deref() {
        state.db.set_title(&id, t.trim()).map_err(|e| e.to_string())?;
    }
    if let Some(m) = model.as_deref() {
        state.db.set_model(&id, m.trim()).map_err(|e| e.to_string())?;
    }
    if let Some(m) = mode.as_deref().filter(|m| settings::valid_mode(m)) {
        state.db.set_mode(&id, m).map_err(|e| e.to_string())?;
    }
    state
        .db
        .get_session(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "unknown session".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    pty_id: Option<String>,
    alive: bool,
}

/// Every session's PTY, for a webview that reloaded while the terminals kept running.
#[tauri::command(async)]
fn pilot_runtime_state(app: AppHandle, state: State<'_, PilotState>) -> Result<HashMap<String, RuntimeInfo>, String> {
    let sessions = state.db.list_sessions().map_err(|e| e.to_string())?;
    Ok(sessions
        .iter()
        .map(|m| {
            let (pty_id, alive, _) = live_of(&app, &state, &m.id);
            (m.id.clone(), RuntimeInfo { pty_id, alive })
        })
        .collect())
}

// ── Plugin ──

pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("pilot")
        .setup(|app, _api| {
            let data_dir = qs_core::paths::ensure_module("pilot")?;
            let db = Arc::new(Database::open(&data_dir.join("pilot.db"))?);
            let settings = settings::read(&data_dir);

            let sink: Sink = {
                let app = app.clone();
                let db = db.clone();
                Arc::new(move |session_id: &str, signal: Signal| {
                    persist(&db, session_id, &signal);
                    if let Signal::Launched { adapter } = &signal {
                        if let Some(state) = app.try_state::<PilotState>() {
                            on_launched(&state, session_id, adapter);
                        }
                    }
                    let _ = app.emit(EVENT_NAME, Envelope { session_id, event: &signal });
                    if let Signal::Turn { status } = &signal {
                        if status == "completed" {
                            let _ = qs_core::bus::publish(
                                &app,
                                qs_core::bus::Event::new("pilot.turn.completed", "pilot", json!({ "sessionId": session_id })),
                            );
                        }
                    }
                })
            };

            let relay_port = match relay::start(sink.clone()) {
                Ok(port) => port,
                Err(e) => {
                    eprintln!("pilot: {e} — the wrappers cannot report and Claude Code hooks will not arrive");
                    0
                }
            };

            app.manage(PilotState {
                data_dir,
                db,
                settings: Mutex::new(settings),
                relay_port,
                live: Mutex::new(HashMap::new()),
                sink,
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "pilot".into(),
                    label: "QuantPilot".into(),
                    route: "/pilot".into(),
                    toggle_window: None,
                },
            );
            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            pilot_status,
            pilot_contexts,
            pilot_settings_get,
            pilot_settings_set,
            pilot_sessions_list,
            pilot_session_create,
            pilot_session_open,
            pilot_session_launch,
            pilot_session_stop,
            pilot_session_delete,
            pilot_session_update,
            pilot_runtime_state,
        ]))
        .build()
}
