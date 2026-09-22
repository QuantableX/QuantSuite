use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Git helper functions for worktree and branch management
// ---------------------------------------------------------------------------

/// CREATE_NO_WINDOW — the release binary has no console; without the flag
/// every git call flashes a console window in installed builds.
fn git() -> Command {
    let cmd = Command::new("git");
    #[cfg(windows)]
    let cmd = {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x08000000);
        cmd
    };
    cmd
}

fn run_git(args: &[&str], cwd: &str) -> Result<String, String> {
    let output = git()
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!(
                "git {:?} failed with exit code {:?}",
                args,
                output.status.code()
            )
        } else {
            stderr
        })
    }
}

/// Verify the path is inside a git repository.
pub fn verify_git_repo(project_root: &str) -> Result<(), String> {
    run_git(&["rev-parse", "--is-inside-work-tree"], project_root)?;
    Ok(())
}

/// Get the current branch name.
pub fn get_current_branch(project_root: &str) -> Result<String, String> {
    run_git(&["rev-parse", "--abbrev-ref", "HEAD"], project_root)
}

/// Get the default (main) branch name reliably.
/// Tries: remote HEAD → common names (main, master) → current branch as fallback.
pub fn get_default_branch(project_root: &str) -> Result<String, String> {
    // Try remote HEAD first (most reliable)
    if let Ok(remote_head) = run_git(
        &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
        project_root,
    ) {
        // Returns "origin/main" or "origin/master" — strip the "origin/" prefix
        if let Some(branch) = remote_head.strip_prefix("origin/") {
            return Ok(branch.to_string());
        }
        return Ok(remote_head);
    }

    // Fallback: check if common branch names exist
    for name in &["main", "master"] {
        if branch_exists(project_root, name) {
            return Ok(name.to_string());
        }
    }

    // Last resort: current branch
    get_current_branch(project_root)
}

/// Check if a branch exists.
pub fn branch_exists(project_root: &str, branch_name: &str) -> bool {
    run_git(&["branch", "--list", branch_name], project_root)
        .map(|out| !out.is_empty())
        .unwrap_or(false)
}

/// Create a git worktree for a card with automatic branch name collision avoidance.
/// Returns (branch_name, worktree_path, what `worktree::prepare` linked in).
pub fn create_worktree(
    project_root: &str,
    card_id: &str,
    agent_id: &str,
) -> Result<(String, String, crate::worktree::Prepared), String> {
    verify_git_repo(project_root)?;

    // Build branch name with collision avoidance
    let short_id = &card_id[..8.min(card_id.len())];
    let base_branch = format!("agent/{}-{}", agent_id, short_id);
    let mut branch_name = base_branch.clone();
    let mut suffix = 2;
    while branch_exists(project_root, &branch_name) {
        branch_name = format!("{}-v{}", base_branch, suffix);
        suffix += 1;
        if suffix > 100 {
            return Err("Too many branch name collisions".into());
        }
    }

    // `.qs-worktrees/`, not the legacy `workspaces/` — "workspace" now means a
    // registered suite folder, and a hidden dir keeps card checkouts out of
    // the user's way (PLAN-WORKSPACE-UNIFY D7). Joined with `Path::join`, as
    // `worktree::create` does: a `format!("{}/…")` on a Windows root gave
    // `C:\…\QuantSuite/.qs-worktrees\<id>`, and `cmd /C mklink` (run by
    // `worktree::link_dir`) took `/.qs-worktrees` for a switch — so no card
    // worktree ever got its node_modules linked (2026-09-04).
    let wt_parent = Path::new(project_root).join(".qs-worktrees");
    let worktree_dir = wt_parent.join(short_id).to_string_lossy().into_owned();
    let _ = std::fs::create_dir_all(&wt_parent);

    // Add to .gitignore if not already there
    add_to_gitignore(project_root, ".qs-worktrees/");

    // Create worktree + branch
    let output = git()
        .args(["worktree", "add", "-b", &branch_name, &worktree_dir, "HEAD"])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("Failed to run git worktree add: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add failed: {}", stderr.trim()));
    }

    // Buildable from the start: the main checkout's node_modules / .venv are
    // linked in (worktree.rs, "Build hygiene").
    let prepared = crate::worktree::prepare(project_root, &worktree_dir);

    Ok((branch_name, worktree_dir, prepared))
}

/// Remove a card's worktree — `worktree::remove_checkout`: links dropped
/// first, git's removal retried, a folder git left behind deleted, stale
/// cargo outputs in the main checkout reset. A folder that still cannot be
/// deleted is an error for the caller to report; it used to be swallowed,
/// which is how a half-deleted checkout stayed on disk unnoticed (2026-09-03).
pub fn remove_worktree(project_root: &str, worktree_path: &str) -> Result<crate::worktree::Removed, String> {
    crate::worktree::remove_checkout(Path::new(project_root), Path::new(worktree_path))
}

