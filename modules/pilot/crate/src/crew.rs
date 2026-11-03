//! Crews (round 2, card 07): an orchestrator thread splits work into tasks
//! and starts worker threads — any agent, each in its own git worktree by
//! default — along a task graph. Workers report progress, ask and finish
//! with QuantMCP's `quantsuite.pilot.crew_*` tools; the orchestrator
//! dispatches, replies, retries, stops and asks the user at decision
//! gates. Every message lands in the crew's one log (the message bus).
//!
//! Nothing polls an agent: the orchestrator ends its turn after a dispatch
//! and QuantPilot wakes it — a message in its queue — with each question,
//! result, failure and decision (Orca's coordinator loop). A worker that
//! asks ends its turn and is woken the same way with the answer.
//!
//! Permissions are the caller-thread checks of the other pilot tools: the
//! connection's token names the thread; workers get the orchestrator's
//! permission mode, never more; only the orchestrator steers its workers;
//! workers talk to the orchestrator, not to each other, unless the crew
//! allows it (`peers`).

use crate::db::{Crew, CrewMember, CrewMessage, CrewTask, Database, HistoryRequest, QueuePayload, TaskPatch, ThreadMeta};
use crate::driver::TurnStatus;
use crate::{agent_tools, git, lock, notify, threads, PilotState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};

/// The frontend's event: one crew's snapshot after every change.
pub const EVENT: &str = "pilot-crew";
const KEY_MAX: usize = 24;
const TASKS_MAX: usize = 24;
const BATCH_MAX: usize = 12;
const SPEC_MAX: usize = 32 * 1024;
const TEXT_MAX: usize = 16 * 1024;
/// Starts in a row an agent may make for one task; the user may always retry.
pub const ATTEMPTS_MAX: i64 = 3;
const WAIT_MAX_SECS: u64 = 300;
/// In a snapshot: the newest messages, each clipped (the log dialog reads all).
const SNAPSHOT_MESSAGES: usize = 200;
const SNAPSHOT_CLIP: usize = 600;
const WATCHDOG_SECS: u64 = 30;

pub fn settled(status: &str) -> bool {
    matches!(status, "done" | "failed" | "stopped")
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

// ── Planning: keys, the task graph, readiness (pure) ──

/// A task as the orchestrator describes it in `crew_dispatch`.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpec {
    pub key: String,
    pub spec: String,
    #[serde(default)]
    pub deps: Vec<String>,
    /// Agent id; default the orchestrator's.
    pub agent: Option<String>,
    /// Several agents: the same task once per agent, each in its own
    /// worktree — compare and keep the winner (the composer's fan-out).
    #[serde(default)]
    pub agents: Vec<String>,
    pub model: Option<String>,
    /// A kanban card id: the worker follows AGENT.md's card workflow.
    pub card: Option<String>,
    /// Own git worktree; default the setting.
    pub worktree: Option<bool>,
    pub timeout_minutes: Option<i64>,
}

/// `Billing API` → `billing-api`.
pub fn normalize_key(raw: &str) -> Result<String, String> {
    let mut key = String::new();
    for c in raw.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            key.push(c);
        } else if (c.is_whitespace() || c == '.' || c == '/') && !key.ends_with('-') {
            key.push('-');
        }
    }
    let key = key.trim_matches('-').to_string();
    if key.is_empty() {
        return Err(format!("\"{}\" is not a task key: use a short name like billing or search-index.", raw.trim()));
    }
    if key.chars().count() > KEY_MAX {
        return Err(format!("The task key \"{key}\" is longer than {KEY_MAX} characters."));
    }
    Ok(key)
}

/// New tasks for a crew, checked against the ones it has: unique keys,
/// known dependencies, no cycle. `worktree_default` is the setting.
pub fn plan_batch(crew_id: &str, existing: &[CrewTask], specs: Vec<TaskSpec>, worktree_default: bool, known_agent: &dyn Fn(&str) -> bool) -> Result<Vec<CrewTask>, String> {
    if specs.is_empty() {
        return Err("Name at least one task.".into());
    }
    if specs.len() > BATCH_MAX {
        return Err(format!("At most {BATCH_MAX} tasks per dispatch."));
    }
    let mut keys: HashSet<String> = existing.iter().map(|t| t.key.clone()).collect();
    let mut out: Vec<CrewTask> = Vec::new();
    for s in specs {
        let key = normalize_key(&s.key)?;
        let spec = s.spec.trim().to_string();
        if spec.is_empty() {
            return Err(format!("Task \"{key}\" has no spec: say the target, the change, what it may edit and the evidence that proves it done."));
        }
        if spec.len() > SPEC_MAX {
            return Err(format!("Task \"{key}\": the spec is longer than {} KB.", SPEC_MAX / 1024));
        }
        let deps = s.deps.iter().map(|d| normalize_key(d)).collect::<Result<Vec<_>, _>>()?;
        let mut agents: Vec<String> = s.agents.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
        agents.dedup();
        let compare = agents.len() > 1;
        if !compare {
            if let Some(a) = agents.pop().or(s.agent.clone()).map(|a| a.trim().to_string()).filter(|a| !a.is_empty()) {
                agents.push(a);
            }
        }
        for a in &agents {
            if !known_agent(a) {
                return Err(format!("Task \"{key}\": '{a}' cannot run a chat thread here — pick an installed agent (list_threads shows which run)."));
            }
        }
        let runs: Vec<(String, String)> = if compare {
            agents.iter().map(|a| (format!("{key}-{}", normalize_key(a).unwrap_or_else(|_| a.clone())), a.clone())).collect()
        } else {
            vec![(key.clone(), agents.first().cloned().unwrap_or_default())]
        };
        for (k, provider) in runs {
            if !keys.insert(k.clone()) {
                return Err(format!("There is a task \"{k}\" already: give this one another key."));
            }
            let card = s.card.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
            out.push(CrewTask {
                id: format!("task-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]),
                crew_id: crew_id.to_string(),
                key: k,
                spec: spec.clone(),
                deps: deps.clone(),
                // A compared task always gets worktrees: the same files, side by side.
                worktree: compare || (card.is_none() && s.worktree.unwrap_or(worktree_default)),
                card_id: card,
                provider,
                model: s.model.clone().map(|m| m.trim().to_string()).unwrap_or_default(),
                timeout_min: s.timeout_minutes.unwrap_or(0).clamp(0, 24 * 60),
                status: "waiting".into(),
                ..Default::default()
            });
        }
    }
    if existing.len() + out.len() > TASKS_MAX {
        return Err(format!("A crew holds at most {TASKS_MAX} tasks; this one has {}.", existing.len()));
    }
    for t in &out {
        for d in &t.deps {
            if *d == t.key {
                return Err(format!("Task \"{}\" cannot depend on itself.", t.key));
            }
            if !keys.contains(d) {
                return Err(format!("Task \"{}\" depends on \"{d}\", which is not a task of this crew.", t.key));
            }
        }
    }
    // Existing tasks never depend on new ones: a cycle can only be new.
    let deps: HashMap<&str, &Vec<String>> = out.iter().map(|t| (t.key.as_str(), &t.deps)).collect();
    fn visit<'a>(k: &'a str, deps: &HashMap<&'a str, &'a Vec<String>>, seen: &mut HashSet<&'a str>, path: &mut Vec<&'a str>) -> Result<(), String> {
        if path.contains(&k) {
            return Err(format!("The tasks wait on each other in a circle: {} → {k}.", path.join(" → ")));
        }
        if !seen.insert(k) {
            return Ok(());
        }
        path.push(k);
        if let Some(ds) = deps.get(k).copied() {
            for d in ds.iter() {
                visit(d.as_str(), deps, seen, path)?;
            }
        }
        path.pop();
        Ok(())
    }
    let mut seen = HashSet::new();
    for t in &out {
        visit(t.key.as_str(), &deps, &mut seen, &mut Vec::new())?;
    }
    Ok(out)
}

/// The waiting tasks whose dependencies are all done, in order.
pub fn ready(tasks: &[CrewTask]) -> Vec<String> {
    let done: HashSet<&str> = tasks.iter().filter(|t| t.status == "done").map(|t| t.key.as_str()).collect();
    tasks.iter().filter(|t| t.status == "waiting" && t.deps.iter().all(|d| done.contains(d.as_str()))).map(|t| t.id.clone()).collect()
}

