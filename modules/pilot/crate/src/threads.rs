//! V3 thread commands (docs/PLAN-QUANTPILOT.md §4): create, read, send,
//! answer, interrupt, update — and how a Claude Code process is started for
//! a thread. The process work itself is `runtime.rs`; the protocol is
//! `claude.rs`.

use crate::checkpoints::{self, FileChange};
use crate::claude::{self, ClaudeDriver, LaunchOptions, SessionRef};
use crate::codex::{self, CodexDriver, CodexOptions};
use crate::db::{Arrangement, QueueItem, ThreadMeta};
use crate::context::{self, Attachment, Chip, FileHit, SkillInfo};
use crate::driver::{Answer, Catalog, ImageInput, SkillRef, TurnInput, TurnStatus};
use crate::events::{Item, ItemKind, ThreadEvent};
use crate::git::{self, BranchInfo, CleanupCheck, RepoInfo};
use crate::gitops::{self, Generated, GitStatus, Link};
use crate::files::{self, DirEntry, FileContent, PortOwner, SearchResult};
use crate::terminals::{self, DrawerTerm, Hook};
use crate::browser::{self, BrowserState, Listener};
use crate::db::TurnRow;
use crate::lifecycle;
use crate::runtime::{self, LaunchPlan};
use crate::settings::{self, Settings};
use crate::{adapters, context_of, contexts, lock, mcp_url, now, prompt, providers, shell, PilotState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use crate::acp::{AcpDriver, AcpOptions};
use crate::pi::{PiDriver, PiOptions};

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
    /// `local` | `worktree`; absent = the setting (Local outside git).
    workspace_mode: Option<String>,
    base_branch: Option<String>,
    /// An existing worktree to work in ("New thread in this worktree").
    worktree_path: Option<String>,
}

#[tauri::command(async)]
pub fn pilot_thread_create(app: AppHandle, state: State<'_, PilotState>, req: CreateThread) -> Result<ThreadMeta, String> {
    let settings = lock(&state.settings).clone();
    let context_id = req.context_id.unwrap_or_else(|| contexts::GENERAL_ID.into());
    let ctx = contexts::resolve(&app, &context_id).ok_or_else(|| format!("unknown context '{context_id}'"))?;
    // What the request leaves open comes from the settings: default → project (card 19).
    let defaults = settings.defaults_for(&ctx.id);
    let provider = req.provider.unwrap_or_else(|| defaults.provider.clone());
    if !settings.is_chat_provider(&provider) {
        return Err(format!("'{provider}' cannot run a chat thread yet"));
    }
    let stamp = now();
    let title = req.title.unwrap_or_default().trim().to_string();
    let in_repo = git::repo_info(Path::new(&ctx.path)).is_some();
    let worktree = req.worktree_path.filter(|p| Path::new(p).is_dir());
    let workspace_mode = match (&worktree, req.workspace_mode.as_deref()) {
        (Some(_), _) => "worktree".to_string(),
        (None, _) if !in_repo => "local".to_string(),
        (None, Some(m)) if matches!(m, "local" | "worktree") => m.to_string(),
        (None, _) => defaults.workspace.clone(),
    };
    let wt_branch = worktree.as_deref().and_then(|p| git::repo_info(Path::new(p))).map(|r| r.branch).unwrap_or_default();
    let meta = ThreadMeta {
        id: uuid::Uuid::new_v4().to_string(),
        provider,
        provider_session_id: None,
        title_source: if title.is_empty() { "auto".into() } else { "user".into() },
        title,
        context_id: ctx.id,
        context_name: ctx.name,
        cwd: worktree.clone().unwrap_or(ctx.path),
        model: req.model.unwrap_or_default().trim().to_string(),
        mode: req.mode.filter(|m| settings::valid_mode(m)).unwrap_or(defaults.mode),
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
        workspace_mode,
        base_branch: req.base_branch.unwrap_or_default().trim().to_string(),
        branch: wt_branch,
        worktree_path: worktree,
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
    /// Scheduled continuation, limit reset, owed history, open TUI (card 15).
    life: Life,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Life {
    resume_at: Option<i64>,
    limit_reset_at: Option<i64>,
    /// The provider the next message carries the conversation from.
    handoff_from: Option<String>,
    /// The conversation is open in the CLI's terminal UI.
    tui: bool,
    /// The provider's goal for the thread (card 16).
    goal: Option<Value>,
}

fn life_of(state: &PilotState, id: &str) -> Life {
    let (resume_at, limit_reset_at) = state.db.schedule(id).unwrap_or_default();
    let handoff_from = state
        .db
        .handoff(id)
        .ok()
        .flatten()
        .and_then(|h| serde_json::from_str::<Value>(&h).ok())
        .and_then(|v| v["from"].as_str().map(str::to_string));
    let goal = state.db.goal(id).ok().flatten().and_then(|g| serde_json::from_str(&g).ok());
    Life { resume_at, limit_reset_at, handoff_from, tui: lock(&state.tui).contains_key(id), goal }
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
        life: life_of(&state, &id),
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
        "{}|{}|{}|{}|{}|{}|{}",
        meta.provider,
        effective_model(meta, settings),
        effective_effort(meta, settings),
        meta.mode,
        settings.attach_quantmcp,
        settings.include_general_vault,
        settings.launch_fingerprint(&meta.provider)
    )
}

fn effective_model(meta: &ThreadMeta, settings: &Settings) -> String {
    if !meta.model.trim().is_empty() {
        return meta.model.trim().to_string();
    }
    settings.model_in(&meta.context_id, &meta.provider).unwrap_or_default().to_string()
}

/// The base environment of an agent process plus what the settings add (card 19).
fn agent_env(settings: &Settings, provider: &str, base: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut env = base;
    env.extend(settings.launch_env(provider));
    env
}

/// The user's own model ids join the CLI's list in the picker (card 19).
fn with_custom_models(mut catalog: Catalog, settings: &Settings) -> Catalog {
    if let Some(p) = settings.providers.get(&catalog.provider) {
        for id in &p.models {
            if !catalog.models.iter().any(|m| &m.id == id) {
                catalog.models.push(crate::driver::ModelInfo { id: id.clone(), display_name: id.clone(), description: "Your own model id".into(), ..Default::default() });
            }
        }
    }
    catalog
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
    let rewind = pending_rewind(state, &meta.id);
    let session = match meta.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => SessionRef::Resume(id.to_string()),
        // A rewind to before the first message starts a session of its own.
        None if rewind.is_some() => SessionRef::New(uuid::Uuid::new_v4().to_string()),
        None => SessionRef::New(meta.id.clone()),
    };
    let resume_at = rewind.and_then(|r| r.get("at").and_then(Value::as_str).map(str::to_string));
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
        resume_at,
    }));
    args.extend(settings.extra_args("claude"));
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: agent_env(
            settings,
            "claude",
            vec![
                ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
                ("QS_PILOT_THREAD".into(), meta.id.clone()),
                // Headless Claude Code 2.1.x has its task tools (TaskCreate,
                // TaskUpdate) only with this set — verified live 2026-10-30 (card 16).
                ("CLAUDE_CODE_ENABLE_TODO_TOOLS".into(), "1".into()),
            ],
        ),
        driver: Box::new(ClaudeDriver::new()),
    })
}

/// Codex runs `codex app-server`; the thread starts (or resumes) inside the
/// protocol, with the suite's context as developer instructions and
/// QuantMCP as a per-run config override.
fn codex_plan(app: &AppHandle, state: &PilotState, meta: &ThreadMeta, settings: &Settings) -> Result<LaunchPlan, String> {
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
    // Global options (`-c key=value`) go before the subcommand.
    args.extend(settings.extra_args("codex"));
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
        revert_before: pending_rewind(state, &meta.id).and_then(|r| r.get("before").and_then(Value::as_str).map(str::to_string)),
        fork: pending_rewind(state, &meta.id)
            .and_then(|r| r.get("fork").and_then(Value::as_str).map(|src| (src.to_string(), r.get("last").and_then(Value::as_str).map(str::to_string)))),
    });
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: agent_env(
            settings,
            "codex",
            vec![
                ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
                ("QS_PILOT_THREAD".into(), meta.id.clone()),
            ],
        ),
        driver: Box::new(driver),
    })
}

/// An agent id as the runtime keeps it (`&'static str`): the built-ins are
/// literals, a user's own agent is interned once.
fn static_id(id: &str) -> &'static str {
    static IDS: std::sync::OnceLock<std::sync::Mutex<Vec<&'static str>>> = std::sync::OnceLock::new();
    let mut ids = lock(IDS.get_or_init(Default::default));
    if let Some(known) = ids.iter().find(|k| **k == id) {
        return known;
    }
    let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
    ids.push(leaked);
    leaked
}

/// Gemini CLI, OpenCode, Copilot and the user's own agents over the Agent
/// Client Protocol (card 20). QuantMCP goes in as an HTTP server under a
/// name of this thread's own, so threads in one folder never share it.
fn acp_plan(app: &AppHandle, _state: &PilotState, meta: &ThreadMeta, settings: &Settings) -> Result<LaunchPlan, String> {
    let adapter = adapters::find(settings, &meta.provider).ok_or_else(|| format!("unknown agent '{}'", meta.provider))?;
    let exe = providers::resolve(&adapter, settings).ok_or_else(|| format!("{} was not found. Install it, or set its path in Settings → Providers.", adapter.label))?;
    let ctx = context_of(meta);
    if ctx.kind == "general" {
        std::fs::create_dir_all(&ctx.path).map_err(|e| format!("general vault: {e}"))?;
    }
    let general = contexts::general_vault(app);
    let mcp = settings.attach_quantmcp.then(mcp_url);
    let instructions = prompt::system_append(&ctx, &general, mcp.as_deref());
    let mut args = exe.prefix.clone();
    args.extend(adapter.acp_args());
    args.extend(settings.extra_args(&meta.provider));
    let short: String = meta.id.chars().take(8).collect();
    let driver = AcpDriver::new(AcpOptions {
        provider: static_id(&meta.provider),
        label: adapter.label.clone(),
        cwd: meta.cwd.clone(),
        resume: meta.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        mode: meta.mode.clone(),
        model: effective_model(meta, settings),
        mcp: mcp.map(|url| (format!("quantsuite-{short}"), url)),
        instructions: Some(instructions),
    });
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: agent_env(
            settings,
            &meta.provider,
            vec![
                ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
                ("QS_PILOT_THREAD".into(), meta.id.clone()),
            ],
        ),
        driver: Box::new(driver),
    })
}

