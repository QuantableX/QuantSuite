//! V3 thread commands (docs/PLAN-QUANTPILOT.md §4): create, read, send,
//! answer, interrupt, update — and how a Claude Code process is started for
//! a thread. The process work itself is `runtime.rs`; the protocol is
//! `claude.rs`.

use crate::claude::{self, ClaudeDriver, LaunchOptions, SessionRef};
use crate::codex::{self, CodexDriver, CodexOptions};
use crate::db::{Arrangement, QueueItem, ThreadMeta};
use crate::driver::{Answer, Catalog, ImageInput, TurnInput, TurnStatus};
use crate::events::{Item, ThreadEvent};
use crate::runtime::{self, LaunchPlan};
use crate::settings::{self, Settings};
use crate::{adapters, context_of, contexts, lock, mcp_url, now, prompt, providers, shell, PilotState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

/// The providers a chat thread can run today.
const CHAT_PROVIDERS: [&str; 2] = ["claude", "codex"];

const PAGE: i64 = 100;
const TITLE_MAX: usize = 60;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThread {
    context_id: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
    title: Option<String>,
}

#[tauri::command(async)]
pub fn pilot_thread_create(app: AppHandle, state: State<'_, PilotState>, req: CreateThread) -> Result<ThreadMeta, String> {
    let settings = lock(&state.settings).clone();
    let context_id = req.context_id.unwrap_or_else(|| contexts::GENERAL_ID.into());
    let ctx = contexts::resolve(&app, &context_id).ok_or_else(|| format!("unknown context '{context_id}'"))?;
    let provider = req.provider.unwrap_or_else(|| "claude".into());
    if !CHAT_PROVIDERS.contains(&provider.as_str()) {
        return Err(format!("'{provider}' cannot run a chat thread yet"));
    }
    let stamp = now();
    let title = req.title.unwrap_or_default().trim().to_string();
    let meta = ThreadMeta {
        id: uuid::Uuid::new_v4().to_string(),
        provider,
        provider_session_id: None,
        title_source: if title.is_empty() { "auto".into() } else { "user".into() },
        title,
        context_id: ctx.id,
        context_name: ctx.name,
        cwd: ctx.path,
        model: req.model.unwrap_or_default().trim().to_string(),
        mode: req.mode.filter(|m| settings::valid_mode(m)).unwrap_or(settings.default_mode),
        created_at: stamp,
        updated_at: stamp,
        cost_usd: 0.0,
        input_tokens: 0,
        output_tokens: 0,
        last_error: None,
        kind: "chat".into(),
        effort: req.effort.unwrap_or_default().trim().to_string(),
        state: "idle".into(),
        context_tokens: None,
        context_window: None,
        last_activity_at: stamp,
        pinned: false,
        sort_key: stamp as f64,
        settled_at: None,
        snoozed_until: None,
        auto_settle: true,
    };
    state.db.insert_session(&meta).map_err(|e| e.to_string())?;
    Ok(meta)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    thread: ThreadMeta,
    items: Vec<Item>,
    /// Live events up to this seq are already in `items`.
    seq: u64,
    /// Older items exist before the first one returned.
    has_more: bool,
    turn_id: Option<String>,
    process_alive: bool,
    queue: QueueState,
    /// Whether a message can join the running turn (the provider steers).
    can_steer: bool,
}

/// The thread and a page of its timeline: the newest [`PAGE`] items, or
/// the page before `before_seq` when scrolling back.
#[tauri::command(async)]
pub fn pilot_thread_snapshot(
    state: State<'_, PilotState>,
    id: String,
    before_seq: Option<i64>,
    limit: Option<i64>,
) -> Result<Snapshot, String> {
    let limit = limit.unwrap_or(PAGE).clamp(1, 500);
    let (items, seq) = state.runtime.snapshot(&id, before_seq, limit + 1).map_err(|e| e.to_string())?;
    let thread = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let has_more = items.len() as i64 > limit;
    let items = if has_more { items[1..].to_vec() } else { items };
    let can_steer = thread_can_steer(&thread);
    Ok(Snapshot {
        thread,
        items,
        seq,
        has_more,
        turn_id: state.runtime.active_turn(&id),
        process_alive: state.runtime.process_alive(&id),
        queue: queue_state(&state, &id),
        can_steer,
    })
}

/// Whether the thread's provider takes messages mid-turn — known without a
/// running process, so the composer can offer Steer before it starts.
fn thread_can_steer(meta: &ThreadMeta) -> bool {
    use crate::driver::Driver;
    match meta.provider.as_str() {
        "claude" | "" => ClaudeDriver::new().can_steer(),
        "codex" => CodexDriver::new(CodexOptions::default()).can_steer(),
        _ => false,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageArg {
    media_type: String,
    base64: String,
}

/// Openers a title does without ("Please can you fix …" → "Fix …").
const FILLER: [&str; 18] = [
    "please", "pls", "hey", "hi", "hello", "ok", "okay", "so", "can you", "could you", "would you", "will you",
    "i want you to", "i need you to", "i'd like you to", "bitte", "kannst du", "könntest du",
];

/// The local cleanup of a first-prompt title: filler openers go, the first
/// letter is upper case.
fn tidy_title(line: &str) -> String {
    let mut rest = line.trim();
    loop {
        let lower = rest.to_lowercase();
        let Some(word) = FILLER.iter().find(|w| {
            lower.starts_with(*w) && lower.get(w.len()..).and_then(|t| t.chars().next()).is_some_and(|c| c == ' ' || c == ',' || c == '!')
        }) else {
            break;
        };
        // Lower-casing can change byte lengths; such a line keeps its opener.
        let Some(next) = rest.get(word.len()..).map(|t| t.trim_start_matches([' ', ',', '!'])) else {
            break;
        };
        if next.is_empty() {
            break;
        }
        rest = next;
    }
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The first line of the first prompt, tidied and cut at a word near [`TITLE_MAX`].
pub fn title_from_prompt(text: &str) -> String {
    let first = text
        .lines()
        .map(|l| l.trim_start_matches(['#', '>', '-', '*', ' ']).trim())
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let tidy = tidy_title(first);
    let line = tidy.as_str();
    if line.chars().count() <= TITLE_MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_MAX).collect();
    let at_word = cut.rfind(' ').filter(|&i| i > TITLE_MAX / 2).map(|i| &cut[..i]).unwrap_or(&cut);
    format!("{}…", at_word.trim_end())
}

/// What decides whether a warm process can take the next turn.
fn plan_key(meta: &ThreadMeta, settings: &Settings) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        meta.provider,
        effective_model(meta, settings),
        effective_effort(meta, settings),
        meta.mode,
        settings.attach_quantmcp,
        settings.include_general_vault
    )
}

fn effective_model(meta: &ThreadMeta, settings: &Settings) -> String {
    if !meta.model.trim().is_empty() {
        return meta.model.trim().to_string();
    }
    settings.model_for(&meta.provider).unwrap_or_default().to_string()
}

fn effective_effort(meta: &ThreadMeta, settings: &Settings) -> String {
    if !meta.effort.trim().is_empty() {
        meta.effort.trim().to_string()
    } else {
        settings.effort.trim().to_string()
    }
}

/// `context.txt` and, with QuantMCP attached, `mcp.json` in the thread's own folder.
fn write_thread_files(dir: &Path, system_append: &str, mcp: Option<&str>) -> Result<(PathBuf, Option<PathBuf>), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("thread folder: {e}"))?;
    let context = dir.join("context.txt");
    std::fs::write(&context, system_append).map_err(|e| format!("context.txt: {e}"))?;
    let mcp_path = match mcp {
        Some(url) => {
            let p = dir.join("mcp.json");
            let cfg = json!({ "mcpServers": { "quantsuite": { "type": "http", "url": url } } });
            qs_core::paths::write_atomic(&p, cfg.to_string().as_bytes()).map_err(|e| format!("mcp.json: {e}"))?;
            Some(p)
        }
        None => None,
    };
    Ok((context, mcp_path))
}

