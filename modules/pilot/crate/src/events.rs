//! V3's one vocabulary between the runtime and the webview
//! (docs/PLAN-QUANTPILOT.md §4.2).
//!
//! A thread's timeline is a list of [`Item`]s — user messages, assistant
//! messages, reasoning, tool calls, approvals, questions, plans, todos,
//! notices — each upserted whole when it changes, plus [`ThreadEvent::Delta`]
//! for the text that streams into an assistant message. Everything else is
//! thread-level state: the turn, the live state, usage, limits, the bound
//! provider session. Provider protocols never leak past `Emit` (what a
//! driver says) and this file (what the UI sees).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The Tauri event every V3 envelope rides on.
pub const EVENT_NAME: &str = "pilot-thread-event";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    User,
    Assistant,
    Reasoning,
    Tool,
    Approval,
    Question,
    Plan,
    Todo,
    Notice,
    Error,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::User => "user",
            ItemKind::Assistant => "assistant",
            ItemKind::Reasoning => "reasoning",
            ItemKind::Tool => "tool",
            ItemKind::Approval => "approval",
            ItemKind::Question => "question",
            ItemKind::Plan => "plan",
            ItemKind::Todo => "todo",
            ItemKind::Notice => "notice",
            ItemKind::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "user" => ItemKind::User,
            "assistant" => ItemKind::Assistant,
            "reasoning" => ItemKind::Reasoning,
            "tool" => ItemKind::Tool,
            "approval" => ItemKind::Approval,
            "question" => ItemKind::Question,
            "plan" => ItemKind::Plan,
            "todo" => ItemKind::Todo,
            "notice" => ItemKind::Notice,
            "error" => ItemKind::Error,
            _ => return None,
        })
    }
}

/// One entry of a thread's timeline. `data` depends on `kind`:
///
/// - `user`: `{ text, attachments: [] }`
/// - `assistant`: `{ text, phase?, streaming }`
/// - `reasoning`: `{ text, estimatedTokens?, streaming }`
/// - `tool`: `{ callId, name, category, title, input, output?, status, isError }`
///   — `status` = `running` | `completed` | `failed` | `denied`
/// - `approval`: `{ requestId, toolName, category, title, input, status, reply? }`
///   — `status` = `pending` | `allowed` | `allowedSession` | `denied` | `expired`
/// - `question`: `{ requestId, questions: [{ question, header, options, multiSelect }], status, answers? }`
/// - `plan`: `{ requestId, plan, status }` — `pending` | `approved` | `rejected` | `expired`
/// - `todo`: `{ todos: [{ content, status, activeForm? }] }`
/// - `notice` / `error`: `{ text }`
///
/// `parent` is the tool call that started a subagent, for its own items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub thread_id: String,
    pub turn_id: String,
    /// Timeline order within the thread.
    pub seq: i64,
    pub kind: ItemKind,
    pub parent: Option<String>,
    pub data: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    /// `five_hour`, `seven_day`, … as the provider names them.
    pub kind: String,
    /// 0–100, how much is used.
    pub used_pct: Option<f64>,
    /// Unix milliseconds.
    pub resets_at: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ThreadEvent {
    /// Create or replace one timeline item.
    Item { item: Item },
    /// Text appended to a streaming assistant or reasoning item.
    Delta { item_id: String, text: String },
    /// `started` | `completed` | `failed` | `interrupted` | `limited`
    Turn {
        turn_id: String,
        status: String,
        duration_ms: Option<i64>,
        cost_usd: Option<f64>,
        error: Option<String>,
    },
    /// `idle` | `starting` | `running` | `waiting` | `error`
    State { state: String, detail: Option<String> },
    Session {
        provider: String,
        provider_session_id: String,
        model: Option<String>,
    },
    Usage {
        context_tokens: Option<i64>,
        context_window: Option<i64>,
        cost_usd: Option<f64>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    },
    RateLimit { windows: Vec<RateWindow> },
    Title { title: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<'a> {
    pub thread_id: &'a str,
    /// Per-thread, monotonically increasing across runs; a snapshot carries
    /// the seq it reflects so the webview drops older live events.
    pub seq: u64,
    pub event: &'a ThreadEvent,
}

/// UTF-8-safe truncation for previews: the first `max` bytes, cut at a
/// character boundary, with a note when something was left out.
pub fn bounded(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… {} more bytes", &text[..end], text.len() - end)
}

/// Head and tail of a long output — the end of a command's output is
/// usually what matters, the start says what it was.
pub fn bounded_output(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let head_len = max / 4;
    let tail_len = max - head_len;
    let mut head_end = head_len;
    while head_end > 0 && !text.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = text.len() - tail_len;
    while tail_start < text.len() && !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let omitted = tail_start - head_end;
    format!("{}\n… {omitted} bytes omitted …\n{}", &text[..head_end], &text[tail_start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_cuts_at_char_boundaries() {
        let s = "äöü".repeat(10);
        let b = bounded(&s, 5);
        assert!(b.starts_with("äö"));
        assert!(b.contains("more bytes"));
        assert_eq!(bounded("short", 10), "short");
    }

    #[test]
    fn bounded_output_keeps_head_and_tail() {
        let s: String = (0..1000).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let b = bounded_output(&s, 100);
        assert!(b.starts_with(&s[..25]));
        assert!(b.ends_with(&s[s.len() - 75..]));
        assert!(b.contains("900 bytes omitted"));
    }

    #[test]
    fn events_serialize_with_camel_case_tags() {
        let e = ThreadEvent::Delta { item_id: "a".into(), text: "hi".into() };
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"type":"delta","itemId":"a","text":"hi"}"#);
    }
}
