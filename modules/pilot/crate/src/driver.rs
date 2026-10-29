//! The adapter boundary (docs/PLAN-QUANTPILOT.md §4.2, §5).
//!
//! A [`Driver`] speaks one CLI's machine protocol over newline-delimited
//! stdin/stdout and translates it into [`Emit`]s. It owns no process, no
//! database and no UI: the runtime spawns the CLI, feeds every stdout line
//! to [`Driver::on_line`], writes what the driver returns to stdin, and
//! turns the emits into timeline items. That keeps every provider quirk in
//! its driver file and makes each driver testable from recorded frames.

use crate::events::RateWindow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Completed,
    Failed,
    Interrupted,
    Limited,
}

impl TurnStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnStatus::Completed => "completed",
            TurnStatus::Failed => "failed",
            TurnStatus::Interrupted => "interrupted",
            TurnStatus::Limited => "limited",
        }
    }
}

/// What a request from the agent asks the user for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    /// Run a tool (command, edit, fetch, MCP call).
    Approval,
    /// Structured questions with options.
    Question,
    /// A plan waiting to be accepted before implementation.
    Plan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub request_id: String,
    pub kind: RequestKind,
    pub tool_name: String,
    /// The tool call the request belongs to, when the provider says.
    pub call_id: Option<String>,
    /// The tool input exactly as the agent sent it (the reply may need it).
    pub input: Value,
    /// A subagent's request carries its parent tool call.
    pub parent: Option<String>,
    /// The provider offers "allow for this session" for this request.
    pub allow_session: bool,
    /// Answered by a new user message instead of a protocol reply (Codex
    /// async questions): it stays pending after the turn and the process end.
    pub by_message: bool,
}

/// The user's reply to a [`Request`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "decision", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Answer {
    Allow,
    /// Allow this and the same kind of call for the rest of the session.
    AllowSession,
    /// Decline; `message` goes back to the agent that asked.
    Deny { message: Option<String> },
    /// Answers to a question request: question text → chosen label(s) or free text.
    Answers { answers: serde_json::Map<String, Value> },
    /// Close a question without answering.
    Dismiss,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// What is passed to `--model` / the protocol — never the display name.
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub resolved_model: Option<String>,
    pub efforts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub argument_hint: String,
}

/// What the CLI reports about itself at startup.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub provider: String,
    pub version: Option<String>,
    pub models: Vec<ModelInfo>,
    pub commands: Vec<CommandInfo>,
    /// Skills the CLI offers (`$name` in the composer, card 9).
    #[serde(default)]
    pub skills: Vec<crate::context::SkillInfo>,
}

/// An image for a user message, already normalized to PNG by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInput {
    pub media_type: String,
    pub base64: String,
    /// The saved copy, for providers that read images from disk (Codex `localImage`).
    pub path: Option<String>,
}

/// A skill picked with `$` (Codex takes it as its own input item).
#[derive(Debug, Clone, PartialEq)]
pub struct SkillRef {
    pub name: String,
    pub path: String,
}

/// What a driver tells the runtime. Keys (`key`, `call_id`) are the
/// driver's own stable ids for an item; the runtime maps them to items.
#[derive(Debug, Clone, PartialEq)]
pub enum Emit {
    /// The provider's session id for this thread is known.
    Bound { session_id: String, model: Option<String> },
    Catalog(Catalog),
    MessageStart { key: String, phase: Option<String>, parent: Option<String> },
    TextDelta { key: String, text: String },
    /// The message is complete; `text` is the full text.
    MessageDone { key: String, text: String },
    ReasoningStart { key: String, parent: Option<String> },
    ReasoningDelta { key: String, text: String, estimated_tokens: Option<i64> },
    ReasoningDone { key: String },
    ToolStart { call_id: String, name: String, input: Value, parent: Option<String> },
    /// The tool's input is complete (it streams in pieces).
    ToolInput { call_id: String, input: Value },
    ToolResult { call_id: String, output: String, is_error: bool },
    /// A file the agent wrote: from a completed edit tool.
    FileChanged { path: String, call_id: String },
    Request(Request),
    Todos { todos: Value },
    Usage {
        context_tokens: Option<i64>,
        context_window: Option<i64>,
        cost_usd: Option<f64>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    },
    RateLimit { windows: Vec<RateWindow> },
    Notice { text: String },
    TurnEnd {
        status: TurnStatus,
        error: Option<String>,
        duration_ms: Option<i64>,
        cost_usd: Option<f64>,
    },
    /// A line the driver itself must send (an automatic protocol reply).
    Write(String),
}

/// What a turn is started with.
#[derive(Debug, Clone, Default)]
pub struct TurnInput {
    /// What the provider gets: chips and documents already expanded.
    pub text: String,
    pub images: Vec<ImageInput>,
    /// What the timeline shows when it differs (the typed text with its chips).
    pub display: Option<String>,
    pub skills: Vec<SkillRef>,
}

/// How a tool call is shown: a category for grouping ("Ran 3 commands") and
/// a one-line title (the command, the path, the pattern).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    /// `command` | `read` | `search` | `edit` | `web` | `agent` | `todo` | `mcp` | `tool`
    pub category: &'static str,
    pub title: String,
}

pub trait Driver: Send {
    fn provider(&self) -> &'static str;

    /// Category and title for a tool call of this provider.
    fn tool_info(&self, name: &str, input: &Value) -> ToolInfo;

    /// Lines to send right after the process started, before any prompt.
    fn handshake(&mut self) -> Vec<String>;

    /// Lines that start a turn with this user message.
    fn user_message(&mut self, input: &TurnInput) -> Vec<String>;

    /// One complete stdout line.
    fn on_line(&mut self, line: &str) -> Vec<Emit>;

    /// The reply to a pending request; an unknown request id is an error.
    fn answer(&mut self, request_id: &str, answer: &Answer) -> Result<Vec<String>, String>;

    /// Lines that ask the CLI to stop the running turn.
    fn interrupt(&mut self) -> Vec<String>;

    /// A line that switches the running process to another permission mode
    /// (`ask` | `edits` | `auto` | `plan` | `full`); `None` when the CLI
    /// cannot — the next turn then starts a fresh process with the mode.
    fn set_mode(&mut self, _mode: &str) -> Option<String> {
        None
    }

    /// A line that switches the running process to another model; `None`
    /// when the CLI cannot.
    fn set_model(&mut self, _model: &str) -> Option<String> {
        None
    }

    /// Whether a user message written while a turn runs joins that turn
    /// (steering). Claude Code does: verified 2026-10-29, one `result` and
    /// the agent answers "per your mid-turn message".
    fn can_steer(&self) -> bool {
        false
    }
}
