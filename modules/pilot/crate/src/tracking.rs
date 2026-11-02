//! What a turn wrote, captured file by file (parity card 01; PandaCode's
//! `pandacode_track_files`).
//!
//! A turn's changes are no longer "the turn's snapshot diff restricted to the
//! paths its edit tools named". Each written file gets a frozen *before* and
//! *after*:
//!
//! - **Tracked** — the agent calls `track_files` `begin` with the exact paths
//!   before a write batch (shell scripts included) and `complete` after it.
//!   The baseline is read at `begin`, the result at `complete`; nothing later
//!   leaks in.
//! - **Reported** — the provider's structured edit events (Claude Edit/Write,
//!   Codex fileChange, ACP, pi). The result is read the moment the event
//!   arrives; the baseline is the turn-start Git checkpoint. Outside Git
//!   there is no baseline, and the file is listed as partial instead of
//!   inventing a diff.
//!
//! What cannot be attributed with certainty is said, never hidden: another
//! thread writing the same file in the same window, a file that changed
//! between this turn's own writes, captures the agent never completed,
//! oversized or unreadable files. Nothing here touches the Git index, HEAD
//! or hooks; diffs come from `git diff --no-index` over temporary copies.

use crate::checkpoints::FileChange;
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Named paths per turn, bytes kept per file and per turn.
pub const MAX_PATHS: usize = 200;
pub const FILE_CAP: usize = 1024 * 1024;
pub const TURN_BYTES: usize = 32 * 1024 * 1024;
/// Open captures per turn (begin without complete).
const MAX_OPEN: usize = 64;
/// Saved diff-editor sides per turn (both sides of every file together).
pub const SIDES_TURN: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// The file did not exist.
    Missing,
    Bytes(Vec<u8>),
    /// Larger than [`FILE_CAP`]: its size only.
    TooLarge(u64),
    /// Could not be read (locked, a folder, permissions).
    Unreadable(String),
    /// No baseline exists (outside Git and not tracked).
    Unknown,
}

impl Content {
    fn size(&self) -> usize {
        match self {
            Content::Bytes(b) => b.len(),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    Tracked,
    Reported,
}

#[derive(Debug, Clone)]
struct PathState {
    abs: PathBuf,
    before: Content,
    after: Option<Content>,
    source: Source,
    notes: Vec<String>,
    /// A tracked capture holding this path is still open.
    open: Option<String>,
}

#[derive(Debug)]
struct TurnState {
    thread: String,
    root: PathBuf,
    /// The turn-start Git checkpoint, the baseline for reported writes.
    git_before: Option<String>,
    started_ms: i64,
    paths: BTreeMap<String, PathState>,
    open: HashMap<String, Vec<String>>,
    bytes: usize,
    notes: Vec<String>,
}

#[derive(Default)]
struct Registry {
    turns: HashMap<String, TurnState>,
    /// capture id → turn id
    captures: HashMap<String, String>,
    /// lowercased absolute path → (thread, turn, ms) of frozen writes
    writes: HashMap<String, Vec<(String, String, i64)>>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}

fn lock() -> std::sync::MutexGuard<'static, Registry> {
    registry().lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// A path inside `root`, as `(relative with forward slashes, absolute)`.
/// Parent steps, other drives and symlinked folders that leave the root are refused.
pub fn resolve(root: &Path, cwd: &Path, raw: &str) -> Result<(String, PathBuf), String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 1024 || raw.chars().any(char::is_control) {
        return Err("not a usable path".into());
    }
    let p = Path::new(raw);
    let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let mut norm = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                if !norm.pop() {
                    return Err("leaves the folder".into());
                }
            }
            Component::CurDir => {}
            other => norm.push(other.as_os_str()),
        }
    }
    let (r, a) = (key(root).trim_end_matches('/').to_string(), key(&norm));
    let rel_lower = a.strip_prefix(&format!("{r}/")).ok_or("is outside the thread's folder")?.to_string();
    if rel_lower.is_empty() {
        return Err("is the folder itself".into());
    }
    // A symlinked folder on the way may point elsewhere.
    if let (Ok(real_root), Some(parent)) = (std::fs::canonicalize(root), norm.parent()) {
        if let Ok(real_parent) = std::fs::canonicalize(parent) {
            if !key(&real_parent).starts_with(&key(&real_root)) {
                return Err("goes through a link that leaves the folder".into());
            }
        }
    }
    let start = norm.to_string_lossy().len() - rel_lower.len();
    Ok((norm.to_string_lossy()[start..].replace('\\', "/"), norm))
}

