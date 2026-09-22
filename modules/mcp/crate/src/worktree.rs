//! Agent worktrees — the `*_worktree` tools (docs/PLAN-WORKTREES.md).
//!
//! Several agents in one repository write over each other. An agent that
//! asks for it gets its own checkout — `<repo>/.qs-worktrees/<slug>-<short>`
//! on a branch `agent/<slug>-<short>`, cut from the branch the repository
//! is on (or a base it names) — and works there. Whoever reviews (the user,
//! or an agent told to) reads the branch's diff against its base, merges it
//! back with a plain `--no-ff` merge (the main checkout must be on the base
//! branch), or throws the whole worktree away.
//!
//! Everything shells out to `git`: worktrees are a porcelain feature, and
//! the user's git — credentials, hooks, config — is the one that should
//! run. `.qs-worktrees/` is the directory the kanban's claimed cards use
//! too (`git_helpers.rs`, PLAN-WORKSPACE-UNIFY D7); `list` shows both. The
//! base a worktree was cut from is remembered in the repository's own
//! config (`branch.<name>.quantsuite-base`), so no database is involved and
//! a worktree made by hand still resolves against the default branch.
//!
//! Build hygiene (2026-09-03). A fresh worktree is made buildable by linking
//! the main checkout's ignored dependency dirs into it (`prepare`). An
//! agent's own Rust builds go into the worktree's own `target/`: cargo keys
//! `build/<crate>-<hash>/` by the crate path relative to the workspace
//! root, so a worktree that shares CARGO_TARGET_DIR with the main checkout
//! writes the very same dirs, and Tauri plugins leave absolute worktree
//! paths in them — the main build then fails once the worktree is gone
//! ("failed to read plugin permissions … .qs-worktrees\…"). Removal drops
//! the links first (never follows them), finishes a delete git gave up on,
//! and resets any such stale outputs (`invalidate_stale_cargo_outputs`).
//! That reset is also what makes the USER's test commands (`test_commands`,
//! one per shell) safe: they share the main `target/` on purpose — an
//! incremental build instead of a cold one — and every tool reply carries
//! them so the agent can hand them on. The kanban's card worktrees (`git_helpers.rs`) go through
//! the same `prepare` / `remove_checkout`.

use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub const DIR: &str = ".qs-worktrees";
pub const BRANCH_PREFIX: &str = "agent/";
const BASE_KEY: &str = "quantsuite-base";
/// A diff bigger than this is cut; the caller is told.
pub const DIFF_CAP: usize = 512 * 1024;
const UNTRACKED_IN_DIFF: usize = 40;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub repo_root: String,
    pub path: String,
    pub branch: String,
    /// The branch the worktree was cut from — what the diff compares to
    /// and what a merge lands on.
    pub base: String,
}

/// One line of `git worktree list`, plus the remembered base.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub branch: Option<String>,
    pub head: String,
    /// The repository's own checkout, not an agent worktree.
    pub main: bool,
    pub base: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// `modified` | `added` | `deleted` | `renamed` | `untracked` | `conflict`
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub branch: String,
    pub base: String,
    pub path: String,
    pub exists: bool,
    /// Commits on the branch that are not on the base.
    pub commits: u32,
    /// Uncommitted changes in the worktree.
    pub files: Vec<FileChange>,
    /// Working tree against the base, committed or not (untracked excluded).
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub stat: String,
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub merged: bool,
    pub commits: u32,
    /// Uncommitted work was committed first.
    pub committed: bool,
    /// The worktree and branch are gone.
    pub cleaned: bool,
    pub message: String,
}

/// Ignored dependency directories a fresh worktree gets linked from the
/// main checkout (a junction on Windows, a symlink elsewhere), so it builds
/// and runs without an install. `target/` is deliberately NOT here — see
/// `invalidate_stale_cargo_outputs`.
pub const LINKED_DEP_DIRS: &[&str] = &["node_modules", ".venv", "venv"];
/// Never descended into when looking for links to drop.
const OPAQUE_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "venv", ".nuxt", ".output", "dist"];
const LINK_SCAN_DEPTH: usize = 5;
/// A cargo build-script file bigger than this, or an `out/` dir with more
/// files than this, is not scanned — the offenders are small text lists.
const CARGO_SCAN_FILE_CAP: u64 = 256 * 1024;
const CARGO_SCAN_FILES_PER_UNIT: usize = 500;

/// The rule every worktree reply carries — the agent's side of the build
/// contract. The user's side is [`test_command`]: it shares the main
/// checkout's target/ on purpose, and removal resets what that leaves.
pub const BUILD_RULE: &str = "Build rule: for your own Rust builds use the worktree's own target/ (cargo's default) — do not point CARGO_TARGET_DIR at the main checkout's target yourself, and never start `npm run tauri:dev` (port 1420 and the app window belong to the user). A frontend dev server can run from here on a spare port. The user runs the app from this worktree with the test command in this reply (get_worktree_test_command returns it again): it shares the main target/ deliberately for an incremental build, and the tools reset the outputs that leaves behind when the worktree is removed. Show the test command to the user verbatim whenever you report on this worktree.";

/// The one-liner the user pastes to run the app from a worktree, once per
/// shell. Shares the main checkout's target/ on purpose: a worktree's own
/// target is a cold, minutes-long Rust build, the shared one is
/// incremental. The price — Tauri plugin build scripts then record
/// absolute worktree paths in that target/ — is paid by `remove_checkout`,
/// which resets exactly those units (`invalidate_stale_cargo_outputs`)
/// when the worktree goes.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct TestCommands {
    /// PowerShell — the suite's shell.
    pub powershell: String,
    /// Windows Command Prompt.
    pub cmd: String,
    /// bash / Git Bash / zsh — forward slashes, which every Windows tool
    /// involved (cd, cargo, npm) takes as well.
    pub bash: String,
}

