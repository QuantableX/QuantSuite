//! Import existing CLI conversations (parity 08): Claude Code session logs
//! (`<home>/projects/<project>/<session>.jsonl`) and Codex rollouts
//! (`<home>/sessions|archived_sessions/…/rollout-…-<id>.jsonl[.zst]`), from
//! every home the usage page reads (CLAUDE_CONFIG_DIR, CODEX_HOME, defaults).
//!
//! Read-only: originals are never written, nothing is sent to a model. An
//! import becomes a thread bound to the original native session id, in the
//! conversation's own folder, so the next message resumes it natively.
//! Identity is provider + home + session id: importing again finds the thread
//! (and adds what the source gained since), identical ids in two homes stay
//! two conversations. What a log cannot give — attachments it only names,
//! unreadable or cut lines — is said in the thread, never made up.

use crate::usage::scan::{self, Provider};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Logs summarized for the list, newest first.
const SCAN_LIMIT: usize = 600;
/// Entries one import brings at most; the rest is named in a notice.
pub const ENTRY_LIMIT: usize = 4000;
/// One message or tool body at most (the source keeps the whole).
const TEXT_LIMIT: usize = 256 * 1024;
/// A log larger than this is listed but not imported.
const FILE_LIMIT: u64 = 256 * 1024 * 1024;

static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}
pub fn reset_cancel() {
    CANCEL.store(false, Ordering::SeqCst);
}
pub fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub provider: String,
    /// The CLI home the log lives in (identity, with the session id).
    pub home: String,
    pub path: String,
    pub session_id: String,
    pub cwd: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: u32,
    pub model: Option<String>,
    /// Unreadable lines, a cut last line, a compressed archive.
    pub problems: Vec<String>,
    /// Started by QuantPilot itself.
    pub from_quantpilot: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User { text: String, images: Vec<(String, String)>, missing: Vec<String> },
    Assistant { text: String, uuid: Option<String> },
    Tool { call_id: String, name: String, input: Value, output: Option<String>, is_error: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    pub at: i64,
    pub entry: Entry,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Parsed {
    pub entries: Vec<Timed>,
    pub unreadable: usize,
    pub truncated: bool,
}

fn ms(ts: &Value) -> Option<i64> {
    ts.as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis())
}

fn clip(s: &str) -> String {
    if s.len() <= TEXT_LIMIT {
        return s.to_string();
    }
    let mut end = TEXT_LIMIT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut at {} KiB; the original log keeps the whole)", &s[..end], TEXT_LIMIT / 1024)
}

/// The bytes of a log, `.zst` decompressed.
pub fn read_log(path: &Path) -> std::io::Result<Vec<u8>> {
    if path.extension().is_some_and(|e| e == "zst") {
        let file = std::fs::File::open(path)?;
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(std::io::BufReader::new(file)).map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    } else {
        std::fs::read(path)
    }
}

fn lines(bytes: &[u8]) -> (Vec<Value>, usize, bool) {
    let truncated = !bytes.is_empty() && !bytes.ends_with(b"\n");
    let mut rows = Vec::new();
    let mut bad = 0;
    for line in bytes.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<Value>(line) {
            Ok(v) => rows.push(v),
            Err(_) => bad += 1,
        }
    }
    (rows, bad, truncated)
}

fn text_parts(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| matches!(p["type"].as_str(), Some("text" | "input_text" | "output_text")))
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn first_line_title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let t: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        format!("{t}…")
    } else {
        t
    }
}

fn noise(text: &str) -> bool {
    let t = text.trim_start();
    t.is_empty()
        || t.starts_with("<command-")
        || t.starts_with("<local-command")
        || t.starts_with("<environment_context>")
        || t.starts_with("<user_instructions>")
        || t.starts_with("# AGENTS.md")
        || t.starts_with("<system-reminder>")
}

// ── Claude Code ──