/// pi and omp in RPC mode (card 20), with QuantPilot's mode extension and
/// the suite's context appended to their system prompt.
fn pi_plan(app: &AppHandle, state: &PilotState, meta: &ThreadMeta, settings: &Settings) -> Result<LaunchPlan, String> {
    let adapter = adapters::find(settings, &meta.provider).ok_or_else(|| format!("unknown agent '{}'", meta.provider))?;
    if !crate::pi::supports(&meta.mode) {
        return Err(format!("{} has no automatic reviewer — choose another permission mode.", adapter.label));
    }
    let exe = providers::resolve(&adapter, settings).ok_or_else(|| format!("{} was not found. Install it, or set its path in Settings → Providers.", adapter.label))?;
    let ctx = context_of(meta);
    if ctx.kind == "general" {
        std::fs::create_dir_all(&ctx.path).map_err(|e| format!("general vault: {e}"))?;
    }
    let general = contexts::general_vault(app);
    let system = prompt::system_append(&ctx, &general, None);
    let dir = shell::session_dir(&state.data_dir, &meta.id);
    let (context_file, _) = write_thread_files(&dir, &system, None)?;
    let extension = dir.join("qp-modes.ts");
    std::fs::write(&extension, crate::pi::MODES_EXTENSION).map_err(|e| format!("qp-modes.ts: {e}"))?;
    let mut args = exe.prefix.clone();
    let session = meta.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    args.extend(crate::pi::launch_args(&extension.to_string_lossy(), session, &effective_model(meta, settings)));
    args.extend(["--append-system-prompt".to_string(), context_file.to_string_lossy().into_owned()]);
    args.extend(settings.extra_args(&meta.provider));
    Ok(LaunchPlan {
        key: plan_key(meta, settings),
        program: exe.program,
        args,
        cwd: meta.cwd.clone(),
        env: agent_env(
            settings,
            &meta.provider,
            vec![
                ("PATH".into(), qs_core::system_path::current().to_string_lossy().into_owned()),
                ("QS_PILOT_THREAD".into(), meta.id.clone()),
                ("QP_PILOT_MODE".into(), meta.mode.clone()),
            ],
        ),
        driver: Box::new(PiDriver::new(PiOptions { provider: static_id(&meta.provider), label: adapter.label.clone(), mode: meta.mode.clone(), model: effective_model(meta, settings) })),
    })
}

/// An attachment from the composer: saved by `pilot_attachment_*` first.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentArg {
    kind: String,
    name: String,
    path: String,
    /// The shelf's thumbnail, kept with the sent message (images).
    thumb: Option<String>,
}

/// The message as the provider gets it (chips and documents expanded,
/// images read from their saved copies) and what the timeline keeps: the
/// typed text with its chips, the attachments with thumbnails.
fn prepare(provider: &str, text: String, images: Vec<ImageInput>, chips: Vec<Chip>, attachments: Vec<AttachmentArg>) -> Result<(TurnInput, Value), String> {
    if attachments.len() > context::MAX_ATTACHMENTS {
        return Err(format!("a message can carry up to {} attachments", context::MAX_ATTACHMENTS));
    }
    let mut images = images;
    let mut shown = Vec::new();
    for a in &attachments {
        if a.kind == "image" {
            let bytes = std::fs::read(&a.path).map_err(|e| format!("{} could not be read: {e}", a.name))?;
            images.push(ImageInput { media_type: "image/png".into(), base64: context::b64(&bytes), path: Some(a.path.clone()) });
        }
        let thumb = a.thumb.clone().filter(|t| t.len() < 96 * 1024);
        shown.push(json!({ "kind": a.kind, "name": a.name, "path": a.path, "thumb": thumb, "mediaType": (a.kind == "image").then_some("image/png") }));
    }
    for c in &chips {
        let content: String = c.content.chars().take(4000).collect();
        shown.push(json!({ "kind": "chip", "chipKind": c.kind, "label": c.label, "content": content, "path": c.path }));
    }
    let files: Vec<Attachment> = attachments.iter().map(|a| Attachment { kind: a.kind.clone(), name: a.name.clone(), path: a.path.clone() }).collect();
    let read = |p: &str| std::fs::read(p).ok();
    let tree = |p: &str| context::folder_tree(Path::new(p));
    let composed = context::compose(&text, &chips, &files, provider, &read, &tree);
    let skills = chips
        .iter()
        .filter(|c| c.kind == "skill")
        .filter_map(|c| c.path.clone().map(|path| SkillRef { name: c.content.clone(), path }))
        .collect();
    let display = (composed != text).then_some(text);
    if composed.trim().is_empty() && images.is_empty() {
        return Err("nothing to send".into());
    }
    Ok((TurnInput { text: composed, images, display, skills }, Value::Array(shown)))
}

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn pilot_thread_send(
    app: AppHandle,
    state: State<'_, PilotState>,
    id: String,
    text: String,
    images: Option<Vec<ImageArg>>,
    chips: Option<Vec<Chip>>,
    attachments: Option<Vec<AttachmentArg>>,
) -> Result<String, String> {
    let images: Vec<ImageInput> = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| ImageInput { media_type: i.media_type, base64: i.base64, path: None })
        .collect();
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let (input, shown) = prepare(&meta.provider, text, images, chips.unwrap_or_default(), attachments.unwrap_or_default())?;
    // A new message resumes a held queue: it runs after this turn.
    if state.db.queue_held(&id).unwrap_or(false) {
        let _ = state.db.set_queue_held(&id, false);
        emit_queue(&state, &id);
    }
    start_input(&app, &state, &id, input, shown)
}

/// Start a turn on a thread with plain text — a dequeued follow-up, an answer.
fn start_turn(app: &AppHandle, state: &PilotState, id: &str, text: String, images: Vec<ImageInput>) -> Result<String, String> {
    start_input(app, state, id, TurnInput { text, images, ..Default::default() }, json!([]))
}

/// Start a turn on a thread — the composer's send, a dequeued follow-up.
fn start_input(app: &AppHandle, state: &PilotState, id: &str, input: TurnInput, shown: Value) -> Result<String, String> {
    if input.text.trim().is_empty() && input.images.is_empty() {
        return Err("nothing to send".into());
    }
    let mut meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let id = id.to_string();
    if meta.kind != "chat" {
        // A V2 terminal row continues as a chat when its CLI can.
        let provider = if meta.provider.is_empty() { "claude".to_string() } else { meta.provider.clone() };
        if !lock(&state.settings).is_chat_provider(&provider) {
            return Err(format!("this {provider} session can only continue in its terminal for now"));
        }
        state.db.make_chat(&id, &provider).map_err(|e| e.to_string())?;
        meta.kind = "chat".into();
        meta.provider = provider;
    }
    if meta.workspace_mode == "worktree" {
        let fresh = meta.worktree_path.is_none();
        ensure_worktree(state, &mut meta, input.display.as_deref().unwrap_or(&input.text))?;
        if fresh && meta.worktree_path.is_some() {
            // The project's "run on worktree create" actions (card 13).
            let settings = lock(&state.settings).clone();
            let shell = shell::resolve(&settings.shell);
            state.drawer.run_hook(app, &settings, &shell, &meta, Hook::WorktreeCreate);
        }
    }
    if meta.title.trim().is_empty() {
        let typed = input.display.as_deref().unwrap_or(&input.text).replace([context::CHIP_OPEN, context::CHIP_CLOSE], "");
        let title = title_from_prompt(&typed);
        if !title.is_empty() && state.db.set_title_from(&id, &title, "auto").unwrap_or(false) {
            state.runtime.emit(&id, ThreadEvent::Title { title });
        }
    }
    // Card 15: the terminal UI owns the session while it is open; a message
    // replaces a scheduled continuation; a provider new to the conversation
    // gets it (bounded) with this message.
    tui_check(app, state, &id)?;
    if state.db.schedule(&id).map(|s| s.0.is_some()).unwrap_or(false) {
        let _ = state.db.set_resume_at(&id, None);
    }
    let mut input = input;
    if let Some(from) = state.db.handoff(&id).ok().flatten().and_then(|h| serde_json::from_str::<Value>(&h).ok()).and_then(|v| v["from"].as_str().map(str::to_string)) {
        if let Some((history, dropped)) = lifecycle::pack_history(&state.db.items(&id, None, 5000).unwrap_or_default()) {
            if input.display.is_none() {
                input.display = Some(input.text.clone());
            }
            input.text = lifecycle::handoff_prompt(&history, dropped, &from, &input.text);
        }
    }
    let settings = lock(&state.settings).clone();
    let key = plan_key(&meta, &settings);
    // Images sent without a saved copy (pasted base64) still show up as images.
    let attachments = if shown.as_array().is_some_and(|a| !a.is_empty()) {
        shown
    } else {
        json!(input.images.iter().map(|i| json!({ "kind": "image", "mediaType": i.media_type })).collect::<Vec<_>>())
    };
    // The checkout before the turn: what its changes and a revert compare to (card 11).
    let root = checkpoints::repo_root(Path::new(&meta.cwd));
    let before = root.as_ref().and_then(|r| checkpoints::snapshot(r).ok());
    let rewinding = pending_rewind(state, &id).is_some();
    let provider = meta.provider.clone();
    let started = state.runtime.send(
        &id,
        input,
        attachments,
        || match adapters::find(&settings, &provider).and_then(|a| a.protocol()) {
            Some(adapters::Protocol::Codex) => codex_plan(app, state, &meta, &settings),
            Some(adapters::Protocol::Acp) => acp_plan(app, state, &meta, &settings),
            Some(adapters::Protocol::PiRpc) => pi_plan(app, state, &meta, &settings),
            _ => claude_plan(app, state, &meta, &settings),
        },
        &key,
    )?;
    if let (Some(root), Some(before)) = (root, before) {
        let root_s = root.to_string_lossy().into_owned();
        if checkpoints::keep(&root, &id, &started, "before", &before).is_ok() {
            let _ = state.db.set_checkpoint(&started, &id, &root_s, &before);
        }
    }
    if rewinding {
        let _ = state.db.set_rewind(&id, None);
    }
    Ok(started)
}

/// The rewind the next launch applies (`{"provider", "at" | "before"}`).
fn pending_rewind(state: &PilotState, id: &str) -> Option<Value> {
    state.db.rewind(id).ok().flatten().and_then(|r| serde_json::from_str(&r).ok())
}