impl TestCommands {
    /// One labelled line per shell, for tool replies.
    pub fn lines(&self) -> [String; 3] {
        [
            format!("PowerShell: {}", self.powershell),
            format!("cmd: {}", self.cmd),
            format!("bash: {}", self.bash),
        ]
    }
}

pub fn test_commands(repo_root: &str, wt_path: &str) -> TestCommands {
    // No trailing separator inside the quotes: `"…\"` would swallow the
    // quote in most shells.
    let repo = native(repo_root).trim_end_matches(['/', '\\']).to_string();
    let wt = native(wt_path).trim_end_matches(['/', '\\']).to_string();
    let target = format!("{repo}{}target", std::path::MAIN_SEPARATOR);
    let slashes = |p: &str| p.replace('\\', "/");
    TestCommands {
        powershell: format!("cd \"{wt}\"; $env:CARGO_TARGET_DIR = \"{target}\"; npm run tauri:dev"),
        // `/d` also switches the drive; `set "K=V"` is cmd's safe quoting.
        cmd: format!("cd /d \"{wt}\" && set \"CARGO_TARGET_DIR={target}\" && npm run tauri:dev"),
        bash: format!(
            "cd \"{}\" && CARGO_TARGET_DIR=\"{}\" npm run tauri:dev",
            slashes(&wt),
            slashes(&target)
        ),
    }
}

/// The test commands from a worktree path alone — for a kanban card, which
/// stores the path but not the repository. Only for checkouts under
/// `<repo>/.qs-worktrees/`, which is where every tool puts them.
pub fn test_commands_from_path(wt_path: &str) -> Option<TestCommands> {
    let normalized = wt_path.replace('\\', "/");
    let path = Path::new(normalized.trim_end_matches('/'));
    let parent = path.parent()?;
    if parent.file_name()?.to_str()? != DIR {
        return None;
    }
    let repo = parent.parent()?.to_str()?;
    if repo.is_empty() {
        return None;
    }
    Some(test_commands(repo, wt_path))
}

/// The lines a tool reply carries so the agent can hand the commands on.
pub fn test_command_notes(repo_root: &str, wt_path: &str) -> Vec<String> {
    let mut out = vec![
        "Test commands (for the USER — stop the main checkout's tauri:dev first; they share that target/ for an incremental build):".to_string(),
    ];
    out.extend(test_commands(repo_root, wt_path).lines().into_iter().map(|l| format!("  {l}")));
    out.push(format!("AI artifacts: use a task subfolder of {} for screenshots, scratch scripts and logs; keep them outside the checkout.", qs_core::paths::workspace_artifacts_dir(repo_root).display()));
    out
}

/// What `prepare` did to a fresh worktree.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// Relative paths linked from the main checkout.
    pub linked: Vec<String>,
    /// Links that could not be made; the worktree exists all the same.
    pub warnings: Vec<String>,
}

impl Prepared {
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.linked.is_empty() {
            out.push(format!("Linked from the main checkout, no install needed: {}", self.linked.join(", ")));
        }
        out.extend(self.warnings.iter().map(|w| format!("Could not link {w}")));
        out
    }
}

/// What `remove_checkout` did besides deleting the folder.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    /// Directory links dropped before the delete.
    pub unlinked: Vec<String>,
    /// Crates whose build outputs in the main checkout's `target/` pointed
    /// into a worktree and were dropped so cargo re-runs them.
    pub cargo_reset: Vec<String>,
}

impl Removed {
    /// One line per thing worth telling the caller; empty when nothing is.
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.unlinked.is_empty() {
            out.push(format!(
                "Dropped {} dependency link(s) before deleting; the main checkout's copies are untouched.",
                self.unlinked.len()
            ));
        }
        if !self.cargo_reset.is_empty() {
            out.push(format!(
                "Reset stale cargo build outputs in the main checkout's target/ for {} — they pointed into a worktree (a build shared CARGO_TARGET_DIR); the next build re-runs those build scripts.",
                self.cargo_reset.join(", ")
            ));
        }
        out
    }
}

/// CREATE_NO_WINDOW: the release binary has no console, and a bare `git`
/// child would flash one per call.
fn git() -> Command {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

pub(super) fn run(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = git()
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git could not be run ({e}) — is git installed and on PATH?"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() { format!("git {} failed ({})", args.join(" "), out.status) } else { stderr })
    }
}

