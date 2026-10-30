//! QuantMCP's `quantsuite.pilot.*` tools (card 22): what an agent in a
//! QuantPilot thread can do to its own thread — name it, show a file or a
//! page in the user's inspector, answer with a rendered page — and to the
//! other threads of its workspace: list, read, start, message.
//!
//! The thread is never an argument. Every thread's QuantMCP URL carries a
//! random token of its own (`?pilot=`, minted per app run); QuantMCP puts
//! it into each `quantsuite.pilot.*` call as `pilotToken` (an agent's own
//! value is dropped), and the token names the calling thread. Other threads
//! are reachable by id only inside the caller's workspace.

use crate::db::{Database, ThreadMeta};
use crate::events::{ItemKind, ThreadEvent};
use crate::{browser, lock, mcp_url, threads, PilotState};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

const NOT_A_THREAD: &str = "This tool works only from a QuantPilot thread: its QuantMCP connection names the thread.";
/// A page as the agent wrote it, and with its local images embedded.
const PAGE_MAX: usize = 1024 * 1024;
const PAGE_MAX_EMBEDDED: usize = 8 * 1024 * 1024;
const IMAGE_MAX: u64 = 4 * 1024 * 1024;
/// `read_thread`: messages returned at most, and the cut per message.
const READ_MAX: usize = 60;
const MESSAGE_CLIP: usize = 2000;

/// The token in a thread's QuantMCP URL.
pub fn token_for(state: &PilotState, thread_id: &str) -> String {
    lock(&state.mcp_tokens)
        .entry(thread_id.to_string())
        .or_insert_with(|| uuid::Uuid::new_v4().simple().to_string())
        .clone()
}

/// QuantMCP's URL as one thread's agent gets it.
pub fn thread_mcp_url(state: &PilotState, thread_id: &str) -> String {
    format!("{}?pilot={}", mcp_url(), token_for(state, thread_id))
}

fn caller(state: &PilotState, token: Option<String>) -> Result<ThreadMeta, String> {
    let tokens = lock(&state.mcp_tokens).clone();
    caller_in(&tokens, &state.db, token)
}

/// The thread a token names: thread → token is the table.
fn caller_in(tokens: &HashMap<String, String>, db: &Database, token: Option<String>) -> Result<ThreadMeta, String> {
    let token = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).ok_or(NOT_A_THREAD)?;
    let id = tokens.iter().find(|(_, t)| **t == token).map(|(id, _)| id.clone()).ok_or(NOT_A_THREAD)?;
    db.get_session(&id).map_err(|e| e.to_string())?.ok_or_else(|| "This thread was deleted.".to_string())
}

/// Another thread of the caller's workspace.
fn sibling(db: &Database, me: &ThreadMeta, id: &str) -> Result<ThreadMeta, String> {
    let other = db.get_session(id.trim()).map_err(|e| e.to_string())?;
    match other {
        Some(t) if t.context_id == me.context_id => Ok(t),
        _ => Err(format!("No thread '{}' in {} — list_threads names them.", id.trim(), me.context_name)),
    }
}

/// A thread may message another of its workspace — not itself, and not one that may do more.
fn may_message(me: &ThreadMeta, target: &ThreadMeta) -> Result<(), String> {
    if target.id == me.id {
        return Err("That is this thread — answer here instead.".into());
    }
    if mode_rank(&target.mode) > mode_rank(&me.mode) {
        return Err(format!(
            "\"{}\" runs with more permissions ({}) than this thread ({}); the user can message it in QuantPilot.",
            target.title, target.mode, me.mode
        ));
    }
    Ok(())
}

/// Permission modes from the most careful up: a thread may not hand work to one that may do more.
fn mode_rank(mode: &str) -> u8 {
    match mode {
        "plan" => 0,
        "ask" => 1,
        "edits" => 2,
        "auto" => 3,
        _ => 4,
    }
}

fn on_screen(state: &PilotState, me: &ThreadMeta) -> Result<(), String> {
    if lock(&state.ui_visible).as_deref() == Some(me.id.as_str()) {
        Ok(())
    } else {
        Err("This thread is not on screen in QuantPilot right now, so nothing was opened. Mention the file or address in your answer instead.".into())
    }
}

/// The thread the user sees with its inspector — the frontend reports it.
#[tauri::command(async)]
pub fn pilot_ui_visible(state: State<'_, PilotState>, thread_id: Option<String>) {
    *lock(&state.ui_visible) = thread_id.filter(|t| !t.is_empty());
}

