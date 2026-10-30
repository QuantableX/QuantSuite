//! An Agent Client Protocol v1 client (card 20; agentclientprotocol.com):
//! Gemini CLI (`gemini --experimental-acp`), OpenCode (`opencode acp`),
//! GitHub Copilot CLI (`copilot --acp`) and the user's own ACP agents.
//!
//! JSON-RPC over stdio. The client offers no file system and no terminal:
//! the agent does its own work and asks before tools through
//! `session/request_permission`. A session is created (`session/new`) or,
//! when the agent advertises `loadSession`, reloaded (`session/load` — the
//! history it replays is not shown again). The thread's permission mode maps
//! onto the agent's own modes where it has them (`session/set_mode`,
//! acknowledged before the prompt goes out) and onto the permission replies
//! everywhere: Full access answers "allow once" by itself, Accept edits does
//! so for edits, Plan declines anything that is not reading. A mode the
//! agent cannot honour fails the turn instead of quietly running under
//! another policy. Messages without a `messageId` are split where a tool
//! call or a thought interrupts the text.

use crate::driver::{Answer, Catalog, CommandInfo, Driver, Emit, ModelInfo, Request, RequestKind, ToolInfo, TurnInput, TurnStatus};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AcpOptions {
    /// The thread's agent id (`gemini`, `opencode`, `copilot`, a custom id).
    pub provider: &'static str,
    /// What the agent is called in messages.
    pub label: String,
    pub cwd: String,
    /// A session to reload.
    pub resume: Option<String>,
    /// `ask` | `edits` | `auto` | `plan` | `full`
    pub mode: String,
    pub model: String,
    /// QuantMCP for this thread, under a name of its own (threads in one
    /// folder never share an entry).
    pub mcp: Option<(String, String)>,
    /// The suite's context, ahead of a new session's first prompt (ACP has
    /// no system prompt).
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Call {
    Initialize,
    New,
    Load,
    Prompt,
    SetMode,
    /// A mode switch while the session is open (the composer's picker).
    LiveMode,
    SetModel,
}

#[derive(Debug, Clone)]
struct Pending {
    rpc_id: Value,
    /// optionId by kind (`allow_once`, `allow_always`, `reject_once`, `reject_always`).
    options: HashMap<String, String>,
}

pub struct AcpDriver {
    opts: AcpOptions,
    next_id: u64,
    calls: HashMap<u64, Call>,
    session: Option<String>,
    can_load: bool,
    images: bool,
    http_mcp: bool,
    /// The agent's modes: (id, name).
    modes: Vec<(String, String)>,
    current_mode: Option<String>,
    models: Vec<ModelInfo>,
    commands: Vec<CommandInfo>,
    version: Option<String>,
    /// Prompts that wait for the session or for a mode switch.
    queued: Vec<TurnInput>,
    /// A `session/load` replays history: nothing of it is shown again.
    loading: bool,
    prompting: bool,
    cancelled: bool,
    instructions_sent: bool,
    pending: HashMap<String, Pending>,
    next_request: u64,
    msg: Option<(String, String)>,
    msg_seq: u64,
    thought: Option<String>,
    tools: HashMap<String, String>,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// The agent's mode for a thread mode: `Ok(None)` when the agent has no
/// modes (the permission replies carry the policy), `Err` when it has modes
/// but none for this one.
pub fn mode_for(mode: &str, available: &[(String, String)]) -> Result<Option<String>, String> {
    if available.is_empty() {
        return if mode == "auto" { Err("has no automatic reviewer".into()) } else { Ok(None) };
    }
    let find = |words: &[&str]| {
        available
            .iter()
            .find(|(id, name)| {
                let id = id.to_ascii_lowercase();
                let name = name.to_ascii_lowercase();
                words.iter().any(|w| id == *w || id.replace(['-', '_'], "") == w.replace(['-', '_'], "") || name == *w)
            })
            .map(|(id, _)| id.clone())
    };
    let plain = || {
        available
            .iter()
            .find(|(id, _)| {
                let l = id.to_ascii_lowercase();
                !l.contains("plan") && !l.contains("yolo") && !l.contains("bypass")
            })
            .map(|(id, _)| id.clone())
    };
    let found = match mode {
        "plan" => find(&["plan", "plan-mode", "planning", "architect"]),
        "full" => find(&["yolo", "bypassPermissions", "bypass", "full", "full-access", "autopilot"]).or_else(plain),
        "edits" => find(&["autoEdit", "auto_edit", "acceptEdits", "accept-edits", "edit", "edits"]).or_else(plain),
        "ask" => find(&["default", "ask", "build", "normal", "agent"]).or_else(plain),
        _ => None,
    };
    found.map(Some).ok_or_else(|| format!("has no {} mode", match mode {
        "plan" => "Plan",
        "auto" => "Auto",
        "full" => "Full access",
        "edits" => "Accept edits",
        _ => "Supervised",
    }))
}

impl AcpDriver {
    pub fn new(opts: AcpOptions) -> Self {
        Self {
            opts,
            next_id: 0,
            calls: HashMap::new(),
            session: None,
            can_load: false,
            images: false,
            http_mcp: false,
            modes: Vec::new(),
            current_mode: None,
            models: Vec::new(),
            commands: Vec::new(),
            version: None,
            queued: Vec::new(),
            loading: false,
            prompting: false,
            cancelled: false,
            instructions_sent: false,
            pending: HashMap::new(),
            next_request: 0,
            msg: None,
            msg_seq: 0,
            thought: None,
            tools: HashMap::new(),
        }
    }

    fn request(&mut self, call: Call, method: &str, params: Value) -> String {
        self.next_id += 1;
        self.calls.insert(self.next_id, call);
        json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params }).to_string()
    }

    fn mcp_servers(&self) -> Value {
        match &self.opts.mcp {
            Some((name, url)) if self.http_mcp => json!([{ "type": "http", "name": name, "url": url, "headers": [] }]),
            _ => json!([]),
        }
    }

    fn catalog(&self) -> Emit {
        Emit::Catalog(Catalog { provider: self.opts.provider.into(), version: self.version.clone(), models: self.models.clone(), commands: self.commands.clone(), skills: Vec::new() })
    }

    /// Text and thoughts end where something else begins.
    fn close_text(&mut self, out: &mut Vec<Emit>) {
        if let Some(key) = self.thought.take() {
            out.push(Emit::ReasoningDone { key });
        }
        if let Some((key, text)) = self.msg.take() {
            out.push(Emit::MessageDone { key, text });
        }
    }

    fn fail(&mut self, out: &mut Vec<Emit>, error: String) {
        self.close_text(out);
        self.prompting = false;
        out.push(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(error), duration_ms: None, cost_usd: None });
    }

    /// The prompt itself, once the session and its mode are ready.
    fn prompt(&mut self, input: &TurnInput) -> Vec<String> {
        let Some(session) = self.session.clone() else {
            self.queued.push(input.clone());
            return Vec::new();
        };
        let mut blocks = Vec::new();
        if !self.instructions_sent && self.opts.resume.is_none() {
            if let Some(ctx) = self.opts.instructions.clone().filter(|c| !c.trim().is_empty()) {
                blocks.push(json!({ "type": "text", "text": format!("<context>\n{ctx}\n</context>") }));
            }
        }
        self.instructions_sent = true;
        blocks.push(json!({ "type": "text", "text": input.text }));
        if self.images {
            for img in &input.images {
                blocks.push(json!({ "type": "image", "mimeType": img.media_type, "data": img.base64 }));
            }
        }
        self.prompting = true;
        self.cancelled = false;
        vec![self.request(Call::Prompt, "session/prompt", json!({ "sessionId": session, "prompt": blocks }))]
    }

    /// Set the mode the thread asks for, then send what waits.
    fn go(&mut self) -> Vec<Emit> {
        let mut out = Vec::new();
        if self.queued.is_empty() {
            return out;
        }
        match mode_for(&self.opts.mode, &self.modes) {
            Err(why) => {
                self.queued.clear();
                self.prompting = true;
                let error = format!("{} {why} — choose another permission mode", self.opts.label);
                self.fail(&mut out, error);
            }
            Ok(Some(id)) if self.current_mode.as_deref() != Some(id.as_str()) => {
                let session = self.session.clone().unwrap_or_default();
                self.prompting = true;
                out.push(Emit::Write(self.request(Call::SetMode, "session/set_mode", json!({ "sessionId": session, "modeId": id }))));
            }
            Ok(_) => {
                for input in std::mem::take(&mut self.queued) {
                    out.extend(self.prompt(&input).into_iter().map(Emit::Write));
                }
            }
        }
        out
    }

    fn on_response(&mut self, id: u64, msg: &Value) -> Vec<Emit> {
        let Some(call) = self.calls.remove(&id) else {
            return Vec::new();
        };
        let result = msg.get("result").cloned().unwrap_or(Value::Null);
        let error = msg.get("error").map(|e| s(e, "message").unwrap_or("the agent refused").to_string());
        let mut out = Vec::new();
        match call {
            Call::Initialize => {
                if let Some(e) = error {
                    self.prompting = true;
                    self.fail(&mut out, format!("{} did not start: {e}", self.opts.label));
                    return out;
                }
                let caps = &result["agentCapabilities"];
                self.can_load = caps["loadSession"].as_bool().unwrap_or(false);
                self.images = caps["promptCapabilities"]["image"].as_bool().unwrap_or(false);
                self.http_mcp = caps["mcpCapabilities"]["http"].as_bool().unwrap_or(false);
                self.version = result["agentInfo"]["version"].as_str().map(str::to_string);
                let cwd = self.opts.cwd.clone();
                let servers = self.mcp_servers();
                let line = match self.opts.resume.clone().filter(|_| self.can_load) {
                    Some(sid) => {
                        self.loading = true;
                        self.session = Some(sid.clone());
                        self.request(Call::Load, "session/load", json!({ "sessionId": sid, "cwd": cwd, "mcpServers": servers }))
                    }
                    None => self.request(Call::New, "session/new", json!({ "cwd": cwd, "mcpServers": servers })),
                };
                out.push(Emit::Write(line));
            }
            Call::New | Call::Load => {
                self.loading = false;
                if let Some(e) = error {
                    if call == Call::Load {
                        // A session the agent no longer has: start a new one.
                        self.session = None;
                        self.opts.resume = None;
                        out.push(Emit::Notice { text: format!("{} could not reopen the conversation ({e}); it starts a new one.", self.opts.label) });
                        let line = self.request(Call::New, "session/new", json!({ "cwd": self.opts.cwd, "mcpServers": self.mcp_servers() }));
                        out.push(Emit::Write(line));
                        return out;
                    }
                    self.prompting = !self.queued.is_empty();
                    self.queued.clear();
                    if self.prompting {
                        self.fail(&mut out, format!("{} could not open a session: {e}", self.opts.label));
                    } else {
                        out.push(Emit::Notice { text: format!("{} could not open a session: {e}", self.opts.label) });
                    }
                    return out;
                }
                if let Some(sid) = s(&result, "sessionId") {
                    self.session = Some(sid.to_string());
                }
                if let Some(modes) = result.get("modes").filter(|m| m.is_object()) {
                    self.current_mode = s(modes, "currentModeId").map(str::to_string);
                    self.modes = modes["availableModes"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|m| Some((s(m, "id")?.to_string(), s(m, "name").unwrap_or_default().to_string()))).collect())
                        .unwrap_or_default();
                }
                let mut model = None;
                if let Some(models) = result.get("models").filter(|m| m.is_object()) {
                    model = s(models, "currentModelId").map(str::to_string);
                    self.models = models["availableModels"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|m| {
                                    let id = s(m, "modelId")?.to_string();
                                    Some(ModelInfo { display_name: s(m, "name").unwrap_or(&id).to_string(), description: s(m, "description").unwrap_or_default().to_string(), id, ..Default::default() })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
                if let Some(sid) = self.session.clone() {
                    out.push(Emit::Bound { session_id: sid, model: model.clone() });
                }
                out.push(self.catalog());
                // The thread's model, when the agent offers it and runs another one.
                let wanted = self.opts.model.trim().to_string();
                if !wanted.is_empty() && model.as_deref() != Some(wanted.as_str()) && self.models.iter().any(|m| m.id == wanted) {
                    let sid = self.session.clone().unwrap_or_default();
                    out.push(Emit::Write(self.request(Call::SetModel, "session/set_model", json!({ "sessionId": sid, "modelId": wanted }))));
                }
                out.extend(self.go());
            }
            Call::SetMode => {
                if let Some(e) = error {
                    self.queued.clear();
                    self.fail(&mut out, format!("{} did not switch its mode: {e}", self.opts.label));
                    return out;
                }
                if let Ok(Some(id)) = mode_for(&self.opts.mode, &self.modes) {
                    self.current_mode = Some(id);
                }
                for input in std::mem::take(&mut self.queued) {
                    out.extend(self.prompt(&input).into_iter().map(Emit::Write));
                }
            }
            Call::SetModel => {
                if let Some(e) = error {
                    out.push(Emit::Notice { text: format!("{} kept its model: {e}", self.opts.label) });
                }
            }
            Call::LiveMode => {
                if let Some(e) = error {
                    self.current_mode = None;
                    out.push(Emit::Notice { text: format!("{} did not switch its mode ({e}); the next message tries again.", self.opts.label) });
                }
            }
            Call::Prompt => {
                self.close_text(&mut out);
                self.prompting = false;
                if let Some(e) = error {
                    out.push(Emit::TurnEnd { status: if self.cancelled { TurnStatus::Interrupted } else { TurnStatus::Failed }, error: (!self.cancelled).then_some(e), duration_ms: None, cost_usd: None });
                    return out;
                }
                if let Some(u) = result.get("usage").filter(|u| u.is_object()) {
                    let n = |k: &str| u.get(k).and_then(Value::as_i64);
                    out.push(Emit::Usage { context_tokens: None, context_window: None, cost_usd: None, input_tokens: n("inputTokens"), output_tokens: n("outputTokens") });
                }
                let (status, error) = match s(&result, "stopReason").unwrap_or("end_turn") {
                    "cancelled" => (TurnStatus::Interrupted, None),
                    "refusal" => (TurnStatus::Failed, Some(format!("{} refused to continue.", self.opts.label))),
                    "max_tokens" => (TurnStatus::Completed, None),
                    "max_turn_requests" => (TurnStatus::Completed, None),
                    _ if self.cancelled => (TurnStatus::Interrupted, None),
                    _ => (TurnStatus::Completed, None),
                };
                if s(&result, "stopReason") == Some("max_tokens") {
                    out.push(Emit::Notice { text: "The reply reached the token limit and stopped.".into() });
                }
                out.push(Emit::TurnEnd { status, error, duration_ms: None, cost_usd: None });
            }
        }
        out
    }

    fn on_update(&mut self, update: &Value) -> Vec<Emit> {
        let mut out = Vec::new();
        if self.loading {
            return out;
        }
        match s(update, "sessionUpdate") {
            Some("agent_message_chunk") => {
                if let Some(key) = self.thought.take() {
                    out.push(Emit::ReasoningDone { key });
                }
                let text = s(&update["content"], "text").unwrap_or_default().to_string();
                let id = s(update, "messageId").map(str::to_string);
                // A new message id ends the one before it.
                if let (Some(id), Some((key, _))) = (&id, &self.msg) {
                    if !key.ends_with(id.as_str()) {
                        self.close_text(&mut out);
                    }
                }
                if self.msg.is_none() {
                    self.msg_seq += 1;
                    let key = format!("acp-{}-{}", self.msg_seq, id.unwrap_or_default());
                    out.push(Emit::MessageStart { key: key.clone(), phase: None, parent: None });
                    self.msg = Some((key, String::new()));
                }
                if let Some((key, all)) = self.msg.as_mut() {
                    all.push_str(&text);
                    out.push(Emit::TextDelta { key: key.clone(), text });
                }
            }
            Some("agent_thought_chunk") => {
                if let Some((key, text)) = self.msg.take() {
                    out.push(Emit::MessageDone { key, text });
                }
                let text = s(&update["content"], "text").unwrap_or_default().to_string();
                if self.thought.is_none() {
                    self.msg_seq += 1;
                    let key = format!("acp-think-{}", self.msg_seq);
                    out.push(Emit::ReasoningStart { key: key.clone(), parent: None });
                    self.thought = Some(key);
                }
                if let Some(key) = &self.thought {
                    out.push(Emit::ReasoningDelta { key: key.clone(), text, estimated_tokens: None });
                }
            }
            Some("tool_call") => {
                self.close_text(&mut out);
                let Some(id) = s(update, "toolCallId").map(str::to_string) else { return out };
                let kind = s(update, "kind").unwrap_or("other").to_string();
                let mut input = update.get("rawInput").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}));
                if let (Some(obj), Some(title)) = (input.as_object_mut(), s(update, "title")) {
                    obj.entry("title").or_insert_with(|| json!(title));
                }
                if let (Some(obj), Some(locs)) = (input.as_object_mut(), update.get("locations").and_then(Value::as_array)) {
                    if let Some(path) = locs.first().and_then(|l| s(l, "path")) {
                        obj.entry("path").or_insert_with(|| json!(path));
                    }
                }
                self.tools.insert(id.clone(), kind.clone());
                out.push(Emit::ToolStart { call_id: id.clone(), name: kind, input, parent: None });
                out.extend(self.tool_done(&id, update));
            }
            Some("tool_call_update") => {
                let Some(id) = s(update, "toolCallId").map(str::to_string) else { return out };
                if let Some(raw) = update.get("rawInput").filter(|v| !v.is_null()) {
                    out.push(Emit::ToolInput { call_id: id.clone(), input: raw.clone() });
                }
                out.extend(self.tool_done(&id, update));
            }
            Some("plan") => {
                let todos: Vec<Value> = update["entries"]
                    .as_array()
                    .map(|a| a.iter().map(|e| json!({ "content": s(e, "content").unwrap_or_default(), "status": s(e, "status").unwrap_or("pending") })).collect())
                    .unwrap_or_default();
                out.push(Emit::Todos { todos: Value::Array(todos) });
            }
            Some("available_commands_update") => {
                self.commands = update["availableCommands"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| Some(CommandInfo { name: s(c, "name")?.to_string(), description: s(c, "description").unwrap_or_default().to_string(), argument_hint: c["input"]["hint"].as_str().unwrap_or_default().to_string() }))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(self.catalog());
            }
            Some("current_mode_update") => {
                self.current_mode = s(update, "currentModeId").map(str::to_string);
            }
            Some("usage_update") => {
                let used = update.get("used").and_then(Value::as_i64);
                let size = update.get("size").and_then(Value::as_i64);
                let cost = update["cost"]["amount"].as_f64();
                out.push(Emit::Usage { context_tokens: used, context_window: size, cost_usd: cost, input_tokens: None, output_tokens: None });
            }
            _ => {}
        }
        out
    }

    fn tool_done(&mut self, id: &str, update: &Value) -> Vec<Emit> {
        let status = s(update, "status").unwrap_or_default();
        if !matches!(status, "completed" | "failed") {
            return Vec::new();
        }
        let mut text = Vec::new();
        for c in update.get("content").and_then(Value::as_array).into_iter().flatten() {
            match s(c, "type") {
                Some("content") => text.extend(s(&c["content"], "text").map(str::to_string)),
                Some("diff") => text.push(format!("Edited {}", s(c, "path").unwrap_or("a file"))),
                Some("terminal") => text.push("(terminal output)".into()),
                _ => {}
            }
        }
        if text.is_empty() {
            if let Some(raw) = update.get("rawOutput").filter(|v| !v.is_null()) {
                text.push(raw.as_str().map(str::to_string).unwrap_or_else(|| raw.to_string()));
            }
        }
        let mut out = vec![Emit::ToolResult { call_id: id.to_string(), output: text.join("\n"), is_error: status == "failed" }];
        if status == "completed" && matches!(self.tools.get(id).map(String::as_str), Some("edit" | "move" | "delete")) {
            for c in update.get("content").and_then(Value::as_array).into_iter().flatten() {
                if let Some(path) = (s(c, "type") == Some("diff")).then(|| s(c, "path")).flatten() {
                    out.push(Emit::FileChanged { path: path.to_string(), call_id: id.to_string() });
                }
            }
        }
        out
    }

    /// `session/request_permission`: answered by itself where the mode decides, else asked.
    fn on_permission(&mut self, rpc_id: Value, params: &Value) -> Vec<Emit> {
        let tool = &params["toolCall"];
        let kind = s(tool, "kind").unwrap_or("other").to_string();
        let options: HashMap<String, String> = params["options"]
            .as_array()
            .map(|a| a.iter().filter_map(|o| Some((s(o, "kind")?.to_string(), s(o, "optionId")?.to_string()))).collect())
            .unwrap_or_default();
        let reading = matches!(kind.as_str(), "read" | "search" | "fetch" | "think");
        let editing = matches!(kind.as_str(), "edit" | "move" | "delete");
        let auto = match self.opts.mode.as_str() {
            "full" => Some("allow_once"),
            "edits" if editing || reading => Some("allow_once"),
            "plan" if reading => Some("allow_once"),
            "plan" => Some("reject_once"),
            _ => None,
        };
        if let Some(choice) = auto {
            return vec![Emit::Write(Self::reply(&rpc_id, options.get(choice).cloned()))];
        }
        self.next_request += 1;
        let request_id = format!("acp-{}", self.next_request);
        let mut input = tool.get("rawInput").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}));
        if let (Some(obj), Some(title)) = (input.as_object_mut(), s(tool, "title")) {
            obj.entry("title").or_insert_with(|| json!(title));
        }
        let allow_session = options.contains_key("allow_always");
        self.pending.insert(request_id.clone(), Pending { rpc_id, options });
        vec![Emit::Request(Request {
            request_id,
            kind: RequestKind::Approval,
            tool_name: kind,
            call_id: s(tool, "toolCallId").map(str::to_string),
            input,
            parent: None,
            allow_session,
            by_message: false,
        })]
    }

    fn reply(rpc_id: &Value, option: Option<String>) -> String {
        let outcome = match option {
            Some(id) => json!({ "outcome": "selected", "optionId": id }),
            None => json!({ "outcome": "cancelled" }),
        };
        json!({ "jsonrpc": "2.0", "id": rpc_id, "result": { "outcome": outcome } }).to_string()
    }
}