/// Why a waiting task waits — said in the panel and to the orchestrator.
pub fn wait_note(task: &CrewTask, tasks: &[CrewTask], running: usize, max: i64) -> String {
    let by_key: HashMap<&str, &CrewTask> = tasks.iter().map(|t| (t.key.as_str(), t)).collect();
    let failed: Vec<&str> = task.deps.iter().filter(|d| by_key.get(d.as_str()).is_some_and(|t| matches!(t.status.as_str(), "failed" | "stopped"))).map(String::as_str).collect();
    if !failed.is_empty() {
        return format!("blocked: {} did not finish", failed.join(", "));
    }
    let open: Vec<&str> = task.deps.iter().filter(|d| by_key.get(d.as_str()).is_some_and(|t| t.status != "done")).map(String::as_str).collect();
    if !open.is_empty() {
        return format!("waits for {}", open.join(", "));
    }
    format!("waits for a free slot ({running} of {max} running)")
}

/// What a worker's turn ending means for its task, when it did not report:
/// the new status and why. `None` = nothing to say.
pub fn turn_end(status: &str, queue_waiting: bool, turn: TurnStatus) -> Option<(&'static str, &'static str)> {
    if status != "running" {
        return None;
    }
    match turn {
        TurnStatus::Completed if queue_waiting => None,
        TurnStatus::Completed => Some(("stalled", "ended its turn without crew_done")),
        TurnStatus::Interrupted => Some(("stalled", "was stopped")),
        TurnStatus::Limited => Some(("stalled", "hit a usage limit")),
        TurnStatus::Failed => Some(("failed", "failed")),
    }
}

/// One line per state: `2 done, 1 running (ledger), 1 waiting (notify)`.
pub fn overview(tasks: &[CrewTask]) -> String {
    let mut parts = Vec::new();
    for status in ["running", "asking", "stalled", "waiting", "done", "failed", "stopped"] {
        let keys: Vec<&str> = tasks.iter().filter(|t| t.status == status).map(|t| t.key.as_str()).collect();
        if keys.is_empty() {
            continue;
        }
        parts.push(format!("{} {status} ({})", keys.len(), keys.join(", ")));
    }
    if parts.is_empty() {
        "no tasks".into()
    } else {
        parts.join(", ")
    }
}

fn all_settled(tasks: &[CrewTask]) -> bool {
    !tasks.is_empty() && tasks.iter().all(|t| settled(&t.status))
}

fn status_tail(tasks: &[CrewTask]) -> String {
    let mut s = format!("Crew: {}.", overview(tasks));
    if all_settled(tasks) {
        s.push_str("\nEvery task has settled — report to the user per task: outcome, evidence, open blockers. Merge finished worktree branches as AGENT.md says (both sides' changes survive a conflict).");
    } else {
        s.push_str("\nEnd your turn when nothing needs you now; QuantPilot wakes this thread again.");
    }
    s
}

// ── Messages: what a worker is told, what wakes the orchestrator (pure) ──

pub struct Brief<'a> {
    pub orchestrator: &'a str,
    pub objective: &'a str,
    pub task: &'a CrewTask,
    pub worktree: bool,
}

/// A worker's first message.
pub fn worker_prompt(b: &Brief) -> String {
    let t = b.task;
    let mut s = format!("You are a worker in a QuantPilot crew led by the thread \"{}\".\n", b.orchestrator);
    if !b.objective.trim().is_empty() {
        s.push_str(&format!("Crew objective: {}\n", b.objective.trim()));
    }
    s.push_str(&format!("Your task: {}\n\n{}\n\n", t.key, t.spec.trim()));
    if let Some(card) = &t.card_id {
        s.push_str(&format!("This task is the kanban card {card}. Follow AGENT.md's card workflow for it: claim it, work in the worktree the claim names, commit, complete it — then crew_done.\n"));
    } else if b.worktree {
        s.push_str("You work in your own git worktree on its own branch. Commit your work there before crew_done and name the branch in the result; the coordinator merges it.\n");
    } else {
        s.push_str("You share this checkout with other workers: edit only what your task needs.\n");
    }
    if !t.brief.trim().is_empty() {
        s.push_str(&format!("\n{}\n", t.brief.trim()));
    }
    s.push_str(
        "\nReport with QuantMCP's quantsuite.pilot crew tools:\n\
         - crew_report — progress in percent and a short note, after each step.\n\
         - crew_ask — a question only the coordinator can answer; then end your turn. The answer arrives as your next message.\n\
         - crew_done — always at the end: a summary, the result (what changed and where, the evidence) and failed: true when the task could not be done.\n\
         Work only on this task, and talk to the coordinator, not to other workers.",
    );
    s
}

pub fn ask_wake(key: &str, question_id: &str, text: &str) -> String {
    format!(
        "[Crew] {key} asks (question {question_id}):\n\n{}\n\nAnswer with crew_reply (task \"{key}\", questionId \"{question_id}\") when the task and the user's instructions settle it; otherwise ask the user first.",
        text.trim()
    )
}

pub fn done_wake(task: &CrewTask, failed: bool, tasks: &[CrewTask]) -> String {
    let head = if failed { format!("[Crew] {} failed", task.key) } else { format!("[Crew] {} is done", task.key) };
    let mut s = format!("{head}: {}", task.summary.trim());
    if !task.result.trim().is_empty() {
        s.push_str(&format!("\n\nResult:\n{}", task.result.trim()));
    }
    if failed {
        s.push_str("\n\nRetry it with crew_retry (agent: another agent if this one is stuck), or tell the user. Never report a failed task as done.");
    }
    s.push_str(&format!("\n\n{}", status_tail(tasks)));
    s
}

pub fn escalation_wake(key: &str, reason: &str, last: &str, tasks: &[CrewTask]) -> String {
    let mut s = format!("[Crew] {key} {reason} — its task is not reported done.");
    if !last.trim().is_empty() {
        s.push_str(&format!("\nIts last words: {}", clip(last, 600)));
    }
    s.push_str(&format!("\n\nSteer it with crew_reply, retry it with crew_retry, or tell the user.\n\n{}", status_tail(tasks)));
    s
}

pub fn timeout_wake(key: &str, minutes: i64, tasks: &[CrewTask]) -> String {
    format!("[Crew] {key} worked past its {minutes}-minute limit and was stopped. Retry it with crew_retry or tell the user.\n\n{}", status_tail(tasks))
}

pub fn decision_wake(name: &str, approved: bool, note: &str) -> String {
    let note = note.trim();
    let said = if note.is_empty() { String::new() } else { format!(" They said: {note}") };
    if approved {
        format!("[Crew] The user approved the gate \"{name}\".{said} Continue.")
    } else {
        format!("[Crew] The user rejected the gate \"{name}\".{said} Do not go past it; ask what they want instead.")
    }
}

// ── Snapshots and events ──

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrewSnapshot {
    pub crew: Crew,
    pub tasks: Vec<CrewTask>,
    pub members: Vec<CrewMember>,
    /// The newest messages, clipped; `pilot_crew_log` has them whole.
    pub messages: Vec<CrewMessage>,
    pub message_count: i64,
    /// Open questions and pending gates, whole — however old.
    pub open: Vec<CrewMessage>,
    /// Workers at once, after the setting.
    pub max_parallel: i64,
    /// Thread id → title, for the members.
    pub titles: HashMap<String, String>,
}

pub fn snapshot(db: &Database, settings_parallel: u32, crew_id: &str) -> Result<Option<CrewSnapshot>, String> {
    let Some(crew) = db.crew(crew_id).map_err(|e| e.to_string())? else { return Ok(None) };
    let tasks = db.crew_tasks(crew_id).map_err(|e| e.to_string())?;
    let members = db.crew_members(crew_id).map_err(|e| e.to_string())?;
    let all = db.crew_messages(crew_id, 0).map_err(|e| e.to_string())?;
    let count = all.len() as i64;
    let messages: Vec<CrewMessage> = all
        .into_iter()
        .rev()
        .take(SNAPSHOT_MESSAGES)
        .rev()
        .map(|mut m| {
            m.text = clip(&m.text, SNAPSHOT_CLIP);
            m
        })
        .collect();
    let titles = members
        .iter()
        .filter_map(|m| db.get_session(&m.thread_id).ok().flatten().map(|t| (m.thread_id.clone(), t.title)))
        .collect();
    let max_parallel = effective_parallel(&crew, settings_parallel);
    let open = db.crew_open_requests(crew_id).map_err(|e| e.to_string())?;
    Ok(Some(CrewSnapshot { crew, tasks, members, messages, message_count: count, open, max_parallel, titles }))
}