/// After a turn: snapshot the checkout again and keep what the turn's own
/// tools wrote as its changes, with a "changes" item under the answer.
fn record_changes(state: &PilotState, thread_id: &str) {
    let Some(turn) = state.db.turn_refs(thread_id).ok().and_then(|t| t.last().map(|x| x.0.clone())) else {
        return;
    };
    let Ok(Some((root, before, after))) = state.db.checkpoint(&turn) else {
        return;
    };
    if after.is_some() {
        return;
    }
    let root = PathBuf::from(root);
    let Ok(after) = checkpoints::snapshot(&root) else {
        return;
    };
    if checkpoints::keep(&root, thread_id, &turn, "after", &after).is_err() {
        return;
    }
    let _ = state.db.finish_checkpoint(&turn, &after);
    let cwd = state.db.get_session(thread_id).ok().flatten().map(|m| m.cwd).unwrap_or_default();
    let mut paths: Vec<String> = state
        .db
        .turn_files(&turn)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| checkpoints::relative(&root, Path::new(&cwd), p))
        .collect();
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return;
    }
    let Ok(changed) = checkpoints::changes(&root, &before, &after, &paths) else {
        return;
    };
    if changed.is_empty() {
        return;
    }
    let mut patches: Vec<String> = changed.iter().map(|c| checkpoints::patch(&root, &before, &after, &c.path)).collect();
    checkpoints::within_budget(&mut patches);
    let rows: Vec<(FileChange, String)> = changed.iter().cloned().zip(patches).collect();
    let _ = state.db.save_changes(&turn, thread_id, &rows);
    let added: i64 = changed.iter().map(|c| c.added).sum();
    let removed: i64 = changed.iter().map(|c| c.removed).sum();
    state.runtime.append_item(thread_id, &turn, ItemKind::Changes, json!({ "files": changed, "added": added, "removed": removed }));
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
    record_changes(&state, thread_id);
    link_agent_prs(&state, thread_id);
    after_turn_life(&state, thread_id, status);
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
pub fn pilot_queue_add(
    app: AppHandle,
    state: State<'_, PilotState>,
    id: String,
    text: String,
    chips: Option<Vec<Chip>>,
    attachments: Option<Vec<AttachmentArg>>,
) -> Result<QueueState, String> {
    let (chips, attachments) = (chips.unwrap_or_default(), attachments.unwrap_or_default());
    if attachments.iter().any(|a| a.kind == "image") {
        return Err("images go with a message sent now — send it after this turn or steer it in".into());
    }
    // A queued message is plain text: its chips and documents are expanded now.
    let text = if chips.is_empty() && attachments.is_empty() {
        text
    } else {
        let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
        prepare(&meta.provider, text, Vec::new(), chips, attachments)?.0.text
    };
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
        state.runtime.steer(&thread_id, TurnInput { text: item.text.clone(), ..Default::default() })
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
pub fn pilot_thread_steer(
    app: AppHandle,
    state: State<'_, PilotState>,
    id: String,
    text: String,
    chips: Option<Vec<Chip>>,
    attachments: Option<Vec<AttachmentArg>>,
) -> Result<(), String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let (input, shown) = prepare(&meta.provider, text, Vec::new(), chips.unwrap_or_default(), attachments.unwrap_or_default())?;
    if state.runtime.active_turn(&id).is_some() {
        return state.runtime.steer(&id, input);
    }
    start_input(&app, &state, &id, input, shown).map(|_| ())
}

// ── Branch toolbar and worktrees (card 10) ──

/// The first message of a Worktree thread creates its worktree — QuantMCP's
/// implementation, on the planned branch (`quantpilot/<title>` unless the
/// user named one) from the chosen base. A worktree that went missing
/// blocks sending until it is recreated or the thread is settled.
fn ensure_worktree(state: &PilotState, meta: &mut ThreadMeta, prompt: &str) -> Result<(), String> {
    if let Some(path) = meta.worktree_path.clone() {
        if Path::new(&path).is_dir() {
            return Ok(());
        }
        return Err(format!("The worktree {path} is missing — recreate it or settle this thread."));
    }
    let repo = git::repo_info(Path::new(&meta.cwd)).ok_or("Worktree threads need a git repository — this project has none.")?;
    let settings = lock(&state.settings).clone();
    let branch = if meta.branch.trim().is_empty() {
        let words = if meta.title.trim().is_empty() { title_from_prompt(prompt) } else { meta.title.clone() };
        let wanted = git::suggest_branch(&settings.worktree_prefix, &words);
        unique_branch(Path::new(&repo.root), &wanted)
    } else {
        meta.branch.trim().to_string()
    };
    git::validate_branch(&branch)?;
    let base = Some(meta.base_branch.as_str()).filter(|b| !b.trim().is_empty());
    let (wt, _) = qs_mod_mcp::worktree::create_on_branch(&repo.root, &branch, base)?;
    state.db.set_workspace(&meta.id, "worktree", &wt.base, &wt.branch, Some(&wt.path), Some(&wt.path)).map_err(|e| e.to_string())?;
    meta.cwd = wt.path.clone();
    meta.branch = wt.branch;
    meta.base_branch = wt.base;
    meta.worktree_path = Some(wt.path);
    Ok(())
}

/// An auto-named branch never takes over an existing one: `-2`, `-3`, …
fn unique_branch(repo: &Path, wanted: &str) -> String {
    let taken: Vec<String> = git::branches(repo).unwrap_or_default().into_iter().map(|b| b.name).collect();
    if !taken.iter().any(|b| b == wanted) {
        return wanted.to_string();
    }
    (2..).map(|n| format!("{wanted}-{n}")).find(|c| !taken.contains(c)).unwrap_or_else(|| wanted.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    /// The checkout the thread works in (its worktree once there is one).
    repo: Option<RepoInfo>,
    mode: String,
    base_branch: String,
    /// The worktree's branch, or the name it will get.
    branch: String,
    worktree_path: Option<String>,
    worktree_missing: bool,
    /// The thread has had its first message: mode and base are fixed.
    started: bool,
}

#[tauri::command(async)]
pub fn pilot_git_info(state: State<'_, PilotState>, id: String) -> Result<GitInfo, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let missing = meta.worktree_path.as_deref().is_some_and(|p| !Path::new(p).is_dir());
    let repo = if missing { None } else { git::repo_info(Path::new(&meta.cwd)) };
    let settings = lock(&state.settings).clone();
    let branch = if meta.branch.is_empty() && meta.workspace_mode == "worktree" {
        let words = if meta.title.trim().is_empty() { "new thread".to_string() } else { meta.title.clone() };
        git::suggest_branch(&settings.worktree_prefix, &words)
    } else {
        meta.branch.clone()
    };
    Ok(GitInfo {
        repo,
        mode: meta.workspace_mode.clone(),
        base_branch: meta.base_branch.clone(),
        branch,
        worktree_path: meta.worktree_path.clone(),
        worktree_missing: missing,
        // A conversation that moved to another agent (card 15) has no session yet, but it has started.
        started: meta.provider_session_id.as_deref().is_some_and(|s| !s.trim().is_empty())
            || state.runtime.active_turn(&id).is_some()
            || state.db.user_messages(&id).unwrap_or(0) > 0,
    })
}

#[tauri::command(async)]
pub fn pilot_git_branches(state: State<'_, PilotState>, id: String) -> Result<Vec<BranchInfo>, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    git::branches(Path::new(&meta.cwd))
}

/// Switch (or create) the branch of a Local thread's checkout.
#[tauri::command(async)]
pub fn pilot_git_checkout(state: State<'_, PilotState>, id: String, branch: String, create: bool, base: Option<String>) -> Result<RepoInfo, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if state.runtime.active_turn(&id).is_some() {
        return Err("the agent is working in this checkout — switch when it is done".into());
    }
    let cwd = Path::new(&meta.cwd);
    git::checkout(cwd, branch.trim(), create, base.as_deref())?;
    git::repo_info(cwd).ok_or_else(|| "not a git repository".into())
}

/// Local or Worktree, the base branch and the branch name — only before
/// the first message; afterwards the thread's checkout is fixed.
#[tauri::command(async)]
pub fn pilot_thread_set_workspace(
    state: State<'_, PilotState>,
    id: String,
    mode: String,
    base_branch: Option<String>,
    branch: Option<String>,
    worktree_path: Option<String>,
) -> Result<ThreadMeta, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if meta.provider_session_id.as_deref().is_some_and(|s| !s.trim().is_empty()) || state.runtime.active_turn(&id).is_some() || state.db.user_messages(&id).unwrap_or(0) > 0 {
        return Err("This thread has started — its checkout is fixed. Start a new thread to work elsewhere.".into());
    }
    if !matches!(mode.as_str(), "local" | "worktree") {
        return Err(format!("unknown workspace mode '{mode}'"));
    }
    let ctx = context_of(&meta);
    let project = ctx.path.clone();
    if mode == "worktree" && git::repo_info(Path::new(&project)).is_none() {
        return Err("Worktree threads need a git repository — this project has none.".into());
    }
    let branch = branch.unwrap_or_default().trim().to_string();
    if !branch.is_empty() {
        git::validate_branch(&branch)?;
    }
    // Reusing a worktree: the thread works in it right away.
    let reuse = worktree_path.filter(|p| Path::new(p).is_dir());
    let cwd = reuse.clone().or_else(|| (meta.worktree_path.is_some() || mode == "local").then(|| project.clone()));
    let branch = match &reuse {
        Some(p) => git::repo_info(Path::new(p)).map(|r| r.branch).unwrap_or(branch),
        None => branch,
    };
    let mode = if reuse.is_some() { "worktree".to_string() } else { mode };
    state
        .db
        .set_workspace(&id, &mode, base_branch.as_deref().unwrap_or("").trim(), &branch, reuse.as_deref(), cwd.as_deref())
        .map_err(|e| e.to_string())?;
    state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown thread".into())
}

/// The newest worktree another thread of this project used (Ctrl+Shift+L).
#[tauri::command(async)]
pub fn pilot_worktree_previous(state: State<'_, PilotState>, id: String) -> Result<Option<ThreadMeta>, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let ctx = context_of(&meta);
    let Some(repo) = git::repo_info(Path::new(&ctx.path)) else {
        return Ok(None);
    };
    Ok(state
        .db
        .latest_worktree(&repo.root)
        .map_err(|e| e.to_string())?
        .filter(|t| t.id != id && t.worktree_path.as_deref().is_some_and(|p| Path::new(p).is_dir())))
}

fn worktree_of(state: &PilotState, id: &str) -> Result<(ThreadMeta, String, String), String> {
    let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let path = meta.worktree_path.clone().ok_or("this thread has no worktree")?;
    let ctx = context_of(&meta);
    let root = git::repo_info(Path::new(&ctx.path)).map(|r| r.root).ok_or("the project is not a git repository")?;
    Ok((meta, path, root))
}

