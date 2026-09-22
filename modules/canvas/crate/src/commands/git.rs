use git2::{DiffOptions, Repository, StatusOptions};
use serde::Serialize;
use std::path::Path;

// ---------------------------------------------------------------------------
// Types returned to the frontend
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct GitFileEntry {
    pub path: String,
    /// One of: "new", "modified", "deleted", "renamed", "typechange", "conflicted", "unknown"
    pub status: String,
    /// Whether this change is staged (in the index)
    pub staged: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct GitStatusResult {
    pub branch: String,
    pub files: Vec<GitFileEntry>,
    pub ahead: usize,
    pub behind: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_string(status: git2::Status) -> &'static str {
    if status.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW) {
        "new"
    } else if status.intersects(git2::Status::INDEX_MODIFIED | git2::Status::WT_MODIFIED) {
        "modified"
    } else if status.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
        "deleted"
    } else if status.intersects(git2::Status::INDEX_RENAMED | git2::Status::WT_RENAMED) {
        "renamed"
    } else if status.intersects(git2::Status::INDEX_TYPECHANGE | git2::Status::WT_TYPECHANGE) {
        "typechange"
    } else if status.is_conflicted() {
        "conflicted"
    } else {
        "unknown"
    }
}

fn is_staged(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    )
}

/// Try to compute how many commits the local branch is ahead/behind its upstream.
fn ahead_behind(repo: &Repository) -> (usize, usize) {
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return (0, 0),
    };
    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return (0, 0),
    };
    let branch_name = match head.shorthand() {
        Some(n) => n.to_string(),
        None => return (0, 0),
    };
    let upstream_ref = format!("refs/remotes/origin/{}", branch_name);
    let upstream_oid = match repo.refname_to_id(&upstream_ref) {
        Ok(oid) => oid,
        Err(_) => return (0, 0),
    };
    repo.graph_ahead_behind(local_oid, upstream_oid)
        .unwrap_or((0, 0))
}

// ---------------------------------------------------------------------------
// git_status
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn git_status(repo_path: String) -> Result<GitStatusResult, String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    // Branch name
    let branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(String::from))
        .unwrap_or_else(|| "HEAD (detached)".to_string());

    // Status entries
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_unmodified(false);

    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| format!("Failed to get status: {}", e))?;

    let mut files: Vec<GitFileEntry> = Vec::new();
    for entry in statuses.iter() {
        let path = entry
            .path()
            .unwrap_or("<non-utf8 path>")
            .to_string();
        let st = entry.status();

        // If a file has both staged and unstaged changes, emit two entries.
        let has_staged = is_staged(st);
        let has_unstaged = st.intersects(
            git2::Status::WT_NEW
                | git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        );

        if has_staged {
            files.push(GitFileEntry {
                path: path.clone(),
                status: status_string(st).to_string(),
                staged: true,
            });
        }
        if has_unstaged {
            files.push(GitFileEntry {
                path: path.clone(),
                status: status_string(st).to_string(),
                staged: false,
            });
        }
        // Edge case: conflicted files
        if !has_staged && !has_unstaged {
            files.push(GitFileEntry {
                path,
                status: status_string(st).to_string(),
                staged: false,
            });
        }
    }

    let (ahead, behind) = ahead_behind(&repo);

    Ok(GitStatusResult {
        branch,
        files,
        ahead,
        behind,
    })
}

// ---------------------------------------------------------------------------
// git_diff
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn git_diff(
    repo_path: String,
    staged: Option<bool>,
    mode: Option<String>,
) -> Result<String, String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true);

    // Determine diff mode: "all" (default), "staged", or "unstaged"
    let effective_mode = mode.as_deref().unwrap_or({
        match staged {
            Some(true) => "staged",
            Some(false) => "unstaged",
            None => "all",
        }
    });

    let head_tree = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_tree().ok());

    let diff = match effective_mode {
        "staged" => {
            // Staged diff: compare HEAD tree to index
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
                .map_err(|e| format!("Failed to get staged diff: {}", e))?
        }
        "unstaged" => {
            // Unstaged diff: compare index to workdir
            repo.diff_index_to_workdir(None, Some(&mut opts))
                .map_err(|e| format!("Failed to get unstaged diff: {}", e))?
        }
        _ => {
            // All changes: compare HEAD to workdir (includes both staged + unstaged)
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))
                .map_err(|e| format!("Failed to get all changes diff: {}", e))?
        }
    };

    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        match origin {
            '+' | '-' | ' ' => {
                diff_text.push(origin);
            }
            _ => {}
        }
        diff_text.push_str(
            std::str::from_utf8(line.content()).unwrap_or("<binary>"),
        );
        true
    })
    .map_err(|e| format!("Failed to print diff: {}", e))?;

    Ok(diff_text)
}