fn effective_parallel(crew: &Crew, settings_parallel: u32) -> i64 {
    if crew.max_parallel > 0 {
        crew.max_parallel
    } else {
        i64::from(settings_parallel.max(1))
    }
}

fn emit(app: &AppHandle, state: &PilotState, crew_id: &str) {
    let parallel = lock(&state.settings).crew_parallel;
    if let Ok(Some(snap)) = snapshot(&state.db, parallel, crew_id) {
        let _ = app.emit(EVENT, &snap);
    }
}

/// The orchestrator, woken with a message in its queue.
fn wake(app: &AppHandle, state: &PilotState, thread_id: &str, text: &str) {
    if let Err(e) = threads::enqueue(app, state, thread_id, QueuePayload::text(text)) {
        eprintln!("[pilot] crew: could not wake {thread_id}: {e}");
    }
}

fn log(state: &PilotState, crew_id: &str, task_id: Option<&str>, from: &str, to: &str, kind: &str, text: &str) -> Option<CrewMessage> {
    state.db.crew_message_add(crew_id, task_id, from, to, kind, text, None, "").ok()
}

/// A thread's newest assistant text, for an escalation.
fn last_words(db: &Database, thread_id: &str) -> String {
    let page = db.history_page(thread_id, HistoryRequest { limit: Some(12), start: Some("newest".into()), ..Default::default() });
    page.ok()
        .and_then(|p| p.items.into_iter().rev().find(|i| i.kind == "assistant").map(|i| i.preview))
        .unwrap_or_default()
}

// ── Running a crew ──

fn orchestrator(state: &PilotState, crew: &Crew) -> Result<ThreadMeta, String> {
    state
        .db
        .get_session(&crew.orchestrator_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "The crew's orchestrator thread is gone.".to_string())
}

fn worker_title(crew: &Crew, task: &CrewTask) -> String {
    let objective = clip(&crew.objective, 36);
    if objective.is_empty() {
        format!("{} · crew", task.key)
    } else {
        format!("{} · {objective}", task.key)
    }
}

/// Start a claimed task's worker: a new thread with the orchestrator's
/// permission mode, its own worktree unless the task says otherwise.
fn start_worker(app: &AppHandle, state: &PilotState, crew: &Crew, task: &CrewTask) -> Result<String, String> {
    let orch = orchestrator(state, crew)?;
    let provider = if task.provider.trim().is_empty() { orch.provider.clone() } else { task.provider.clone() };
    let repo = git::repo_info(Path::new(&orch.cwd));
    let own_worktree = task.worktree && task.card_id.is_none() && repo.is_some();
    // A worker without its own worktree shares the orchestrator's checkout.
    let shared = if own_worktree { None } else { orch.worktree_path.clone().filter(|p| Path::new(p).is_dir()) };
    let meta = threads::create(
        app,
        state,
        threads::CreateThread {
            context_id: Some(orch.context_id.clone()),
            provider: Some(provider.clone()),
            model: Some(task.model.clone()).filter(|m| !m.trim().is_empty()),
            // A worker may do what the orchestrator may, no more.
            mode: Some(orch.mode.clone()),
            title: Some(worker_title(crew, task)),
            workspace_mode: Some(if own_worktree { "worktree" } else { "local" }.into()),
            base_branch: if own_worktree { repo.map(|r| r.branch).filter(|b| !b.is_empty()) } else { None },
            worktree_path: shared,
            ..Default::default()
        },
    )?;
    let _ = app.emit("pilot-thread-created", &meta);
    let attempt = task.attempts + 1;
    let _ = state.db.crew_task_update(
        &task.id,
        &TaskPatch {
            status: Some("running".into()),
            worker_id: Some(Some(meta.id.clone())),
            attempts: Some(attempt),
            progress: Some(0),
            note: Some(String::new()),
            error: Some(String::new()),
            summary: Some(String::new()),
            result: Some(String::new()),
            started_at: Some(Some(now())),
            finished_at: Some(None),
            provider: Some(provider.clone()),
            brief: Some(String::new()),
            ..Default::default()
        },
    );
    let _ = state.db.crew_member_add(&CrewMember {
        crew_id: crew.id.clone(),
        thread_id: meta.id.clone(),
        task_id: Some(task.id.clone()),
        role: "worker".into(),
        attempt,
        status: "active".into(),
        provider: provider.clone(),
        joined_at: now(),
    });
    let prompt = worker_prompt(&Brief { orchestrator: &orch.title, objective: &crew.objective, task, worktree: own_worktree });
    let place = match (&task.card_id, own_worktree) {
        (Some(card), _) => format!("for kanban card {card}"),
        (None, true) => "in its own worktree".to_string(),
        (None, false) => "in the shared checkout".to_string(),
    };
    let again = if attempt > 1 { format!(" (attempt {attempt})") } else { String::new() };
    log(state, &crew.id, Some(&task.id), &orch.id, &meta.id, "dispatch", &format!("Started {} with {provider}{again}, {place}.", task.key));
    if let Err(e) = threads::start_turn(app, state, &meta.id, prompt, Vec::new()) {
        fail(app, state, crew, &task.id, &format!("could not start: {e}"));
        return Err(e);
    }
    Ok(meta.id)
}

/// A task failed outside its worker's own report: start error, timeout, deleted thread.
fn fail(app: &AppHandle, state: &PilotState, crew: &Crew, task_id: &str, why: &str) {
    let _ = state.db.crew_task_update(
        task_id,
        &TaskPatch { status: Some("failed".into()), error: Some(why.to_string()), finished_at: Some(Some(now())), ..Default::default() },
    );
    let Ok(Some(task)) = state.db.crew_task(task_id) else { return };
    if let Some(w) = &task.worker_id {
        let _ = state.db.crew_member_status(&crew.id, w, "failed");
    }
    log(state, &crew.id, Some(task_id), "quantpilot", &crew.orchestrator_id, "failed", &format!("{} {why}", task.key));
    let tasks = state.db.crew_tasks(&crew.id).unwrap_or_default();
    let mut shown = task.clone();
    shown.summary = why.to_string();
    wake(app, state, &crew.orchestrator_id, &done_wake(&shown, true, &tasks));
}

/// Start what is ready while slots are free; say why the rest waits.
pub fn pump(app: &AppHandle, state: &PilotState, crew_id: &str) {
    let Ok(Some(crew)) = state.db.crew(crew_id) else { return };
    if crew.status == "running" {
        let max = effective_parallel(&crew, lock(&state.settings).crew_parallel);
        let tasks = state.db.crew_tasks(crew_id).unwrap_or_default();
        for id in ready(&tasks) {
            if !state.db.crew_task_claim(&id, crew_id, max).unwrap_or(false) {
                continue;
            }
            if let Ok(Some(task)) = state.db.crew_task(&id) {
                let _ = start_worker(app, state, &crew, &task);
            }
        }
        let tasks = state.db.crew_tasks(crew_id).unwrap_or_default();
        let running = tasks.iter().filter(|t| t.status == "running").count();
        for t in tasks.iter().filter(|t| t.status == "waiting") {
            let note = wait_note(t, &tasks, running, max);
            if note != t.note {
                let _ = state.db.crew_task_update(&t.id, &TaskPatch { note: Some(note), ..Default::default() });
            }
        }
        if all_settled(&tasks) {
            let _ = state.db.crew_update(crew_id, Some("done"), None, None, None);
        }
    }
    let _ = state.db.crew_touch(crew_id);
    emit(app, state, crew_id);
}

/// Stop a worker's turn without the stop waking anyone.
fn halt(state: &PilotState, thread_id: &str) {
    let _ = state.db.set_queue_held(thread_id, true);
    threads::emit_queue(state, thread_id);
    if state.runtime.active_turn(thread_id).is_some() {
        let _ = state.runtime.interrupt(thread_id);
    }
}

/// The orchestrator hears its crew again after the user stopped it: the
/// held queue (Stop crew holds it) is released and delivered.
fn release(app: &AppHandle, state: &PilotState, thread_id: &str) {
    if state.db.queue_held(thread_id).unwrap_or(false) {
        let _ = state.db.set_queue_held(thread_id, false);
        threads::emit_queue(state, thread_id);
    }
    if state.runtime.active_turn(thread_id).is_none() {
        let _ = threads::send_next(app, state, thread_id);
    }
}

