//! Codex over `codex app-server` — newline-delimited JSON-RPC without the
//! `jsonrpc` field (docs/PLAN-QUANTPILOT.md §5.2).
//!
//! Verified against codex-cli 0.165.0 (`codex app-server
//! generate-json-schema --experimental`, live probes, fixtures in
//! `tests/fixtures/codex-*.jsonl`):
//! - `initialize` → `initialized`, then `thread/start` (or `thread/resume`
//!   by the saved thread id) answers with the thread; `turn/start` carries
//!   the input, model, effort, approval policy, reviewer and sandbox policy
//!   and answers with the turn id; `turn/steer` joins a running turn
//!   (`expectedTurnId`), `turn/interrupt` stops it.
//! - Items arrive as `item/started` / `item/completed` with deltas in
//!   between (`item/agentMessage/delta`, `item/reasoning/*Delta`,
//!   `item/commandExecution/outputDelta`). Agent messages carry a phase
//!   (`commentary`, `final_answer`). Codex echoes the user message as an
//!   item; the runtime has its own.
//! - Approvals are server requests: `item/commandExecution/requestApproval`
//!   (with `availableDecisions` — `decline` and `acceptForSession` are not
//!   always offered), `item/fileChange/requestApproval`,
//!   `item/permissions/requestApproval`, and MCP tool calls as
//!   `mcpServer/elicitation/request` (`_meta.codex_approval_kind`).
//!   Questions are `item/tool/requestUserInput`; async questions are agent
//!   messages with `delivery: "async"`, answered by a new user message.
//! - `thread/tokenUsage/updated`: `last.totalTokens` is the context in use,
//!   `total` is cumulative for the thread; `account/rateLimits/updated`
//!   carries the 5-hour (`primary`) and weekly (`secondary`) windows.
//! - Subagents run in child threads (`collabAgentToolCall.receiverThreadIds`);
//!   their items nest under the call, their turns never end ours.

use crate::context::SkillInfo;
use crate::driver::{
    Answer, Catalog, Driver, Emit, ImageInput, ModelInfo, Request, RequestKind, ToolInfo, TurnInput, TurnStatus,
};
use crate::events::RateWindow;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Command output kept per call while it streams (the item keeps less).
const OUTPUT_KEEP: usize = 64 * 1024;

/// What a thread is started or resumed with.
#[derive(Debug, Clone, Default)]
pub struct CodexOptions {
    pub cwd: String,
    /// The saved Codex thread id, when the conversation continues.
    pub resume: Option<String>,
    pub model: String,
    pub effort: String,
    /// `ask` | `edits` | `auto` | `plan` | `full`
    pub mode: String,
    /// The suite's context for the agent (Codex `developerInstructions`).
    pub instructions: String,
    /// QuantMCP for this run only, as a config override — the user's own
    /// `config.toml` is never edited.
    pub mcp_url: Option<String>,
    /// Extra writable roots in the workspace-write sandbox (the general vault).
    pub writable_roots: Vec<String>,
    /// Rewind: drop this turn and every later one from the thread before
    /// the next turn (`thread/revert`, card 11).
    pub revert_before: Option<String>,
    /// Fork: a new Codex thread from this one, up to this turn (`thread/fork`, card 15).
    pub fork: Option<(String, Option<String>)>,
}

/// The `codex` arguments after the executable.
pub fn launch_args() -> Vec<String> {
    vec!["app-server".into()]
}

