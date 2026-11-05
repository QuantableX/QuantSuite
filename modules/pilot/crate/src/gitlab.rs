//! GitLab through the signed-in `glab` (round 4, card 6), by GitLab's own REST
//! API through `glab api`, so what comes back is documented and does not shift
//! with the CLI's table output. No tokens are stored and nothing runs when the
//! switch in Settings → Source control is off. Merge requests and issues come
//! out in the same shape as GitHub's, so the view does not care which it is.

use crate::github::{BrowsePage, Checks, Issue, Pull, Remote};
use crate::gitops;
use crate::hosts::HostRepo;
use crate::stacks::Merged;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GLAB_TIMEOUT: Duration = Duration::from_secs(30);
const PER_PAGE: &str = "30";

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The GitLab Browse and the status look at: `GITLAB_HOST`, as `glab` itself
/// reads it, else gitlab.com. A checkout's own domain comes from its remote.
pub fn default_domain() -> String {
    std::env::var("GITLAB_HOST")
        .ok()
        .map(|h| h.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "gitlab.com".into())
}

/// What to do about a `glab` that cannot help yet.
pub fn sign_in_hint(domain: &str, complaint: &str) -> String {
    let c = complaint.to_lowercase();
    if c.contains("program not found") || c.contains("cannot find") || c.contains("os error 2") || c.contains("could not start") {
        return "Install the GitLab CLI (glab) and sign in with `glab auth login` to see merge requests and issues.".into();
    }
    if domain == "gitlab.com" {
        return "Sign in to GitLab with `glab auth login`.".into();
    }
    format!("Sign in to {domain} with `glab auth login --hostname {domain}`.")
}

/// `glab` may run for this GitLab: the switch is on, installed and signed in.
pub fn ready(domain: &str) -> Result<(), String> {
    type Cache = Mutex<Option<HashMap<String, (Instant, Result<(), String>)>>>;
    static CACHE: Cache = Mutex::new(None);
    if !enabled() {
        return Err("GitLab is off in Settings → Source control.".into());
    }
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some((at, r)) = map.get(domain) {
        if at.elapsed() < Duration::from_secs(300) {
            return r.clone();
        }
    }
    let r = gitops::run("glab", &std::env::temp_dir(), &["auth", "status", "--hostname", domain], Duration::from_secs(20)).map(|_| ()).map_err(|e| sign_in_hint(domain, &e));
    map.insert(domain.to_string(), (Instant::now(), r.clone()));
    r
}

/// `glab`'s complaint, readable and without anything token-shaped.
pub fn error_text(raw: &str, domain: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("rate limit") || lower.contains("429") {
        return "GitLab's rate limit is reached — try again in a few minutes.".into();
    }
    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("auth login") {
        return sign_in_hint(domain, "");
    }
    if lower.contains("could not resolve host") || lower.contains("no such host") || lower.contains("connection refused") || lower.contains("timeout") {
        return format!("{domain} cannot be reached — check the connection.");
    }
    let re = regex::Regex::new(r"(glpat-[A-Za-z0-9_\-]{10,}|glrt-[A-Za-z0-9_\-]{10,})").expect("a valid pattern");
    re.replace_all(raw.trim(), "[token]").chars().take(400).collect()
}

/// A path or value for a URL: everything but the unreserved characters encoded.
pub fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn project(repo: &HostRepo) -> String {
    format!("projects/{}", encoded(&repo.repo.slug()))
}

// ── argv ──

fn get(domain: &str, path: String) -> Vec<String> {
    vec!["api".into(), "--hostname".into(), domain.into(), path]
}

fn put(domain: &str, path: String) -> Vec<String> {
    vec!["api".into(), "--hostname".into(), domain.into(), "-X".into(), "PUT".into(), path]
}

fn with_search(mut path: String, search: &str) -> String {
    if !search.trim().is_empty() {
        path.push_str(&format!("&search={}", encoded(search.trim())));
    }
    path
}

pub fn mr_list_args(repo: &HostRepo, search: &str) -> Vec<String> {
    get(&repo.domain, with_search(format!("{}/merge_requests?state=opened&per_page={PER_PAGE}&order_by=updated_at", project(repo)), search))
}