/// `git diff --no-index` exits 1 when the files differ — the answer, not a failure.
fn run_diff(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = git().args(args).current_dir(cwd).output().map_err(|e| format!("git could not be run ({e})"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

/// git prints `C:/Projects/x`; the rest of the suite stores `C:\Projects\x`.
pub fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.to_string()
    }
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

pub fn repo_root(path: &Path) -> Option<String> {
    if !path.is_dir() {
        return None;
    }
    run(path, &["rev-parse", "--show-toplevel"]).ok().filter(|s| !s.is_empty()).map(|s| native(&s))
}

/// The branch the checkout is on; a short hash when detached.
pub fn current_branch(repo: &Path) -> Result<String, String> {
    let name = run(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if name == "HEAD" {
        run(repo, &["rev-parse", "--short", "HEAD"])
    } else {
        Ok(name)
    }
}

/// Remote HEAD → `main` → `master` → the current branch.
pub fn default_branch(repo: &Path) -> String {
    if let Ok(remote) = run(repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"]) {
        return remote.strip_prefix("origin/").unwrap_or(&remote).to_string();
    }
    for name in ["main", "master"] {
        if branch_exists(repo, name) {
            return name.to_string();
        }
    }
    current_branch(repo).unwrap_or_else(|_| "main".into())
}

fn branch_exists(repo: &Path, name: &str) -> bool {
    run(repo, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_ok()
}

fn ref_exists(repo: &Path, name: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")]).is_ok()
}

/// A name as a branch segment: lowercase, dashes, at most 32 chars.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
        if out.len() >= 32 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "work".into()
    } else {
        out
    }
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..6].to_string()
}

/// `.qs-worktrees/` in the repository's `.gitignore`, once.
fn ensure_gitignore(repo: &Path) {
    let file = repo.join(".gitignore");
    let entry = format!("{DIR}/");
    let contents = std::fs::read_to_string(&file).unwrap_or_default();
    if contents.lines().any(|l| l.trim() == entry || l.trim() == DIR) {
        return;
    }
    let mut next = contents;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(&entry);
    next.push('\n');
    let _ = std::fs::write(&file, next);
}

fn base_of(repo: &Path, branch: &str) -> Option<String> {
    run(repo, &["config", "--get", &format!("branch.{branch}.{BASE_KEY}")]).ok().filter(|s| !s.is_empty())
}

/// A new worktree: `agent/<slug>-<short>` from `base` (default: the branch
/// the repository is on), checked out at `<repo>/.qs-worktrees/<slug>-<short>`
/// and prepared for building (`prepare`).
pub fn create(repo_root: &str, name: &str, base: Option<&str>) -> Result<(Worktree, Prepared), String> {
    let repo = Path::new(repo_root);
    if !repo.is_dir() {
        return Err(format!("{repo_root} is not a directory"));
    }
    let base = match base.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => {
            if !ref_exists(repo, b) {
                return Err(format!("base '{b}' is not a branch or commit in {repo_root}"));
            }
            b.to_string()
        }
        None => current_branch(repo)?,
    };
    let slug = slug(name);
    let (branch, dir) = loop {
        let short = short_id();
        let branch = format!("{BRANCH_PREFIX}{slug}-{short}");
        let dir = repo.join(DIR).join(format!("{slug}-{short}"));
        if !branch_exists(repo, &branch) && !dir.exists() {
            break (branch, dir);
        }
    };
    let parent = repo.join(DIR);
    std::fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    ensure_gitignore(repo);
    let dir_s = dir.to_string_lossy().into_owned();
    run(repo, &["worktree", "add", "-b", &branch, &dir_s, &base])?;
    let _ = run(repo, &["config", &format!("branch.{branch}.{BASE_KEY}"), &base]);
    let prepared = prepare(repo_root, &dir_s);
    Ok((Worktree { repo_root: repo_root.to_string(), path: dir_s, branch, base }, prepared))
}

// ── Links: dependency dirs shared with the main checkout ──

/// A directory link — a junction or directory symlink on Windows, a symlink
/// elsewhere. Junctions are reparse points but not "symlinks" to `std`'s
/// `FileType`, so the attribute is checked directly.
fn is_dir_link(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// `link` → `target`. A junction on Windows (`mklink /J` needs no
/// privilege, unlike a symlink), a symlink elsewhere.
fn link_dir(target: &Path, link: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Backslashes only: cmd takes a `/` inside a path for a switch
        // (`C:\repo/.qs-worktrees\x` fails with `Invalid switch -
        // ".qs-worktrees"`), and a caller may well hand over a mixed path.
        let link = native(&link.to_string_lossy());
        let target = native(&target.to_string_lossy());
        let out = Command::new("cmd")
            .args(["/C", "mklink", "/J", &link, &target])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("cmd could not be run ({e})"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err };
        Err(msg)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).map_err(|e| e.to_string())
    }
}

/// Remove a directory link without touching what it points to.
fn unlink_dir(link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::fs::remove_dir(link)
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(link)
    }
}

/// Make a fresh worktree buildable: the main checkout's ignored dependency
/// directories (`LINKED_DEP_DIRS` — `node_modules`, `apps/shell/node_modules`,
/// a `.venv`) are linked into it at the same relative paths. git never sees
/// them (they are ignored), and `remove_checkout` drops them before any
/// delete. Nothing else is shared — a Rust `target/` in particular is not.
pub fn prepare(repo_root: &str, wt_path: &str) -> Prepared {
    let repo = Path::new(repo_root);
    let wt = Path::new(wt_path);
    let mut out = Prepared::default();
    // The main checkout's ignored directories, one per line with a trailing
    // slash; `--directory` stops at the first ignored level, so this is
    // cheap even with a huge node_modules.
    let Ok(listing) = run(repo, &["ls-files", "--others", "--ignored", "--exclude-standard", "--directory"]) else {
        return out;
    };
    let mut rels: Vec<&str> = listing
        .lines()
        .map(|l| l.trim().trim_matches('"').trim_end_matches('/'))
        .filter(|l| !l.is_empty() && !l.starts_with(DIR))
        .filter(|l| l.rsplit('/').next().map(|leaf| LINKED_DEP_DIRS.contains(&leaf)).unwrap_or(false))
        .collect();
    rels.sort_unstable();
    rels.dedup();
    for rel in rels {
        let src = repo.join(rel);
        let dest = wt.join(rel);
        if !src.is_dir() || dest.exists() || is_dir_link(&dest) {
            continue;
        }
        // The parent is a tracked directory, so it exists in the worktree;
        // when it does not, the whole subtree is ignored and not ours to make.
        if !dest.parent().map(Path::is_dir).unwrap_or(false) {
            continue;
        }
        match link_dir(&src, &dest) {
            Ok(()) => out.linked.push(rel.to_string()),
            Err(e) => out.warnings.push(format!("{rel} ({e})")),
        }
    }
    out
}

