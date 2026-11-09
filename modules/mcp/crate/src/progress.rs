//! Agent progress (docs/PLAN-AGENT-STATUS.md): what every agent is doing, as
//! bars the user watches instead of asking "wie ist der stand".
//!
//! Three sources feed one board:
//! - the agent itself, through the `report_progress` tool: a title, step k of
//!   n, a percent, an estimate, done or failed;
//! - the work itself: a file a bar follows (a Smithery run's `events.jsonl`,
//!   a log with `PROGRESS 12/48` lines) fills the current step while the
//!   agent only waits on it, and any script can POST the same report to
//!   `/progress/report`;
//! - the agent's shells, through the client hooks (`/quantprogress/...`,
//!   installed under QuantMCP › Settings): every Bash/PowerShell call shows
//!   up under its agent with its running time, without the agent doing
//!   anything.
//!
//! ONE template renders the board (packages/ui/src/progress/board.js) in two
//! hosts: the drawer's Agent › Status view (`progress_board`) and the page
//! this module serves at `/progress`. Both poll once a second.
//!
//! A hook knows its client session and folder, an MCP call knows its agent
//! and folder — there is no shared key. A hook session joins an agent's lane
//! only when exactly one lane of the same client claims its folder (equal,
//! or the lane's folder holds it); otherwise its shells stay a lane of their
//! own, labelled by client and folder. Agents pass `folder` (their worktree)
//! to make the join exact.

use crate::memory_hook::{local_request, loopback_host, HookClient, HookEvent, HookFeature};
use axum::{
    body::Bytes,
    extract::{Path as UrlPath, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The shell hooks post here (`HookFeature::Shells`).
pub const HOOK_SEGMENT: &str = "quantprogress";

/// The board as a page, for a browser on this machine.
pub const PAGE_PATH: &str = "/progress";

const SECOND: i64 = 1000;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;

/// A running bar with no sign of life for this long is marked stale: no
/// report, no running shell of its agent, no change in its followed file.
pub const STALE_AFTER: i64 = 15 * MINUTE;
/// Finished bars stay (grey) this long, failed ones (red) longer.
const KEEP_DONE: i64 = 30 * MINUTE;
const KEEP_FAILED: i64 = 2 * HOUR;
/// An ended shell stays this long, then only the bar remains.
const KEEP_ENDED_SHELL: i64 = 2 * MINUTE;
/// A foreground shell whose end never arrived (the agent was stopped):
/// clients cap a foreground call well below this.
const LOST_FOREGROUND: i64 = 30 * MINUTE;
/// A background shell has no end event: it goes with its session (Claude
/// Code's SessionEnd) or after this.
const KEEP_BACKGROUND: i64 = 12 * HOUR;
/// A running bar nobody touched for this long was abandoned.
const ABANDONED: i64 = 24 * HOUR;
const MAX_TASKS: usize = 12;
const MAX_SHELLS: usize = 12;
const MAX_TEXT: usize = 160;
const MAX_COMMAND: usize = 300;
/// A followed file is read from its end, at most this much.
const TAIL_BYTES: u64 = 64 * 1024;
/// The fraction a rate-based estimate needs before it is shown.
const MIN_RATE_FRACTION: f64 = 0.03;
const MIN_RATE_ELAPSED: i64 = 30 * SECOND;

// ── model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    Done,
    Failed,
}

/// One bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub step: Option<u32>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub step_text: Option<String>,
    /// Inside the current step, 0–1, as the agent reported it.
    #[serde(default)]
    pub step_fraction: Option<f64>,
    /// Overall 0–100 for a bar without steps, as the agent reported it.
    #[serde(default)]
    pub percent: Option<f64>,
    /// The agent's estimate of the whole run, from its start.
    #[serde(default)]
    pub estimate_secs: Option<i64>,
    /// A file whose progress lines fill the current step.
    #[serde(default)]
    pub follow: Option<String>,
    /// What the followed file said last: a fraction 0–1 and its newest line.
    #[serde(default)]
    pub follow_fraction: Option<f64>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub follow_changed_at: Option<i64>,
    pub state: TaskState,
    #[serde(default)]
    pub note: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
}

impl Task {
    fn new(id: String, title: String, now: i64) -> Task {
        Task {
            id,
            title,
            step: None,
            steps: None,
            step_text: None,
            step_fraction: None,
            percent: None,
            estimate_secs: None,
            follow: None,
            follow_fraction: None,
            detail: None,
            follow_changed_at: None,
            state: TaskState::Running,
            note: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
        }
    }

    fn clear_step_progress(&mut self) {
        self.step_text = None;
        self.step_fraction = None;
        self.clear_follow();
    }

    fn clear_follow(&mut self) {
        self.follow = None;
        self.follow_fraction = None;
        self.detail = None;
        self.follow_changed_at = None;
    }

    /// The whole bar, 0–1, or `None` when nothing says how far it is.
    ///
    /// With `steps`: the finished steps plus the current one's progress (the
    /// followed file, else `step_percent`) — step 3 of 5 at 50 % is 2.5 / 5.
    /// Without: the followed file, else `percent`.
    pub fn overall(&self) -> Option<f64> {
        if self.state == TaskState::Done {
            return Some(1.0);
        }
        let inner = self.follow_fraction.or(self.step_fraction);
        let value = match self.steps.filter(|n| *n > 0) {
            Some(steps) => {
                let step = self.step.unwrap_or(1).clamp(1, steps);
                Some((f64::from(step - 1) + inner.unwrap_or(0.0)) / f64::from(steps))
            }
            None => self.follow_fraction.or(self.percent.map(|p| p / 100.0)).or(self.step_fraction),
        };
        value.map(|v| v.clamp(0.0, 1.0))
    }

    /// The whole run's length in seconds: done → what it took; running → the
    /// agent's estimate while it holds, else elapsed ÷ progress once there is
    /// enough of both to be worth showing.
    pub fn total_secs(&self, now: i64) -> Option<i64> {
        if let Some(end) = self.finished_at {
            return Some(((end - self.started_at).max(0)) / SECOND);
        }
        let elapsed = (now - self.started_at).max(0);
        let by_rate = self
            .overall()
            .filter(|f| *f >= MIN_RATE_FRACTION && elapsed >= MIN_RATE_ELAPSED)
            .map(|f| ((elapsed as f64 / f) / SECOND as f64).round() as i64);
        match self.estimate_secs {
            Some(estimate) if elapsed <= estimate * SECOND => Some(estimate),
            _ => by_rate,
        }
    }

    fn last_sign_of_life(&self) -> i64 {
        self.updated_at.max(self.follow_changed_at.unwrap_or(0))
    }
}

/// One shell call of an agent, from the hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shell {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub background: bool,
    pub started_at: i64,
    #[serde(default)]
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub failed: bool,
}

/// One agent's bars and shells. `agent:<id>` lanes come from reports,
/// `hook:<client>:<session>` lanes from the shell hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    pub key: String,
    pub agent: String,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub shells: Vec<Shell>,
    pub updated_at: i64,
}

impl Lane {
    fn new(key: String, agent: String, client: Option<String>, folder: Option<String>, now: i64) -> Lane {
        Lane { key, agent, client, folder, tasks: Vec::new(), shells: Vec::new(), updated_at: now }
    }

    fn is_hook(&self) -> bool {
        self.key.starts_with("hook:")
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    #[serde(default)]
    pub lanes: Vec<Lane>,
    /// Hook lane → agent lane, once a hook session joined an agent.
    #[serde(default)]
    pub bindings: HashMap<String, String>,
    /// MCP session → the agent lane it last reported to, so a later report
    /// without `agent_id` lands on the same lane instead of a second one.
    #[serde(default)]
    pub sessions: HashMap<String, String>,
}

// ── reports ─────────────────────────────────────────────────────────────

/// One `report_progress` call (or POST to `/progress/report`). Everything is
/// optional; what is given changes the bar, what is left out stays.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub id: Option<String>,
    pub title: Option<String>,
    pub agent: Option<String>,
    pub folder: Option<String>,
    pub step: Option<u32>,
    pub steps: Option<u32>,
    pub step_text: Option<String>,
    pub step_percent: Option<f64>,
    pub percent: Option<f64>,
    pub estimate_secs: Option<i64>,
    /// `Some(None)`: stop following.
    pub follow: Option<Option<String>>,
    pub state: Option<TaskState>,
    pub note: Option<String>,
}

