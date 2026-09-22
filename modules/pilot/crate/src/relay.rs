//! The relay: one loopback HTTP listener the CLIs report to.
//!
//! Claude Code gets a hooks block and a statusLine on its command line
//! (`adapters::claude_settings_json`), each a `curl` to this listener with
//! the CLI's own JSON on stdin. The session id rides in that payload — it
//! is the id the pilot minted — so nothing per session has to be written
//! anywhere. `/signal` takes an already-normalized [`Signal`] for any shim
//! a future adapter ships.
//!
//! Bound to 127.0.0.1 on a port the OS picks; the port goes to every child
//! as `QS_PILOT_PORT`.

use crate::signals::{clip, Signal, Sink};
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Router,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `/clear` inside Claude Code ends the session and starts a new one with a
/// new id, in the same terminal. The two hooks arrive back to back; this is
/// how long the second may trail the first and still count as the same row.
const CLEAR_WINDOW: Duration = Duration::from_secs(5);

struct RelayState {
    sink: Sink,
    /// Claude ids that replaced another (after `/clear`) → the row's id.
    aliases: Mutex<HashMap<String, String>>,
    /// The last `SessionEnd` with reason `clear`: (row id, when).
    last_clear: Mutex<Option<(String, Instant)>>,
}

impl RelayState {
    /// The row a Claude session id belongs to, following `/clear` renames.
    fn row_of(&self, session_id: &str) -> String {
        self.aliases
            .lock()
            .ok()
            .and_then(|a| a.get(session_id).cloned())
            .unwrap_or_else(|| session_id.to_string())
    }

    fn note_clear(&self, row: &str) {
        if let Ok(mut l) = self.last_clear.lock() {
            *l = Some((row.to_string(), Instant::now()));
        }
    }

    /// A fresh id right after a clear adopts the cleared row.
    fn adopt_after_clear(&self, new_id: &str) -> Option<String> {
        let mut guard = self.last_clear.lock().ok()?;
        let (row, when) = guard.take()?;
        if when.elapsed() > CLEAR_WINDOW {
            return None;
        }
        if let Ok(mut a) = self.aliases.lock() {
            a.insert(new_id.to_string(), row.clone());
        }
        Some(row)
    }
}

pub fn start(sink: Sink) -> Result<u16, String> {
    let std_listener =
        std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("relay: could not bind a loopback port: {e}"))?;
    let port = std_listener.local_addr().map_err(|e| e.to_string())?.port();
    std_listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let state = Arc::new(RelayState { sink, aliases: Mutex::new(HashMap::new()), last_clear: Mutex::new(None) });
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("pilot: relay listener failed: {e}");
                return;
            }
        };
        let app = Router::new()
            .route("/launched", get(launched))
            .route("/claude/hook", post(claude_hook))
            .route("/claude/status", post(claude_status))
            .route("/signal", post(signal))
            .with_state(state);
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("pilot: relay stopped: {e}");
        }
    });
    Ok(port)
}

/// `GET /launched?session=<id>&adapter=<id>` — the wrapper in the session's
/// terminal, just before it starts the CLI. A GET with two query fields so
/// every shell can send it without quoting anything.
async fn launched(State(state): State<Arc<RelayState>>, Query(q): Query<HashMap<String, String>>) -> &'static str {
    if let (Some(session), Some(adapter)) = (q.get("session"), q.get("adapter")) {
        if !session.is_empty() && !adapter.is_empty() {
            (state.sink)(session, Signal::Launched { adapter: adapter.clone() });
        }
    }
    ""
}

async fn claude_hook(State(state): State<Arc<RelayState>>, body: String) -> &'static str {
    let Ok(v) = serde_json::from_str::<Value>(&body) else {
        return "";
    };
    let Some(sid) = v.get("session_id").and_then(Value::as_str) else {
        return "";
    };
    let event = v.get("hook_event_name").and_then(Value::as_str).unwrap_or("");
    let reason = v.get("reason").or_else(|| v.get("source")).and_then(Value::as_str).unwrap_or("");
    let mut row = state.row_of(sid);
    if event == "SessionEnd" && reason == "clear" {
        // The terminal lives on; the next SessionStart is the same row.
        state.note_clear(&row);
        return "";
    }
    if event == "SessionStart" && row == sid {
        if let Some(adopted) = state.adopt_after_clear(sid) {
            row = adopted;
        }
    }
    for s in map_claude_hook(&v) {
        (state.sink)(&row, s);
    }
    ""
}