/// Drop every directory link under `root` — depth-limited, never descending
/// into dependency or build trees — so no recursive delete, ours or git's,
/// can follow a junction into the main checkout's `node_modules`.
fn unlink_links(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_dir_link(&path) {
            if unlink_dir(&path).is_ok() {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        } else if depth < LINK_SCAN_DEPTH && !OPAQUE_DIRS.contains(&name.as_str()) && path.is_dir() {
            unlink_links(root, &path, depth + 1, out);
        }
    }
}

/// Delete a worktree's checkout and unregister it. Links inside are dropped
/// first (never followed). `git worktree remove --force` is retried — a
/// shell that just exited may still hold the folder on Windows — and when
/// git gives up after dropping the `.git` link (the folder then stays behind,
/// seen 2026-09-03) a plain recursive delete finishes the job. Whatever
/// happens to the folder, stale cargo outputs in the main checkout are reset.
pub fn remove_checkout(repo: &Path, path: &Path) -> Result<Removed, String> {
    let mut removed = Removed::default();
    let path_s = path.to_string_lossy().into_owned();
    let mut failure: Option<String> = None;
    if path.exists() {
        unlink_links(path, path, 0, &mut removed.unlinked);
        let mut last = String::new();
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(300));
            }
            if path.join(".git").exists() {
                if let Err(e) = run(repo, &["worktree", "remove", "--force", &path_s]) {
                    last = e;
                }
            }
            if !path.exists() {
                break;
            }
            if let Err(e) = std::fs::remove_dir_all(path) {
                last = e.to_string();
            }
            if !path.exists() {
                break;
            }
        }
        if path.exists() {
            failure = Some(format!(
                "the folder {path_s} could not be deleted ({last}) — something still holds it (a shell, an editor, a build). Close the process holding it to allow cleanup."
            ));
        }
    }
    let _ = run(repo, &["worktree", "prune"]);
    removed.cargo_reset = invalidate_stale_cargo_outputs(&repo.to_string_lossy());
    match failure {
        Some(f) => {
            let retry = match crate::worktree_cleanup::enqueue(repo, path, &f) {
                Ok(()) => "Automatic cleanup queued; retried each minute and when QuantSuite starts.".to_string(),
                Err(e) => format!("Automatic cleanup not queued: {e}"),
            };
            Err(format!("{f} {retry}"))
        }
        None => Ok(removed),
    }
}

// ── Cargo: outputs of the main checkout that point into a worktree ──

fn dirs_in(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

fn file_names_worktree(file: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(file) else { return false };
    if !meta.is_file() || meta.len() > CARGO_SCAN_FILE_CAP {
        return false;
    }
    let Ok(bytes) = std::fs::read(file) else { return false };
    let text = String::from_utf8_lossy(&bytes);
    text.contains(&format!("{DIR}/")) || text.contains(&format!("{DIR}\\"))
}

/// Does a `build/<crate>-<hash>/` unit's `output`, `root-output`, or a file
/// directly in its `out/` name a `.qs-worktrees` path?
fn unit_names_worktree(unit: &Path) -> bool {
    let mut files = vec![unit.join("output"), unit.join("root-output")];
    if let Ok(it) = std::fs::read_dir(unit.join("out")) {
        files.extend(it.flatten().map(|e| e.path()).filter(|p| p.is_file()).take(CARGO_SCAN_FILES_PER_UNIT));
    }
    files.iter().any(|f| file_names_worktree(f))
}

/// Cargo build-script outputs in `<repo>/target` that name a path under
/// `.qs-worktrees/`, dropped so cargo re-runs them; returns the crate names.
///
/// They only get there when a worktree build was pointed at the main
/// checkout's target dir (CARGO_TARGET_DIR): cargo keys `build/<crate>-<hash>/`
/// by the crate's path relative to the workspace root, so a worktree with the
/// same layout writes into the very same dirs, and Tauri plugins record their
/// permission files there as absolute paths. Once the worktree is deleted the
/// main build fails with "failed to read plugin permissions … .qs-worktrees\…".
/// Dropping the unit and its `.fingerprint/` entry makes cargo re-run the build
/// script against the main checkout — a few seconds, not a rebuild.
pub fn invalidate_stale_cargo_outputs(repo_root: &str) -> Vec<String> {
    let target = Path::new(repo_root).join("target");
    if !target.is_dir() {
        return Vec::new();
    }
    // `target/<profile>/build` and `target/<triple>/<profile>/build`.
    let mut profiles = Vec::new();
    for lvl1 in dirs_in(&target) {
        if lvl1.join("build").is_dir() {
            profiles.push(lvl1.clone());
        }
        for lvl2 in dirs_in(&lvl1) {
            if lvl2.join("build").is_dir() {
                profiles.push(lvl2);
            }
        }
    }
    let mut names: Vec<String> = Vec::new();
    for profile in profiles {
        for unit in dirs_in(&profile.join("build")) {
            if !unit_names_worktree(&unit) {
                continue;
            }
            let Some(dir_name) = unit.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let _ = std::fs::remove_dir_all(&unit);
            let _ = std::fs::remove_dir_all(profile.join(".fingerprint").join(&dir_name));
            let krate = dir_name.rsplit_once('-').map(|(n, _)| n.to_string()).unwrap_or(dir_name);
            if !names.contains(&krate) {
                names.push(krate);
            }
        }
    }
    names.sort();
    names
}

/// Every worktree of the repository, the main checkout first.
pub fn list(repo_root: &str) -> Result<Vec<Entry>, String> {
    let repo = Path::new(repo_root);
    let text = run(repo, &["worktree", "list", "--porcelain"])?;
    let root_norm = norm(repo_root);
    let mut out = Vec::new();
    let mut path = String::new();
    let mut head = String::new();
    let mut branch: Option<String> = None;
    let flush = |path: &mut String, head: &mut String, branch: &mut Option<String>, out: &mut Vec<Entry>| {
        if path.is_empty() {
            return;
        }
        let p = native(path);
        let main = norm(&p) == root_norm;
        let base = branch.as_deref().and_then(|b| base_of(repo, b));
        out.push(Entry { path: p, branch: branch.take(), head: std::mem::take(head), main, base });
        path.clear();
    };
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            flush(&mut path, &mut head, &mut branch, &mut out);
            path = p.to_string();
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            head = h.chars().take(10).collect();
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.trim_start_matches("refs/heads/").to_string());
        }
    }
    flush(&mut path, &mut head, &mut branch, &mut out);
    Ok(out)
}