pub fn issue_list_args(repo: &HostRepo, search: &str) -> Vec<String> {
    get(&repo.domain, with_search(format!("{}/issues?state=opened&per_page={PER_PAGE}&order_by=updated_at", project(repo)), search))
}

pub fn merged_args(repo: &HostRepo) -> Vec<String> {
    get(&repo.domain, format!("{}/merge_requests?state=merged&per_page={PER_PAGE}&order_by=updated_at", project(repo)))
}

pub fn mr_view_args(repo: &HostRepo, iid: i64) -> Vec<String> {
    get(&repo.domain, format!("{}/merge_requests/{iid}", project(repo)))
}

pub fn merge_args(repo: &HostRepo, iid: i64) -> Vec<String> {
    put(&repo.domain, format!("{}/merge_requests/{iid}/merge", project(repo)))
}

pub fn retarget_args(repo: &HostRepo, iid: i64, base: &str) -> Vec<String> {
    let mut a = put(&repo.domain, format!("{}/merge_requests/{iid}", project(repo)));
    a.extend(["-f".into(), format!("target_branch={base}")]);
    a
}

pub fn browse_args(domain: &str, query: &str, page: u32) -> Vec<String> {
    get(domain, with_search(format!("projects?membership=true&per_page=50&page={page}&order_by=last_activity_at&simple=false"), query))
}

fn json(cwd: &Path, domain: &str, args: &[String]) -> Result<Value, String> {
    ready(domain)?;
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = gitops::run_whole("glab", cwd, &argv, GLAB_TIMEOUT).map_err(|e| error_text(&e, domain))?;
    serde_json::from_str(&out).map_err(|e| format!("glab answered something else: {e}"))
}

// ── JSON ──

