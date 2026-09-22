//! One thread per launched session: watches the PTY for its exit, and —
//! for adapters that keep a session file — finds that file and tails it
//! into signals. Codex appends a rollout as it works, pi and omp append
//! their session file; both are the CLI's own record, read, never written.
//!
//! The file is also how Codex's and omp's session ids are learned: the
//! newest file in the right folder written after the launch, checked
//! against the cwd where the format carries one.

use crate::adapters::{encode_pi_dir, Store};
use crate::signals::{clip, Signal, Sink};
use qs_mod_console::ConsoleState;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Manager};

/// What to tail: set when the user starts an adapter in the terminal, and
/// again when they start another.
#[derive(Clone, Debug)]
pub struct Target {
    pub store: Store,
    pub known_id: Option<String>,
    pub launched_at: SystemTime,
}

pub struct Watch {
    stop: Arc<AtomicBool>,
    target: Arc<Mutex<Option<Target>>>,
}

impl Watch {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Point the watcher at a (new) session file; the current tail is dropped.
    pub fn set_target(&self, target: Target) {
        if let Ok(mut slot) = self.target.lock() {
            *slot = Some(target);
        }
    }
}

pub struct WatchSpec {
    pub session_id: String,
    pub pty_id: String,
    pub cwd: String,
}

pub fn start(app: AppHandle, sink: Sink, spec: WatchSpec) -> Watch {
    let stop = Arc::new(AtomicBool::new(false));
    let target: Arc<Mutex<Option<Target>>> = Arc::new(Mutex::new(None));
    let flag = stop.clone();
    let slot = target.clone();
    std::thread::Builder::new()
        .name(format!("pilot-watch-{}", &spec.session_id[..8.min(spec.session_id.len())]))
        .spawn(move || run(&app, &sink, spec, &flag, &slot))
        .ok();
    Watch { stop, target }
}

fn pty_alive(app: &AppHandle, pty_id: &str) -> bool {
    app.try_state::<ConsoleState>()
        .map(|s| s.sessions().iter().any(|m| m.id == pty_id && !m.exited))
        .unwrap_or(false)
}

fn run(app: &AppHandle, sink: &Sink, spec: WatchSpec, stop: &AtomicBool, slot: &Mutex<Option<Target>>) {
    let mut tail: Option<Tail> = None;
    let mut target: Option<Target> = None;
    let mut known_id: Option<String> = None;
    let mut last_probe = Instant::now() - Duration::from_secs(10);
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        if !pty_alive(app, &spec.pty_id) {
            sink(&spec.session_id, Signal::State { state: "offline".into(), detail: None });
            sink(&spec.session_id, Signal::Exited);
            return;
        }
        if let Some(next) = slot.lock().ok().and_then(|mut s| s.take()) {
            known_id = next.known_id.clone();
            target = Some(next);
            tail = None;
            last_probe = Instant::now() - Duration::from_secs(10);
        }
        let store = target.as_ref().map(|t| &t.store).filter(|s| !matches!(s, Store::None));
        if let (Some(store), Some(t)) = (store, target.as_ref()) {
            if tail.is_none() && last_probe.elapsed() >= Duration::from_millis(700) {
                last_probe = Instant::now();
                if let Some((path, kind)) = discover(store, known_id.as_deref(), &spec.cwd, t.launched_at) {
                    // A resumed session's file holds its whole history; the
                    // face only wants what happens from now on.
                    tail = Some(if known_id.is_some() { Tail::at_end(path, kind) } else { Tail::new(path, kind) });
                }
            }
        }
        if let Some(t) = tail.as_mut() {
            for line in t.read_lines() {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                for s in t.map(&v) {
                    if let Signal::Bound { provider_session_id, .. } = &s {
                        known_id = Some(provider_session_id.clone());
                    }
                    sink(&spec.session_id, s);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(350));
    }
}

// ── Finding the file ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TailKind {
    Codex,
    Pi,
}

fn norm_path(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false))
        .collect()
}

fn newest_since(files: Vec<PathBuf>, since: SystemTime) -> Vec<PathBuf> {
    let cutoff = since.checked_sub(Duration::from_secs(5)).unwrap_or(since);
    let mut dated: Vec<(SystemTime, PathBuf)> = files
        .into_iter()
        .filter_map(|p| mtime(&p).map(|t| (t, p)))
        .filter(|(t, _)| *t >= cutoff)
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0));
    dated.into_iter().map(|(_, p)| p).collect()
}

