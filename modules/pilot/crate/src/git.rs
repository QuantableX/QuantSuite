//! A thread's checkout (card 10): which repository and branch it is on,
//! switching and creating branches in a Local thread, and the guarded
//! cleanup of a Worktree thread's worktree. Worktrees themselves are built
//! by QuantMCP's implementation (`qs_mod_mcp::worktree`), never a second
//! one. Every git call: argv only, no stdin, prompts off, no window.

use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("git could not be run ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Git's own branch-name rules (`git check-ref-format --branch`), so a bad
/// name is explained before git refuses it.
pub fn validate_branch(name: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("'{name}' cannot be a branch name: {why}"));
    if name.trim().is_empty() {
        return bad("it is empty");
    }
    if name.chars().any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c)) {
        return bad("no spaces or ~ ^ : ? * [ \\");
    }
    if name.contains("..") || name.contains("@{") || name.contains("//") || name == "@" {
        return bad("no '..', '@{' or '//'");
    }
    if name.starts_with('/') || name.ends_with('/') || name.ends_with('.') || name.starts_with('-') {
        return bad("it cannot start with '/' or '-' or end with '/' or '.'");
    }
    if name.split('/').any(|part| part.starts_with('.') || part.ends_with(".lock")) {
        return bad("no part may start with '.' or end with '.lock'");
    }
    Ok(())
}

/// `quantpilot/fix-the-login-bug` from a prefix and a title or prompt.
pub fn suggest_branch(prefix: &str, words: &str) -> String {
    let prefix = prefix.trim().trim_start_matches('/');
    let leaf = qs_mod_mcp::worktree::slug(words);
    let prefix = if prefix.is_empty() || prefix.ends_with('/') { prefix.to_string() } else { format!("{prefix}/") };
    format!("{prefix}{leaf}")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: String,
    pub branch: String,
    pub default_branch: String,
    /// Changed or untracked files in the checkout.
    pub dirty: usize,
}

/// The repository a folder belongs to, and its state; `None` outside git.
pub fn repo_info(cwd: &Path) -> Option<RepoInfo> {
    let root = qs_mod_mcp::worktree::repo_root(cwd)?;
    let branch = qs_mod_mcp::worktree::current_branch(cwd).unwrap_or_default();
    let default_branch = qs_mod_mcp::worktree::default_branch(Path::new(&root));
    Some(RepoInfo { root, branch, default_branch, dirty: dirty_files(cwd).len() })
}

pub fn dirty_files(cwd: &Path) -> Vec<String> {
    git(cwd, &["status", "--porcelain", "--untracked-files=normal"])
        .map(|s| s.lines().map(|l| l.get(3..).unwrap_or(l).to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    /// Unix milliseconds of the last commit.
    pub updated_at: i64,
    /// Checked out in another worktree (cannot be switched to here).
    pub elsewhere: bool,
}

/// Local branches, newest commit first.
pub fn branches(cwd: &Path) -> Result<Vec<BranchInfo>, String> {
    let out = git(cwd, &["for-each-ref", "--sort=-committerdate", "refs/heads", "--format=%(refname:short)%09%(committerdate:unix)%09%(HEAD)%09%(worktreepath)"])?;
    let here = qs_mod_mcp::worktree::repo_root(cwd).unwrap_or_default().to_lowercase();
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let name = parts.next()?.to_string();
            let at = parts.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0) * 1000;
            let current = parts.next() == Some("*");
            let path = qs_mod_mcp::worktree::native(parts.next().unwrap_or_default()).to_lowercase();
            Some(BranchInfo { name, current, updated_at: at, elsewhere: !current && !path.is_empty() && path != here })
        })
        .collect())
}

