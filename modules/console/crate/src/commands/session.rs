//! Session lifecycle — the plain-terminal contract (2026-08-20 rollback).
//!
//! Bytes go from the PTY straight to the webview; xterm.js is the terminal.
//! There is no VT parsing, no block model and no persistence on this side —
//! that stack was removed by the user's decision to return to the plain
//! QuantCode-style terminal (see docs/CONSOLE-ROLLBACK.md).
//!
//! Two rules that are easy to break and fail silently:
//!
//!   - every command is `async`. A synchronous `#[tauri::command]` runs on the
//!     main thread, and a PTY write blocks while the child is not reading.
//!   - output is emitted with `emit_to(webview)`, never `emit`. A broadcast
//!     hands every byte to the HUD overlay and every canvas browser webview,
//!     multiplying main-thread work by the number of open webviews.

use crate::state::{ConsoleState, SessionMeta};
use qs_pty::{PtyEvent, PtyOptions, PtySession};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// Output event payload. An empty `data` means EOF — the child has exited and
/// the stream has drained.
#[derive(Clone, Serialize)]
struct OutputPayload {
    id: String,
    data: String,
}

/// Emitted separately from the byte stream so the UI can mark a pane dead
/// without having to interpret an empty output chunk.
#[derive(Clone, Serialize)]
struct SessionEventPayload {
    id: String,
    state: &'static str,
}

fn publish(app: &AppHandle, topic: &str, payload: serde_json::Value) {
    // Best-effort: the bus is for coordination, and a session must open even if
    // nobody is listening (ARCHITECTURE.md §3).
    if let Err(e) = qs_core::bus::publish(app, qs_core::bus::Event::new(topic, "console", payload)) {
        eprintln!("console: failed to publish {topic}: {e}");
    }
}

/// What a caller has to say to open a session.
///
/// One struct rather than six parameters: the list had grown past what is
/// readable at a call site, and `owner` — the field that keeps a canvas window's
/// shell from becoming a console tab — is exactly the kind of flag that gets
/// passed positionally by accident.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub cwd: String,
    /// Executable; `None` picks the platform default.
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    /// Which surface is asking; defaults to the console's own tabs.
    #[serde(default)]
    pub owner: Option<String>,
    /// argv after the executable. Empty for a plain shell; QuantPilot uses it
    /// to start an agent CLI (`claude --session-id …`) in the same registry.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment for the child, applied over the PTY defaults.
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

#[tauri::command]
pub async fn open_session(
    state: tauri::State<'_, ConsoleState>,
    app_handle: AppHandle,
    webview: tauri::Webview,
    request: OpenSessionRequest,
) -> Result<String, String> {
    spawn_session(&app_handle, &state, webview.label(), request)
}

