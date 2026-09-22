//! Watches the vault for edits made outside the suite — Obsidian, an editor,
//! a sync client. Events are debounced into one rescan per quiet gap, and
//! the module's own writes are filtered out so a save through QuantMemory
//! does not come back around as an "external" change.

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Paths this process wrote recently, keyed case-insensitively — Windows
/// paths arrive from notify in whatever casing the OS feels like.
#[derive(Clone, Default)]
pub struct SelfWrites(Arc<Mutex<HashMap<String, Instant>>>);

const SELF_WRITE_WINDOW: Duration = Duration::from_secs(3);
const DEBOUNCE: Duration = Duration::from_millis(500);

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase().replace('\\', "/")
}

impl SelfWrites {
    /// Call right before (or after) writing a vault file yourself.
    pub fn mark(&self, path: &Path) {
        if let Ok(mut map) = self.0.lock() {
            let now = Instant::now();
            map.retain(|_, t| now.duration_since(*t) < SELF_WRITE_WINDOW);
            map.insert(path_key(path), now);
        }
    }

    fn contains(&self, path: &Path) -> bool {
        self.0
            .lock()
            .map(|map| {
                map.get(&path_key(path))
                    .is_some_and(|t| t.elapsed() < SELF_WRITE_WINDOW)
            })
            .unwrap_or(false)
    }
}

/// Keeps the underlying OS watcher alive; dropping it stops the thread.
pub struct WatcherHandle {
    _watcher: RecommendedWatcher,
}

/// True for an event the index cares about: something under the vault that
/// is not hidden (`.trash`, `.obsidian`, …) and not our own write echoing.
fn relevant(event: &notify::Event, vault: &Path, self_writes: &SelfWrites) -> bool {
    event.paths.iter().any(|path| {
        if self_writes.contains(path) {
            return false;
        }
        let rel = path.strip_prefix(vault).unwrap_or(path);
        if rel
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            return false;
        }
        // Directory ops carry no extension; they may move many files at once,
        // so they count. Everything else must be markdown.
        match path.extension() {
            Some(ext) => ext.eq_ignore_ascii_case("md"),
            None => true,
        }
    })
}

/// Watch `vault` recursively and call `on_change` once per quiet gap after
/// relevant events. The callback runs on the watcher thread — it should do
/// its own locking and never panic.
pub fn start(
    vault: PathBuf,
    self_writes: SelfWrites,
    on_change: Box<dyn Fn() + Send>,
) -> Result<WatcherHandle, String> {
    let (tx, rx) = channel();
    let mut watcher =
        notify::recommended_watcher(tx).map_err(|e| format!("Create watcher: {e}"))?;
    watcher
        .watch(&vault, RecursiveMode::Recursive)
        .map_err(|e| format!("Watch {}: {e}", vault.display()))?;

    std::thread::Builder::new()
        .name("qm-vault-watch".into())
        .spawn(move || {
            let mut dirty = false;
            loop {
                match rx.recv_timeout(DEBOUNCE) {
                    Ok(Ok(event)) => {
                        if relevant(&event, &vault, &self_writes) {
                            dirty = true;
                        }
                    }
                    Ok(Err(_)) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        if dirty {
                            dirty = false;
                            on_change();
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|e| format!("Spawn watcher thread: {e}"))?;

    Ok(WatcherHandle { _watcher: watcher })
}