// ---------------------------------------------------------------------------
// git_stage
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn git_stage(repo_path: String, file_paths: Vec<String>) -> Result<(), String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let repo_root = repo
        .workdir()
        .ok_or_else(|| "Bare repository has no workdir".to_string())?;

    for file_path in &file_paths {
        let full_path = repo_root.join(file_path);
        if full_path.exists() {
            index
                .add_path(Path::new(file_path))
                .map_err(|e| format!("Failed to stage {}: {}", file_path, e))?;
        } else {
            // File was deleted — remove from index
            index
                .remove_path(Path::new(file_path))
                .map_err(|e| format!("Failed to stage deletion of {}: {}", file_path, e))?;
        }
    }

    index
        .write()
        .map_err(|e| format!("Failed to write index: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// git_branch_list
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn git_branch_list(repo_path: String) -> Result<Vec<String>, String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    let branches = repo
        .branches(Some(git2::BranchType::Local))
        .map_err(|e| format!("Failed to list branches: {}", e))?;

    let mut result: Vec<String> = Vec::new();
    for branch_result in branches {
        let (branch, _) = branch_result.map_err(|e| format!("Branch error: {}", e))?;
        if let Some(name) = branch.name().map_err(|e| format!("Branch name error: {}", e))? {
            result.push(name.to_string());
        }
    }

    result.sort_by_key(|a| a.to_lowercase());
    Ok(result)
}

// ---------------------------------------------------------------------------
// git_log
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitEntry {
    /// Full OID, what `git_commit_files` wants back.
    pub oid: String,
    pub short: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds.
    pub time: i64,
}

/// The last `limit` commits reachable from HEAD, newest first.
#[tauri::command(async)]
pub fn git_log(repo_path: String, limit: Option<usize>) -> Result<Vec<GitCommitEntry>, String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    let mut walk = repo
        .revwalk()
        .map_err(|e| format!("Failed to walk history: {}", e))?;
    // An unborn HEAD (fresh repo, no commit yet) is an empty history, not an
    // error — the panel shows "no commits" the way it shows "no changes".
    if walk.push_head().is_err() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for oid in walk.take(limit.unwrap_or(30)) {
        let oid = oid.map_err(|e| format!("Walk error: {}", e))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| format!("Missing commit {}: {}", oid, e))?;
        out.push(GitCommitEntry {
            oid: oid.to_string(),
            short: oid.to_string()[..7].to_string(),
            summary: commit.summary().unwrap_or("<no message>").to_string(),
            author: commit.author().name().unwrap_or("<unknown>").to_string(),
            time: commit.time().seconds(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// git_commit_files
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitFile {
    pub path: String,
    /// One of: "added", "modified", "deleted", "renamed", "typechange", "unknown"
    pub status: String,
}

/// The files one commit touched — its tree against the first parent's.
#[tauri::command(async)]
pub fn git_commit_files(repo_path: String, oid: String) -> Result<Vec<GitCommitFile>, String> {
    let repo =
        Repository::open(Path::new(&repo_path)).map_err(|e| format!("Cannot open repo: {}", e))?;

    let oid = git2::Oid::from_str(&oid).map_err(|e| format!("Bad oid: {}", e))?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| format!("Missing commit: {}", e))?;
    let tree = commit
        .tree()
        .map_err(|e| format!("Missing tree: {}", e))?;
    // A root commit has no parent — its diff is against the empty tree.
    let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
        .map_err(|e| format!("Failed to diff commit: {}", e))?;

    let mut files = Vec::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let status = match delta.status() {
            git2::Delta::Added => "added",
            git2::Delta::Deleted => "deleted",
            git2::Delta::Modified => "modified",
            git2::Delta::Renamed => "renamed",
            git2::Delta::Typechange => "typechange",
            _ => "unknown",
        };
        files.push(GitCommitFile {
            path,
            status: status.to_string(),
        });
    }
    Ok(files)
}