/// Stop one task's worker, or every worker of the crew (the cascade).
pub fn stop(app: &AppHandle, state: &PilotState, crew_id: &str, task_id: Option<&str>, by: &str) -> Result<usize, String> {
    state.db.crew(crew_id).map_err(|e| e.to_string())?.ok_or("No such crew.")?;
    let tasks = state.db.crew_tasks(crew_id).map_err(|e| e.to_string())?;
    let mut n = 0;
    let picked = |t: &CrewTask| match task_id {
        None => true,
        Some(id) => t.id == id,
    };
    for t in tasks.iter().filter(|t| picked(t) && !settled(&t.status)) {
        // Settled first: the interrupted turn then has nothing to escalate.
        let _ = state.db.crew_task_update(&t.id, &TaskPatch { status: Some("stopped".into()), finished_at: Some(Some(now())), ..Default::default() });
        if let Some(w) = &t.worker_id {
            let _ = state.db.crew_member_status(crew_id, w, "stopped");
            halt(state, w);
        }
        n += 1;
    }
    if task_id.is_none() {
        let _ = state.db.crew_update(crew_id, Some("stopped"), None, None, None);
        log(state, crew_id, None, by, "crew", "stop", &format!("Stopped the crew ({n} task(s) were open)."));
    } else if let Some(t) = tasks.iter().find(|t| task_id == Some(t.id.as_str())) {
        log(state, crew_id, Some(&t.id), by, t.worker_id.as_deref().unwrap_or("crew"), "stop", &format!("Stopped {}.", t.key));
    }
    pump(app, state, crew_id);
    Ok(n)
}

/// Start a task again — after a failure, a stop, a stall, or to swap its agent.
pub fn retry(app: &AppHandle, state: &PilotState, task_id: &str, provider: Option<String>, model: Option<String>, note: Option<String>, by: &str) -> Result<CrewTask, String> {
    let task = state.db.crew_task(task_id).map_err(|e| e.to_string())?.ok_or("No such task.")?;
    let crew = state.db.crew(&task.crew_id).map_err(|e| e.to_string())?.ok_or("No such crew.")?;
    if by != "user" && task.attempts >= ATTEMPTS_MAX {
        return Err(format!(
            "\"{}\" was started {} times; QuantPilot stops automatic retries there. Tell the user — they can retry it from the crew panel.",
            task.key, task.attempts
        ));
    }
    if let Some(p) = provider.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        if !lock(&state.settings).is_chat_provider(p) {
            return Err(format!("'{p}' cannot run a chat thread here."));
        }
    }
    // What the next worker hears about the last attempt.
    let mut brief = Vec::new();
    if task.attempts > 0 {
        let what = if !task.error.trim().is_empty() {
            task.error.clone()
        } else if !task.summary.trim().is_empty() {
            task.summary.clone()
        } else {
            format!("it ended as {}", task.status)
        };
        let agent = if task.provider.is_empty() { "an agent".to_string() } else { task.provider.clone() };
        brief.push(format!("An earlier attempt ({agent}) did not finish this task: {}.", clip(&what, 800)));
        if let Some(w) = &task.worker_id {
            brief.push(format!("Its thread is {w}; read_thread shows what it did."));
        }
    }
    if let Some(n) = note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        brief.push(n.to_string());
    }
    if let Some(w) = &task.worker_id {
        if !settled(&task.status) {
            halt(state, w);
        }
        let _ = state.db.crew_member_status(&crew.id, w, "replaced");
    }
    let swapped = provider.clone().filter(|p| !p.trim().is_empty() && p.trim() != task.provider);
    let _ = state.db.crew_task_update(
        task_id,
        &TaskPatch {
            status: Some("waiting".into()),
            provider: provider.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()),
            model: model.map(|m| m.trim().to_string()),
            brief: Some(brief.join("\n")),
            finished_at: Some(None),
            ..Default::default()
        },
    );
    if crew.status != "running" {
        let _ = state.db.crew_update(&crew.id, Some("running"), None, None, None);
    }
    let what = match &swapped {
        Some(p) => format!("Retrying {} with {p}.", task.key),
        None => format!("Retrying {}.", task.key),
    };
    log(state, &crew.id, Some(task_id), by, "crew", "retry", &what);
    if by == "user" {
        release(app, state, &crew.orchestrator_id);
    }
    pump(app, state, &crew.id);
    state.db.crew_task(task_id).map_err(|e| e.to_string())?.ok_or_else(|| "No such task.".into())
}

/// After every turn (threads::after_turn): a worker that stopped without
/// reporting wakes its orchestrator — a failed worker is never hidden.
pub fn after_turn(app: &AppHandle, state: &PilotState, thread_id: &str, turn: TurnStatus) {
    let Ok(Some(task)) = state.db.crew_task_of_worker(thread_id) else { return };
    let Ok(Some(crew)) = state.db.crew(&task.crew_id) else { return };
    let queue_waiting = !state.db.queue(thread_id).unwrap_or_default().is_empty();
    let Some((next, reason)) = turn_end(&task.status, queue_waiting, turn) else { return };
    let last = last_words(&state.db, thread_id);
    let error = if next == "failed" {
        state.db.get_session(thread_id).ok().flatten().and_then(|t| t.last_error).unwrap_or_else(|| "its turn failed".into())
    } else {
        String::new()
    };
    let _ = state.db.crew_task_update(
        &task.id,
        &TaskPatch {
            status: Some(next.into()),
            note: Some(reason.into()),
            error: (!error.is_empty()).then(|| error.clone()),
            finished_at: (next == "failed").then_some(Some(now())),
            ..Default::default()
        },
    );
    if next == "failed" {
        let _ = state.db.crew_member_status(&crew.id, thread_id, "failed");
    }
    let tasks = state.db.crew_tasks(&crew.id).unwrap_or_default();
    let said = if error.is_empty() { reason.to_string() } else { format!("{reason}: {}", clip(&error, 300)) };
    log(state, &crew.id, Some(&task.id), "quantpilot", &crew.orchestrator_id, "escalation", &format!("{} {said}", task.key));
    wake(app, state, &crew.orchestrator_id, &escalation_wake(&task.key, &said, &last, &tasks));
    pump(app, state, &crew.id);
}

/// The app closed while workers worked: their turns are over. Each is
/// stalled — nothing restarts on its own (round 2, card 12's rule); the
/// orchestrator or the user picks up from the crew panel.
pub fn after_restart(db: &Database) {
    let Ok(tasks) = db.crew_tasks_active() else { return };
    for t in tasks.iter().filter(|t| t.status == "running") {
        let _ = db.crew_task_update(&t.id, &TaskPatch { status: Some("stalled".into()), note: Some("the app closed while it worked".into()), ..Default::default() });
        let _ = db.crew_message_add(&t.crew_id, Some(&t.id), "quantpilot", "crew", "note", &format!("{}: the app closed while it worked.", t.key), None, "");
    }
}

/// Timeouts: a worker still running past its limit is stopped and its
/// orchestrator woken. One sleeper for every crew; nothing polls an agent.
pub async fn watchdog(app: AppHandle) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(WATCHDOG_SECS)).await;
        let Some(state) = app.try_state::<PilotState>() else { continue };
        let default_min = i64::from(lock(&state.settings).crew_timeout_minutes);
        let Ok(tasks) = state.db.crew_tasks_active() else { continue };
        for t in tasks.iter().filter(|t| t.status == "running") {
            let minutes = if t.timeout_min > 0 { t.timeout_min } else { default_min };
            let Some(started) = t.started_at else { continue };
            if minutes <= 0 || now() - started < minutes * 60_000 {
                continue;
            }
            let Ok(Some(crew)) = state.db.crew(&t.crew_id) else { continue };
            let _ = state.db.crew_task_update(
                &t.id,
                &TaskPatch { status: Some("failed".into()), error: Some(format!("timed out after {minutes} min")), finished_at: Some(Some(now())), ..Default::default() },
            );
            if let Some(w) = &t.worker_id {
                let _ = state.db.crew_member_status(&crew.id, w, "failed");
                halt(&state, w);
            }
            log(&state, &crew.id, Some(&t.id), "quantpilot", &crew.orchestrator_id, "failed", &format!("{} timed out after {minutes} min", t.key));
            let all = state.db.crew_tasks(&crew.id).unwrap_or_default();
            wake(&app, &state, &crew.orchestrator_id, &timeout_wake(&t.key, minutes, &all));
            pump(&app, &state, &crew.id);
        }
    }
}