impl Driver for AcpDriver {
    fn provider(&self) -> &'static str {
        self.opts.provider
    }

    fn tool_info(&self, name: &str, input: &Value) -> ToolInfo {
        let title = s(input, "title").or_else(|| s(input, "command")).or_else(|| s(input, "path")).unwrap_or(name).to_string();
        let category = match name {
            "execute" => "command",
            "read" => "read",
            "edit" | "delete" | "move" => "edit",
            "search" => "search",
            "fetch" => "web",
            _ => "tool",
        };
        ToolInfo { category, title: title.lines().next().unwrap_or_default().to_string() }
    }

    fn handshake(&mut self) -> Vec<String> {
        vec![self.request(
            Call::Initialize,
            "initialize",
            json!({ "protocolVersion": 1, "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false }, "clientInfo": { "name": "QuantPilot", "version": env!("CARGO_PKG_VERSION") } }),
        )]
    }

    fn user_message(&mut self, input: &TurnInput) -> Vec<String> {
        self.queued.push(input.clone());
        if self.session.is_none() || self.loading {
            return Vec::new();
        }
        self.go()
            .into_iter()
            .filter_map(|e| match e {
                Emit::Write(l) => Some(l),
                _ => None,
            })
            .collect()
    }

    fn on_line(&mut self, line: &str) -> Vec<Emit> {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match (msg.get("id").cloned(), s(&msg, "method")) {
            (Some(id), Some("session/request_permission")) => self.on_permission(id, &msg["params"]),
            // The client offers no file system or terminal: anything else it is asked is refused.
            (Some(id), Some(method)) => vec![Emit::Write(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("QuantPilot does not offer {method}") } }).to_string())],
            (Some(id), None) => match id.as_u64() {
                Some(n) => self.on_response(n, &msg),
                None => Vec::new(),
            },
            (None, Some("session/update")) => self.on_update(&msg["params"]["update"]),
            _ => Vec::new(),
        }
    }

    fn answer(&mut self, request_id: &str, answer: &Answer) -> Result<Vec<String>, String> {
        let p = self.pending.remove(request_id).ok_or("this request is no longer waiting")?;
        let pick = |kinds: &[&str]| kinds.iter().find_map(|k| p.options.get(*k).cloned());
        let option = match answer {
            Answer::Allow => pick(&["allow_once", "allow_always"]),
            Answer::AllowSession => pick(&["allow_always", "allow_once"]),
            Answer::Deny { .. } | Answer::Dismiss => pick(&["reject_once", "reject_always"]),
            Answer::Answers { .. } => return Err("an approval takes allow or deny".into()),
        };
        Ok(vec![Self::reply(&p.rpc_id, option)])
    }

    fn interrupt(&mut self) -> Vec<String> {
        let Some(session) = self.session.clone() else {
            return Vec::new();
        };
        if !self.prompting {
            return Vec::new();
        }
        self.cancelled = true;
        // Pending permission requests are answered "cancelled" (the protocol asks for it).
        let mut lines: Vec<String> = self.pending.drain().map(|(_, p)| Self::reply(&p.rpc_id, None)).collect();
        lines.push(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": session } }).to_string());
        lines
    }

    fn set_mode(&mut self, mode: &str) -> Option<String> {
        self.opts.mode = mode.to_string();
        let session = self.session.clone()?;
        match mode_for(mode, &self.modes) {
            Ok(Some(id)) if self.current_mode.as_deref() != Some(id.as_str()) => {
                self.current_mode = Some(id.clone());
                Some(self.request(Call::LiveMode, "session/set_mode", json!({ "sessionId": session, "modeId": id })))
            }
            // Without modes the replies carry the policy: nothing to send, but nothing to restart either.
            Ok(_) => Some(json!({ "jsonrpc": "2.0", "method": "$/qp/noop" }).to_string()),
            Err(_) => None,
        }
    }

    fn set_model(&mut self, model: &str) -> Option<String> {
        self.opts.model = model.to_string();
        let session = self.session.clone()?;
        self.models.iter().any(|m| m.id == model).then(|| self.request(Call::SetModel, "session/set_model", json!({ "sessionId": session, "modelId": model })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }
    fn writes(out: &[Emit]) -> Vec<Value> {
        out.iter().filter_map(|e| if let Emit::Write(l) = e { Some(parse(l)) } else { None }).collect()
    }
    fn opts(mode: &str) -> AcpOptions {
        AcpOptions { provider: "gemini", label: "Gemini CLI".into(), cwd: "C:\\p".into(), mode: mode.into(), instructions: Some("You are in QuantSuite.".into()), mcp: Some(("quantsuite-t1".into(), "http://127.0.0.1:3100/mcp".into())), ..Default::default() }
    }
    /// Through initialize and session/new, with modes `default`, `autoEdit`, `yolo`, `plan`.
    fn ready(mode: &str, modes: bool) -> AcpDriver {
        let mut d = AcpDriver::new(opts(mode));
        let init = parse(&d.handshake()[0]);
        assert_eq!(init["params"]["clientCapabilities"]["terminal"], false);
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": init["id"], "result": { "protocolVersion": 1, "agentCapabilities": { "loadSession": true, "promptCapabilities": { "image": true }, "mcpCapabilities": { "http": true } } } }).to_string());
        let new = writes(&out).remove(0);
        assert_eq!(new["method"], "session/new");
        assert_eq!(new["params"]["mcpServers"][0]["name"], "quantsuite-t1", "QuantMCP under the thread's own name");
        let mut result = json!({ "sessionId": "s-1", "models": { "currentModelId": "gemini-3-pro", "availableModels": [{ "modelId": "gemini-3-pro", "name": "Gemini 3 Pro" }, { "modelId": "gemini-3-flash", "name": "Gemini 3 Flash" }] } });
        if modes {
            result["modes"] = json!({ "currentModeId": "default", "availableModes": [{ "id": "default", "name": "Default" }, { "id": "autoEdit", "name": "Auto Edit" }, { "id": "yolo", "name": "YOLO" }, { "id": "plan", "name": "Plan" }] });
        }
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": new["id"], "result": result }).to_string());
        assert!(out.iter().any(|e| matches!(e, Emit::Bound { session_id, model: Some(m) } if session_id == "s-1" && m == "gemini-3-pro")));
        assert!(out.iter().any(|e| matches!(e, Emit::Catalog(c) if c.models.len() == 2)));
        d
    }

    #[test]
    fn a_prompt_turn_streams_messages_thoughts_tools_and_a_plan() {
        let mut d = ready("ask", true);
        let prompt = parse(&d.user_message(&TurnInput { text: "fix it".into(), ..Default::default() })[0]);
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(prompt["params"]["prompt"][0]["text"], "<context>\nYou are in QuantSuite.\n</context>", "the context goes ahead of the first prompt");
        assert_eq!(prompt["params"]["prompt"][1]["text"], "fix it");
        let up = |u: Value| json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s-1", "update": u } }).to_string();
        let mut out = Vec::new();
        for u in [
            json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Thinking" } }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Let me " } }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "look." } }),
            json!({ "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Read a.rs", "kind": "read", "status": "pending", "locations": [{ "path": "C:\\p\\a.rs" }] }),
            json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed", "content": [{ "type": "content", "content": { "type": "text", "text": "fn main() {}" } }] }),
            json!({ "sessionUpdate": "plan", "entries": [{ "content": "Read", "priority": "high", "status": "completed" }, { "content": "Fix", "priority": "high", "status": "in_progress" }] }),
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Fixed." } }),
        ] {
            out.extend(d.on_line(&up(u)));
        }
        out.extend(d.on_line(&json!({ "jsonrpc": "2.0", "id": prompt["id"], "result": { "stopReason": "end_turn" } }).to_string()));
        let done: Vec<&String> = out.iter().filter_map(|e| if let Emit::MessageDone { text, .. } = e { Some(text) } else { None }).collect();
        assert_eq!(done, ["Let me look.", "Fixed."], "the tool call splits the text into two messages");
        assert!(out.iter().any(|e| matches!(e, Emit::ReasoningDone { .. })));
        assert!(out.iter().any(|e| matches!(e, Emit::ToolStart { call_id, name, input, .. } if call_id == "t1" && name == "read" && input["path"] == "C:\\p\\a.rs")));
        assert!(out.iter().any(|e| matches!(e, Emit::ToolResult { output, is_error: false, .. } if output == "fn main() {}")));
        assert!(out.iter().any(|e| matches!(e, Emit::Todos { todos } if todos[1]["status"] == "in_progress")));
        assert!(matches!(out.last(), Some(Emit::TurnEnd { status: TurnStatus::Completed, .. })));
        assert_eq!(d.tool_info("read", &json!({ "title": "Read a.rs" })).category, "read");
    }

    #[test]
    fn permissions_follow_the_mode() {
        let req = |n: u64, kind: &str| json!({ "jsonrpc": "2.0", "id": 100 + n, "method": "session/request_permission", "params": { "sessionId": "s-1",
            "toolCall": { "toolCallId": format!("t{n}"), "title": "rm -rf build", "kind": kind, "rawInput": { "command": "rm -rf build" } },
            "options": [{ "optionId": "once", "name": "Allow", "kind": "allow_once" }, { "optionId": "always", "name": "Always", "kind": "allow_always" }, { "optionId": "no", "name": "Reject", "kind": "reject_once" }] } }).to_string();
        // Supervised: the user decides.
        let mut d = ready("ask", true);
        let out = d.on_line(&req(1, "execute"));
        let Some(Emit::Request(r)) = out.first() else { panic!("{out:?}") };
        assert!(r.allow_session && r.tool_name == "execute");
        let reply = parse(&d.answer(&r.request_id, &Answer::AllowSession).unwrap()[0]);
        assert_eq!((reply["id"].as_u64(), reply["result"]["outcome"]["optionId"].as_str()), (Some(101), Some("always")));
        let out = d.on_line(&req(2, "execute"));
        let Some(Emit::Request(r)) = out.first() else { panic!() };
        assert_eq!(parse(&d.answer(&r.request_id, &Answer::Deny { message: None }).unwrap()[0])["result"]["outcome"]["optionId"], "no");
        // Full access answers "once" by itself; Accept edits does so for edits only; Plan declines changes.
        let mut full = ready("full", false);
        assert_eq!(writes(&full.on_line(&req(3, "execute")))[0]["result"]["outcome"]["optionId"], "once");
        let mut edits = ready("edits", false);
        assert_eq!(writes(&edits.on_line(&req(4, "edit")))[0]["result"]["outcome"]["optionId"], "once");
        assert!(matches!(edits.on_line(&req(5, "execute")).first(), Some(Emit::Request(_))));
        let mut plan = ready("plan", false);
        assert_eq!(writes(&plan.on_line(&req(6, "edit")))[0]["result"]["outcome"]["optionId"], "no");
        assert_eq!(writes(&plan.on_line(&req(7, "read")))[0]["result"]["outcome"]["optionId"], "once");
        // Stop answers what waits with "cancelled", then cancels.
        let mut d = ready("ask", true);
        d.user_message(&TurnInput { text: "go".into(), ..Default::default() });
        d.on_line(&req(8, "execute"));
        let stop: Vec<Value> = d.interrupt().iter().map(|l| parse(l)).collect();
        assert_eq!(stop[0]["result"]["outcome"]["outcome"], "cancelled");
        assert_eq!(stop[1]["method"], "session/cancel");
    }

    #[test]
    fn the_mode_is_set_and_acknowledged_before_the_prompt() {
        let mut d = ready("plan", true);
        let lines = d.user_message(&TurnInput { text: "plan it".into(), ..Default::default() });
        assert_eq!(lines.len(), 1);
        let set = parse(&lines[0]);
        assert_eq!((set["method"].as_str(), set["params"]["modeId"].as_str()), (Some("session/set_mode"), Some("plan")));
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": set["id"], "result": {} }).to_string());
        assert_eq!(writes(&out)[0]["method"], "session/prompt", "the prompt follows the acknowledgement");
        // A refused switch fails the turn; it never runs under another policy.
        let mut d = ready("full", true);
        let set = parse(&d.user_message(&TurnInput { text: "go".into(), ..Default::default() })[0]);
        assert_eq!(set["params"]["modeId"], "yolo");
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": set["id"], "error": { "code": -32000, "message": "mode not allowed" } }).to_string());
        assert!(matches!(out.last(), Some(Emit::TurnEnd { status: TurnStatus::Failed, error: Some(e), .. }) if e.contains("mode not allowed")), "{out:?}");
        assert!(writes(&out).is_empty(), "no prompt went out");
        // A mode the agent does not have fails at once.
        let mut d = ready("auto", true);
        let lines = d.user_message(&TurnInput { text: "go".into(), ..Default::default() });
        assert!(lines.is_empty());
        assert_eq!(mode_for("auto", &[("default".into(), "Default".into())]), Err("has no Auto mode".into()));
        assert_eq!(mode_for("full", &[("build".into(), "Build".into()), ("plan".into(), "Plan".into())]), Ok(Some("build".into())), "full access through the replies under the plain mode");
        assert_eq!(mode_for("ask", &[]), Ok(None));
    }

    #[test]
    fn a_reloaded_session_skips_the_replay_and_a_lost_one_starts_new() {
        let mut d = AcpDriver::new(AcpOptions { resume: Some("old".into()), ..opts("ask") });
        let init = parse(&d.handshake()[0]);
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": init["id"], "result": { "agentCapabilities": { "loadSession": true } } }).to_string());
        let load = writes(&out).remove(0);
        assert_eq!((load["method"].as_str(), load["params"]["sessionId"].as_str()), (Some("session/load"), Some("old")));
        assert!(load["params"]["mcpServers"].as_array().unwrap().is_empty(), "no http MCP capability, no entry");
        let replay = json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "old", "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "old reply" } } } }).to_string();
        assert!(d.on_line(&replay).is_empty(), "history is not shown twice");
        let out = d.on_line(&json!({ "jsonrpc": "2.0", "id": load["id"], "error": { "code": -32002, "message": "not found" } }).to_string());
        assert!(writes(&out).iter().any(|w| w["method"] == "session/new"));
        // The client offers no file system: such requests are refused.
        let fs = d.on_line(&json!({ "jsonrpc": "2.0", "id": 9, "method": "fs/read_text_file", "params": {} }).to_string());
        assert_eq!(writes(&fs)[0]["error"]["code"], -32601);
    }
}