/// Switch a Local thread's checkout — refused while files are changed,
/// because switching would carry or clash with them.
pub fn checkout(cwd: &Path, branch: &str, create: bool, base: Option<&str>) -> Result<(), String> {
    validate_branch(branch)?;
    let dirty = dirty_files(cwd);
    if !dirty.is_empty() {
        let list = dirty.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        return Err(format!(
            "{} file{} changed in this checkout ({list}{}). Commit or stash them before switching — or work in a worktree.",
            dirty.len(),
            if dirty.len() == 1 { " is" } else { "s are" },
            if dirty.len() > 5 { ", …" } else { "" }
        ));
    }
    if create {
        let mut args = vec!["switch", "-c", branch];
        if let Some(b) = base.filter(|b| !b.trim().is_empty()) {
            args.push(b);
        }
        git(cwd, &args).map(|_| ())
    } else {
        git(cwd, &["switch", branch]).map(|_| ())
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCheck {
    pub exists: bool,
    /// Uncommitted files: the worktree is never removed with these.
    pub dirty: Vec<String>,
    /// The branch is fully in its base: deleting it loses nothing.
    pub merged: bool,
    /// Commits on the branch that are not in the base.
    pub ahead: usize,
    pub branch: String,
    pub base: String,
    /// The dialog's defaults (`defaults`): remove the worktree, delete the branch.
    pub suggest_remove: bool,
    pub suggest_delete_branch: bool,
}

impl CleanupCheck {
    /// What settling may do by default: remove a clean worktree, and its
    /// branch only when merged. Unmerged commits keep the branch unless the
    /// user ticks it; uncommitted changes keep everything.
    pub fn defaults(&self) -> (bool, bool) {
        let remove = self.exists && self.dirty.is_empty();
        (remove, remove && self.merged)
    }
}

pub fn cleanup_check(repo_root: &Path, wt: &Path, branch: &str, base: &str) -> CleanupCheck {
    let exists = wt.is_dir();
    let dirty = if exists { dirty_files(wt) } else { Vec::new() };
    let base = if base.trim().is_empty() { qs_mod_mcp::worktree::default_branch(repo_root) } else { base.to_string() };
    let merged = git(repo_root, &["merge-base", "--is-ancestor", branch, &base]).is_ok();
    let ahead = git(repo_root, &["rev-list", "--count", &format!("{base}..{branch}")]).ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    let mut check = CleanupCheck { exists, dirty, merged, ahead, branch: branch.to_string(), base, ..Default::default() };
    (check.suggest_remove, check.suggest_delete_branch) = check.defaults();
    check
}

/// Remove a worktree that has no uncommitted changes; the branch goes too
/// when asked — with `git branch -d` when merged, `-D` only when the user
/// ticked "delete unmerged commits".
pub fn remove_worktree(repo_root: &Path, wt: &Path, branch: &str, delete_branch: bool, unmerged_ok: bool) -> Result<(), String> {
    if wt.is_dir() {
        let dirty = dirty_files(wt);
        if !dirty.is_empty() {
            return Err(format!("{} uncommitted file(s) in the worktree — it stays.", dirty.len()));
        }
        qs_mod_mcp::worktree::remove_checkout(repo_root, wt)?;
    }
    if delete_branch && !branch.is_empty() {
        let flag = if unmerged_ok { "-D" } else { "-d" };
        git(repo_root, &["branch", flag, branch])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_follow_git_rules() {
        for ok in ["main", "quantpilot/fix-login", "feature/x.y", "a-b_c"] {
            assert!(validate_branch(ok).is_ok(), "{ok}");
        }
        for bad in ["", "with space", "a..b", "x~1", "a:b", "/lead", "trail/", "dot.", "-dash", "a//b", "x.lock", "a/.hidden", "@", "a@{1}", "q?"] {
            assert!(validate_branch(bad).is_err(), "{bad}");
        }
        assert_eq!(suggest_branch("quantpilot/", "Fix the Login bug!"), "quantpilot/fix-the-login-bug");
        assert_eq!(suggest_branch("qp", "x"), "qp/x");
        assert_eq!(suggest_branch("", "Tidy"), "tidy");
    }

    fn repo() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("qp-git-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@t"], &["config", "user.name", "t"]] {
            git(&dir, args).unwrap();
        }
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        git(&dir, &["add", "."]).unwrap();
        git(&dir, &["commit", "-q", "-m", "first"]).unwrap();
        dir
    }

    #[test]
    fn switching_is_refused_with_changes() {
        let dir = repo();
        assert!(checkout(&dir, "feature", true, None).is_ok());
        assert_eq!(repo_info(&dir).unwrap().branch, "feature");
        std::fs::write(dir.join("a.txt"), "changed").unwrap();
        let err = checkout(&dir, "main", false, None).unwrap_err();
        assert!(err.contains("1 file is changed") && err.contains("a.txt"), "{err}");
        let names: Vec<String> = branches(&dir).unwrap().into_iter().map(|b| b.name).collect();
        assert!(names.contains(&"main".to_string()) && names.contains(&"feature".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn worktree_cleanup_is_guarded() {
        let dir = repo();
        let root = dir.to_string_lossy().into_owned();
        // The shared implementation, the same one QuantMCP uses.
        let (wt, _) = qs_mod_mcp::worktree::create_on_branch(&root, "quantpilot/try", Some("main")).unwrap();
        let path = std::path::PathBuf::from(&wt.path);
        assert!(path.join("a.txt").is_file());
        let fresh = cleanup_check(&dir, &path, "quantpilot/try", "main");
        assert_eq!((fresh.exists, fresh.merged, fresh.ahead), (true, true, 0));
        assert_eq!(fresh.defaults(), (true, true), "clean and merged: worktree and branch go");

        std::fs::write(path.join("b.txt"), "b").unwrap();
        let dirty = cleanup_check(&dir, &path, "quantpilot/try", "main");
        assert_eq!(dirty.defaults(), (false, false), "uncommitted changes keep everything");
        assert!(remove_worktree(&dir, &path, "quantpilot/try", true, false).is_err());

        git(&path, &["add", "."]).unwrap();
        git(&path, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "b"]).unwrap();
        let ahead = cleanup_check(&dir, &path, "quantpilot/try", "main");
        assert_eq!((ahead.merged, ahead.ahead), (false, 1));
        assert_eq!(ahead.defaults(), (true, false), "unmerged commits keep the branch by default");
        remove_worktree(&dir, &path, "quantpilot/try", false, false).unwrap();
        assert!(!path.exists());
        assert!(branches(&dir).unwrap().iter().any(|b| b.name == "quantpilot/try"), "the branch with its commit stays");
        assert!(remove_worktree(&dir, &path, "quantpilot/try", true, false).is_err(), "-d refuses an unmerged branch");
        remove_worktree(&dir, &path, "quantpilot/try", true, true).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
