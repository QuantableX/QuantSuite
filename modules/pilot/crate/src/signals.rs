//! The one vocabulary the face and the panel read, whatever produced it:
//! Claude's hooks, a Codex rollout file, a pi session file, or nothing at
//! all (then the UI falls back to output activity and the bell).
//!
//! Small on purpose. A signal says what the agent is doing, which tool it
//! runs, which file it touched, what a turn cost — never what it said.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Signal {
    /// The user typed an agent's command in the session's terminal; the
    /// wrapper reported it before starting the CLI.
    Launched { adapter: String },
    /// The provider's own session id is known (read back from its files, or
    /// confirmed by its first hook).
    Bound {
        provider_session_id: String,
        #[serde(default)]
        model: Option<String>,
    },
    /// `thinking` | `working` | `waiting` | `idle` | `offline` | `error`
    State {
        state: String,
        #[serde(default)]
        detail: Option<String>,
    },
    Tool {
        name: String,
        title: String,
        done: bool,
        #[serde(default)]
        is_error: bool,
    },
    /// A file the agent wrote or edited.
    File { path: String },
    /// `started` | `completed`
    Turn { status: String },
    Vitals {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        cost_usd: Option<f64>,
        /// Context window fill, 0–100.
        #[serde(default)]
        context_pct: Option<f64>,
        #[serde(default)]
        lines_added: Option<i64>,
        #[serde(default)]
        lines_removed: Option<i64>,
        #[serde(default)]
        input_tokens: Option<i64>,
        #[serde(default)]
        output_tokens: Option<i64>,
    },
    /// The CLI named the session itself.
    Title { title: String },
    Notice { text: String },
    /// The process is gone.
    Exited,
}

/// Where signals go: `(session id, signal)`.
pub type Sink = Arc<dyn Fn(&str, Signal) + Send + Sync>;

pub fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_tagged_camel_case() {
        let v = serde_json::to_value(Signal::Vitals {
            model: Some("opus".into()),
            cost_usd: Some(0.5),
            context_pct: None,
            lines_added: None,
            lines_removed: None,
            input_tokens: None,
            output_tokens: None,
        })
        .unwrap();
        assert_eq!(v["type"], "vitals");
        assert_eq!(v["costUsd"], 0.5);
        let back: Signal = serde_json::from_value(serde_json::json!({ "type": "file", "path": "a.rs" })).unwrap();
        assert_eq!(back, Signal::File { path: "a.rs".into() });
    }

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("hello", 10), "hello");
        assert_eq!(clip("hello world", 6), "hello…");
    }
}