/// A deleted thread: an orchestrator takes its crew along (workers stop and
/// become threads of their own); a worker's task fails, said to its orchestrator.
pub fn on_thread_deleted(app: &AppHandle, state: &PilotState, thread_id: &str) {
    let led: Vec<Crew> = state.db.crews().unwrap_or_default().into_iter().filter(|c| c.orchestrator_id == thread_id).collect();
    for crew in led {
        let _ = stop(app, state, &crew.id, None, "quantpilot");
        let _ = state.db.crew_delete(&crew.id);
        let _ = app.emit(EVENT, serde_json::json!({ "deleted": crew.id }));
    }
    if let Ok(Some(task)) = state.db.crew_task_of_worker(thread_id) {
        if !settled(&task.status) {
            if let Ok(Some(crew)) = state.db.crew(&task.crew_id) {
                fail(app, state, &crew, &task.id, "lost its thread (deleted)");
                pump(app, state, &crew.id);
            }
        }
    }
}

/// `message_thread` from a crew worker: to its orchestrator, or — when the
/// crew allows it — to another worker of the crew; logged either way.
pub fn check_worker_message(db: &Database, me: &ThreadMeta, target: &ThreadMeta, text: &str) -> Result<(), String> {
    let Ok(Some(task)) = db.crew_task_of_worker(&me.id) else { return Ok(()) };
    if settled(&task.status) {
        return Ok(());
    }
    let Ok(Some(crew)) = db.crew(&task.crew_id) else { return Ok(()) };
    let peer = db.crew_task_of_worker(&target.id).ok().flatten().is_some_and(|t| t.crew_id == crew.id);
    let kind = if target.id == crew.orchestrator_id {
        "note"
    } else if peer && crew.allow_peers {
        "peer"
    } else {
        return Err("Crew workers talk to their coordinator: use crew_ask for a question, crew_report for progress. Workers message each other only when the crew allows it.".into());
    };
    let _ = db.crew_message_add(&crew.id, Some(&task.id), &me.id, &target.id, kind, text, None, "");
    Ok(())
}

// ── The agents' tools: quantsuite.pilot.crew_* ──

fn me(state: &PilotState, token: Option<String>) -> Result<ThreadMeta, String> {
    agent_tools::caller(state, token)
}

/// The caller's crew as its orchestrator.
fn led_crew(state: &PilotState, me: &ThreadMeta) -> Result<Crew, String> {
    state
        .db
        .crew_led_by(&me.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "This thread leads no crew yet — crew_dispatch starts one.".to_string())
}

/// The caller's task as a worker.
fn my_task(state: &PilotState, me: &ThreadMeta) -> Result<(Crew, CrewTask), String> {
    let task = state.db.crew_task_of_worker(&me.id).map_err(|e| e.to_string())?.ok_or("This thread is not a crew worker.")?;
    let crew = state.db.crew(&task.crew_id).map_err(|e| e.to_string())?.ok_or("Its crew is gone.")?;
    if task.worker_id.as_deref() != Some(me.id.as_str()) {
        return Err("This task was given to another worker.".into());
    }
    Ok((crew, task))
}

fn find_task(state: &PilotState, crew: &Crew, key_or_id: &str) -> Result<CrewTask, String> {
    let want = key_or_id.trim();
    let key = normalize_key(want).unwrap_or_default();
    state
        .db
        .crew_tasks(&crew.id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|t| t.id == want || t.key == key)
        .ok_or_else(|| format!("No task \"{want}\" in this crew — crew_wait lists them."))
}

fn checked_text(text: &str, what: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("The {what} is empty."));
    }
    if t.len() > TEXT_MAX {
        return Err(format!("The {what} is longer than {} KB.", TEXT_MAX / 1024));
    }
    Ok(t.to_string())
}

/// The state as the orchestrator reads it.
fn status_text(state: &PilotState, crew: &Crew) -> String {
    let tasks = state.db.crew_tasks(&crew.id).unwrap_or_default();
    let mut lines = vec![format!("Crew {} ({}){}", crew.id, crew.status, if crew.objective.is_empty() { String::new() } else { format!(": {}", crew.objective) })];
    for t in &tasks {
        let mut l = format!("- {} [{}", t.key, t.status);
        if matches!(t.status.as_str(), "running" | "asking" | "stalled") {
            l.push_str(&format!(" {}%", t.progress));
        }
        l.push(']');
        if !t.provider.is_empty() {
            l.push_str(&format!(" {}", t.provider));
        }
        if let Some(w) = &t.worker_id {
            l.push_str(&format!(" thread {w}"));
        }
        if t.attempts > 1 {
            l.push_str(&format!(", attempt {}", t.attempts));
        }
        let detail = if !t.summary.is_empty() { &t.summary } else if !t.error.is_empty() { &t.error } else { &t.note };
        if !detail.trim().is_empty() {
            l.push_str(&format!(" — {}", clip(detail, 240)));
        }
        lines.push(l);
    }
    for r in state.db.crew_open_requests(&crew.id).unwrap_or_default() {
        if r.kind == "ask" {
            let key = tasks.iter().find(|t| Some(&t.id) == r.task_id.as_ref()).map(|t| t.key.as_str()).unwrap_or("?");
            lines.push(format!("Open question {} from {key}: {}", r.id, clip(&r.text, 300)));
        } else {
            lines.push(format!("Gate {} waits for the user: {}", r.id, clip(&r.text, 300)));
        }
    }
    lines.push(String::new());
    lines.push(status_tail(&tasks));
    lines.join("\n")
}

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn pilot_agent_crew_dispatch(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    tasks: Vec<TaskSpec>,
    objective: Option<String>,
    max_parallel: Option<i64>,
    peers: Option<bool>,
) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    if let Ok(Some(t)) = state.db.crew_task_of_worker(&me.id) {
        if !settled(&t.status) {
            return Err("A crew worker cannot lead a crew of its own — ask the coordinator with crew_ask.".into());
        }
    }
    let objective = objective.map(|o| clip(&o, 400)).filter(|o| !o.is_empty());
    let max_parallel = max_parallel.map(|m| m.clamp(1, 16));
    let crew = match state.db.crew_led_by(&me.id).map_err(|e| e.to_string())? {
        Some(c) => {
            state.db.crew_update(&c.id, Some("running"), objective.as_deref(), max_parallel, peers).map_err(|e| e.to_string())?;
            state.db.crew(&c.id).map_err(|e| e.to_string())?.ok_or("crew vanished")?
        }
        None => state
            .db
            .crew_create(&me.id, &me.context_id, objective.as_deref().unwrap_or(""), max_parallel.unwrap_or(0), peers.unwrap_or(false))
            .map_err(|e| e.to_string())?,
    };
    let existing = state.db.crew_tasks(&crew.id).map_err(|e| e.to_string())?;
    let settings = lock(&state.settings).clone();
    let planned = plan_batch(&crew.id, &existing, tasks, settings.crew_worktrees, &|a| settings.is_chat_provider(a))?;
    for t in &planned {
        state.db.crew_task_add(t).map_err(|e| e.to_string())?;
        let deps = if t.deps.is_empty() { String::new() } else { format!(" (after {})", t.deps.join(", ")) };
        log(&state, &crew.id, Some(&t.id), &me.id, "crew", "plan", &format!("Task {}{deps}: {}", t.key, clip(&t.spec, 400)));
    }
    pump(&app, &state, &crew.id);
    let crew = state.db.crew(&crew.id).map_err(|e| e.to_string())?.ok_or("crew vanished")?;
    let mut out = format!("Dispatched {} task(s).\n\n{}", planned.len(), status_text(&state, &crew));
    out.push_str("\n\nnext: End your turn now. QuantPilot wakes this thread with each question, failure and result; crew_wait shows the state.");
    Ok(out)
}

#[tauri::command(async)]
pub fn pilot_agent_crew_ask(app: AppHandle, state: State<'_, PilotState>, pilot_token: Option<String>, question: String) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let (crew, task) = my_task(&state, &me)?;
    let question = checked_text(&question, "question")?;
    let msg = state.db.crew_message_add(&crew.id, Some(&task.id), &me.id, &crew.orchestrator_id, "ask", &question, None, "open").map_err(|e| e.to_string())?;
    let _ = state.db.crew_task_update(&task.id, &TaskPatch { status: Some("asking".into()), note: Some(clip(&question, 200)), ..Default::default() });
    wake(&app, &state, &crew.orchestrator_id, &ask_wake(&task.key, &msg.id, &question));
    let _ = state.db.crew_touch(&crew.id);
    emit(&app, &state, &crew.id);
    Ok(format!("Asked (question {}). End your turn now; the answer arrives as your next message.", msg.id))
}

