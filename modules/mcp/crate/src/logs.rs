use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogDirection {
    In,
    Out,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: u64,
    pub direction: LogDirection,
    pub source: String,
    pub source_type: String,
    pub content: String,
}

pub struct LogStore {
    entries: VecDeque<LogEntry>,
    max_entries: usize,
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: 1000,
        }
    }

    pub fn add(
        &mut self,
        direction: LogDirection,
        source: String,
        source_type: String,
        content: String,
    ) {
        let entry = LogEntry {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            direction,
            source,
            source_type,
            content,
        };
        self.entries.push_back(entry);
        if self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    pub fn list(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Entries newer than `since_id`. Ids are monotonic, so a poll can fetch
    /// just the tail instead of cloning the whole store every time.
    pub fn list_since(&self, since_id: u64) -> Vec<LogEntry> {
        self.entries
            .iter()
            .skip_while(|entry| entry.id <= since_id)
            .cloned()
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