/// Sub-directories sorted by name, newest (highest) first.
fn subdirs_desc(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs.reverse();
    dirs
}

/// Codex's `sessions/YYYY/MM/DD` folders, newest first, at most `limit`.
fn codex_day_dirs(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for year in subdirs_desc(root) {
        for month in subdirs_desc(&year) {
            for day in subdirs_desc(&month) {
                out.push(day);
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}

fn first_line_json(path: &Path) -> Option<Value> {
    let mut f = File::open(path).ok()?;
    let mut buf = vec![0u8; 16 * 1024];
    let n = f.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let line = text.lines().next()?;
    serde_json::from_str(line).ok()
}

pub fn codex_cwd_of(first: &Value) -> Option<String> {
    first.get("payload").and_then(|p| p.get("cwd")).and_then(Value::as_str).map(str::to_string)
}

fn discover(store: &Store, known_id: Option<&str>, cwd: &str, launched_at: SystemTime) -> Option<(PathBuf, TailKind)> {
    match store {
        Store::None => None,
        Store::Codex(root) => {
            if let Some(id) = known_id {
                let suffix = format!("-{id}.jsonl");
                // A resumed session keeps writing to its original day's file.
                for day in codex_day_dirs(root, 400) {
                    if let Some(p) = jsonl_files(&day).into_iter().find(|p| p.to_string_lossy().ends_with(&suffix)) {
                        return Some((p, TailKind::Codex));
                    }
                }
                return None;
            }
            let want = norm_path(cwd);
            for day in codex_day_dirs(root, 2) {
                for p in newest_since(jsonl_files(&day), launched_at) {
                    let Some(first) = first_line_json(&p) else {
                        continue;
                    };
                    if codex_cwd_of(&first).map(|c| norm_path(&c)) == Some(want.clone()) {
                        return Some((p, TailKind::Codex));
                    }
                }
            }
            None
        }
        Store::PiLike(root) => {
            let dir = root.join(encode_pi_dir(cwd));
            if let Some(id) = known_id {
                let suffix = format!("_{id}.jsonl");
                return jsonl_files(&dir)
                    .into_iter()
                    .find(|p| p.to_string_lossy().ends_with(&suffix))
                    .map(|p| (p, TailKind::Pi));
            }
            newest_since(jsonl_files(&dir), launched_at).into_iter().next().map(|p| (p, TailKind::Pi))
        }
    }
}

// ── Tailing ──

#[derive(Default)]
pub struct MapState {
    /// Tool call id → name, so a result can name what finished.
    calls: HashMap<String, String>,
    last_tool: Option<String>,
}

pub struct Tail {
    path: PathBuf,
    offset: u64,
    remainder: String,
    kind: TailKind,
    state: MapState,
}

impl Tail {
    pub fn new(path: PathBuf, kind: TailKind) -> Self {
        Self { path, offset: 0, remainder: String::new(), kind, state: MapState::default() }
    }

    /// Start after what is already written.
    pub fn at_end(path: PathBuf, kind: TailKind) -> Self {
        let offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self { path, offset, remainder: String::new(), kind, state: MapState::default() }
    }

    pub fn read_lines(&mut self) -> Vec<String> {
        let Ok(mut f) = File::open(&self.path) else {
            return Vec::new();
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Rewritten from scratch: start over.
            self.offset = 0;
            self.remainder.clear();
        }
        if len == self.offset {
            return Vec::new();
        }
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset += buf.len() as u64;
        self.remainder.push_str(&String::from_utf8_lossy(&buf));
        let mut lines = Vec::new();
        while let Some(i) = self.remainder.find('\n') {
            let line = self.remainder[..i].trim_end_matches('\r').to_string();
            self.remainder.drain(..=i);
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        lines
    }

    pub fn map(&mut self, v: &Value) -> Vec<Signal> {
        match self.kind {
            TailKind::Codex => map_codex_line(v, &mut self.state),
            TailKind::Pi => map_pi_line(v, &mut self.state),
        }
    }
}

fn state(state: &str, detail: Option<String>) -> Signal {
    Signal::State { state: state.into(), detail }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// `*** Update File: path` lines of a Codex `apply_patch` body.
pub fn patch_files(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            ["*** Update File:", "*** Add File:", "*** Delete File:", "*** Move to:"]
                .iter()
                .find_map(|p| l.strip_prefix(p))
                .map(|rest| rest.trim().to_string())
        })
        .filter(|p| !p.is_empty())
        .collect()
}

fn codex_tool_title(name: &str, args: &Value) -> String {
    let detail = match name {
        "shell" | "shell_command" | "container.exec" | "exec_command" => args
            .get("command")
            .and_then(|c| match c {
                Value::Array(parts) => {
                    Some(parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
                }
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .or_else(|| str_of(args, "cmd").map(str::to_string)),
        "apply_patch" => {
            let body = str_of(args, "input").or_else(|| str_of(args, "patch")).unwrap_or("");
            let files = patch_files(body);
            (!files.is_empty()).then(|| files.join(", "))
        }
        "read_file" | "view_image" => str_of(args, "path").map(str::to_string),
        _ => None,
    };
    match detail {
        Some(d) if !d.trim().is_empty() => format!("{name}: {}", clip(d.lines().next().unwrap_or(""), 80)),
        _ => name.to_string(),
    }
}

pub fn map_codex_line(v: &Value, st: &mut MapState) -> Vec<Signal> {
    let kind = str_of(v, "type").unwrap_or("");
    let payload = v.get("payload").cloned().unwrap_or(Value::Null);
    let ptype = str_of(&payload, "type").unwrap_or("");
    match kind {
        "session_meta" => {
            let id = str_of(&payload, "id").or_else(|| str_of(&payload, "session_id")).unwrap_or("");
            if id.is_empty() {
                return Vec::new();
            }
            vec![Signal::Bound { provider_session_id: id.into(), model: None }]
        }
        "turn_context" => str_of(&payload, "model")
            .map(|m| {
                vec![Signal::Vitals {
                    model: Some(m.into()),
                    cost_usd: None,
                    context_pct: None,
                    lines_added: None,
                    lines_removed: None,
                    input_tokens: None,
                    output_tokens: None,
                }]
            })
            .unwrap_or_default(),
        "event_msg" => match ptype {
            "task_started" => vec![Signal::Turn { status: "started".into() }, state("thinking", None)],
            "task_complete" | "turn_aborted" => vec![Signal::Turn { status: "completed".into() }, state("idle", None)],
            "exec_approval_request" | "apply_patch_approval_request" | "request_user_input" | "elicitation_request" => {
                let detail = str_of(&payload, "reason")
                    .or_else(|| str_of(&payload, "message"))
                    .map(str::to_string)
                    .or_else(|| st.last_tool.clone());
                vec![state("waiting", detail)]
            }
            "token_count" => {
                let info = payload.get("info").cloned().unwrap_or(Value::Null);
                let total = info.get("total_token_usage").cloned().unwrap_or(Value::Null);
                let last = info.get("last_token_usage").cloned().unwrap_or(Value::Null);
                let window = info.get("model_context_window").and_then(Value::as_f64);
                let used = last.get("total_tokens").and_then(Value::as_f64);
                let context_pct = match (used, window) {
                    (Some(u), Some(w)) if w > 0.0 => Some((u / w * 100.0).clamp(0.0, 100.0)),
                    _ => None,
                };
                vec![Signal::Vitals {
                    model: None,
                    cost_usd: None,
                    context_pct,
                    lines_added: None,
                    lines_removed: None,
                    input_tokens: total.get("input_tokens").and_then(Value::as_i64),
                    output_tokens: total.get("output_tokens").and_then(Value::as_i64),
                }]
            }
            "error" | "stream_error" => {
                let msg = str_of(&payload, "message").unwrap_or("error").to_string();
                vec![state("error", Some(clip(&msg, 160)))]
            }
            _ => Vec::new(),
        },
        "response_item" => match ptype {
            "function_call" | "custom_tool_call" => {
                let name = str_of(&payload, "name").unwrap_or("tool").to_string();
                let args: Value = match payload.get("arguments") {
                    Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                    Some(other) => other.clone(),
                    None => payload
                        .get("input")
                        .map(|i| serde_json::json!({ "input": i }))
                        .unwrap_or(Value::Null),
                };
                let title = codex_tool_title(&name, &args);
                if let Some(id) = str_of(&payload, "call_id") {
                    st.calls.insert(id.into(), name.clone());
                }
                st.last_tool = Some(title.clone());
                let mut out = vec![
                    state("working", Some(title.clone())),
                    Signal::Tool { name: name.clone(), title, done: false, is_error: false },
                ];
                if name == "apply_patch" {
                    let body = str_of(&args, "input").or_else(|| str_of(&args, "patch")).unwrap_or("");
                    out.extend(patch_files(body).into_iter().map(|path| Signal::File { path }));
                }
                out
            }
            "function_call_output" | "custom_tool_call_output" => {
                let name = str_of(&payload, "call_id")
                    .and_then(|id| st.calls.remove(id))
                    .unwrap_or_else(|| "tool".into());
                let title = st.last_tool.take().unwrap_or_else(|| name.clone());
                vec![Signal::Tool { name, title, done: true, is_error: false }, state("thinking", None)]
            }
            "reasoning" => vec![state("thinking", None)],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn pi_tool_title(name: &str, args: &Value) -> String {
    let detail = str_of(args, "command")
        .or_else(|| str_of(args, "path"))
        .or_else(|| str_of(args, "file_path"))
        .or_else(|| str_of(args, "pattern"))
        .or_else(|| str_of(args, "query"))
        .or_else(|| str_of(args, "url"));
    match detail {
        Some(d) if !d.trim().is_empty() => format!("{name}: {}", clip(d.lines().next().unwrap_or(""), 80)),
        _ => name.to_string(),
    }
}

fn pi_touched(name: &str, args: &Value) -> Option<String> {
    if !matches!(name, "edit" | "write" | "multi_edit" | "multiedit" | "create" | "patch") {
        return None;
    }
    str_of(args, "path").or_else(|| str_of(args, "file_path")).map(str::to_string)
}

pub fn map_pi_line(v: &Value, st: &mut MapState) -> Vec<Signal> {
    let kind = str_of(v, "type").unwrap_or("");
    match kind {
        "session" => {
            let mut out = Vec::new();
            if let Some(id) = str_of(v, "id") {
                out.push(Signal::Bound { provider_session_id: id.into(), model: None });
            }
            if let Some(t) = str_of(v, "title").filter(|t| !t.trim().is_empty()) {
                out.push(Signal::Title { title: t.trim().into() });
            }
            out
        }
        "title" | "title_change" => str_of(v, "title")
            .filter(|t| !t.trim().is_empty())
            .map(|t| vec![Signal::Title { title: t.trim().into() }])
            .unwrap_or_default(),
        "model_change" => str_of(v, "modelId")
            .or_else(|| str_of(v, "model"))
            .map(|m| {
                vec![Signal::Vitals {
                    model: Some(m.into()),
                    cost_usd: None,
                    context_pct: None,
                    lines_added: None,
                    lines_removed: None,
                    input_tokens: None,
                    output_tokens: None,
                }]
            })
            .unwrap_or_default(),
        "message" => {
            let message = v.get("message").cloned().unwrap_or(Value::Null);
            match str_of(&message, "role").unwrap_or("") {
                "user" => vec![Signal::Turn { status: "started".into() }, state("thinking", None)],
                "assistant" => {
                    let empty = Vec::new();
                    let content = message.get("content").and_then(Value::as_array).unwrap_or(&empty);
                    let calls: Vec<&Value> = content.iter().filter(|c| str_of(c, "type") == Some("toolCall")).collect();
                    let mut out = Vec::new();
                    if let Some(usage) = message.get("usage") {
                        let cost = usage.get("cost").and_then(|c| c.get("total")).and_then(Value::as_f64);
                        let input = usage.get("input").and_then(Value::as_i64);
                        let output = usage.get("output").and_then(Value::as_i64);
                        if cost.is_some() || input.is_some() || output.is_some() {
                            out.push(Signal::Vitals {
                                model: None,
                                cost_usd: cost,
                                context_pct: None,
                                lines_added: None,
                                lines_removed: None,
                                input_tokens: input,
                                output_tokens: output,
                            });
                        }
                    }
                    if calls.is_empty() {
                        out.push(Signal::Turn { status: "completed".into() });
                        out.push(state("idle", None));
                        return out;
                    }
                    for call in calls {
                        let name = str_of(call, "name").unwrap_or("tool").to_string();
                        let args = call.get("arguments").cloned().unwrap_or(Value::Null);
                        let title = pi_tool_title(&name, &args);
                        if let Some(id) = str_of(call, "id") {
                            st.calls.insert(id.into(), title.clone());
                        }
                        st.last_tool = Some(title.clone());
                        out.push(state("working", Some(title.clone())));
                        out.push(Signal::Tool { name: name.clone(), title, done: false, is_error: false });
                        if let Some(path) = pi_touched(&name, &args) {
                            out.push(Signal::File { path });
                        }
                    }
                    out
                }
                "toolResult" => {
                    let is_error = message.get("isError").and_then(Value::as_bool).unwrap_or(false);
                    let title = str_of(&message, "toolCallId")
                        .and_then(|id| st.calls.remove(id))
                        .or_else(|| st.last_tool.take())
                        .unwrap_or_else(|| "tool".into());
                    let name = str_of(&message, "toolName").unwrap_or("tool").to_string();
                    vec![Signal::Tool { name, title, done: true, is_error }, state("thinking", None)]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_rollout_maps_to_bound_turns_and_tools() {
        let mut st = MapState::default();
        let meta = json!({ "type": "session_meta", "payload": { "id": "abc", "cwd": "C:\\Projects\\X" } });
        assert_eq!(map_codex_line(&meta, &mut st)[0], Signal::Bound { provider_session_id: "abc".into(), model: None });
        assert_eq!(codex_cwd_of(&meta).unwrap(), "C:\\Projects\\X");

        let started = json!({ "type": "event_msg", "payload": { "type": "task_started" } });
        assert_eq!(map_codex_line(&started, &mut st)[0], Signal::Turn { status: "started".into() });

        let call = json!({ "type": "response_item", "payload": { "type": "function_call", "name": "shell", "call_id": "c1", "arguments": "{\"command\":[\"bash\",\"-lc\",\"ls\"]}" } });
        let out = map_codex_line(&call, &mut st);
        assert_eq!(out[0], Signal::State { state: "working".into(), detail: Some("shell: bash -lc ls".into()) });

        let patch = json!({ "type": "response_item", "payload": { "type": "custom_tool_call", "name": "apply_patch", "call_id": "c2", "input": "*** Begin Patch\n*** Update File: src/a.rs\n*** Add File: b.md\n*** End Patch" } });
        let out = map_codex_line(&patch, &mut st);
        assert!(out.contains(&Signal::File { path: "src/a.rs".into() }));
        assert!(out.contains(&Signal::File { path: "b.md".into() }));

        let output = json!({ "type": "response_item", "payload": { "type": "function_call_output", "call_id": "c1", "output": "..." } });
        let out = map_codex_line(&output, &mut st);
        assert!(matches!(&out[0], Signal::Tool { name, done: true, .. } if name == "shell"));

        let tokens = json!({ "type": "event_msg", "payload": { "type": "token_count", "info": { "total_token_usage": { "input_tokens": 100, "output_tokens": 20 }, "last_token_usage": { "total_tokens": 50000 }, "model_context_window": 200000 } } });
        let out = map_codex_line(&tokens, &mut st);
        assert!(matches!(&out[0], Signal::Vitals { input_tokens: Some(100), context_pct: Some(p), .. } if (*p - 25.0).abs() < 1e-9));

        let done = json!({ "type": "event_msg", "payload": { "type": "task_complete" } });
        assert_eq!(map_codex_line(&done, &mut st)[0], Signal::Turn { status: "completed".into() });
    }

    #[test]
    fn pi_session_maps_messages_and_tool_calls() {
        let mut st = MapState::default();
        let session = json!({ "type": "session", "version": 3, "id": "s-1", "cwd": "C:\\P", "title": "Fix it" });
        let out = map_pi_line(&session, &mut st);
        assert_eq!(out[0], Signal::Bound { provider_session_id: "s-1".into(), model: None });
        assert_eq!(out[1], Signal::Title { title: "Fix it".into() });

        let user = json!({ "type": "message", "message": { "role": "user", "content": [{ "type": "text", "text": "hi" }] } });
        assert_eq!(map_pi_line(&user, &mut st)[0], Signal::Turn { status: "started".into() });

        let call = json!({ "type": "message", "message": { "role": "assistant", "content": [
            { "type": "thinking", "thinking": "..." },
            { "type": "toolCall", "id": "t1", "name": "edit", "arguments": { "path": "src/x.ts", "oldText": "a", "newText": "b" } }
        ] } });
        let out = map_pi_line(&call, &mut st);
        assert!(out.contains(&Signal::File { path: "src/x.ts".into() }));
        assert!(matches!(&out[0], Signal::State { state, detail: Some(d) } if state == "working" && d == "edit: src/x.ts"));

        let result = json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "t1", "toolName": "edit", "isError": false } });
        let out = map_pi_line(&result, &mut st);
        assert!(matches!(&out[0], Signal::Tool { title, done: true, .. } if title == "edit: src/x.ts"));

        let answer = json!({ "type": "message", "message": { "role": "assistant", "content": [{ "type": "text", "text": "done" }] } });
        let out = map_pi_line(&answer, &mut st);
        assert_eq!(out[0], Signal::Turn { status: "completed".into() });
    }

    #[test]
    fn tail_splits_lines_and_keeps_partials() {
        let dir = std::env::temp_dir().join(format!("qs-pilot-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":").unwrap();
        let mut t = Tail::new(path.clone(), TailKind::Pi);
        assert_eq!(t.read_lines(), vec!["{\"a\":1}"]);
        assert!(t.read_lines().is_empty());
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n").unwrap();
        assert_eq!(t.read_lines(), vec!["{\"b\":2}"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The mappers against real files on this machine — the newest Codex
    /// rollout and the newest pi session. Nothing is asserted about their
    /// content beyond a session id; the point is the shapes the CLIs write
    /// today. `cargo test -p tauri-plugin-pilot real_files -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_files_map_without_surprises() {
        let home = dirs::home_dir().unwrap();
        let mut checked = 0;

        let codex_root = home.join(".codex").join("sessions");
        if let Some(day) = codex_day_dirs(&codex_root, 1).into_iter().next() {
            if let Some(path) = newest_since(jsonl_files(&day), SystemTime::UNIX_EPOCH).into_iter().next() {
                let mut t = Tail::new(path.clone(), TailKind::Codex);
                let mut counts: HashMap<String, usize> = HashMap::new();
                for line in t.read_lines() {
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    for s in t.map(&v) {
                        let key = serde_json::to_value(&s).unwrap()["type"].as_str().unwrap().to_string();
                        *counts.entry(key).or_default() += 1;
                    }
                }
                eprintln!("codex {} → {counts:?}", path.display());
                assert!(counts.contains_key("bound"), "no session_meta in the codex rollout");
                checked += 1;
            }
        }

        let pi_root = home.join(".pi").join("agent").join("sessions");
        let newest_pi = subdirs_desc(&pi_root)
            .into_iter()
            .flat_map(|d| jsonl_files(&d))
            .filter_map(|p| mtime(&p).map(|t| (t, p)))
            .max_by_key(|(t, _)| *t)
            .map(|(_, p)| p);
        if let Some(path) = newest_pi {
            let mut t = Tail::new(path.clone(), TailKind::Pi);
            let mut counts: HashMap<String, usize> = HashMap::new();
            for line in t.read_lines() {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                for s in t.map(&v) {
                    let key = serde_json::to_value(&s).unwrap()["type"].as_str().unwrap().to_string();
                    *counts.entry(key).or_default() += 1;
                }
            }
            eprintln!("pi {} → {counts:?}", path.display());
            assert!(counts.contains_key("bound"), "no session line in the pi file");
            checked += 1;
        }
        eprintln!("{checked} real file(s) checked");
    }

    #[test]
    fn patch_files_reads_every_header() {
        let files = patch_files("*** Begin Patch\n*** Update File: a\n*** Delete File: b\n*** Add File: c\n*** End Patch");
        assert_eq!(files, vec!["a", "b", "c"]);
    }
}