#[tauri::command(async)]
pub fn pilot_agent_set_title(state: State<'_, PilotState>, pilot_token: Option<String>, title: String) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let title = threads::clean_title(&title);
    if title.is_empty() {
        return Err("The title is empty.".into());
    }
    if me.title_source == "user" {
        return Ok(format!("Kept \"{}\": the user named this thread.", me.title));
    }
    if state.db.set_title_from(&me.id, &title, "agent").map_err(|e| e.to_string())? {
        state.runtime.emit(&me.id, ThreadEvent::Title { title: title.clone() });
    }
    Ok(format!("Titled \"{title}\"."))
}

#[tauri::command(async)]
pub fn pilot_agent_open_file(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    path: String,
    line: Option<i64>,
) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let root = PathBuf::from(&me.cwd);
    let wanted = Path::new(path.trim());
    let full = if wanted.is_absolute() { wanted.to_path_buf() } else { root.join(wanted) };
    if !full.is_file() {
        return Err(format!("{} is not a file.", full.display()));
    }
    on_screen(&state, &me)?;
    let shown = full
        .strip_prefix(&root)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| full.to_string_lossy().into_owned());
    let line = line.filter(|l| *l > 0);
    let _ = app.emit("pilot-agent-open", json!({ "threadId": me.id, "kind": "file", "path": shown, "line": line }));
    Ok(match line {
        Some(l) => format!("Opened {shown} at line {l} in the Files tab."),
        None => format!("Opened {shown} in the Files tab."),
    })
}

#[tauri::command(async)]
pub fn pilot_agent_open_preview(app: AppHandle, state: State<'_, PilotState>, pilot_token: Option<String>, url: String) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let url = browser::normalize_url(url.trim(), None)?;
    on_screen(&state, &me)?;
    let _ = app.emit("pilot-agent-open", json!({ "threadId": me.id, "kind": "preview", "url": url.as_str() }));
    Ok(format!("Opened {url} in the Browser tab."))
}

#[tauri::command(async)]
pub fn pilot_agent_render_page(state: State<'_, PilotState>, pilot_token: Option<String>, html: String, title: Option<String>) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let turn = state.runtime.active_turn(&me.id).ok_or("A page is shown during a turn of the thread.")?;
    if html.trim().is_empty() {
        return Err("The page is empty.".into());
    }
    if html.len() > PAGE_MAX {
        return Err(format!("The page is {} KB; at most {} KB.", html.len() / 1024, PAGE_MAX / 1024));
    }
    let (html, embedded) = embed_images(&html, Path::new(&me.cwd));
    if html.len() > PAGE_MAX_EMBEDDED {
        return Err("The page with its images is over 8 MB; use smaller images.".into());
    }
    let title = title.map(|t| t.trim().chars().take(120).collect::<String>()).filter(|t| !t.is_empty());
    state.runtime.append_item(&me.id, &turn, ItemKind::Page, json!({ "title": title, "html": html }));
    Ok(match embedded {
        0 => "Shown in the thread.".to_string(),
        n => format!("Shown in the thread, {n} local image(s) embedded."),
    })
}

