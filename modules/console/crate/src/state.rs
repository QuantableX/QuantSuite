use qs_pty::PtyRegistry;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// What the frontend knows about a session besides its bytes.
///
/// Kept beside the PTY registry rather than inside it: `qs-pty` deliberately
/// knows nothing about shells or titles (docs/PLAN-CONSOLE.md §4).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    /// Which surface opened it: `console` for a tab, `canvas` for a canvas
    /// window. The console's tab bar adopts running sessions on a reload —
    /// without this it would also adopt every canvas terminal and grow a tab
    /// for a shell the user opened somewhere else entirely.
    pub owner: String,
    pub shell: String,
    /// The directory the session was opened in. Plain terminals do not track
    /// the shell's later `cd`s — the shell's own prompt shows where it is.
    pub cwd: String,
    /// Unix millis.
    pub started_at: i64,
    /// Set when the child has exited. The session stays listed so its output
    /// remains readable — closing it is the user's call.
    pub exited: bool,
}

pub struct ConsoleState {
    /// The PTYs, shared implementation with QuantCanvas.
    pub ptys: PtyRegistry,
    /// id -> metadata. A separate lock from the registry's: metadata reads must
    /// never queue behind a blocking write to some other session's PTY.
    pub meta: Mutex<HashMap<String, SessionMeta>>,
}

impl ConsoleState {
    pub fn new() -> Self {
        Self {
            ptys: PtyRegistry::new(),
            meta: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert_meta(&self, meta: SessionMeta) {
        if let Ok(mut map) = self.meta.lock() {
            map.insert(meta.id.clone(), meta);
        }
    }

    pub fn mark_exited(&self, id: &str) {
        if let Ok(mut map) = self.meta.lock() {
            if let Some(meta) = map.get_mut(id) {
                meta.exited = true;
            }
        }
    }

    pub fn remove_meta(&self, id: &str) -> Option<SessionMeta> {
        self.meta.lock().ok().and_then(|mut map| map.remove(id))
    }

    /// Sessions oldest first, so the sidebar order matches the order they were
    /// opened in rather than hash order.
    pub fn sessions(&self) -> Vec<SessionMeta> {
        let mut list: Vec<SessionMeta> = self
            .meta
            .lock()
            .map(|map| map.values().cloned().collect())
            .unwrap_or_default();
        list.sort_by_key(|s| s.started_at);
        list
    }

    /// Kill every PTY. Registered as a shutdown hook: the suite window closes to
    /// tray, so a shell left running here would outlive the visible app.
    pub fn close_all(&self) {
        self.ptys.close_all("console");
        if let Ok(mut map) = self.meta.lock() {
            map.clear();
        }
    }
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self::new()
    }
}
