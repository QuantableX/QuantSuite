//! Claude Code over its stream-json control protocol — the same transport
//! the Agent SDK uses (docs/PLAN-QUANTPILOT.md §5.1).
//!
//! ```text
//! claude -p --input-format stream-json --output-format stream-json --verbose
//!        --include-partial-messages --permission-prompt-tool stdio …
//! ```
//!
//! Verified against Claude Code 2.1.312 (fixtures in `tests/fixtures/`):
//! - `initialize` is a `control_request`; its `control_response` carries
//!   `models` (value, displayName, description, resolvedModel,
//!   supportedEffortLevels), `commands` and `claude_code_version`.
//! - Permission prompts reach the host only with
//!   `--permission-prompt-tool stdio` — `--permission-prompts host` alone
//!   denies them. They arrive as `control_request` `can_use_tool` (tool_name,
//!   input, tool_use_id) and are answered with a `control_response`
//!   (`behavior` allow + `updatedInput`, or deny + `message`).
//! - Each content block streams as `stream_event`s; a whole-block
//!   `assistant` frame arrives just before its `content_block_stop`.
//! - `result` carries cumulative `total_cost_usd` for the process and
//!   `modelUsage[model].contextWindow`.
//! - `--permission-mode` accepts acceptEdits, auto, bypassPermissions,
//!   manual, dontAsk and plan; `default` is still accepted as manual.

use crate::driver::{
    Answer, Catalog, CommandInfo, Driver, Emit, ImageInput, ModelInfo, Request, RequestKind, ToolInfo, TurnInput,
    TurnStatus,
};
use crate::events::RateWindow;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// The thread's permission mode, stored as these strings.
pub fn permission_args(mode: &str) -> Vec<String> {
    let native = match mode {
        "edits" => "acceptEdits",
        "auto" => "auto",
        "plan" => "plan",
        "full" => "bypassPermissions",
        _ => "default",
    };
    let mut args = vec!["--permission-mode".to_string(), native.to_string()];
    if mode == "full" {
        args.push("--dangerously-skip-permissions".into());
    }
    args
}

/// A new conversation takes the thread's id; a known one is resumed.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionRef {
    New(String),
    Resume(String),
}

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub session: SessionRef,
    pub model: String,
    pub effort: String,
    pub mode: String,
    pub mcp_config: Option<PathBuf>,
    pub append_prompt_file: Option<PathBuf>,
    pub add_dirs: Vec<String>,
}

fn path_arg(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

const STREAM_ARGS: [&str; 9] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--include-partial-messages",
    "--permission-prompt-tool",
    "stdio",
];

pub fn launch_args(o: &LaunchOptions) -> Vec<String> {
    let mut args: Vec<String> = STREAM_ARGS.iter().map(|s| s.to_string()).collect();
    match &o.session {
        SessionRef::New(id) => args.extend(["--session-id".into(), id.clone()]),
        SessionRef::Resume(id) => args.extend(["--resume".into(), id.clone()]),
    }
    args.extend(permission_args(&o.mode));
    if !o.model.trim().is_empty() && o.model != "default" {
        args.extend(["--model".into(), o.model.trim().to_string()]);
    }
    if !o.effort.trim().is_empty() && o.effort != "default" {
        args.extend(["--effort".into(), o.effort.trim().to_string()]);
    }
    if let Some(p) = &o.mcp_config {
        args.extend(["--mcp-config".into(), path_arg(p)]);
    }
    for d in &o.add_dirs {
        args.extend(["--add-dir".into(), d.clone()]);
    }
    if let Some(p) = &o.append_prompt_file {
        args.extend(["--append-system-prompt-file".into(), path_arg(p)]);
    }
    args
}