#[tauri::command(async)]
pub fn pilot_agent_crew_reply(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    task: String,
    text: String,
    question_id: Option<String>,
) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let crew = led_crew(&state, &me)?;
    let t = find_task(&state, &crew, &task)?;
    let text = checked_text(&text, "reply")?;
    let worker = t.worker_id.clone().ok_or_else(|| format!("\"{}\" has no worker yet: {}.", t.key, t.note))?;
    let question = question_id.as_deref().map(str::trim).filter(|q| !q.is_empty());
    if let Some(q) = question {
        let m = state.db.crew_message(q).map_err(|e| e.to_string())?.filter(|m| m.crew_id == crew.id && m.kind == "ask").ok_or_else(|| format!("No question {q} in this crew."))?;
        if m.status == "answered" {
            return Err(format!("Question {q} is answered already — send a plain crew_reply to steer."));
        }
        state.db.crew_message_status(q, "answered").map_err(|e| e.to_string())?;
    }
    let kind = if question.is_some() { "reply" } else { "steer" };
    state.db.crew_message_add(&crew.id, Some(&t.id), &me.id, &worker, kind, &text, question, "").map_err(|e| e.to_string())?;
    let body = match question {
        Some(q) => format!("[Crew] The coordinator answers your question {q}:\n\n{text}\n\nContinue your task; finish with crew_done."),
        None => format!("[Crew] Message from the coordinator:\n\n{text}\n\nContinue your task; finish with crew_done."),
    };
    // A reply reopens a stalled or settled task: the worker goes on with it.
    if t.status != "running" {
        let _ = state.db.crew_task_update(&t.id, &TaskPatch { status: Some("running".into()), note: Some(String::new()), finished_at: Some(None), ..Default::default() });
        let _ = state.db.crew_member_status(&crew.id, &worker, "active");
        if crew.status != "running" {
            let _ = state.db.crew_update(&crew.id, Some("running"), None, None, None);
        }
    }
    let _ = state.db.set_queue_held(&worker, false);
    threads::enqueue(&app, &state, &worker, QueuePayload::text(&body))?;
    let _ = state.db.crew_touch(&crew.id);
    emit(&app, &state, &crew.id);
    Ok(format!("Sent to {}; it takes it when its current turn ends.", t.key))
}

#[tauri::command(async)]
pub fn pilot_agent_crew_report(app: AppHandle, state: State<'_, PilotState>, pilot_token: Option<String>, progress: i64, note: Option<String>) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let (crew, task) = my_task(&state, &me)?;
    let progress = progress.clamp(0, 100);
    let note = note.map(|n| clip(&n, 300)).unwrap_or_default();
    let _ = state.db.crew_task_update(&task.id, &TaskPatch { status: Some("running".into()), progress: Some(progress), note: Some(note.clone()), ..Default::default() });
    let text = if note.is_empty() { format!("{progress}%") } else { format!("{progress}% — {note}") };
    log(&state, &crew.id, Some(&task.id), &me.id, &crew.orchestrator_id, "progress", &text);
    emit(&app, &state, &crew.id);
    Ok("Noted.".into())
}

#[tauri::command(async)]
pub fn pilot_agent_crew_done(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    summary: String,
    result: Option<String>,
    failed: Option<bool>,
) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let (crew, task) = my_task(&state, &me)?;
    let summary = checked_text(&summary, "summary")?;
    let result = result.map(|r| r.trim().to_string()).unwrap_or_default();
    if result.len() > TEXT_MAX * 2 {
        return Err(format!("The result is longer than {} KB — put the details in a file and name it.", TEXT_MAX * 2 / 1024));
    }
    let failed = failed.unwrap_or(false);
    let status = if failed { "failed" } else { "done" };
    state
        .db
        .crew_task_update(
            &task.id,
            &TaskPatch {
                status: Some(status.into()),
                progress: (!failed).then_some(100),
                summary: Some(summary.clone()),
                result: Some(result.clone()),
                error: Some(if failed { summary.clone() } else { String::new() }),
                note: Some(String::new()),
                finished_at: Some(Some(now())),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
    let _ = state.db.crew_member_status(&crew.id, &me.id, status);
    let text = if result.is_empty() { summary.clone() } else { format!("{summary}\n\n{result}") };
    log(&state, &crew.id, Some(&task.id), &me.id, &crew.orchestrator_id, status, &text);
    // Dependents start before the orchestrator hears of it.
    pump(&app, &state, &crew.id);
    let tasks = state.db.crew_tasks(&crew.id).unwrap_or_default();
    let task = state.db.crew_task(&task.id).ok().flatten().unwrap_or(task);
    wake(&app, &state, &crew.orchestrator_id, &done_wake(&task, failed, &tasks));
    Ok("Reported. Your task is settled; end your turn.".into())
}

#[tauri::command(async)]
pub async fn pilot_agent_crew_wait(app: AppHandle, pilot_token: Option<String>, timeout_seconds: Option<u64>) -> Result<String, String> {
    let state = app.try_state::<PilotState>().ok_or("QuantPilot is not ready.")?;
    let me = me(&state, pilot_token)?;
    let crew = match led_crew(&state, &me) {
        Ok(c) => c,
        Err(e) => match my_task(&state, &me) {
            Ok((c, _)) => c,
            Err(_) => return Err(e),
        },
    };
    let wait = timeout_seconds.unwrap_or(0).min(WAIT_MAX_SECS);
    let start = crew.rev;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let rev = state.db.crew(&crew.id).ok().flatten().map(|c| c.rev).unwrap_or(start);
        if rev != start {
            break;
        }
    }
    let crew = state.db.crew(&crew.id).map_err(|e| e.to_string())?.ok_or("crew vanished")?;
    Ok(status_text(&state, &crew))
}

#[tauri::command(async)]
pub fn pilot_agent_crew_gate(app: AppHandle, state: State<'_, PilotState>, pilot_token: Option<String>, name: String, question: String) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let crew = led_crew(&state, &me)?;
    let name = clip(&checked_text(&name, "gate name")?, 60);
    let question = checked_text(&question, "question")?;
    let text = format!("{name}: {question}");
    let msg = state.db.crew_message_add(&crew.id, None, &me.id, "user", "gate", &text, None, "pending").map_err(|e| e.to_string())?;
    let mode = lock(&state.settings).notify.clone();
    notify::needs_you(&app, &mode, &me.id, &me.title, &format!("Decision: {name}"));
    emit(&app, &state, &crew.id);
    Ok(format!("Gate {} waits for the user. End your turn now; QuantPilot wakes this thread with the decision.", msg.id))
}

#[tauri::command(async)]
pub fn pilot_agent_crew_retry(
    app: AppHandle,
    state: State<'_, PilotState>,
    pilot_token: Option<String>,
    task: String,
    agent: Option<String>,
    model: Option<String>,
    note: Option<String>,
) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let crew = led_crew(&state, &me)?;
    let t = find_task(&state, &crew, &task)?;
    let t = retry(&app, &state, &t.id, agent, model, note, &me.id)?;
    Ok(format!("{} is {} again (attempt {}). End your turn; QuantPilot wakes this thread with its result.", t.key, t.status, t.attempts.max(1)))
}

#[tauri::command(async)]
pub fn pilot_agent_crew_stop(app: AppHandle, state: State<'_, PilotState>, pilot_token: Option<String>, task: Option<String>) -> Result<String, String> {
    let me = me(&state, pilot_token)?;
    let crew = led_crew(&state, &me)?;
    match task.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(k) => {
            let t = find_task(&state, &crew, k)?;
            stop(&app, &state, &crew.id, Some(&t.id), &me.id)?;
            Ok(format!("Stopped {}.", t.key))
        }
        None => {
            let n = stop(&app, &state, &crew.id, None, &me.id)?;
            Ok(format!("Stopped the crew: {n} open task(s)."))
        }
    }
}

// ── The user's side: panel, log, gates ──