/// Local images (`src` paths, relative to the thread's folder or absolute)
/// become data URIs, so the page shows them without any file access.
pub fn embed_images(html: &str, root: &Path) -> (String, usize) {
    static SRC: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = SRC.get_or_init(|| regex::Regex::new(r#"(?i)\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());
    let mut embedded = 0;
    let out = re.replace_all(html, |c: &regex::Captures| {
        let raw = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or_default();
        match image_data_uri(raw, root) {
            Some(uri) => {
                embedded += 1;
                format!("src=\"{uri}\"")
            }
            None => c[0].to_string(),
        }
    });
    (out.into_owned(), embedded)
}

fn image_data_uri(raw: &str, root: &Path) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    if raw.is_empty() || ["http:", "https:", "data:", "blob:", "about:", "//", "#"].iter().any(|p| lower.starts_with(p)) {
        return None;
    }
    let local = raw.strip_prefix("file:///").unwrap_or(raw);
    let p = Path::new(local);
    let full = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    let mime = match full.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    std::fs::metadata(&full).ok().filter(|m| m.is_file() && m.len() <= IMAGE_MAX)?;
    let bytes = std::fs::read(&full).ok()?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command(async)]
pub fn pilot_agent_list_threads(state: State<'_, PilotState>, pilot_token: Option<String>) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let mut rows: Vec<ThreadMeta> = state
        .db
        .list_sessions()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|t| t.context_id == me.context_id && t.kind == "chat")
        .collect();
    rows.sort_by_key(|t| std::cmp::Reverse(t.last_activity_at));
    let label = |provider: &str| crate::adapters::find(&lock(&state.settings), provider).map(|a| a.label).unwrap_or_else(|| provider.to_string());
    let list: Vec<Value> = rows
        .iter()
        .take(100)
        .map(|t| {
            json!({
                "id": t.id,
                "title": if t.title.trim().is_empty() { "New thread" } else { t.title.as_str() },
                "agent": label(&t.provider),
                "state": t.state,
                "mode": t.mode,
                "branch": t.branch,
                "lastActivity": iso(t.last_activity_at),
                "you": t.id == me.id,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({ "workspace": me.context_name, "threads": list })).map_err(|e| e.to_string())
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)).unwrap_or_default()
}

#[tauri::command(async)]
pub fn pilot_agent_read_thread(state: State<'_, PilotState>, pilot_token: Option<String>, thread_id: String, limit: Option<i64>) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let t = sibling(&state.db, &me, &thread_id)?;
    let limit = limit.unwrap_or(20).clamp(1, READ_MAX as i64) as usize;
    let items = state.db.items(&t.id, None, 600).map_err(|e| e.to_string())?;
    Ok(transcript(&t, &items, limit))
}

/// The newest messages of a thread, as text: who said it, what (cut long).
pub fn transcript(t: &ThreadMeta, items: &[crate::events::Item], limit: usize) -> String {
    let messages: Vec<(&str, String)> = items
        .iter()
        .filter(|i| i.parent.is_none())
        .filter_map(|i| {
            let who = match i.kind {
                ItemKind::User => "user",
                ItemKind::Assistant => "assistant",
                ItemKind::Page => "page",
                _ => return None,
            };
            let text = match i.kind {
                ItemKind::Page => format!("(a rendered page{})", i.data["title"].as_str().map(|s| format!(": {s}")).unwrap_or_default()),
                _ => i.data["text"].as_str().unwrap_or_default().trim().to_string(),
            };
            (!text.is_empty()).then_some((who, text))
        })
        .collect();
    let from = messages.len().saturating_sub(limit);
    let title = if t.title.trim().is_empty() { "New thread" } else { t.title.as_str() };
    let mut out = format!("Thread \"{title}\" ({}, {}) — the last {} of {} messages.\n", t.provider, t.state, messages.len() - from, messages.len());
    for (who, text) in &messages[from..] {
        let clipped: String = text.chars().take(MESSAGE_CLIP).collect();
        let more = if text.chars().count() > MESSAGE_CLIP { " …" } else { "" };
        out.push_str(&format!("\n## {who}\n{clipped}{more}\n"));
    }
    out
}

#[tauri::command(async)]
pub fn pilot_agent_start_thread(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    prompt: String,
    title: Option<String>,
    provider: Option<String>,
) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    if prompt.trim().is_empty() {
        return Err("The prompt is empty.".into());
    }
    let meta = threads::create(
        &app,
        &state,
        threads::CreateThread {
            context_id: Some(me.context_id.clone()),
            provider: Some(provider.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).unwrap_or_else(|| me.provider.clone())),
            // The new thread may do what the caller may, no more.
            mode: Some(me.mode.clone()),
            title: title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            ..Default::default()
        },
    )?;
    let _ = app.emit("pilot-thread-created", &meta);
    threads::start_turn(&app, &state, &meta.id, prompt, Vec::new())?;
    Ok(format!("Started thread {} in {}; it works on its own. Look in with read_thread.", meta.id, me.context_name))
}