/// The file's bytes now.
pub fn read(abs: &Path) -> Content {
    match std::fs::metadata(abs) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Content::Missing,
        Err(e) => Content::Unreadable(e.to_string()),
        Ok(m) if m.is_dir() => Content::Unreadable("a folder".into()),
        Ok(m) if m.len() as usize > FILE_CAP => Content::TooLarge(m.len()),
        Ok(_) => match std::fs::read(abs) {
            Ok(b) if b.len() > FILE_CAP => Content::TooLarge(b.len() as u64),
            Ok(b) => Content::Bytes(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Content::Missing,
            Err(e) => Content::Unreadable(e.to_string()),
        },
    }
}

/// A turn starts: its root (repository root, or the thread's folder outside Git).
pub fn start_turn(thread: &str, turn: &str, root: &Path, git_before: Option<String>) {
    let mut r = lock();
    r.turns.insert(
        turn.to_string(),
        TurnState {
            thread: thread.to_string(),
            root: root.to_path_buf(),
            git_before,
            started_ms: now_ms(),
            paths: BTreeMap::new(),
            open: HashMap::new(),
            bytes: 0,
            notes: Vec::new(),
        },
    );
}

fn record_write(r: &mut Registry, abs: &Path, thread: &str, turn: &str) {
    let list = r.writes.entry(key(abs)).or_default();
    list.push((thread.to_string(), turn.to_string(), now_ms()));
    // Keep a bounded history per path.
    if list.len() > 32 {
        list.remove(0);
    }
}

/// `track_files` `begin`: baselines for the exact paths about to be written.
pub fn begin(thread: &str, cwd: &Path, paths: &[String]) -> Result<serde_json::Value, String> {
    let mut r = lock();
    let turn_id = r
        .turns
        .iter()
        .filter(|(_, t)| t.thread == thread)
        .max_by_key(|(_, t)| t.started_ms)
        .map(|(id, _)| id.clone())
        .ok_or("No turn is running in this thread, so there is nothing to attribute writes to.")?;
    if paths.is_empty() {
        return Err("Name the files you are about to write (relative to the thread's folder).".into());
    }
    let others: Vec<(String, String)> = r
        .captures
        .iter()
        .filter(|(_, t)| **t != turn_id)
        .filter_map(|(c, t)| r.turns.get(t).map(|ts| (c.clone(), ts.thread.clone())))
        .collect();
    let open_elsewhere: HashMap<String, String> = others
        .iter()
        .flat_map(|(c, thread)| {
            let t = r.captures.get(c).and_then(|t| r.turns.get(t));
            t.into_iter().flat_map(move |ts| {
                ts.open.get(c).into_iter().flatten().filter_map(move |rel| ts.paths.get(rel).map(|p| (key(&p.abs), thread.clone())))
            })
        })
        .collect();
    let turn = r.turns.get_mut(&turn_id).unwrap();
    if turn.open.len() >= MAX_OPEN {
        return Err(format!("{MAX_OPEN} captures are already open in this turn — complete them first."));
    }
    let capture = format!("cap-{}", uuid::Uuid::new_v4().simple());
    let mut tracked = Vec::new();
    let mut skipped = Vec::new();
    let root = turn.root.clone();
    for raw in paths {
        let (rel, abs) = match resolve(&root, cwd, raw) {
            Ok(x) => x,
            Err(why) => {
                skipped.push(serde_json::json!({ "path": raw, "reason": why }));
                continue;
            }
        };
        if tracked.contains(&rel) {
            continue;
        }
        if !turn.paths.contains_key(&rel) && turn.paths.len() >= MAX_PATHS {
            skipped.push(serde_json::json!({ "path": rel, "reason": format!("more than {MAX_PATHS} files in one turn") }));
            continue;
        }
        let now = read(&abs);
        let size = now.size();
        if turn.bytes + size > TURN_BYTES {
            skipped.push(serde_json::json!({ "path": rel, "reason": "the turn's capture budget is used up" }));
            continue;
        }
        turn.bytes += size;
        let elsewhere = open_elsewhere.get(&key(&abs)).cloned();
        match turn.paths.get_mut(&rel) {
            Some(state) => {
                if let Some(other) = &state.open {
                    skipped.push(serde_json::json!({ "path": rel, "reason": format!("already open in capture {other}") }));
                    continue;
                }
                // Written before in this turn: anything since is not ours.
                if state.after.as_ref().is_some_and(|a| *a != now) {
                    state.notes.push("changed outside this turn between its writes".into());
                }
                state.open = Some(capture.clone());
                if let Some(t) = elsewhere {
                    state.notes.push(format!("another thread ({t}) was writing it at the same time"));
                }
            }
            None => {
                let mut notes = Vec::new();
                if let Some(t) = elsewhere {
                    notes.push(format!("another thread ({t}) was writing it at the same time"));
                }
                turn.paths.insert(rel.clone(), PathState { abs, before: now, after: None, source: Source::Tracked, notes, open: Some(capture.clone()) });
            }
        }
        tracked.push(rel);
    }
    if tracked.is_empty() {
        return Err(format!("No file could be tracked: {}", serde_json::Value::Array(skipped)));
    }
    turn.open.insert(capture.clone(), tracked.clone());
    r.captures.insert(capture.clone(), turn_id);
    Ok(serde_json::json!({ "captureId": capture, "tracking": tracked, "skipped": skipped }))
}