/// What settling may clean up (dialog defaults: `CleanupCheck::defaults`).
#[tauri::command(async)]
pub fn pilot_worktree_check(state: State<'_, PilotState>, id: String) -> Result<CleanupCheck, String> {
    let (meta, path, root) = worktree_of(&state, &id)?;
    // Another thread working in the same worktree keeps it.
    let shared = state
        .db
        .list_sessions()
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|t| t.id != id && t.settled_at.is_none() && t.worktree_path.as_deref() == Some(path.as_str()));
    let mut check = git::cleanup_check(Path::new(&root), Path::new(&path), &meta.branch, &meta.base_branch);
    if shared {
        check.exists = false;
        (check.suggest_remove, check.suggest_delete_branch) = check.defaults();
    }
    Ok(check)
}

/// Remove the worktree (never with uncommitted changes) and, when asked, its branch.
#[tauri::command(async)]
pub fn pilot_worktree_remove(state: State<'_, PilotState>, id: String, delete_branch: bool, unmerged_ok: bool) -> Result<(), String> {
    let (meta, path, root) = worktree_of(&state, &id)?;
    if state.runtime.active_turn(&id).is_some() {
        return Err("the agent is still working in this worktree".into());
    }
    state.runtime.stop(&id);
    git::remove_worktree(Path::new(&root), Path::new(&path), &meta.branch, delete_branch, unmerged_ok)
}

/// A worktree that went missing comes back on its branch (or a fresh one).
#[tauri::command(async)]
pub fn pilot_worktree_recreate(state: State<'_, PilotState>, id: String) -> Result<ThreadMeta, String> {
    let (meta, _path, root) = worktree_of(&state, &id)?;
    let base = Some(meta.base_branch.as_str()).filter(|b| !b.trim().is_empty());
    let branch = if meta.branch.is_empty() { git::suggest_branch(&lock(&state.settings).worktree_prefix, &meta.title) } else { meta.branch.clone() };
    let (wt, _) = qs_mod_mcp::worktree::create_on_branch(&root, &branch, base)?;
    state.runtime.stop(&id);
    state.db.set_workspace(&id, "worktree", &wt.base, &wt.branch, Some(&wt.path), Some(&wt.path)).map_err(|e| e.to_string())?;
    state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown thread".into())
}

// ── Diff inspector and revert (card 11) ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffList {
    files: Vec<FileChange>,
    /// Files staged in the user's index (Uncommitted).
    staged: Vec<String>,
    /// What the scope compares: commit ids or labels, for the header.
    from: String,
    to: String,
    /// The thread's turns that have changes, newest first: `(turn id, label)`.
    turns: Vec<(String, String)>,
}

/// "Turn 3 · Make it friendlier": the turn's number and the start of its prompt.
fn turn_label(n: usize, prompt: &str) -> String {
    let plain: String = prompt.chars().filter(|c| *c != context::CHIP_OPEN && *c != context::CHIP_CLOSE).collect();
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return format!("Turn {n}");
    }
    let short: String = line.chars().take(48).collect();
    let more = if line.chars().count() > 48 { "…" } else { "" };
    format!("Turn {n} · {short}{more}")
}

fn repo_of(meta: &ThreadMeta) -> Result<PathBuf, String> {
    checkpoints::repo_root(Path::new(&meta.cwd)).ok_or_else(|| "this thread's folder is not a git repository".into())
}