/// Open a session for any surface — the command above, and QuantPilot's own
/// launch command, which builds the request in Rust and needs the same
/// registry, the same output events and the same owner rule.
///
/// `target` is the webview label the output is emitted to (see the module
/// doc: `emit_to`, never `emit`).
pub fn spawn_session(
    app_handle: &AppHandle,
    state: &ConsoleState,
    target: &str,
    request: OpenSessionRequest,
) -> Result<String, String> {
    let OpenSessionRequest { cwd, shell, cols, rows, owner, args, env } = request;
    let id = Uuid::new_v4().to_string();
    let target = target.to_string();
    let started_at = qs_core::db::now_ms();

    // "." is wherever the *app process* runs — under `tauri dev` that is
    // `apps/src-tauri`, which no terminal user ever asked for. Without a real
    // directory the honest default is the user's home, same as every terminal
    // emulator's.
    let cwd = if cwd.trim().is_empty() || cwd.trim() == "." || !std::path::Path::new(&cwd).is_dir()
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or(cwd)
    } else {
        cwd
    };

    let shell_path = shell
        .clone()
        .unwrap_or_else(|| qs_pty::default_shell().to_string());

    let event_id = id.clone();
    let sink_app = app_handle.clone();
    let session = PtySession::spawn(
        &PtyOptions {
            cwd: cwd.clone(),
            shell: Some(shell_path.clone()),
            args,
            env,
            cols: cols.unwrap_or(80),
            rows: rows.unwrap_or(24),
        },
        Box::new(move |event| match event {
            PtyEvent::Output(text) => {
                let _ = sink_app.emit_to(
                    &target,
                    "console-output",
                    OutputPayload {
                        id: event_id.clone(),
                        data: text,
                    },
                );
            }
            PtyEvent::Dropped { bytes } => {
                // The stream has a hole. xterm renders what arrives; the log is
                // the only place that can say bytes went missing.
                eprintln!("console: {bytes} byte(s) of output dropped for {event_id}");
            }
            PtyEvent::Eof => {
                // Keep the historical contract of an empty chunk *and* say it
                // plainly on the session channel.
                let _ = sink_app.emit_to(
                    &target,
                    "console-output",
                    OutputPayload {
                        id: event_id.clone(),
                        data: String::new(),
                    },
                );
                let _ = sink_app.emit_to(
                    &target,
                    "console-session",
                    SessionEventPayload {
                        id: event_id.clone(),
                        state: "exited",
                    },
                );
                if let Some(state) = sink_app.try_state::<ConsoleState>() {
                    state.mark_exited(&event_id);
                }
            }
        }),
    )?;

    state.ptys.insert(id.clone(), session)?;
    state.insert_meta(SessionMeta {
        id: id.clone(),
        owner: owner.unwrap_or_else(|| "console".to_string()),
        shell: shell_path,
        cwd: cwd.clone(),
        started_at,
        exited: false,
    });

    publish(
        app_handle,
        "console.session.opened",
        serde_json::json!({ "session": id, "cwd": cwd }),
    );

    Ok(id)
}

#[tauri::command]
pub async fn write_session(
    state: tauri::State<'_, ConsoleState>,
    id: String,
    data: String,
) -> Result<(), String> {
    state.ptys.write(&id, data.as_bytes())
}

#[tauri::command]
pub async fn resize_session(
    state: tauri::State<'_, ConsoleState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.ptys.resize(&id, cols, rows)
}

#[tauri::command]
pub async fn close_session(
    state: tauri::State<'_, ConsoleState>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    state.ptys.close(&id)?;
    state.remove_meta(&id);
    publish(
        &app_handle,
        "console.session.closed",
        serde_json::json!({ "session": id }),
    );
    Ok(())
}

/// Stop streaming without killing the child; output buffers in `qs-pty` until
/// the next [`attach_session`].
#[tauri::command]
pub async fn detach_session(
    state: tauri::State<'_, ConsoleState>,
    id: String,
) -> Result<(), String> {
    state.ptys.detach(&id)
}

/// What a re-attaching frontend has to be told about the time it was away.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachResult {
    /// Everything the child printed while nobody listened, verbatim — xterm
    /// replays it and the screen is whole again.
    pub raw: String,
}

/// Coming back: resume streaming and replay what was missed.
///
/// The replay runs inside [`qs_pty::PtyRegistry::attach_with`], which is what
/// keeps it ahead of whatever the child prints in the same millisecond — see
/// there for why the order is not merely tidier.
#[tauri::command]
pub async fn attach_session(
    state: tauri::State<'_, ConsoleState>,
    id: String,
) -> Result<AttachResult, String> {
    let raw: Result<String, String> =
        state.ptys.attach_with(&id, |buffered| Ok(buffered.to_string()))?;
    Ok(AttachResult { raw: raw? })
}

#[tauri::command]
pub async fn session_alive(
    state: tauri::State<'_, ConsoleState>,
    id: String,
) -> Result<bool, String> {
    Ok(state.ptys.contains(&id))
}

/// Live sessions, optionally only those a given surface opened.
///
/// The filter is what keeps the console's tab bar from adopting canvas windows'
/// shells on a reload: both live in the same registry, and only the owner tells
/// them apart.
#[tauri::command]
pub async fn list_sessions(
    state: tauri::State<'_, ConsoleState>,
    owner: Option<String>,
) -> Result<Vec<SessionMeta>, String> {
    let mut sessions = state.sessions();
    if let Some(owner) = owner {
        sessions.retain(|s| s.owner == owner);
    }
    Ok(sessions)
}