fn claude_plan(app: &AppHandle, state: &PilotState, meta: &ThreadMeta, settings: &Settings) -> Result<LaunchPlan, String> {
    let adapter = adapters::find(settings, "claude").ok_or("Claude Code is not a known adapter")?;
    let exe = providers::resolve(&adapter, settings)
        .ok_or("Claude Code was not found. Install it, then sign in with `claude auth login`.")?;
    let ctx = context_of(meta);
    if ctx.kind == "general" {
        std::fs::create_dir_all(&ctx.path).map_err(|e| format!("general vault: {e}"))?;
    }
    let general = contexts::general_vault(app);
    let mcp = settings.attach_quantmcp.then(mcp_url);
    let system = prompt::system_append(&ctx, &general, mcp.as_deref());
    let (context_file, mcp_file) = write_thread_files(&shell::session_dir(&state.data_dir, &meta.id), &system, mcp.as_deref())?;
    let session = match meta.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => SessionRef::Resume(id.to_string()),
        None => SessionRef::New(meta.id.clone()),
    };
    let add_dirs = if ctx.kind != "general" && settings.include_general_vault {
        vec![general.to_string_lossy().into_owned()]
    } else {
        Vec::new()
    };
    let mut args = exe.prefix.clone();
    args.extend(claude::launch_args(&LaunchOptions {
        session,
        model: effective_model(meta, settings),
        effort: effective_effort(meta, settings),
        mode: meta.mode.clone(),
        mcp_config: mcp_file,
        append_prompt_file: Some(context_file),
        add_dirs,
    }));
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: vec![
            ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
            ("QS_PILOT_THREAD".into(), meta.id.clone()),
        ],
        driver: Box::new(ClaudeDriver::new()),
    })
}