/// Approval policy, reviewer and sandbox policy for a QuantPilot mode
/// (§6; the same mapping as T3 Code's runtime modes).
pub fn policy(mode: &str, writable_roots: &[String]) -> (Value, &'static str, Value) {
    let workspace = || {
        let mut s = json!({ "type": "workspaceWrite" });
        if !writable_roots.is_empty() {
            s["writableRoots"] = json!(writable_roots);
        }
        s
    };
    match mode {
        "ask" => (json!("untrusted"), "user", json!({ "type": "readOnly" })),
        "edits" => (json!("on-request"), "user", workspace()),
        "auto" => (json!("on-request"), "auto_review", workspace()),
        "plan" => (json!("on-request"), "user", json!({ "type": "readOnly" })),
        _ => (json!("never"), "user", json!({ "type": "dangerFullAccess" })),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Call {
    Initialize,
    ThreadStart,
    ThreadResume,
    ThreadFork,
    Compact,
    TurnStart,
    TurnSteer,
    Interrupt,
    Models,
    Skills,
    RateLimits,
    Settings,
    Revert,
}

#[derive(Debug, Clone)]
enum Pending {
    Command { rpc: Value, decisions: Vec<Value> },
    File { rpc: Value },
    Permissions { rpc: Value, requested: Value },
    /// An MCP tool-call approval (empty form) or a form with fields.
    Elicitation { rpc: Value, fields: Vec<FormField> },
    UserInput { rpc: Value, questions: Vec<(String, String)> },
    /// v1 `execCommandApproval` / `applyPatchApproval`.
    Legacy { rpc: Value },
}

#[derive(Debug, Clone)]
struct FormField {
    key: String,
    title: String,
    /// `string` | `boolean` | `number` | `integer`
    kind: String,
}

/// What was shown for a Codex item.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    Message,
    Reasoning,
    Tool,
}

pub struct CodexDriver {
    opts: CodexOptions,
    next_id: u64,
    calls: HashMap<u64, Call>,
    thread: Option<String>,
    /// Input that arrived before the thread was ready.
    queued: Vec<TurnInput>,
    /// The running Codex turn, once `turn/start` answered or `turn/started` came.
    turn: Option<String>,
    /// `turn/start` is on its way; a message now waits for its answer.
    starting: bool,
    interrupt_pending: bool,
    /// Child thread → the agent tool call that started it.
    children: HashMap<String, String>,
    requests: HashMap<String, Pending>,
    shown: HashMap<String, Shown>,
    output: HashMap<String, String>,
    /// File changes of an item, for its approval and its result.
    changes: HashMap<String, Value>,
    /// The thread's cumulative input/output tokens last seen.
    totals: Option<(i64, i64)>,
    retry_noted: bool,
    /// A declined command was answered with `cancel` (no `decline` offered):
    /// Codex ends the turn, and the timeline says why.
    cancelled: bool,
    version: Option<String>,
    models: Option<Vec<ModelInfo>>,
    skills: Option<Vec<SkillInfo>>,
    catalog_sent: bool,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn i(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// `C:\…\powershell.exe -Command 'echo hi'` → `echo hi`: the command the
/// agent meant, from the parsed actions when Codex gives them.
fn display_command(item: &Value) -> String {
    let actions = item.get("commandActions").and_then(Value::as_array);
    let parsed: Vec<&str> = actions.map(|a| a.iter().filter_map(|x| s(x, "command")).collect()).unwrap_or_default();
    if !parsed.is_empty() {
        return parsed.join(" && ");
    }
    s(item, "command").unwrap_or_default().to_string()
}

/// The category a command's parsed actions suggest: reading, listing and
/// searching are shown as such, like Claude's Read/Grep.
fn command_category(item: &Value) -> &'static str {
    let kinds: Vec<&str> = item
        .get("commandActions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| s(x, "type")).collect())
        .unwrap_or_default();
    if kinds.is_empty() || kinds.contains(&"unknown") {
        "command"
    } else if kinds.iter().all(|k| *k == "read") {
        "read"
    } else {
        "search"
    }
}

fn rate_window(kind: &str, w: &Value) -> Option<RateWindow> {
    if w.is_null() {
        return None;
    }
    let used = w.get("usedPercent").and_then(Value::as_f64);
    let mins = i(w, "windowDurationMins");
    let kind = match mins {
        Some(300) => "five_hour".to_string(),
        Some(10080) => "seven_day".to_string(),
        Some(m) => format!("{m}m"),
        None => kind.to_string(),
    };
    Some(RateWindow {
        kind,
        used_pct: used,
        resets_at: i(w, "resetsAt").map(|t| t * 1000),
        status: used.filter(|u| *u >= 100.0).map(|_| "rejected".to_string()),
    })
}

fn rate_windows(limits: &Value) -> Vec<RateWindow> {
    ["primary", "secondary"]
        .iter()
        .filter_map(|k| limits.get(*k).and_then(|w| rate_window(k, w)))
        .collect()
}

impl CodexDriver {
    pub fn new(opts: CodexOptions) -> Self {
        Self {
            opts,
            next_id: 0,
            calls: HashMap::new(),
            thread: None,
            queued: Vec::new(),
            turn: None,
            starting: false,
            interrupt_pending: false,
            children: HashMap::new(),
            requests: HashMap::new(),
            shown: HashMap::new(),
            output: HashMap::new(),
            changes: HashMap::new(),
            totals: None,
            retry_noted: false,
            cancelled: false,
            version: None,
            models: None,
            skills: None,
            catalog_sent: false,
        }
    }

    /// A driver that only asks for the catalog (models and skills).
    pub fn discovery(cwd: &str) -> Self {
        Self::new(CodexOptions { cwd: cwd.to_string(), mode: "full".into(), ..Default::default() })
    }

    fn request(&mut self, call: Call, method: &str, params: Value) -> String {
        self.next_id += 1;
        self.calls.insert(self.next_id, call);
        json!({ "id": self.next_id, "method": method, "params": params }).to_string()
    }

    fn reply(rpc: &Value, result: Value) -> String {
        json!({ "id": rpc, "result": result }).to_string()
    }

    fn input_items(input: &TurnInput) -> Value {
        let mut items: Vec<Value> = input
            .images
            .iter()
            .map(|ImageInput { media_type, base64, path }| match path {
                Some(p) => json!({ "type": "localImage", "path": p }),
                None => json!({ "type": "image", "url": format!("data:{media_type};base64,{base64}") }),
            })
            .collect();
        items.push(json!({ "type": "text", "text": input.text, "text_elements": [] }));
        items.extend(input.skills.iter().map(|s| json!({ "type": "skill", "name": s.name, "path": s.path })));
        Value::Array(items)
    }

    fn thread_params(&self) -> Value {
        let mut p = json!({ "cwd": self.opts.cwd });
        if !self.opts.instructions.trim().is_empty() {
            p["developerInstructions"] = json!(self.opts.instructions);
        }
        if let Some(url) = &self.opts.mcp_url {
            // Named like the suite's own Connect snippet, so a QuantMCP the
            // user configured is updated for this run instead of doubled.
            p["config"] = json!({ "mcp_servers.QuantMCP": { "url": url } });
        }
        if !self.opts.model.is_empty() {
            p["model"] = json!(self.opts.model);
        }
        p
    }

    fn open_thread(&mut self) -> String {
        let mut params = self.thread_params();
        if let Some((source, last)) = self.opts.fork.clone() {
            params["threadId"] = json!(source);
            if let Some(turn) = last {
                params["lastTurnId"] = json!(turn);
            }
            return self.request(Call::ThreadFork, "thread/fork", params);
        }
        match self.opts.resume.clone() {
            Some(id) => {
                params["threadId"] = json!(id);
                params["excludeTurns"] = json!(true);
                self.request(Call::ThreadResume, "thread/resume", params)
            }
            None => self.request(Call::ThreadStart, "thread/start", params),
        }
    }

    fn turn_start(&mut self, input: &TurnInput) -> String {
        // `/compact` is Codex's own compaction, a turn of its own (card 15).
        if input.text.trim() == "/compact" && input.images.is_empty() {
            self.starting = true;
            self.retry_noted = false;
            self.cancelled = false;
            return self.request(Call::Compact, "thread/compact/start", json!({ "threadId": self.thread }));
        }
        let (approval, reviewer, sandbox) = policy(&self.opts.mode, &self.opts.writable_roots);
        let mut params = json!({
            "threadId": self.thread,
            "input": Self::input_items(input),
            "approvalPolicy": approval,
            "approvalsReviewer": reviewer,
            "sandboxPolicy": sandbox,
        });
        if !self.opts.model.is_empty() {
            params["model"] = json!(self.opts.model);
        }
        if !self.opts.effort.is_empty() {
            params["effort"] = json!(self.opts.effort);
        }
        self.starting = true;
        self.retry_noted = false;
        self.cancelled = false;
        self.request(Call::TurnStart, "turn/start", params)
    }

    fn catalog_ready(&mut self) -> Vec<Emit> {
        let (Some(models), Some(skills)) = (self.models.clone(), self.skills.clone()) else {
            return Vec::new();
        };
        self.catalog_sent = true;
        vec![Emit::Catalog(Catalog { provider: "codex".into(), version: self.version.clone(), models, commands: Vec::new(), skills })]
    }

    /// Whether a notification belongs to this thread (or one of its subagents).
    fn ours(&self, params: &Value) -> Option<Option<String>> {
        let thread = s(params, "threadId")?;
        if Some(thread) == self.thread.as_deref() {
            Some(None)
        } else {
            self.children.get(thread).map(|parent| Some(parent.clone()))
        }
    }

    fn on_response(&mut self, id: u64, msg: &Value) -> Vec<Emit> {
        let Some(call) = self.calls.remove(&id) else {
            return Vec::new();
        };
        let result = msg.get("result").cloned().unwrap_or(Value::Null);
        let error = msg.get("error").map(|e| s(e, "message").unwrap_or("Codex refused the request").to_string());
        let mut out = Vec::new();
        match call {
            Call::Initialize => {
                self.version = s(&result, "userAgent")
                    .and_then(|ua| ua.split('/').nth(1))
                    .and_then(|v| v.split_whitespace().next())
                    .map(str::to_string);
                out.push(Emit::Write(json!({ "method": "initialized" }).to_string()));
                let cwd = self.opts.cwd.clone();
                out.push(Emit::Write(self.request(Call::Skills, "skills/list", json!({ "cwds": [cwd] }))));
                out.push(Emit::Write(self.request(Call::Models, "model/list", json!({}))));
                out.push(Emit::Write(self.request(Call::RateLimits, "account/rateLimits/read", json!({}))));
                // A discovery run (nothing to send) opens no thread.
                if !self.queued.is_empty() || self.opts.resume.is_some() {
                    out.push(Emit::Write(self.open_thread()));
                }
            }
            Call::Models => {
                let list: Vec<Value> = result
                    .get("data")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().filter(|m| !m.get("hidden").and_then(Value::as_bool).unwrap_or(false)).cloned().collect())
                    .unwrap_or_default();
                let mut models: Vec<ModelInfo> = list
                    .iter()
                    .map(|m| ModelInfo {
                                id: s(m, "id").unwrap_or_default().to_string(),
                                display_name: s(m, "displayName").unwrap_or_default().to_string(),
                                description: s(m, "description").unwrap_or_default().to_string(),
                                resolved_model: s(m, "model").map(str::to_string),
                        efforts: m
                            .get("supportedReasoningEfforts")
                            .and_then(Value::as_array)
                            .map(|e| e.iter().filter_map(|x| s(x, "reasoningEffort").map(str::to_string)).collect())
                            .unwrap_or_default(),
                    })
                    .collect();
                // "Default" = no override: Codex picks (like Claude Code's `default` entry).
                if let Some(pos) = list.iter().position(|m| m.get("isDefault").and_then(Value::as_bool).unwrap_or(false)) {
                    let d = models[pos].clone();
                    models.insert(
                        0,
                        ModelInfo {
                            id: "default".into(),
                            display_name: "Default".into(),
                            description: "Codex's own choice".into(),
                            resolved_model: d.resolved_model.clone(),
                            efforts: d.efforts.clone(),
                        },
                    );
                }
                self.models = Some(models);
                out.extend(self.catalog_ready());
            }
            Call::Skills => {
                let skills = result
                    .get("data")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .flat_map(|e| e.get("skills").and_then(Value::as_array).cloned().unwrap_or_default())
                            .filter(|k| k.get("enabled").and_then(Value::as_bool).unwrap_or(true))
                            .map(|k| SkillInfo {
                                name: s(&k, "name").unwrap_or_default().to_string(),
                                description: s(&k, "shortDescription").or_else(|| s(&k, "description")).unwrap_or_default().to_string(),
                                source: s(&k, "scope").unwrap_or("codex").to_string(),
                                path: s(&k, "path").unwrap_or_default().to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.skills = Some(skills);
                out.extend(self.catalog_ready());
            }
            Call::RateLimits => {
                let windows = result.get("rateLimits").map(rate_windows).unwrap_or_default();
                if !windows.is_empty() {
                    out.push(Emit::RateLimit { windows });
                }
            }
            Call::ThreadStart | Call::ThreadResume | Call::ThreadFork => {
                if let Some(e) = error {
                    if call == Call::ThreadFork {
                        // The source thread is gone or too old to fork: a fresh one, said so.
                        out.push(Emit::Notice { text: format!("Codex could not fork the earlier conversation ({e}); this message starts a new one.") });
                        self.opts.fork = None;
                        out.push(Emit::Write(self.open_thread()));
                        return out;
                    }
                    if call == Call::ThreadResume {
                        // The saved conversation is gone (another CODEX_HOME, deleted): start fresh, say so.
                        out.push(Emit::Notice { text: format!("Codex could not continue the earlier conversation ({e}); this message starts a new one.") });
                        self.opts.resume = None;
                        out.push(Emit::Write(self.open_thread()));
                        return out;
                    }
                    self.queued.clear();
                    out.push(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(format!("Codex could not start a thread: {e}")), duration_ms: None, cost_usd: None });
                    return out;
                }
                let thread = result.get("thread").cloned().unwrap_or(Value::Null);
                let Some(id) = s(&thread, "id").map(str::to_string) else {
                    return out;
                };
                self.thread = Some(id.clone());
                out.push(Emit::Bound { session_id: id.clone(), model: s(&result, "model").map(str::to_string) });
                if let Some(before) = self.opts.revert_before.take() {
                    // The turn starts once the history is cut back.
                    out.push(Emit::Write(self.request(Call::Revert, "thread/revert", json!({ "threadId": id, "beforeTurnId": before }))));
                    return out;
                }
                if let Some(input) = (!self.queued.is_empty()).then(|| self.queued.remove(0)) {
                    out.push(Emit::Write(self.turn_start(&input)));
                }
            }
            Call::TurnStart => {
                self.starting = false;
                if let Some(e) = error {
                    out.push(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(e), duration_ms: None, cost_usd: None });
                    return out;
                }
                if let Some(t) = result.get("turn").and_then(|t| s(t, "id")) {
                    self.turn = Some(t.to_string());
                }
                // Steers that waited for the turn id go in now, then a deferred stop.
                let waiting: Vec<TurnInput> = self.queued.drain(..).collect();
                for input in waiting {
                    out.push(Emit::Write(self.steer_line(&input)));
                }
                if self.interrupt_pending {
                    self.interrupt_pending = false;
                    out.extend(self.interrupt().into_iter().map(Emit::Write));
                }
            }
            Call::TurnSteer => {
                if let Some(e) = error {
                    out.push(Emit::Notice { text: format!("Codex did not take the message into the running turn: {e}") });
                }
            }
            Call::Compact => {
                self.starting = false;
                if let Some(e) = error {
                    out.push(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(format!("Codex could not compact the conversation: {e}")), duration_ms: None, cost_usd: None });
                }
            }
            Call::Revert => {
                if let Some(e) = error {
                    out.push(Emit::Notice { text: format!("Codex could not rewind the conversation: {e}") });
                }
                if let Some(input) = (!self.queued.is_empty()).then(|| self.queued.remove(0)) {
                    out.push(Emit::Write(self.turn_start(&input)));
                }
            }
            Call::Interrupt | Call::Settings => {
                if let Some(e) = error {
                    out.push(Emit::Notice { text: e });
                }
            }
        }
        out
    }

    fn steer_line(&mut self, input: &TurnInput) -> String {
        let params = json!({ "threadId": self.thread, "expectedTurnId": self.turn, "input": Self::input_items(input) });
        self.request(Call::TurnSteer, "turn/steer", params)
    }

    fn on_item(&mut self, params: &Value, done: bool, parent: Option<String>) -> Vec<Emit> {
        let item = params.get("item").cloned().unwrap_or(Value::Null);
        let Some(id) = s(&item, "id").map(str::to_string) else {
            return Vec::new();
        };
        let kind = s(&item, "type").unwrap_or_default();
        let mut out = Vec::new();
        match kind {
            "userMessage" | "hookPrompt" => {}
            "agentMessage" => {
                let asked = item.get("questions").and_then(Value::as_array).filter(|q| !q.is_empty());
                if let Some(asked) = asked.filter(|_| done && s(&item, "delivery") == Some("async")) {
                    // A question for later: answered by the next user message.
                    let questions: Vec<Value> = asked
                        .iter()
                        .map(|q| {
                            let options: Vec<Value> = q
                                .get("options")
                                .and_then(Value::as_array)
                                .map(|o| o.iter().filter_map(Value::as_str).map(|l| json!({ "label": l, "description": "" })).collect())
                                .unwrap_or_default();
                            json!({ "question": s(q, "title").unwrap_or_default(), "header": "Question", "options": options, "multiSelect": false })
                        })
                        .collect();
                    if self.shown.remove(&id).is_some() {
                        out.push(Emit::MessageDone { key: id.clone(), text: s(&item, "text").unwrap_or_default().to_string() });
                    }
                    out.push(Emit::Request(Request {
                        request_id: format!("async:{id}"),
                        kind: RequestKind::Question,
                        tool_name: "question".into(),
                        call_id: None,
                        input: json!({ "questions": questions }),
                        parent,
                        allow_session: false,
                        by_message: true,
                    }));
                    return out;
                }
                if !self.shown.contains_key(&id) {
                    self.shown.insert(id.clone(), Shown::Message);
                    out.push(Emit::MessageStart { key: id.clone(), phase: s(&item, "phase").map(str::to_string), parent: parent.clone() });
                }
                if done {
                    self.shown.remove(&id);
                    out.push(Emit::MessageDone { key: id, text: s(&item, "text").unwrap_or_default().to_string() });
                }
            }
            "plan" => {
                // Plan mode's proposal (card 16 adds Implement): shown as a message for now.
                if done {
                    out.push(Emit::MessageStart { key: id.clone(), phase: Some("plan".into()), parent });
                    out.push(Emit::MessageDone { key: id, text: s(&item, "text").unwrap_or_default().to_string() });
                }
            }
            "reasoning" => {
                if !done {
                    return out;
                }
                if self.shown.remove(&id).is_some() {
                    out.push(Emit::ReasoningDone { key: id });
                } else {
                    // A summary that came only with the finished item.
                    let summary: Vec<&str> = item.get("summary").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                    let text = summary.join("\n\n");
                    if !text.trim().is_empty() {
                        out.push(Emit::ReasoningStart { key: id.clone(), parent });
                        out.push(Emit::ReasoningDelta { key: id.clone(), text, estimated_tokens: None });
                        out.push(Emit::ReasoningDone { key: id });
                    }
                }
            }
            "contextCompaction" => {
                if done {
                    out.push(Emit::Notice { text: "Codex compacted the conversation to free context.".into() });
                }
            }
            "enteredReviewMode" | "exitedReviewMode" => {}
            _ => out.extend(self.on_tool_item(&id, kind, &item, done, parent)),
        }
        out
    }

    /// Commands, file changes, MCP and dynamic tool calls, web search,
    /// image views and subagent calls: tool rows.
    fn on_tool_item(&mut self, id: &str, kind: &str, item: &Value, done: bool, parent: Option<String>) -> Vec<Emit> {
        let mut out = Vec::new();
        let (name, input) = match kind {
            "commandExecution" => (
                "command".to_string(),
                json!({
                    "command": display_command(item),
                    "cwd": s(item, "cwd"),
                    "category": command_category(item),
                    "raw": s(item, "command"),
                }),
            ),
            "fileChange" => {
                let changes: Vec<Value> = item
                    .get("changes")
                    .and_then(Value::as_array)
                    .map(|c| c.iter().map(|x| json!({ "path": s(x, "path"), "kind": x.get("kind").and_then(|k| s(k, "type")).unwrap_or("update") })).collect())
                    .unwrap_or_default();
                if let Some(c) = item.get("changes") {
                    self.changes.insert(id.to_string(), c.clone());
                }
                ("edit".to_string(), json!({ "changes": changes }))
            }
            "mcpToolCall" => (
                format!("mcp__{}__{}", s(item, "server").unwrap_or("mcp"), s(item, "tool").unwrap_or("tool")),
                item.get("arguments").cloned().unwrap_or(Value::Null),
            ),
            "dynamicToolCall" => (s(item, "tool").unwrap_or("tool").to_string(), item.get("arguments").cloned().unwrap_or(Value::Null)),
            "webSearch" => ("web_search".to_string(), json!({ "query": s(item, "query") })),
            "imageView" => ("view_image".to_string(), json!({ "path": s(item, "path") })),
            "imageGeneration" => ("image_generation".to_string(), json!({ "prompt": s(item, "revisedPrompt") })),
            "collabAgentToolCall" => {
                for child in item.get("receiverThreadIds").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    self.children.insert(child.to_string(), id.to_string());
                }
                (
                    "agent".to_string(),
                    json!({ "tool": s(item, "tool"), "prompt": s(item, "prompt"), "model": s(item, "model") }),
                )
            }
            "subAgentActivity" | "functionCallOutput" | "sleep" => return out,
            other => (other.to_string(), json!({})),
        };
        if !self.shown.contains_key(id) {
            self.shown.insert(id.to_string(), Shown::Tool);
            out.push(Emit::ToolStart { call_id: id.to_string(), name, input: input.clone(), parent });
        } else if !done {
            out.push(Emit::ToolInput { call_id: id.to_string(), input });
        }
        if !done {
            return out;
        }
        self.shown.remove(id);
        let status = s(item, "status").unwrap_or("completed");
        let failed = matches!(status, "failed" | "declined");
        let output = match kind {
            "commandExecution" => {
                let streamed = self.output.remove(id).unwrap_or_default();
                let text = s(item, "aggregatedOutput").map(str::to_string).unwrap_or(streamed);
                match i(item, "exitCode") {
                    Some(code) if code != 0 => format!("{text}\n(exit code {code})"),
                    _ => text,
                }
            }
            "fileChange" => {
                let changes = self.changes.remove(id).unwrap_or(Value::Null);
                for c in changes.as_array().into_iter().flatten() {
                    if let Some(path) = s(c, "path") {
                        if !failed {
                            out.push(Emit::FileChanged { path: path.to_string(), call_id: id.to_string() });
                        }
                    }
                }
                changes
                    .as_array()
                    .map(|c| c.iter().filter_map(|x| s(x, "diff")).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default()
            }
            "mcpToolCall" => match item.get("error").filter(|e| !e.is_null()) {
                Some(e) => s(e, "message").unwrap_or("failed").to_string(),
                None => item
                    .get("result")
                    .and_then(|r| r.get("content"))
                    .and_then(Value::as_array)
                    .map(|c| c.iter().filter_map(|x| s(x, "text")).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default(),
            },
            "dynamicToolCall" => item
                .get("contentItems")
                .and_then(Value::as_array)
                .map(|c| c.iter().filter_map(|x| s(x, "text")).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
            "webSearch" => item
                .get("results")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(|x| s(x, "title").or_else(|| s(x, "url"))).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
            "collabAgentToolCall" => item
                .get("agentsStates")
                .and_then(Value::as_object)
                .map(|m| m.values().filter_map(|st| s(st, "message").or_else(|| s(st, "status"))).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
            "imageGeneration" => item.get("savedPath").and_then(Value::as_str).unwrap_or_default().to_string(),
            _ => String::new(),
        };
        let output = if status == "declined" && output.trim().is_empty() { "Declined.".to_string() } else { output };
        out.push(Emit::ToolResult { call_id: id.to_string(), output, is_error: failed });
        out
    }

    fn on_notification(&mut self, method: &str, params: &Value) -> Vec<Emit> {
        // Account-wide news, no thread id.
        if method == "account/rateLimits/updated" {
            let windows = params.get("rateLimits").map(rate_windows).unwrap_or_default();
            return if windows.is_empty() { Vec::new() } else { vec![Emit::RateLimit { windows }] };
        }
        if method == "thread/started" {
            // A subagent's thread: linked to its call once the call names it.
            return Vec::new();
        }
        let Some(parent) = self.ours(params) else {
            return Vec::new();
        };
        let child = parent.is_some();
        let mut out = Vec::new();
        match method {
            "turn/started" if !child => {
                if let Some(t) = params.get("turn").and_then(|t| s(t, "id")) {
                    self.turn = Some(t.to_string());
                    out.push(Emit::TurnRef { id: t.to_string() });
                }
            }
            "turn/completed" if !child => {
                let turn = params.get("turn").cloned().unwrap_or(Value::Null);
                if self.turn.as_deref().is_some_and(|t| Some(t) != s(&turn, "id")) {
                    // An older turn finishing late: not the one that runs now.
                    return out;
                }
                self.turn = None;
                self.starting = false;
                self.interrupt_pending = false;
                let error = turn.get("error").filter(|e| !e.is_null());
                let info = error.and_then(|e| e.get("codexErrorInfo")).cloned().unwrap_or(Value::Null);
                let limited = matches!(info.as_str(), Some("usageLimitExceeded" | "rateLimitExceeded"));
                let status = match s(&turn, "status") {
                    Some("completed") => TurnStatus::Completed,
                    Some("interrupted") => TurnStatus::Interrupted,
                    _ if limited => TurnStatus::Limited,
                    _ => TurnStatus::Failed,
                };
                let message = error.map(|e| {
                    let base = s(e, "message").unwrap_or_default().to_string();
                    match s(e, "additionalDetails").filter(|d| !d.trim().is_empty()) {
                        Some(d) => format!("{base}\n{d}"),
                        None => base,
                    }
                });
                let message = match status {
                    TurnStatus::Interrupted if std::mem::take(&mut self.cancelled) => {
                        Some("You declined, so Codex ended this turn.".to_string())
                    }
                    TurnStatus::Interrupted => None,
                    _ => message,
                };
                out.push(Emit::TurnEnd { status, error: message, duration_ms: i(&turn, "durationMs"), cost_usd: None });
            }
            "item/started" => out.extend(self.on_item(params, false, parent)),
            "item/completed" => out.extend(self.on_item(params, true, parent)),
            "item/agentMessage/delta" => {
                if let (Some(key), Some(text)) = (s(params, "itemId"), s(params, "delta")) {
                    if self.shown.get(key) == Some(&Shown::Message) {
                        out.push(Emit::TextDelta { key: key.to_string(), text: text.to_string() });
                    }
                }
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" | "item/reasoning/summaryPartAdded" => {
                let Some(key) = s(params, "itemId").map(str::to_string) else {
                    return out;
                };
                let text = if method == "item/reasoning/summaryPartAdded" { "\n\n" } else { s(params, "delta").unwrap_or_default() };
                if !self.shown.contains_key(&key) {
                    if text.trim().is_empty() {
                        return out;
                    }
                    self.shown.insert(key.clone(), Shown::Reasoning);
                    out.push(Emit::ReasoningStart { key: key.clone(), parent });
                }
                out.push(Emit::ReasoningDelta { key, text: text.to_string(), estimated_tokens: None });
            }
            "item/fileChange/patchUpdated" => {
                if let (Some(key), Some(changes)) = (s(params, "itemId"), params.get("changes")) {
                    self.changes.insert(key.to_string(), changes.clone());
                }
            }
            "item/commandExecution/outputDelta" => {
                if let (Some(key), Some(delta)) = (s(params, "itemId"), s(params, "delta")) {
                    let buf = self.output.entry(key.to_string()).or_default();
                    if buf.len() < OUTPUT_KEEP {
                        buf.push_str(&clip(delta, OUTPUT_KEEP - buf.len()));
                    }
                }
            }
            "turn/plan/updated" if !child => {
                let todos: Vec<Value> = params
                    .get("plan")
                    .and_then(Value::as_array)
                    .map(|p| {
                        p.iter()
                            .map(|step| {
                                let status = match s(step, "status") {
                                    Some("inProgress") => "in_progress",
                                    Some("completed") => "completed",
                                    _ => "pending",
                                };
                                json!({ "content": s(step, "step").unwrap_or_default(), "status": status, "activeForm": s(step, "step").unwrap_or_default() })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(Emit::Todos { todos: Value::Array(todos) });
            }
            "thread/tokenUsage/updated" if !child => {
                let usage = params.get("tokenUsage").cloned().unwrap_or(Value::Null);
                let last = usage.get("last").cloned().unwrap_or(Value::Null);
                let total = usage.get("total").cloned().unwrap_or(Value::Null);
                let now = (i(&total, "inputTokens").unwrap_or(0), i(&total, "outputTokens").unwrap_or(0));
                // On a resumed thread the first total includes earlier runs:
                // count only this call (`last`) from it.
                let before = self.totals.unwrap_or((
                    now.0 - i(&last, "inputTokens").unwrap_or(0),
                    now.1 - i(&last, "outputTokens").unwrap_or(0),
                ));
                self.totals = Some(now);
                out.push(Emit::Usage {
                    context_tokens: i(&last, "totalTokens"),
                    context_window: i(&usage, "modelContextWindow"),
                    cost_usd: None,
                    input_tokens: Some((now.0 - before.0).max(0)),
                    output_tokens: Some((now.1 - before.1).max(0)),
                });
            }
            "error" if !child => {
                let retry = params.get("willRetry").and_then(Value::as_bool).unwrap_or(false);
                let message = params.get("error").and_then(|e| s(e, "message")).unwrap_or_default();
                if retry && !self.retry_noted && !message.is_empty() {
                    self.retry_noted = true;
                    out.push(Emit::Notice { text: format!("Codex is retrying: {message}") });
                }
            }
            "model/rerouted" if !child => {
                let to = s(params, "toModel").or_else(|| s(params, "model")).unwrap_or("another model");
                out.push(Emit::Notice { text: format!("Codex switched this turn to {to}.") });
            }
            "warning" | "configWarning" if !child => {
                if let Some(text) = s(params, "message").or_else(|| s(params, "summary")) {
                    out.push(Emit::Notice { text: clip(text, 2000) });
                }
            }
            _ => {}
        }
        out
    }

    fn on_server_request(&mut self, rpc: Value, method: &str, params: &Value) -> Vec<Emit> {
        let request_id = match &rpc {
            Value::String(s) => format!("rpc-{s}"),
            other => format!("rpc-{other}"),
        };
        let parent = self.ours(params).flatten();
        let unsupported = |rpc: &Value, why: &str| Emit::Write(json!({ "id": rpc, "error": { "code": -32601, "message": why } }).to_string());
        let request = |kind: RequestKind, tool_name: String, call_id: Option<String>, input: Value, allow_session: bool| {
            Emit::Request(Request { request_id: request_id.clone(), kind, tool_name, call_id, input, parent: parent.clone(), allow_session, by_message: false })
        };
        match method {
            "item/commandExecution/requestApproval" => {
                let decisions: Vec<Value> = params.get("availableDecisions").and_then(Value::as_array).cloned().unwrap_or_default();
                let allow_session = decisions.is_empty() || decisions.iter().any(|d| d.as_str() == Some("acceptForSession"));
                let mut input = json!({ "command": display_command(params), "cwd": s(params, "cwd") });
                if let Some(r) = s(params, "reason") {
                    input["reason"] = json!(r);
                }
                if let Some(r) = s(params, "command") {
                    input["raw"] = json!(r);
                }
                self.requests.insert(request_id.clone(), Pending::Command { rpc, decisions });
                vec![request(RequestKind::Approval, "command".into(), s(params, "itemId").map(str::to_string), input, allow_session)]
            }
            "item/fileChange/requestApproval" => {
                let item = s(params, "itemId").unwrap_or_default().to_string();
                let changes: Vec<Value> = self
                    .changes
                    .get(&item)
                    .and_then(Value::as_array)
                    .map(|c| c.iter().map(|x| json!({ "path": s(x, "path"), "diff": s(x, "diff").map(|d| clip(d, 2000)) })).collect())
                    .unwrap_or_default();
                let mut input = json!({ "changes": changes });
                if let Some(r) = s(params, "reason") {
                    input["reason"] = json!(r);
                }
                if let Some(root) = s(params, "grantRoot") {
                    input["grantRoot"] = json!(root);
                }
                self.requests.insert(request_id.clone(), Pending::File { rpc });
                vec![request(RequestKind::Approval, "edit".into(), Some(item), input, true)]
            }
            "item/permissions/requestApproval" => {
                let requested = params.get("permissions").cloned().unwrap_or(json!({}));
                let input = json!({ "permissions": requested, "reason": s(params, "reason"), "cwd": s(params, "cwd") });
                self.requests.insert(request_id.clone(), Pending::Permissions { rpc, requested });
                vec![request(RequestKind::Approval, "permissions".into(), s(params, "itemId").map(str::to_string), input, true)]
            }
            "mcpServer/elicitation/request" => {
                if s(params, "mode") == Some("url") {
                    return vec![
                        Emit::Write(Self::reply(&rpc, json!({ "action": "decline" }))),
                        Emit::Notice { text: format!("{} asked to open a web page to continue; QuantPilot declined it.", s(params, "serverName").unwrap_or("An MCP server")) },
                    ];
                }
                let meta = params.get("_meta").cloned().unwrap_or(Value::Null);
                let server = s(params, "serverName").unwrap_or("mcp");
                let fields: Vec<FormField> = params
                    .get("requestedSchema")
                    .and_then(|r| r.get("properties"))
                    .and_then(Value::as_object)
                    .map(|props| {
                        props
                            .iter()
                            .map(|(k, v)| FormField {
                                key: k.clone(),
                                title: s(v, "title").or_else(|| s(v, "description")).unwrap_or(k).to_string(),
                                kind: s(v, "type").unwrap_or("string").to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let message = s(params, "message").unwrap_or_default().to_string();
                if fields.is_empty() {
                    let tool = meta.get("tool_name").and_then(Value::as_str).map(str::to_string).or_else(|| {
                        message.split('"').nth(1).map(str::to_string)
                    });
                    let name = match &tool {
                        Some(t) => format!("mcp__{server}__{t}"),
                        None => format!("mcp__{server}"),
                    };
                    let input = meta.get("tool_params").cloned().unwrap_or(json!({}));
                    let persist = meta.get("persist").map(|p| p.to_string()).unwrap_or_default();
                    self.requests.insert(request_id.clone(), Pending::Elicitation { rpc, fields });
                    return vec![request(RequestKind::Approval, name, None, input, persist.contains("session"))];
                }
                let questions: Vec<Value> = fields
                    .iter()
                    .map(|f| {
                        let options = if f.kind == "boolean" { json!([{ "label": "Yes", "description": "" }, { "label": "No", "description": "" }]) } else { json!([]) };
                        json!({ "question": f.title, "header": server, "options": options, "multiSelect": false })
                    })
                    .collect();
                self.requests.insert(request_id.clone(), Pending::Elicitation { rpc, fields });
                vec![request(RequestKind::Question, "mcp".into(), None, json!({ "questions": questions, "message": message }), false)]
            }
            "item/tool/requestUserInput" => {
                let list = params.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
                let questions: Vec<Value> = list
                    .iter()
                    .map(|q| {
                        let options = q.get("options").cloned().filter(|o| o.is_array()).unwrap_or(json!([]));
                        json!({ "question": s(q, "question"), "header": s(q, "header").unwrap_or("Question"), "options": options, "multiSelect": false })
                    })
                    .collect();
                let ids = list
                    .iter()
                    .map(|q| (s(q, "id").unwrap_or_default().to_string(), s(q, "question").unwrap_or_default().to_string()))
                    .collect();
                self.requests.insert(request_id.clone(), Pending::UserInput { rpc, questions: ids });
                vec![request(RequestKind::Question, "question".into(), s(params, "itemId").map(str::to_string), json!({ "questions": questions }), false)]
            }
            "execCommandApproval" | "applyPatchApproval" => {
                let tool = if method == "execCommandApproval" { "command" } else { "edit" };
                let command = params.get("command").and_then(Value::as_array).map(|c| c.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "));
                let input = json!({ "command": command, "reason": s(params, "reason"), "changes": params.get("fileChanges") });
                self.requests.insert(request_id.clone(), Pending::Legacy { rpc });
                vec![request(RequestKind::Approval, tool.into(), s(params, "callId").map(str::to_string), input, true)]
            }
            _ => vec![unsupported(&rpc, "QuantPilot does not handle this request")],
        }
    }
}

impl Driver for CodexDriver {
    fn provider(&self) -> &'static str {
        "codex"
    }

    fn tool_info(&self, name: &str, input: &Value) -> ToolInfo {
        let f = |k: &str| s(input, k).unwrap_or_default().to_string();
        let (category, title) = match name {
            "command" => {
                let category = match s(input, "category") {
                    Some("read") => "read",
                    Some("search") => "search",
                    _ => "command",
                };
                (category, f("command"))
            }
            "edit" => {
                let paths: Vec<&str> = input
                    .get("changes")
                    .and_then(Value::as_array)
                    .map(|c| c.iter().filter_map(|x| s(x, "path")).collect())
                    .unwrap_or_default();
                let title = match paths.as_slice() {
                    [] => String::new(),
                    [one] => one.to_string(),
                    [first, rest @ ..] => format!("{first} and {} more", rest.len()),
                };
                ("edit", title)
            }
            "web_search" => ("web", f("query")),
            "view_image" => ("read", f("path")),
            "agent" => ("agent", {
                let p = f("prompt");
                if p.is_empty() { f("tool") } else { p }
            }),
            "permissions" => ("tool", "More access for the sandbox".into()),
            "question" | "mcp" => ("tool", String::new()),
            n if n.starts_with("mcp__") => ("mcp", n.trim_start_matches("mcp__").replacen("__", " · ", 1)),
            _ => ("tool", String::new()),
        };
        let title = if title.is_empty() { name.to_string() } else { title };
        ToolInfo { category, title: title.lines().next().unwrap_or_default().to_string() }
    }

    fn handshake(&mut self) -> Vec<String> {
        let params = json!({
            "clientInfo": { "name": "quantpilot", "title": "QuantPilot", "version": env!("CARGO_PKG_VERSION") },
            // Diffs come from checkpoints (card 11), not from Codex.
            "capabilities": { "experimentalApi": true, "optOutNotificationMethods": ["turn/diff/updated"] },
        });
        vec![self.request(Call::Initialize, "initialize", params)]
    }

    fn user_message(&mut self, input: &TurnInput) -> Vec<String> {
        if self.thread.is_none() {
            // Sent once `thread/start` (or resume) answered.
            self.queued.push(input.clone());
            return Vec::new();
        }
        if self.turn.is_some() {
            return vec![self.steer_line(input)];
        }
        if self.starting {
            // The turn id is on its way; the message steers once it is here.
            self.queued.push(input.clone());
            return Vec::new();
        }
        vec![self.turn_start(input)]
    }

    fn on_line(&mut self, line: &str) -> Vec<Emit> {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let method = s(&msg, "method").map(str::to_string);
        match (msg.get("id").cloned(), method) {
            (Some(id), Some(method)) => {
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                self.on_server_request(id, &method, &params)
            }
            (Some(id), None) => match id.as_u64() {
                Some(n) => self.on_response(n, &msg),
                None => Vec::new(),
            },
            (None, Some(method)) => {
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                self.on_notification(&method, &params)
            }
            (None, None) => Vec::new(),
        }
    }

    fn answer(&mut self, request_id: &str, answer: &Answer) -> Result<Vec<String>, String> {
        let pending = self.requests.remove(request_id).ok_or("this request is no longer waiting")?;
        let line = match pending {
            Pending::Command { rpc, decisions } => {
                let offered = |d: &str| decisions.is_empty() || decisions.iter().any(|x| x.as_str() == Some(d));
                let decision = match answer {
                    Answer::Allow => "accept",
                    Answer::AllowSession if offered("acceptForSession") => "acceptForSession",
                    Answer::AllowSession => "accept",
                    _ if offered("decline") => "decline",
                    _ => {
                        self.cancelled = true;
                        "cancel"
                    }
                };
                Self::reply(&rpc, json!({ "decision": decision }))
            }
            Pending::File { rpc } => {
                let decision = match answer {
                    Answer::Allow => "accept",
                    Answer::AllowSession => "acceptForSession",
                    _ => "decline",
                };
                Self::reply(&rpc, json!({ "decision": decision }))
            }
            Pending::Permissions { rpc, requested } => match answer {
                Answer::Allow => Self::reply(&rpc, json!({ "permissions": requested, "scope": "turn" })),
                Answer::AllowSession => Self::reply(&rpc, json!({ "permissions": requested, "scope": "session" })),
                _ => Self::reply(&rpc, json!({ "permissions": {} })),
            },
            Pending::Elicitation { rpc, fields } => match answer {
                Answer::Allow if fields.is_empty() => Self::reply(&rpc, json!({ "action": "accept", "content": {} })),
                Answer::AllowSession if fields.is_empty() => {
                    Self::reply(&rpc, json!({ "action": "accept", "content": {}, "_meta": { "persist": "session" } }))
                }
                Answer::Answers { answers } => {
                    let mut content = Map::new();
                    for f in &fields {
                        let raw = answers.get(&f.title).and_then(Value::as_str).unwrap_or_default().trim().to_string();
                        let value = match f.kind.as_str() {
                            "boolean" => json!(raw.eq_ignore_ascii_case("yes") || raw.eq_ignore_ascii_case("true")),
                            "number" | "integer" => raw.parse::<f64>().map(|n| json!(n)).unwrap_or(json!(raw)),
                            _ => json!(raw),
                        };
                        content.insert(f.key.clone(), value);
                    }
                    Self::reply(&rpc, json!({ "action": "accept", "content": content }))
                }
                Answer::Dismiss => Self::reply(&rpc, json!({ "action": "cancel" })),
                _ => Self::reply(&rpc, json!({ "action": "decline" })),
            },
            Pending::UserInput { rpc, questions } => {
                let mut out = Map::new();
                if let Answer::Answers { answers } = answer {
                    for (id, text) in &questions {
                        let value = answers.get(text).and_then(Value::as_str).unwrap_or_default();
                        if !value.is_empty() {
                            out.insert(id.clone(), json!({ "answers": [value] }));
                        }
                    }
                }
                Self::reply(&rpc, json!({ "answers": out }))
            }
            Pending::Legacy { rpc } => {
                let decision = match answer {
                    Answer::Allow => "approved",
                    Answer::AllowSession => "approved_for_session",
                    _ => "denied",
                };
                Self::reply(&rpc, json!({ "decision": decision }))
            }
        };
        Ok(vec![line])
    }

    fn interrupt(&mut self) -> Vec<String> {
        match (&self.thread, &self.turn) {
            (Some(thread), Some(turn)) => {
                let params = json!({ "threadId": thread, "turnId": turn });
                vec![self.request(Call::Interrupt, "turn/interrupt", params)]
            }
            _ => {
                // Nothing to name yet: stop as soon as the turn id is known.
                self.interrupt_pending = true;
                self.queued.clear();
                Vec::new()
            }
        }
    }

    fn set_mode(&mut self, mode: &str) -> Option<String> {
        self.opts.mode = mode.to_string();
        let thread = self.thread.clone()?;
        let (approval, reviewer, sandbox) = policy(mode, &self.opts.writable_roots);
        let params = json!({ "threadId": thread, "approvalPolicy": approval, "approvalsReviewer": reviewer, "sandboxPolicy": sandbox });
        Some(self.request(Call::Settings, "thread/settings/update", params))
    }

    fn set_model(&mut self, model: &str) -> Option<String> {
        self.opts.model = model.to_string();
        let thread = self.thread.clone()?;
        let mut params = json!({ "threadId": thread });
        if !model.is_empty() {
            params["model"] = json!(model);
        }
        Some(self.request(Call::Settings, "thread/settings/update", params))
    }

    fn can_steer(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> CodexDriver {
        CodexDriver::new(CodexOptions {
            cwd: "C:\\work".into(),
            mode: "ask".into(),
            instructions: "Context.".into(),
            mcp_url: Some("http://127.0.0.1:3100/mcp".into()),
            ..Default::default()
        })
    }

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }

    fn writes(emits: &[Emit]) -> Vec<Value> {
        emits.iter().filter_map(|e| if let Emit::Write(l) = e { Some(parse(l)) } else { None }).collect()
    }

    /// Feed recorded frames; returns all emits.
    fn feed(d: &mut CodexDriver, frames: &[Value]) -> Vec<Emit> {
        frames.iter().flat_map(|f| d.on_line(&f.to_string())).collect()
    }

    /// Handshake, thread start and turn start, as the runtime drives them.
    fn started() -> CodexDriver {
        started_on("th-1", "tu-1")
    }

    fn started_on(thread: &str, turn_id: &str) -> CodexDriver {
        let mut d = driver();
        let init = parse(&d.handshake()[0]);
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["capabilities"]["optOutNotificationMethods"][0], "turn/diff/updated");
        assert!(d.user_message(&TurnInput { text: "hi".into(), ..Default::default() }).is_empty(), "waits for the thread");
        let out = feed(&mut d, &[json!({ "id": 1, "result": { "userAgent": "quantpilot/0.165.0 (Windows)", "platformOs": "windows" } })]);
        let w = writes(&out);
        assert_eq!(w[0]["method"], "initialized");
        let start = w.iter().find(|m| m["method"] == "thread/start").expect("thread/start");
        assert_eq!(start["params"]["developerInstructions"], "Context.");
        assert_eq!(start["params"]["config"]["mcp_servers.QuantMCP"]["url"], "http://127.0.0.1:3100/mcp");
        let id = start["id"].as_u64().unwrap();
        let out = feed(&mut d, &[json!({ "id": id, "result": { "thread": { "id": thread }, "model": "gpt-x" } })]);
        assert!(out.iter().any(|e| matches!(e, Emit::Bound { session_id, .. } if session_id == thread)));
        let turn = writes(&out).into_iter().find(|m| m["method"] == "turn/start").expect("turn/start");
        assert_eq!(turn["params"]["approvalPolicy"], "untrusted");
        assert_eq!(turn["params"]["sandboxPolicy"]["type"], "readOnly");
        assert_eq!(turn["params"]["input"][0]["text"], "hi");
        let tid = turn["id"].as_u64().unwrap();
        feed(&mut d, &[json!({ "id": tid, "result": { "turn": { "id": turn_id, "status": "inProgress" } } })]);
        d
    }

    /// A real Supervised turn of codex-cli 0.165.0 (scrubbed probe): an MCP
    /// tool approval, a command the read-only sandbox blocks, the same
    /// command again after an escalation approval, the answer.
    #[test]
    fn a_recorded_supervised_turn_replays() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_approval_turn.jsonl");
        let text = std::fs::read_to_string(path).unwrap();
        let mut d = started_on("01a187cc-6c72-79e3-b2bd-067959b5eade", "01a187cc-6ccf-7fd1-bcb1-7807af059417");
        let mut emits = Vec::new();
        let mut replies = Vec::new();
        for line in text.lines() {
            for e in d.on_line(line) {
                if let Emit::Request(r) = &e {
                    replies.push(parse(&d.answer(&r.request_id, &Answer::Allow).unwrap()[0]));
                }
                emits.push(e);
            }
        }
        let requests: Vec<&Request> = emits.iter().filter_map(|e| if let Emit::Request(r) = e { Some(r) } else { None }).collect();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].tool_name, "mcp__QuantMCP__get_instructions");
        assert_eq!(requests[1].input["command"], "echo hello > hello.txt");
        assert!(requests[2].input["reason"].as_str().unwrap().contains("read-only sandbox"));
        assert_eq!(replies[0]["result"]["action"], "accept");
        assert_eq!(replies[1]["result"]["decision"], "accept");
        let tools: Vec<(&str, &str)> = emits
            .iter()
            .filter_map(|e| if let Emit::ToolStart { call_id, name, .. } = e { Some((call_id.as_str(), name.as_str())) } else { None })
            .collect();
        assert_eq!(tools.iter().map(|t| t.1).collect::<Vec<_>>(), vec!["mcp__QuantMCP__get_instructions", "command", "command"]);
        let results: Vec<bool> = emits.iter().filter_map(|e| if let Emit::ToolResult { is_error, .. } = e { Some(*is_error) } else { None }).collect();
        assert_eq!(results, vec![true, true, false], "the rejected MCP call and the blocked write fail, the approved one runs");
        let messages: Vec<&str> = emits.iter().filter_map(|e| if let Emit::MessageDone { text, .. } = e { Some(text.as_str()) } else { None }).collect();
        assert_eq!(messages.last(), Some(&"done"));
        assert!(matches!(emits.last(), Some(Emit::TurnEnd { status: TurnStatus::Completed, .. })));
        assert!(
            !emits.iter().any(|e| matches!(e, Emit::ReasoningStart { .. })),
            "empty reasoning items stay out of the timeline"
        );
    }

    #[test]
    fn a_turn_streams_and_completes() {
        let mut d = started();
        let out = feed(
            &mut d,
            &[
                json!({ "method": "turn/started", "params": { "threadId": "th-1", "turn": { "id": "tu-1" } } }),
                json!({ "method": "item/started", "params": { "threadId": "th-1", "turnId": "tu-1", "item": { "type": "userMessage", "id": "u", "content": [] } } }),
                json!({ "method": "item/started", "params": { "threadId": "th-1", "turnId": "tu-1", "item": { "type": "agentMessage", "id": "m1", "text": "", "phase": "commentary" } } }),
                json!({ "method": "item/agentMessage/delta", "params": { "threadId": "th-1", "turnId": "tu-1", "itemId": "m1", "delta": "Look" } }),
                json!({ "method": "item/agentMessage/delta", "params": { "threadId": "th-1", "turnId": "tu-1", "itemId": "m1", "delta": "ing." } }),
                json!({ "method": "item/completed", "params": { "threadId": "th-1", "turnId": "tu-1", "item": { "type": "agentMessage", "id": "m1", "text": "Looking.", "phase": "commentary" } } }),
                json!({ "method": "thread/tokenUsage/updated", "params": { "threadId": "th-1", "turnId": "tu-1", "tokenUsage": {
                    "total": { "inputTokens": 900, "outputTokens": 40, "totalTokens": 940, "cachedInputTokens": 0, "reasoningOutputTokens": 0 },
                    "last": { "inputTokens": 100, "outputTokens": 10, "totalTokens": 110, "cachedInputTokens": 0, "reasoningOutputTokens": 0 },
                    "modelContextWindow": 258000 } } }),
                json!({ "method": "turn/completed", "params": { "threadId": "th-1", "turn": { "id": "tu-1", "status": "completed", "durationMs": 1200 } } }),
            ],
        );
        assert_eq!(
            out,
            vec![
                Emit::TurnRef { id: "tu-1".into() },
                Emit::MessageStart { key: "m1".into(), phase: Some("commentary".into()), parent: None },
                Emit::TextDelta { key: "m1".into(), text: "Look".into() },
                Emit::TextDelta { key: "m1".into(), text: "ing.".into() },
                Emit::MessageDone { key: "m1".into(), text: "Looking.".into() },
                Emit::Usage { context_tokens: Some(110), context_window: Some(258000), cost_usd: None, input_tokens: Some(100), output_tokens: Some(10) },
                Emit::TurnEnd { status: TurnStatus::Completed, error: None, duration_ms: Some(1200), cost_usd: None },
            ],
            "the user echo is skipped; a resumed thread counts only this call"
        );
        // The next message starts a new turn on the same thread.
        let next = parse(&d.user_message(&TurnInput { text: "again".into(), ..Default::default() })[0]);
        assert_eq!(next["method"], "turn/start");
    }

    #[test]
    fn commands_and_file_changes_become_tool_rows() {
        let mut d = started();
        let cmd = json!({ "type": "commandExecution", "id": "c1", "command": "\"C:\\\\pwsh.exe\" -Command 'cargo test'", "cwd": "C:\\work",
            "status": "inProgress", "commandActions": [{ "type": "unknown", "command": "cargo test" }] });
        let mut done = cmd.clone();
        done["status"] = json!("failed");
        done["exitCode"] = json!(101);
        let fc = json!({ "type": "fileChange", "id": "f1", "status": "inProgress", "changes": [
            { "path": "src/a.rs", "kind": { "type": "update" }, "diff": "@@ -1 +1 @@\n-a\n+b" } ] });
        let mut fc_done = fc.clone();
        fc_done["status"] = json!("completed");
        let out = feed(
            &mut d,
            &[
                json!({ "method": "item/started", "params": { "threadId": "th-1", "item": cmd } }),
                json!({ "method": "item/commandExecution/outputDelta", "params": { "threadId": "th-1", "itemId": "c1", "delta": "test failed\n" } }),
                json!({ "method": "item/completed", "params": { "threadId": "th-1", "item": done } }),
                json!({ "method": "item/started", "params": { "threadId": "th-1", "item": fc } }),
                json!({ "method": "item/completed", "params": { "threadId": "th-1", "item": fc_done } }),
            ],
        );
        match &out[0] {
            Emit::ToolStart { call_id, name, input, .. } => {
                assert_eq!((call_id.as_str(), name.as_str()), ("c1", "command"));
                assert_eq!(input["command"], "cargo test", "the wrapper is unwrapped");
                assert_eq!(d.tool_info(name, input).title, "cargo test");
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(out[1], Emit::ToolResult { call_id: "c1".into(), output: "test failed\n\n(exit code 101)".into(), is_error: true });
        assert!(matches!(&out[2], Emit::ToolStart { name, .. } if name == "edit"));
        assert_eq!(out[3], Emit::FileChanged { path: "src/a.rs".into(), call_id: "f1".into() });
        assert!(matches!(&out[4], Emit::ToolResult { output, is_error: false, .. } if output.contains("+b")));
    }

    #[test]
    fn approvals_round_trip() {
        let mut d = started();
        let out = feed(
            &mut d,
            &[json!({ "id": 7, "method": "item/commandExecution/requestApproval", "params": {
                "threadId": "th-1", "turnId": "tu-1", "itemId": "c2", "command": "pwsh -Command 'rm x'", "reason": "outside the sandbox",
                "commandActions": [{ "type": "unknown", "command": "rm x" }],
                "availableDecisions": ["accept", { "acceptWithExecpolicyAmendment": { "execpolicy_amendment": ["rm"] } }, "cancel"] } })],
        );
        let Emit::Request(r) = &out[0] else { panic!("{out:?}") };
        assert_eq!(r.request_id, "rpc-7");
        assert_eq!(r.input["command"], "rm x");
        assert!(!r.allow_session, "Codex did not offer a session grant here");
        let deny = parse(&d.answer("rpc-7", &Answer::Deny { message: None }).unwrap()[0]);
        assert_eq!(deny, json!({ "id": 7, "result": { "decision": "cancel" } }), "no decline offered → cancel");
        assert!(d.answer("rpc-7", &Answer::Allow).is_err(), "answered once");
        let end = feed(&mut d, &[json!({ "method": "turn/completed", "params": { "threadId": "th-1", "turn": { "id": "tu-1", "status": "interrupted" } } })]);
        assert!(
            matches!(&end[0], Emit::TurnEnd { status: TurnStatus::Interrupted, error: Some(e), .. } if e.contains("declined")),
            "a cancel says why the turn ended: {end:?}"
        );
        d.turn = Some("tu-1".into());

        let out = feed(&mut d, &[json!({ "id": "e1", "method": "mcpServer/elicitation/request", "params": {
            "threadId": "th-1", "turnId": "tu-1", "serverName": "QuantMCP", "mode": "form",
            "_meta": { "codex_approval_kind": "mcp_tool_call", "persist": ["session", "always"], "tool_params": { "include_global": false } },
            "message": "Allow the QuantMCP MCP server to run tool \"get_instructions\"?", "requestedSchema": { "type": "object", "properties": {} } } })]);
        let Emit::Request(r) = &out[0] else { panic!("{out:?}") };
        assert_eq!(r.tool_name, "mcp__QuantMCP__get_instructions");
        assert!(r.allow_session);
        let ok = parse(&d.answer("rpc-e1", &Answer::AllowSession).unwrap()[0]);
        assert_eq!(ok, json!({ "id": "e1", "result": { "action": "accept", "content": {}, "_meta": { "persist": "session" } } }));

        feed(&mut d, &[json!({ "method": "item/started", "params": { "threadId": "th-1", "item": { "type": "fileChange", "id": "f9", "status": "inProgress",
            "changes": [{ "path": "b.rs", "kind": { "type": "add" }, "diff": "+x" }] } } })]);
        let out = feed(&mut d, &[json!({ "id": 8, "method": "item/fileChange/requestApproval", "params": { "threadId": "th-1", "turnId": "tu-1", "itemId": "f9" } })]);
        let Emit::Request(r) = &out[0] else { panic!("{out:?}") };
        assert_eq!(r.input["changes"][0]["path"], "b.rs", "the approval shows what changes");
        assert_eq!(d.tool_info(&r.tool_name, &r.input).title, "b.rs");
        assert_eq!(parse(&d.answer("rpc-8", &Answer::Allow).unwrap()[0])["result"]["decision"], "accept");
    }

    #[test]
    fn questions_blocking_and_async() {
        let mut d = started();
        let out = feed(&mut d, &[json!({ "id": 9, "method": "item/tool/requestUserInput", "params": {
            "threadId": "th-1", "turnId": "tu-1", "itemId": "q", "isBlocking": true,
            "questions": [{ "id": "db", "header": "Database", "question": "Which database?", "options": [{ "label": "SQLite", "description": "file" }, { "label": "Postgres", "description": "" }] }] } })]);
        let Emit::Request(r) = &out[0] else { panic!("{out:?}") };
        assert_eq!(r.kind, RequestKind::Question);
        assert_eq!(r.input["questions"][0]["question"], "Which database?");
        let mut answers = Map::new();
        answers.insert("Which database?".into(), json!("SQLite"));
        let reply = parse(&d.answer("rpc-9", &Answer::Answers { answers }).unwrap()[0]);
        assert_eq!(reply, json!({ "id": 9, "result": { "answers": { "db": { "answers": ["SQLite"] } } } }));

        let out = feed(&mut d, &[
            json!({ "method": "item/started", "params": { "threadId": "th-1", "item": { "type": "agentMessage", "id": "a1", "text": "", "delivery": "async" } } }),
            json!({ "method": "item/completed", "params": { "threadId": "th-1", "item": { "type": "agentMessage", "id": "a1", "text": "Before I go on:",
                "delivery": "async", "questions": [{ "title": "Keep the old API?", "options": ["Yes", "No"] }] } } }),
        ]);
        let req = out.iter().find_map(|e| if let Emit::Request(r) = e { Some(r.clone()) } else { None }).expect("async question");
        assert!(req.by_message, "answered with a new user message");
        assert_eq!(req.request_id, "async:a1");
        assert_eq!(req.input["questions"][0]["options"][1]["label"], "No");
        assert!(out.contains(&Emit::MessageDone { key: "a1".into(), text: "Before I go on:".into() }));
    }

    #[test]
    fn subagents_nest_and_never_end_the_turn() {
        let mut d = started();
        let out = feed(&mut d, &[
            json!({ "method": "item/started", "params": { "threadId": "th-1", "item": { "type": "collabAgentToolCall", "id": "s1", "tool": "spawnAgent",
                "prompt": "Check the tests", "senderThreadId": "th-1", "receiverThreadIds": ["child"], "agentsStates": {}, "status": "inProgress" } } }),
            json!({ "method": "item/started", "params": { "threadId": "child", "item": { "type": "agentMessage", "id": "cm", "text": "" } } }),
            json!({ "method": "turn/completed", "params": { "threadId": "child", "turn": { "id": "ct", "status": "completed" } } }),
            json!({ "method": "item/started", "params": { "threadId": "stranger", "item": { "type": "agentMessage", "id": "x", "text": "" } } }),
        ]);
        assert!(matches!(&out[0], Emit::ToolStart { name, .. } if name == "agent"));
        assert_eq!(out[1], Emit::MessageStart { key: "cm".into(), phase: None, parent: Some("s1".into()) });
        assert_eq!(out.len(), 2, "the child's turn end and unknown threads are ignored");
    }

    #[test]
    fn limits_interrupts_and_failures() {
        let mut d = started();
        let stop = parse(&d.interrupt()[0]);
        assert_eq!(stop["method"], "turn/interrupt");
        assert_eq!(stop["params"], json!({ "threadId": "th-1", "turnId": "tu-1" }));
        let out = feed(&mut d, &[
            json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": { "primary": { "usedPercent": 100, "windowDurationMins": 300, "resetsAt": 1700000000 }, "secondary": { "usedPercent": 9, "windowDurationMins": 10080 } } } }),
            json!({ "method": "turn/completed", "params": { "threadId": "th-1", "turn": { "id": "tu-1", "status": "failed",
                "error": { "message": "You've hit your usage limit.", "codexErrorInfo": "usageLimitExceeded" } } } }),
        ]);
        let Emit::RateLimit { windows } = &out[0] else { panic!("{out:?}") };
        assert_eq!(windows[0].kind, "five_hour");
        assert_eq!(windows[0].resets_at, Some(1_700_000_000_000));
        assert_eq!(windows[0].status.as_deref(), Some("rejected"));
        assert_eq!(windows[1].kind, "seven_day");
        assert!(matches!(&out[1], Emit::TurnEnd { status: TurnStatus::Limited, error: Some(e), .. } if e.contains("usage limit")));
    }

    #[test]
    fn steering_and_live_settings() {
        let mut d = started();
        let steer = parse(&d.user_message(&TurnInput { text: "also docs".into(), ..Default::default() })[0]);
        assert_eq!(steer["method"], "turn/steer");
        assert_eq!(steer["params"]["expectedTurnId"], "tu-1");
        let full = parse(&d.set_mode("full").unwrap());
        assert_eq!(full["method"], "thread/settings/update");
        assert_eq!(full["params"]["approvalPolicy"], "never");
        assert_eq!(full["params"]["sandboxPolicy"]["type"], "dangerFullAccess");
        assert!(driver().set_mode("full").is_none(), "no thread yet: the next turn starts with it");
    }

    #[test]
    fn modes_map_to_policies() {
        let roots = vec!["C:\\vault".to_string()];
        assert_eq!(policy("ask", &roots).0, json!("untrusted"));
        assert_eq!(policy("edits", &roots).2["writableRoots"][0], "C:\\vault");
        assert_eq!(policy("auto", &roots).1, "auto_review");
        assert_eq!(policy("plan", &roots).2["type"], "readOnly");
        assert_eq!(policy("full", &roots).0, json!("never"));
    }

    #[test]
    fn a_failed_resume_starts_fresh() {
        let mut d = CodexDriver::new(CodexOptions { resume: Some("gone".into()), mode: "full".into(), ..Default::default() });
        d.handshake();
        d.user_message(&TurnInput { text: "hi".into(), ..Default::default() });
        let out = feed(&mut d, &[json!({ "id": 1, "result": { "userAgent": "x/1.0" } })]);
        let resume = writes(&out).into_iter().find(|m| m["method"] == "thread/resume").unwrap();
        assert_eq!(resume["params"]["threadId"], "gone");
        let out = feed(&mut d, &[json!({ "id": resume["id"], "error": { "code": -1, "message": "no rollout found" } })]);
        assert!(matches!(&out[0], Emit::Notice { text } if text.contains("no rollout found")));
        assert_eq!(writes(&out)[0]["method"], "thread/start");
    }

    #[test]
    fn discovery_reports_models_and_skills() {
        let mut d = CodexDriver::discovery("C:\\vault");
        d.handshake();
        let out = feed(&mut d, &[json!({ "id": 1, "result": { "userAgent": "quantpilot/0.165.0 (Windows)" } })]);
        assert!(!writes(&out).iter().any(|m| m["method"] == "thread/start"), "discovery opens no thread");
        let out = feed(&mut d, &[
            json!({ "id": 2, "result": { "data": [{ "cwd": "C:\\vault", "errors": [], "skills": [
                { "name": "pdf", "description": "Read PDFs", "enabled": true, "path": "x", "scope": "user" },
                { "name": "off", "description": "", "enabled": false, "path": "y", "scope": "user" }] }] } }),
            json!({ "id": 3, "result": { "data": [
                { "id": "gpt-a", "model": "gpt-a", "displayName": "GPT-A", "description": "fast", "hidden": false, "isDefault": true,
                  "defaultReasoningEffort": "low", "supportedReasoningEfforts": [{ "reasoningEffort": "low", "description": "" }, { "reasoningEffort": "high", "description": "" }] },
                { "id": "hidden", "model": "hidden", "displayName": "H", "description": "", "hidden": true, "isDefault": false,
                  "defaultReasoningEffort": "low", "supportedReasoningEfforts": [] }] } }),
        ]);
        let Emit::Catalog(c) = &out[0] else { panic!("{out:?}") };
        assert_eq!(c.version.as_deref(), Some("0.165.0"));
        assert_eq!(c.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["default", "gpt-a"], "hidden models stay out");
        assert_eq!(c.models[0].resolved_model.as_deref(), Some("gpt-a"));
        assert_eq!(c.models[1].efforts, vec!["low", "high"]);
        assert_eq!(c.skills.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), vec!["pdf"]);
    }
}
