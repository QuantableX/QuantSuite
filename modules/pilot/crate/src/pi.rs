//! pi and omp in RPC mode (card 20; pi's docs/rpc.md): `pi --mode rpc`,
//! JSON lines both ways. A command carries an `id` its `response` repeats;
//! the run streams as events (`message_update` with text, thinking and tool
//! call deltas, `tool_execution_*`, `agent_end`). A conversation is a
//! session file: launched again with `--session <file>` it continues.
//!
//! Permission modes come from `pi_modes.ts`, loaded with `-e`: it stops tool
//! calls the mode does not allow and asks the rest through the RPC extension
//! UI — a `select` titled `QP_APPROVAL {json}` becomes an approval here.
//! `/qp-mode <mode>` switches it while the session runs. Pi has no automatic
//! reviewer: Auto fails the turn rather than running under another policy.

use crate::driver::{Answer, Catalog, CommandInfo, Driver, Emit, ImageInput, ModelInfo, Request, RequestKind, ToolInfo, TurnInput, TurnStatus};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The mode extension, written next to the thread's other files at launch.
pub const MODES_EXTENSION: &str = include_str!("pi_modes.ts");

/// The modes `pi_modes.ts` knows.
pub fn supports(mode: &str) -> bool {
    matches!(mode, "full" | "edits" | "ask" | "plan")
}

#[derive(Debug, Clone, Default)]
pub struct PiOptions {
    /// `pi` or `omp`.
    pub provider: &'static str,
    pub label: String,
    pub mode: String,
    /// `provider/id` as the picker shows it.
    pub model: String,
}

/// pi's arguments: RPC mode, the mode extension, the session to continue, the model.
pub fn launch_args(extension: &str, session_file: Option<&str>, model: &str) -> Vec<String> {
    let mut args = vec!["--mode".to_string(), "rpc".to_string(), "-e".to_string(), extension.to_string()];
    if let Some(f) = session_file.filter(|f| !f.trim().is_empty()) {
        args.extend(["--session".to_string(), f.to_string()]);
    }
    if !model.trim().is_empty() {
        args.extend(["--model".to_string(), model.trim().to_string()]);
    }
    args
}

#[derive(Debug, Clone)]
struct Ask {
    ui_id: String,
    /// A QuantPilot approval (Allow / Allow for this session / Deny), else
    /// someone's extension asking (`select` options or `confirm`).
    approval: bool,
    confirm: bool,
}

pub struct PiDriver {
    opts: PiOptions,
    next_id: u64,
    /// Our commands by id: `prompt`, `get_state`, …
    calls: HashMap<String, &'static str>,
    models: Vec<ModelInfo>,
    commands: Vec<CommandInfo>,
    /// The mode pi_modes.ts runs with.
    applied_mode: String,
    running: bool,
    cancelled: bool,
    /// Text and thinking blocks open in the current assistant message, by
    /// content index: key, thinking, the text so far.
    blocks: HashMap<u64, (String, bool, String)>,
    msg_seq: u64,
    asks: HashMap<String, Ask>,
    next_ask: u64,
    usage: Option<Value>,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| s(p, "text")).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn images(list: &[ImageInput]) -> Value {
    Value::Array(list.iter().map(|i| json!({ "type": "image", "data": i.base64, "mimeType": i.media_type })).collect())
}

impl PiDriver {
    pub fn new(opts: PiOptions) -> Self {
        Self {
            applied_mode: opts.mode.clone(),
            opts,
            next_id: 0,
            calls: HashMap::new(),
            models: Vec::new(),
            commands: Vec::new(),
            running: false,
            cancelled: false,
            blocks: HashMap::new(),
            msg_seq: 0,
            asks: HashMap::new(),
            next_ask: 0,
            usage: None,
        }
    }

    fn command(&mut self, kind: &'static str, mut body: Value) -> String {
        self.next_id += 1;
        let id = format!("qp-{}", self.next_id);
        self.calls.insert(id.clone(), kind);
        body["id"] = json!(id);
        body["type"] = json!(kind);
        body.to_string()
    }