/// The statusLine call: vitals in, one line out — what Claude Code shows
/// under its prompt while it runs inside the pilot.
async fn claude_status(State(state): State<Arc<RelayState>>, body: String) -> String {
    let Ok(v) = serde_json::from_str::<Value>(&body) else {
        return "QuantPilot".into();
    };
    let (signal, line) = map_claude_status(&v);
    if let Some(sid) = v.get("session_id").and_then(Value::as_str) {
        (state.sink)(&state.row_of(sid), signal);
    }
    line
}

/// `{ "session": "<id>", "signal": { "type": "...", ... } }`
async fn signal(State(state): State<Arc<RelayState>>, body: String) -> &'static str {
    let Ok(v) = serde_json::from_str::<Value>(&body) else {
        return "";
    };
    let sid = v.get("session").and_then(Value::as_str);
    let sig = v.get("signal").cloned().and_then(|s| serde_json::from_value::<Signal>(s).ok());
    if let (Some(sid), Some(sig)) = (sid, sig) {
        (state.sink)(sid, sig);
    }
    ""
}

// ── Claude Code's hook payloads → signals ──

fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
}

pub fn claude_tool_title(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str);
    let detail = match name {
        "Bash" => s("command").map(first_line),
        "Edit" | "Write" | "MultiEdit" | "Read" => s("file_path"),
        "NotebookEdit" => s("notebook_path"),
        "Glob" | "Grep" => s("pattern"),
        "Agent" | "Task" => s("description"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query"),
        _ => None,
    };
    match detail {
        Some(d) if !d.trim().is_empty() => format!("{name}: {}", clip(d, 80)),
        _ => name.to_string(),
    }
}