/// `detailed_merge_status` in words; what a person needs to do about it.
fn status_note(status: &str) -> String {
    match status {
        "" | "mergeable" | "requested_changes" | "draft_status" | "not_open" => String::new(),
        "not_approved" | "approvals_syncing" => "Needs approval".into(),
        "ci_must_pass" | "status_checks_must_pass" => "Pipeline must pass".into(),
        "ci_still_running" => "Pipeline running".into(),
        "conflict" => "Has conflicts".into(),
        "need_rebase" => "Needs a rebase".into(),
        "discussions_not_resolved" => "Open discussions".into(),
        "checking" | "unchecked" | "preparing" => "Being checked".into(),
        other => {
            let words = other.replace('_', " ");
            let mut c = words.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

fn labels(v: &Value) -> Vec<String> {
    v.as_array().map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

pub fn pull_from(v: &Value) -> Option<Pull> {
    let status = v["detailed_merge_status"].as_str().unwrap_or_default();
    let draft = v["draft"].as_bool().or_else(|| v["work_in_progress"].as_bool()).unwrap_or(false);
    let foreign = match (v["source_project_id"].as_i64(), v["target_project_id"].as_i64()) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    };
    Some(Pull {
        number: v["iid"].as_i64()?,
        title: v["title"].as_str().unwrap_or_default().trim().into(),
        url: v["web_url"].as_str().unwrap_or_default().into(),
        state: if draft { "draft" } else { "open" }.into(),
        author: v["author"]["username"].as_str().unwrap_or_default().into(),
        branch: v["source_branch"].as_str().unwrap_or_default().into(),
        base: v["target_branch"].as_str().unwrap_or_default().into(),
        head_oid: v["sha"].as_str().unwrap_or_default().into(),
        foreign,
        review: if status == "requested_changes" { "CHANGES_REQUESTED" } else { "" }.into(),
        checks: Checks::default(),
        additions: 0,
        deletions: 0,
        labels: labels(&v["labels"]),
        updated_at: v["updated_at"].as_str().unwrap_or_default().into(),
        // GitLab's own verdict: pipeline, approvals, discussions, conflicts.
        ready: status == "mergeable" && !draft,
        note: status_note(status),
        stack: None,
    })
}

pub fn issue_from(v: &Value) -> Option<Issue> {
    Some(Issue {
        number: v["iid"].as_i64()?,
        title: v["title"].as_str().unwrap_or_default().trim().into(),
        url: v["web_url"].as_str().unwrap_or_default().into(),
        author: v["author"]["username"].as_str().unwrap_or_default().into(),
        labels: labels(&v["labels"]),
        comments: v["user_notes_count"].as_i64().unwrap_or(0),
        updated_at: v["updated_at"].as_str().unwrap_or_default().into(),
    })
}

pub fn merged_from(v: &Value) -> Option<Merged> {
    Some(Merged {
        number: v["iid"].as_i64()?,
        branch: v["source_branch"].as_str()?.to_string(),
        base: v["target_branch"].as_str()?.to_string(),
        head: v["sha"].as_str()?.to_string(),
    })
}

/// One project the signed-in account belongs to. Access: GitLab's levels, 40
/// Maintainer and up is `admin`, 30 Developer is `write`.
pub fn remote_from(v: &Value) -> Option<Remote> {
    let slug = v["path_with_namespace"].as_str()?.to_string();
    let (owner, name) = slug.rsplit_once('/')?;
    let level = |k: &str| v["permissions"][k]["access_level"].as_i64().unwrap_or(0);
    let access = level("project_access").max(level("group_access"));
    Some(Remote {
        owner: owner.to_string(),
        name: name.to_string(),
        description: v["description"].as_str().unwrap_or_default().chars().take(300).collect(),
        private: v["visibility"].as_str().is_some_and(|s| s != "public"),
        archived: v["archived"] == true,
        fork: v["forked_from_project"].is_object(),
        access: if access >= 40 { "admin" } else if access >= 30 { "write" } else { "read" }.into(),
        updated_at: v["last_activity_at"].as_str().unwrap_or_default().to_string(),
        local: Vec::new(),
        slug,
    })
}

/// A merge request's or issue's address: its domain, project and number.
pub fn parse_item_url(url: &str) -> Option<(String, String, &'static str, i64)> {
    let rest = url.trim().strip_prefix("https://").or_else(|| url.trim().strip_prefix("http://"))?;
    let (domain, path) = rest.split_once('/')?;
    let (project, tail) = path.split_once("/-/")?;
    let (kind, number) = tail.split_once('/')?;
    let number = number.split(['/', '?', '#']).next()?.parse().ok()?;
    let kind = match kind {
        "merge_requests" => "merge_requests",
        "issues" => "issues",
        _ => return None,
    };
    Some((domain.to_string(), project.to_string(), kind, number))
}

/// The state a linked thread's chip shows.
pub fn state_word(v: &Value) -> &'static str {
    match v["state"].as_str().unwrap_or_default() {
        "merged" => "merged",
        "closed" | "locked" => "closed",
        _ if v["draft"].as_bool().unwrap_or(false) => "draft",
        _ => "open",
    }
}

// ── Calls ──

pub fn pulls(cwd: &Path, repo: &HostRepo, search: &str) -> Result<Vec<Pull>, String> {
    let v = json(cwd, &repo.domain, &mr_list_args(repo, search))?;
    Ok(v.as_array().into_iter().flatten().filter_map(pull_from).collect())
}

pub fn issues(cwd: &Path, repo: &HostRepo, search: &str) -> Result<Vec<Issue>, String> {
    let v = json(cwd, &repo.domain, &issue_list_args(repo, search))?;
    Ok(v.as_array().into_iter().flatten().filter_map(issue_from).collect())
}

pub fn merged(cwd: &Path, repo: &HostRepo) -> Result<Vec<Merged>, String> {
    let v = json(cwd, &repo.domain, &merged_args(repo))?;
    Ok(v.as_array().into_iter().flatten().filter_map(merged_from).collect())
}

/// Merge when ready: asked again right before, and only GitLab's own "mergeable" goes.
pub fn merge_when_ready(cwd: &Path, repo: &HostRepo, iid: i64) -> Result<String, String> {
    let v = json(cwd, &repo.domain, &mr_view_args(repo, iid))?;
    let pull = pull_from(&v).ok_or("GitLab answered something else.")?;
    if !pull.ready {
        let why = if pull.note.is_empty() { "it is a draft or changes were requested".to_string() } else { pull.note.to_lowercase() };
        return Err(format!("!{iid} is not ready: {why}."));
    }
    json(cwd, &repo.domain, &merge_args(repo, iid))?;
    Ok(format!("Merged !{iid}."))
}

pub fn retarget(cwd: &Path, repo: &HostRepo, iid: i64, base: &str) -> Result<(), String> {
    json(cwd, &repo.domain, &retarget_args(repo, iid, base)).map(|_| ())
}

pub fn browse(domain: &str, query: &str, page: u32) -> Result<BrowsePage, String> {
    let page = page.max(1);
    let v = json(&std::env::temp_dir(), domain, &browse_args(domain, query, page))?;
    let list = v.as_array().cloned().unwrap_or_default();
    Ok(BrowsePage { more: list.len() >= 50, page, items: list.iter().filter_map(remote_from).collect() })
}

/// A linked merge request's or issue's current title and state.
pub fn item_state(cwd: &Path, url: &str) -> Result<(String, String), String> {
    let (domain, project_path, kind, number) = parse_item_url(url).ok_or("not a GitLab address")?;
    let v = json(cwd, &domain, &get(&domain, format!("projects/{}/{kind}/{number}", encoded(&project_path))))?;
    Ok((v["title"].as_str().unwrap_or_default().to_string(), state_word(&v).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::HostKind;
    use crate::github::Repo;
    use serde_json::json;

    fn repo() -> HostRepo {
        HostRepo { kind: HostKind::Gitlab, domain: "gitlab.com".into(), repo: Repo { owner: "group/sub".into(), name: "tool".into() } }
    }

    /// The shape GitLab's documentation gives for a merge request in a list.
    fn mr() -> Value {
        json!({
            "id": 900, "iid": 12, "project_id": 3, "title": " Faster rows ", "state": "opened",
            "draft": false, "work_in_progress": false,
            "author": { "id": 1, "username": "ann", "name": "Ann" },
            "source_branch": "rows", "target_branch": "main", "sha": "abc123",
            "source_project_id": 3, "target_project_id": 3,
            "labels": ["speed", "ui"],
            "detailed_merge_status": "mergeable", "has_conflicts": false,
            "updated_at": "2026-11-05T10:00:00.000Z",
            "web_url": "https://gitlab.com/group/sub/tool/-/merge_requests/12"
        })
    }

    #[test]
    fn a_merge_request_reads_like_a_pull_request() {
        let p = pull_from(&mr()).unwrap();
        assert_eq!((p.number, p.title.as_str(), p.author.as_str()), (12, "Faster rows", "ann"));
        assert_eq!((p.branch.as_str(), p.base.as_str(), p.head_oid.as_str()), ("rows", "main", "abc123"));
        assert_eq!(p.labels, vec!["speed", "ui"]);
        assert_eq!(p.state, "open");
        assert!(p.ready);
        assert_eq!(p.note, "");
        assert!(!p.foreign);
    }

    #[test]
    fn ready_is_gitlabs_own_verdict_and_says_what_is_missing() {
        let with = |status: &str| pull_from(&json!({ "iid": 1, "detailed_merge_status": status })).unwrap();
        for (status, note) in [
            ("not_approved", "Needs approval"),
            ("ci_must_pass", "Pipeline must pass"),
            ("ci_still_running", "Pipeline running"),
            ("conflict", "Has conflicts"),
            ("need_rebase", "Needs a rebase"),
            ("discussions_not_resolved", "Open discussions"),
            ("unchecked", "Being checked"),
            ("security_policy_violations", "Security policy violations"),
        ] {
            let p = with(status);
            assert!(!p.ready, "{status}");
            assert_eq!(p.note, note, "{status}");
        }
        let draft = pull_from(&json!({ "iid": 1, "draft": true, "detailed_merge_status": "mergeable" })).unwrap();
        assert!(!draft.ready);
        assert_eq!(draft.state, "draft");
        let asked = with("requested_changes");
        assert_eq!(asked.review, "CHANGES_REQUESTED");
        assert_eq!(asked.note, "", "the review badge says it already");
    }

    #[test]
    fn a_merge_request_from_a_fork_is_foreign() {
        let mut v = mr();
        v["source_project_id"] = json!(99);
        assert!(pull_from(&v).unwrap().foreign);
    }

    #[test]
    fn an_issue_and_a_merged_request_read_too() {
        let i = issue_from(&json!({
            "iid": 7, "title": "Tabs vanish", "author": { "username": "bob" }, "labels": ["bug"],
            "user_notes_count": 3, "updated_at": "2026-11-05T09:00:00Z", "web_url": "https://gitlab.com/group/sub/tool/-/issues/7"
        }))
        .unwrap();
        assert_eq!((i.number, i.comments, i.author.as_str()), (7, 3, "bob"));
        assert_eq!(i.labels, vec!["bug"]);
        assert_eq!(pull_from(&json!({ "title": "no iid" })), None);
        let m = merged_from(&mr()).unwrap();
        assert_eq!((m.number, m.branch.as_str(), m.base.as_str(), m.head.as_str()), (12, "rows", "main", "abc123"));
    }

    #[test]
    fn a_project_reads_like_a_repository() {
        let r = remote_from(&json!({
            "path_with_namespace": "group/sub/tool", "description": "A tool", "visibility": "private", "archived": false,
            "last_activity_at": "2026-11-05T08:00:00Z",
            "permissions": { "project_access": { "access_level": 30 }, "group_access": null },
            "forked_from_project": { "id": 1 }
        }))
        .unwrap();
        assert_eq!((r.slug.as_str(), r.owner.as_str(), r.name.as_str()), ("group/sub/tool", "group/sub", "tool"));
        assert!(r.private && r.fork && !r.archived);
        assert_eq!(r.access, "write");
        let public = remote_from(&json!({ "path_with_namespace": "a/b", "visibility": "public", "permissions": { "group_access": { "access_level": 50 } } })).unwrap();
        assert!(!public.private && !public.fork);
        assert_eq!(public.access, "admin");
    }

    #[test]
    fn calls_name_the_project_encoded_and_the_right_gitlab() {
        let a = mr_list_args(&repo(), "two words&x");
        assert_eq!(&a[..3], ["api", "--hostname", "gitlab.com"]);
        assert_eq!(a[3], "projects/group%2Fsub%2Ftool/merge_requests?state=opened&per_page=30&order_by=updated_at&search=two%20words%26x");
        assert!(issue_list_args(&repo(), "").last().unwrap().ends_with("/issues?state=opened&per_page=30&order_by=updated_at"));
        let own = HostRepo { domain: "gitlab.example.com".into(), ..repo() };
        assert_eq!(merge_args(&own, 12), ["api", "--hostname", "gitlab.example.com", "-X", "PUT", "projects/group%2Fsub%2Ftool/merge_requests/12/merge"]);
        assert_eq!(retarget_args(&repo(), 12, "main")[3..], ["-X", "PUT", "projects/group%2Fsub%2Ftool/merge_requests/12", "-f", "target_branch=main"]);
    }

    #[test]
    fn a_missing_or_signed_out_cli_says_what_to_do() {
        assert!(sign_in_hint("gitlab.com", "glab auth: program not found").starts_with("Install the GitLab CLI (glab)"));
        assert_eq!(sign_in_hint("gitlab.com", "not logged in"), "Sign in to GitLab with `glab auth login`.");
        assert_eq!(sign_in_hint("gitlab.example.com", "401"), "Sign in to gitlab.example.com with `glab auth login --hostname gitlab.example.com`.");
        assert_eq!(error_text("HTTP 401 Unauthorized", "gitlab.com"), "Sign in to GitLab with `glab auth login`.");
        let leaked = error_text("bad glpat-abcdefghij1234567890 here", "gitlab.com");
        assert!(!leaked.contains("glpat-"), "{leaked}");
    }

    #[test]
    fn an_items_address_is_understood() {
        assert_eq!(parse_item_url("https://gitlab.com/group/sub/tool/-/merge_requests/12"), Some(("gitlab.com".into(), "group/sub/tool".into(), "merge_requests", 12)));
        assert_eq!(parse_item_url("https://gitlab.example.com/a/b/-/issues/7#note_1"), Some(("gitlab.example.com".into(), "a/b".into(), "issues", 7)));
        assert_eq!(parse_item_url("https://github.com/a/b/pull/7"), None);
        assert_eq!(parse_item_url("https://gitlab.com/a/b/-/pipelines/7"), None);
        assert_eq!(state_word(&json!({ "state": "opened", "draft": true })), "draft");
        assert_eq!(state_word(&json!({ "state": "merged" })), "merged");
        assert_eq!(state_word(&json!({ "state": "closed" })), "closed");
        assert_eq!(state_word(&json!({ "state": "opened" })), "open");
    }
}