#[tauri::command(async)]
pub fn pilot_crews(state: State<'_, PilotState>) -> Result<Vec<CrewSnapshot>, String> {
    let parallel = lock(&state.settings).crew_parallel;
    let crews = state.db.crews().map_err(|e| e.to_string())?;
    Ok(crews.iter().filter_map(|c| snapshot(&state.db, parallel, &c.id).ok().flatten()).collect())
}

#[tauri::command(async)]
pub fn pilot_crew_log(state: State<'_, PilotState>, crew_id: String) -> Result<Vec<CrewMessage>, String> {
    state.db.crew_messages(&crew_id, 0).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn pilot_crew_decide(app: AppHandle, state: State<'_, PilotState>, gate_id: String, approve: bool, note: Option<String>) -> Result<(), String> {
    let gate = state.db.crew_message(&gate_id).map_err(|e| e.to_string())?.filter(|m| m.kind == "gate").ok_or("No such gate.")?;
    if gate.status != "pending" {
        return Err("This gate is decided already.".into());
    }
    let crew = state.db.crew(&gate.crew_id).map_err(|e| e.to_string())?.ok_or("No such crew.")?;
    state.db.crew_message_status(&gate_id, if approve { "approved" } else { "rejected" }).map_err(|e| e.to_string())?;
    let note = note.map(|n| n.trim().to_string()).unwrap_or_default();
    let name = gate.text.split(':').next().unwrap_or("").trim().to_string();
    let said = format!("{} {name}{}", if approve { "Approved" } else { "Rejected" }, if note.is_empty() { String::new() } else { format!(": {note}") });
    let _ = state.db.crew_message_add(&crew.id, None, "user", &crew.orchestrator_id, "decision", &said, Some(&gate_id), "");
    release(&app, &state, &crew.orchestrator_id);
    wake(&app, &state, &crew.orchestrator_id, &decision_wake(&name, approve, &note));
    emit(&app, &state, &crew.id);
    Ok(())
}

#[tauri::command(async)]
pub fn pilot_crew_retry(app: AppHandle, state: State<'_, PilotState>, task_id: String, provider: Option<String>, model: Option<String>) -> Result<CrewTask, String> {
    retry(&app, &state, &task_id, provider, model, None, "user")
}

#[tauri::command(async)]
pub fn pilot_crew_stop(app: AppHandle, state: State<'_, PilotState>, crew_id: String, task_id: Option<String>) -> Result<(), String> {
    stop(&app, &state, &crew_id, task_id.as_deref(), "user")?;
    if task_id.is_none() {
        // The cascade: the orchestrator's own turn stops too.
        if let Ok(Some(crew)) = state.db.crew(&crew_id) {
            halt(&state, &crew.orchestrator_id);
        }
    }
    Ok(())
}

/// Go on after a stop or a restart: start what is ready.
#[tauri::command(async)]
pub fn pilot_crew_resume(app: AppHandle, state: State<'_, PilotState>, crew_id: String) -> Result<(), String> {
    let crew = state.db.crew(&crew_id).map_err(|e| e.to_string())?.ok_or("No such crew.")?;
    let tasks = state.db.crew_tasks(&crew_id).map_err(|e| e.to_string())?;
    // Stopped tasks wait again; finished ones stay finished.
    for t in tasks.iter().filter(|t| t.status == "stopped") {
        let _ = state.db.crew_task_update(&t.id, &TaskPatch { status: Some("waiting".into()), finished_at: Some(None), ..Default::default() });
    }
    state.db.crew_update(&crew.id, Some("running"), None, None, None).map_err(|e| e.to_string())?;
    log(&state, &crew.id, None, "user", "crew", "note", "Resumed the crew.");
    release(&app, &state, &crew.orchestrator_id);
    pump(&app, &state, &crew.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: &str, deps: &[&str]) -> TaskSpec {
        TaskSpec { key: key.into(), spec: format!("do {key}"), deps: deps.iter().map(|d| d.to_string()).collect(), ..Default::default() }
    }

    fn any(_: &str) -> bool {
        true
    }

    fn task(key: &str, status: &str, deps: &[&str]) -> CrewTask {
        CrewTask { id: format!("id-{key}"), key: key.into(), status: status.into(), deps: deps.iter().map(|d| d.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn keys_are_short_slugs() {
        assert_eq!(normalize_key(" Billing API ").unwrap(), "billing-api");
        assert_eq!(normalize_key("db/search.index").unwrap(), "db-search-index");
        assert!(normalize_key("  ").is_err());
        assert!(normalize_key("!!!").is_err());
        assert!(normalize_key(&"a".repeat(25)).is_err());
    }

    #[test]
    fn a_batch_checks_keys_deps_and_cycles() {
        let ok = plan_batch("c", &[], vec![spec("billing", &[]), spec("notify", &["billing"])], true, &any).unwrap();
        assert_eq!(ok.len(), 2);
        assert!(ok.iter().all(|t| t.status == "waiting" && t.worktree));
        assert_eq!(ok[1].deps, vec!["billing"]);
        assert!(plan_batch("c", &[], vec![spec("a", &[]), spec("A", &[])], true, &any).unwrap_err().contains("already"));
        assert!(plan_batch("c", &[], vec![spec("a", &["nope"])], true, &any).unwrap_err().contains("not a task"));
        assert!(plan_batch("c", &[], vec![spec("a", &["a"])], true, &any).unwrap_err().contains("itself"));
        let circle = plan_batch("c", &[], vec![spec("a", &["c"]), spec("b", &["a"]), spec("c", &["b"])], true, &any).unwrap_err();
        assert!(circle.contains("circle"), "{circle}");
        // A dependency on an existing task is fine; the key may not repeat.
        let existing = vec![task("db", "done", &[])];
        assert_eq!(plan_batch("c", &existing, vec![spec("search", &["db"])], true, &any).unwrap().len(), 1);
        assert!(plan_batch("c", &existing, vec![spec("db", &[])], true, &any).is_err());
        assert!(plan_batch("c", &[], vec![TaskSpec { key: "x".into(), ..Default::default() }], true, &any).unwrap_err().contains("no spec"));
        assert!(plan_batch("c", &[], Vec::new(), true, &any).is_err());
    }

    #[test]
    fn several_agents_fan_out_into_compared_worktree_tasks() {
        let s = TaskSpec { key: "fix".into(), spec: "fix it".into(), agents: vec!["claude".into(), "codex".into()], worktree: Some(false), ..Default::default() };
        let out = plan_batch("c", &[], vec![s], false, &any).unwrap();
        assert_eq!(out.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(), vec!["fix-claude", "fix-codex"]);
        assert_eq!(out.iter().map(|t| t.provider.as_str()).collect::<Vec<_>>(), vec!["claude", "codex"]);
        assert!(out.iter().all(|t| t.worktree), "compared runs always get worktrees");
        let unknown = TaskSpec { key: "x".into(), spec: "y".into(), agent: Some("ghost".into()), ..Default::default() };
        assert!(plan_batch("c", &[], vec![unknown], true, &|a| a != "ghost").unwrap_err().contains("ghost"));
    }

    #[test]
    fn a_card_task_uses_the_card_workflow_not_a_second_worktree() {
        let s = TaskSpec { key: "card".into(), spec: "do".into(), card: Some(" 607d54dd ".into()), ..Default::default() };
        let out = plan_batch("c", &[], vec![s], true, &any).unwrap();
        assert_eq!(out[0].card_id.as_deref(), Some("607d54dd"));
        assert!(!out[0].worktree);
    }

    #[test]
    fn ready_tasks_follow_the_graph() {
        let tasks = vec![task("a", "done", &[]), task("b", "waiting", &["a"]), task("c", "waiting", &["b"]), task("d", "waiting", &[]), task("e", "running", &[])];
        assert_eq!(ready(&tasks), vec!["id-b", "id-d"]);
        assert_eq!(wait_note(&tasks[2], &tasks, 1, 4), "waits for b");
        assert_eq!(wait_note(&tasks[3], &tasks, 4, 4), "waits for a free slot (4 of 4 running)");
        let failed = vec![task("a", "failed", &[]), task("b", "waiting", &["a"])];
        assert!(ready(&failed).is_empty());
        assert_eq!(wait_note(&failed[1], &failed, 0, 4), "blocked: a did not finish");
    }

    #[test]
    fn a_worker_that_stops_without_reporting_is_never_hidden() {
        assert_eq!(turn_end("running", false, TurnStatus::Completed), Some(("stalled", "ended its turn without crew_done")));
        assert_eq!(turn_end("running", true, TurnStatus::Completed), None, "a queued reply continues the work");
        assert_eq!(turn_end("running", false, TurnStatus::Failed).unwrap().0, "failed");
        assert_eq!(turn_end("running", false, TurnStatus::Interrupted).unwrap().0, "stalled");
        assert_eq!(turn_end("running", false, TurnStatus::Limited).unwrap().0, "stalled");
        for s in ["asking", "done", "failed", "stopped", "stalled", "waiting"] {
            assert_eq!(turn_end(s, false, TurnStatus::Completed), None, "{s}");
        }
    }

    #[test]
    fn wake_messages_say_what_to_do_next() {
        let tasks = vec![task("a", "done", &[]), task("b", "running", &[])];
        assert_eq!(overview(&tasks), "1 running (b), 1 done (a)");
        let mut t = task("a", "done", &[]);
        t.summary = "Billing works".into();
        t.result = "branch quantpilot/billing".into();
        let w = done_wake(&t, false, &tasks);
        assert!(w.starts_with("[Crew] a is done: Billing works") && w.contains("branch quantpilot/billing") && w.contains("End your turn"), "{w}");
        let all = vec![task("a", "done", &[]), task("b", "failed", &[])];
        let w = done_wake(&all[1], true, &all);
        assert!(w.contains("failed") && w.contains("crew_retry") && w.contains("Every task has settled"), "{w}");
        let q = ask_wake("b", "m-1", "Which table?");
        assert!(q.contains("question m-1") && q.contains("crew_reply") && q.contains("Which table?"));
        assert!(escalation_wake("b", "ended its turn without crew_done", "I am done", &tasks).contains("Its last words: I am done"));
        assert!(decision_wake("merge", true, "").ends_with("Continue."));
        assert!(decision_wake("merge", false, "not yet").contains("They said: not yet"));
        assert!(timeout_wake("b", 30, &tasks).contains("30-minute"));
    }

    #[test]
    fn the_worker_prompt_carries_the_task_and_the_protocol() {
        let mut t = task("billing", "running", &[]);
        t.spec = "Add the invoice endpoint.".into();
        let p = worker_prompt(&Brief { orchestrator: "Lead", objective: "Payments", task: &t, worktree: true });
        assert!(p.contains("led by the thread \"Lead\"") && p.contains("Crew objective: Payments") && p.contains("Your task: billing"));
        assert!(p.contains("Add the invoice endpoint.") && p.contains("own git worktree") && p.contains("crew_done") && p.contains("crew_ask"));
        t.card_id = Some("abc".into());
        t.brief = "An earlier attempt (codex) did not finish this task: tests failed.".into();
        let p = worker_prompt(&Brief { orchestrator: "Lead", objective: "", task: &t, worktree: false });
        assert!(p.contains("kanban card abc") && p.contains("An earlier attempt") && !p.contains("Crew objective"));
    }

    #[test]
    fn crews_tasks_members_and_the_log_persist() {
        let dir = std::env::temp_dir().join(format!("qp-crew-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pilot.db");
        let (crew_id, task_id) = {
            let db = Database::open(&path).unwrap();
            let crew = db.crew_create("orch", "ctx", "Payments", 2, false).unwrap();
            let tasks = plan_batch(&crew.id, &[], vec![spec("billing", &[]), spec("notify", &["billing"])], true, &any).unwrap();
            for t in &tasks {
                db.crew_task_add(t).unwrap();
            }
            assert!(db.crew_task_claim(&tasks[0].id, &crew.id, 1).unwrap());
            assert!(!db.crew_task_claim(&tasks[0].id, &crew.id, 1).unwrap(), "claimed once");
            assert!(!db.crew_task_claim(&tasks[1].id, &crew.id, 1).unwrap(), "no free slot");
            db.crew_task_update(&tasks[0].id, &TaskPatch { worker_id: Some(Some("w1".into())), progress: Some(40), attempts: Some(1), ..Default::default() }).unwrap();
            db.crew_member_add(&CrewMember { crew_id: crew.id.clone(), thread_id: "w1".into(), task_id: Some(tasks[0].id.clone()), role: "worker".into(), attempt: 1, status: "active".into(), provider: "claude".into(), joined_at: 1 }).unwrap();
            let q = db.crew_message_add(&crew.id, Some(&tasks[0].id), "w1", "orch", "ask", "Which table?", None, "open").unwrap();
            db.crew_message_add(&crew.id, None, "orch", "user", "gate", "merge: ok?", None, "pending").unwrap();
            assert_eq!(db.crew_open_requests(&crew.id).unwrap().len(), 2);
            db.crew_message_status(&q.id, "answered").unwrap();
            assert_eq!(db.crew_open_requests(&crew.id).unwrap().len(), 1);
            (crew.id, tasks[0].id.clone())
        };
        // A restart: everything is there; a running worker is stalled now.
        let db = Database::open(&path).unwrap();
        after_restart(&db);
        let crew = db.crew_led_by("orch").unwrap().unwrap();
        assert_eq!((crew.id.as_str(), crew.objective.as_str(), crew.max_parallel), (crew_id.as_str(), "Payments", 2));
        let t = db.crew_task(&task_id).unwrap().unwrap();
        assert_eq!((t.status.as_str(), t.progress, t.worker_id.as_deref()), ("stalled", 40, Some("w1")));
        assert_eq!(db.crew_task_of_worker("w1").unwrap().unwrap().id, task_id);
        let members = db.crew_members(&crew_id).unwrap();
        assert_eq!(members.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(), vec!["orchestrator", "worker"]);
        let log = db.crew_messages(&crew_id, 0).unwrap();
        assert_eq!(log.iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(), vec!["ask", "gate", "note"]);
        assert_eq!(log.iter().map(|m| m.n).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(db.crews_of_thread("w1").unwrap(), vec![crew_id.clone()]);
        let snap = snapshot(&db, 4, &crew_id).unwrap().unwrap();
        assert_eq!((snap.max_parallel, snap.tasks.len(), snap.message_count), (2, 2, 3));
        db.crew_delete(&crew_id).unwrap();
        assert!(db.crew(&crew_id).unwrap().is_none() && db.crew_tasks(&crew_id).unwrap().is_empty() && db.crew_messages(&crew_id, 0).unwrap().is_empty());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_schema_19_file_gains_the_crew_tables() {
        let dir = std::env::temp_dir().join(format!("qp-crew-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pilot.db");
        drop(Database::open(&path).unwrap());
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("DROP TABLE crew_messages; DROP TABLE crew_members; DROP TABLE crew_tasks; DROP TABLE crews; PRAGMA user_version = 19;").unwrap();
        }
        let db = Database::open(&path).unwrap();
        let crew = db.crew_create("o", "ctx", "", 0, false).unwrap();
        assert!(db.crew(&crew.id).unwrap().is_some());
        drop(db);
        let version: i64 = rusqlite::Connection::open(&path).unwrap().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, crate::db::SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workers_message_the_coordinator_not_each_other_unless_allowed() {
        let db = Database::open_in_memory().unwrap();
        let crew = db.crew_create("orch", "ctx", "", 0, false).unwrap();
        for (key, w) in [("a", "w1"), ("b", "w2")] {
            let mut t = task(key, "running", &[]);
            t.id = format!("t-{key}");
            t.crew_id = crew.id.clone();
            t.worker_id = Some(w.into());
            db.crew_task_add(&t).unwrap();
        }
        let meta = |id: &str| crate::db::tests::thread(id);
        assert!(check_worker_message(&db, &meta("w1"), &meta("orch"), "hi").is_ok());
        assert!(check_worker_message(&db, &meta("w1"), &meta("w2"), "hi").unwrap_err().contains("crew_ask"));
        assert!(check_worker_message(&db, &meta("w1"), &meta("elsewhere"), "hi").is_err());
        assert!(check_worker_message(&db, &meta("solo"), &meta("w2"), "hi").is_ok(), "not a worker: today's rules");
        db.crew_update(&crew.id, None, None, None, Some(true)).unwrap();
        assert!(check_worker_message(&db, &meta("w1"), &meta("w2"), "hi").is_ok());
        let kinds: Vec<String> = db.crew_messages(&crew.id, 0).unwrap().into_iter().map(|m| m.kind).collect();
        assert_eq!(kinds, vec!["note", "peer"]);
    }
}