/// Delete a git branch.
pub fn delete_branch(project_root: &str, branch_name: &str, force: bool) -> Result<(), String> {
    let flag = if force { "-D" } else { "-d" };
    run_git(&["branch", flag, branch_name], project_root)?;
    Ok(())
}

/// Merge a branch into main with --no-ff.
/// Returns Ok(()) on success, or Err with conflict info on failure.
pub fn merge_branch(
    project_root: &str,
    branch_name: &str,
    main_branch: &str,
    card_id: &str,
    card_title: &str,
) -> Result<(), String> {
    // First checkout main
    run_git(&["checkout", main_branch], project_root)
        .map_err(|e| format!("Failed to checkout {}: {}", main_branch, e))?;

    let merge_msg = format!(
        "merge: card-{} - {}",
        &card_id[..8.min(card_id.len())],
        card_title
    );

    let output = git()
        .args(["merge", "--no-ff", branch_name, "-m", &merge_msg])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("Failed to run git merge: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // Abort the merge to restore clean state
        let _ = run_git(&["merge", "--abort"], project_root);
        return Err(format!(
            "Merge conflict for branch '{}': {}. Merge aborted, worktree preserved for manual resolution.",
            branch_name, stderr
        ));
    }

    Ok(())
}

/// Get diff between a branch and main.
pub fn get_diff(
    project_root: &str,
    main_branch: &str,
    branch_name: &str,
) -> Result<String, String> {
    let stat = run_git(
        &[
            "diff",
            "--stat",
            &format!("{}...{}", main_branch, branch_name),
        ],
        project_root,
    )
    .unwrap_or_else(|_| "(no stat available)".into());

    let diff = run_git(
        &["diff", &format!("{}...{}", main_branch, branch_name)],
        project_root,
    )
    .unwrap_or_else(|_| "(no diff available)".into());

    Ok(format!(
        "--- Diff stat ---\n{}\n\n--- Full diff ---\n{}",
        stat, diff
    ))
}

/// Get list of modified files in a worktree.
pub fn get_modified_files(worktree_path: &str) -> Result<Vec<String>, String> {
    if !Path::new(worktree_path).exists() {
        return Ok(vec![]);
    }

    let mut files = std::collections::HashSet::new();

    // Staged changes
    if let Ok(output) = run_git(&["diff", "--cached", "--name-only"], worktree_path) {
        for line in output.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Unstaged changes
    if let Ok(output) = run_git(&["diff", "--name-only"], worktree_path) {
        for line in output.lines() {
            if !line.is_empty() {
                files.insert(line.to_string());
            }
        }
    }

    // Changes committed on this branch since it forked off the main branch
    if let Ok(main_branch) = get_default_branch(worktree_path) {
        if let Ok(output) = run_git(
            &["diff", "--name-only", &format!("{}...HEAD", main_branch)],
            worktree_path,
        ) {
            for line in output.lines() {
                if !line.is_empty() {
                    files.insert(line.to_string());
                }
            }
        }
    }

    Ok(files.into_iter().collect())
}

/// List all worktrees for a project.
pub fn list_worktrees(project_root: &str) -> Result<Vec<(String, String, String)>, String> {
    // Returns: Vec<(path, branch, head_commit)>
    let output = run_git(&["worktree", "list", "--porcelain"], project_root)?;

    let mut worktrees = Vec::new();
    let mut path = String::new();
    let mut head = String::new();
    let mut branch = String::new();

    for line in output.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = p.to_string();
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = b.replace("refs/heads/", "");
        } else if line.is_empty() && !path.is_empty() {
            worktrees.push((path.clone(), branch.clone(), head.clone()));
            path.clear();
            head.clear();
            branch.clear();
        }
    }
    // Capture last entry
    if !path.is_empty() {
        worktrees.push((path, branch, head));
    }

    Ok(worktrees)
}

/// Push a branch to remote.
pub fn push_to_remote(project_root: &str, branch_name: &str) -> Result<String, String> {
    run_git(&["push", "-u", "origin", branch_name], project_root)
}

/// Check if a remote named "origin" exists.
pub fn has_remote(project_root: &str) -> bool {
    run_git(&["remote", "get-url", "origin"], project_root).is_ok()
}

/// Add an entry to .gitignore if not already present.
fn add_to_gitignore(project_root: &str, entry: &str) {
    let gitignore_path = format!("{}/.gitignore", project_root);
    let contents = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if !contents.lines().any(|line| line.trim() == entry) {
        let new_contents = if contents.ends_with('\n') || contents.is_empty() {
            format!("{}{}\n", contents, entry)
        } else {
            format!("{}\n{}\n", contents, entry)
        };
        let _ = std::fs::write(&gitignore_path, new_contents);
    }
}
