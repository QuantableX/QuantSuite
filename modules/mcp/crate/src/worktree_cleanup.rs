//! Durable retry of an already-authorized removal. Only unregistered remnants
//! directly inside .qs-worktrees qualify. Never discover arbitrary folders and
//! infer permission to delete them from age or a missing Git entry.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

static QUEUE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: u128,
    directory: bool,
}

#[derive(Deserialize, Serialize)]
struct Pending {
    repo: PathBuf,
    path: PathBuf,
    created: u128,
    entries: BTreeMap<PathBuf, Stamp>,
    last_error: String,
}

fn err(e: impl std::fmt::Display) -> String { e.to_string() }

fn queue_dir() -> PathBuf {
    qs_core::paths::module_dir("mcp").join("pending-worktree-cleanup")
}

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    { meta.file_type().is_symlink() }
}

fn timestamp(time: std::io::Result<std::time::SystemTime>) -> Result<u128, String> {
    Ok(time.map_err(err)?.duration_since(UNIX_EPOCH).map_err(err)?.as_nanos())
}

fn snapshot(root: &Path, dir: &Path, entries: &mut BTreeMap<PathBuf, Stamp>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(err)? {
        let path = entry.map_err(err)?.path();
        let meta = fs::symlink_metadata(&path).map_err(err)?;
        if is_link(&meta) || (!meta.is_file() && !meta.is_dir()) {
            return Err(format!("Linked or unsupported path retained: {}", path.display()));
        }
        entries.insert(path.strip_prefix(root).map_err(err)?.to_path_buf(), Stamp {
            bytes: if meta.is_dir() { 0 } else { meta.len() },
            // Directory timestamps change when a partial delete removes files.
            modified: if meta.is_dir() { 0 } else { timestamp(meta.modified())? },
            directory: meta.is_dir(),
        });
        if meta.is_dir() { snapshot(root, &path, entries)?; }
    }
    Ok(())
}

fn validate(repo: &Path, path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(err)?;
    if is_link(&meta) || !meta.is_dir() {
        return Err("Cleanup target is no longer an ordinary directory".into());
    }
    let repo = fs::canonicalize(repo).map_err(err)?;
    let path = fs::canonicalize(path).map_err(err)?;
    let parent = path.parent().ok_or("Missing cleanup parent")?;
    // Compare the canonical path to the literal child of the canonical repo:
    // a replaced .qs-worktrees junction cannot redirect deletion elsewhere.
    if parent != repo.join(super::worktree::DIR) {
        return Err("Cleanup target is outside this repository's .qs-worktrees directory".into());
    }
    if fs::symlink_metadata(path.join(".git")).is_ok() {
        return Err("Worktree has Git metadata; automatic cleanup paused".into());
    }
    let registered = super::worktree::run(&repo, &["worktree", "list", "--porcelain"])?;
    for line in registered.lines().filter_map(|l| l.strip_prefix("worktree ")) {
        if fs::canonicalize(line).is_ok_and(|p| p == path) {
            return Err("Worktree is registered; automatic cleanup paused".into());
        }
    }
    Ok(())
}

fn persist(file: &Path, pending: &Pending) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(pending).map_err(err)?;
    qs_core::paths::write_atomic(file, &bytes).map_err(err)
}

fn enqueue_in(queue: &Path, repo: &Path, path: &Path, reason: &str) -> Result<(), String> {
    validate(repo, path)?;
    let repo = fs::canonicalize(repo).map_err(err)?;
    let path = fs::canonicalize(path).map_err(err)?;
    let created = timestamp(fs::metadata(&path).map_err(err)?.created())?;
    let mut entries = BTreeMap::new();
    snapshot(&path, &path, &mut entries)?;
    fs::create_dir_all(queue).map_err(err)?;
    let id = qs_core::workspaces::workspace_id_for(&path.to_string_lossy());
    let file = queue.join(format!("{}.json", id.rsplit(':').next().unwrap()));
    persist(&file, &Pending { repo, path, created, entries, last_error: reason.into() })
}

pub(super) fn enqueue(repo: &Path, path: &Path, reason: &str) -> Result<(), String> {
    let _guard = QUEUE_LOCK.lock().map_err(err)?;
    enqueue_in(&queue_dir(), repo, path, reason)
}

fn retry_one(pending: &mut Pending) -> Result<bool, String> {
    match fs::symlink_metadata(&pending.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(err(e)),
        Ok(meta) => {
            if timestamp(meta.created())? != pending.created {
                return Err("Cleanup directory was replaced; automatic cleanup paused".into());
            }
        }
    }
    validate(&pending.repo, &pending.path)?;
    let mut current = BTreeMap::new();
    snapshot(&pending.path, &pending.path, &mut current)?;
    if current.iter().any(|(path, stamp)| pending.entries.get(path) != Some(stamp)) {
        return Err("Files were added or changed after removal; automatic cleanup paused".into());
    }
    // A subset is allowed: Git or a previous attempt may have removed some
    // files before encountering a Windows sharing violation.
    fs::remove_dir_all(&pending.path).map_err(err)?;
    Ok(true)
}