fn text_arg(args: &Value, key: &str) -> Option<String> {
    match args.get(key)? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn number_arg(args: &Value, key: &str) -> Result<Option<f64>, String> {
    let parsed = match args.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) if s.trim().is_empty() => return Ok(None),
        Some(Value::String(s)) => s.trim().trim_end_matches('%').trim().parse::<f64>().ok(),
        Some(_) => None,
    };
    parsed
        .filter(|v| v.is_finite())
        .map(Some)
        .ok_or_else(|| format!("`{key}` must be a number"))
}

fn whole_arg(args: &Value, key: &str) -> Result<Option<u32>, String> {
    number_arg(args, key)?
        .map(|v| if v >= 0.0 && v <= f64::from(u32::MAX) { Ok(v.round() as u32) } else { Err(format!("`{key}` must be 0 or more")) })
        .transpose()
}

/// "3h", "1h30m", "90m", "45s", "2.5h", "~3h", "1d"; a bare number is minutes.
pub fn parse_duration(text: &str) -> Option<i64> {
    let s: String = text.trim().trim_start_matches('~').to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    if let Ok(minutes) = s.parse::<f64>() {
        return (minutes.is_finite() && minutes >= 0.0).then(|| (minutes * 60.0).round() as i64);
    }
    let mut total = 0.0;
    let mut chars = s.chars().peekable();
    let mut any = false;
    while chars.peek().is_some() {
        let mut number = String::new();
        while let Some(c) = chars.peek().copied().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',') {
            number.push(if c == ',' { '.' } else { c });
            chars.next();
        }
        let mut unit = String::new();
        while let Some(c) = chars.peek().copied().filter(|c| c.is_alphabetic()) {
            unit.push(c);
            chars.next();
        }
        let value: f64 = number.parse().ok()?;
        let seconds = match unit.as_str() {
            "d" | "day" | "days" => 86_400.0,
            "h" | "hr" | "hrs" | "hour" | "hours" | "std" => 3600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            _ => return None,
        };
        total += value * seconds;
        any = true;
    }
    (any && total.is_finite()).then(|| total.round() as i64)
}

fn parse_state(text: &str) -> Result<TaskState, String> {
    match text.trim().to_lowercase().as_str() {
        "running" | "run" | "started" | "start" | "working" | "in_progress" => Ok(TaskState::Running),
        "done" | "finished" | "complete" | "completed" | "ok" | "success" => Ok(TaskState::Done),
        "failed" | "fail" | "error" | "aborted" | "cancelled" | "canceled" => Ok(TaskState::Failed),
        other => Err(format!("`state` must be running, done or failed — not {other:?}")),
    }
}

impl Report {
    pub fn parse(args: &Value) -> Result<Report, String> {
        let estimate_secs = match args.get("estimate") {
            None | Some(Value::Null) => None,
            Some(Value::Number(n)) => n.as_f64().filter(|v| v.is_finite() && *v >= 0.0).map(|m| (m * 60.0).round() as i64),
            Some(Value::String(s)) if s.trim().is_empty() => None,
            Some(Value::String(s)) => Some(parse_duration(s).ok_or_else(|| {
                format!("`estimate` {s:?} is not a duration — write e.g. \"3h\", \"1h30m\" or \"45m\"")
            })?),
            Some(_) => return Err("`estimate` must be a duration such as \"3h\"".into()),
        };
        let follow = match args.get("follow") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.trim().is_empty() => Some(None),
            Some(Value::String(s)) => Some(Some(s.trim().to_string())),
            Some(_) => return Err("`follow` must be a file path".into()),
        };
        Ok(Report {
            id: text_arg(args, "id"),
            title: text_arg(args, "title").map(|t| clip(&t, MAX_TEXT)),
            agent: text_arg(args, "agent_id").or_else(|| text_arg(args, "agent")).map(|a| clip(&a, 60)),
            folder: text_arg(args, "folder"),
            step: whole_arg(args, "step")?,
            steps: whole_arg(args, "steps")?,
            step_text: text_arg(args, "step_text").map(|t| clip(&t, MAX_TEXT)),
            step_percent: number_arg(args, "step_percent")?,
            percent: number_arg(args, "percent")?,
            estimate_secs,
            follow,
            state: text_arg(args, "state").map(|s| parse_state(&s)).transpose()?,
            note: text_arg(args, "note").map(|t| clip(&t, MAX_TEXT)),
        })
    }
}

/// Who reports: the MCP session's client and folder. Empty for a script.
#[derive(Debug, Default, Clone)]
pub struct Caller {
    /// The `clients.rs` id (`claude-code`, `codex-cli`).
    pub client: Option<String>,
    /// The client's display name — the lane's label when no `agent_id` is given.
    pub client_name: Option<String>,
    pub folder: Option<String>,
    /// The MCP session the report came in on.
    pub session: Option<String>,
}

fn clip(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        format!("{}…", one_line.chars().take(max - 1).collect::<String>())
    }
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(60).collect()
}

/// Folders compare as the registry compares them: `/`, no trailing slash,
/// lowercase.
fn folder_key(folder: &str) -> String {
    qs_core::workspaces::normalize_path(folder.trim())
}

/// The lane a report without `agent_id` belongs to: the one its MCP session
/// reported to before, else the one lane of its client whose running bar has
/// the same id. Agents often name themselves only on their first call.
fn remembered_lane(board: &Board, caller: &Caller, id: Option<&str>) -> Option<(String, String)> {
    let session_lane = caller
        .session
        .as_deref()
        .and_then(|session| board.sessions.get(session))
        .and_then(|key| board.lanes.iter().find(|lane| &lane.key == key));
    if let Some(lane) = session_lane {
        return Some((lane.key.clone(), lane.agent.clone()));
    }
    let id = id?;
    let owners: Vec<&Lane> = board
        .lanes
        .iter()
        .filter(|lane| !lane.is_hook())
        .filter(|lane| caller.client.is_none() || lane.client.is_none() || lane.client == caller.client)
        .filter(|lane| lane.tasks.iter().any(|t| t.id == id && t.state == TaskState::Running))
        .collect();
    match owners.as_slice() {
        [only] => Some((only.key.clone(), only.agent.clone())),
        _ => None,
    }
}