    fn catalog(&self) -> Emit {
        Emit::Catalog(Catalog { provider: self.opts.provider.into(), version: None, models: self.models.clone(), commands: self.commands.clone(), skills: Vec::new() })
    }

    fn end(&mut self, out: &mut Vec<Emit>, status: TurnStatus, error: Option<String>) {
        if !self.running {
            return;
        }
        self.close_blocks(out);
        self.running = false;
        if let Some(u) = self.usage.take() {
            let n = |k: &str| u.get(k).and_then(Value::as_i64);
            let context = ["input", "cacheRead", "cacheWrite"].iter().filter_map(|k| n(k)).sum::<i64>() + n("output").unwrap_or(0);
            out.push(Emit::Usage {
                context_tokens: (context > 0).then_some(context),
                context_window: None,
                cost_usd: u["cost"]["total"].as_f64(),
                input_tokens: n("input"),
                output_tokens: n("output"),
            });
        }
        out.push(Emit::TurnEnd { status, error, duration_ms: None, cost_usd: None });
    }

    fn close_blocks(&mut self, out: &mut Vec<Emit>) {
        let mut open: Vec<(u64, (String, bool, String))> = self.blocks.drain().collect();
        open.sort_by_key(|(i, _)| *i);
        for (_, (key, thinking, text)) in open {
            out.push(if thinking { Emit::ReasoningDone { key } } else { Emit::MessageDone { key, text } });
        }
    }