fn touched_file(name: &str, input: &Value) -> Option<String> {
    let key = match name {
        "Edit" | "Write" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    input.get(key).and_then(Value::as_str).map(str::to_string)
}

fn state(state: &str, detail: Option<String>) -> Signal {
    Signal::State { state: state.into(), detail }
}

pub fn map_claude_hook(v: &Value) -> Vec<Signal> {
    let event = v.get("hook_event_name").and_then(Value::as_str).unwrap_or("");
    let tool = v.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    match event {
        "SessionStart" => {
            let sid = v.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
            let model = v.get("model").and_then(Value::as_str).map(str::to_string);
            vec![Signal::Bound { provider_session_id: sid, model }, state("idle", None)]
        }
        "UserPromptSubmit" => vec![Signal::Turn { status: "started".into() }, state("thinking", None)],
        "PreToolUse" => {
            let title = claude_tool_title(tool, &input);
            vec![
                state("working", Some(title.clone())),
                Signal::Tool { name: tool.into(), title, done: false, is_error: false },
            ]
        }
        "PostToolUse" | "PostToolUseFailure" => {
            let is_error = event == "PostToolUseFailure";
            let title = claude_tool_title(tool, &input);
            let mut out = vec![Signal::Tool { name: tool.into(), title, done: true, is_error }];
            if let Some(path) = touched_file(tool, &input) {
                out.push(Signal::File { path });
            }
            out.push(state("thinking", None));
            out
        }
        "PermissionRequest" => vec![state("waiting", Some(claude_tool_title(tool, &input)))],
        "Notification" => {
            let kind = v.get("notification_type").and_then(Value::as_str).unwrap_or("");
            let message = v.get("message").and_then(Value::as_str).unwrap_or("").to_string();
            match kind {
                "permission_prompt" | "elicitation_dialog" => vec![state("waiting", Some(message))],
                "idle_prompt" => vec![state("idle", None)],
                _ if message.is_empty() => Vec::new(),
                _ => vec![Signal::Notice { text: message }],
            }
        }
        "Stop" => vec![Signal::Turn { status: "completed".into() }, state("idle", None)],
        "SessionEnd" => vec![state("offline", None)],
        "PreCompact" => vec![Signal::Notice { text: "compacting the context".into() }],
        _ => Vec::new(),
    }
}

fn f64_of(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn i64_of(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_i64)
}

pub fn map_claude_status(v: &Value) -> (Signal, String) {
    let model = v
        .get("model")
        .and_then(|m| m.get("display_name").or_else(|| m.get("id")))
        .and_then(Value::as_str)
        .map(str::to_string);
    let cost = v.get("cost");
    let ctx = v.get("context_window");
    let cost_usd = f64_of(cost.and_then(|c| c.get("total_cost_usd")));
    let context_pct = f64_of(ctx.and_then(|c| c.get("used_percentage")));
    let signal = Signal::Vitals {
        model: model.clone(),
        cost_usd,
        context_pct,
        lines_added: i64_of(cost.and_then(|c| c.get("total_lines_added"))),
        lines_removed: i64_of(cost.and_then(|c| c.get("total_lines_removed"))),
        input_tokens: i64_of(ctx.and_then(|c| c.get("total_input_tokens"))),
        output_tokens: i64_of(ctx.and_then(|c| c.get("total_output_tokens"))),
    };
    let mut parts = vec!["QuantPilot".to_string()];
    if let Some(m) = model {
        parts.push(m);
    }
    if let Some(p) = context_pct {
        parts.push(format!("ctx {}%", p.round() as i64));
    }
    if let Some(c) = cost_usd {
        parts.push(format!("${c:.2}"));
    }
    (signal, parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pre_tool_use_is_working_with_the_command() {
        let v = json!({
            "session_id": "s1", "hook_event_name": "PreToolUse", "tool_name": "Bash",
            "tool_input": { "command": "cargo test\n# second line" }
        });
        let out = map_claude_hook(&v);
        assert_eq!(out[0], Signal::State { state: "working".into(), detail: Some("Bash: cargo test".into()) });
        assert!(matches!(&out[1], Signal::Tool { name, done: false, .. } if name == "Bash"));
    }

    #[test]
    fn post_tool_use_on_an_edit_reports_the_file() {
        let v = json!({
            "session_id": "s1", "hook_event_name": "PostToolUse", "tool_name": "Edit",
            "tool_input": { "file_path": "C:\\p\\a.rs", "old_string": "x", "new_string": "y" }
        });
        let out = map_claude_hook(&v);
        assert!(out.iter().any(|s| *s == Signal::File { path: "C:\\p\\a.rs".into() }));
        assert_eq!(out.last(), Some(&Signal::State { state: "thinking".into(), detail: None }));
    }

    #[test]
    fn notifications_map_to_waiting_or_idle() {
        let waiting = map_claude_hook(&json!({ "session_id": "s", "hook_event_name": "Notification", "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash" }));
        assert!(matches!(&waiting[0], Signal::State { state, .. } if state == "waiting"));
        let idle = map_claude_hook(&json!({ "session_id": "s", "hook_event_name": "Notification", "notification_type": "idle_prompt" }));
        assert!(matches!(&idle[0], Signal::State { state, .. } if state == "idle"));
        let stop = map_claude_hook(&json!({ "session_id": "s", "hook_event_name": "Stop" }));
        assert_eq!(stop[0], Signal::Turn { status: "completed".into() });
    }

    /// The relay, driven by the very command line the hooks run: `curl`
    /// as it exists on this machine, JSON on stdin, the status line back.
    #[test]
    fn relay_accepts_the_curl_the_hooks_use() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::sync::Mutex;

        if Command::new("curl").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err() {
            eprintln!("skipped: no curl on this machine");
            return;
        }
        let got: Arc<Mutex<Vec<(String, Signal)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: Sink = {
            let got = got.clone();
            Arc::new(move |sid: &str, s: Signal| got.lock().unwrap().push((sid.to_string(), s)))
        };
        let port = start(sink).expect("relay starts");

        let post = |path: &str, payload: &str| -> String {
            let mut child = Command::new("curl")
                .args(["-s", "-X", "POST", "-H", "Content-Type: application/json", "--data-binary", "@-"])
                .arg(format!("http://127.0.0.1:{port}{path}"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("curl spawns");
            child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success(), "curl failed for {path}");
            String::from_utf8_lossy(&out.stdout).into_owned()
        };

        let hook = post(
            "/claude/hook",
            r#"{"session_id":"s-test","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test"}}"#,
        );
        assert_eq!(hook, "");
        // `/clear`: the end of s-test and the start of s-new are one row.
        post("/claude/hook", r#"{"session_id":"s-test","hook_event_name":"SessionEnd","reason":"clear"}"#);
        post("/claude/hook", r#"{"session_id":"s-new","hook_event_name":"SessionStart","source":"clear"}"#);
        post("/claude/hook", r#"{"session_id":"s-new","hook_event_name":"UserPromptSubmit"}"#);
        let line = post(
            "/claude/status",
            r#"{"session_id":"s-test","model":{"display_name":"Opus"},"cost":{"total_cost_usd":0.1},"context_window":{"used_percentage":10}}"#,
        );
        assert_eq!(line, "QuantPilot · Opus · ctx 10% · $0.10");
        post("/signal", r#"{"session":"s-test","signal":{"type":"file","path":"x.rs"}}"#);
        // The wrapper's report, as cmd/PowerShell/bash send it.
        let out = Command::new("curl")
            .args(["-s", &format!("http://127.0.0.1:{port}/launched?session=s-test&adapter=claude")])
            .output()
            .expect("curl get");
        assert!(out.status.success());

        let got = got.lock().unwrap();
        assert!(got.iter().any(|(sid, s)| sid == "s-test" && *s == Signal::Launched { adapter: "claude".into() }));
        assert!(got.iter().any(|(sid, s)| sid == "s-test" && matches!(s, Signal::State { state, .. } if state == "working")));
        assert!(got.iter().any(|(_, s)| matches!(s, Signal::Vitals { cost_usd: Some(c), .. } if (*c - 0.1).abs() < 1e-9)));
        assert!(got.iter().any(|(_, s)| *s == Signal::File { path: "x.rs".into() }));
        assert!(
            got.iter().any(|(sid, s)| sid == "s-test" && *s == Signal::Bound { provider_session_id: "s-new".into(), model: None }),
            "the new id after /clear must bind to the old row"
        );
        assert!(got.iter().any(|(sid, s)| sid == "s-test" && *s == Signal::Turn { status: "started".into() }));
        assert!(!got.iter().any(|(sid, _)| sid == "s-new"), "nothing may be reported under the replaced id");
        assert!(!got.iter().any(|(sid, s)| sid == "s-test" && matches!(s, Signal::State { state, .. } if state == "offline")));
    }

    /// The whole Claude path, live: a one-turn `claude -p` started with the
    /// generated `--settings` (hooks + statusLine), reporting to this relay.
    /// Costs one short model turn — run on purpose:
    /// `cargo test -p tauri-plugin-pilot live_claude -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_claude_hooks_report_to_the_relay() {
        use std::sync::Mutex;
        use std::time::Duration;

        let Some(exe) = crate::providers::which("claude") else {
            eprintln!("skipped: no claude on PATH");
            return;
        };
        let got: Arc<Mutex<Vec<(String, Signal)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: Sink = {
            let got = got.clone();
            Arc::new(move |sid: &str, s: Signal| got.lock().unwrap().push((sid.to_string(), s)))
        };
        let port = start(sink).expect("relay starts");
        let session = uuid::Uuid::new_v4().to_string();
        let mut cmd = crate::providers::base_cmd(&exe);
        cmd.args(["-p", "Reply with exactly the word ok and nothing else."])
            .args(["--session-id", &session])
            .args(["--model", "haiku"])
            .args(["--max-turns", "1"])
            .args(["--permission-mode", "default"])
            .args(["--settings", &crate::adapters::claude_settings_json(port)]);
        let (ok, text) = crate::providers::run_capture(cmd, Duration::from_secs(180)).expect("claude ran");
        eprintln!("claude -p exit ok={ok}; output: {}", text.trim());
        let got = got.lock().unwrap();
        let mine: Vec<&Signal> = got.iter().filter(|(sid, _)| *sid == session).map(|(_, s)| s).collect();
        eprintln!("signals for {session}: {mine:#?}");
        assert!(ok, "claude -p failed: {text}");
        assert!(
            mine.iter().any(|s| matches!(s, Signal::Turn { status } if status == "started"))
                || mine.iter().any(|s| matches!(s, Signal::Bound { .. })),
            "no hook reached the relay — hooks via --settings did not fire"
        );
    }

    #[test]
    fn status_line_becomes_vitals_and_text() {
        let v = json!({
            "session_id": "s", "model": { "id": "claude-opus-4-1", "display_name": "Opus 4.1" },
            "cost": { "total_cost_usd": 0.1234, "total_lines_added": 12, "total_lines_removed": 3 },
            "context_window": { "used_percentage": 41.6 }
        });
        let (sig, line) = map_claude_status(&v);
        assert_eq!(line, "QuantPilot · Opus 4.1 · ctx 42% · $0.12");
        assert!(matches!(sig, Signal::Vitals { lines_added: Some(12), context_pct: Some(p), .. } if (p - 41.6).abs() < 1e-9));
    }
}