/// Apply a report; the bar it touched, or why nothing was changed.
pub fn apply_report(board: &mut Board, report: &Report, caller: &Caller, now: i64) -> Result<Task, String> {
    let named_id = report.id.clone().or_else(|| report.title.as_deref().map(slug).filter(|s| !s.is_empty()));
    let (key, agent) = match &report.agent {
        Some(agent) => (format!("agent:{}", agent.to_lowercase()), agent.clone()),
        None => remembered_lane(board, caller, named_id.as_deref()).unwrap_or_else(|| {
            let agent = caller.client_name.clone().unwrap_or_else(|| "script".into());
            (format!("agent:{}", agent.to_lowercase()), agent)
        }),
    };
    // The session named itself differently before: its bar of this id moves
    // over instead of staying behind as a second, stuck one.
    let moved = match (&caller.session, &named_id) {
        (Some(session), Some(id)) => board
            .sessions
            .get(session)
            .filter(|old| **old != key)
            .cloned()
            .and_then(|old| board.lanes.iter_mut().find(|lane| lane.key == old))
            .and_then(|old| old.tasks.iter().position(|t| &t.id == id).map(|at| old.tasks.remove(at))),
        _ => None,
    };
    if let Some(session) = &caller.session {
        board.sessions.insert(session.clone(), key.clone());
    }
    let folder = report.folder.clone().or_else(|| caller.folder.clone());
    let lane_index = match board.lanes.iter().position(|lane| lane.key == key) {
        Some(index) => index,
        None => {
            board.lanes.push(Lane::new(key, agent.clone(), caller.client.clone(), folder.clone(), now));
            board.lanes.len() - 1
        }
    };
    let lane = &mut board.lanes[lane_index];
    lane.agent = agent;
    if report.folder.is_some() || lane.folder.is_none() {
        lane.folder = folder.or(lane.folder.take());
    }
    if lane.client.is_none() {
        lane.client = caller.client.clone();
    }
    if let Some(task) = moved {
        if !lane.tasks.iter().any(|t| t.id == task.id) {
            lane.tasks.push(task);
        }
    }

    let id = match named_id {
        Some(id) => id,
        None => {
            let running: Vec<&Task> = lane.tasks.iter().filter(|t| t.state == TaskState::Running).collect();
            match running.as_slice() {
                [only] => only.id.clone(),
                [] => return Err("Give a `title` for a new bar.".into()),
                _ => return Err("Several bars are running: give the `title` or `id` of the one you mean.".into()),
            }
        }
    };
    let restart = report.state.unwrap_or(TaskState::Running) == TaskState::Running;
    let index = match lane.tasks.iter().position(|t| t.id == id) {
        Some(index) => {
            // A finished bar reported as running again is a new run of it.
            if restart && lane.tasks[index].state != TaskState::Running {
                let title = report.title.clone().unwrap_or_else(|| lane.tasks[index].title.clone());
                lane.tasks[index] = Task::new(id, title, now);
            }
            index
        }
        None => {
            let title = report.title.clone().unwrap_or_else(|| id.clone());
            lane.tasks.push(Task::new(id, title, now));
            lane.tasks.len() - 1
        }
    };
    let task = &mut lane.tasks[index];
    if let Some(title) = &report.title {
        task.title = title.clone();
    }
    if let Some(steps) = report.steps {
        task.steps = (steps > 0).then_some(steps);
    }
    if let Some(step) = report.step {
        // A new step starts empty: its text, percent and followed file belong
        // to the step before unless this report gives them again.
        if task.step != Some(step) {
            task.clear_step_progress();
        }
        task.step = Some(step.max(1));
    }
    if let Some(text) = &report.step_text {
        task.step_text = Some(text.clone());
    }
    if let Some(percent) = report.step_percent {
        task.step_fraction = Some((percent / 100.0).clamp(0.0, 1.0));
    }
    if let Some(percent) = report.percent {
        task.percent = Some(percent.clamp(0.0, 100.0));
    }
    if let Some(estimate) = report.estimate_secs {
        task.estimate_secs = Some(estimate);
    }
    match &report.follow {
        Some(Some(path)) if task.follow.as_deref() != Some(path.as_str()) => {
            task.clear_follow();
            task.follow = Some(path.clone());
        }
        Some(None) => task.clear_follow(),
        _ => {}
    }
    if let Some(note) = &report.note {
        task.note = Some(note.clone());
    }
    match report.state {
        Some(TaskState::Done) => {
            task.state = TaskState::Done;
            task.finished_at = Some(now);
            if let Some(steps) = task.steps {
                task.step = Some(steps);
            }
        }
        Some(TaskState::Failed) => {
            task.state = TaskState::Failed;
            task.finished_at = Some(now);
        }
        _ => {}
    }
    task.updated_at = now;
    let touched = task.clone();
    lane.updated_at = now;
    trim_tasks(lane);
    let all_over = touched.state != TaskState::Running && !lane.tasks.iter().any(|t| t.state == TaskState::Running);
    if all_over {
        let key = lane.key.clone();
        end_background_shells(board, &key, now);
    }
    Ok(touched)
}

/// The agent says its work is over: the background commands it started
/// (in its own lane or a hook session joined to it) count as over too. A
/// background launch has no hook event of its own when it finishes.
fn end_background_shells(board: &mut Board, agent_key: &str, now: i64) {
    let joined: Vec<String> =
        board.bindings.iter().filter(|(_, agent)| agent.as_str() == agent_key).map(|(hook, _)| hook.clone()).collect();
    for lane in board.lanes.iter_mut().filter(|lane| lane.key == agent_key || joined.contains(&lane.key)) {
        let mut ended = false;
        for shell in lane.shells.iter_mut().filter(|s| s.background && s.ended_at.is_none()) {
            shell.ended_at = Some(now);
            ended = true;
        }
        if ended {
            lane.updated_at = now;
        }
    }
}

/// At most `MAX_TASKS` per lane: running bars first, then the newest
/// finished ones.
fn trim_tasks(lane: &mut Lane) {
    if lane.tasks.len() <= MAX_TASKS {
        return;
    }
    lane.tasks.sort_by_key(|t| (t.state != TaskState::Running, -t.finished_at.unwrap_or(t.updated_at)));
    lane.tasks.truncate(MAX_TASKS);
    lane.tasks.sort_by_key(|t| t.started_at);
}

/// One line for the agent: what the bar shows now.
pub fn describe(task: &Task, now: i64) -> String {
    let mut parts = Vec::new();
    match (task.step, task.steps) {
        (Some(step), Some(steps)) => parts.push(format!("step {step}/{steps}")),
        (Some(step), None) => parts.push(format!("step {step}")),
        _ => {}
    }
    if let Some(text) = &task.step_text {
        parts.push(text.clone());
    }
    match task.overall() {
        Some(fraction) => parts.push(format!("{}%", (fraction * 100.0).floor())),
        None => parts.push("no percent yet".into()),
    }
    if let Some(total) = task.total_secs(now) {
        parts.push(format!("~{} total", short_duration(total)));
    }
    let state = match task.state {
        TaskState::Running => "running",
        TaskState::Done => "done",
        TaskState::Failed => "failed",
    };
    format!("\"{}\" {state}: {}", task.title, parts.join(" · "))
}

fn short_duration(secs: i64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if (s % 3600) / 60 == 0 => format!("{}h", s / 3600),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

// ── shell hooks ─────────────────────────────────────────────────────────

fn is_shell_tool(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "bash" | "powershell" | "shell" | "local_shell" | "exec_command" | "unified_exec" | "container.exec" | "run_terminal_cmd"
    )
}

/// The command line of a shell tool call: a string, or an argv whose last
/// element is the script when it runs through `bash -lc` and the like.
fn shell_command(input: &Value) -> Option<String> {
    let raw = match input.get("command").or_else(|| input.get("cmd"))? {
        Value::String(s) => s.clone(),
        Value::Array(parts) => {
            let words: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            match words.as_slice() {
                [shell, flag, script] if flag.starts_with('-') && !shell.is_empty() => script.to_string(),
                _ => words.join(" "),
            }
        }
        _ => return None,
    };
    let mut lines = raw.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    let more = lines.next().is_some();
    let command = clip(first, MAX_COMMAND);
    Some(if more { format!("{command} …") } else { command })
}

fn str_field<'a>(body: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| body.get(*key).and_then(Value::as_str)).filter(|s| !s.is_empty())
}

fn client_label(client: &str) -> String {
    crate::clients::spec(client).map_or_else(|| client.to_string(), |spec| spec.name.to_string())
}