/// A worktree by its branch (`agent/x-1a2b3c` or `x-1a2b3c`), its folder
/// name, or its path. The main checkout never matches — it is not an
/// agent worktree, and a merge into itself makes no sense.
pub fn find(repo_root: &str, ident: &str) -> Result<Worktree, String> {
    let ident = ident.trim();
    if ident.is_empty() {
        return Err("name the worktree: its branch, its folder, or its path".into());
    }
    let repo = Path::new(repo_root);
    let entries = list(repo_root)?;
    let wanted = norm(ident);
    let leaf = wanted.rsplit('/').next().unwrap_or("").to_string();
    let hit = entries.iter().filter(|e| !e.main).find(|e| {
        let p = norm(&e.path);
        let dir = p.rsplit('/').next().unwrap_or("").to_string();
        let b = e.branch.as_deref().map(str::to_lowercase);
        p == wanted
            || p.ends_with(&format!("/{wanted}"))
            || dir == leaf
            || b.as_deref() == Some(wanted.as_str())
            || b.as_deref() == Some(format!("{BRANCH_PREFIX}{wanted}").as_str())
    });
    let Some(e) = hit else {
        let names: Vec<String> = entries
            .iter()
            .filter(|e| !e.main)
            .map(|e| e.branch.clone().unwrap_or_else(|| e.path.clone()))
            .collect();
        return Err(if names.is_empty() {
            format!("no agent worktrees in {repo_root}")
        } else {
            format!("no worktree '{ident}' in {repo_root}. Existing: {}", names.join(", "))
        });
    };
    let branch = e.branch.clone().ok_or_else(|| format!("the worktree at {} is detached; nothing to merge", e.path))?;
    let base = e.base.clone().unwrap_or_else(|| default_branch(repo));
    Ok(Worktree { repo_root: repo_root.to_string(), path: e.path.clone(), branch, base })
}

/// `XY path` lines of `git status --porcelain`.
fn parse_porcelain(text: &str) -> Vec<FileChange> {
    text.lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let (code, rest) = l.split_at(2);
            let mut path = rest.trim_start().to_string();
            if let Some((_, to)) = path.split_once(" -> ") {
                path = to.to_string();
            }
            let path = path.trim_matches('"').to_string();
            let x = code.as_bytes()[0] as char;
            let y = code.as_bytes()[1] as char;
            let kind = if code == "??" {
                "untracked"
            } else if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
                "conflict"
            } else if x == 'R' || y == 'R' {
                "renamed"
            } else if x == 'A' || y == 'A' {
                "added"
            } else if x == 'D' || y == 'D' {
                "deleted"
            } else {
                "modified"
            };
            FileChange { path, kind: kind.into() }
        })
        .collect()
}

/// `3 files changed, 10 insertions(+), 2 deletions(-)` → (10, 2).
fn parse_shortstat(text: &str) -> (u32, u32) {
    let mut add = 0;
    let mut del = 0;
    for part in text.split(',') {
        let part = part.trim();
        let n: u32 = part.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
        if part.contains("insertion") {
            add = n;
        } else if part.contains("deletion") {
            del = n;
        }
    }
    (add, del)
}