    fn on_response(&mut self, msg: &Value) -> Vec<Emit> {
        let mut out = Vec::new();
        // omp does not repeat the id: the command name says what answered.
        let kind = s(msg, "id").and_then(|id| self.calls.remove(id)).or_else(|| match s(msg, "command") {
            Some("prompt") => Some("prompt"),
            Some("get_state") => Some("get_state"),
            Some("get_available_models") => Some("get_available_models"),
            Some("get_commands") => Some("get_commands"),
            Some("set_model") => Some("set_model"),
            _ => None,
        });
        let ok = msg["success"].as_bool().unwrap_or(true);
        let data = msg.get("data").cloned().unwrap_or(Value::Null);
        match kind {
            Some("prompt") if !ok => {
                self.running = true;
                let e = s(msg, "error").unwrap_or("the prompt was not accepted").to_string();
                self.end(&mut out, TurnStatus::Failed, Some(format!("{}: {e}", self.opts.label)));
            }
            Some("get_state") => {
                // The session file is what `--session` reopens.
                let sid = s(&data, "sessionFile").or_else(|| s(&data, "sessionId")).map(str::to_string);
                if let Some(sid) = sid {
                    let model = data["model"].as_object().map(|m| format!("{}/{}", m.get("provider").and_then(Value::as_str).unwrap_or_default(), m.get("id").and_then(Value::as_str).unwrap_or_default())).or_else(|| s(&data, "model").map(str::to_string));
                    out.push(Emit::Bound { session_id: sid, model });
                }
            }
            Some("get_available_models") => {
                self.models = data["models"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| {
                                let id = format!("{}/{}", s(m, "provider")?, s(m, "id")?);
                                Some(ModelInfo { display_name: s(m, "name").map(str::to_string).unwrap_or_else(|| id.clone()), id, ..Default::default() })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(self.catalog());
            }
            // omp has no get_commands; it announces them (available_commands_update).
            Some("get_commands") if !ok => {}
            Some("get_commands") => {
                self.commands = data["commands"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|c| Some(CommandInfo { name: s(c, "name")?.to_string(), description: s(c, "description").unwrap_or_default().to_string(), argument_hint: String::new() })).filter(|c| c.name != "qp-mode").collect())
                    .unwrap_or_default();
                out.push(self.catalog());
            }
            Some("set_model") if !ok => out.push(Emit::Notice { text: format!("{} kept its model: {}", self.opts.label, s(msg, "error").unwrap_or("refused")) }),
            _ => {}
        }
        out
    }

    fn on_message_update(&mut self, ev: &Value) -> Vec<Emit> {
        let mut out = Vec::new();
        let i = ev["contentIndex"].as_u64().unwrap_or(0);
        match s(ev, "type") {
            Some("text_start") => {
                self.msg_seq += 1;
                let key = format!("pi-{}-{i}", self.msg_seq);
                out.push(Emit::MessageStart { key: key.clone(), phase: None, parent: None });
                self.blocks.insert(i, (key, false, String::new()));
            }
            Some("text_delta") => {
                if let Some((key, false, text)) = self.blocks.get_mut(&i) {
                    let delta = s(ev, "delta").unwrap_or_default();
                    text.push_str(delta);
                    out.push(Emit::TextDelta { key: key.clone(), text: delta.to_string() });
                }
            }
            Some("text_end") => {
                if let Some((key, _, text)) = self.blocks.remove(&i) {
                    out.push(Emit::MessageDone { key, text: s(ev, "content").map(str::to_string).unwrap_or(text) });
                }
            }
            Some("thinking_start") => {
                self.msg_seq += 1;
                let key = format!("pi-think-{}-{i}", self.msg_seq);
                out.push(Emit::ReasoningStart { key: key.clone(), parent: None });
                self.blocks.insert(i, (key, true, String::new()));
            }
            Some("thinking_delta") => {
                if let Some((key, true, _)) = self.blocks.get(&i) {
                    out.push(Emit::ReasoningDelta { key: key.clone(), text: s(ev, "delta").unwrap_or_default().to_string(), estimated_tokens: None });
                }
            }
            Some("thinking_end") => {
                if let Some((key, ..)) = self.blocks.remove(&i) {
                    out.push(Emit::ReasoningDone { key });
                }
            }
            _ => {}
        }
        out
    }

    fn on_ui(&mut self, msg: &Value) -> Vec<Emit> {
        let Some(ui_id) = s(msg, "id").map(str::to_string) else { return Vec::new() };
        let method = s(msg, "method").unwrap_or_default();
        let title = s(msg, "title").unwrap_or_default();
        match method {
            "select" | "confirm" => {
                self.next_ask += 1;
                let request_id = format!("pi-{}", self.next_ask);
                if let Some(json) = title.strip_prefix("QP_APPROVAL ") {
                    let info: Value = serde_json::from_str(json).unwrap_or(Value::Null);
                    self.asks.insert(request_id.clone(), Ask { ui_id, approval: true, confirm: false });
                    return vec![Emit::Request(Request {
                        request_id,
                        kind: RequestKind::Approval,
                        tool_name: s(&info, "tool").unwrap_or("tool").to_string(),
                        call_id: s(&info, "id").map(str::to_string),
                        input: info.get("input").cloned().unwrap_or(Value::Null),
                        parent: None,
                        allow_session: true,
                        by_message: false,
                    })];
                }
                // Another extension asks: a question with its options (yes/no for a confirm).
                let confirm = method == "confirm";
                let options: Vec<Value> = if confirm {
                    vec![json!({ "label": "Yes" }), json!({ "label": "No" })]
                } else {
                    msg["options"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(|o| json!({ "label": o })).collect()).unwrap_or_default()
                };
                let question = if confirm { format!("{title}\n{}", s(msg, "message").unwrap_or_default()).trim().to_string() } else { title.to_string() };
                self.asks.insert(request_id.clone(), Ask { ui_id, approval: false, confirm });
                vec![Emit::Request(Request {
                    request_id,
                    kind: RequestKind::Question,
                    tool_name: "question".into(),
                    call_id: None,
                    input: json!({ "questions": [{ "question": question, "header": self.opts.label, "options": options, "multiSelect": false }] }),
                    parent: None,
                    allow_session: false,
                    by_message: false,
                })]
            }
            // A free-text dialog QuantPilot does not show: dismissed so the extension goes on.
            "input" | "editor" => vec![Emit::Write(json!({ "type": "extension_ui_response", "id": ui_id, "cancelled": true }).to_string())],
            "notify" => s(msg, "message").map(|m| vec![Emit::Notice { text: m.to_string() }]).unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

impl Driver for PiDriver {
    fn provider(&self) -> &'static str {
        self.opts.provider
    }

    fn tool_info(&self, name: &str, input: &Value) -> ToolInfo {
        let arg = |k: &str| s(input, k).unwrap_or_default().to_string();
        let (category, title) = match name {
            "bash" => ("command", arg("command")),
            "read" | "ls" => ("read", if arg("path").is_empty() { arg("file_path") } else { arg("path") }),
            "write" | "edit" | "multi_edit" | "apply_patch" => ("edit", if arg("path").is_empty() { arg("file_path") } else { arg("path") }),
            "grep" | "find" | "glob" | "search" => ("search", arg("pattern")),
            "web_fetch" | "fetch" | "web_search" => ("web", if arg("url").is_empty() { arg("query") } else { arg("url") }),
            n if n.starts_with("mcp") => ("mcp", n.to_string()),
            _ => ("tool", String::new()),
        };
        let title = if title.is_empty() { name.to_string() } else { title };
        ToolInfo { category, title: title.lines().next().unwrap_or_default().to_string() }
    }

    fn handshake(&mut self) -> Vec<String> {
        vec![
            self.command("get_state", json!({})),
            self.command("get_available_models", json!({})),
            self.command("get_commands", json!({})),
        ]
    }

    fn user_message(&mut self, input: &TurnInput) -> Vec<String> {
        let mut lines = Vec::new();
        if self.running {
            // Into the running turn: pi delivers it before its next model call.
            lines.push(self.command("prompt", json!({ "message": input.text, "images": images(&input.images), "streamingBehavior": "steer" })));
            return lines;
        }
        if self.opts.mode != self.applied_mode {
            let mode = self.opts.mode.clone();
            lines.push(self.command("prompt", json!({ "message": format!("/qp-mode {mode}") })));
            self.applied_mode = mode;
        }
        self.running = true;
        self.cancelled = false;
        self.usage = None;
        lines.push(self.command("prompt", json!({ "message": input.text, "images": images(&input.images) })));
        lines
    }

    fn on_line(&mut self, line: &str) -> Vec<Emit> {
        let Ok(msg) = serde_json::from_str::<Value>(line.trim_end_matches('\r')) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match s(&msg, "type") {
            Some("response") => out = self.on_response(&msg),
            Some("message_update") => {
                if let Some(u) = msg.get("usage").filter(|u| u.is_object()) {
                    self.usage = Some(u.clone());
                }
                out = self.on_message_update(&msg["assistantMessageEvent"]);
            }
            Some("message_end") => {
                let m = &msg["message"];
                if s(m, "role") == Some("assistant") {
                    if let Some(u) = m.get("usage").filter(|u| u.is_object()) {
                        self.usage = Some(u.clone());
                    }
                    self.close_blocks(&mut out);
                    match s(m, "stopReason") {
                        Some("error") => {
                            let e = s(m, "errorMessage").unwrap_or("the model call failed").to_string();
                            self.end(&mut out, TurnStatus::Failed, Some(e));
                        }
                        Some("aborted") => self.end(&mut out, TurnStatus::Interrupted, None),
                        _ => {}
                    }
                }
            }
            Some("tool_execution_start") => {
                let id = s(&msg, "toolCallId").unwrap_or_default().to_string();
                out.push(Emit::ToolStart { call_id: id, name: s(&msg, "toolName").unwrap_or("tool").to_string(), input: msg.get("args").cloned().unwrap_or(Value::Null), parent: None });
            }
            Some("tool_execution_end") => {
                let id = s(&msg, "toolCallId").unwrap_or_default().to_string();
                let is_error = msg["isError"].as_bool().unwrap_or(false);
                out.push(Emit::ToolResult { call_id: id.clone(), output: text_of(&msg["result"]["content"]), is_error });
                let tool = s(&msg, "toolName").unwrap_or_default();
                if !is_error && matches!(tool, "write" | "edit" | "multi_edit") {
                    let args = &msg["args"];
                    if let Some(path) = s(args, "path").or_else(|| s(args, "file_path")) {
                        out.push(Emit::FileChanged { path: path.to_string(), call_id: id });
                    }
                }
            }
            Some("agent_end") => {
                if msg["willRetry"].as_bool() != Some(true) {
                    let status = if self.cancelled { TurnStatus::Interrupted } else { TurnStatus::Completed };
                    self.end(&mut out, status, None);
                }
            }
            Some("auto_retry_start") => out.push(Emit::Notice { text: format!("{} retries after an error.", self.opts.label) }),
            Some("compaction_start") => out.push(Emit::Notice { text: format!("{} is compacting the conversation.", self.opts.label) }),
            Some("extension_ui_request") => out = self.on_ui(&msg),
            Some("extension_error") => out.push(Emit::Notice { text: format!("An extension failed: {}", s(&msg, "error").unwrap_or("unknown error")) }),
            // omp announces commands by itself.
            Some("available_commands_update") => {
                self.commands = msg["commands"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| Some(CommandInfo { name: s(c, "name")?.to_string(), description: s(c, "description").unwrap_or_default().to_string(), argument_hint: c["input"]["hint"].as_str().unwrap_or_default().to_string() }))
                            .filter(|c| c.name != "qp-mode")
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(self.catalog());
            }
            _ => {}
        }
        out
    }

    fn answer(&mut self, request_id: &str, answer: &Answer) -> Result<Vec<String>, String> {
        let ask = self.asks.remove(request_id).ok_or("this request is no longer waiting")?;
        let reply = |body: Value| {
            let mut b = body;
            b["type"] = json!("extension_ui_response");
            b["id"] = json!(ask.ui_id);
            b.to_string()
        };
        let line = match answer {
            Answer::Allow if ask.approval => reply(json!({ "value": "Allow" })),
            Answer::AllowSession if ask.approval => reply(json!({ "value": "Allow for this session" })),
            Answer::Deny { .. } if ask.approval => reply(json!({ "value": "Deny" })),
            Answer::Answers { answers } => {
                let picked = answers.values().next().map(|v| match v {
                    Value::Array(a) => a.first().and_then(Value::as_str).unwrap_or_default().to_string(),
                    v => v.as_str().unwrap_or_default().to_string(),
                });
                match (ask.confirm, picked) {
                    (true, Some(p)) => reply(json!({ "confirmed": p == "Yes" })),
                    (false, Some(p)) => reply(json!({ "value": p })),
                    (_, None) => reply(json!({ "cancelled": true })),
                }
            }
            _ => reply(json!({ "cancelled": true })),
        };
        Ok(vec![line])
    }

    fn interrupt(&mut self) -> Vec<String> {
        if !self.running {
            return Vec::new();
        }
        self.cancelled = true;
        let mut lines: Vec<String> = self.asks.drain().map(|(_, a)| json!({ "type": "extension_ui_response", "id": a.ui_id, "cancelled": true }).to_string()).collect();
        lines.push(self.command("abort", json!({})));
        lines
    }

    fn set_mode(&mut self, mode: &str) -> Option<String> {
        if !supports(mode) {
            return None;
        }
        self.opts.mode = mode.to_string();
        if self.running {
            // Applied before the next prompt.
            return Some(json!({ "type": "get_state", "id": "qp-mode-later" }).to_string());
        }
        self.applied_mode = mode.to_string();
        Some(self.command("prompt", json!({ "message": format!("/qp-mode {mode}") })))
    }

    fn set_model(&mut self, model: &str) -> Option<String> {
        let (provider, id) = model.split_once('/')?;
        self.opts.model = model.to_string();
        Some(self.command("set_model", json!({ "provider": provider, "modelId": id })))
    }

    fn can_steer(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }
    fn feed(d: &mut PiDriver, lines: &[Value]) -> Vec<Emit> {
        lines.iter().flat_map(|l| d.on_line(&l.to_string())).collect()
    }
    fn driver(mode: &str) -> PiDriver {
        let mut d = PiDriver::new(PiOptions { provider: "pi", label: "pi".into(), mode: mode.into(), model: String::new() });
        let hs: Vec<Value> = d.handshake().iter().map(|l| parse(l)).collect();
        assert_eq!(hs.iter().map(|h| h["type"].as_str().unwrap()).collect::<Vec<_>>(), ["get_state", "get_available_models", "get_commands"]);
        let out = feed(&mut d, &[
            json!({ "type": "ready", "protocolVersion": 1 }),
            json!({ "type": "response", "id": hs[0]["id"], "command": "get_state", "success": true, "data": { "sessionFile": "C:\\s\\a.jsonl", "sessionId": "a", "model": { "provider": "anthropic", "id": "claude-x" } } }),
            json!({ "type": "response", "id": hs[1]["id"], "command": "get_available_models", "success": true, "data": { "models": [{ "provider": "anthropic", "id": "claude-x", "name": "Claude X" }] } }),
            json!({ "type": "response", "id": hs[2]["id"], "command": "get_commands", "success": true, "data": { "commands": [{ "name": "qp-mode" }, { "name": "skill:review", "description": "Review" }] } }),
        ]);
        assert!(out.iter().any(|e| matches!(e, Emit::Bound { session_id, model: Some(m) } if session_id == "C:\\s\\a.jsonl" && m == "anthropic/claude-x")));
        assert!(out.iter().any(|e| matches!(e, Emit::Catalog(c) if c.models[0].id == "anthropic/claude-x" && c.commands.len() == 1)), "our own command stays hidden");
        d
    }

    #[test]
    fn rpc_events_become_a_turn() {
        let mut d = driver("full");
        let p = parse(&d.user_message(&TurnInput { text: "fix it".into(), ..Default::default() })[0]);
        assert_eq!((p["type"].as_str(), p["message"].as_str()), (Some("prompt"), Some("fix it")));
        let upd = |e: Value| json!({ "type": "message_update", "usage": { "input": 10, "output": 2, "cacheRead": 5, "cacheWrite": 0, "cost": { "total": 0.01 } }, "assistantMessageEvent": e });
        let out = feed(&mut d, &[
            json!({ "type": "response", "id": p["id"], "command": "prompt", "success": true }),
            json!({ "type": "agent_start" }),
            upd(json!({ "type": "thinking_start", "contentIndex": 0 })),
            upd(json!({ "type": "thinking_delta", "contentIndex": 0, "delta": "hmm" })),
            upd(json!({ "type": "thinking_end", "contentIndex": 0 })),
            upd(json!({ "type": "text_start", "contentIndex": 1 })),
            upd(json!({ "type": "text_delta", "contentIndex": 1, "delta": "Editing." })),
            upd(json!({ "type": "text_end", "contentIndex": 1, "content": "Editing." })),
            json!({ "type": "tool_execution_start", "toolCallId": "c1", "toolName": "edit", "args": { "path": "a.rs" } }),
            json!({ "type": "tool_execution_end", "toolCallId": "c1", "toolName": "edit", "args": { "path": "a.rs" }, "result": { "content": [{ "type": "text", "text": "ok" }] }, "isError": false }),
            json!({ "type": "message_end", "message": { "role": "assistant", "stopReason": "stop", "usage": { "input": 10, "output": 4, "cacheRead": 5, "cost": { "total": 0.02 } } } }),
            json!({ "type": "agent_end", "messages": [], "willRetry": false }),
            json!({ "type": "agent_settled" }),
        ]);
        assert!(out.iter().any(|e| matches!(e, Emit::ReasoningDone { .. })));
        assert!(out.iter().any(|e| matches!(e, Emit::MessageDone { text, .. } if text == "Editing.")));
        assert!(out.iter().any(|e| matches!(e, Emit::FileChanged { path, .. } if path == "a.rs")));
        assert!(out.iter().any(|e| matches!(e, Emit::Usage { cost_usd: Some(c), output_tokens: Some(4), .. } if (*c - 0.02).abs() < 1e-9)));
        assert_eq!(out.iter().filter(|e| matches!(e, Emit::TurnEnd { .. })).count(), 1, "agent_settled after agent_end ends nothing twice");
        assert!(matches!(out.last(), Some(Emit::TurnEnd { status: TurnStatus::Completed, .. })));
        assert_eq!(d.tool_info("bash", &json!({ "command": "cargo test" })).title, "cargo test");
    }

    #[test]
    fn the_mode_extension_asks_and_switches() {
        let mut d = driver("ask");
        let lines = d.user_message(&TurnInput { text: "go".into(), ..Default::default() });
        assert_eq!(lines.len(), 1, "the extension already runs in ask mode");
        let out = feed(&mut d, &[json!({ "type": "extension_ui_request", "id": "ui-1", "method": "select", "title": "QP_APPROVAL {\"tool\":\"bash\",\"input\":{\"command\":\"rm -rf build\"},\"id\":\"c2\"}", "options": ["Allow", "Allow for this session", "Deny"] })]);
        let Some(Emit::Request(r)) = out.first() else { panic!("{out:?}") };
        assert_eq!((r.kind, r.tool_name.as_str(), r.input["command"].as_str(), r.call_id.as_deref()), (RequestKind::Approval, "bash", Some("rm -rf build"), Some("c2")));
        let reply = parse(&d.answer(&r.request_id, &Answer::AllowSession).unwrap()[0]);
        assert_eq!((reply["type"].as_str(), reply["id"].as_str(), reply["value"].as_str()), (Some("extension_ui_response"), Some("ui-1"), Some("Allow for this session")));
        // Someone else's confirm is a yes/no question.
        let out = feed(&mut d, &[json!({ "type": "extension_ui_request", "id": "ui-2", "method": "confirm", "title": "Clear session?", "message": "All lost." })]);
        let Some(Emit::Request(q)) = out.first() else { panic!() };
        assert_eq!(q.kind, RequestKind::Question);
        let mut answers = serde_json::Map::new();
        answers.insert("Clear session?".into(), json!("Yes"));
        assert_eq!(parse(&d.answer(&q.request_id, &Answer::Answers { answers }).unwrap()[0])["confirmed"], true);
        // Stop cancels what waits and aborts.
        feed(&mut d, &[json!({ "type": "extension_ui_request", "id": "ui-3", "method": "select", "title": "QP_APPROVAL {\"tool\":\"write\"}", "options": [] })]);
        let stop: Vec<Value> = d.interrupt().iter().map(|l| parse(l)).collect();
        assert_eq!((stop[0]["cancelled"].as_bool(), stop[1]["type"].as_str()), (Some(true), Some("abort")));
        let out = feed(&mut d, &[json!({ "type": "message_end", "message": { "role": "assistant", "stopReason": "aborted" } })]);
        assert!(matches!(out.last(), Some(Emit::TurnEnd { status: TurnStatus::Interrupted, .. })));
        // A switch between turns goes to the extension before the next prompt.
        assert_eq!(parse(&d.set_mode("plan").unwrap())["message"], "/qp-mode plan");
        assert!(d.set_mode("auto").is_none(), "pi has no automatic reviewer");
        let mut later = driver("ask");
        later.opts.mode = "edits".into();
        let lines: Vec<Value> = later.user_message(&TurnInput { text: "next".into(), ..Default::default() }).iter().map(|l| parse(l)).collect();
        assert_eq!((lines[0]["message"].as_str(), lines[1]["message"].as_str()), (Some("/qp-mode edits"), Some("next")));
        assert_eq!(launch_args("C:\\x\\qp-modes.ts", Some("C:\\s\\a.jsonl"), "anthropic/claude-x"), ["--mode", "rpc", "-e", "C:\\x\\qp-modes.ts", "--session", "C:\\s\\a.jsonl", "--model", "anthropic/claude-x"]);
    }

    #[test]
    fn a_rejected_prompt_or_a_failed_model_call_fails_the_turn() {
        let mut d = driver("full");
        let p = parse(&d.user_message(&TurnInput { text: "go".into(), ..Default::default() })[0]);
        let out = feed(&mut d, &[json!({ "type": "response", "id": p["id"], "command": "prompt", "success": false, "error": "No model selected" })]);
        assert!(matches!(out.last(), Some(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(e), .. }) if e.contains("No model selected")));
        d.user_message(&TurnInput { text: "again".into(), ..Default::default() });
        let out = feed(&mut d, &[json!({ "type": "message_end", "message": { "role": "assistant", "stopReason": "error", "errorMessage": "401 Unauthorized" } }), json!({ "type": "agent_end", "willRetry": false })]);
        assert_eq!(out.iter().filter(|e| matches!(e, Emit::TurnEnd { .. })).count(), 1);
        assert!(matches!(out.iter().find(|e| matches!(e, Emit::TurnEnd { .. })), Some(Emit::TurnEnd { error: Some(e), .. }) if e == "401 Unauthorized"));
    }
}

#[cfg(test)]
mod live {
    use crate::settings::Settings;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    /// Live, opt-in, no inference (card 20): the installed pi and omp load
    /// QuantPilot's mode extension in RPC mode — its `qp-mode` command shows.
    #[test]
    #[ignore = "live: starts the installed pi and omp in RPC mode, no prompt"]
    fn live_pi_and_omp_load_the_mode_extension() {
        let settings = Settings::default();
        let ext = std::env::temp_dir().join(format!("qp-modes-{}.ts", uuid::Uuid::new_v4()));
        std::fs::write(&ext, super::MODES_EXTENSION).unwrap();
        for id in ["pi", "omp"] {
            let Some(exe) = crate::adapters::find(&settings, id).and_then(|a| crate::providers::resolve(&a, &settings)) else {
                println!("{id}: not installed, skipped");
                continue;
            };
            let mut cmd = Command::new(&exe.program);
            cmd.args(&exe.prefix).args(["--mode", "rpc", "--no-session", "-e"]).arg(&ext).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
            let mut child = cmd.spawn().unwrap();
            let mut stdin = child.stdin.take().unwrap();
            writeln!(stdin, r#"{{"id":"c","type":"get_commands"}}"#).unwrap();
            stdin.flush().unwrap();
            let reader = BufReader::new(child.stdout.take().unwrap());
            let mut names = Vec::new();
            for line in reader.lines().map_while(Result::ok).take(200) {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                // pi answers get_commands; omp announces its commands by itself.
                let list = match v["type"].as_str() {
                    Some("response") if v["command"] == "get_commands" && v["success"] == true => &v["data"]["commands"],
                    Some("available_commands_update") => &v["commands"],
                    _ => continue,
                };
                names = list.as_array().into_iter().flatten().filter_map(|c| c["name"].as_str().map(str::to_string)).collect();
                if names.iter().any(|n| n == "qp-mode") {
                    break;
                }
            }
            let _ = child.kill();
            println!("{id} ({}): qp-mode loaded = {}", exe.found, names.iter().any(|n| n == "qp-mode"));
            assert!(names.iter().any(|n| n == "qp-mode"), "{id}: {names:?}");
        }
        let _ = std::fs::remove_file(ext);
    }
}