/// `track_files` `complete`: freeze the files actually written. `edited`
/// names that subset (default: every tracked file).
pub fn complete(thread: &str, cwd: &Path, capture: &str, edited: Option<&[String]>) -> Result<serde_json::Value, String> {
    let mut r = lock();
    let turn_id = r.captures.get(capture).cloned().ok_or("Unknown or already completed capture id.")?;
    let (abs_written, changed, unchanged, dropped) = {
        let turn = r.turns.get_mut(&turn_id).ok_or("The turn of this capture has ended.")?;
        if turn.thread != thread {
            return Err("This capture belongs to another thread.".into());
        }
        let rels = turn.open.remove(capture).unwrap_or_default();
        let root = turn.root.clone();
        let wanted: Option<Vec<String>> = edited.map(|list| list.iter().filter_map(|p| resolve(&root, cwd, p).ok().map(|x| x.0)).collect());
        let (mut changed, mut unchanged, mut dropped, mut written) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for rel in rels {
            let Some(state) = turn.paths.get_mut(&rel) else { continue };
            state.open = None;
            let now = read(&state.abs);
            let named = wanted.as_ref().map_or(true, |list| list.iter().any(|p| p.eq_ignore_ascii_case(&rel)));
            let previous = state.after.clone().unwrap_or_else(|| state.before.clone());
            if !named {
                if now != previous {
                    // Changed, but not reported as this batch's write: left out.
                    state.notes.push("changed during a capture that did not report it".into());
                    dropped.push(rel.clone());
                }
                if state.after.is_none() {
                    turn.paths.remove(&rel);
                }
                continue;
            }
            if now == previous {
                unchanged.push(rel.clone());
                if state.after.is_none() {
                    turn.paths.remove(&rel);
                }
                continue;
            }
            turn.bytes += now.size();
            state.after = Some(now);
            state.source = Source::Tracked;
            written.push(state.abs.clone());
            changed.push(rel);
        }
        (written, changed, unchanged, dropped)
    };
    r.captures.remove(capture);
    let thread_s = thread.to_string();
    for abs in &abs_written {
        record_write(&mut r, abs, &thread_s, &turn_id);
    }
    Ok(serde_json::json!({ "changed": changed, "unchanged": unchanged, "leftOut": dropped }))
}

