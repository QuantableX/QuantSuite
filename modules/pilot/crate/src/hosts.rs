//! Which host a checkout's `origin` is on (round 4, card 6), and the one way
//! the view and the stacks ask for lists, merges and new bases. Only GitHub and
//! GitLab: nothing uses anything else.

use crate::github::{self, Issue, Pull, Repo};
use crate::gitlab;
use crate::gitops;
use crate::stacks::{self, Merged, Outcome, Plan};
use crate::contexts;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    Github,
    Gitlab,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRepo {
    pub kind: HostKind,
    pub domain: String,
    pub repo: Repo,
}

impl HostRepo {
    pub fn url(&self) -> String {
        match self.kind {
            HostKind::Github => self.repo.url(),
            HostKind::Gitlab => format!("https://{}/{}", self.domain, self.repo.slug()),
        }
    }
}

/// The domain and path of a remote: https, ssh or scp form.
fn split_remote(url: &str) -> Option<(String, String)> {
    let u = url.trim();
    let (domain, path) = if let Some(rest) = ["https://", "http://", "ssh://", "git://"].iter().find_map(|p| u.strip_prefix(p)) {
        let (host, path) = rest.split_once('/')?;
        // user@ and :port are not part of the name.
        let host = host.rsplit('@').next()?;
        (host.split(':').next()?.to_string(), path.to_string())
    } else {
        // git@host:path; `C:/path` and `/path` have no user.
        let (user_host, path) = u.split_once(':')?;
        let (_, host) = user_host.split_once('@')?;
        (host.to_string(), path.to_string())
    };
    (!domain.is_empty()).then_some((domain.to_lowercase(), path))
}

/// gitlab.com, a server named gitlab.something, or the one `GITLAB_HOST` names.
fn is_gitlab(domain: &str) -> bool {
    domain == "gitlab.com" || domain.split('.').any(|label| label == "gitlab") || gitlab::default_domain().eq_ignore_ascii_case(domain)
}

/// Which host and project a remote address means.
pub fn parse(url: &str) -> Option<HostRepo> {
    if let Some(repo) = github::parse_remote(url) {
        return Some(HostRepo { kind: HostKind::Github, domain: "github.com".into(), repo });
    }
    let (domain, path) = split_remote(url)?;
    if !is_gitlab(&domain) {
        return None;
    }
    let path = path.trim_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = path.split('/').collect();
    let valid = |s: &&str| !s.is_empty() && *s != "." && *s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if parts.len() < 2 || !parts.iter().all(valid) {
        return None;
    }
    let (name, owner) = parts.split_last()?;
    Some(HostRepo { kind: HostKind::Gitlab, domain, repo: Repo { owner: owner.join("/"), name: (*name).to_string() } })
}

/// The host behind a checkout's `origin` — git only, no CLI. The address as
/// written counts too, for one a local rule rewrites (`url.*.insteadOf`).
pub fn repo_of(cwd: &Path) -> Option<HostRepo> {
    let read = |args: &[&str]| gitops::run("git", cwd, args, Duration::from_secs(10)).ok();
    read(&["remote", "get-url", "origin"]).and_then(|u| parse(&u)).or_else(|| read(&["config", "--get", "remote.origin.url"]).and_then(|u| parse(&u)))
}

/// A host the switch allows.
fn usable(r: HostRepo) -> Option<HostRepo> {
    let on = match r.kind {
        HostKind::Github => github::enabled(),
        HostKind::Gitlab => gitlab::enabled(),
    };
    on.then_some(r)
}

pub fn repo_if_on(cwd: &Path) -> Option<HostRepo> {
    repo_of(cwd).and_then(usable)
}

// ── The same questions of either host ──

pub fn pulls(cwd: &Path, repo: &HostRepo, search: &str) -> Result<Vec<Pull>, String> {
    let rows = match repo.kind {
        HostKind::Github => github::pulls(cwd, &repo.repo, search)?,
        HostKind::Gitlab => gitlab::pulls(cwd, repo, search)?,
    };
    // A search shows part of a chain at most: those rows stay as they are.
    Ok(if search.trim().is_empty() { stacks::arrange(rows) } else { rows })
}

pub fn issues(cwd: &Path, repo: &HostRepo, search: &str) -> Result<Vec<Issue>, String> {
    match repo.kind {
        HostKind::Github => github::issues(cwd, &repo.repo, search),
        HostKind::Gitlab => gitlab::issues(cwd, repo, search),
    }
}

pub fn merge_when_ready(cwd: &Path, repo: &HostRepo, number: i64) -> Result<String, String> {
    match repo.kind {
        HostKind::Github => github::merge_when_ready(cwd, &repo.repo, number),
        HostKind::Gitlab => gitlab::merge_when_ready(cwd, repo, number),
    }
}

fn merged(cwd: &Path, repo: &HostRepo) -> Result<Vec<Merged>, String> {
    match repo.kind {
        HostKind::Github => github::merged(cwd, &repo.repo),
        HostKind::Gitlab => gitlab::merged(cwd, repo),
    }
}

fn retarget(cwd: &Path, repo: &HostRepo, number: i64, base: &str) -> Result<(), String> {
    match repo.kind {
        HostKind::Github => github::retarget(cwd, &repo.repo, number, base),
        HostKind::Gitlab => gitlab::retarget(cwd, repo, number, base),
    }
}

/// The new base's command as a person would read it.
fn retarget_shown(repo: &HostRepo, number: i64, base: &str) -> String {
    match repo.kind {
        HostKind::Github => github::retarget_args(&repo.repo, number, base).iter().fold("gh".to_string(), |all, a| format!("{all} {a}")),
        HostKind::Gitlab => gitlab::retarget_args(repo, number, base).iter().fold("glab".to_string(), |all, a| format!("{all} {a}")),
    }
}