#[tauri::command(async)]
pub fn pilot_agent_message_thread(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    thread_id: String,
    message: String,
) -> Result<String, String> {
    let me = caller(&state, pilot_token)?;
    let target = sibling(&state.db, &me, &thread_id)?;
    may_message(&me, &target)?;
    if message.trim().is_empty() {
        return Err("The message is empty.".into());
    }
    let from = if me.title.trim().is_empty() { "another thread" } else { me.title.as_str() };
    let text = format!("{}\n\n(Sent by the thread \"{from}\" through QuantMCP.)", message.trim());
    if state.runtime.active_turn(&target.id).is_none() && !state.db.queue_held(&target.id).unwrap_or(false) {
        threads::start_turn(&app, &state, &target.id, text, Vec::new())?;
        Ok("Sent; the thread is working on it.".into())
    } else {
        state.db.queue_add(&target.id, &text).map_err(|e| e.to_string())?;
        threads::emit_queue(&state, &target.id);
        Ok("Queued; the thread takes it after its current turn.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_images_are_embedded_and_remote_ones_kept() {
        let dir = std::env::temp_dir().join(format!("qp-page-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/a.png"), [137u8, 80, 78, 71]).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let html = r#"<img src="img/a.png"><img src='https://x.org/b.png'><img src="data:image/png;base64,AA"><img src="notes.txt"><img src="missing.png">"#;
        let (out, n) = embed_images(html, &dir);
        assert_eq!(n, 1);
        assert!(out.contains(r#"src="data:image/png;base64,iVBORw==""#), "{out}");
        assert!(out.contains("src='https://x.org/b.png'") && out.contains(r#"src="notes.txt""#) && out.contains(r#"src="missing.png""#));
        let abs = dir.join("img/a.png").to_string_lossy().into_owned();
        assert_eq!(embed_images(&format!(r#"<img src="{abs}">"#), Path::new("C:/nowhere")).1, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modes_rank_from_plan_to_full() {
        assert!(mode_rank("plan") < mode_rank("ask") && mode_rank("ask") < mode_rank("edits"));
        assert!(mode_rank("edits") < mode_rank("auto") && mode_rank("auto") < mode_rank("full"));
    }

    fn thread(id: &str, context: &str, mode: &str) -> ThreadMeta {
        serde_json::from_value(json!({
            "id": id, "provider": "claude", "providerSessionId": null, "title": "", "contextId": context, "contextName": "W", "cwd": ".",
            "model": "", "mode": mode, "createdAt": 0, "updatedAt": 0, "costUsd": 0.0, "inputTokens": 0, "outputTokens": 0,
            "lastError": null, "kind": "chat", "effort": "", "titleSource": "auto", "state": "idle", "contextTokens": null,
            "contextWindow": null, "lastActivityAt": 0, "pinned": false, "sortKey": 0.0, "settledAt": null, "snoozedUntil": null,
            "autoSettle": true, "workspaceMode": "local", "baseBranch": "", "branch": "", "worktreePath": null
        }))
        .unwrap()
    }

    #[test]
    fn only_the_token_names_the_caller_and_only_its_workspace_is_reachable() {
        let db = Database::open_in_memory().unwrap();
        for t in [thread("me", "ws", "ask"), thread("peer", "ws", "plan"), thread("boss", "ws", "full"), thread("far", "other", "plan")] {
            db.insert_session(&t).unwrap();
        }
        let tokens: HashMap<String, String> = [("me".to_string(), "tok1".to_string()), ("gone".to_string(), "tok2".to_string())].into();
        assert_eq!(caller_in(&tokens, &db, Some("tok1".into())).unwrap().id, "me");
        for bad in [None, Some(String::new()), Some("me".into()), Some("guess".into())] {
            assert_eq!(caller_in(&tokens, &db, bad).unwrap_err(), NOT_A_THREAD);
        }
        assert_eq!(caller_in(&tokens, &db, Some("tok2".into())).unwrap_err(), "This thread was deleted.");
        let me = caller_in(&tokens, &db, Some(" tok1 ".into())).unwrap();
        assert_eq!(sibling(&db, &me, "peer").unwrap().id, "peer");
        assert!(sibling(&db, &me, "far").unwrap_err().starts_with("No thread 'far'"));
        assert!(sibling(&db, &me, "nope").is_err());
        assert!(may_message(&me, &sibling(&db, &me, "peer").unwrap()).is_ok());
        assert!(may_message(&me, &me).unwrap_err().starts_with("That is this thread"));
        assert!(may_message(&me, &sibling(&db, &me, "boss").unwrap()).unwrap_err().contains("more permissions (full)"));
    }

    #[test]
    fn a_transcript_keeps_the_newest_messages() {
        let mut t = thread("t1", "ws", "full");
        t.title = "Parser".into();
        let item = |n: i64, kind: ItemKind, data: Value| crate::events::Item {
            id: format!("i{n}"),
            thread_id: "t1".into(),
            turn_id: "u".into(),
            seq: n,
            kind,
            parent: None,
            data,
            created_at: n,
            updated_at: n,
        };
        let long = "x".repeat(MESSAGE_CLIP + 10);
        let items = vec![
            item(1, ItemKind::User, json!({ "text": "first" })),
            item(2, ItemKind::Tool, json!({ "title": "ls" })),
            item(3, ItemKind::Assistant, json!({ "text": long })),
            item(4, ItemKind::Page, json!({ "title": "Chart", "html": "<b>" })),
            item(5, ItemKind::User, json!({ "text": "second" })),
        ];
        let out = transcript(&t, &items, 3);
        assert!(out.starts_with("Thread \"Parser\" (claude, idle) — the last 3 of 4 messages."), "{out}");
        assert!(!out.contains("first") && out.contains("(a rendered page: Chart)") && out.contains("## user\nsecond"));
        assert!(out.contains(" …"));
    }
}