/// A provider's structured edit event: freeze the file as it is now.
/// `baseline` reads the turn-start version (Git) when the path is new to the turn.
pub fn reported(thread: &str, turn: &str, cwd: &Path, path: &str, baseline: impl FnOnce(&Path, &str, &str) -> Content) {
    let mut r = lock();
    let Some(ts) = r.turns.get_mut(turn) else { return };
    if ts.thread != thread {
        return;
    }
    let Ok((rel, abs)) = resolve(&ts.root, cwd, path) else { return };
    let now = read(&abs);
    match ts.paths.get_mut(&rel) {
        // A tracked capture is open: its `complete` freezes the file.
        Some(state) if state.open.is_some() => return,
        Some(state) => {
            ts.bytes += now.size();
            state.after = Some(now);
        }
        None => {
            if ts.paths.len() >= MAX_PATHS || ts.bytes + now.size() > TURN_BYTES {
                ts.notes.push(format!("{rel}: not captured, the turn's limits were reached"));
                return;
            }
            let before = match &ts.git_before {
                Some(commit) => baseline(&ts.root, commit, &rel),
                None => Content::Unknown,
            };
            ts.bytes += before.size() + now.size();
            ts.paths.insert(rel, PathState { abs: abs.clone(), before, after: Some(now), source: Source::Reported, notes: Vec::new(), open: None });
        }
    }
    let t = turn.to_string();
    record_write(&mut r, &abs, thread, &t);
}

/// One finished file of a turn.
#[derive(Debug, Clone, PartialEq)]
pub struct Captured {
    pub change: FileChange,
    pub patch: String,
    /// Both sides for the diff editor, when they are text and kept.
    pub sides: Option<(String, String)>,
}

/// A finished turn's capture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Finished {
    pub files: Vec<Captured>,
    /// Turn-level notes (incomplete captures, limits).
    pub notes: Vec<String>,
}

impl Finished {
    pub fn partial(&self) -> bool {
        !self.notes.is_empty() || self.files.iter().any(|f| f.change.note.is_some())
    }
}

/// The turn ended: diff every captured file and forget the turn.
pub fn finish(turn: &str) -> Option<Finished> {
    let (ts, overlaps) = {
        let mut r = lock();
        let ts = r.turns.remove(turn)?;
        let caps: Vec<String> = ts.open.keys().cloned().collect();
        for c in caps {
            r.captures.remove(&c);
        }
        let mut overlaps: HashMap<String, Vec<String>> = HashMap::new();
        for (rel, p) in &ts.paths {
            if let Some(list) = r.writes.get(&key(&p.abs)) {
                let others: Vec<String> = list
                    .iter()
                    .filter(|(th, tu, ms)| th != &ts.thread && tu != turn && *ms >= ts.started_ms)
                    .map(|(th, _, _)| th.clone())
                    .collect();
                if !others.is_empty() {
                    overlaps.insert(rel.clone(), others);
                }
            }
        }
        // Old write records no running turn can still ask about.
        let oldest = r.turns.values().map(|t| t.started_ms).min().unwrap_or(i64::MAX);
        r.writes.retain(|_, list| {
            list.retain(|(_, _, ms)| *ms >= oldest);
            !list.is_empty()
        });
        (ts, overlaps)
    };
    let mut out = Finished::default();
    if !ts.open.is_empty() {
        let n: usize = ts.open.values().map(Vec::len).sum();
        out.notes.push(format!("{} tracked capture(s) with {n} file(s) were never completed; those files are left out", ts.open.len()));
    }
    out.notes.extend(ts.notes.iter().cloned());
    let mut sides_budget = SIDES_TURN;
    for (rel, p) in ts.paths {
        let Some(after) = p.after else { continue };
        if p.before == after {
            continue;
        }
        let mut notes = p.notes.clone();
        if let Some(others) = overlaps.get(&rel) {
            let mut o = others.clone();
            o.dedup();
            notes.push(format!("another thread wrote it during this turn ({}) — attribution uncertain", o.join(", ")));
        }
        let mut captured = diff(&rel, &p.before, &after);
        captured.change.source = match p.source {
            Source::Tracked => "tracked".into(),
            Source::Reported => "reported".into(),
        };
        if let Some(n) = captured.change.note.take() {
            notes.insert(0, n);
        }
        notes.dedup();
        captured.change.note = (!notes.is_empty()).then(|| notes.join("; "));
        if let Some((a, b)) = &captured.sides {
            if a.len() + b.len() > sides_budget {
                captured.sides = None;
            } else {
                sides_budget -= a.len() + b.len();
            }
        }
        out.files.push(captured);
    }
    pair_renames(&mut out.files);
    let mut patches: Vec<String> = out.files.iter().map(|f| f.patch.clone()).collect();
    crate::checkpoints::within_budget(&mut patches);
    for (f, p) in out.files.iter_mut().zip(patches) {
        f.patch = p;
    }
    Some(out)
}