/// Codex runs `codex app-server`; the thread starts (or resumes) inside the
/// protocol, with the suite's context as developer instructions and
/// QuantMCP as a per-run config override.
fn codex_plan(app: &AppHandle, meta: &ThreadMeta, settings: &Settings) -> Result<LaunchPlan, String> {
    let adapter = adapters::find(settings, "codex").ok_or("Codex is not a known adapter")?;
    let exe = providers::resolve(&adapter, settings).ok_or("Codex was not found. Install it, then sign in with `codex login`.")?;
    let ctx = context_of(meta);
    if ctx.kind == "general" {
        std::fs::create_dir_all(&ctx.path).map_err(|e| format!("general vault: {e}"))?;
    }
    let general = contexts::general_vault(app);
    let mcp = settings.attach_quantmcp.then(mcp_url);
    let instructions = prompt::system_append(&ctx, &general, mcp.as_deref());
    let writable_roots = if ctx.kind != "general" && settings.include_general_vault {
        vec![general.to_string_lossy().into_owned()]
    } else {
        Vec::new()
    };
    let mut args = exe.prefix.clone();
    args.extend(codex::launch_args());
    let driver = CodexDriver::new(CodexOptions {
        cwd: meta.cwd.clone(),
        resume: meta.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        model: effective_model(meta, settings),
        effort: effective_effort(meta, settings),
        mode: meta.mode.clone(),
        instructions,
        mcp_url: mcp,
        writable_roots,
    });
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: vec![
            ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
            ("QS_PILOT_THREAD".into(), meta.id.clone()),
        ],
        driver: Box::new(driver),
    })
}

#[tauri::command(async)]
pub fn pilot_thread_send(
    app: AppHandle,
    state: State<'_, PilotState>,
    id: String,
    text: String,
    images: Option<Vec<ImageArg>>,
) -> Result<String, String> {
    let images: Vec<ImageInput> = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| ImageInput { media_type: i.media_type, base64: i.base64 })
        .collect();
    // A new message resumes a held queue: it runs after this turn.
    if state.db.queue_held(&id).unwrap_or(false) {
        let _ = state.db.set_queue_held(&id, false);
        emit_queue(&state, &id);
    }
    start_turn(&app, &state, &id, text, images)
}

