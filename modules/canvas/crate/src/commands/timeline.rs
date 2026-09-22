//! Local save history — QuantCode's Timeline panel.
//!
//! Every successful save drops a snapshot of the file's content under
//! `~/.quantsuite/modules/canvas/timeline/<key>/`, where `<key>` is derived
//! from the absolute path. Git answers "what did the last commit look like";
//! this answers "what did this file look like twenty minutes ago", which git
//! cannot. Snapshots are plain files named `<unix-millis>-<content-hash>.snap`
//! so listing needs no index and deduping needs no read.

use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many snapshots one file keeps. Beyond this the oldest are pruned —
/// the panel is a safety net for the last few hours, not an archive.
const MAX_SNAPSHOTS: usize = 50;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    /// The snapshot's file name — the id `timeline_read` wants back.
    pub id: String,
    /// Unix milliseconds of the save.
    pub saved_at: i64,
    pub bytes: u64,
}

fn hash64(value: &str) -> String {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The directory holding one file's snapshots. A readable tail plus a hash:
/// the tail makes the directory identifiable in a file manager, the hash
/// makes it unique.
fn dir_for(path: &str) -> PathBuf {
    let normalized = path.replace('\\', "/").to_lowercase();
    let tail: String = normalized
        .rsplit('/')
        .next()
        .unwrap_or("file")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .take(40)
        .collect();
    qs_core::paths::module_dir("canvas")
        .join("timeline")
        .join(format!("{}-{}", tail, hash64(&normalized)))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Snapshot file names, newest first. The name sorts by time because it
/// starts with zero-padded millis.
fn list_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .filter(|n| n.ends_with(".snap"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.reverse();
    names
}

// ---------------------------------------------------------------------------
// timeline_snapshot
// ---------------------------------------------------------------------------

/// Record a save. Returns whether a snapshot was written — a save that did
/// not change the content against the latest snapshot writes nothing.
#[tauri::command(async)]
pub fn timeline_snapshot(path: String, content: String) -> Result<bool, String> {
    let dir = dir_for(&path);
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create timeline dir: {}", e))?;

    let content_hash = hash64(&content);
    let names = list_names(&dir);

    // The latest snapshot already holds this exact content — a re-save.
    if let Some(latest) = names.first() {
        if latest.contains(&content_hash) {
            return Ok(false);
        }
    }

    let name = format!("{:013}-{}.snap", now_millis(), content_hash);
    fs::write(dir.join(&name), content).map_err(|e| format!("Cannot write snapshot: {}", e))?;

    // Prune the oldest beyond the cap. `names` predates the write, so the cap
    // is applied to the new count.
    for stale in names.iter().skip(MAX_SNAPSHOTS - 1) {
        let _ = fs::remove_file(dir.join(stale));
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// timeline_list
// ---------------------------------------------------------------------------

/// The file's snapshots, newest first. A file never saved here lists empty.
#[tauri::command(async)]
pub fn timeline_list(path: String) -> Result<Vec<TimelineEntry>, String> {
    let dir = dir_for(&path);
    let mut out = Vec::new();
    for name in list_names(&dir) {
        let saved_at: i64 = name.split('-').next().and_then(|m| m.parse().ok()).unwrap_or(0);
        let bytes = fs::metadata(dir.join(&name)).map(|m| m.len()).unwrap_or(0);
        out.push(TimelineEntry { id: name, saved_at, bytes });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// timeline_read
// ---------------------------------------------------------------------------

/// One snapshot's content. `id` comes from `timeline_list` — it is a bare
/// file name, and anything shaped like a path is refused.
#[tauri::command(async)]
pub fn timeline_read(path: String, id: String) -> Result<String, String> {
    if id.contains('/') || id.contains('\\') || id.contains("..") || !id.ends_with(".snap") {
        return Err(format!("Not a snapshot id: {}", id));
    }
    fs::read_to_string(dir_for(&path).join(&id)).map_err(|e| format!("Cannot read snapshot: {}", e))
}
