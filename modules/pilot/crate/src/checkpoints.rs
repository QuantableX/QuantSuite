//! Checkpoints and turn changes (card 11; T3 Code's hidden-ref checkpoints,
//! PandaCode's "only the task's own writes").
//!
//! Before and after every turn the checkout is snapshotted into a commit
//! that no branch points at: a temporary index (`GIT_INDEX_FILE`) takes
//! `HEAD` plus every tracked and untracked, not ignored file, `write-tree`
//! and `commit-tree` make the commit, and `refs/quantpilot/checkpoints/
//! <thread>/<turn>-before|after` keeps it. The user's branch, HEAD and
//! index are never touched. A turn's changes are the diff between its two
//! snapshots, restricted to the files its tools wrote. Reverting files
//! restores a snapshot into the working tree only (`git restore
//! --worktree`), deleting what was added since.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Saved patch text per file and per turn (the rest is cut, and said so).
pub const PATCH_FILE: usize = 256 * 1024;
pub const PATCH_TURN: usize = 2 * 1024 * 1024;
/// A side of a file shown in the diff editor is cut beyond this.
pub const CONTENT_CAP: usize = 1024 * 1024;
const REF_ROOT: &str = "refs/quantpilot/checkpoints";

fn git_env(cwd: &Path, args: &[&str], index: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "QuantPilot")
        .env("GIT_AUTHOR_EMAIL", "quantpilot@localhost")
        .env("GIT_COMMITTER_NAME", "QuantPilot")
        .env("GIT_COMMITTER_EMAIL", "quantpilot@localhost")
        .stdin(Stdio::null());
    if let Some(i) = index {
        cmd.env("GIT_INDEX_FILE", i);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("git could not be run ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    git_env(cwd, args, None)
}

/// The repository root of a folder, `None` outside git.
pub fn repo_root(cwd: &Path) -> Option<PathBuf> {
    git(cwd, &["rev-parse", "--show-toplevel"]).ok().map(|s| PathBuf::from(s.trim())).filter(|p| p.is_dir())
}

/// A commit of the checkout as it is now — tracked and untracked files,
/// .gitignore honored — without touching HEAD, the branch or the index.
pub fn snapshot(root: &Path) -> Result<String, String> {
    let index = std::env::temp_dir().join(format!("qp-index-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let head = git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok().map(|h| h.trim().to_string());
        if let Some(h) = &head {
            git_env(root, &["read-tree", h], Some(&index))?;
        }
        git_env(root, &["add", "-A", "--", "."], Some(&index))?;
        let tree = git_env(root, &["write-tree"], Some(&index))?.trim().to_string();
        let mut args = vec!["commit-tree", tree.as_str(), "-m", "quantpilot checkpoint"];
        if let Some(h) = &head {
            args.extend(["-p", h.as_str()]);
        }
        Ok(git(root, &args)?.trim().to_string())
    })();
    let _ = std::fs::remove_file(&index);
    result
}

pub fn ref_name(thread: &str, turn: &str, which: &str) -> String {
    format!("{REF_ROOT}/{thread}/{turn}-{which}")
}

/// Keep a snapshot reachable (so `git gc` leaves it) under the thread's refs.
pub fn keep(root: &Path, thread: &str, turn: &str, which: &str, commit: &str) -> Result<(), String> {
    git(root, &["update-ref", &ref_name(thread, turn, which), commit]).map(|_| ())
}

/// Drop every checkpoint of a thread (when it is deleted).
pub fn prune(root: &Path, thread: &str) {
    if let Ok(refs) = git(root, &["for-each-ref", "--format=%(refname)", &format!("{REF_ROOT}/{thread}/")]) {
        for r in refs.lines().filter(|r| !r.is_empty()) {
            let _ = git(root, &["update-ref", "-d", r]);
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Relative to the repository, forward slashes.
    pub path: String,
    /// `A` added, `M` modified, `D` deleted, `R` renamed.
    pub status: String,
    pub added: i64,
    pub removed: i64,
    pub binary: bool,
}

/// Paths an agent's tool reported, relative to the repository root.
pub fn relative(root: &Path, cwd: &Path, path: &str) -> Option<String> {
    let p = Path::new(path);
    let abs = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let norm = |x: &Path| x.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
    let (a, r) = (norm(&abs), norm(root));
    let rel = a.strip_prefix(&format!("{r}/"))?;
    // Keep the path's own case from the original string.
    let start = abs.to_string_lossy().len() - rel.len();
    Some(abs.to_string_lossy()[start..].replace('\\', "/"))
}

/// Files that differ between two commits (all of them when `paths` is empty).
pub fn changes(root: &Path, from: &str, to: &str, paths: &[String]) -> Result<Vec<FileChange>, String> {
    let mut stat_args = vec!["diff", "--numstat", "-z", "--no-renames", from, to, "--"];
    let mut status_args = vec!["diff", "--name-status", "--no-renames", from, to, "--"];
    for p in paths {
        stat_args.push(p);
        status_args.push(p);
    }
    let status = git(root, &status_args)?;
    let numstat = git(root, &stat_args)?;
    let mut counts = std::collections::HashMap::new();
    for rec in numstat.split('\0').filter(|r| !r.is_empty()) {
        let mut parts = rec.splitn(3, '\t');
        let (a, d, p) = (parts.next().unwrap_or("0"), parts.next().unwrap_or("0"), parts.next().unwrap_or(""));
        counts.insert(p.to_string(), (a.parse::<i64>().ok(), d.parse::<i64>().ok()));
    }
    Ok(status
        .lines()
        .filter_map(|l| {
            let (s, p) = l.split_once('\t')?;
            let (a, d) = counts.get(p).copied().unwrap_or((None, None));
            Some(FileChange { path: p.to_string(), status: s.chars().next().unwrap_or('M').to_string(), added: a.unwrap_or(0), removed: d.unwrap_or(0), binary: a.is_none() })
        })
        .collect())
}

/// One file's unified diff between two commits, cut at [`PATCH_FILE`].
pub fn patch(root: &Path, from: &str, to: &str, path: &str) -> String {
    let text = git(root, &["diff", "--no-renames", from, to, "--", path]).unwrap_or_default();
    cut(text, PATCH_FILE)
}

fn cut(text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut at {} KiB)", &text[..end], max / 1024)
}

/// A file as a commit has it — empty when it does not exist there.
pub fn show(root: &Path, commit: &str, path: &str) -> String {
    cut(git(root, &["show", &format!("{commit}:{path}")]).unwrap_or_default(), CONTENT_CAP)
}

/// Put the working tree back to a snapshot for these files (all files when
/// empty): changed and deleted ones come back, ones added since go. The
/// index and HEAD stay as they are.
pub fn restore(root: &Path, commit: &str, paths: &[String]) -> Result<Vec<String>, String> {
    let now = snapshot(root)?;
    let touched = changes(root, commit, &now, paths)?;
    let mut done = Vec::new();
    for c in &touched {
        if c.status == "A" {
            let file = root.join(&c.path);
            std::fs::remove_file(&file).map_err(|e| format!("{}: {e}", c.path))?;
        } else {
            git(root, &["restore", "--source", commit, "--worktree", "--", &c.path])?;
        }
        done.push(c.path.clone());
    }
    Ok(done)
}

/// The merge base of HEAD and `base` (Branch vs base).
pub fn merge_base(root: &Path, base: &str) -> Option<String> {
    git(root, &["merge-base", "HEAD", base]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Stage or unstage one file in the user's own index (the Uncommitted scope's actions).
pub fn stage(root: &Path, path: &str, on: bool) -> Result<(), String> {
    if on {
        git(root, &["add", "--", path]).map(|_| ())
    } else {
        git(root, &["restore", "--staged", "--", path]).map(|_| ())
    }
}

/// Throw away one file's uncommitted changes (an untracked file is deleted).
pub fn discard(root: &Path, path: &str) -> Result<(), String> {
    let tracked = git(root, &["ls-files", "--error-unmatch", "--", path]).is_ok();
    if tracked {
        git(root, &["restore", "--staged", "--worktree", "--source", "HEAD", "--", path]).map(|_| ())
    } else {
        std::fs::remove_file(root.join(path)).map_err(|e| e.to_string())
    }
}

/// Files staged in the user's index (for the Uncommitted list's marks).
pub fn staged(root: &Path) -> Vec<String> {
    git(root, &["diff", "--cached", "--name-only"]).map(|s| s.lines().map(str::to_string).collect()).unwrap_or_default()
}

/// The working tree's content of a file (Uncommitted's right side).
pub fn working(root: &Path, path: &str) -> String {
    let bytes = std::fs::read(root.join(path)).unwrap_or_default();
    cut(String::from_utf8_lossy(&bytes).into_owned(), CONTENT_CAP)
}

/// Total patch budget per turn: files after the budget keep their counts, not their patch.
pub fn within_budget(patches: &mut [String]) {
    let mut used = 0usize;
    for p in patches.iter_mut() {
        if used + p.len() > PATCH_TURN {
            *p = String::new();
        } else {
            used += p.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qp-ckpt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]).unwrap();
        git(&dir, &["config", "user.email", "t@t"]).unwrap();
        git(&dir, &["config", "user.name", "t"]).unwrap();
        // Checkouts follow the machine's line-ending rules; the test pins them.
        git(&dir, &["config", "core.autocrlf", "false"]).unwrap();
        std::fs::write(dir.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git(&dir, &["add", "."]).unwrap();
        git(&dir, &["commit", "-q", "-m", "first"]).unwrap();
        dir
    }

    #[test]
    fn checkpoints_leave_branch_head_and_index_alone() {
        let dir = repo();
        std::fs::write(dir.join("staged.txt"), "s\n").unwrap();
        git(&dir, &["add", "staged.txt"]).unwrap();
        let head = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        let index_before = git(&dir, &["diff", "--cached", "--name-only"]).unwrap();
        let before = snapshot(&dir).unwrap();
        keep(&dir, "t1", "u1", "before", &before).unwrap();
        // The turn: an edit, a new file, a deleted one, an ignored one.
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("b.txt"), "new\n").unwrap();
        std::fs::create_dir_all(dir.join("ignored")).unwrap();
        std::fs::write(dir.join("ignored/x.log"), "x").unwrap();
        std::fs::remove_file(dir.join("staged.txt")).unwrap();
        let after = snapshot(&dir).unwrap();
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]).unwrap(), head, "HEAD untouched");
        assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]).unwrap(), index_before, "the user's index untouched");
        assert_eq!(git(&dir, &["branch", "--show-current"]).unwrap().trim(), "main");

        let all = changes(&dir, &before, &after, &[]).unwrap();
        let summary: Vec<(String, String, i64)> = all.iter().map(|c| (c.path.clone(), c.status.clone(), c.added)).collect();
        assert_eq!(
            summary,
            vec![("a.txt".into(), "M".into(), 1), ("b.txt".into(), "A".into(), 1), ("staged.txt".into(), "D".into(), 0)],
            "ignored files stay out"
        );
        let only = changes(&dir, &before, &after, &["a.txt".to_string()]).unwrap();
        assert_eq!(only.len(), 1, "restricted to the turn's own writes");
        assert!(patch(&dir, &before, &after, "a.txt").contains("+two"));
        assert_eq!(show(&dir, &before, "a.txt"), "one\n");
        assert_eq!(show(&dir, &before, "b.txt"), "", "absent before");

        let restored = restore(&dir, &before, &[]).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
        assert!(!dir.join("b.txt").exists(), "added since → removed");
        assert!(dir.join("staged.txt").exists(), "deleted since → back");
        assert!(dir.join("ignored/x.log").exists(), "ignored files are not the turn's");
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]).unwrap(), head);

        prune(&dir, "t1");
        assert!(git(&dir, &["for-each-ref", "refs/quantpilot"]).unwrap().trim().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paths_become_repo_relative() {
        let root = Path::new("C:/Repo");
        assert_eq!(relative(root, root, "C:\\Repo\\src\\A.rs").as_deref(), Some("src/A.rs"));
        assert_eq!(relative(root, Path::new("C:/Repo/src"), "b.rs").as_deref(), Some("src/b.rs"));
        assert_eq!(relative(root, root, "D:/elsewhere.rs"), None);
        let mut p = vec!["x".repeat(PATCH_TURN - 10), "y".repeat(100)];
        within_budget(&mut p);
        assert!(p[1].is_empty(), "the turn budget");
    }

    #[test]
    fn uncommitted_stage_and_discard() {
        let dir = repo();
        std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
        std::fs::write(dir.join("new.txt"), "n\n").unwrap();
        stage(&dir, "a.txt", true).unwrap();
        assert_eq!(staged(&dir), vec!["a.txt"]);
        stage(&dir, "a.txt", false).unwrap();
        assert!(staged(&dir).is_empty());
        discard(&dir, "a.txt").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
        discard(&dir, "new.txt").unwrap();
        assert!(!dir.join("new.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