/// Start a turn on a thread — the composer's send, a dequeued follow-up.
fn start_turn(app: &AppHandle, state: &PilotState, id: &str, text: String, images: Vec<ImageInput>) -> Result<String, String> {
    if text.trim().is_empty() && images.is_empty() {
        return Err("nothing to send".into());
    }
    let mut meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let id = id.to_string();
    if meta.kind != "chat" {
        // A V2 terminal row continues as a chat when its CLI can.
        let provider = if meta.provider.is_empty() { "claude".to_string() } else { meta.provider.clone() };
        if !CHAT_PROVIDERS.contains(&provider.as_str()) {
            return Err(format!("this {provider} session can only continue in its terminal for now"));
        }
        state.db.make_chat(&id, &provider).map_err(|e| e.to_string())?;
        meta.kind = "chat".into();
        meta.provider = provider;
    }
    if meta.title.trim().is_empty() {
        let title = title_from_prompt(&text);
        if !title.is_empty() && state.db.set_title_from(&id, &title, "auto").unwrap_or(false) {
            state.runtime.emit(&id, ThreadEvent::Title { title });
        }
    }
    let settings = lock(&state.settings).clone();
    let key = plan_key(&meta, &settings);
    let input = TurnInput { text, images };
    let attachments = json!(input.images.iter().map(|i| json!({ "kind": "image", "mediaType": i.media_type })).collect::<Vec<_>>());
    let provider = meta.provider.clone();
    state.runtime.send(
        &id,
        input,
        attachments,
        || match provider.as_str() {
            "codex" => codex_plan(app, &meta, &settings),
            _ => claude_plan(app, state, &meta, &settings),
        },
        &key,
    )
}

// ── Follow-ups: queue and steer (card 4) ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    items: Vec<QueueItem>,
    held: bool,
}

fn queue_state(state: &PilotState, thread_id: &str) -> QueueState {
    QueueState {
        items: state.db.queue(thread_id).unwrap_or_default(),
        held: state.db.queue_held(thread_id).unwrap_or(false),
    }
}

fn emit_queue(state: &PilotState, thread_id: &str) {
    let q = queue_state(state, thread_id);
    state.runtime.emit(thread_id, ThreadEvent::Queue { items: q.items, held: q.held });
}

/// After every turn: a completed one hands the thread its next follow-up;
/// a stopped, failed or limited one holds the queue until Resume.
pub fn after_turn(app: &AppHandle, thread_id: &str, status: TurnStatus) {
    let Some(state) = app.try_state::<PilotState>() else {
        return;
    };
    let queue = state.db.queue(thread_id).unwrap_or_default();
    if queue.is_empty() {
        return;
    }
    if status != TurnStatus::Completed {
        let _ = state.db.set_queue_held(thread_id, true);
        emit_queue(&state, thread_id);
        return;
    }
    if state.db.queue_held(thread_id).unwrap_or(false) || state.runtime.active_turn(thread_id).is_some() {
        return;
    }
    send_next(app, &state, thread_id);
}

fn send_next(app: &AppHandle, state: &PilotState, thread_id: &str) {
    let Some(first) = state.db.queue(thread_id).unwrap_or_default().into_iter().next() else {
        return;
    };
    let Ok(Some(item)) = state.db.queue_take(&first.id) else {
        return;
    };
    emit_queue(state, thread_id);
    if let Err(e) = start_turn(app, state, thread_id, item.text.clone(), Vec::new()) {
        // It did not start: back to the front of the queue, held.
        if let Ok(back) = state.db.queue_add(thread_id, &item.text) {
            let _ = state.db.queue_reorder_first(&back.id);
        }
        let _ = state.db.set_queue_held(thread_id, true);
        emit_queue(state, thread_id);
        eprintln!("pilot: a queued message could not start: {e}");
    }
}

#[tauri::command(async)]
pub fn pilot_queue_list(state: State<'_, PilotState>, id: String) -> QueueState {
    queue_state(&state, &id)
}