pub fn parse_claude(bytes: &[u8]) -> Parsed {
    let (rows, unreadable, truncated) = lines(bytes);
    let mut out = Parsed { unreadable, truncated, ..Default::default() };
    let mut tools: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        if r["isSidechain"] == true || r["isMeta"] == true {
            continue;
        }
        let at = ms(&r["timestamp"]).unwrap_or(0);
        let content = &r["message"]["content"];
        match r["type"].as_str() {
            Some("user") => {
                // Tool results belong to their call.
                if let Value::Array(parts) = content {
                    for p in parts.iter().filter(|p| p["type"] == "tool_result") {
                        let id = p["tool_use_id"].as_str().unwrap_or_default();
                        if let Some(&i) = tools.get(id) {
                            if let Entry::Tool { output, is_error, .. } = &mut out.entries[i].entry {
                                *output = Some(clip(&text_parts(&p["content"])));
                                *is_error = p["is_error"] == true;
                            }
                        }
                    }
                }
                let text = text_parts(content);
                let images: Vec<(String, String)> = content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|p| p["type"] == "image" && p["source"]["type"] == "base64")
                    .filter_map(|p| Some((p["source"]["media_type"].as_str()?.to_string(), p["source"]["data"].as_str()?.to_string())))
                    .collect();
                if noise(&text) && images.is_empty() {
                    continue;
                }
                out.entries.push(Timed { at, entry: Entry::User { text: clip(text.trim()), images, missing: Vec::new() } });
            }
            Some("assistant") => {
                let uuid = r["uuid"].as_str().map(str::to_string);
                let text = text_parts(content);
                if !text.trim().is_empty() {
                    out.entries.push(Timed { at, entry: Entry::Assistant { text: clip(text.trim()), uuid: uuid.clone() } });
                }
                for p in content.as_array().into_iter().flatten().filter(|p| p["type"] == "tool_use") {
                    let call_id = p["id"].as_str().unwrap_or_default().to_string();
                    tools.insert(call_id.clone(), out.entries.len());
                    out.entries.push(Timed {
                        at,
                        entry: Entry::Tool { call_id, name: p["name"].as_str().unwrap_or("tool").to_string(), input: p["input"].clone(), output: None, is_error: false },
                    });
                }
                // The resume point is the last assistant row, text or not.
                if text.trim().is_empty() {
                    if let Some(Timed { entry: Entry::Assistant { uuid: u, .. }, .. }) = out.entries.iter_mut().rev().find(|e| matches!(e.entry, Entry::Assistant { .. })) {
                        if uuid.is_some() {
                            *u = uuid;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn summarize_claude(home: &Path, path: &Path, bytes: &[u8]) -> Option<Summary> {
    let (rows, bad, truncated) = lines(bytes);
    let session_id = rows.iter().find_map(|r| r["sessionId"].as_str()).map(str::to_string).or_else(|| path.file_stem()?.to_str().map(str::to_string))?;
    let cwd = rows.iter().find_map(|r| r["cwd"].as_str()).unwrap_or_default().to_string();
    let main = |r: &&Value| r["isSidechain"] != true && r["isMeta"] != true;
    let titled = rows.iter().rev().find(|r| r["type"] == "summary").and_then(|r| r["summary"].as_str()).map(str::to_string);
    let first_user = rows
        .iter()
        .filter(main)
        .filter(|r| r["type"] == "user")
        .map(|r| text_parts(&r["message"]["content"]))
        .find(|t| !noise(t))
        .map(|t| first_line_title(&t));
    let messages = rows.iter().filter(main).filter(|r| matches!(r["type"].as_str(), Some("user" | "assistant"))).count() as u32;
    if messages == 0 {
        return None;
    }
    let times: Vec<i64> = rows.iter().filter_map(|r| ms(&r["timestamp"])).collect();
    let mut problems = Vec::new();
    if bad > 0 {
        problems.push(format!("{bad} unreadable line(s) will be skipped"));
    }
    if truncated {
        problems.push("the last line is cut (the session may still be running)".into());
    }
    let from_quantpilot = rows.iter().any(|r| r["entrypoint"] == "sdk-cli" || r["entrypoint"] == "sdk-ts") && cwd.contains(".quantsuite");
    Some(Summary {
        provider: "claude".into(),
        home: home.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        session_id,
        cwd,
        title: titled.or(first_user).unwrap_or_else(|| "Untitled conversation".into()),
        created_at: times.iter().copied().min().unwrap_or(0),
        updated_at: times.iter().copied().max().unwrap_or(0),
        messages,
        model: rows.iter().rev().find_map(|r| r["message"]["model"].as_str()).filter(|m| !m.starts_with('<')).map(str::to_string),
        problems,
        from_quantpilot,
    })
}

// ── Codex ──

pub fn parse_codex(bytes: &[u8]) -> Parsed {
    let (rows, unreadable, truncated) = lines(bytes);
    let mut out = Parsed { unreadable, truncated, ..Default::default() };
    let mut tools: HashMap<String, usize> = HashMap::new();
    for r in rows.iter().filter(|r| r["type"] == "response_item") {
        let at = ms(&r["timestamp"]).unwrap_or(0);
        let p = &r["payload"];
        match p["type"].as_str() {
            Some("message") => {
                let text = text_parts(&p["content"]);
                match p["role"].as_str() {
                    Some("user") => {
                        let mut images = Vec::new();
                        let mut missing = Vec::new();
                        for part in p["content"].as_array().into_iter().flatten().filter(|x| x["type"] == "input_image") {
                            let url = part["image_url"].as_str().unwrap_or_default();
                            match url.strip_prefix("data:").and_then(|d| d.split_once(";base64,")) {
                                Some((mime, data)) => images.push((mime.to_string(), data.to_string())),
                                None => missing.push(url.chars().take(200).collect()),
                            }
                        }
                        if noise(&text) && images.is_empty() {
                            continue;
                        }
                        out.entries.push(Timed { at, entry: Entry::User { text: clip(text.trim()), images, missing } });
                    }
                    Some("assistant") if !text.trim().is_empty() => out.entries.push(Timed { at, entry: Entry::Assistant { text: clip(text.trim()), uuid: None } }),
                    _ => {}
                }
            }
            Some("function_call" | "custom_tool_call" | "local_shell_call") => {
                let call_id = p["call_id"].as_str().unwrap_or_default().to_string();
                let input = match p.get("arguments").and_then(Value::as_str) {
                    Some(a) => serde_json::from_str(a).unwrap_or_else(|_| json!(a)),
                    None => p.get("input").or_else(|| p.get("action")).cloned().unwrap_or(Value::Null),
                };
                tools.insert(call_id.clone(), out.entries.len());
                let name = p["name"].as_str().unwrap_or(if p["type"] == "local_shell_call" { "shell" } else { "tool" }).to_string();
                out.entries.push(Timed { at, entry: Entry::Tool { call_id, name, input, output: None, is_error: false } });
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                let id = p["call_id"].as_str().unwrap_or_default();
                if let Some(&i) = tools.get(id) {
                    let text = match &p["output"] {
                        Value::String(s) => s.clone(),
                        other => other.get("content").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| other.to_string()),
                    };
                    if let Entry::Tool { output, .. } = &mut out.entries[i].entry {
                        *output = Some(clip(&text));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn summarize_codex(home: &Path, path: &Path, bytes: &[u8]) -> Option<Summary> {
    let (rows, bad, truncated) = lines(bytes);
    let meta = rows.iter().find(|r| r["type"] == "session_meta").map(|r| &r["payload"])?;
    // A subagent's rollout belongs to its parent conversation.
    if meta["source"].is_object() && meta["source"].get("subagent").is_some() {
        return None;
    }
    let session_id = meta["id"].as_str().or_else(|| meta["session_id"].as_str())?.to_string();
    let parsed = parse_codex(bytes);
    let messages = parsed.entries.iter().filter(|e| matches!(e.entry, Entry::User { .. } | Entry::Assistant { .. })).count() as u32;
    if messages == 0 {
        return None;
    }
    let title = rows
        .iter()
        .rev()
        .find(|r| r["type"] == "event_msg" && r["payload"]["type"] == "thread_name_updated")
        .and_then(|r| r["payload"]["thread_name"].as_str().or_else(|| r["payload"]["name"].as_str()))
        .map(str::to_string)
        .or_else(|| {
            parsed.entries.iter().find_map(|e| match &e.entry {
                Entry::User { text, .. } if !text.is_empty() => Some(first_line_title(text)),
                _ => None,
            })
        });
    let times: Vec<i64> = rows.iter().filter_map(|r| ms(&r["timestamp"])).collect();
    let mut problems = Vec::new();
    if bad > 0 {
        problems.push(format!("{bad} unreadable line(s) will be skipped"));
    }
    if truncated {
        problems.push("the last line is cut (the session may still be running)".into());
    }
    if path.extension().is_some_and(|e| e == "zst") {
        problems.push("compressed archive (read-only, imported as is)".into());
    }
    Some(Summary {
        provider: "codex".into(),
        home: home.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        session_id,
        cwd: meta["cwd"].as_str().unwrap_or_default().to_string(),
        title: title.unwrap_or_else(|| "Untitled conversation".into()),
        created_at: ms(&meta["timestamp"]).or_else(|| times.iter().copied().min()).unwrap_or(0),
        updated_at: times.iter().copied().max().unwrap_or(0),
        messages,
        model: rows.iter().rev().find(|r| r["type"] == "turn_context").and_then(|r| r["payload"]["model"].as_str()).map(str::to_string),
        problems,
        from_quantpilot: meta["originator"] == "quantpilot",
    })
}

// ── Discovery ──

type Cache = HashMap<PathBuf, (u64, std::time::SystemTime, Option<Summary>)>;
fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn summary_of(provider: Provider, home: &Path, path: &Path) -> Result<Option<Summary>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let key = (meta.len(), meta.modified().map_err(|e| e.to_string())?);
    if let Some((len, modified, s)) = cache().lock().map_err(|e| e.to_string())?.get(path) {
        if (*len, *modified) == key {
            return Ok(s.clone());
        }
    }
    if meta.len() > FILE_LIMIT {
        return Err(format!("larger than {} MiB", FILE_LIMIT / 1024 / 1024));
    }
    let bytes = read_log(path).map_err(|e| e.to_string())?;
    let s = match provider {
        Provider::Claude => summarize_claude(home, path, &bytes),
        Provider::Codex => summarize_codex(home, path, &bytes),
    };
    if let Ok(mut c) = cache().lock() {
        c.insert(path.to_path_buf(), (key.0, key.1, s.clone()));
    }
    Ok(s)
}

/// The home a log belongs to (the parent of `projects` / `sessions`).
fn home_of(provider: Provider, path: &Path) -> PathBuf {
    let marker = match provider {
        Provider::Claude => "projects",
        Provider::Codex => "sessions",
    };
    path.ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == marker || n == "archived_sessions"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Every importable log of `provider`, newest first, with what could not be read.
pub fn discover(provider: &str) -> (Vec<Summary>, Vec<String>) {
    let want = match provider {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return (Vec::new(), vec!["Import from Claude Code or Codex.".into()]),
    };
    let roots: Vec<(Provider, PathBuf)> = scan::roots().into_iter().filter(|(p, _)| *p == want).collect();
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = scan::files(&roots)
        .into_iter()
        .filter(|(_, p)| match want {
            // Session logs sit directly in a project folder; subagent logs deeper.
            Provider::Claude => p.parent().and_then(Path::parent).and_then(Path::file_name).is_some_and(|n| n == "projects"),
            Provider::Codex => true,
        })
        .filter_map(|(_, p)| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (p, t)))
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.1));
    let mut out: Vec<Summary> = Vec::new();
    let mut problems = Vec::new();
    for (path, _) in files.into_iter().take(SCAN_LIMIT) {
        let home = home_of(want, &path);
        match summary_of(want, &home, &path) {
            Ok(Some(s)) => {
                // The same session in one home (an archived copy): list it once.
                if !out.iter().any(|o| o.session_id == s.session_id && o.home == s.home) {
                    out.push(s);
                }
            }
            Ok(None) => {}
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    (out, problems)
}

/// A log's entries for import.
pub fn parse(provider: &str, path: &Path) -> Result<Parsed, String> {
    let len = std::fs::metadata(path).map_err(|_| "The log is gone. Refresh the list.".to_string())?.len();
    if len > FILE_LIMIT {
        return Err(format!("The log is larger than {} MiB.", FILE_LIMIT / 1024 / 1024));
    }
    let bytes = read_log(path).map_err(|e| format!("The log could not be read ({e})."))?;
    Ok(match provider {
        "codex" => parse_codex(&bytes),
        _ => parse_claude(&bytes),
    })
}

// ── Commands ──

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    #[serde(flatten)]
    pub summary: Summary,
    /// The thread this conversation already is in QuantPilot.
    pub thread_id: Option<String>,
    /// `imported` | `native` (QuantPilot started it) | absent.
    pub state: Option<String>,
    /// Why it cannot be imported (folder gone, too large).
    pub unavailable: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPage {
    pub items: Vec<Listed>,
    pub total: usize,
    pub problems: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub imported: Vec<String>,
    /// Already in QuantPilot: the thread, and how many new messages were added.
    pub existing: Vec<(String, usize)>,
    pub failed: Vec<(String, String)>,
    pub cancelled: bool,
}

fn folder_problem(cwd: &str) -> Option<String> {
    if cwd.trim().is_empty() {
        return Some("The log does not record its folder.".into());
    }
    (!Path::new(cwd).is_dir()).then(|| format!("Its folder {cwd} no longer exists; the session can only resume there."))
}

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

#[tauri::command]
pub async fn pilot_import_list(app: tauri::AppHandle, webview: tauri::Webview, provider: String, search: Option<String>, offset: Option<usize>) -> Result<ImportPage, String> {
    crate::browser::guard(&webview)?;
    let search = search.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<crate::PilotState>();
        let (all, problems) = discover(&provider);
        let matching: Vec<Summary> = all
            .into_iter()
            .filter(|s| search.as_ref().is_none_or_match(&[&s.title, &s.cwd, &s.session_id]))
            .collect();
        let total = matching.len();
        let items = matching
            .into_iter()
            .skip(offset.unwrap_or(0))
            .take(50)
            .map(|s| {
                let imported = state.db.imported_thread(&s.provider, &s.home, &s.session_id).ok().flatten();
                let native = if imported.is_none() { state.db.native_thread(&s.provider, &s.session_id).ok().flatten() } else { None };
                let unavailable = folder_problem(&s.cwd);
                Listed {
                    state: imported.as_ref().map(|_| "imported".to_string()).or_else(|| native.as_ref().map(|_| "native".to_string())),
                    thread_id: imported.or(native),
                    unavailable,
                    summary: s,
                }
            })
            .collect();
        Ok(ImportPage { items, total, problems: problems.into_iter().take(20).collect() })
    })
    .await
    .map_err(|e| e.to_string())?
}

trait SearchMatch {
    fn is_none_or_match(&self, fields: &[&str]) -> bool;
}
impl SearchMatch for Option<&String> {
    fn is_none_or_match(&self, fields: &[&str]) -> bool {
        match self {
            None => true,
            Some(q) => fields.iter().any(|f| f.to_lowercase().contains(q.as_str())),
        }
    }
}

#[tauri::command(async)]
pub fn pilot_import_cancel() {
    cancel();
}

/// Import the chosen logs (by path). `context` maps folders outside every
/// registered workspace (`general` by default); a folder inside one goes there.
#[tauri::command]
pub async fn pilot_import(app: tauri::AppHandle, webview: tauri::Webview, provider: String, paths: Vec<String>, context: Option<String>) -> Result<ImportOutcome, String> {
    crate::browser::guard(&webview)?;
    if !matches!(provider.as_str(), "claude" | "codex") {
        return Err("Import from Claude Code or Codex.".into());
    }
    if paths.is_empty() || paths.len() > 50 {
        return Err("Choose 1 to 50 conversations.".into());
    }
    reset_cancel();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::{Emitter, Manager};
        let state = app.state::<crate::PilotState>();
        let mut out = ImportOutcome::default();
        for path in paths {
            if cancelled() {
                out.cancelled = true;
                break;
            }
            match import_one(&app, &state, &provider, Path::new(&path), context.as_deref()) {
                Ok(Imported::New(meta)) => {
                    let _ = app.emit("pilot-thread-created", &meta);
                    out.imported.push(meta.id);
                }
                Ok(Imported::Existing(id, added)) => out.existing.push((id, added)),
                Err(e) if e == CANCELLED => {
                    out.cancelled = true;
                    break;
                }
                Err(e) => out.failed.push((path, e)),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

const CANCELLED: &str = "cancelled";

enum Imported {
    New(Box<crate::db::ThreadMeta>),
    Existing(String, usize),
}

fn import_one(app: &tauri::AppHandle, state: &crate::PilotState, provider: &str, path: &Path, context: Option<&str>) -> Result<Imported, String> {
    let want = if provider == "codex" { Provider::Codex } else { Provider::Claude };
    let home = home_of(want, path);
    let summary = summary_of(want, &home, path)?.ok_or("This log has no conversation to import.")?;
    let parsed = parse(provider, path)?;
    // Already here: bring only what the log gained since.
    if let Some(thread) = state.db.imported_thread(provider, &summary.home, &summary.session_id).map_err(|e| e.to_string())? {
        let known = state.db.user_messages(&thread).unwrap_or(0);
        let added = append(state, &thread, provider, &parsed, known, false)?;
        return Ok(Imported::Existing(thread, added));
    }
    if let Some(thread) = state.db.native_thread(provider, &summary.session_id).map_err(|e| e.to_string())? {
        return Ok(Imported::Existing(thread, 0));
    }
    if let Some(problem) = folder_problem(&summary.cwd) {
        return Err(problem);
    }
    let settings = crate::lock(&state.settings).clone();
    let contexts = crate::contexts::list(app);
    let cwd = norm(&summary.cwd);
    let ctx = contexts
        .iter()
        .filter(|c| c.kind == "workspace")
        .filter(|c| cwd == norm(&c.path) || cwd.starts_with(&format!("{}/", norm(&c.path))))
        .max_by_key(|c| c.path.len())
        .or_else(|| contexts.iter().find(|c| Some(c.id.as_str()) == context))
        .or_else(|| contexts.iter().find(|c| c.id == crate::contexts::GENERAL_ID))
        .cloned()
        .ok_or("No project to put the conversation in.")?;
    let defaults = settings.defaults_for(&ctx.id);
    let at = |t: i64| if t > 0 { t } else { crate::now() };
    let meta = crate::db::ThreadMeta {
        id: uuid::Uuid::new_v4().to_string(),
        provider: provider.to_string(),
        provider_session_id: Some(summary.session_id.clone()),
        title_source: "auto".into(),
        fast: false,
        notice: None,
        tags: Vec::new(),
        parent_id: None,
        title: summary.title.clone(),
        context_id: ctx.id.clone(),
        context_name: ctx.name.clone(),
        cwd: summary.cwd.clone(),
        model: String::new(),
        mode: defaults.mode.clone(),
        created_at: at(summary.created_at),
        updated_at: at(summary.updated_at),
        cost_usd: 0.0,
        input_tokens: 0,
        output_tokens: 0,
        last_error: None,
        kind: "chat".into(),
        effort: String::new(),
        state: "idle".into(),
        context_tokens: None,
        context_window: None,
        last_activity_at: at(summary.updated_at),
        pinned: false,
        sort_key: at(summary.updated_at) as f64,
        settled_at: None,
        snoozed_until: None,
        auto_settle: true,
        workspace_mode: "local".into(),
        base_branch: String::new(),
        branch: String::new(),
        worktree_path: None,
    };
    state.db.insert_session(&meta).map_err(|e| e.to_string())?;
    let result = append(state, &meta.id, provider, &parsed, 0, true).and_then(|_| {
        let mut lines = vec![format!(
            "Imported from {} ({}), session {}. The original log is unchanged; your next message resumes this session.",
            if provider == "codex" { "Codex" } else { "Claude Code" },
            summary.home,
            summary.session_id
        )];
        if let Some(m) = &summary.model {
            lines.push(format!("It last used the model {m}; the thread starts on the default model."));
        }
        lines.extend(summary.problems.iter().map(|p| format!("Note: {p}.")));
        let last = state.db.turn_refs(&meta.id).ok().and_then(|t| t.last().map(|x| x.0.clone())).unwrap_or_default();
        state.runtime.append_item(&meta.id, &last, crate::events::ItemKind::Notice, json!({ "text": lines.join(" ") }));
        state.db.record_import(provider, &summary.home, &summary.session_id, &meta.id, &summary.path).map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        // Nothing half-imported stays behind.
        let _ = state.db.delete_session(&meta.id);
        return Err(e);
    }
    Ok(Imported::New(Box::new(meta)))
}

/// The log's entries after the `known` user messages, as turns of the
/// thread with their original times. Returns the user messages added.
fn append(state: &crate::PilotState, thread: &str, provider: &str, parsed: &Parsed, known: usize, fresh: bool) -> Result<usize, String> {
    use crate::driver::Driver;
    use crate::events::{Item, ItemKind};
    let info_driver: Box<dyn Driver> = if provider == "codex" {
        Box::new(crate::codex::CodexDriver::new(crate::codex::CodexOptions::default()))
    } else {
        Box::new(crate::claude::ClaudeDriver::new())
    };
    let mut seen_users = 0usize;
    let start = parsed
        .entries
        .iter()
        .position(|e| {
            if matches!(e.entry, Entry::User { .. }) {
                seen_users += 1;
                seen_users > known
            } else {
                false
            }
        })
        .unwrap_or(parsed.entries.len());
    let start = if known == 0 { 0 } else { start };
    let todo = &parsed.entries[start..];
    let limit = todo.len().min(ENTRY_LIMIT);
    let mut seq = state.db.next_seq(thread).map_err(|e| e.to_string())?;
    let mut turn: Option<(String, i64, i64, Option<String>)> = None;
    let mut added_users = 0;
    let mut missing_total = 0;
    let finish = |t: &Option<(String, i64, i64, Option<String>)>| -> Result<(), String> {
        if let Some((id, started, ended, provider_ref)) = t {
            state
                .db
                .insert_turn(&crate::db::TurnRow {
                    id: id.clone(),
                    thread_id: thread.to_string(),
                    status: "completed".into(),
                    started_at: *started,
                    ended_at: Some(*ended),
                    duration_ms: None,
                    cost_usd: None,
                    error: None,
                })
                .map_err(|e| e.to_string())?;
            if let Some(r) = provider_ref {
                let _ = state.db.set_provider_ref(id, r);
            }
        }
        Ok(())
    };
    for (n, e) in todo[..limit].iter().enumerate() {
        if n % 200 == 0 && cancelled() {
            return Err(CANCELLED.into());
        }
        let at = if e.at > 0 { e.at } else { crate::now() };
        if matches!(e.entry, Entry::User { .. }) || turn.is_none() {
            finish(&turn)?;
            turn = Some((uuid::Uuid::new_v4().to_string(), at, at, None));
        }
        let t = turn.as_mut().expect("a turn");
        t.2 = t.2.max(at);
        let (kind, data) = match &e.entry {
            Entry::User { text, images, missing } => {
                added_users += 1;
                missing_total += missing.len();
                let mut attachments = Vec::new();
                for (i, (mime, data)) in images.iter().enumerate() {
                    use base64::Engine;
                    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else { continue };
                    if let Ok(saved) = crate::threads::save_image(state, thread, &format!("imported-{}", i + 1), &bytes) {
                        attachments.push(json!({ "kind": "image", "name": format!("image {}", i + 1), "path": saved.path, "mediaType": mime, "width": saved.width, "height": saved.height }));
                    }
                }
                let mut text = text.clone();
                if !missing.is_empty() {
                    text.push_str(&format!("\n\n(The original message referenced {} image(s) the log does not contain: {}.)", missing.len(), missing.join(", ")));
                }
                (ItemKind::User, json!({ "text": text, "attachments": attachments, "provider": provider, "imported": true }))
            }
            Entry::Assistant { text, uuid } => {
                if let Some(u) = uuid {
                    t.3 = Some(u.clone());
                }
                (ItemKind::Assistant, json!({ "text": text, "streaming": false }))
            }
            Entry::Tool { name, input, output, is_error, .. } => {
                let ti = info_driver.tool_info(name, input);
                let pretty = serde_json::to_string_pretty(input).unwrap_or_default();
                (
                    ItemKind::Tool,
                    json!({
                        "name": name,
                        "category": ti.category,
                        "title": ti.title,
                        "input": clip(&pretty).chars().take(4000).collect::<String>(),
                        "output": output.as_deref().map(|o| o.chars().take(8000).collect::<String>()),
                        "status": if output.is_none() { "completed" } else if *is_error { "failed" } else { "completed" },
                        "isError": is_error,
                    }),
                )
            }
        };
        let item = Item { id: uuid::Uuid::new_v4().to_string(), thread_id: thread.to_string(), turn_id: t.0.clone(), seq, kind, parent: None, data, created_at: at, updated_at: at };
        state.db.upsert_item(&item).map_err(|e| e.to_string())?;
        seq += 1;
    }
    finish(&turn)?;
    if !fresh && added_users > 0 {
        let last = state.db.turn_refs(thread).ok().and_then(|t| t.last().map(|x| x.0.clone())).unwrap_or_default();
        state.runtime.append_item(thread, &last, ItemKind::Notice, json!({ "text": format!("{added_users} newer message(s) imported from the original log.") }));
    }
    let mut notes = Vec::new();
    if todo.len() > limit {
        notes.push(format!("{} later entries were not imported (limit {ENTRY_LIMIT} per import; import again for the rest).", todo.len() - limit));
    }
    if parsed.unreadable > 0 {
        notes.push(format!("{} unreadable line(s) of the log were skipped.", parsed.unreadable));
    }
    if missing_total > 0 {
        notes.push(format!("{missing_total} image(s) were only referenced by the log and could not be imported."));
    }
    if !notes.is_empty() && (fresh || added_users > 0) {
        let last = state.db.turn_refs(thread).ok().and_then(|t| t.last().map(|x| x.0.clone())).unwrap_or_default();
        state.runtime.append_item(thread, &last, ItemKind::Notice, json!({ "text": notes.join(" ") }));
    }
    Ok(added_users)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_log() -> String {
        [
            json!({ "type": "summary", "summary": "Fix the parser" }),
            json!({ "type": "user", "sessionId": "s-1", "cwd": "C:/proj ü", "uuid": "u1", "timestamp": "2026-10-01T10:00:00Z", "message": { "role": "user", "content": "Fix the parser please" } }),
            json!({ "type": "assistant", "uuid": "a1", "timestamp": "2026-10-01T10:00:05Z", "message": { "model": "claude-x", "content": [{ "type": "text", "text": "Looking." }, { "type": "tool_use", "id": "t1", "name": "Read", "input": { "file_path": "a.rs" } }] } }),
            json!({ "type": "user", "uuid": "u2", "timestamp": "2026-10-01T10:00:06Z", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}" }] } }),
            json!({ "type": "user", "isSidechain": true, "uuid": "x", "message": { "content": "subagent" } }),
            json!({ "type": "user", "uuid": "u3", "timestamp": "2026-10-01T10:01:00Z", "message": { "role": "user", "content": [{ "type": "text", "text": "Here is a screenshot" }, { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } }] } }),
            json!({ "type": "assistant", "uuid": "a2", "timestamp": "2026-10-01T10:01:09Z", "message": { "content": [{ "type": "text", "text": "Done — ünïcode ✓" }] } }),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n{broken json\n"
    }

    #[test]
    fn claude_logs_keep_order_tools_images_and_say_what_was_skipped() {
        let log = claude_log();
        let p = parse_claude(log.as_bytes());
        assert_eq!(p.unreadable, 1);
        assert!(!p.truncated);
        let kinds: Vec<&str> = p.entries.iter().map(|e| match &e.entry {
            Entry::User { .. } => "user",
            Entry::Assistant { .. } => "assistant",
            Entry::Tool { .. } => "tool",
        }).collect();
        assert_eq!(kinds, ["user", "assistant", "tool", "user", "assistant"], "the sidechain and the tool-result row are not messages");
        match &p.entries[2].entry {
            Entry::Tool { name, output, .. } => assert_eq!((name.as_str(), output.as_deref()), ("Read", Some("fn main() {}"))),
            e => panic!("{e:?}"),
        }
        match &p.entries[3].entry {
            Entry::User { images, .. } => assert_eq!(images[0].0, "image/png"),
            e => panic!("{e:?}"),
        }
        assert_eq!(p.entries[0].at, chrono::DateTime::parse_from_rfc3339("2026-10-01T10:00:00Z").unwrap().timestamp_millis());
        let s = summarize_claude(Path::new("C:/h"), Path::new("C:/h/projects/p/s-1.jsonl"), log.as_bytes()).unwrap();
        assert_eq!((s.session_id.as_str(), s.cwd.as_str(), s.title.as_str()), ("s-1", "C:/proj ü", "Fix the parser"));
        assert_eq!(s.model.as_deref(), Some("claude-x"));
        assert!(s.problems[0].contains("1 unreadable"));
        // A log cut mid-line says so.
        let cut = &log[..log.len() - 3];
        assert!(parse_claude(cut.as_bytes()).truncated);
    }

    #[test]
    fn codex_rollouts_skip_instructions_and_pair_tool_outputs() {
        let log = [
            json!({ "type": "session_meta", "timestamp": "2026-10-02T08:00:00Z", "payload": { "id": "c-9", "cwd": "D:/repo", "originator": "codex_cli_rs", "timestamp": "2026-10-02T08:00:00Z" } }),
            json!({ "type": "response_item", "timestamp": "2026-10-02T08:00:01Z", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "<environment_context>x</environment_context>" }] } }),
            json!({ "type": "response_item", "timestamp": "2026-10-02T08:00:02Z", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "List the files" }, { "type": "input_image", "image_url": "data:image/png;base64,AAAA" }, { "type": "input_image", "image_url": "C:/gone.png" }] } }),
            json!({ "type": "response_item", "timestamp": "2026-10-02T08:00:03Z", "payload": { "type": "function_call", "name": "shell", "arguments": "{\"command\":[\"ls\"]}", "call_id": "f1" } }),
            json!({ "type": "response_item", "timestamp": "2026-10-02T08:00:04Z", "payload": { "type": "function_call_output", "call_id": "f1", "output": "a.txt" } }),
            json!({ "type": "response_item", "timestamp": "2026-10-02T08:00:05Z", "payload": { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "One file." }] } }),
            json!({ "type": "turn_context", "payload": { "model": "gpt-x" } }),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n";
        let p = parse_codex(log.as_bytes());
        assert_eq!(p.entries.len(), 3);
        match &p.entries[0].entry {
            Entry::User { text, images, missing } => {
                assert_eq!(text, "List the files");
                assert_eq!(images.len(), 1);
                assert_eq!(missing, &vec!["C:/gone.png".to_string()]);
            }
            e => panic!("{e:?}"),
        }
        match &p.entries[1].entry {
            Entry::Tool { input, output, .. } => {
                assert_eq!(input["command"][0], "ls");
                assert_eq!(output.as_deref(), Some("a.txt"));
            }
            e => panic!("{e:?}"),
        }
        let s = summarize_codex(Path::new("C:/c"), Path::new("C:/c/sessions/x.jsonl"), log.as_bytes()).unwrap();
        assert_eq!((s.session_id.as_str(), s.cwd.as_str(), s.title.as_str(), s.messages), ("c-9", "D:/repo", "List the files", 2));
        assert_eq!(s.model.as_deref(), Some("gpt-x"));
        assert!(!s.from_quantpilot);
    }

    #[test]
    fn homes_come_from_the_path() {
        assert_eq!(home_of(Provider::Claude, Path::new("C:/u/.claude/projects/p/s.jsonl")), PathBuf::from("C:/u/.claude"));
        assert_eq!(home_of(Provider::Codex, Path::new("C:/u/.codex/archived_sessions/r.jsonl.zst")), PathBuf::from("C:/u/.codex"));
        assert_eq!(home_of(Provider::Codex, Path::new("C:/alt/sessions/2026/10/01/r.jsonl")), PathBuf::from("C:/alt"));
    }
}