/// The files of a scope: `turn` (a turn's own changes), `uncommitted`
/// (working tree against HEAD, untracked included) or `branch` (HEAD
/// against its merge base with the base branch).
#[tauri::command(async)]
pub fn pilot_diff_list(state: State<'_, PilotState>, id: String, scope: String, turn: Option<String>) -> Result<DiffList, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = repo_of(&meta)?;
    let mut prompts: HashMap<String, String> = HashMap::new();
    for i in state.db.items(&id, None, 2000).unwrap_or_default() {
        if i.kind == ItemKind::User && i.data.get("steer").is_none() {
            prompts.entry(i.turn_id.clone()).or_insert_with(|| i.data["text"].as_str().unwrap_or_default().to_string());
        }
    }
    let turns = state
        .db
        .turn_refs(&id)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter(|(_, (t, _))| state.db.turn_changes(t).map(|c| !c.is_empty()).unwrap_or(false))
        .map(|(n, (t, _))| {
            let label = turn_label(n + 1, prompts.get(&t).map(String::as_str).unwrap_or_default());
            (t, label)
        })
        .rev()
        .collect::<Vec<_>>();
    match scope.as_str() {
        "turn" => {
            let turn = turn.or_else(|| turns.first().map(|t| t.0.clone())).ok_or("no turn has changed files yet")?;
            let (_, before, after) = state.db.checkpoint(&turn).map_err(|e| e.to_string())?.ok_or("this turn has no checkpoint")?;
            Ok(DiffList { files: state.db.turn_changes(&turn).map_err(|e| e.to_string())?, staged: Vec::new(), from: before, to: after.unwrap_or_default(), turns })
        }
        "uncommitted" => {
            let now = checkpoints::snapshot(&root)?;
            let files = checkpoints::changes(&root, "HEAD", &now, &[])?;
            Ok(DiffList { files, staged: checkpoints::staged(&root), from: "HEAD".into(), to: "working tree".into(), turns })
        }
        "branch" => {
            let base = if meta.base_branch.trim().is_empty() { qs_mod_mcp::worktree::default_branch(&root) } else { meta.base_branch.clone() };
            let from = checkpoints::merge_base(&root, &base).ok_or_else(|| format!("no common history with {base}"))?;
            let files = checkpoints::changes(&root, &from, "HEAD", &[])?;
            Ok(DiffList { files, staged: Vec::new(), from, to: "HEAD".into(), turns })
        }
        other => Err(format!("unknown diff scope '{other}'")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSides {
    original: String,
    modified: String,
}

/// Both sides of one file for the diff editor.
#[tauri::command(async)]
pub fn pilot_diff_file(state: State<'_, PilotState>, id: String, from: String, to: String, path: String) -> Result<DiffSides, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = repo_of(&meta)?;
    let original = checkpoints::show(&root, &from, &path);
    let modified = if to == "working tree" { checkpoints::working(&root, &path) } else { checkpoints::show(&root, &to, &path) };
    Ok(DiffSides { original, modified })
}

/// Stage, unstage or discard one file (Uncommitted). Discard is asked for in the app first.
#[tauri::command(async)]
pub fn pilot_diff_action(state: State<'_, PilotState>, id: String, path: String, action: String) -> Result<(), String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = repo_of(&meta)?;
    match action.as_str() {
        "stage" => checkpoints::stage(&root, &path, true),
        "unstage" => checkpoints::stage(&root, &path, false),
        "discard" => {
            if state.runtime.active_turn(&id).is_some() {
                return Err("the agent is working — discard when it is done".into());
            }
            checkpoints::discard(&root, &path)
        }
        other => Err(format!("unknown action '{other}'")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reverted {
    /// The prompt of the turn reverted from, back in the composer.
    prompt: String,
    /// Its chips and attachments, as the timeline kept them.
    attachments: Value,
    files: Vec<String>,
}

/// "Edit from here": the turn of `item_id` and every later one leave the
/// thread; the provider resumes before it (Claude: at the previous turn's
/// last message, forked; Codex: `thread/revert`). With `files`, the
/// checkout goes back to the turn's checkpoint — only in a worktree
/// thread, and never while another live thread works in the same folder.
/// A provider that cannot rewind is refused before anything changes.
#[tauri::command(async)]
pub fn pilot_thread_revert(state: State<'_, PilotState>, id: String, item_id: String, files: bool) -> Result<Reverted, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if state.runtime.active_turn(&id).is_some() {
        return Err("stop the agent first — it is still working".into());
    }
    let item = state.db.get_item(&item_id).map_err(|e| e.to_string())?.ok_or("this message is gone")?;
    let turns = state.db.turn_refs(&id).map_err(|e| e.to_string())?;
    let at = turns.iter().position(|(t, _)| *t == item.turn_id).ok_or("this message has no turn to rewind")?;
    let rewind = match meta.provider.as_str() {
        "claude" | "" => {
            if at == 0 {
                json!({ "provider": "claude", "at": null })
            } else {
                let prev = turns[at - 1].1.clone().ok_or("Claude Code did not report where to resume before this message")?;
                json!({ "provider": "claude", "at": prev })
            }
        }
        "codex" => {
            let this = turns[at].1.clone().ok_or("Codex did not report this turn's id — it cannot rewind to it")?;
            json!({ "provider": "codex", "before": this })
        }
        other => return Err(format!("{other} cannot rewind a conversation")),
    };
    let mut restored = Vec::new();
    if files {
        if meta.worktree_path.is_none() {
            return Err("Files are reverted only in a worktree thread — the project's own checkout keeps them.".into());
        }
        let shared = state
            .db
            .list_sessions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .any(|t| t.id != id && t.cwd == meta.cwd && state.runtime.active_turn(&t.id).is_some());
        if shared {
            return Err("another thread is working in this folder — files stay as they are".into());
        }
        let (root, before, _) = state.db.checkpoint(&item.turn_id).map_err(|e| e.to_string())?.ok_or("this turn has no checkpoint to go back to")?;
        restored = checkpoints::restore(Path::new(&root), &before, &[])?;
    }
    let first = state
        .db
        .items(&id, None, 2000)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.turn_id == item.turn_id && i.kind == ItemKind::User);
    let prompt = first.as_ref().and_then(|i| i.data["text"].as_str().map(str::to_string)).unwrap_or_default();
    let attachments = first.map(|i| i.data["attachments"].clone()).filter(Value::is_array).unwrap_or_else(|| json!([]));
    state.runtime.stop(&id);
    let dropped: Vec<String> = turns[at..].iter().map(|(t, _)| t.clone()).collect();
    state.db.drop_turns_from(&id, &dropped).map_err(|e| e.to_string())?;
    if rewind["provider"] == "claude" && at == 0 {
        let _ = state.db.clear_provider_session(&id);
    }
    state.db.set_rewind(&id, Some(&rewind.to_string())).map_err(|e| e.to_string())?;
    Ok(Reverted { prompt, attachments, files: restored })
}

// ── Git actions (card 12) ──

/// A thread whose checkout git can act on.
fn git_thread(state: &PilotState, id: &str) -> Result<(ThreadMeta, PathBuf), String> {
    let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if meta.worktree_path.as_deref().is_some_and(|p| !Path::new(p).is_dir()) {
        return Err("this thread's worktree is missing".into());
    }
    let cwd = PathBuf::from(&meta.cwd);
    Ok((meta, cwd))
}

/// Nothing changes a checkout under a working agent — this thread's, or another live thread's in the same folder.
fn checkout_idle(state: &PilotState, id: &str, folder: &str) -> Result<(), String> {
    if state.runtime.active_turn(id).is_some() {
        return Err("the agent is working — wait for it to finish, or stop it".into());
    }
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let busy = state
        .db
        .list_sessions()
        .unwrap_or_default()
        .into_iter()
        .any(|t| t.id != id && norm(&t.cwd) == norm(folder) && state.runtime.active_turn(&t.id).is_some());
    if busy {
        return Err("another thread is working in this folder — try again when it is done".into());
    }
    Ok(())
}

#[tauri::command(async)]
pub fn pilot_git_status(state: State<'_, PilotState>, id: String) -> Result<GitStatus, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    gitops::status(&cwd, &meta.base_branch, meta.worktree_path.is_some())
}

/// The CLI that writes a message: the thread's own provider when it can, else the other one.
fn helper_exe(state: &PilotState, provider: &str) -> Result<(String, Vec<String>, String), String> {
    let settings = lock(&state.settings).clone();
    let order = if provider == "codex" { ["codex", "claude"] } else { ["claude", "codex"] };
    for p in order {
        if let Some(exe) = adapters::find(&settings, p).and_then(|a| providers::resolve(&a, &settings)) {
            return Ok((exe.program, exe.prefix, p.to_string()));
        }
    }
    Err("writing a message needs Claude Code or Codex".into())
}

/// A commit message (`kind` = `commit`, of the selected files) or a pull
/// request's title and description (`pr`, against `base`), written by a
/// short helper run; never with AI credits.
#[tauri::command(async)]
pub fn pilot_git_generate(state: State<'_, PilotState>, id: String, kind: String, files: Option<Vec<String>>, base: Option<String>) -> Result<Generated, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    let style = lock(&state.settings).commit_style.clone();
    let (prompt, key) = if kind == "pr" {
        let base = match base.filter(|b| !b.trim().is_empty()) {
            Some(b) => b,
            None => gitops::status(&cwd, &meta.base_branch, meta.worktree_path.is_some())?.base,
        };
        let (first, last) = state.db.title_material(&id).unwrap_or_default();
        let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
        let thread = format!("The user asked:\n{}\n\nThe latest answer:\n{}", clip(&first, 3000), clip(&last, 3000));
        (gitops::pr_request(&cwd, &base, &thread)?, "title")
    } else {
        (gitops::commit_request(&cwd, &style, files.as_deref())?, "subject")
    };
    let (program, prefix, provider) = helper_exe(&state, &meta.provider)?;
    let model = if provider == meta.provider { meta.model.clone() } else { String::new() };
    let raw = gitops::helper_run(Path::new(&program), &prefix, &provider, &model, &prompt)?;
    let g = gitops::parse_generated(&raw, key);
    if g.title.is_empty() {
        return Err("the answer had no subject — try again, or write it yourself".into());
    }
    Ok(g)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDone {
    message: String,
    /// The action half-succeeded: committed, but the push failed.
    error: Option<String>,
}

fn done(message: String) -> GitDone {
    GitDone { message, error: None }
}

#[tauri::command(async)]
pub fn pilot_git_commit(state: State<'_, PilotState>, id: String, message: String, files: Option<Vec<String>>, push: Option<bool>) -> Result<GitDone, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    checkout_idle(&state, &id, &meta.cwd)?;
    let sha = gitops::commit(&cwd, &message, files.as_deref())?;
    let committed = format!("Committed {sha}.");
    if !push.unwrap_or(false) {
        return Ok(done(committed));
    }
    let pushed = gitops::status(&cwd, &meta.base_branch, meta.worktree_path.is_some()).and_then(|st| {
        let remote = st.remote.ok_or("this repository has no remote to push to")?;
        gitops::push(&cwd, &remote, &st.branch, st.upstream.is_none())
    });
    Ok(match pushed {
        Ok(m) => done(format!("{committed} {m}")),
        Err(e) => GitDone { message: committed, error: Some(format!("The push failed: {e}")) },
    })
}

#[tauri::command(async)]
pub fn pilot_git_push(state: State<'_, PilotState>, id: String) -> Result<GitDone, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    let st = gitops::status(&cwd, &meta.base_branch, meta.worktree_path.is_some())?;
    let remote = st.remote.ok_or("this repository has no remote to push to")?;
    gitops::push(&cwd, &remote, &st.branch, st.upstream.is_none()).map(done)
}

#[tauri::command(async)]
pub fn pilot_git_pull(state: State<'_, PilotState>, id: String) -> Result<GitDone, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    checkout_idle(&state, &id, &meta.cwd)?;
    gitops::pull(&cwd).map(done)
}

/// Merge a worktree thread's branch into its base in the project's checkout.
#[tauri::command(async)]
pub fn pilot_git_merge(state: State<'_, PilotState>, id: String) -> Result<GitDone, String> {
    let (meta, path, root) = worktree_of(&state, &id)?;
    checkout_idle(&state, &id, &meta.cwd)?;
    // The merge writes into the project's own checkout.
    checkout_idle(&state, &id, &root)?;
    let wt = Path::new(&path);
    let branch = qs_mod_mcp::worktree::current_branch(wt).unwrap_or_else(|_| meta.branch.clone());
    let base = if meta.base_branch.trim().is_empty() { qs_mod_mcp::worktree::default_branch(Path::new(&root)) } else { meta.base_branch.clone() };
    gitops::merge_into_base(&root, &path, &branch, &base).map(done)
}

#[tauri::command(async)]
pub fn pilot_git_discard_all(state: State<'_, PilotState>, id: String) -> Result<(), String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    checkout_idle(&state, &id, &meta.cwd)?;
    gitops::discard_all(&cwd)
}

/// Push when needed, open the pull request with `gh`, link it to the thread.
#[tauri::command(async)]
pub fn pilot_pr_create(state: State<'_, PilotState>, id: String, base: String, title: String, body: String, draft: Option<bool>) -> Result<Link, String> {
    let (meta, cwd) = git_thread(&state, &id)?;
    checkout_idle(&state, &id, &meta.cwd)?;
    let st = gitops::status(&cwd, &meta.base_branch, meta.worktree_path.is_some())?;
    if st.github.is_none() {
        return Err("pull requests need a GitHub remote".into());
    }
    if let Some(why) = st.pr_blocked {
        return Err(why);
    }
    if st.branch.is_empty() || st.branch == base {
        return Err(format!("work on a branch other than {base} to open a pull request"));
    }
    if !st.files.is_empty() {
        return Err("commit or discard the changed files first".into());
    }
    let remote = st.remote.ok_or("this repository has no remote to push to")?;
    if st.upstream.is_none() || st.ahead > 0 {
        gitops::push(&cwd, &remote, &st.branch, st.upstream.is_none())?;
    }
    let draft = draft.unwrap_or(false);
    let (url, number) = gitops::pr_create(&cwd, &base, &title, &body, draft)?;
    let t = now();
    let link = Link {
        thread_id: id,
        kind: "pr".into(),
        url,
        number,
        title: gitops::strip_ai_credits(title.trim()),
        state: if draft { "draft" } else { "open" }.into(),
        checked_at: t,
        created_at: t,
    };
    state.db.add_link(&link).map_err(|e| e.to_string())?;
    Ok(link)
}

/// Every thread's links (the sidebar), or one thread's.
#[tauri::command(async)]
pub fn pilot_links(state: State<'_, PilotState>, id: Option<String>) -> Result<Vec<Link>, String> {
    state.db.links(id.as_deref()).map_err(|e| e.to_string())
}

/// A thread's pull requests from GitHub: open ones at most every five
/// minutes, all of them when forced.
#[tauri::command(async)]
pub fn pilot_pr_refresh(state: State<'_, PilotState>, id: String, force: Option<bool>) -> Result<Vec<Link>, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let force = force.unwrap_or(false);
    let t = now();
    for mut l in state.db.links(Some(&id)).map_err(|e| e.to_string())? {
        let live = matches!(l.state.as_str(), "open" | "draft");
        if l.kind != "pr" || !(force || (live && t - l.checked_at > 5 * 60 * 1000)) {
            continue;
        }
        if let Ok((title, st)) = gitops::pr_view(Path::new(&meta.cwd), &l.url) {
            l.title = title;
            l.state = st;
            l.checked_at = t;
            let _ = state.db.add_link(&l);
        }
    }
    state.db.links(Some(&id)).map_err(|e| e.to_string())
}

/// A pull request the agent opened with `gh pr create` in this turn joins the thread's links.
fn link_agent_prs(state: &PilotState, thread_id: &str) {
    let Some(turn) = state.db.turn_refs(thread_id).ok().and_then(|t| t.last().map(|x| x.0.clone())) else {
        return;
    };
    let t = now();
    for item in state.db.items(thread_id, None, 400).unwrap_or_default() {
        if item.turn_id != turn || item.kind != ItemKind::Tool || !item.data["input"].as_str().unwrap_or_default().contains("gh pr create") {
            continue;
        }
        for (url, number) in gitops::pr_urls(item.data["output"].as_str().unwrap_or_default()) {
            let link = Link { thread_id: thread_id.into(), kind: "pr".into(), url, number, title: String::new(), state: "open".into(), checked_at: 0, created_at: t };
            let _ = state.db.add_link(&link);
        }
    }
}

// ── Files tab, project search, terminal drawer, dev servers (card 13) ──

fn checkout_of(state: &PilotState, id: &str) -> Result<(ThreadMeta, PathBuf), String> {
    let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = PathBuf::from(&meta.cwd);
    if !root.is_dir() {
        return Err(format!("{} is not there any more", meta.cwd));
    }
    Ok((meta, root))
}

#[tauri::command(async)]
pub fn pilot_files_list(state: State<'_, PilotState>, id: String, rel: Option<String>) -> Result<Vec<DirEntry>, String> {
    let (_, root) = checkout_of(&state, &id)?;
    files::list_dir(&root, rel.as_deref().unwrap_or_default())
}

/// A file for the viewer — relative to the thread's checkout, or absolute (outside it: read-only).
#[tauri::command(async)]
pub fn pilot_file_read(state: State<'_, PilotState>, id: String, path: String) -> Result<FileContent, String> {
    let (_, root) = checkout_of(&state, &id)?;
    files::read(&root, &path)
}

#[tauri::command(async)]
pub fn pilot_file_write(state: State<'_, PilotState>, id: String, path: String, content: String) -> Result<(), String> {
    let (_, root) = checkout_of(&state, &id)?;
    files::write(&root, &path, &content)
}

#[tauri::command(async)]
pub fn pilot_file_save_as(state: State<'_, PilotState>, id: String, path: String, target: String, content: Option<String>) -> Result<(), String> {
    let (_, root) = checkout_of(&state, &id)?;
    files::save_as(&root, &path, &target, content.as_deref())
}

/// The command palette's "In messages" (card 17): at least three letters.
#[tauri::command(async)]
pub fn pilot_messages_search(state: State<'_, PilotState>, query: String, limit: Option<i64>) -> Result<Vec<crate::db::MessageHit>, String> {
    let q = query.trim();
    if q.chars().count() < 3 {
        return Ok(Vec::new());
    }
    state.db.search_messages(q, limit.unwrap_or(20).clamp(1, 50)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn pilot_text_search(state: State<'_, PilotState>, id: String, query: String, regex: Option<bool>, case: Option<bool>) -> Result<SearchResult, String> {
    let (_, root) = checkout_of(&state, &id)?;
    files::search(&root, &query, regex.unwrap_or(false), case.unwrap_or(false))
}

#[tauri::command(async)]
pub fn pilot_ports_alive(ports: Vec<u16>) -> Vec<u16> {
    files::alive_ports(&ports)
}

#[tauri::command(async)]
pub fn pilot_port_owner(port: u16) -> Result<PortOwner, String> {
    files::port_owner(port)
}

/// Stop a dev server the user picked: the process tree that still holds the port.
#[tauri::command(async)]
pub fn pilot_port_stop(port: u16, pid: u32) -> Result<(), String> {
    files::stop_port(port, pid)
}

/// A shell in the thread's checkout, or a project action's Run terminal (a live one is reused).
#[tauri::command(async)]
pub fn pilot_terminal_open(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, PilotState>,
    id: String,
    action: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<DrawerTerm, String> {
    let (meta, root) = checkout_of(&state, &id)?;
    let settings = lock(&state.settings).clone();
    let picked = match action.as_deref() {
        Some(a) => Some(
            terminals::actions_of(&settings, &meta.context_id)
                .iter()
                .find(|x| x.id == a)
                .cloned()
                .ok_or_else(|| format!("the project has no action '{a}'"))?,
        ),
        None => None,
    };
    let shell = shell::resolve(&settings.shell);
    state.drawer.open(&app, webview.label(), &shell, &id, &root.to_string_lossy(), picked.as_ref(), (cols, rows))
}

#[tauri::command(async)]
pub fn pilot_terminals(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<Vec<DrawerTerm>, String> {
    let console = app.try_state::<qs_mod_console::ConsoleState>().ok_or("the console is not running")?;
    Ok(state.drawer.list(&console, &id))
}

#[tauri::command(async)]
pub fn pilot_terminal_close(app: AppHandle, state: State<'_, PilotState>, pty_id: String) -> Result<(), String> {
    terminals::close(&app, &state.drawer, &pty_id);
    Ok(())
}

// ── Browser tab (card 14) ──

#[tauri::command]
pub async fn pilot_browser_open(app: AppHandle, webview: tauri::Webview, url: String, bounds: browser::Bounds, zoom: Option<f64>) -> Result<browser::Status, String> {
    browser::guard(&webview)?;
    let state = app.state::<BrowserState>();
    let _serial = state.serial().lock().await;
    browser::open(&app, &webview, &url, bounds, zoom.unwrap_or(1.0))
}

#[tauri::command]
pub async fn pilot_browser_bounds(app: AppHandle, webview: tauri::Webview, bounds: browser::Bounds, visible: bool) -> Result<(), String> {
    browser::guard(&webview)?;
    let state = app.state::<BrowserState>();
    let _serial = state.serial().lock().await;
    browser::set_bounds(&app, &webview, bounds, visible)
}

#[tauri::command]
pub async fn pilot_browser_action(app: AppHandle, webview: tauri::Webview, action: String) -> Result<(), String> {
    browser::guard(&webview)?;
    browser::action(&app, &action)
}

#[tauri::command]
pub async fn pilot_browser_zoom(app: AppHandle, webview: tauri::Webview, zoom: f64) -> Result<f64, String> {
    browser::guard(&webview)?;
    browser::zoom(&app, zoom)
}

#[tauri::command]
pub async fn pilot_browser_close(app: AppHandle, webview: tauri::Webview) -> Result<(), String> {
    browser::guard(&webview)?;
    let state = app.state::<BrowserState>();
    let _serial = state.serial().lock().await;
    browser::close(&app)
}

#[tauri::command]
pub async fn pilot_browser_pick(app: AppHandle, webview: tauri::Webview, enabled: bool) -> Result<(), String> {
    browser::guard(&webview)?;
    browser::pick(app, enabled).await
}

/// The preview's visible viewport as an image attachment of the thread (5 MiB at most); nothing is sent.
#[tauri::command]
pub async fn pilot_browser_capture(app: AppHandle, webview: tauri::Webview, id: String) -> Result<SavedAttachment, String> {
    browser::guard(&webview)?;
    let png = browser::capture(&app).await?;
    let state = app.state::<PilotState>();
    let saved = save_image(&state, &id, "screenshot", &png)?;
    if saved.size > 5 * 1024 * 1024 {
        let _ = std::fs::remove_file(&saved.path);
        return Err("the screenshot is larger than 5 MiB — make the browser smaller".into());
    }
    Ok(saved)
}

/// Local TCP listeners for the start page — once per open or refresh, never polled.
#[tauri::command(async)]
pub fn pilot_listeners() -> Result<Vec<Listener>, String> {
    browser::listeners()
}

// ── Thread lifecycle (card 15) ──

fn label_of(state: &PilotState, provider: &str) -> String {
    let settings = lock(&state.settings).clone();
    adapters::find(&settings, if provider.is_empty() { "claude" } else { provider }).map(|a| a.label.to_string()).unwrap_or_else(|| provider.to_string())
}

/// After a turn: owed history counts as delivered once a turn completed
/// with a native session; a limit stop applies the settings' defaults.
fn after_turn_life(state: &PilotState, thread_id: &str, status: TurnStatus) {
    let Ok(Some(meta)) = state.db.get_session(thread_id) else {
        return;
    };
    let native = meta.provider_session_id.as_deref().is_some_and(|s| !s.trim().is_empty());
    if lifecycle::handoff_delivered(status == TurnStatus::Completed, native) && state.db.handoff(thread_id).ok().flatten().is_some() {
        let _ = state.db.set_handoff(thread_id, None);
    }
    if status != TurnStatus::Limited {
        return;
    }
    let Some(reset) = state.db.schedule(thread_id).ok().and_then(|s| s.1) else {
        return;
    };
    let settings = lock(&state.settings).clone();
    if settings.limit_resume {
        // A minute after the reset, so the window has really turned over.
        let _ = state.db.set_resume_at(thread_id, Some(reset + 60_000));
        state.wake.notify_one();
    }
    if settings.limit_snooze {
        let _ = state.db.arrange(thread_id, &Arrangement { snoozed_until: Some(Some(reset)), ..Default::default() });
    }
}

/// Continue an interrupted or limited turn.
fn resume_now(app: &AppHandle, state: &PilotState, id: &str) -> Result<String, String> {
    if state.runtime.active_turn(id).is_some() {
        return Err("the agent is already working".into());
    }
    start_input(app, state, id, TurnInput { text: lifecycle::RESUME_TEXT.into(), ..Default::default() }, json!([]))
}

/// The thread's lifecycle facts: scheduled continuation, limit reset, owed history, open TUI.
#[tauri::command(async)]
pub fn pilot_thread_life(app: AppHandle, state: State<'_, PilotState>, id: String) -> Life {
    // A TUI that closed on its own hands the session back here.
    if lock(&state.tui).contains_key(&id) {
        let _ = tui_check(&app, &state, &id);
    }
    life_of(&state, &id)
}

#[tauri::command(async)]
pub fn pilot_thread_resume(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<String, String> {
    resume_now(&app, &state, &id)
}

/// Resume at reset (`at`, Unix ms) or cancel it (`None`).
#[tauri::command(async)]
pub fn pilot_thread_schedule(state: State<'_, PilotState>, id: String, at: Option<i64>) -> Result<(), String> {
    state.db.set_resume_at(&id, at).map_err(|e| e.to_string())?;
    state.wake.notify_one();
    Ok(())
}

/// Runs scheduled continuations when they are due — also those that came
/// due while the app was closed. Sleeps until the next one, or until a
/// schedule changes.
pub async fn resume_scheduler(app: AppHandle) {
    loop {
        let (next, wake) = {
            let Some(state) = app.try_state::<PilotState>() else {
                return;
            };
            let (due, next) = state.db.resumes(now()).unwrap_or_default();
            for id in due {
                let _ = state.db.set_resume_at(&id, None);
                let meta = state.db.get_session(&id).ok().flatten();
                // Settled or snoozed-away threads do not wake up talking.
                if meta.is_some_and(|m| m.settled_at.is_none()) {
                    if let Err(e) = resume_now(&app, &state, &id) {
                        state.runtime.append_item(&id, &last_turn(&state, &id), ItemKind::Notice, json!({ "text": format!("The scheduled continuation did not start: {e}") }));
                    }
                }
            }
            (next, state.wake.clone())
        };
        let wait = next.map(|t| std::time::Duration::from_millis((t - now()).max(0) as u64)).unwrap_or(std::time::Duration::from_secs(3600));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = wake.notified() => {}
        }
    }
}

fn last_turn(state: &PilotState, id: &str) -> String {
    state.db.turn_refs(id).ok().and_then(|t| t.last().map(|x| x.0.clone())).unwrap_or_default()
}

/// A new thread from this one, up to and including the turn of `item_id`
/// — in the same checkout, or a new worktree from this one's branch. The
/// provider continues natively where it can (Claude: forked session at
/// that turn, same folder only; Codex: `thread/fork`); otherwise the
/// first message carries the history.
#[tauri::command(async)]
pub fn pilot_thread_fork(state: State<'_, PilotState>, id: String, item_id: String, worktree: bool) -> Result<ThreadMeta, String> {
    let src = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if state.runtime.active_turn(&id).is_some() {
        return Err("wait for the agent to finish (or stop it) before forking".into());
    }
    let item = state.db.get_item(&item_id).map_err(|e| e.to_string())?.ok_or("that message is gone")?;
    let t = now();
    let mut meta = src.clone();
    meta.id = uuid::Uuid::new_v4().to_string();
    meta.title = format!("{} (fork)", if src.title.trim().is_empty() { "Thread" } else { src.title.trim() });
    meta.title_source = "user".into();
    meta.provider_session_id = None;
    meta.created_at = t;
    meta.updated_at = t;
    meta.last_activity_at = t;
    meta.cost_usd = 0.0;
    meta.input_tokens = 0;
    meta.output_tokens = 0;
    meta.last_error = None;
    meta.state = "idle".into();
    meta.context_tokens = None;
    meta.context_window = None;
    meta.pinned = false;
    meta.sort_key = t as f64;
    meta.settled_at = None;
    meta.snoozed_until = None;
    if worktree {
        let ctx = context_of(&src);
        let branch = git::repo_info(Path::new(&src.cwd)).map(|r| r.branch).filter(|b| !b.is_empty());
        meta.workspace_mode = "worktree".into();
        meta.base_branch = branch.unwrap_or_else(|| src.base_branch.clone());
        meta.branch = String::new();
        meta.worktree_path = None;
        meta.cwd = ctx.path.clone();
    }
    state.db.insert_session(&meta).map_err(|e| e.to_string())?;
    let copied = state.db.copy_turns(&src.id, &meta.id, &item.turn_id).map_err(|e| e.to_string())?;
    let Some((_, last_new, last_ref)) = copied.last().cloned() else {
        let _ = state.db.delete_session(&meta.id);
        return Err("that message has no turn to fork from".into());
    };
    let sid = src.provider_session_id.clone().filter(|s| !s.trim().is_empty());
    let native = match (src.provider.as_str(), sid, last_ref) {
        ("claude" | "", Some(sid), Some(at)) if !worktree => {
            state.db.set_provider_session(&meta.id, &sid).map_err(|e| e.to_string())?;
            Some(json!({ "provider": "claude", "at": at }))
        }
        ("codex", Some(sid), Some(turn)) => Some(json!({ "provider": "codex", "fork": sid, "last": turn })),
        _ => None,
    };
    let note = match native {
        Some(r) => {
            state.db.set_rewind(&meta.id, Some(&r.to_string())).map_err(|e| e.to_string())?;
            format!("Forked from \"{}\" — the conversation continues from here.", src.title)
        }
        None => {
            state.db.set_handoff(&meta.id, Some(&json!({ "from": label_of(&state, &src.provider), "reason": "fork" }).to_string())).map_err(|e| e.to_string())?;
            format!("Forked from \"{}\" — the next message brings the conversation along.", src.title)
        }
    };
    let note = if worktree { format!("{note} A new worktree branches from the source's branch with the next message; uncommitted changes stay behind.") } else { note };
    state.runtime.append_item(&meta.id, &last_new, ItemKind::Notice, json!({ "text": note }));
    state.db.get_session(&meta.id).map_err(|e| e.to_string())?.ok_or_else(|| "the fork was not saved".into())
}

/// Restart the agent's process (after changing skills or MCP config); the conversation continues on the next message.
#[tauri::command(async)]
pub fn pilot_thread_restart(state: State<'_, PilotState>, id: String) -> Result<(), String> {
    if state.runtime.active_turn(&id).is_some() {
        return Err("stop the agent first — it is still working".into());
    }
    state.runtime.stop(&id);
    state.runtime.append_item(&id, &last_turn(&state, &id), ItemKind::Notice, json!({ "text": "Agent session restarted — the next message starts it fresh on the same conversation." }));
    Ok(())
}

/// While the CLI's terminal UI has the session, the thread does not send;
/// once it closed, what it added comes into the timeline.
fn tui_check(app: &AppHandle, state: &PilotState, id: &str) -> Result<(), String> {
    let pty = lock(&state.tui).get(id).cloned();
    let console = app.try_state::<qs_mod_console::ConsoleState>();
    match lifecycle::tui_lock(pty.as_deref(), |p| console.as_ref().is_some_and(|c| c.ptys.contains(p))) {
        lifecycle::TuiLock::Free => Ok(()),
        lifecycle::TuiLock::Held => Err("This conversation is open in the terminal — close it there to continue here.".into()),
        lifecycle::TuiLock::Closed => {
            lock(&state.tui).remove(id);
            import_tui(state, id);
            Ok(())
        }
    }
}

/// The conversation in its CLI's own terminal UI, in the drawer, on the
/// same session and permission mode. The thread's own process lets go of
/// the session first.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn pilot_thread_tui(
    app: AppHandle,
    webview: tauri::Webview,
    state: State<'_, PilotState>,
    id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<DrawerTerm, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let sid = meta.provider_session_id.clone().filter(|s| !s.trim().is_empty()).ok_or("Send a message first — the terminal opens the conversation's own session.")?;
    if pending_rewind(&state, &id).is_some() {
        return Err("Send a message first — this thread starts its own session with it.".into());
    }
    if state.runtime.active_turn(&id).is_some() {
        return Err("wait for the agent to finish (or stop it) before opening it in the terminal".into());
    }
    if let Some(pty) = lock(&state.tui).get(&id).cloned() {
        if let Some(console) = app.try_state::<qs_mod_console::ConsoleState>() {
            if let Some(t) = state.drawer.list(&console, &id).into_iter().find(|t| t.pty_id == pty) {
                return Ok(t);
            }
        }
    }
    let settings = lock(&state.settings).clone();
    let provider = if meta.provider.is_empty() { "claude" } else { meta.provider.as_str() };
    let adapter = adapters::find(&settings, provider).ok_or_else(|| format!("{provider} is not a known agent"))?;
    let exe = providers::resolve(&adapter, &settings).ok_or_else(|| format!("{} was not found", adapter.label))?;
    state.runtime.stop(&id);
    let mut args = exe.prefix.clone();
    args.extend(lifecycle::tui_args(provider, &sid, &meta.mode));
    let term = state.drawer.open_program(&app, webview.label(), &id, &meta.cwd, &format!("{} (terminal)", adapter.label), &exe.program, args, (cols, rows))?;
    lock(&state.tui).insert(id, term.pty_id.clone());
    Ok(term)
}

/// The TUI closed: the thread takes its session back and shows what the TUI added.
#[tauri::command(async)]
pub fn pilot_thread_tui_done(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<usize, String> {
    let Some(pty) = lock(&state.tui).get(&id).cloned() else {
        return Ok(0);
    };
    if app.try_state::<qs_mod_console::ConsoleState>().is_some_and(|c| c.ptys.contains(&pty)) {
        terminals::close(&app, &state.drawer, &pty);
    }
    lock(&state.tui).remove(&id);
    Ok(import_tui(&state, &id))
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default()
}

/// Claude Code's session file: `<config>/projects/<project>/<id>.jsonl`.
fn claude_session_file(sid: &str) -> Option<PathBuf> {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"));
    std::fs::read_dir(root.join("projects")).ok()?.flatten().map(|d| d.path().join(format!("{sid}.jsonl"))).find(|p| p.is_file())
}

/// Codex's rollout file: `<home>/sessions/YYYY/MM/DD/rollout-…-<id>.jsonl`, newest days first.
fn codex_rollout_file(sid: &str) -> Option<PathBuf> {
    let root = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex")).join("sessions");
    let desc = |p: &Path| {
        let mut v: Vec<PathBuf> = std::fs::read_dir(p).map(|r| r.flatten().map(|d| d.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        v.sort();
        v.reverse();
        v
    };
    let suffix = format!("-{sid}.jsonl");
    let mut checked = 0;
    for year in desc(&root) {
        for month in desc(&year) {
            for day in desc(&month) {
                checked += 1;
                if checked > 400 {
                    return None;
                }
                if let Some(f) = std::fs::read_dir(&day).ok()?.flatten().map(|d| d.path()).find(|p| p.to_string_lossy().ends_with(&suffix)) {
                    return Some(f);
                }
            }
        }
    }
    None
}

/// What the terminal session added, as turns of the thread. Display only:
/// the provider resumes its own session natively either way.
fn import_tui(state: &PilotState, id: &str) -> usize {
    let Ok(Some(meta)) = state.db.get_session(id) else {
        return 0;
    };
    let Some(sid) = meta.provider_session_id.clone().filter(|s| !s.trim().is_empty()) else {
        return 0;
    };
    let read = |p: PathBuf| std::fs::read_to_string(p).map(|t| t.lines().map(str::to_string).collect::<Vec<_>>()).unwrap_or_default();
    let turns = if meta.provider == "codex" {
        let lines = codex_rollout_file(&sid).map(read).unwrap_or_default();
        lifecycle::codex_tail(&lifecycle::codex_messages(&lines), state.db.user_messages(id).unwrap_or(0))
    } else {
        let after = state.db.turn_refs(id).unwrap_or_default().into_iter().rev().find_map(|(_, r)| r);
        let lines = claude_session_file(&sid).map(read).unwrap_or_default();
        lifecycle::claude_tail(&lines, after.as_deref())
    };
    for t in &turns {
        let turn_id = uuid::Uuid::new_v4().to_string();
        let _ = state.db.insert_turn(&TurnRow {
            id: turn_id.clone(),
            thread_id: id.to_string(),
            status: "completed".into(),
            started_at: now(),
            ended_at: Some(now()),
            duration_ms: None,
            cost_usd: None,
            error: None,
        });
        state.runtime.append_item(id, &turn_id, ItemKind::User, json!({ "text": t.user, "attachments": [], "provider": meta.provider, "fromTerminal": true }));
        for a in &t.answers {
            state.runtime.append_item(id, &turn_id, ItemKind::Assistant, json!({ "text": a, "streaming": false }));
        }
        if let Some(r) = &t.provider_ref {
            let _ = state.db.set_provider_ref(&turn_id, r);
        }
    }
    if !turns.is_empty() {
        let n = turns.len();
        state.runtime.append_item(
            id,
            &last_turn(state, id),
            ItemKind::Notice,
            json!({ "text": format!("{n} message{} from the terminal session.", if n == 1 { "" } else { "s" }) }),
        );
    }
    turns.len()
}

// ── Composer context (card 9) ──

/// `@` in the composer: files and folders of the thread's checkout.
#[tauri::command(async)]
pub fn pilot_files_search(state: State<'_, PilotState>, id: String, query: String) -> Result<Vec<FileHit>, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = PathBuf::from(&meta.cwd);
    if !root.is_dir() {
        return Err(format!("{} is not there any more", meta.cwd));
    }
    Ok(state.files.search(&root, &query))
}

fn attachment_dir(state: &PilotState, id: &str) -> Result<PathBuf, String> {
    let dir = state.data_dir.join("attachments").join(id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("attachments folder: {e}"))?;
    Ok(dir)
}

fn safe_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .chars()
        .take(60)
        .collect();
    if safe.is_empty() { "file".into() } else { safe }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAttachment {
    kind: String,
    name: String,
    path: String,
    size: u64,
    width: Option<u32>,
    height: Option<u32>,
    /// A PNG data URL for the shelf.
    thumb: Option<String>,
}

/// An image pasted, dropped or picked: normalized to a PNG (longest side
/// 2048 px) next to the thread, never inside the project.
#[tauri::command(async)]
pub fn pilot_attachment_save_image(state: State<'_, PilotState>, id: String, name: String, data: String) -> Result<SavedAttachment, String> {
    let bytes = context::unb64(&data)?;
    save_image(&state, &id, &name, &bytes)
}

fn save_image(state: &PilotState, id: &str, name: &str, bytes: &[u8]) -> Result<SavedAttachment, String> {
    let img = context::normalize_image(bytes)?;
    let stem = safe_name(Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("image"));
    let file = attachment_dir(state, id)?.join(format!("{stem}-{}.png", &uuid::Uuid::new_v4().to_string()[..8]));
    std::fs::write(&file, &img.png).map_err(|e| format!("could not save the image: {e}"))?;
    Ok(SavedAttachment {
        kind: "image".into(),
        name: format!("{stem}.png"),
        path: file.to_string_lossy().into_owned(),
        size: img.png.len() as u64,
        width: Some(img.width),
        height: Some(img.height),
        thumb: Some(format!("data:image/png;base64,{}", context::b64(&img.thumb))),
    })
}

/// A file dropped or picked: copied next to the thread, so the message
/// keeps what was attached even when the original changes.
#[tauri::command(async)]
pub fn pilot_attachment_save_file(state: State<'_, PilotState>, id: String, name: String, data: Option<String>, path: Option<String>) -> Result<SavedAttachment, String> {
    let bytes = match (data, path.as_deref()) {
        (Some(d), _) => context::unb64(&d)?,
        (None, Some(p)) => {
            let meta = std::fs::metadata(p).map_err(|e| format!("{name} could not be read: {e}"))?;
            if meta.is_dir() {
                // Folders stay where they are: the message carries their tree.
                return Ok(SavedAttachment { kind: "folder".into(), name, path: p.to_string(), size: 0, width: None, height: None, thumb: None });
            }
            if meta.len() > 50 * 1024 * 1024 {
                return Err(format!("{name} is larger than 50 MiB"));
            }
            std::fs::read(p).map_err(|e| format!("{name} could not be read: {e}"))?
        }
        (None, None) => return Err("nothing to attach".into()),
    };
    if image::guess_format(&bytes).is_ok_and(|f| matches!(f, image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)) {
        return pilot_attachment_save_image(state, id, name, context::b64(&bytes));
    }
    let file = attachment_dir(&state, &id)?.join(format!("{}-{}", &uuid::Uuid::new_v4().to_string()[..8], safe_name(&name)));
    std::fs::write(&file, &bytes).map_err(|e| format!("could not save {name}: {e}"))?;
    Ok(SavedAttachment { kind: "file".into(), name, path: file.to_string_lossy().into_owned(), size: bytes.len() as u64, width: None, height: None, thumb: None })
}

/// `$` in the composer: the provider's skills — Codex's from its
/// catalog, Claude Code's from the user's, the project's and the plugins' folders.
#[tauri::command(async)]
pub fn pilot_skills(app: AppHandle, state: State<'_, PilotState>, id: String) -> Result<Vec<SkillInfo>, String> {
    let meta = state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    if meta.provider == "codex" {
        let catalog = match state.runtime.cached_catalog("codex") {
            Some(c) => c,
            None => pilot_provider_catalog(app, state.clone(), "codex".into(), None)?,
        };
        return Ok(catalog.skills);
    }
    let home = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".claude"))).ok_or("no home folder")?;
    Ok(context::claude_skills(&home, Path::new(&meta.cwd)))
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
        if !lock(&state.settings).is_chat_provider(p) {
            return Err(format!("'{p}' cannot run a chat thread yet"));
        }
        if state.runtime.active_turn(&id).is_some() {
            return Err("wait for the agent to finish (or stop it) before switching agents".into());
        }
        if lock(&state.tui).contains_key(&id) {
            return Err("close the terminal session first".into());
        }
        // An idle conversation moves to the other agent: it starts fresh and
        // gets the history with the next message (card 15).
        let started = before.provider_session_id.as_deref().is_some_and(|s| !s.trim().is_empty());
        let has_history = state.db.user_messages(&id).unwrap_or(0) > 0;
        state.runtime.stop(&id);
        state.db.set_provider(&id, p).map_err(|e| e.to_string())?;
        if started || has_history {
            let (from, to) = (label_of(&state, &before.provider), label_of(&state, p));
            let _ = state.db.clear_provider_session(&id);
            let _ = state.db.set_rewind(&id, None);
            state.db.set_handoff(&id, Some(&json!({ "from": from, "reason": "switch" }).to_string())).map_err(|e| e.to_string())?;
            state.runtime.append_item(
                &id,
                &last_turn(&state, &id),
                ItemKind::Notice,
                json!({ "text": format!("Conversation carried forward from {from} — the next message brings it to {to}.") }),
            );
        }
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
pub fn pilot_thread_arrange(app: AppHandle, state: State<'_, PilotState>, id: String, arrangement: Arrangement) -> Result<ThreadMeta, String> {
    let was = state.db.get_session(&id).map_err(|e| e.to_string())?.and_then(|m| m.settled_at);
    state.db.arrange(&id, &arrangement).map_err(|e| e.to_string())?;
    if arrangement.settled == Some(true) && was.is_none() {
        on_settled(&app, &state, &id);
    }
    state.db.get_session(&id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown thread".into())
}

/// A thread settled: its project's "run on settle" actions run in its checkout.
pub fn on_settled(app: &AppHandle, state: &PilotState, id: &str) {
    let Ok(Some(meta)) = state.db.get_session(id) else {
        return;
    };
    if !Path::new(&meta.cwd).is_dir() {
        return;
    }
    let settings = lock(&state.settings).clone();
    let shell = shell::resolve(&settings.shell);
    state.drawer.run_hook(app, &settings, &shell, &meta, Hook::Settle);
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
    let settings = lock(&state.settings).clone();
    if !refresh.unwrap_or(false) {
        if let Some(c) = state.runtime.cached_catalog(&provider) {
            return Ok(with_custom_models(c, &settings));
        }
    }
    let cwd = contexts::general_vault(&app);
    let _ = std::fs::create_dir_all(&cwd);
    let env = agent_env(&settings, &provider, vec![("PATH".to_string(), qs_core::system_path::current().to_string_lossy().into_owned())]);
    if provider == "codex" {
        let adapter = adapters::find(&settings, "codex").ok_or("Codex is not a known adapter")?;
        let exe = providers::resolve(&adapter, &settings).ok_or("Codex was not found")?;
        let mut args = exe.prefix.clone();
        args.extend(settings.extra_args("codex"));
        args.extend(codex::launch_args());
        let driver = Box::new(CodexDriver::discovery(&cwd.to_string_lossy()));
        let catalog = runtime::discover(&exe.program, &args, &cwd.to_string_lossy(), &env, driver)?;
        state.runtime.store_catalog(catalog.clone());
        return Ok(with_custom_models(catalog, &settings));
    }
    let adapter = adapters::find(&settings, &provider).ok_or_else(|| format!("unknown agent '{provider}'"))?;
    match adapter.protocol() {
        Some(adapters::Protocol::PiRpc) => {
            let exe = providers::resolve(&adapter, &settings).ok_or_else(|| format!("{} was not found", adapter.label))?;
            let mut args = exe.prefix.clone();
            args.extend(["--mode".to_string(), "rpc".to_string(), "--no-session".to_string()]);
            let driver = Box::new(PiDriver::new(PiOptions { provider: static_id(&provider), label: adapter.label.clone(), ..Default::default() }));
            let catalog = runtime::discover(&exe.program, &args, &cwd.to_string_lossy(), &env, driver)?;
            state.runtime.store_catalog(catalog.clone());
            return Ok(with_custom_models(catalog, &settings));
        }
        // An ACP agent names its models when a session opens: until a thread
        // has run, the picker offers its default and the user's own ids.
        Some(adapters::Protocol::Acp) => {
            return Ok(with_custom_models(Catalog { provider: provider.clone(), ..Default::default() }, &settings));
        }
        Some(adapters::Protocol::Claude) => {}
        _ => return Err(format!("{} runs in a terminal only", adapter.label)),
    }
    let adapter = adapters::find(&settings, "claude").ok_or("Claude Code is not a known adapter")?;
    let exe = providers::resolve(&adapter, &settings).ok_or("Claude Code was not found")?;
    let mut args = exe.prefix.clone();
    args.extend(claude::discovery_args());
    let catalog = runtime::discover(&exe.program, &args, &cwd.to_string_lossy(), &env, Box::new(ClaudeDriver::new()))?;
    state.runtime.store_catalog(catalog.clone());
    Ok(with_custom_models(catalog, &settings))
}

/// Codex's subscription windows without a turn (card 18): a discovery run
/// answers `account/rateLimits/read`.
pub fn codex_limits(app: &AppHandle, state: &PilotState) -> Result<Vec<crate::events::RateWindow>, String> {
    let settings = lock(&state.settings).clone();
    let adapter = adapters::find(&settings, "codex").ok_or("Codex is not a known adapter")?;
    let exe = providers::resolve(&adapter, &settings).ok_or("Codex is not installed")?;
    let cwd = contexts::general_vault(app);
    let _ = std::fs::create_dir_all(&cwd);
    let env = agent_env(&settings, "codex", vec![("PATH".to_string(), qs_core::system_path::current().to_string_lossy().into_owned())]);
    let mut args = exe.prefix.clone();
    args.extend(settings.extra_args("codex"));
    args.extend(codex::launch_args());
    let driver = Box::new(CodexDriver::discovery(&cwd.to_string_lossy()));
    runtime::probe_limits(&exe.program, &args, &cwd.to_string_lossy(), &env, driver)
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
    fn turn_labels_read_as_number_and_prompt() {
        assert_eq!(turn_label(3, "Make it\n  friendlier"), "Turn 3 · Make it friendlier");
        assert_eq!(turn_label(1, "Use \u{2068}README.md\u{2069} here"), "Turn 1 · Use README.md here");
        assert_eq!(turn_label(2, "  "), "Turn 2");
        let long = turn_label(4, &"word ".repeat(30));
        assert!(long.ends_with('…') && long.chars().count() == "Turn 4 · ".chars().count() + 49);
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