/// A deleted and an added file with the same bytes in one turn: a rename.
fn pair_renames(files: &mut Vec<Captured>) {
    let mut i = 0;
    while i < files.len() {
        if files[i].change.status == "D" {
            let gone = files[i].sides.as_ref().map(|s| s.0.clone());
            let j = files.iter().position(|f| f.change.status == "A" && gone.is_some() && f.sides.as_ref().map(|s| &s.1) == gone.as_ref());
            if let Some(j) = j {
                let from = files[i].change.path.clone();
                files[j].change.status = "R".into();
                files[j].change.from = Some(from);
                files[j].change.added = 0;
                files[j].change.removed = 0;
                files.remove(i);
                continue;
            }
        }
        i += 1;
    }
}

fn is_binary(b: &[u8]) -> bool {
    b.iter().take(8000).any(|c| *c == 0) || std::str::from_utf8(b).is_err()
}

/// One file's change between two frozen contents.
pub fn diff(rel: &str, before: &Content, after: &Content) -> Captured {
    let status = match (before, after) {
        (Content::Missing, Content::Missing) => "M",
        (Content::Missing, _) => "A",
        (_, Content::Missing) => "D",
        _ => "M",
    };
    let mut change = FileChange { path: rel.to_string(), status: status.into(), ..Default::default() };
    let side = |c: &Content| match c {
        Content::Missing => Some(Vec::new()),
        Content::Bytes(b) => Some(b.clone()),
        _ => None,
    };
    let reason = |c: &Content, which: &str| match c {
        Content::TooLarge(n) => Some(format!("{which} is larger than {} KiB ({} KiB), no diff kept", FILE_CAP / 1024, n / 1024)),
        Content::Unreadable(e) => Some(format!("{which} could not be read ({e})")),
        Content::Unknown => Some("no baseline: the folder is not a Git repository and the agent did not track this file".into()),
        _ => None,
    };
    let (Some(a), Some(b)) = (side(before), side(after)) else {
        change.note = reason(before, "the old version").or_else(|| reason(after, "the new version"));
        change.binary = matches!(before, Content::TooLarge(_)) || matches!(after, Content::TooLarge(_));
        return Captured { change, patch: String::new(), sides: None };
    };
    if is_binary(&a) || is_binary(&b) {
        change.binary = true;
        return Captured { change, patch: format!("Binary file {rel} changed\n"), sides: None };
    }
    let patch = match unified(rel, &a, &b, status) {
        Ok(p) => p,
        Err(e) => {
            change.note = Some(format!("no diff: {e}"));
            String::new()
        }
    };
    for line in patch.lines().skip_while(|l| !l.starts_with("@@")) {
        if line.starts_with('+') {
            change.added += 1;
        } else if line.starts_with('-') {
            change.removed += 1;
        }
    }
    let sides = Some((String::from_utf8_lossy(&a).into_owned(), String::from_utf8_lossy(&b).into_owned()));
    Captured { change, patch, sides }
}