#[cfg(test)]
mod live {
    use super::*;
    use crate::settings::Settings;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    /// Live, opt-in, no session, no inference (card 20): each installed ACP
    /// agent answers `initialize`. Agents that are not installed are skipped.
    #[test]
    #[ignore = "live: starts the installed Gemini CLI, OpenCode and Copilot in ACP mode"]
    fn live_acp_agents_initialize() {
        let settings = Settings::default();
        let mut failed = Vec::new();
        for id in ["gemini", "opencode", "copilot"] {
            let adapter = crate::adapters::find(&settings, id).unwrap();
            let Some(exe) = crate::providers::resolve(&adapter, &settings) else {
                println!("{id}: not installed, skipped");
                continue;
            };
            let mut cmd = Command::new(&exe.program);
            cmd.args(&exe.prefix).args(adapter.acp_args()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            let mut d = AcpDriver::new(AcpOptions { provider: "x", label: adapter.label.clone(), ..Default::default() });
            let mut stdin = child.stdin.take().unwrap();
            writeln!(stdin, "{}", d.handshake()[0]).unwrap();
            stdin.flush().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let out = child.stdout.take().unwrap();
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            let mut answered = None;
            while let Ok(line) = rx.recv_timeout(std::time::Duration::from_secs(30)) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if v["id"] == 1 {
                        answered = Some(v);
                        break;
                    }
                }
            }
            let _ = child.kill();
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            match answered {
                Some(v) if v.get("result").is_some() => println!("{id}: initialize ok — loadSession {}", v["result"]["agentCapabilities"]["loadSession"]),
                other => {
                    println!("{id}: no initialize answer ({other:?}); stderr: {}", err.lines().take(4).collect::<Vec<_>>().join(" | "));
                    failed.push(id);
                }
            }
        }
        assert!(failed.is_empty(), "installed but not answering: {failed:?}");
    }
}