fn commits_ahead(cwd: &Path, base: &str, tip: &str) -> u32 {
    run(cwd, &["rev-list", "--count", &format!("{base}..{tip}")])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn status(wt: &Worktree) -> Status {
    let dir = Path::new(&wt.path);
    let exists = dir.is_dir() && dir.join(".git").exists();
    if !exists {
        return Status {
            branch: wt.branch.clone(),
            base: wt.base.clone(),
            path: wt.path.clone(),
            exists: false,
            commits: commits_ahead(Path::new(&wt.repo_root), &wt.base, &wt.branch),
            files: Vec::new(),
            additions: 0,
            deletions: 0,
        };
    }
    let files = run(dir, &["status", "--porcelain", "--untracked-files=all"]).map(|s| parse_porcelain(&s)).unwrap_or_default();
    let (additions, deletions) = run(dir, &["diff", "--shortstat", &wt.base]).map(|s| parse_shortstat(&s)).unwrap_or((0, 0));
    Status {
        branch: wt.branch.clone(),
        base: wt.base.clone(),
        path: wt.path.clone(),
        exists: true,
        commits: commits_ahead(dir, &wt.base, "HEAD"),
        files,
        additions,
        deletions,
    }
}

/// Everything the branch changed against its base — committed, staged and
/// unstaged alike — plus the untracked files as new-file diffs.
pub fn diff(wt: &Worktree, cap: usize) -> Result<Diff, String> {
    let dir = Path::new(&wt.path);
    if !dir.is_dir() {
        return Err(format!("the worktree at {} is gone", wt.path));
    }
    let cap = cap.clamp(4 * 1024, DIFF_CAP);
    let stat = run(dir, &["diff", "--stat=110", &wt.base])?;
    let mut text = run(dir, &["diff", &wt.base])?;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let untracked: Vec<String> = run(dir, &["ls-files", "--others", "--exclude-standard"])
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect();
    let mut truncated = false;
    for (i, file) in untracked.iter().enumerate() {
        if i >= UNTRACKED_IN_DIFF {
            text.push_str(&format!("# untracked (not shown): {file}\n"));
            continue;
        }
        if text.len() > cap {
            truncated = true;
            break;
        }
        // `/dev/null` is understood by git on every platform.
        if let Ok(d) = run_diff(dir, &["diff", "--no-index", "--", "/dev/null", file]) {
            text.push_str(&d);
            if !d.ends_with('\n') {
                text.push('\n');
            }
        }
    }
    if text.len() > cap {
        let mut cut = cap;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… diff cut here (too large) …\n");
        truncated = true;
    }
    let stat = if untracked.is_empty() {
        stat
    } else {
        format!("{stat}\n {} untracked file(s)", untracked.len()).trim_start().to_string()
    };
    Ok(Diff { stat, diff: text, truncated })
}

/// Stage everything and commit it; `Ok(false)` when there was nothing.
pub fn commit_all(dir: &Path, message: &str) -> Result<bool, String> {
    run(dir, &["add", "-A"])?;
    if run(dir, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(false);
    }
    run(dir, &["commit", "-q", "-m", message])?;
    Ok(true)
}

/// Land the branch on its base with `--no-ff`. Uncommitted work in the
/// worktree is committed first. The main checkout must be on the base
/// branch — nobody's branch is switched behind their back; on a conflict
/// the merge is aborted and the worktree stays for whoever sorts it out.
pub fn merge(wt: &Worktree, message: Option<&str>, cleanup: bool) -> Result<MergeResult, String> {
    let repo = Path::new(&wt.repo_root);
    let dir = Path::new(&wt.path);
    let label = wt.branch.trim_start_matches(BRANCH_PREFIX).to_string();
    let wip = message.map(str::trim).filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Work in progress on {}", wt.branch));
    let committed = if dir.is_dir() { commit_all(dir, &wip)? } else { false };
    let commits = commits_ahead(repo, &wt.base, &wt.branch);
    if commits == 0 {
        let mut message = format!("Nothing to merge — {} has no commits beyond {}.", wt.branch, wt.base);
        let mut cleaned = false;
        if cleanup {
            match remove(wt) {
                Ok(r) => {
                    cleaned = true;
                    for n in r.notes() {
                        message.push(' ');
                        message.push_str(&n);
                    }
                }
                Err(e) => message.push_str(&format!(" Cleanup failed: {e}")),
            }
        }
        return Ok(MergeResult { merged: false, commits: 0, committed, cleaned, message });
    }
    let on = current_branch(repo)?;
    if on != wt.base {
        return Err(format!(
            "The main checkout at {} is on '{on}', not '{}'. Switch it to '{}' (or ask the user to), then merge again.",
            wt.repo_root, wt.base, wt.base
        ));
    }
    let msg = format!("Merge agent worktree '{label}' ({})", wt.branch);
    if let Err(e) = run(repo, &["merge", "--no-ff", "-m", &msg, &wt.branch]) {
        let _ = run(repo, &["merge", "--abort"]);
        return Err(format!("Merge failed and was aborted — the worktree and branch are untouched. git said: {e}"));
    }
    let mut message = format!("Merged {} commit(s) from {} into {}.", commits, wt.branch, wt.base);
    let mut cleaned = false;
    if cleanup {
        match remove(wt) {
            Ok(r) => {
                cleaned = true;
                for n in r.notes() {
                    message.push(' ');
                    message.push_str(&n);
                }
            }
            Err(e) => message.push_str(&format!(" Cleanup failed: {e}")),
        }
    }
    Ok(MergeResult { merged: true, commits, committed, cleaned, message })
}

/// Delete the worktree and its branch, commits included (`remove_checkout`
/// for the folder). A folder that cannot be deleted keeps its branch, so a
/// second call finishes the job.
pub fn remove(wt: &Worktree) -> Result<Removed, String> {
    let repo = Path::new(&wt.repo_root);
    if norm(&wt.path) == norm(&wt.repo_root) {
        return Err("that is the repository's own checkout, not an agent worktree".into());
    }
    let removed = remove_checkout(repo, Path::new(&wt.path))?;
    if branch_exists(repo, &wt.branch) {
        run(repo, &["branch", "-D", &wt.branch])?;
    }
    let _ = run(repo, &["config", "--unset", &format!("branch.{}.{BASE_KEY}", wt.branch)]);
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_names() {
        assert_eq!(slug("Fix the login bug!"), "fix-the-login-bug");
        assert_eq!(slug("  "), "work");
        assert!(slug("a very long name that keeps going and going and going on").len() <= 32);
    }

    #[test]
    fn parses_porcelain_and_shortstat() {
        let files = parse_porcelain(" M src/a.rs\n?? new.txt\nA  b.rs\nD  c.rs\nR  old.rs -> new.rs\nUU x.rs\n");
        assert_eq!(files.len(), 6);
        assert_eq!(files[0], FileChange { path: "src/a.rs".into(), kind: "modified".into() });
        assert_eq!(files[1].kind, "untracked");
        assert_eq!(files[4], FileChange { path: "new.rs".into(), kind: "renamed".into() });
        assert_eq!(files[5].kind, "conflict");
        assert_eq!(parse_shortstat(" 3 files changed, 10 insertions(+), 2 deletions(-)"), (10, 2));
        assert_eq!(parse_shortstat(""), (0, 0));
    }

    fn init_repo(dir: &Path) -> Result<(), String> {
        run(dir, &["init", "-q"])?;
        run(dir, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
        run(dir, &["config", "user.name", "QuantMCP Test"])?;
        run(dir, &["config", "user.email", "mcp@example.invalid"])?;
        run(dir, &["config", "commit.gpgsign", "false"])?;
        run(dir, &["config", "core.autocrlf", "false"])?;
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "node_modules/\n").unwrap();
        run(dir, &["add", "."])?;
        run(dir, &["commit", "-q", "-m", "init"])?;
        Ok(())
    }

    /// The whole life of a worktree against a real repository: create (with
    /// the main checkout's `node_modules` linked in), find it three ways,
    /// change, status, diff, merge with cleanup — then the main checkout has
    /// the change, its `node_modules` is intact, and nothing of the worktree
    /// is left.
    #[test]
    fn round_trip_in_a_temp_repo() {
        if git().arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
            eprintln!("skipped: no git here");
            return;
        }
        let root = std::env::temp_dir().join(format!("qs-mcp-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root).expect("repo");
        let repo = repo_root(&root).expect("a repo");
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules").join("marker.txt"), "dep\n").unwrap();

        let (wt, prep) = create(&repo, "Fix the thing", None).expect("worktree");
        assert!(wt.branch.starts_with("agent/fix-the-thing-"), "{}", wt.branch);
        assert_eq!(wt.base, "main");
        assert!(Path::new(&wt.path).join("README.md").is_file());
        assert!(std::fs::read_to_string(root.join(".gitignore")).unwrap().contains(".qs-worktrees/"));
        assert_eq!(prep.linked, vec!["node_modules".to_string()], "{prep:?}");
        let linked = Path::new(&wt.path).join("node_modules");
        assert!(is_dir_link(&linked), "a junction / symlink, not a copy");
        assert_eq!(std::fs::read_to_string(linked.join("marker.txt")).unwrap(), "dep\n", "readable through the link");

        let entries = list(&repo).expect("list");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].main);
        assert_eq!(entries[1].branch.as_deref(), Some(wt.branch.as_str()));
        assert_eq!(entries[1].base.as_deref(), Some("main"), "the base is remembered in git config");

        let short = wt.branch.trim_start_matches(BRANCH_PREFIX).to_string();
        assert_eq!(find(&repo, &wt.branch).unwrap().path, wt.path, "by branch");
        assert_eq!(find(&repo, &short).unwrap().path, wt.path, "by branch without the prefix / folder name");
        assert_eq!(find(&repo, &wt.path).unwrap().branch, wt.branch, "by path");
        assert!(find(&repo, "main").is_err(), "the main checkout is not an agent worktree");

        std::fs::write(Path::new(&wt.path).join("README.md"), "hello\nworld\n").unwrap();
        std::fs::write(Path::new(&wt.path).join("new.txt"), "brand new\n").unwrap();
        let st = status(&wt);
        assert!(st.exists);
        assert_eq!(st.commits, 0);
        assert_eq!(st.files.len(), 2, "{:?}", st.files);
        assert_eq!(st.additions, 1);
        let d = diff(&wt, DIFF_CAP).expect("diff");
        assert!(d.diff.contains("+world"), "{}", d.diff);
        assert!(d.diff.contains("+brand new"), "untracked files are in the diff: {}", d.diff);
        assert!(!d.truncated);

        let r = merge(&wt, Some("finish the thing"), true).expect("merge");
        assert!(r.merged && r.committed && r.cleaned, "{r:?}");
        assert_eq!(r.commits, 1);
        assert!(r.message.contains("Dropped 1 dependency link"), "{}", r.message);
        assert_eq!(std::fs::read_to_string(root.join("README.md")).unwrap().replace("\r\n", "\n"), "hello\nworld\n");
        assert!(root.join("new.txt").is_file());
        assert!(!Path::new(&wt.path).exists());
        assert!(root.join("node_modules").join("marker.txt").is_file(), "the link was dropped, never followed");
        assert!(!branch_exists(&root, &wt.branch));
        let log = run(&root, &["log", "--oneline", "-3"]).unwrap();
        assert!(log.contains("Merge agent worktree 'fix-the-thing-"), "{log}");
        assert!(log.contains("finish the thing"), "{log}");
        assert_eq!(list(&repo).unwrap().len(), 1);

        // An explicit base, and nothing to merge: cleaned up without a merge commit.
        let (wt2, _) = create(&repo, "", Some("main")).expect("second");
        assert!(wt2.branch.starts_with("agent/work-"));
        let r2 = merge(&wt2, None, true).expect("empty merge");
        assert!(!r2.merged && r2.cleaned && r2.commits == 0);
        assert!(root.join("node_modules").join("marker.txt").is_file());
        assert!(create(&repo, "x", Some("no-such-branch")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A kanban card's worktree (`git_helpers::create_worktree`) gets the
    /// main checkout's `node_modules` like `create` does. Regression: the
    /// path was built with a forward slash, and `cmd /C mklink` read
    /// `/.qs-worktrees` as a switch — no card worktree was ever buildable
    /// (2026-09-04).
    #[test]
    fn card_worktree_links_node_modules() {
        if git().arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
            eprintln!("skipped: no git here");
            return;
        }
        let root = std::env::temp_dir().join(format!("qs-mcp-card-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root).expect("repo");
        let repo = repo_root(&root).expect("a repo");
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules").join("marker.txt"), "dep\n").unwrap();

        let (branch, wt_path, prep) =
            crate::git_helpers::create_worktree(&repo, "33742415-5b86-423f-b63c-2a0a4d9bd8bd", "claude-code-5")
                .expect("card worktree");
        assert_eq!(branch, "agent/claude-code-5-33742415");
        assert_eq!(Path::new(&wt_path).parent().and_then(Path::file_name).and_then(|n| n.to_str()), Some(DIR));
        assert!(Path::new(&wt_path).join("README.md").is_file());
        assert_eq!(prep.linked, vec!["node_modules".to_string()], "{prep:?}");
        assert!(prep.warnings.is_empty(), "{prep:?}");
        let linked = Path::new(&wt_path).join("node_modules");
        assert!(is_dir_link(&linked), "a junction / symlink, not a copy");
        assert_eq!(std::fs::read_to_string(linked.join("marker.txt")).unwrap(), "dep\n", "readable through the link");

        let removed = crate::git_helpers::remove_worktree(&repo, &wt_path).expect("removed");
        assert_eq!(removed.unlinked.len(), 1, "{removed:?}");
        assert!(!Path::new(&wt_path).exists());
        assert!(root.join("node_modules").join("marker.txt").is_file(), "the link was dropped, never followed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `link_dir` hands both paths to `cmd`, which reads a `/` in either as
    /// a switch — so it normalises them itself, whatever a caller passes.
    #[cfg(windows)]
    #[test]
    fn link_dir_takes_mixed_separators() {
        let root = std::env::temp_dir().join(format!("qs-mcp-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("marker.txt"), "dep\n").unwrap();
        let mixed = root.to_string_lossy().replace('\\', "/");
        let target = format!("{mixed}\\src");
        let link = format!("{mixed}/lnk");
        link_dir(Path::new(&target), Path::new(&link)).expect("a junction despite forward slashes");
        assert!(is_dir_link(&root.join("lnk")));
        assert_eq!(std::fs::read_to_string(root.join("lnk").join("marker.txt")).unwrap(), "dep\n");
        unlink_dir(&root.join("lnk")).unwrap();
        assert!(root.join("src").join("marker.txt").is_file(), "unlinked, not deleted through");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The user's test commands: worktree first, the MAIN checkout's target,
    /// native separators per shell — and derivable from a card's stored
    /// path alone.
    #[test]
    fn test_commands_share_the_main_target() {
        let tc = test_commands("C:/Projects/App", "C:/Projects/App/.qs-worktrees/fa286e99");
        if cfg!(windows) {
            assert_eq!(
                tc.powershell,
                "cd \"C:\\Projects\\App\\.qs-worktrees\\fa286e99\"; $env:CARGO_TARGET_DIR = \"C:\\Projects\\App\\target\"; npm run tauri:dev"
            );
            assert_eq!(
                tc.cmd,
                "cd /d \"C:\\Projects\\App\\.qs-worktrees\\fa286e99\" && set \"CARGO_TARGET_DIR=C:\\Projects\\App\\target\" && npm run tauri:dev"
            );
        }
        assert_eq!(
            tc.bash,
            "cd \"C:/Projects/App/.qs-worktrees/fa286e99\" && CARGO_TARGET_DIR=\"C:/Projects/App/target\" npm run tauri:dev"
        );
        assert!(tc.lines()[1].starts_with("cmd: cd /d"));
        // A card stores `<registry path>/.qs-worktrees/<short>` — mixed separators on Windows.
        assert_eq!(test_commands_from_path("C:\\Projects\\App/.qs-worktrees/fa286e99"), Some(tc.clone()));
        assert_eq!(test_commands_from_path("C:/Projects/App/.qs-worktrees/fa286e99/"), Some(tc));
        assert!(test_commands_from_path("C:/Projects/App/elsewhere/fa286e99").is_none(), "not a suite worktree");
        assert!(test_commands_from_path("C:/Projects/App").is_none());
        assert!(test_commands_from_path("").is_none());
    }

    /// A build that shared CARGO_TARGET_DIR leaves absolute worktree paths in
    /// the main checkout's build-script outputs; exactly those units go —
    /// in `target/debug` and under a target triple — clean ones stay.
    #[test]
    fn resets_only_stale_cargo_outputs() {
        let root = std::env::temp_dir().join(format!("qs-mcp-cargo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let debug = root.join("target").join("debug");
        let stale = debug.join("build").join("tauri-plugin-x-0123456789abcdef");
        let clean = debug.join("build").join("serde-fedcba9876543210");
        std::fs::create_dir_all(stale.join("out")).unwrap();
        std::fs::create_dir_all(&clean).unwrap();
        std::fs::create_dir_all(debug.join(".fingerprint").join("tauri-plugin-x-0123456789abcdef")).unwrap();
        std::fs::create_dir_all(debug.join(".fingerprint").join("serde-fedcba9876543210")).unwrap();
        std::fs::write(stale.join("output"), "cargo:rerun-if-changed=permissions\n").unwrap();
        std::fs::write(
            stale.join("out").join("tauri-plugin-x-permission-files"),
            r"[\\?\C:\Projects\App\.qs-worktrees\ab12\modules\x\permissions\a.toml]",
        )
        .unwrap();
        std::fs::write(clean.join("output"), r"cargo:rerun-if-changed=C:\Projects\App\build.rs").unwrap();
        let triple = root.join("target").join("x86_64-pc-windows-msvc").join("release").join("build").join("quantsuite-1111111111111111");
        std::fs::create_dir_all(&triple).unwrap();
        std::fs::write(triple.join("output"), "cargo:rerun-if-changed=/home/u/app/.qs-worktrees/x-1/tauri.conf.json\n").unwrap();

        let names = invalidate_stale_cargo_outputs(&root.to_string_lossy());
        assert_eq!(names, vec!["quantsuite".to_string(), "tauri-plugin-x".to_string()]);
        assert!(!stale.exists(), "the poisoned unit is gone");
        assert!(!debug.join(".fingerprint").join("tauri-plugin-x-0123456789abcdef").exists(), "and its fingerprint");
        assert!(!triple.exists());
        assert!(clean.join("output").is_file() && debug.join(".fingerprint").join("serde-fedcba9876543210").is_dir());
        assert!(invalidate_stale_cargo_outputs(&root.to_string_lossy()).is_empty(), "a second pass finds nothing");
        assert!(invalidate_stale_cargo_outputs(&root.join("nope").to_string_lossy()).is_empty(), "no target dir, no work");
        let _ = std::fs::remove_dir_all(&root);
    }
}
