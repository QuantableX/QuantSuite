//! The typed event bus — the integration substrate between modules
//! (ARCHITECTURE.md §3).
//!
//! Delivery is best-effort and non-transactional. The bus is for coordination,
//! not state transfer: anything that must survive a crash goes into a database
//! first and is announced here afterwards.

use crate::db::{self, Db};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The single channel every webview listens on.
pub const CHANNEL: &str = "qs://event";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    /// `<domain>.<entity>.<verb>`
    pub topic: String,
    /// Module id, or `core`.
    pub source: String,
    /// Unix millis.
    pub ts: i64,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

impl Event {
    pub fn new(topic: impl Into<String>, source: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            topic: topic.into(),
            source: source.into(),
            ts: db::now_ms(),
            payload,
            correlation_id: None,
        }
    }
}

type Handler = Box<dyn Fn(&Event) + Send + Sync + 'static>;

#[derive(Default)]
pub struct Bus {
    subscribers: RwLock<HashMap<String, Vec<Handler>>>,
    /// Topics appended to `event_log`. Kept small on purpose — the log is for
    /// diagnosis, not for replay.
    persisted: RwLock<HashSet<String>>,
}

impl Bus {
    pub fn new() -> Self {
        let bus = Self::default();
        for t in [
            "core.process.panicked",
            "mcp.tool.invoked",
            "algo.trade.executed",
            "systems.backtest.completed",
            "algo.backtest.completed",
        ] {
            bus.persist(t);
        }
        bus
    }

    /// Register a Rust-side handler. Module plugins call this in their `setup`.
    pub fn subscribe(&self, topic: impl Into<String>, handler: Handler) {
        self.subscribers
            .write()
            .expect("bus subscribers poisoned")
            .entry(topic.into())
            .or_default()
            .push(handler);
    }

    pub fn persist(&self, topic: impl Into<String>) {
        self.persisted.write().expect("bus persisted poisoned").insert(topic.into());
    }

    fn should_persist(&self, topic: &str) -> bool {
        self.persisted.read().map(|p| p.contains(topic)).unwrap_or(false)
    }

    fn dispatch_local(&self, event: &Event) {
        if let Ok(subs) = self.subscribers.read() {
            if let Some(handlers) = subs.get(&event.topic) {
                for h in handlers {
                    h(event);
                }
            }
        }
    }
}

/// A topic must be `<domain>.<entity>.<verb>` — at least two dots, no spaces,
/// lowercase. Validated so a typo surfaces immediately instead of silently
/// never matching a subscriber.
pub fn valid_topic(topic: &str) -> bool {
    let parts: Vec<&str> = topic.split('.').collect();
    parts.len() >= 3
        && parts.iter().all(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

/// Publish an event: Rust subscribers first, then every webview, then the log.
pub fn publish<R: Runtime>(app: &AppHandle<R>, event: Event) -> Result<(), String> {
    if !valid_topic(&event.topic) {
        return Err(format!(
            "invalid topic '{}' — expected <domain>.<entity>.<verb>, lowercase",
            event.topic
        ));
    }

    if let Some(bus) = app.try_state::<Bus>() {
        bus.dispatch_local(&event);

        if bus.should_persist(&event.topic) {
            if let Some(dbs) = app.try_state::<Db>() {
                if let Ok(conn) = dbs.0.lock() {
                    let _ = db::append_event(&conn, &event.id, &event.topic, &event.source, event.ts, &event.payload);
                }
            }
        }
    }

    app.emit(CHANNEL, &event).map_err(|e| e.to_string())
}

/// Convenience for core-sourced events.
pub fn emit<R: Runtime>(app: &AppHandle<R>, topic: &str, payload: serde_json::Value) -> Result<(), String> {
    publish(app, Event::new(topic, "core", payload))
}