/// Discovery: only the initialize handshake — no saved session, no hooks,
/// no MCP servers, no prompt, no inference.
pub fn discovery_args() -> Vec<String> {
    let mut args: Vec<String> = STREAM_ARGS.iter().map(|s| s.to_string()).collect();
    args.extend([
        "--no-session-persistence".into(),
        "--strict-mcp-config".into(),
        "--settings".into(),
        r#"{"disableAllHooks":true}"#.into(),
    ]);
    args
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BlockKind {
    Text,
    Thinking,
    Tool,
}

#[derive(Debug, Clone)]
struct Block {
    key: String,
    kind: BlockKind,
    text: String,
    done: bool,
    /// For a tool block: its call id and the input JSON streamed so far.
    call_id: Option<String>,
}

struct Pending {
    kind: RequestKind,
    tool_name: String,
    input: Value,
    suggestions: Option<Value>,
}

/// Tool calls whose completion means a file was written.
fn written_path(name: &str, input: &Value) -> Option<String> {
    let field = match name {
        "Edit" | "Write" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    input.get(field).and_then(Value::as_str).map(str::to_string)
}

fn text_of_tool_result(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| match p.get("type").and_then(Value::as_str) {
                Some("text") => p.get("text").and_then(Value::as_str).map(str::to_string),
                Some("image") => Some("[image]".into()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn str_field<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn i64_field(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

pub struct ClaudeDriver {
    next_request: u64,
    init_request: Option<String>,
    blocks: HashMap<(String, i64), Block>,
    /// Message ids whose whole-block `assistant` frames were seen without
    /// streaming (subagents, older builds): a counter keeps their keys apart.
    unstreamed: HashMap<String, usize>,
    /// Tool name and complete input by call id, for results and file changes.
    tools: HashMap<String, (String, Value)>,
    finished_tools: HashSet<String>,
    pending: HashMap<String, Pending>,
    interrupt_requested: bool,
    last_total_cost: f64,
    main_model: Option<String>,
    context_window: Option<i64>,
    limit_hit: Option<RateWindow>,
}

impl Default for ClaudeDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeDriver {
    pub fn new() -> Self {
        Self {
            next_request: 0,
            init_request: None,
            blocks: HashMap::new(),
            unstreamed: HashMap::new(),
            tools: HashMap::new(),
            finished_tools: HashSet::new(),
            pending: HashMap::new(),
            interrupt_requested: false,
            last_total_cost: 0.0,
            main_model: None,
            context_window: None,
            limit_hit: None,
        }
    }

    fn request_id(&mut self, what: &str) -> String {
        self.next_request += 1;
        format!("qp-{what}-{}", self.next_request)
    }

    fn control_request(&mut self, what: &str, request: Value) -> (String, String) {
        let id = self.request_id(what);
        let line = json!({ "type": "control_request", "request_id": id, "request": request }).to_string();
        (id, line)
    }

    fn control_success(request_id: &str, response: Value) -> String {
        json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request_id, "response": response }
        })
        .to_string()
    }

    fn control_error(request_id: &str, error: &str) -> String {
        json!({
            "type": "control_response",
            "response": { "subtype": "error", "request_id": request_id, "error": error }
        })
        .to_string()
    }

    fn catalog(response: &Value) -> Catalog {
        let models = response
            .get("models")
            .and_then(Value::as_array)
            .map(|ms| {
                ms.iter()
                    .filter_map(|m| {
                        let id = str_field(m, "value")?.to_string();
                        Some(ModelInfo {
                            display_name: str_field(m, "displayName").unwrap_or(&id).to_string(),
                            description: str_field(m, "description").unwrap_or_default().to_string(),
                            resolved_model: str_field(m, "resolvedModel").map(str::to_string),
                            efforts: m
                                .get("supportedEffortLevels")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                                .unwrap_or_default(),
                            id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let commands = response
            .get("commands")
            .and_then(Value::as_array)
            .map(|cs| {
                cs.iter()
                    .filter_map(|c| {
                        Some(CommandInfo {
                            name: str_field(c, "name")?.to_string(),
                            description: str_field(c, "description").unwrap_or_default().to_string(),
                            argument_hint: str_field(c, "argumentHint").unwrap_or_default().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Catalog {
            provider: "claude".into(),
            version: str_field(response, "claude_code_version").map(str::to_string),
            models,
            commands,
        }
    }

    fn on_control_request(&mut self, msg: &Value) -> Vec<Emit> {
        let request_id = str_field(msg, "request_id").unwrap_or_default().to_string();
        let req = msg.get("request").cloned().unwrap_or(Value::Null);
        match str_field(&req, "subtype") {
            Some("can_use_tool") => {
                let tool_name = str_field(&req, "tool_name").unwrap_or_default().to_string();
                let input = req.get("input").cloned().unwrap_or(Value::Null);
                let kind = match tool_name.as_str() {
                    "AskUserQuestion" => RequestKind::Question,
                    "ExitPlanMode" => RequestKind::Plan,
                    _ => RequestKind::Approval,
                };
                let call_id = str_field(&req, "tool_use_id").map(str::to_string);
                self.pending.insert(
                    request_id.clone(),
                    Pending {
                        kind,
                        tool_name: tool_name.clone(),
                        input: input.clone(),
                        suggestions: req.get("permission_suggestions").cloned().filter(|v| !v.is_null()),
                    },
                );
                vec![Emit::Request(Request { request_id, kind, tool_name, call_id, input, parent: None })]
            }
            // Hook and SDK-MCP callbacks are never registered by QuantPilot;
            // answering keeps the CLI from waiting on them.
            other => vec![Emit::Write(Self::control_error(
                &request_id,
                &format!("QuantPilot does not handle '{}'", other.unwrap_or("unknown")),
            ))],
        }
    }

    fn on_stream_event(&mut self, msg: &Value) -> Vec<Emit> {
        let parent = str_field(msg, "parent_tool_use_id").map(str::to_string);
        let ev = msg.get("event").cloned().unwrap_or(Value::Null);
        let msg_id = str_field(msg, "api_message_id")
            .map(str::to_string)
            .or_else(|| ev.get("message").and_then(|m| str_field(m, "id")).map(str::to_string))
            .unwrap_or_default();
        let index = ev.get("index").and_then(Value::as_i64).unwrap_or(0);
        let mut out = Vec::new();
        match str_field(&ev, "type") {
            Some("content_block_start") => {
                let cb = ev.get("content_block").cloned().unwrap_or(Value::Null);
                let key = format!("{msg_id}:{index}");
                match str_field(&cb, "type") {
                    Some("text") => {
                        out.push(Emit::MessageStart { key: key.clone(), phase: None, parent: parent.clone() });
                        let text = str_field(&cb, "text").unwrap_or_default().to_string();
                        if !text.is_empty() {
                            out.push(Emit::TextDelta { key: key.clone(), text: text.clone() });
                        }
                        self.blocks.insert(
                            (msg_id, index),
                            Block { key, kind: BlockKind::Text, text, done: false, call_id: None },
                        );
                    }
                    Some("thinking") | Some("redacted_thinking") => {
                        out.push(Emit::ReasoningStart { key: key.clone(), parent: parent.clone() });
                        self.blocks.insert(
                            (msg_id, index),
                            Block { key, kind: BlockKind::Thinking, text: String::new(), done: false, call_id: None },
                        );
                    }
                    Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use") => {
                        let call_id = str_field(&cb, "id").unwrap_or_default().to_string();
                        let name = str_field(&cb, "name").unwrap_or_default().to_string();
                        out.push(Emit::ToolStart {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            input: json!({}),
                            parent: parent.clone(),
                        });
                        self.tools.insert(call_id.clone(), (name, json!({})));
                        self.blocks.insert(
                            (msg_id, index),
                            Block { key, kind: BlockKind::Tool, text: String::new(), done: false, call_id: Some(call_id) },
                        );
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let delta = ev.get("delta").cloned().unwrap_or(Value::Null);
                let Some(block) = self.blocks.get_mut(&(msg_id, index)) else {
                    return out;
                };
                match str_field(&delta, "type") {
                    Some("text_delta") => {
                        let t = str_field(&delta, "text").unwrap_or_default();
                        if !t.is_empty() {
                            block.text.push_str(t);
                            out.push(Emit::TextDelta { key: block.key.clone(), text: t.to_string() });
                        }
                    }
                    Some("thinking_delta") => {
                        let t = str_field(&delta, "thinking").unwrap_or_default();
                        block.text.push_str(t);
                        let est = delta.get("estimated_tokens").and_then(Value::as_i64);
                        if !t.is_empty() || est.is_some() {
                            out.push(Emit::ReasoningDelta { key: block.key.clone(), text: t.to_string(), estimated_tokens: est });
                        }
                    }
                    Some("input_json_delta") => {
                        block.text.push_str(str_field(&delta, "partial_json").unwrap_or_default());
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                if let Some(block) = self.blocks.get_mut(&(msg_id, index)) {
                    if !block.done {
                        block.done = true;
                        out.extend(Self::finish_block(block, &mut self.tools));
                    }
                }
            }
            Some("message_delta") if parent.is_none() => {
                if let Some(u) = ev.get("usage") {
                    let ctx = i64_field(u, "input_tokens")
                        + i64_field(u, "cache_creation_input_tokens")
                        + i64_field(u, "cache_read_input_tokens")
                        + i64_field(u, "output_tokens");
                    if ctx > 0 {
                        out.push(Emit::Usage {
                            context_tokens: Some(ctx),
                            context_window: self.context_window,
                            cost_usd: None,
                            input_tokens: None,
                            output_tokens: None,
                        });
                    }
                }
            }
            _ => {}
        }
        out
    }

    fn finish_block(block: &Block, tools: &mut HashMap<String, (String, Value)>) -> Vec<Emit> {
        match block.kind {
            BlockKind::Text => vec![Emit::MessageDone { key: block.key.clone(), text: block.text.clone() }],
            BlockKind::Thinking => vec![Emit::ReasoningDone { key: block.key.clone() }],
            BlockKind::Tool => {
                let Some(call_id) = block.call_id.clone() else {
                    return Vec::new();
                };
                let input: Value = serde_json::from_str(&block.text).unwrap_or_else(|_| json!({}));
                if let Some(entry) = tools.get_mut(&call_id) {
                    entry.1 = input.clone();
                }
                let mut out = vec![Emit::ToolInput { call_id, input: input.clone() }];
                if let Some((name, _)) = block.call_id.as_ref().and_then(|c| tools.get(c)) {
                    if name == "TodoWrite" {
                        if let Some(todos) = input.get("todos") {
                            out.push(Emit::Todos { todos: todos.clone() });
                        }
                    }
                }
                out
            }
        }
    }

    fn on_assistant(&mut self, msg: &Value) -> Vec<Emit> {
        let parent = str_field(msg, "parent_tool_use_id").map(str::to_string);
        let message = msg.get("message").cloned().unwrap_or(Value::Null);
        let msg_id = str_field(&message, "id").unwrap_or_default().to_string();
        let mut out = Vec::new();
        let Some(content) = message.get("content").and_then(Value::as_array) else {
            return out;
        };
        for part in content {
            match str_field(part, "type") {
                Some("text") => {
                    let full = str_field(part, "text").unwrap_or_default().to_string();
                    // The streamed block of this message, if there was one.
                    let streamed = self
                        .blocks
                        .iter_mut()
                        .filter(|((m, _), b)| m == &msg_id && b.kind == BlockKind::Text)
                        .max_by_key(|((_, i), _)| *i)
                        .map(|(_, b)| b);
                    match streamed {
                        Some(block) => {
                            if full.len() > block.text.len() && full.starts_with(&block.text) {
                                let suffix = full[block.text.len()..].to_string();
                                block.text = full.clone();
                                out.push(Emit::TextDelta { key: block.key.clone(), text: suffix });
                            }
                            if block.done {
                                out.push(Emit::MessageDone { key: block.key.clone(), text: block.text.clone() });
                            }
                        }
                        None => {
                            if full.trim().is_empty() {
                                continue;
                            }
                            let n = self.unstreamed.entry(msg_id.clone()).or_insert(0);
                            *n += 1;
                            let key = format!("{msg_id}:a{n}");
                            out.push(Emit::MessageStart { key: key.clone(), phase: None, parent: parent.clone() });
                            out.push(Emit::TextDelta { key: key.clone(), text: full.clone() });
                            out.push(Emit::MessageDone { key, text: full });
                        }
                    }
                }
                Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use") => {
                    let call_id = str_field(part, "id").unwrap_or_default().to_string();
                    let name = str_field(part, "name").unwrap_or_default().to_string();
                    let input = part.get("input").cloned().unwrap_or_else(|| json!({}));
                    if !self.tools.contains_key(&call_id) {
                        out.push(Emit::ToolStart {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            parent: parent.clone(),
                        });
                    }
                    self.tools.insert(call_id.clone(), (name.clone(), input.clone()));
                    out.push(Emit::ToolInput { call_id, input: input.clone() });
                    if name == "TodoWrite" {
                        if let Some(todos) = input.get("todos") {
                            out.push(Emit::Todos { todos: todos.clone() });
                        }
                    }
                }
                _ => {}
            }
        }
        // A streamed TodoWrite already reported its list at block stop.
        dedupe_todos(&mut out);
        out
    }

    fn on_user(&mut self, msg: &Value) -> Vec<Emit> {
        let mut out = Vec::new();
        let Some(content) = msg.get("message").and_then(|m| m.get("content")).and_then(Value::as_array) else {
            return out;
        };
        for part in content {
            if str_field(part, "type") != Some("tool_result") {
                continue;
            }
            let call_id = str_field(part, "tool_use_id").unwrap_or_default().to_string();
            if !self.finished_tools.insert(call_id.clone()) {
                continue;
            }
            let is_error = part.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let output = text_of_tool_result(part.get("content").unwrap_or(&Value::Null));
            out.push(Emit::ToolResult { call_id: call_id.clone(), output, is_error });
            if !is_error {
                if let Some((name, input)) = self.tools.get(&call_id) {
                    if let Some(path) = written_path(name, input) {
                        out.push(Emit::FileChanged { path, call_id: call_id.clone() });
                    }
                }
            }
        }
        out
    }

    fn on_rate_limit(&mut self, msg: &Value) -> Vec<Emit> {
        let info = msg.get("rate_limit_info").cloned().unwrap_or(Value::Null);
        let mut windows = Vec::new();
        if let Some(map) = info.get("unifiedWindows").and_then(Value::as_object) {
            for (kind, w) in map {
                windows.push(RateWindow {
                    kind: kind.clone(),
                    used_pct: w.get("utilization").and_then(Value::as_f64).map(|u| (u * 100.0).clamp(0.0, 100.0)),
                    resets_at: w.get("resetsAt").and_then(Value::as_i64).map(|s| s * 1000),
                    status: str_field(w, "status").map(str::to_string),
                });
            }
        } else if let Some(kind) = str_field(&info, "rateLimitType") {
            windows.push(RateWindow {
                kind: kind.to_string(),
                used_pct: None,
                resets_at: info.get("resetsAt").and_then(Value::as_i64).map(|s| s * 1000),
                status: str_field(&info, "status").map(str::to_string),
            });
        }
        if str_field(&info, "status") == Some("rejected") {
            let kind = str_field(&info, "rateLimitType").unwrap_or("limit").to_string();
            let hit = windows.iter().find(|w| w.kind == kind).cloned().unwrap_or(RateWindow {
                kind,
                used_pct: Some(100.0),
                resets_at: info.get("resetsAt").and_then(Value::as_i64).map(|s| s * 1000),
                status: Some("rejected".into()),
            });
            self.limit_hit = Some(hit);
        }
        windows.sort_by(|a, b| a.kind.cmp(&b.kind));
        if windows.is_empty() {
            Vec::new()
        } else {
            vec![Emit::RateLimit { windows }]
        }
    }

    fn on_result(&mut self, msg: &Value) -> Vec<Emit> {
        if let Some(usage) = msg.get("modelUsage").and_then(Value::as_object) {
            let main = self.main_model.as_deref().and_then(|m| usage.get(m));
            let window = main
                .and_then(|u| u.get("contextWindow"))
                .and_then(Value::as_i64)
                .or_else(|| usage.values().filter_map(|u| u.get("contextWindow").and_then(Value::as_i64)).max());
            if window.is_some() {
                self.context_window = window;
            }
        }
        let total = msg.get("total_cost_usd").and_then(Value::as_f64);
        let turn_cost = total.map(|t| {
            let c = (t - self.last_total_cost).max(0.0);
            self.last_total_cost = t;
            c
        });
        let usage = msg.get("usage").cloned().unwrap_or(Value::Null);
        let input_tokens = usage.get("input_tokens").map(|_| {
            i64_field(&usage, "input_tokens")
                + i64_field(&usage, "cache_creation_input_tokens")
                + i64_field(&usage, "cache_read_input_tokens")
        });
        let output_tokens = usage.get("output_tokens").and_then(Value::as_i64);
        let is_error = msg.get("is_error").and_then(Value::as_bool).unwrap_or(false);
        let subtype = str_field(msg, "subtype").unwrap_or("success");
        let text = str_field(msg, "result").unwrap_or_default().to_string();
        let limited = self.limit_hit.is_some()
            || (is_error && {
                let t = text.to_ascii_lowercase();
                t.contains("usage limit") || t.contains("rate limit") || t.contains("limit reached")
            });
        let status = if self.interrupt_requested {
            TurnStatus::Interrupted
        } else if limited && is_error {
            TurnStatus::Limited
        } else if is_error || subtype != "success" {
            TurnStatus::Failed
        } else {
            TurnStatus::Completed
        };
        let error = match status {
            TurnStatus::Failed | TurnStatus::Limited => {
                Some(if text.trim().is_empty() { subtype.replace('_', " ") } else { text })
            }
            _ => None,
        };
        let out = vec![
            Emit::Usage {
                context_tokens: None,
                context_window: self.context_window,
                cost_usd: turn_cost,
                input_tokens,
                output_tokens,
            },
            Emit::TurnEnd {
                status,
                error,
                duration_ms: msg.get("duration_ms").and_then(Value::as_i64),
                cost_usd: turn_cost,
            },
        ];
        // A turn's streaming state is not needed by the next one.
        self.interrupt_requested = false;
        self.limit_hit = None;
        self.blocks.clear();
        self.unstreamed.clear();
        // Pending requests keep their own copy of the input.
        self.tools.clear();
        self.finished_tools.clear();
        out
    }
}

fn dedupe_todos(out: &mut Vec<Emit>) {
    let mut seen = false;
    out.retain(|e| {
        if matches!(e, Emit::Todos { .. }) {
            if seen {
                return false;
            }
            seen = true;
        }
        true
    });
}

impl Driver for ClaudeDriver {
    fn provider(&self) -> &'static str {
        "claude"
    }

    fn tool_info(&self, name: &str, input: &Value) -> ToolInfo {
        let s = |k: &str| str_field(input, k).unwrap_or_default().to_string();
        let (category, title) = match name {
            "Bash" | "PowerShell" | "BashOutput" | "KillShell" => ("command", s("command")),
            "Read" | "NotebookRead" => ("read", s("file_path")),
            "LS" => ("read", s("path")),
            "Glob" => ("search", s("pattern")),
            "Grep" => ("search", s("pattern")),
            "Edit" | "MultiEdit" | "Write" => ("edit", s("file_path")),
            "NotebookEdit" => ("edit", s("notebook_path")),
            "WebFetch" => ("web", s("url")),
            "WebSearch" => ("web", s("query")),
            "Task" | "Agent" => ("agent", {
                let d = s("description");
                if d.is_empty() { s("subagent_type") } else { d }
            }),
            "TodoWrite" => ("todo", "Updated the task list".into()),
            n if n.starts_with("mcp__") => ("mcp", n.trim_start_matches("mcp__").replacen("__", " · ", 1)),
            _ => ("tool", String::new()),
        };
        let title = if title.is_empty() { name.to_string() } else { title };
        ToolInfo { category, title: title.lines().next().unwrap_or_default().to_string() }
    }

    fn handshake(&mut self) -> Vec<String> {
        let (id, line) = self.control_request("init", json!({ "subtype": "initialize", "hooks": null }));
        self.init_request = Some(id);
        vec![line]
    }

    fn user_message(&mut self, input: &TurnInput) -> Vec<String> {
        let content = if input.images.is_empty() {
            Value::String(input.text.clone())
        } else {
            let mut parts: Vec<Value> = input
                .images
                .iter()
                .map(|ImageInput { media_type, base64 }| {
                    json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": base64 } })
                })
                .collect();
            parts.push(json!({ "type": "text", "text": input.text }));
            Value::Array(parts)
        };
        vec![json!({
            "type": "user",
            "message": { "role": "user", "content": content },
            "parent_tool_use_id": null,
            "session_id": ""
        })
        .to_string()]
    }

    fn on_line(&mut self, line: &str) -> Vec<Emit> {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match str_field(&msg, "type") {
            Some("control_response") => {
                let resp = msg.get("response").cloned().unwrap_or(Value::Null);
                let rid = str_field(&resp, "request_id").unwrap_or_default();
                if Some(rid) == self.init_request.as_deref() {
                    if str_field(&resp, "subtype") == Some("success") {
                        let inner = resp.get("response").cloned().unwrap_or(Value::Null);
                        return vec![Emit::Catalog(Self::catalog(&inner))];
                    }
                    let err = str_field(&resp, "error").unwrap_or("initialize failed").to_string();
                    return vec![Emit::Notice { text: format!("Claude Code: {err}") }];
                }
                Vec::new()
            }
            Some("control_request") => self.on_control_request(&msg),
            Some("system") => match str_field(&msg, "subtype") {
                Some("init") => {
                    let model = str_field(&msg, "model").map(str::to_string);
                    self.main_model = model.clone();
                    match str_field(&msg, "session_id") {
                        Some(sid) => vec![Emit::Bound { session_id: sid.to_string(), model }],
                        None => Vec::new(),
                    }
                }
                Some("compact_boundary") => vec![Emit::Notice { text: "Conversation compacted".into() }],
                _ => Vec::new(),
            },
            Some("stream_event") => self.on_stream_event(&msg),
            Some("assistant") => self.on_assistant(&msg),
            Some("user") => self.on_user(&msg),
            Some("rate_limit_event") => self.on_rate_limit(&msg),
            Some("result") => self.on_result(&msg),
            _ => Vec::new(),
        }
    }

    fn answer(&mut self, request_id: &str, answer: &Answer) -> Result<Vec<String>, String> {
        let pending = self.pending.remove(request_id).ok_or("this request is no longer waiting")?;
        let deny = |message: &str| json!({ "behavior": "deny", "message": message });
        let response = match (pending.kind, answer) {
            (_, Answer::Deny { message }) => {
                let m = message.as_deref().map(str::trim).filter(|m| !m.is_empty());
                deny(m.unwrap_or("The user declined this."))
            }
            (RequestKind::Question, Answer::Answers { answers }) => {
                let mut input = pending.input.clone();
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("answers".into(), Value::Object(answers.clone()));
                }
                json!({ "behavior": "allow", "updatedInput": input })
            }
            (RequestKind::Question, Answer::Dismiss) => {
                deny("The user dismissed the question without answering. Continue with your best judgment or ask in your reply.")
            }
            (RequestKind::Question, _) => {
                self.pending.insert(request_id.to_string(), pending);
                return Err("a question needs answers or a dismissal".into());
            }
            (_, Answer::Allow) => json!({ "behavior": "allow", "updatedInput": pending.input }),
            (_, Answer::AllowSession) => {
                let updates = pending.suggestions.clone().unwrap_or_else(|| {
                    json!([{
                        "type": "addRules",
                        "rules": [{ "toolName": pending.tool_name }],
                        "behavior": "allow",
                        "destination": "session"
                    }])
                });
                json!({ "behavior": "allow", "updatedInput": pending.input, "updatedPermissions": updates })
            }
            (_, Answer::Answers { .. }) | (_, Answer::Dismiss) => {
                self.pending.insert(request_id.to_string(), pending);
                return Err("this request takes allow or deny".into());
            }
        };
        Ok(vec![Self::control_success(request_id, response)])
    }

    fn interrupt(&mut self) -> Vec<String> {
        self.interrupt_requested = true;
        let (_, line) = self.control_request("interrupt", json!({ "subtype": "interrupt" }));
        vec![line]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(name: &str) -> (ClaudeDriver, Vec<Emit>) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        let text = std::fs::read_to_string(path).unwrap();
        let mut d = ClaudeDriver::new();
        d.handshake();
        // The fixtures answer the probe's `req_init`; point the driver at it.
        d.init_request = Some("req_init".into());
        let mut out = Vec::new();
        for line in text.lines() {
            out.extend(d.on_line(line));
        }
        (d, out)
    }

    fn texts(out: &[Emit]) -> HashMap<String, String> {
        let mut m: HashMap<String, String> = HashMap::new();
        for e in out {
            if let Emit::TextDelta { key, text } = e {
                m.entry(key.clone()).or_default().push_str(text);
            }
        }
        m
    }

    #[test]
    fn tool_turn_streams_text_once_and_joins_the_tool() {
        let (_, out) = replay("claude_tool_turn.jsonl");
        let catalog = out.iter().find_map(|e| if let Emit::Catalog(c) = e { Some(c) } else { None }).unwrap();
        assert_eq!(catalog.version.as_deref(), Some("2.1.312"));
        assert_eq!(catalog.models[0].id, "default");
        assert!(catalog.models[0].efforts.contains(&"max".to_string()));
        assert_eq!(catalog.commands[0].name, "compact");

        assert!(out.iter().any(|e| matches!(e, Emit::Bound { session_id, .. } if session_id == "5156b244-fca0-4214-9a7a-5a1bd39a331b")));

        let t = texts(&out);
        let answer: Vec<&String> = t.values().collect();
        assert_eq!(answer.len(), 1, "one text block");
        assert_eq!(answer[0], "Remembered: **KIWI-42**.\n\n`echo hello` printed: `hello`");
        let done: Vec<&String> =
            out.iter().filter_map(|e| if let Emit::MessageDone { text, .. } = e { Some(text) } else { None }).collect();
        assert!(done.iter().all(|d| *d == answer[0]));

        let start = out.iter().find_map(|e| if let Emit::ToolStart { call_id, name, .. } = e { Some((call_id, name)) } else { None });
        let (call_id, name) = start.unwrap();
        assert_eq!(name, "Bash");
        let input = out
            .iter()
            .find_map(|e| if let Emit::ToolInput { call_id: c, input } = e { (c == call_id).then_some(input) } else { None })
            .unwrap();
        assert_eq!(input["command"], "echo hello");
        assert!(out.iter().any(|e| matches!(e, Emit::ToolResult { output, is_error: false, .. } if output == "hello")));

        let end = out.iter().find_map(|e| if let Emit::TurnEnd { status, cost_usd, .. } = e { Some((status, cost_usd)) } else { None });
        let (status, cost) = end.unwrap();
        assert_eq!(*status, TurnStatus::Completed);
        assert!((cost.unwrap() - 0.00899421).abs() < 1e-9);
        assert!(out.iter().any(|e| matches!(e, Emit::Usage { context_window: Some(1_000_000), .. })));
        assert!(out.iter().any(|e| matches!(e, Emit::RateLimit { windows } if windows.len() == 2)));
    }

    #[test]
    fn permission_request_round_trips() {
        let (mut d, out) = replay("claude_permission_turn.jsonl");
        let req = out
            .iter()
            .find_map(|e| if let Emit::Request(r) = e { Some(r.clone()) } else { None })
            .expect("a can_use_tool request");
        assert_eq!(req.kind, RequestKind::Approval);
        assert_eq!(req.tool_name, "Write");
        assert_eq!(req.input["file_path"], "C:\\repo\\third.txt");
        assert!(req.call_id.is_some());

        // Replayed from a finished log the request is still pending here.
        let lines = d.answer(&req.request_id, &Answer::Deny { message: Some("not now".into()) }).unwrap();
        let v: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["type"], "control_response");
        assert_eq!(v["response"]["request_id"], req.request_id.as_str());
        assert_eq!(v["response"]["response"]["behavior"], "deny");
        assert_eq!(v["response"]["response"]["message"], "not now");
        assert!(d.answer(&req.request_id, &Answer::Allow).is_err(), "answered once");
    }

    #[test]
    fn allow_and_allow_session_carry_the_input() {
        let mut d = ClaudeDriver::new();
        let req = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"},"tool_use_id":"t1"}}"#;
        d.on_line(req);
        let v: Value = serde_json::from_str(&d.answer("r1", &Answer::Allow).unwrap()[0]).unwrap();
        assert_eq!(v["response"]["response"]["behavior"], "allow");
        assert_eq!(v["response"]["response"]["updatedInput"]["command"], "ls");

        d.on_line(&req.replace("r1", "r2"));
        let v: Value = serde_json::from_str(&d.answer("r2", &Answer::AllowSession).unwrap()[0]).unwrap();
        let rules = &v["response"]["response"]["updatedPermissions"][0];
        assert_eq!(rules["destination"], "session");
        assert_eq!(rules["rules"][0]["toolName"], "Bash");
    }

    #[test]
    fn questions_and_plans_are_their_own_requests() {
        let mut d = ClaudeDriver::new();
        let q = r#"{"type":"control_request","request_id":"q1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"Which?","header":"Pick","options":[{"label":"A"},{"label":"B"}],"multiSelect":false}]}}}"#;
        let out = d.on_line(q);
        assert!(matches!(&out[0], Emit::Request(r) if r.kind == RequestKind::Question));
        assert!(d.answer("q1", &Answer::Allow).is_err(), "a question is not a yes/no");
        let mut answers = serde_json::Map::new();
        answers.insert("Which?".into(), json!("B"));
        let v: Value = serde_json::from_str(&d.answer("q1", &Answer::Answers { answers }).unwrap()[0]).unwrap();
        assert_eq!(v["response"]["response"]["updatedInput"]["answers"]["Which?"], "B");

        let p = r#"{"type":"control_request","request_id":"p1","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","input":{"plan":"1. do it"}}}"#;
        let out = d.on_line(p);
        assert!(matches!(&out[0], Emit::Request(r) if r.kind == RequestKind::Plan));
        let v: Value = serde_json::from_str(&d.answer("p1", &Answer::Deny { message: Some("keep planning".into()) }).unwrap()[0]).unwrap();
        assert_eq!(v["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn unknown_control_requests_get_an_error_reply() {
        let mut d = ClaudeDriver::new();
        let out = d.on_line(r#"{"type":"control_request","request_id":"h1","request":{"subtype":"hook_callback"}}"#);
        let Emit::Write(line) = &out[0] else { panic!("expected a write") };
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["response"]["subtype"], "error");
        assert_eq!(v["response"]["request_id"], "h1");
    }

    #[test]
    fn interrupt_marks_the_turn_interrupted() {
        let mut d = ClaudeDriver::new();
        let lines = d.interrupt();
        let v: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["request"]["subtype"], "interrupt");
        let out = d.on_line(r#"{"type":"result","subtype":"error_during_execution","is_error":true,"total_cost_usd":0.01,"duration_ms":5}"#);
        assert!(out.iter().any(|e| matches!(e, Emit::TurnEnd { status: TurnStatus::Interrupted, .. })));
        let out = d.on_line(r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.03}"#);
        let cost = out.iter().find_map(|e| if let Emit::TurnEnd { cost_usd, .. } = e { *cost_usd } else { None });
        assert!((cost.unwrap() - 0.02).abs() < 1e-9, "cost per turn, not the process total");
    }

    #[test]
    fn unstreamed_assistant_text_and_subagent_parents() {
        let mut d = ClaudeDriver::new();
        let out = d.on_line(r#"{"type":"assistant","parent_tool_use_id":"task1","message":{"id":"m9","content":[{"type":"text","text":"child says hi"},{"type":"tool_use","id":"t9","name":"Read","input":{"file_path":"a.rs"}}]}}"#);
        assert!(matches!(&out[0], Emit::MessageStart { parent: Some(p), .. } if p == "task1"));
        assert!(out.iter().any(|e| matches!(e, Emit::MessageDone { text, .. } if text == "child says hi")));
        assert!(out.iter().any(|e| matches!(e, Emit::ToolStart { parent: Some(p), name, .. } if p == "task1" && name == "Read")));
        let out = d.on_line(r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t9","content":[{"type":"text","text":"fn main"}]}]}}"#);
        assert!(matches!(&out[0], Emit::ToolResult { output, .. } if output == "fn main"));
    }

    #[test]
    fn edits_report_the_written_file() {
        let mut d = ClaudeDriver::new();
        d.on_line(r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"e1","name":"Edit","input":{"file_path":"src/a.rs","old_string":"a","new_string":"b"}}]}}"#);
        let out = d.on_line(r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e1","content":"ok"}]}}"#);
        assert!(out.iter().any(|e| matches!(e, Emit::FileChanged { path, .. } if path == "src/a.rs")));
        let again = d.on_line(r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e1","content":"ok"}]}}"#);
        assert!(again.is_empty(), "a replayed result is not a second change");
    }

    #[test]
    fn launch_args_carry_session_mode_and_files() {
        let o = LaunchOptions {
            session: SessionRef::New("11111111-2222-3333-4444-555555555555".into()),
            model: "opus".into(),
            effort: "high".into(),
            mode: "full".into(),
            mcp_config: Some(PathBuf::from("C:\\d\\mcp.json")),
            append_prompt_file: Some(PathBuf::from("C:\\d\\context.txt")),
            add_dirs: vec!["C:\\vault".into()],
        };
        let j = launch_args(&o).join(" ");
        assert!(j.starts_with("-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-prompt-tool stdio"));
        assert!(j.contains("--session-id 11111111-2222-3333-4444-555555555555"));
        assert!(j.contains("--permission-mode bypassPermissions --dangerously-skip-permissions"));
        assert!(j.contains("--model opus --effort high"));
        assert!(j.contains("--mcp-config C:\\d\\mcp.json"));
        assert!(j.contains("--add-dir C:\\vault"));
        assert!(j.contains("--append-system-prompt-file C:\\d\\context.txt"));

        let r = LaunchOptions { session: SessionRef::Resume("abc".into()), model: "default".into(), effort: String::new(), mode: "ask".into(), ..o };
        let j = launch_args(&r).join(" ");
        assert!(j.contains("--resume abc"));
        assert!(j.contains("--permission-mode default"));
        assert!(!j.contains("--model") && !j.contains("--effort"));
        assert_eq!(permission_args("edits"), vec!["--permission-mode", "acceptEdits"]);
        assert_eq!(permission_args("plan"), vec!["--permission-mode", "plan"]);
    }

    #[test]
    fn tool_info_titles() {
        let d = ClaudeDriver::new();
        assert_eq!(d.tool_info("Bash", &json!({"command":"cargo test\nmore"})).title, "cargo test");
        assert_eq!(d.tool_info("Edit", &json!({"file_path":"a.rs"})).category, "edit");
        assert_eq!(d.tool_info("mcp__quantsuite__list_kanban_cards", &json!({})).title, "quantsuite · list_kanban_cards");
        assert_eq!(d.tool_info("Mystery", &json!({})).title, "Mystery");
    }
}