fn retry_in(queue: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    let Ok(files) = fs::read_dir(queue) else { return notes };
    for file in files.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")) {
        let result = fs::read(&file).map_err(err).and_then(|bytes| serde_json::from_slice::<Pending>(&bytes).map_err(err));
        let mut pending = match result {
            Ok(p) => p,
            Err(e) => { notes.push(format!("Cannot read cleanup record {}: {e}", file.display())); continue; }
        };
        match retry_one(&mut pending) {
            Ok(true) => {
                if let Err(e) = fs::remove_file(&file) { notes.push(err(e)); }
                notes.push(format!("Removed leftover worktree folder {}", pending.path.display()));
            }
            Ok(false) => {}
            Err(e) => {
                if pending.last_error != e {
                    notes.push(format!("Cleanup pending for {}: {e}", pending.path.display()));
                    pending.last_error = e;
                    if let Err(e) = persist(&file, &pending) { notes.push(e); }
                }
            }
        }
    }
    notes
}

pub(super) fn retry() {
    let Ok(_guard) = QUEUE_LOCK.lock() else { return };
    for note in retry_in(&queue_dir()) { eprintln!("mcp: {note}"); }
}

pub(super) fn notes(repo: &str) -> Vec<String> {
    let Ok(_guard) = QUEUE_LOCK.lock() else { return Vec::new() };
    let Ok(repo) = fs::canonicalize(repo) else { return Vec::new() };
    let Ok(files) = fs::read_dir(queue_dir()) else { return Vec::new() };
    files.flatten().filter_map(|entry| {
        let bytes = fs::read(entry.path()).ok()?;
        let pending: Pending = serde_json::from_slice(&bytes).ok()?;
        (pending.repo == repo).then(|| format!("Cleanup pending: {} - {}. Retried each minute and at startup; modified or registered folders are retained.", pending.path.display(), pending.last_error))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("qs-cleanup-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repo");
        let path = repo.join(".qs-worktrees/finished");
        fs::create_dir_all(&path).unwrap();
        super::super::worktree::run(&repo, &["init", "-q"]).unwrap();
        fs::write(path.join("leftover.txt"), "old build").unwrap();
        (root.clone(), repo, root.join("queue"))
    }

    #[test]
    fn persisted_retry_cleans_only_unchanged_unregistered_remnants() {
        let (root, repo, queue) = fixture();
        let path = repo.join(".qs-worktrees/finished");
        enqueue_in(&queue, &repo, &path, "locked").unwrap();
        // A later run reads the persisted record, not process memory.
        retry_in(&queue);
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_or_reused_folders_and_outside_paths_are_preserved() {
        let (root, repo, queue) = fixture();
        let path = repo.join(".qs-worktrees/finished");
        enqueue_in(&queue, &repo, &path, "locked").unwrap();
        fs::write(path.join("new-work.txt"), "keep me").unwrap();
        retry_in(&queue);
        assert_eq!(fs::read_to_string(path.join("new-work.txt")).unwrap(), "keep me");
        fs::remove_file(path.join("new-work.txt")).unwrap();
        fs::write(path.join(".git"), "new worktree metadata").unwrap();
        retry_in(&queue);
        assert!(path.join("leftover.txt").exists());
        assert!(enqueue_in(&queue, &repo, &repo, "must reject main").is_err());
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        assert!(enqueue_in(&queue, &repo, &outside, "must reject external").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn windows_file_lock_is_retried_after_the_handle_closes() {
        use std::os::windows::fs::OpenOptionsExt;
        let (root, repo, queue) = fixture();
        let path = repo.join(".qs-worktrees/finished");
        let held = fs::OpenOptions::new().read(true).share_mode(3)
            .open(path.join("leftover.txt")).unwrap();
        enqueue_in(&queue, &repo, &path, "initial lock").unwrap();
        retry_in(&queue);
        assert!(path.join("leftover.txt").exists());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 1);
        drop(held);
        retry_in(&queue);
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn directory_junctions_are_never_queued_for_recursive_deletion() {
        let (root, repo, queue) = fixture();
        let path = repo.join(".qs-worktrees/finished");
        let shared = root.join("shared");
        fs::create_dir(&shared).unwrap();
        fs::write(shared.join("keep.txt"), "shared data").unwrap();
        let link = path.join("node_modules");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(shared.to_string_lossy().replace('/', "\\")).output().unwrap();
        assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
        assert!(enqueue_in(&queue, &repo, &path, "linked").is_err());
        assert_eq!(fs::read_to_string(shared.join("keep.txt")).unwrap(), "shared data");
        fs::remove_dir(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

}