// ── Commands ──

fn workspace(app: &AppHandle, context_id: &str) -> Result<(PathBuf, HostRepo), String> {
    let ctx = contexts::resolve(app, context_id).ok_or("unknown workspace")?;
    let cwd = PathBuf::from(&ctx.path);
    let repo = repo_if_on(&cwd).ok_or_else(|| format!("{} has no GitHub or GitLab remote that is turned on.", ctx.name))?;
    Ok((cwd, repo))
}

fn plan_for(cwd: &Path, repo: &HostRepo, root: i64) -> Result<Plan, String> {
    let rows = pulls(cwd, repo, "")?;
    let changes: Vec<_> = rows.iter().map(stacks::change_of).collect();
    let done = merged(cwd, repo)?;
    let sign = if repo.kind == HostKind::Gitlab { "!" } else { "#" };
    stacks::build_plan_as(cwd, root, &changes, &done, sign, &|n, b| retarget_shown(repo, n, b))
}

#[tauri::command(async)]
pub fn pilot_gh_pulls(app: AppHandle, context_id: String, search: Option<String>) -> Result<Vec<Pull>, String> {
    let (cwd, repo) = workspace(&app, &context_id)?;
    pulls(&cwd, &repo, search.as_deref().unwrap_or_default())
}

#[tauri::command(async)]
pub fn pilot_gh_issues(app: AppHandle, context_id: String, search: Option<String>) -> Result<Vec<Issue>, String> {
    let (cwd, repo) = workspace(&app, &context_id)?;
    issues(&cwd, &repo, search.as_deref().unwrap_or_default())
}

#[tauri::command(async)]
pub fn pilot_gh_merge(app: AppHandle, context_id: String, number: i64) -> Result<String, String> {
    let (cwd, repo) = workspace(&app, &context_id)?;
    merge_when_ready(&cwd, &repo, number)
}

/// What updating a chain would do. Fetches; changes nothing else.
#[tauri::command(async)]
pub fn pilot_stack_plan(app: AppHandle, context_id: String, root: i64) -> Result<Plan, String> {
    let (cwd, repo) = workspace(&app, &context_id)?;
    plan_for(&cwd, &repo, root)
}

/// Runs the plan the user saw: looked at again first, and refused when it
/// would now do something else.
#[tauri::command(async)]
pub fn pilot_stack_update(app: AppHandle, context_id: String, root: i64, plan_id: String) -> Result<Outcome, String> {
    let (cwd, repo) = workspace(&app, &context_id)?;
    let plan = plan_for(&cwd, &repo, root)?;
    if plan.id != plan_id {
        return Err("The chain changed since you looked. Look at the plan again.".into());
    }
    Ok(stacks::run_plan(&cwd, &plan, &|n, b| retarget(&cwd, &repo, n, b)))
}

/// Settings → Source control: is `glab` installed and signed in.
#[tauri::command(async)]
pub fn pilot_gl_status() -> Result<(), String> {
    gitlab::ready(&gitlab::default_domain())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_says_which_host_it_is() {
        for url in ["https://github.com/o/r.git", "git@github.com:o/r.git", "ssh://git@github.com/o/r", "https://github.com/o/r/"] {
            let r = parse(url).unwrap();
            assert_eq!((r.kind, r.repo.slug().as_str()), (HostKind::Github, "o/r"), "{url}");
            assert_eq!(r.url(), "https://github.com/o/r");
        }
        for (url, domain, slug) in [
            ("https://gitlab.com/group/tool.git", "gitlab.com", "group/tool"),
            ("git@gitlab.com:group/sub/tool.git", "gitlab.com", "group/sub/tool"),
            ("ssh://git@gitlab.com/group/tool", "gitlab.com", "group/tool"),
            ("ssh://git@gitlab.example.com:2222/group/sub/deep/tool.git", "gitlab.example.com", "group/sub/deep/tool"),
            ("https://deploy:secret@gitlab.example.com/a/b.git", "gitlab.example.com", "a/b"),
            ("https://GitLab.com/a/b", "gitlab.com", "a/b"),
        ] {
            let r = parse(url).unwrap_or_else(|| panic!("{url}"));
            assert_eq!((r.kind, r.domain.as_str(), r.repo.slug().as_str()), (HostKind::Gitlab, domain, slug), "{url}");
        }
        let r = parse("git@gitlab.com:group/sub/tool.git").unwrap();
        assert_eq!(r.url(), "https://gitlab.com/group/sub/tool");
        assert_eq!((r.repo.owner.as_str(), r.repo.name.as_str()), ("group/sub", "tool"));
    }

    #[test]
    fn other_addresses_are_no_host() {
        for url in ["https://bitbucket.org/o/r.git", "https://dev.azure.com/o/p/_git/r", "C:/repos/bare.git", "/srv/git/r.git", "../sibling.git", "git@gitlab.com:toolonly", "https://gitlab.com/a/../b", "https://notgitlab.com/a/b", ""] {
            assert_eq!(parse(url), None, "{url}");
        }
    }

    #[test]
    fn the_address_as_written_counts_when_a_rule_rewrites_it() {
        let dir = std::env::temp_dir().join(format!("qs-hosts-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let g = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&dir).status().unwrap().success());
        g(&["init", "-q"]);
        g(&["remote", "add", "origin", "https://gitlab.com/group/tool.git"]);
        g(&["config", "url.C:/somewhere/mirror.git.insteadOf", "https://gitlab.com/group/tool.git"]);
        assert_eq!(repo_of(&dir).unwrap().repo.slug(), "group/tool");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