/// Apply one shell-hook event; whether the board changed.
pub fn apply_hook(board: &mut Board, client: &str, event: HookEvent, body: &Value, now: i64) -> bool {
    let session = str_field(body, &["session_id", "sessionId", "conversation_id"]).unwrap_or("session");
    let key = format!("hook:{client}:{session}");
    if event == HookEvent::SessionEnd {
        let before = board.lanes.len();
        board.lanes.retain(|lane| lane.key != key);
        board.bindings.remove(&key);
        return board.lanes.len() != before;
    }
    if !matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse | HookEvent::PostToolUseFailure) {
        return false;
    }
    if !str_field(body, &["tool_name", "toolName"]).is_some_and(is_shell_tool) {
        return false;
    }
    let input = body.get("tool_input").or_else(|| body.get("toolInput")).cloned().unwrap_or(Value::Null);
    let Some(command) = shell_command(&input) else { return false };
    let call = str_field(body, &["tool_use_id", "toolUseId", "call_id", "tool_call_id"]).map(str::to_string);
    let background = input.get("run_in_background").and_then(Value::as_bool).unwrap_or(false);
    let cwd = str_field(body, &["cwd"]).map(str::to_string);

    if event == HookEvent::PreToolUse {
        let index = match board.lanes.iter().position(|lane| lane.key == key) {
            Some(index) => index,
            None => {
                board.lanes.push(Lane::new(key, client_label(client), Some(client.to_string()), cwd.clone(), now));
                board.lanes.len() - 1
            }
        };
        let lane = &mut board.lanes[index];
        if cwd.is_some() {
            lane.folder = cwd.clone();
        }
        let id = call.unwrap_or_else(|| format!("{now}-{}", lane.shells.len()));
        lane.shells.retain(|shell| shell.id != id);
        lane.shells.push(Shell { id, command, cwd, background, started_at: now, ended_at: None, failed: false });
        if lane.shells.len() > MAX_SHELLS {
            // Ended ones go first, then the oldest.
            lane.shells.sort_by_key(|s| (s.ended_at.is_none(), s.started_at));
            let excess = lane.shells.len() - MAX_SHELLS;
            lane.shells.drain(..excess);
            lane.shells.sort_by_key(|s| s.started_at);
        }
        lane.updated_at = now;
        return true;
    }

    let Some(lane) = board.lanes.iter_mut().find(|lane| lane.key == key) else { return false };
    let found = match &call {
        Some(id) => lane.shells.iter_mut().find(|s| &s.id == id),
        None => lane
            .shells
            .iter_mut()
            .filter(|s| s.ended_at.is_none() && s.command == command)
            .min_by_key(|s| s.started_at),
    };
    let Some(shell) = found else { return false };
    let failed = event == HookEvent::PostToolUseFailure;
    // A background command's "post" is its launch: it keeps running.
    if shell.background && !failed {
        return false;
    }
    shell.ended_at = Some(now);
    shell.failed = failed;
    lane.updated_at = now;
    true
}

// ── upkeep ──────────────────────────────────────────────────────────────

/// Drop what is over; whether anything went.
pub fn prune(board: &mut Board, now: i64) -> bool {
    let mut changed = false;
    for lane in &mut board.lanes {
        let before = (lane.tasks.len(), lane.shells.len());
        lane.tasks.retain(|task| match (task.state, task.finished_at) {
            (TaskState::Running, _) => now - task.last_sign_of_life() < ABANDONED,
            (TaskState::Done, Some(end)) => now - end < KEEP_DONE,
            (TaskState::Failed, Some(end)) => now - end < KEEP_FAILED,
            (_, None) => false,
        });
        lane.shells.retain(|shell| match shell.ended_at {
            Some(end) => now - end < KEEP_ENDED_SHELL,
            None if shell.background => now - shell.started_at < KEEP_BACKGROUND,
            None => now - shell.started_at < LOST_FOREGROUND,
        });
        changed |= before != (lane.tasks.len(), lane.shells.len());
    }
    let before = board.lanes.len();
    board.lanes.retain(|lane| !lane.tasks.is_empty() || !lane.shells.is_empty());
    changed |= board.lanes.len() != before;
    let keys: Vec<String> = board.lanes.iter().map(|lane| lane.key.clone()).collect();
    let bound = board.bindings.len();
    board.bindings.retain(|hook, agent| keys.contains(hook) && keys.contains(agent));
    let remembered = board.sessions.len();
    board.sessions.retain(|_, lane| keys.contains(lane));
    changed | (board.bindings.len() != bound) | (board.sessions.len() != remembered)
}

/// How a lane's folder relates to a hook session's folder: 2 the same,
/// 1 the lane's holds the session's (a worktree inside the repository).
fn relation(lane_folder: &str, hook_folder: &str) -> u8 {
    let (lane, hook) = (folder_key(lane_folder), folder_key(hook_folder));
    if lane == hook {
        2
    } else if hook.starts_with(&format!("{lane}/")) {
        1
    } else {
        0
    }
}

/// Join hook sessions to agent lanes where exactly one lane claims them;
/// whether a binding was added.
pub fn bind_hooks(board: &mut Board) -> bool {
    let mut added = false;
    for hook in board.lanes.iter().filter(|lane| lane.is_hook()) {
        if board.bindings.contains_key(&hook.key) {
            continue;
        }
        let Some(hook_folder) = hook.folder.as_deref() else { continue };
        let mut best: Vec<(u8, usize, &Lane)> = board
            .lanes
            .iter()
            .filter(|lane| !lane.is_hook())
            .filter(|lane| lane.client.is_none() || lane.client == hook.client)
            .filter_map(|lane| {
                let folder = lane.folder.as_deref()?;
                let rank = relation(folder, hook_folder);
                (rank > 0).then(|| (rank, folder_key(folder).len(), lane))
            })
            .collect();
        best.sort_by_key(|a| std::cmp::Reverse((a.0, a.1)));
        let unique = match best.as_slice() {
            [first, second, ..] => (first.0, first.1) != (second.0, second.1),
            [_] => true,
            [] => false,
        };
        if unique {
            let target = best[0].2.key.clone();
            board.bindings.insert(hook.key.clone(), target);
            added = true;
        }
    }
    added
}