/// Queue a follow-up. With no turn running it is sent at once instead —
/// the turn may have ended between the click and this call.
#[tauri::command(async)]
pub fn pilot_queue_add(app: AppHandle, state: State<'_, PilotState>, id: String, text: String) -> Result<QueueState, String> {
    if text.trim().is_empty() {
        return Err("nothing to queue".into());
    }
    if state.runtime.active_turn(&id).is_none() && !state.db.queue_held(&id).unwrap_or(false) {
        start_turn(&app, &state, &id, text, Vec::new())?;
        return Ok(queue_state(&state, &id));
    }
    state.db.queue_add(&id, &text).map_err(|e| e.to_string())?;
    emit_queue(&state, &id);
    Ok(queue_state(&state, &id))
}

#[tauri::command(async)]
pub fn pilot_queue_update(state: State<'_, PilotState>, item_id: String, text: String) -> Result<(), String> {
    let thread_id = state.db.queue_thread(&item_id).map_err(|e| e.to_string())?.ok_or("this message was already sent")?;
    if text.trim().is_empty() {
        return Err("a queued message cannot be empty — remove it instead".into());
    }
    state.db.queue_update(&item_id, &text).map_err(|e| e.to_string())?;
    emit_queue(&state, &thread_id);
    Ok(())
}