/// `git diff --no-index` over temporary copies, with the headers naming the real path.
fn unified(rel: &str, a: &[u8], b: &[u8], status: &str) -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("qp-diff-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let (old, new) = (dir.join("old").join(rel), dir.join("new").join(rel));
        for (p, bytes) in [(&old, a), (&new, b)] {
            std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(p, bytes).map_err(|e| e.to_string())?;
        }
        let left = if status == "A" { "/dev/null".to_string() } else { format!("old/{rel}") };
        let right = if status == "D" { "/dev/null".to_string() } else { format!("new/{rel}") };
        let mut cmd = Command::new("git");
        cmd.args(["-c", "core.autocrlf=false", "-c", "core.safecrlf=false", "diff", "--no-index", "--no-color", "--no-ext-diff", "--no-prefix", "--", &left, &right])
            .current_dir(&dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().map_err(|e| format!("git could not be run ({e})"))?;
        // Exit 1 = differences found.
        if !out.status.success() && out.status.code() != Some(1) {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines: Vec<String> = Vec::new();
        let mut body = false;
        for line in text.lines() {
            if body || line.starts_with("@@") || line.starts_with("Binary") {
                body = true;
                lines.push(line.to_string());
                continue;
            }
            let fixed = if line.starts_with("diff --git ") {
                format!("diff --git a/{rel} b/{rel}")
            } else if let Some(rest) = line.strip_prefix("--- ") {
                if rest == "/dev/null" { line.to_string() } else { format!("--- a/{rel}") }
            } else if let Some(rest) = line.strip_prefix("+++ ") {
                if rest == "/dev/null" { line.to_string() } else { format!("+++ b/{rel}") }
            } else {
                line.to_string()
            };
            lines.push(fixed);
        }
        let mut s = lines.join("\n");
        if !s.is_empty() {
            s.push('\n');
        }
        Ok(s)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("qp-track-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        d
    }

    fn files(f: &Finished) -> Vec<(String, String, i64, i64)> {
        f.files.iter().map(|c| (c.change.path.clone(), c.change.status.clone(), c.change.added, c.change.removed)).collect()
    }

    #[test]
    fn a_tracked_shell_write_is_frozen_at_complete() {
        let d = dir();
        std::fs::write(d.join("src/a.txt"), "one\ntwo\n").unwrap();
        start_turn("th-shell", "tu-shell", &d, None);
        let cap = begin("th-shell", &d, &["src/a.txt".into(), "src/new.txt".into()]).unwrap();
        // "A shell script" writes both files.
        std::fs::write(d.join("src/a.txt"), "one\nTWO\n").unwrap();
        std::fs::write(d.join("src/new.txt"), "hello\n").unwrap();
        let done = complete("th-shell", &d, cap["captureId"].as_str().unwrap(), None).unwrap();
        assert_eq!(done["changed"].as_array().unwrap().len(), 2);
        // An editor changes the file after the capture: the saved diff does not move.
        std::fs::write(d.join("src/a.txt"), "edited later by someone else\n").unwrap();
        let f = finish("tu-shell").unwrap();
        assert_eq!(files(&f), vec![("src/a.txt".into(), "M".into(), 1, 1), ("src/new.txt".into(), "A".into(), 1, 0)]);
        assert!(f.files[0].patch.contains("+TWO") && !f.files[0].patch.contains("someone else"), "{}", f.files[0].patch);
        assert!(f.files[0].patch.starts_with("diff --git a/src/a.txt b/src/a.txt"));
        assert_eq!(f.files[0].change.source, "tracked");
        assert!(!f.partial());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn unrelated_and_unreported_edits_stay_out() {
        let d = dir();
        std::fs::write(d.join("src/a.txt"), "a\n").unwrap();
        std::fs::write(d.join("src/b.txt"), "b\n").unwrap();
        start_turn("th-out", "tu-out", &d, None);
        let cap = begin("th-out", &d, &["src/a.txt".into(), "src/b.txt".into()]).unwrap();
        std::fs::write(d.join("src/a.txt"), "a2\n").unwrap();
        std::fs::write(d.join("src/b.txt"), "b2\n").unwrap();
        std::fs::write(d.join("src/c.txt"), "never named\n").unwrap();
        let done = complete("th-out", &d, cap["captureId"].as_str().unwrap(), Some(&["src/a.txt".into()])).unwrap();
        assert_eq!(done["leftOut"][0], "src/b.txt", "changed but not reported: left out, and said");
        let f = finish("tu-out").unwrap();
        assert_eq!(files(&f), vec![("src/a.txt".into(), "M".into(), 1, 1)]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn gaps_overlaps_and_open_captures_are_marked() {
        let d = dir();
        std::fs::write(d.join("src/a.txt"), "1\n").unwrap();
        start_turn("th-gap", "tu-gap", &d, None);
        start_turn("th-other", "tu-other", &d, None);
        let c1 = begin("th-gap", &d, &["src/a.txt".into()]).unwrap();
        // The other thread opens the same file while ours is open.
        let c2 = begin("th-other", &d, &["src/a.txt".into()]).unwrap();
        std::fs::write(d.join("src/a.txt"), "2\n").unwrap();
        complete("th-gap", &d, c1["captureId"].as_str().unwrap(), None).unwrap();
        complete("th-other", &d, c2["captureId"].as_str().unwrap(), None).unwrap_or_default();
        // Someone edits it between our writes.
        std::fs::write(d.join("src/a.txt"), "3\n").unwrap();
        let c3 = begin("th-gap", &d, &["src/a.txt".into()]).unwrap();
        std::fs::write(d.join("src/a.txt"), "4\n").unwrap();
        complete("th-gap", &d, c3["captureId"].as_str().unwrap(), None).unwrap();
        // A capture never completed.
        begin("th-gap", &d, &["src/z.txt".into()]).unwrap();
        let f = finish("tu-gap").unwrap();
        let note = f.files[0].change.note.clone().unwrap_or_default();
        assert!(note.contains("between its writes"), "{note}");
        assert!(note.contains("another thread"), "{note}");
        assert!(f.notes.iter().any(|n| n.contains("never completed")), "{:?}", f.notes);
        assert!(f.partial());
        finish("tu-other");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn reported_writes_freeze_at_the_event_and_say_when_no_baseline_exists() {
        let d = dir();
        std::fs::write(d.join("src/a.txt"), "new content\n").unwrap();
        start_turn("th-rep", "tu-rep", &d, Some("BASE".into()));
        reported("th-rep", "tu-rep", &d, "src/a.txt", |_, commit, rel| {
            assert_eq!((commit, rel), ("BASE", "src/a.txt"));
            Content::Bytes(b"old content\n".to_vec())
        });
        // Duplicate event: no duplicate file.
        reported("th-rep", "tu-rep", &d, "src/a.txt", |_, _, _| unreachable!("baseline read once"));
        std::fs::write(d.join("src/a.txt"), "later\n").unwrap();
        let f = finish("tu-rep").unwrap();
        assert_eq!(files(&f), vec![("src/a.txt".into(), "M".into(), 1, 1)]);
        assert!(f.files[0].patch.contains("+new content"));
        assert_eq!(f.files[0].change.source, "reported");

        // Outside Git: listed, but partial instead of an invented diff.
        start_turn("th-ng", "tu-ng", &d, None);
        reported("th-ng", "tu-ng", &d, "src/a.txt", |_, _, _| unreachable!());
        let f = finish("tu-ng").unwrap();
        assert!(f.files[0].change.note.as_deref().unwrap().contains("not a Git repository"));
        assert!(f.partial());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn deletes_renames_and_escapes() {
        let d = dir();
        std::fs::write(d.join("src/old.txt"), "same\n").unwrap();
        std::fs::write(d.join("src/gone.txt"), "bye\n").unwrap();
        start_turn("th-mv", "tu-mv", &d, None);
        let cap = begin("th-mv", &d, &["src/old.txt".into(), "src/new.txt".into(), "src/gone.txt".into(), "../escape.txt".into()]).unwrap();
        assert_eq!(cap["skipped"][0]["path"], "../escape.txt");
        std::fs::rename(d.join("src/old.txt"), d.join("src/new.txt")).unwrap();
        std::fs::remove_file(d.join("src/gone.txt")).unwrap();
        complete("th-mv", &d, cap["captureId"].as_str().unwrap(), None).unwrap();
        let f = finish("tu-mv").unwrap();
        let st: Vec<(String, String, Option<String>)> = f.files.iter().map(|c| (c.change.path.clone(), c.change.status.clone(), c.change.from.clone())).collect();
        assert_eq!(st, vec![("src/gone.txt".into(), "D".into(), None), ("src/new.txt".into(), "R".into(), Some("src/old.txt".into()))]);
        assert!(begin("th-none", &d, &["x".into()]).is_err(), "no running turn");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn oversized_and_binary_files_keep_a_summary() {
        let d = dir();
        start_turn("th-big", "tu-big", &d, None);
        let cap = begin("th-big", &d, &["big.bin".into(), "img.bin".into()]).unwrap();
        std::fs::write(d.join("big.bin"), vec![b'x'; FILE_CAP + 10]).unwrap();
        std::fs::write(d.join("img.bin"), [0u8, 1, 2, 3]).unwrap();
        complete("th-big", &d, cap["captureId"].as_str().unwrap(), None).unwrap();
        let f = finish("tu-big").unwrap();
        let big = f.files.iter().find(|c| c.change.path == "big.bin").unwrap();
        assert!(big.change.note.as_deref().unwrap().contains("larger than"));
        let img = f.files.iter().find(|c| c.change.path == "img.bin").unwrap();
        assert!(img.change.binary && img.sides.is_none());
        let _ = std::fs::remove_dir_all(d);
    }
}