/// Read the followed files of running bars.
pub fn refresh_follows(board: &mut Board) {
    for task in board.lanes.iter_mut().flat_map(|lane| lane.tasks.iter_mut()) {
        if task.state != TaskState::Running {
            continue;
        }
        let Some(path) = task.follow.clone() else { continue };
        if let Some(read) = read_follow(Path::new(&path)) {
            if read.fraction.is_some() {
                task.follow_fraction = read.fraction;
            }
            if read.line.is_some() {
                task.detail = read.line;
            }
            task.follow_changed_at = Some(read.modified);
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct FollowRead {
    pub fraction: Option<f64>,
    pub line: Option<String>,
    pub modified: i64,
}

/// The newest progress in a followed file. A folder means its
/// `events.jsonl` (a Smithery lab run). From the end: the first progress
/// line gives the fraction — a JSON line with `fraction` (Smithery's
/// `{"event":"progress","fraction":…,"message":…}`) or a `PROGRESS 12/48`
/// / `PROGRESS 45%` marker — and the newest readable line is the detail.
pub fn read_follow(path: &Path) -> Option<FollowRead> {
    let file_path: PathBuf = if path.is_dir() { path.join("events.jsonl") } else { path.to_path_buf() };
    let mut file = std::fs::File::open(&file_path).ok()?;
    let meta = file.metadata().ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    let start = meta.len().saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let (fraction, line) = scan_progress(&text);
    Some(FollowRead { fraction, line, modified })
}

/// The fraction and detail line of a log's tail (see [`read_follow`]).
pub fn scan_progress(text: &str) -> (Option<f64>, Option<String>) {
    let mut fraction = None;
    let mut line_out = None;
    // `\r` splits too: progress bars redraw a line with carriage returns.
    for raw in text.rsplit(['\n', '\r']) {
        let line = strip_ansi(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (found, message) = progress_in_line(line);
        if line_out.is_none() {
            line_out = match &message {
                Some(m) if !m.is_empty() => Some(clip(m, MAX_TEXT)),
                // A JSON line that is no progress is no readable detail.
                _ if line.starts_with('{') => None,
                _ => Some(clip(line, MAX_TEXT)),
            };
        }
        if let Some(found) = found {
            fraction = Some(found);
            break;
        }
    }
    (fraction, line_out)
}

/// A line's progress fraction and message, if it carries one.
fn progress_in_line(line: &str) -> (Option<f64>, Option<String>) {
    if line.starts_with('{') {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return (None, None) };
        let message = value.get("message").and_then(Value::as_str).map(str::to_string);
        let kind = value.get("event").or_else(|| value.get("type")).and_then(Value::as_str);
        let fraction = value
            .get("fraction")
            .and_then(Value::as_f64)
            .or_else(|| value.get("percent").and_then(Value::as_f64).map(|p| p / 100.0));
        return match fraction {
            Some(f) if f.is_finite() => (Some(f.clamp(0.0, 1.0)), message),
            _ if kind == Some("progress") => (None, message),
            _ => (None, None),
        };
    }
    let Some(at) = line.find("PROGRESS ") else { return (None, None) };
    let rest = line[at + "PROGRESS ".len()..].trim();
    let (token, message) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let fraction = if let Some(percent) = token.strip_suffix('%') {
        percent.parse::<f64>().ok().map(|p| p / 100.0)
    } else if let Some((done, total)) = token.split_once('/') {
        match (done.parse::<f64>(), total.parse::<f64>()) {
            (Ok(done), Ok(total)) if total > 0.0 => Some(done / total),
            _ => None,
        }
    } else {
        None
    };
    match fraction.filter(|f| f.is_finite()) {
        Some(f) => (Some(f.clamp(0.0, 1.0)), Some(message.trim().to_string())),
        None => (None, None),
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

// ── view ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    #[serde(flatten)]
    pub task: Task,
    pub overall: Option<f64>,
    pub total_secs: Option<i64>,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneView {
    pub key: String,
    pub agent: String,
    pub client: Option<String>,
    pub folder: Option<String>,
    /// The folder for people: `QuantSuite`, `QuantSuite · worktree 84ac3068`.
    pub place: Option<String>,
    pub tasks: Vec<TaskView>,
    pub shells: Vec<Shell>,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardView {
    pub now: i64,
    pub lanes: Vec<LaneView>,
    /// The same board as a page (`/progress` on this QuantMCP).
    pub page_url: Option<String>,
}

fn place(folder: &str) -> String {
    let parts: Vec<&str> = folder.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if let Some(at) = parts.iter().position(|p| p.eq_ignore_ascii_case(".qs-worktrees")) {
        let repo = at.checked_sub(1).map_or("", |i| parts[i]);
        if let Some(tree) = parts.get(at + 1) {
            return format!("{repo} · worktree {tree}");
        }
    }
    parts.last().map_or_else(|| folder.to_string(), |p| p.to_string())
}

fn running_shells(lane: &Lane) -> bool {
    lane.shells.iter().any(|s| s.ended_at.is_none())
}

/// The board as both hosts render it.
pub fn view(board: &Board, now: i64, page_url: Option<String>) -> BoardView {
    let mut lanes: Vec<LaneView> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    let ordered = board.lanes.iter().filter(|l| !l.is_hook()).chain(board.lanes.iter().filter(|l| l.is_hook()));
    for lane in ordered {
        let target = board.bindings.get(&lane.key).and_then(|key| index.get(key.as_str()).copied());
        let shells_alive = running_shells(lane);
        match target {
            Some(at) => {
                let view = &mut lanes[at];
                view.shells.extend(lane.shells.iter().cloned());
                view.updated_at = view.updated_at.max(lane.updated_at);
                if shells_alive {
                    // A running shell is a sign of life for its agent's bars.
                    for task in &mut view.tasks {
                        task.stale = false;
                    }
                }
            }
            None => {
                let tasks = lane
                    .tasks
                    .iter()
                    .map(|task| TaskView {
                        overall: task.overall(),
                        total_secs: task.total_secs(now),
                        stale: task.state == TaskState::Running
                            && !shells_alive
                            && now - task.last_sign_of_life() > STALE_AFTER,
                        task: task.clone(),
                    })
                    .collect();
                index.insert(lane.key.as_str(), lanes.len());
                lanes.push(LaneView {
                    key: lane.key.clone(),
                    agent: lane.agent.clone(),
                    client: lane.client.clone(),
                    folder: lane.folder.clone(),
                    place: lane.folder.as_deref().map(place),
                    tasks,
                    shells: lane.shells.clone(),
                    updated_at: lane.updated_at,
                });
            }
        }
    }
    // Two lanes that would read the same (two Claude sessions in one folder,
    // neither joined to a reporting agent) get a number, in the order they
    // appeared, so they can be told apart.
    let mut seen: HashMap<(String, Option<String>), usize> = HashMap::new();
    for lane in &mut lanes {
        let count = seen.entry((lane.agent.to_lowercase(), lane.place.clone())).or_insert(0);
        *count += 1;
        if *count > 1 {
            lane.agent = format!("{} ({count})", lane.agent);
        }
    }
    for lane in &mut lanes {
        lane.tasks.sort_by_key(|t| (t.task.state != TaskState::Running, t.task.started_at));
        lane.shells.sort_by_key(|s| (s.ended_at.is_some(), -s.started_at));
    }
    let busy = |lane: &LaneView| {
        lane.tasks.iter().any(|t| t.task.state == TaskState::Running) || lane.shells.iter().any(|s| s.ended_at.is_none())
    };
    lanes.sort_by_key(|lane| (!busy(lane), -lane.updated_at));
    BoardView { now, lanes, page_url }
}

// ── the store ───────────────────────────────────────────────────────────

/// The board of this process, saved to `<suite>/modules/mcp/progress.json`
/// so bars survive a restart of the suite (the agents' work does).
pub struct Store {
    board: Mutex<Board>,
    dirty: Mutex<bool>,
    path: PathBuf,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| {
        let path = qs_core::paths::module_dir("mcp").join("progress.json");
        let board = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        // Writes are batched: a hook call answers before the disk is touched.
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            store().save_if_dirty();
        });
        Store { board: Mutex::new(board), dirty: Mutex::new(false), path }
    })
}

impl Store {
    fn mark_dirty(&self) {
        if let Ok(mut dirty) = self.dirty.lock() {
            *dirty = true;
        }
    }

    fn save_if_dirty(&self) {
        let Ok(mut dirty) = self.dirty.lock() else { return };
        if !*dirty {
            return;
        }
        let text = match self.board.lock() {
            Ok(board) => serde_json::to_string(&*board),
            Err(_) => return,
        };
        if let Ok(text) = text {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if qs_core::paths::write_atomic(&self.path, text.as_bytes()).is_ok() {
                *dirty = false;
            }
        }
    }

    pub fn report(&self, report: &Report, caller: &Caller) -> Result<Task, String> {
        let now = now_ms();
        let mut board = self.board.lock().map_err(|e| e.to_string())?;
        prune(&mut board, now);
        let task = apply_report(&mut board, report, caller, now)?;
        // The reply shows what the board will show: a newly followed file
        // counts already.
        refresh_follows(&mut board);
        let task = board
            .lanes
            .iter()
            .flat_map(|lane| lane.tasks.iter())
            .find(|t| t.id == task.id && t.started_at == task.started_at)
            .cloned()
            .unwrap_or(task);
        drop(board);
        self.mark_dirty();
        Ok(task)
    }

    pub fn hook(&self, client: &str, event: HookEvent, body: &Value) {
        let now = now_ms();
        let Ok(mut board) = self.board.lock() else { return };
        let changed = apply_hook(&mut board, client, event, body, now);
        drop(board);
        if changed {
            self.mark_dirty();
        }
    }

    pub fn view(&self, page_url: Option<String>) -> BoardView {
        let now = now_ms();
        let Ok(mut board) = self.board.lock() else {
            return BoardView { now, lanes: Vec::new(), page_url };
        };
        let changed = prune(&mut board, now) | bind_hooks(&mut board);
        refresh_follows(&mut board);
        let view = view(&board, now, page_url);
        drop(board);
        if changed {
            self.mark_dirty();
        }
        view
    }
}

pub fn page_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}{PAGE_PATH}")
}

// ── HTTP: the page, its state, script reports, the shell hooks ──────────

/// The one template, shared with the drawer's Status view.
const BOARD_JS: &str = include_str!("../../../../packages/ui/src/progress/board.js");
const BOARD_CSS: &str = include_str!("../../../../packages/ui/src/progress/board.css");

#[derive(Clone)]
pub(crate) struct ProgressHttp {
    pub port: u16,
}

/// The routes, without the MCP router's CORS layer — merge them after it:
/// no other site may read the board or write to it.
pub(crate) fn router(port: u16) -> Router {
    Router::new()
        .route(PAGE_PATH, get(page))
        .route("/progress/board.js", get(board_js))
        .route("/progress/board.css", get(board_css))
        .route("/progress/state", get(state))
        .route("/progress/report", post(report_http))
        .route(&format!("/{HOOK_SEGMENT}/{{client}}/{{event}}"), post(hook_http))
        .with_state(ProgressHttp { port })
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-store")], body).into_response()
}

async fn page(State(http): State<ProgressHttp>, headers: HeaderMap) -> Response {
    if !loopback_host(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    asset("text/html; charset=utf-8", PAGE_HTML)
}

async fn board_js(State(http): State<ProgressHttp>, headers: HeaderMap) -> Response {
    if !loopback_host(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    asset("text/javascript; charset=utf-8", BOARD_JS)
}

async fn board_css(State(http): State<ProgressHttp>, headers: HeaderMap) -> Response {
    if !loopback_host(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    asset("text/css; charset=utf-8", BOARD_CSS)
}

async fn state(State(http): State<ProgressHttp>, headers: HeaderMap) -> Response {
    if !loopback_host(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let view = tokio::task::spawn_blocking(move || store().view(Some(page_url(http.port))))
        .await
        .map_err(|e| e.to_string());
    match view {
        Ok(view) => ([(header::CACHE_CONTROL, "no-store")], Json(view)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// A script's report: the same fields as the tool, as a JSON body.
/// `agent_id` names its lane (default "script").
async fn report_http(State(http): State<ProgressHttp>, headers: HeaderMap, body: Bytes) -> Response {
    if !local_request(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let outcome = serde_json::from_slice::<Value>(&body)
        .map_err(|e| format!("not JSON: {e}"))
        .and_then(|args| Report::parse(&args))
        .and_then(|report| store().report(&report, &Caller::default()));
    match outcome {
        Ok(task) => Json(json!({ "ok": true, "bar": describe(&task, now_ms()) })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e }))).into_response(),
    }
}

/// A shell hook. Always an empty 200 for the agent: the call goes through
/// whatever the board makes of it.
async fn hook_http(
    State(http): State<ProgressHttp>,
    UrlPath((client, event)): UrlPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !local_request(&headers, http.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (Some(client), Some(event)) = (HookClient::from_slug(&client), HookEvent::from_slug(&event)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !HookFeature::Shells.events(client).contains(&event) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Ok(value) = serde_json::from_slice::<Value>(&body) {
        store().hook(client.slug(), event, &value);
    }
    StatusCode::OK.into_response()
}

/// The page host: the suite's dark tokens, a poll a second, the template.
const PAGE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Agent Status</title>
<link rel="stylesheet" href="/progress/board.css">
<style>
  :root {
    --qss-bg: #212121;
    --qss-bg-card: #2e2e2e;
    --qss-border: #333333;
    --qss-border-subtle: #2b2b2b;
    --qss-text: #e3e3e6;
    --qss-text-secondary: #9a9aa3;
    --qss-text-muted: #67676f;
    --qss-accent: #c9c9d1;
    --qss-success: #2ed573;
    --qss-error: #ff4757;
    --qss-warning: #ffa502;
    --qss-font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    --qss-font-mono: 'CaskaydiaCove Nerd Font', 'Cascadia Code', 'JetBrains Mono', ui-monospace, Consolas, monospace;
    color-scheme: dark;
  }
  html, body { margin: 0; background: var(--qss-bg); color: var(--qss-text); font-family: var(--qss-font-sans); }
  main { max-width: 980px; margin: 0 auto; padding: 20px 16px 40px; }
  .page-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
  .page-head h1 { margin: 0; font: 600 16px/1.2 var(--qss-font-sans); }
  .page-head span { color: var(--qss-text-muted); font-size: 12px; }
  .page-head span.is-off { color: var(--qss-warning); }
</style>
</head>
<body>
<main>
  <header class="page-head"><h1>Agent status</h1><span id="conn">Connecting…</span></header>
  <div id="board"></div>
</main>
<script type="module">
  import { renderBoard } from '/progress/board.js'
  const board = document.getElementById('board')
  const conn = document.getElementById('conn')
  async function tick() {
    try {
      const response = await fetch('/progress/state', { cache: 'no-store' })
      if (!response.ok) throw new Error(String(response.status))
      board.innerHTML = renderBoard(await response.json())
      conn.textContent = 'Live'
      conn.className = ''
    } catch {
      conn.textContent = 'QuantSuite is not answering'
      conn.className = 'is-off'
    }
  }
  tick()
  setInterval(tick, 1000)
</script>
</body>
</html>
"#;

// ── the Status view in the app ──────────────────────────────────────────

#[tauri::command(async)]
pub(crate) fn progress_board(state: tauri::State<'_, crate::AppState>) -> BoardView {
    store().view(Some(page_url(state.mcp_server_port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000_000;

    fn report(args: Value) -> Report {
        Report::parse(&args).unwrap()
    }

    fn claude(folder: &str) -> Caller {
        Caller { client: Some("claude-code".into()), client_name: Some("Claude Code".into()), folder: Some(folder.into()), session: None }
    }

    fn claude_session(folder: &str, session: &str) -> Caller {
        Caller { session: Some(session.into()), ..claude(folder) }
    }

    #[test]
    fn a_report_without_agent_id_stays_on_its_sessions_lane() {
        // The case from 2026-11-09: named once at the start, then the
        // closing report came without agent_id and opened a second lane.
        let mut board = Board::default();
        let desk = "C:/Users/root/Desktop";
        let caller = claude_session(desk, "mcp-1");
        apply_report(&mut board, &report(json!({ "agent_id": "claude-code-1", "title": "Move to D:", "steps": 3, "step": 2 })), &caller, T0).unwrap();
        let done = apply_report(&mut board, &report(json!({ "title": "Move to D:", "step": 3, "state": "done", "note": "22/22" })), &caller, T0 + 9).unwrap();
        assert_eq!(done.state, TaskState::Done);
        assert_eq!(board.lanes.len(), 1, "no second lane");
        assert_eq!(board.lanes[0].agent, "claude-code-1");
        assert_eq!(board.lanes[0].tasks.len(), 1, "no stuck copy of the bar");

        // Without a session: the one running bar of that id in the client's lanes.
        let mut board = Board::default();
        apply_report(&mut board, &report(json!({ "agent_id": "claude-code-1", "title": "Move to D:" })), &claude(desk), T0).unwrap();
        apply_report(&mut board, &report(json!({ "title": "Move to D:", "state": "done" })), &claude(desk), T0 + 1).unwrap();
        assert_eq!((board.lanes.len(), board.lanes[0].tasks[0].state), (1, TaskState::Done));

        // Named only later: the bar moves to the named lane.
        let mut board = Board::default();
        let caller = claude_session(desk, "mcp-2");
        apply_report(&mut board, &report(json!({ "title": "Build" })), &caller, T0).unwrap();
        apply_report(&mut board, &report(json!({ "agent_id": "codex-7", "title": "Build", "step": 2 })), &caller, T0 + 1).unwrap();
        prune(&mut board, T0 + 2);
        assert_eq!(board.lanes.len(), 1);
        assert_eq!((board.lanes[0].agent.as_str(), board.lanes[0].tasks[0].step), ("codex-7", Some(2)));
    }

    #[test]
    fn finishing_the_work_ends_its_background_shells() {
        let mut board = Board::default();
        let desk = "C:\\Users\\root\\Desktop";
        let caller = claude_session(desk, "mcp-1");
        apply_report(&mut board, &report(json!({ "agent_id": "claude-code-1", "title": "Copy", "folder": desk })), &caller, T0).unwrap();
        for (now, (event, body)) in [
            (T0 + 1, hook(HookEvent::PreToolUse, "s1", desk, "robocopy C: D:", "r", true)),
            (T0 + 2, hook(HookEvent::PostToolUse, "s1", desk, "robocopy C: D:", "r", true)),
        ] {
            apply_hook(&mut board, "claude-code", event, &body, now);
        }
        assert!(bind_hooks(&mut board));
        apply_report(&mut board, &report(json!({ "agent_id": "claude-code-1", "title": "Second", "folder": desk })), &caller, T0 + 3).unwrap();
        apply_report(&mut board, &report(json!({ "title": "Copy", "state": "done" })), &caller, T0 + 4).unwrap();
        let robocopy = |board: &Board| board.lanes.iter().flat_map(|l| &l.shells).find(|s| s.id == "r").unwrap().ended_at;
        assert_eq!(robocopy(&board), None, "another bar of the agent still runs");
        apply_report(&mut board, &report(json!({ "title": "Second", "state": "failed" })), &caller, T0 + 5).unwrap();
        assert_eq!(robocopy(&board), Some(T0 + 5));
    }

    #[test]
    fn lanes_that_would_read_the_same_get_a_number() {
        let mut board = Board::default();
        for (session, command) in [("a", "ls"), ("b", "pwd")] {
            let (event, body) = hook(HookEvent::PreToolUse, session, "C:\\Projects\\QuantSuite", command, session, false);
            apply_hook(&mut board, "claude-code", event, &body, T0);
        }
        let mut names: Vec<String> = view(&board, T0, None).lanes.into_iter().map(|l| l.agent).collect();
        names.sort();
        assert_eq!(names, ["Claude Code", "Claude Code (2)"]);
    }

    #[test]
    fn steps_and_the_step_inside_make_the_bar() {
        let mut board = Board::default();
        let caller = claude("C:/Projects/QuantSuite");
        let task = apply_report(
            &mut board,
            &report(json!({ "agent_id": "claude-code-2", "title": "Gauntlet", "step": 3, "steps": 5, "step_percent": 50 })),
            &caller,
            T0,
        )
        .unwrap();
        assert_eq!(task.overall(), Some(0.5), "2 done + half of step 3 = 2.5 / 5");
        assert_eq!(board.lanes[0].key, "agent:claude-code-2");
        assert_eq!(board.lanes[0].agent, "claude-code-2");

        // A new step starts empty; the same bar is found by its title's id.
        let task = apply_report(&mut board, &report(json!({ "agent_id": "claude-code-2", "title": "Gauntlet", "step": 4 })), &caller, T0 + 1).unwrap();
        assert_eq!(task.overall(), Some(0.6));
        assert_eq!(board.lanes[0].tasks.len(), 1);

        // Without a title the one running bar of the agent is meant.
        let task = apply_report(&mut board, &report(json!({ "agent_id": "claude-code-2", "state": "done" })), &caller, T0 + 2).unwrap();
        assert_eq!(task.state, TaskState::Done);
        assert_eq!(task.overall(), Some(1.0));
        assert_eq!(task.step, Some(5));
    }

    #[test]
    fn without_steps_the_percent_or_the_followed_file_is_the_bar() {
        let mut task = Task::new("a".into(), "A".into(), T0);
        assert_eq!(task.overall(), None, "nothing known: an indeterminate bar");
        task.percent = Some(40.0);
        assert_eq!(task.overall(), Some(0.4));
        task.follow_fraction = Some(0.7);
        assert_eq!(task.overall(), Some(0.7), "the work itself wins over an older report");
    }

    #[test]
    fn estimate_holds_while_it_can_then_the_rate_takes_over() {
        let mut task = Task::new("a".into(), "A".into(), T0);
        task.estimate_secs = Some(3 * 3600);
        assert_eq!(task.total_secs(T0 + HOUR), Some(3 * 3600), "1h of ~3h");
        task.percent = Some(25.0);
        assert_eq!(task.total_secs(T0 + 4 * HOUR), Some(16 * 3600), "overdue: 4h for 25 % → ~16h");
        let mut fresh = Task::new("b".into(), "B".into(), T0);
        fresh.percent = Some(1.0);
        assert_eq!(fresh.total_secs(T0 + MINUTE), None, "1 % is too little to extrapolate");
        fresh.percent = Some(10.0);
        assert_eq!(fresh.total_secs(T0 + 10 * MINUTE), Some(6000), "10 % in 10 min → ~100 min");
        fresh.state = TaskState::Done;
        fresh.finished_at = Some(T0 + 23 * MINUTE);
        assert_eq!(fresh.total_secs(T0 + HOUR), Some(23 * 60), "done: what it took");
    }

    #[test]
    fn durations_read_like_people_write_them() {
        assert_eq!(parse_duration("3h"), Some(10_800));
        assert_eq!(parse_duration("~1h30m"), Some(5_400));
        assert_eq!(parse_duration("1 h 30 min"), Some(5_400));
        assert_eq!(parse_duration("2.5h"), Some(9_000));
        assert_eq!(parse_duration("45"), Some(2_700), "a bare number is minutes");
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("3x"), None);
        assert_eq!(short_duration(7200), "2h");
        assert_eq!(short_duration(5400), "1h 30m");
        assert_eq!(short_duration(600), "10m");
        assert!(Report::parse(&json!({ "estimate": "later" })).is_err());
        assert_eq!(report(json!({ "estimate": 30 })).estimate_secs, Some(1800));
    }

    #[test]
    fn reports_validate_their_numbers_and_state() {
        assert!(Report::parse(&json!({ "percent": "half" })).is_err());
        assert!(Report::parse(&json!({ "state": "maybe" })).is_err());
        assert!(Report::parse(&json!({ "steps": -1 })).is_err());
        let parsed = report(json!({ "percent": "45%", "steps": "7", "state": "Finished", "follow": "" }));
        assert_eq!(parsed.percent, Some(45.0));
        assert_eq!(parsed.steps, Some(7));
        assert_eq!(parsed.state, Some(TaskState::Done));
        assert_eq!(parsed.follow, Some(None));
        let mut board = Board::default();
        let err = apply_report(&mut board, &report(json!({ "step": 2 })), &Caller::default(), T0).unwrap_err();
        assert!(err.contains("title"), "{err}");
    }

    #[test]
    fn a_finished_bar_reported_again_starts_a_new_run() {
        let mut board = Board::default();
        let caller = Caller::default();
        apply_report(&mut board, &report(json!({ "title": "Tests", "state": "failed" })), &caller, T0).unwrap();
        let task = apply_report(&mut board, &report(json!({ "title": "Tests", "percent": 10 })), &caller, T0 + MINUTE).unwrap();
        assert_eq!(task.state, TaskState::Running);
        assert_eq!(task.started_at, T0 + MINUTE);
        assert_eq!(task.finished_at, None);
        assert_eq!(board.lanes[0].agent, "script");
    }

    fn hook(event: HookEvent, session: &str, cwd: &str, command: &str, id: &str, background: bool) -> (HookEvent, Value) {
        (
            event,
            json!({
                "session_id": session, "cwd": cwd, "hook_event_name": "x", "tool_name": "Bash", "tool_use_id": id,
                "tool_input": { "command": command, "run_in_background": background }
            }),
        )
    }

    #[test]
    fn shells_start_end_and_background_ones_keep_running() {
        let mut board = Board::default();
        let cwd = "C:\\Projects\\QuantSuite";
        for (now, (event, body)) in [
            (T0, hook(HookEvent::PreToolUse, "s1", cwd, "cargo test -p qs_algo\nsecond line", "t1", false)),
            (T0 + 1, hook(HookEvent::PreToolUse, "s1", cwd, "python gauntlet.py", "t2", true)),
            (T0 + 2, hook(HookEvent::PostToolUse, "s1", cwd, "python gauntlet.py", "t2", true)),
            (T0 + 3, hook(HookEvent::PostToolUseFailure, "s1", cwd, "cargo test -p qs_algo", "t1", false)),
        ] {
            apply_hook(&mut board, "claude-code", event, &body, now);
        }
        let lane = &board.lanes[0];
        assert_eq!(lane.key, "hook:claude-code:s1");
        assert_eq!(lane.agent, "Claude Code");
        let test = lane.shells.iter().find(|s| s.id == "t1").unwrap();
        assert_eq!(test.command, "cargo test -p qs_algo …");
        assert_eq!((test.ended_at, test.failed), (Some(T0 + 3), true));
        let gauntlet = lane.shells.iter().find(|s| s.id == "t2").unwrap();
        assert_eq!(gauntlet.ended_at, None, "a background launch is no end");

        // Read and Edit calls are no shells.
        let mut read = hook(HookEvent::PreToolUse, "s1", cwd, "x", "t3", false).1;
        read["tool_name"] = json!("Read");
        assert!(!apply_hook(&mut board, "claude-code", HookEvent::PreToolUse, &read, T0 + 4));

        // The session's end takes its background shells along.
        assert!(apply_hook(&mut board, "claude-code", HookEvent::SessionEnd, &json!({ "session_id": "s1" }), T0 + 5));
        assert!(board.lanes.is_empty());
    }

    #[test]
    fn codex_argv_commands_read_as_their_script() {
        assert_eq!(shell_command(&json!({ "command": ["bash", "-lc", "npm test"] })).as_deref(), Some("npm test"));
        assert_eq!(shell_command(&json!({ "command": ["git", "status", "--short"] })).as_deref(), Some("git status --short"));
        assert_eq!(shell_command(&json!({ "command": 3 })), None);
    }

    #[test]
    fn a_hook_session_joins_the_one_agent_that_claims_its_folder() {
        let mut board = Board::default();
        let tree = "C:/Projects/QuantSuite/.qs-worktrees/84ac3068";
        apply_report(
            &mut board,
            &report(json!({ "agent_id": "claude-code-2", "title": "Status view", "folder": tree })),
            &claude("C:/Projects/QuantSuite"),
            T0,
        )
        .unwrap();
        apply_report(&mut board, &report(json!({ "agent_id": "codex-1", "title": "Card 791a" })), &Caller {
            client: Some("codex-cli".into()),
            client_name: Some("Codex CLI".into()),
            folder: Some("C:/Projects/QuantSuite".into()),
            session: None,
        }, T0)
        .unwrap();
        let (event, mine) = hook(HookEvent::PreToolUse, "mine", "C:\\Projects\\QuantSuite\\.qs-worktrees\\84ac3068", "cargo check", "a", false);
        apply_hook(&mut board, "claude-code", event, &mine, T0 + 1);
        // Another Claude session in the main checkout: no lane of its client
        // claims that folder (claude-code-2's is the worktree inside it).
        let (event, other) = hook(HookEvent::PreToolUse, "other", "C:\\Projects\\QuantSuite", "python run.py", "b", false);
        apply_hook(&mut board, "claude-code", event, &other, T0 + 1);
        assert!(bind_hooks(&mut board));
        assert_eq!(board.bindings.get("hook:claude-code:mine").map(String::as_str), Some("agent:claude-code-2"));
        assert_eq!(board.bindings.get("hook:claude-code:other"), None);

        let view = view(&board, T0 + 2, None);
        let mine = view.lanes.iter().find(|l| l.agent == "claude-code-2").unwrap();
        assert_eq!(mine.shells.len(), 1);
        assert_eq!(mine.place.as_deref(), Some("QuantSuite · worktree 84ac3068"));
        let other = view.lanes.iter().find(|l| l.key == "hook:claude-code:other").unwrap();
        assert_eq!((other.agent.as_str(), other.place.as_deref()), ("Claude Code", Some("QuantSuite")));
        assert_eq!(view.lanes.len(), 3);
    }

    #[test]
    fn two_equal_claims_bind_nothing() {
        let mut board = Board::default();
        for agent in ["claude-code-1", "claude-code-2"] {
            apply_report(&mut board, &report(json!({ "agent_id": agent, "title": "x" })), &claude("C:/Projects/QuantSuite"), T0).unwrap();
        }
        let (event, body) = hook(HookEvent::PreToolUse, "s", "C:/Projects/QuantSuite", "ls", "a", false);
        apply_hook(&mut board, "claude-code", event, &body, T0);
        assert!(!bind_hooks(&mut board));
    }

    #[test]
    fn silence_makes_a_bar_stale_unless_its_shell_runs() {
        let mut board = Board::default();
        let tree = "C:/w";
        apply_report(&mut board, &report(json!({ "agent_id": "a", "title": "Long", "folder": tree })), &claude(tree), T0).unwrap();
        let later = T0 + STALE_AFTER + MINUTE;
        assert!(view(&board, later, None).lanes[0].tasks[0].stale);
        let (event, body) = hook(HookEvent::PreToolUse, "s", tree, "sleep 999", "a", false);
        apply_hook(&mut board, "claude-code", event, &body, later - SECOND);
        bind_hooks(&mut board);
        assert!(!view(&board, later, None).lanes[0].tasks[0].stale);
    }

    #[test]
    fn prune_drops_what_is_over() {
        let mut board = Board::default();
        let caller = Caller::default();
        apply_report(&mut board, &report(json!({ "title": "Done", "state": "done" })), &caller, T0).unwrap();
        apply_report(&mut board, &report(json!({ "title": "Failed", "state": "failed" })), &caller, T0).unwrap();
        apply_report(&mut board, &report(json!({ "title": "Running" })), &caller, T0).unwrap();
        assert!(prune(&mut board, T0 + KEEP_DONE + 1));
        let titles: Vec<&str> = board.lanes[0].tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Failed", "Running"]);
        assert!(prune(&mut board, T0 + ABANDONED + 1));
        assert!(board.lanes.is_empty());
    }

    #[test]
    fn followed_files_give_fraction_and_detail() {
        let smithery = "{\"event\":\"start\",\"at\":1}\n{\"event\":\"progress\",\"at\":2,\"message\":\"Gauntlet 3/8: Monte-Carlo\",\"fraction\":0.375}\n";
        assert_eq!(scan_progress(smithery), (Some(0.375), Some("Gauntlet 3/8: Monte-Carlo".into())));
        let log = "Compiling a\nPROGRESS 12/48 window 12\n   Running tests\n\u{1b}[32mtest a ... ok\u{1b}[0m\n";
        assert_eq!(scan_progress(log), (Some(0.25), Some("test a ... ok".into())));
        assert_eq!(scan_progress("PROGRESS 45% half\r"), (Some(0.45), Some("half".into())));
        assert_eq!(scan_progress("just text\n"), (None, Some("just text".into())));
        assert_eq!(scan_progress("{\"event\":\"done\"}\n"), (None, None));

        let dir = std::env::temp_dir().join(format!("qs-progress-follow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("events.jsonl"), smithery).unwrap();
        let read = read_follow(&dir).unwrap();
        assert_eq!(read.fraction, Some(0.375));
        assert!(read.modified > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn places_read_like_the_workspace() {
        assert_eq!(place("C:\\Projects\\QuantSuite\\.qs-worktrees\\84ac3068"), "QuantSuite · worktree 84ac3068");
        assert_eq!(place("C:/Projects/QuantGames/"), "QuantGames");
    }

    #[test]
    fn the_board_round_trips_through_its_file() {
        let mut board = Board::default();
        apply_report(&mut board, &report(json!({ "title": "x", "steps": 2, "step": 1, "estimate": "1h" })), &Caller::default(), T0).unwrap();
        let text = serde_json::to_string(&board).unwrap();
        let back: Board = serde_json::from_str(&text).unwrap();
        assert_eq!(back.lanes[0].tasks[0].estimate_secs, Some(3600));
        assert_eq!(serde_json::from_str::<Board>("{}").unwrap().lanes.len(), 0);
    }
}