#[tauri::command(async)]
pub fn pilot_queue_remove(state: State<'_, PilotState>, item_id: String) -> Result<(), String> {
    let Some(thread_id) = state.db.queue_thread(&item_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    state.db.queue_take(&item_id).map_err(|e| e.to_string())?;
    if state.db.queue(&thread_id).unwrap_or_default().is_empty() {
        let _ = state.db.set_queue_held(&thread_id, false);
    }
    emit_queue(&state, &thread_id);
    Ok(())
}

/// Send one queued message now: into the running turn when the agent can
/// take it, otherwise as the next turn.
#[tauri::command(async)]
pub fn pilot_queue_send_now(app: AppHandle, state: State<'_, PilotState>, item_id: String) -> Result<(), String> {
    let thread_id = state.db.queue_thread(&item_id).map_err(|e| e.to_string())?.ok_or("this message was already sent")?;
    if state.runtime.active_turn(&thread_id).is_some() && !state.runtime.can_steer(&thread_id) {
        return Err("this agent cannot take a message while it works — it stays queued".into());
    }
    let item = state.db.queue_take(&item_id).map_err(|e| e.to_string())?.ok_or("this message was already sent")?;
    emit_queue(&state, &thread_id);
    let result = if state.runtime.active_turn(&thread_id).is_some() {
        state.runtime.steer(&thread_id, TurnInput { text: item.text.clone(), images: Vec::new() })
    } else {
        start_turn(&app, &state, &thread_id, item.text.clone(), Vec::new()).map(|_| ())
    };
    if let Err(e) = result {
        if let Ok(back) = state.db.queue_add(&thread_id, &item.text) {
            let _ = state.db.queue_reorder_first(&back.id);
        }
        emit_queue(&state, &thread_id);
        return Err(e);
    }
    Ok(())
}

/// Continue a held queue: the next message starts when nothing runs.
#[tauri::command(async)]
pub fn pilot_queue_resume(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<(), String> {
    state.db.set_queue_held(&id, false).map_err(|e| e.to_string())?;
    emit_queue(&state, &id);
    if state.runtime.active_turn(&id).is_none() {
        send_next(&app, &state, &id);
    }
    Ok(())
}

/// A message into the running turn. With nothing running it starts a turn.
#[tauri::command(async)]
pub fn pilot_thread_steer(app: AppHandle, state: State<'_, PilotState>, id: String, text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("nothing to send".into());
    }
    if state.runtime.active_turn(&id).is_some() {
        return state.runtime.steer(&id, TurnInput { text, images: Vec::new() });
    }
    start_turn(&app, &state, &id, text, Vec::new()).map(|_| ())
}

/// A large paste becomes a file the agent reads on demand (T3 Code: 32 KiB
/// and up), stored with the thread, never inside the project.
#[tauri::command(async)]
pub fn pilot_attachment_save_text(state: State<'_, PilotState>, id: String, name: String, text: String) -> Result<String, String> {
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .chars()
        .take(60)
        .collect();
    let dir = state.data_dir.join("attachments").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("attachments folder: {e}"))?;
    let file = dir.join(format!("{}-{}.txt", if safe.is_empty() { "pasted" } else { &safe }, &uuid::Uuid::new_v4().to_string()[..8]));
    std::fs::write(&file, text).map_err(|e| format!("could not save the pasted text: {e}"))?;
    Ok(file.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn pilot_thread_answer(app: AppHandle, state: State<'_, PilotState>, id: String, request_id: String, answer: Answer) -> Result<(), String> {
    // A question the agent asked for later (Codex async questions) is
    // answered by the next user message, not by a protocol reply.
    if let Some(item) = state.db.request_item(&id, &request_id).map_err(|e| e.to_string())? {
        if item.data["byMessage"].as_bool().unwrap_or(false) {
            return answer_by_message(&app, &state, &id, item, &answer);
        }
    }
    state.runtime.answer(&id, &request_id, &answer)
}

/// The answers as a message — one `question → answer` line each.
pub fn answers_text(questions: &Value, answers: &serde_json::Map<String, Value>) -> String {
    questions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let question = q["question"].as_str()?;
            let answer = answers.get(question).and_then(Value::as_str).filter(|a| !a.trim().is_empty())?;
            Some(format!("{question} → {answer}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn answer_by_message(app: &AppHandle, state: &PilotState, id: &str, mut item: Item, answer: &Answer) -> Result<(), String> {
    if item.data["status"] != "pending" {
        return Err("this question was already answered".into());
    }
    let text = match answer {
        Answer::Answers { answers } => {
            item.data["reply"] = Value::Object(answers.clone());
            answers_text(&item.data["questions"], answers)
        }
        _ => String::new(),
    };
    item.data["status"] = if text.is_empty() { "dismissed" } else { "answered" }.into();
    item.updated_at = now();
    state.db.upsert_item(&item).map_err(|e| e.to_string())?;
    state.runtime.emit(id, ThreadEvent::Item { item });
    if text.is_empty() {
        // Dismissed: nothing goes to the agent.
        return Ok(());
    }
    start_turn(app, state, id, text, Vec::new()).map(|_| ())
}

#[tauri::command(async)]
pub fn pilot_thread_interrupt(state: State<'_, PilotState>, id: String) -> Result<(), String> {
    state.runtime.interrupt(&id)
}

/// A rename always wins over automatic titles. A new mode or model goes
/// straight into the thread's live process when its CLI can switch (Claude
/// Code and Codex can); otherwise — and for effort — it applies from the
/// next turn, which then starts a fresh process. Another agent can take a
/// thread only before its first message: carrying a conversation over is
/// card 15's handoff.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn pilot_thread_update(
    state: State<'_, PilotState>,
    id: String,
    title: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
) -> Result<ThreadMeta, String> {
    if let Some(m) = mode.as_deref() {
        if !settings::valid_mode(m) {
            return Err(format!("unknown permission mode '{m}'"));
        }
    }
    let mut before = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if let Some(p) = provider.as_deref().map(str::trim).filter(|p| !p.is_empty() && *p != before.provider) {
        if !CHAT_PROVIDERS.contains(&p) {
            return Err(format!("'{p}' cannot run a chat thread yet"));
        }
        let started = before.provider_session_id.as_deref().is_some_and(|s| !s.trim().is_empty());
        if started || state.runtime.active_turn(&id).is_some() {
            let settings = lock(&state.settings).clone();
            let label = |a: &str| adapters::find(&settings, a).map(|x| x.label.to_string()).unwrap_or_else(|| a.to_string());
            return Err(format!(
                "This conversation runs on {}. Start a new thread to work with {}.",
                label(&before.provider),
                label(p)
            ));
        }
        state.runtime.stop(&id);
        state.db.set_provider(&id, p).map_err(|e| e.to_string())?;
        before.provider = p.to_string();
    }
    if let Some(t) = title.as_deref().map(str::trim) {
        if !t.is_empty() {
            state.db.set_title_from(&id, t, "user").map_err(|e| e.to_string())?;
            state.runtime.emit(&id, ThreadEvent::Title { title: t.to_string() });
        }
    }
    if let Some(m) = model.as_deref() {
        state.db.set_model(&id, m.trim()).map_err(|e| e.to_string())?;
    }
    if let Some(e) = effort.as_deref() {
        state.db.set_effort(&id, e.trim()).map_err(|e| e.to_string())?;
    }
    if let Some(m) = mode.as_deref() {
        state.db.set_mode(&id, m).map_err(|e| e.to_string())?;
    }
    let after = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let new_mode = mode.as_deref().filter(|m| *m != before.mode);
    let new_model = model.as_deref().map(str::trim).filter(|m| *m != before.model.trim());
    // Effort cannot switch live: a process started with the old one must not
    // count as started with the new key.
    let effort_changed = effort.as_deref().map(str::trim).is_some_and(|e| e != before.effort.trim());
    if (new_mode.is_some() || new_model.is_some()) && !effort_changed {
        let settings = lock(&state.settings).clone();
        let model_value = new_model.map(|_| effective_model(&after, &settings));
        state.runtime.reconfigure(&id, &plan_key(&after, &settings), new_mode, model_value.as_deref());
    }
    Ok(after)
}

// ── The sidebar (card 7) ──

/// Pin, reorder, settle, un-settle, snooze or wake a thread.
#[tauri::command(async)]
pub fn pilot_thread_arrange(state: State<'_, PilotState>, id: String, arrangement: Arrangement) -> Result<ThreadMeta, String> {
    state.db.arrange(&id, &arrangement).map_err(|e| e.to_string())?;
    state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown thread".into())
}

/// A clean title from the model's answer: one line, no quotes or closing
/// period, cut at a word near [`TITLE_MAX`].
pub fn clean_title(raw: &str) -> String {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let line = line.trim_start_matches("Title:").trim().trim_matches(['"', '\'', '`', '*']).trim_end_matches('.').trim();
    title_from_prompt(line)
}

/// A fresh title from the conversation, written by a short, separate Claude
/// Code run (haiku, no session, no hooks, no MCP) — T3 Code's Regenerate
/// title. It replaces any earlier title.
#[tauri::command(async)]
pub fn pilot_thread_regenerate_title(state: State<'_, PilotState>, id: String) -> Result<String, String> {
    let (first, last) = state.db.title_material(&id).map_err(|e| e.to_string())?;
    if first.trim().is_empty() {
        return Err("nothing to name yet — send a message first".into());
    }
    let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let prompt = format!(
        "Write a short title, at most six words, for this conversation. No quotes, no closing period.\n\n\
         The user asked:\n{}\n\nThe latest answer:\n{}\n\nReply with the title only.",
        clip(&first, 2000),
        clip(&last, 1500)
    );
    let settings = lock(&state.settings).clone();
    let adapter = adapters::find(&settings, "claude").ok_or("Claude Code is not a known adapter")?;
    let exe = providers::resolve(&adapter, &settings).ok_or("Claude Code was not found")?;
    let mut cmd = providers::base_cmd(Path::new(&exe.program));
    cmd.args(&exe.prefix).args([
        "-p",
        &prompt,
        "--model",
        "haiku",
        "--output-format",
        "text",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--settings",
        r#"{"disableAllHooks":true}"#,
    ]);
    cmd.current_dir(std::env::temp_dir());
    let (ok, out, err) = providers::run_capture_split(cmd, std::time::Duration::from_secs(60))?;
    let title = clean_title(&out);
    if !ok || title.is_empty() {
        let why = err.lines().chain(out.lines()).map(str::trim).rfind(|l| !l.is_empty()).unwrap_or_default();
        return Err(format!("no title came back{}", if why.is_empty() { String::new() } else { format!(": {why}") }));
    }
    state.db.set_title_regenerated(&id, &title).map_err(|e| e.to_string())?;
    state.runtime.emit(&id, ThreadEvent::Title { title: title.clone() });
    Ok(title)
}

/// Models, efforts and commands the installed CLI reports — from a live
/// thread's handshake or a short discovery run, cached for five minutes.
#[tauri::command(async)]
pub fn pilot_provider_catalog(app: AppHandle, state: State<'_, PilotState>, provider: String, refresh: Option<bool>) -> Result<Catalog, String> {
    if !refresh.unwrap_or(false) {
        if let Some(c) = state.runtime.cached_catalog(&provider) {
            return Ok(c);
        }
    }
    let settings = lock(&state.settings).clone();
    let cwd = contexts::general_vault(&app);
    let _ = std::fs::create_dir_all(&cwd);
    let env = vec![("PATH".to_string(), qs_core::system_path::current().to_string_lossy().into_owned())];
    if provider == "codex" {
        let adapter = adapters::find(&settings, "codex").ok_or("Codex is not a known adapter")?;
        let exe = providers::resolve(&adapter, &settings).ok_or("Codex was not found")?;
        let mut args = exe.prefix.clone();
        args.extend(codex::launch_args());
        let driver = Box::new(CodexDriver::discovery(&cwd.to_string_lossy()));
        let catalog = runtime::discover(&exe.program, &args, &cwd.to_string_lossy(), &env, driver)?;
        state.runtime.store_catalog(catalog.clone());
        return Ok(catalog);
    }
    if provider != "claude" {
        return Err(format!("no catalog for '{provider}' yet"));
    }
    let adapter = adapters::find(&settings, "claude").ok_or("Claude Code is not a known adapter")?;
    let exe = providers::resolve(&adapter, &settings).ok_or("Claude Code was not found")?;
    let mut args = exe.prefix.clone();
    args.extend(claude::discovery_args());
    let catalog = runtime::discover(&exe.program, &args, &cwd.to_string_lossy(), &env, Box::new(ClaudeDriver::new()))?;
    state.runtime.store_catalog(catalog.clone());
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_the_first_meaningful_line() {
        assert_eq!(title_from_prompt("\n# Fix the login bug\nmore"), "Fix the login bug");
        let long = "Please refactor the whole settings module so that every section loads lazily and keeps its state";
        let t = title_from_prompt(long);
        assert!(t.ends_with('…'));
        assert!(t.chars().count() <= TITLE_MAX + 1);
        assert!(!t.contains("  "));
        assert_eq!(title_from_prompt("   "), "");
        assert_eq!(title_from_prompt("hey, can you fix the login bug?"), "Fix the login bug?");
        assert_eq!(title_from_prompt("Bitte kannst du die Tabelle sortieren"), "Die Tabelle sortieren");
        assert_eq!(title_from_prompt("please"), "Please", "a lone filler word stays");
        assert_eq!(title_from_prompt("solve x"), "Solve x", "only whole filler words go");
    }

    #[test]
    fn async_answers_become_one_message() {
        let questions = json!([{ "question": "Keep the old API?" }, { "question": "Which name?" }, { "question": "Skipped?" }]);
        let mut answers = serde_json::Map::new();
        answers.insert("Keep the old API?".into(), json!("No"));
        answers.insert("Which name?".into(), json!("fetch_all"));
        assert_eq!(answers_text(&questions, &answers), "Keep the old API? → No\nWhich name? → fetch_all");
    }

    #[test]
    fn regenerated_titles_are_cleaned() {
        assert_eq!(clean_title("\"Lazy settings sections.\"\n"), "Lazy settings sections");
        assert_eq!(clean_title("Title: Fix the login bug"), "Fix the login bug");
        assert_eq!(clean_title("\n\n**Provider page plan**"), "Provider page plan");
    }
}
