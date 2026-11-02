//! A human pull-request review (parity 11): the PR as GitHub has it at its
//! current head — description, commits, files with their patches, existing
//! review threads, checks and what the viewer may do — draft comments saved
//! locally against that head, and one explicit submission.
//!
//! Provider boundary: everything host-specific sits behind [`Host`]; GitHub
//! (through the signed-in `gh`) is the only implementation. GitLab, Forgejo
//! and others are not implemented and are not claimed.
//!
//! Drafts never leave the machine until Submit. Each remembers the head it
//! was written against and the text of its line; when the head moves, a draft
//! whose line is found exactly once again moves with it (and says so), any
//! other becomes stale and is never posted at a guessed line. A submission is
//! one API call that creates the review with all its comments; a hidden
//! marker in the body lets a retry after a network failure find the review it
//! already created instead of posting it twice. Submitting never merges.

use crate::github::{self, Repo};
use crate::gitops;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Where a review is hosted. Only GitHub is implemented.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Host {
    GitHub,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Draft {
    pub id: String,
    /// Empty for the review summary.
    pub path: String,
    /// `RIGHT` (new side) or `LEFT` (old side).
    pub side: String,
    pub line: Option<i64>,
    pub start_line: Option<i64>,
    pub body: String,
    /// The head the draft was written against.
    pub commit: String,
    /// The anchored line's text then, to find it again after a push.
    pub line_text: String,
    pub created_at: i64,
    /// Set when loading: `ok` | `moved` | `stale`.
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub previous: Option<String>,
    /// `added` | `modified` | `removed` | `renamed` | `copied` | `changed`
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
    /// Absent for binary or very large files (GitHub sends none).
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub mine: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Discussion {
    pub id: String,
    pub path: String,
    pub line: Option<i64>,
    pub side: String,
    pub resolved: bool,
    pub outdated: bool,
    pub can_resolve: bool,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub author: String,
    pub state: String,
    pub draft: bool,
    pub base: String,
    pub head: String,
    pub head_sha: String,
    pub commits: Vec<(String, String)>,
    pub files: Vec<FileDiff>,
    pub truncated_files: bool,
    pub discussions: Vec<Discussion>,
    pub checks: github::Checks,
    pub viewer: String,
    /// `comment`, plus `approve` and `request_changes` unless the viewer wrote the PR.
    pub actions: Vec<String>,
    pub drafts: Vec<Draft>,
}

fn gh(args: &[&str], secs: u64) -> Result<String, String> {
    gitops::run("gh", &std::env::temp_dir(), args, Duration::from_secs(secs)).map_err(|e| github::gh_error(&e))
}

/// The text of `line` on `side` in a unified patch, if the patch shows it.
pub fn line_text(patch: &str, side: &str, line: i64) -> Option<String> {
    let (mut old, mut new) = (0i64, 0i64);
    for raw in patch.lines() {
        if let Some(h) = raw.strip_prefix("@@ -") {
            let mut parts = h.split(' ');
            old = parts.next()?.split(',').next()?.parse().ok()?;
            new = parts.next()?.trim_start_matches('+').split(',').next()?.parse().ok()?;
            continue;
        }
        let (tag, text) = raw.split_at(raw.len().min(1));
        match tag {
            "+" => {
                if side == "RIGHT" && new == line {
                    return Some(text.to_string());
                }
                new += 1;
            }
            "-" => {
                if side == "LEFT" && old == line {
                    return Some(text.to_string());
                }
                old += 1;
            }
            " " | "" => {
                if (side == "RIGHT" && new == line) || (side == "LEFT" && old == line) {
                    return Some(text.to_string());
                }
                old += 1;
                new += 1;
            }
            _ => {}
        }
    }
    None
}

/// Every line on `side` of a patch with this exact text.
fn find_lines(patch: &str, side: &str, text: &str) -> Vec<i64> {
    let mut hits = Vec::new();
    let max = patch.lines().count() as i64 * 2 + 10_000;
    // Walk the shown line numbers of the side and compare.
    let mut seen = std::collections::HashSet::new();
    for raw in patch.lines() {
        if let Some(h) = raw.strip_prefix("@@ -") {
            let mut parts = h.split(' ');
            let o: i64 = parts.next().and_then(|p| p.split(',').next()?.parse().ok()).unwrap_or(0);
            let n: i64 = parts.next().and_then(|p| p.trim_start_matches('+').split(',').next()?.parse().ok()).unwrap_or(0);
            let start = if side == "RIGHT" { n } else { o };
            for l in start..(start + 2000).min(max) {
                if !seen.insert(l) {
                    continue;
                }
                match line_text(patch, side, l) {
                    Some(t) if t == text => hits.push(l),
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
    hits
}

/// A draft against the current head: unchanged, moved to the one line with
/// its text, or stale.
pub fn reanchor(d: &mut Draft, head: &str, files: &[FileDiff]) {
    if d.path.is_empty() {
        d.anchor = "ok".into();
        d.commit = head.to_string();
        return;
    }
    if d.commit == head {
        d.anchor = "ok".into();
        return;
    }
    let patch = files.iter().find(|f| f.path == d.path).and_then(|f| f.patch.clone());
    let hits = patch.as_deref().map(|p| find_lines(p, &d.side, &d.line_text)).unwrap_or_default();
    if hits.len() == 1 && !d.line_text.is_empty() {
        let delta = hits[0] - d.line.unwrap_or(hits[0]);
        d.line = Some(hits[0]);
        d.start_line = d.start_line.map(|s| s + delta);
        d.commit = head.to_string();
        d.anchor = "moved".into();
    } else {
        d.anchor = "stale".into();
    }
}

/// The review as GitHub's API takes it, with the retry marker in the body.
pub fn payload(head: &str, event: &str, summary: &str, drafts: &[Draft], marker: &str) -> Result<Value, String> {
    let event = match event {
        "approve" => "APPROVE",
        "request_changes" => "REQUEST_CHANGES",
        "comment" => "COMMENT",
        other => return Err(format!("unknown review action '{other}'")),
    };
    if let Some(s) = drafts.iter().find(|d| !d.path.is_empty() && d.anchor == "stale") {
        return Err(format!("A comment on {} points at a line that changed — place it again or delete it before submitting.", s.path));
    }
    let comments: Vec<Value> = drafts
        .iter()
        .filter(|d| !d.path.is_empty() && !d.body.trim().is_empty())
        .map(|d| {
            let mut c = json!({ "path": d.path, "body": d.body, "side": d.side, "line": d.line });
            if let Some(s) = d.start_line.filter(|s| Some(*s) != d.line) {
                c["start_line"] = json!(s);
                c["start_side"] = json!(d.side);
            }
            c
        })
        .collect();
    if event != "APPROVE" && summary.trim().is_empty() && comments.is_empty() {
        return Err("Write a summary or at least one comment.".into());
    }
    if event == "REQUEST_CHANGES" && summary.trim().is_empty() {
        return Err("Say what needs to change in the summary.".into());
    }
    let body = format!("{}\n\n<!-- quantpilot-review:{marker} -->", summary.trim());
    Ok(json!({ "commit_id": head, "event": event, "body": body, "comments": comments }))
}

/// Load the PR, its files and discussions; drafts come back re-anchored.
pub fn load(repo: &Repo, number: i64, mut drafts: Vec<Draft>) -> Result<Loaded, String> {
    github::ready()?;
    let slug = repo.slug();
    let n = number.to_string();
    let pr: Value = serde_json::from_str(&gh(
        &["pr", "view", &n, "-R", &slug, "--json", "number,title,body,url,author,state,isDraft,baseRefName,headRefName,headRefOid,commits,statusCheckRollup"],
        30,
    )?)
    .map_err(|_| "GitHub answered with something unexpected.".to_string())?;
    let head = pr["headRefOid"].as_str().unwrap_or_default().to_string();
    let files_raw = gh(&["api", &format!("repos/{slug}/pulls/{number}/files?per_page=100"), "--paginate"], 60)?;
    // --paginate concatenates JSON arrays.
    let files: Vec<FileDiff> = files_raw
        .replace("][", ",")
        .parse::<Value>()
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|f| FileDiff {
            path: f["filename"].as_str().unwrap_or_default().to_string(),
            previous: f["previous_filename"].as_str().map(str::to_string),
            status: f["status"].as_str().unwrap_or("modified").to_string(),
            additions: f["additions"].as_i64().unwrap_or(0),
            deletions: f["deletions"].as_i64().unwrap_or(0),
            patch: f["patch"].as_str().map(str::to_string),
        })
        .collect();
    let viewer = gh(&["api", "user", "--jq", ".login"], 20).unwrap_or_default().trim().to_string();
    let query = format!(
        "query{{repository(owner:\"{}\",name:\"{}\"){{pullRequest(number:{number}){{reviewThreads(first:100){{nodes{{id isResolved isOutdated path line diffSide viewerCanResolve comments(first:50){{nodes{{id author{{login}} body createdAt}}}}}}}}}}}}}}",
        repo.owner, repo.name
    );
    let threads: Value = gh(&["api", "graphql", "-f", &format!("query={query}")], 30).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let discussions = threads["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| Discussion {
            id: t["id"].as_str().unwrap_or_default().to_string(),
            path: t["path"].as_str().unwrap_or_default().to_string(),
            line: t["line"].as_i64(),
            side: t["diffSide"].as_str().unwrap_or("RIGHT").to_string(),
            resolved: t["isResolved"] == true,
            outdated: t["isOutdated"] == true,
            can_resolve: t["viewerCanResolve"] == true,
            comments: t["comments"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    let author = c["author"]["login"].as_str().unwrap_or("ghost").to_string();
                    Comment {
                        id: c["id"].as_str().unwrap_or_default().to_string(),
                        mine: !viewer.is_empty() && author.eq_ignore_ascii_case(&viewer),
                        author,
                        body: c["body"].as_str().unwrap_or_default().to_string(),
                        created_at: c["createdAt"].as_str().unwrap_or_default().to_string(),
                    }
                })
                .collect(),
        })
        .collect();
    for d in &mut drafts {
        reanchor(d, &head, &files);
    }
    let author = pr["author"]["login"].as_str().unwrap_or_default().to_string();
    let mut actions = vec!["comment".to_string()];
    if !author.eq_ignore_ascii_case(&viewer) && pr["state"] == "OPEN" {
        actions.push("approve".into());
        actions.push("request_changes".into());
    }
    Ok(Loaded {
        number,
        title: pr["title"].as_str().unwrap_or_default().to_string(),
        body: pr["body"].as_str().unwrap_or_default().to_string(),
        url: pr["url"].as_str().unwrap_or_default().to_string(),
        state: pr["state"].as_str().unwrap_or_default().to_string(),
        draft: pr["isDraft"] == true,
        base: pr["baseRefName"].as_str().unwrap_or_default().to_string(),
        head: pr["headRefName"].as_str().unwrap_or_default().to_string(),
        commits: pr["commits"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| (c["oid"].as_str().unwrap_or_default().chars().take(7).collect(), c["messageHeadline"].as_str().unwrap_or_default().to_string()))
            .collect(),
        truncated_files: files.len() >= 3000,
        files,
        discussions,
        checks: github::checks_of(&pr["statusCheckRollup"]),
        viewer,
        actions,
        head_sha: head,
        author,
        drafts,
    })
}

/// Post the review once. A retry finds an earlier success by its marker.
pub fn submit(repo: &Repo, number: i64, review: &Value, marker: &str) -> Result<String, String> {
    github::ready()?;
    let slug = repo.slug();
    let find = || -> Option<String> {
        let list = gh(&["api", &format!("repos/{slug}/pulls/{number}/reviews?per_page=100"), "--paginate"], 30).ok()?;
        let v: Value = list.replace("][", ",").parse().ok()?;
        v.as_array()?.iter().find(|r| r["body"].as_str().is_some_and(|b| b.contains(marker))).and_then(|r| r["html_url"].as_str().map(str::to_string))
    };
    if let Some(url) = find() {
        return Ok(url);
    }
    let file = std::env::temp_dir().join(format!("qp-review-{}.json", uuid::Uuid::new_v4().simple()));
    std::fs::write(&file, review.to_string()).map_err(|e| e.to_string())?;
    let out = gh(&["api", "-X", "POST", &format!("repos/{slug}/pulls/{number}/reviews"), "--input", &file.to_string_lossy()], 60);
    let _ = std::fs::remove_file(&file);
    match out {
        Ok(text) => Ok(serde_json::from_str::<Value>(&text).ok().and_then(|v| v["html_url"].as_str().map(str::to_string)).unwrap_or_default()),
        // Did it arrive anyway? Then it is done, not to be repeated.
        Err(e) => find().ok_or(e),
    }
}

/// Resolve or reopen a discussion (only where GitHub allows the viewer).
pub fn resolve(thread_id: &str, resolved: bool) -> Result<(), String> {
    github::ready()?;
    let m = if resolved { "resolveReviewThread" } else { "unresolveReviewThread" };
    let query = format!("mutation{{{m}(input:{{threadId:\"{}\"}}){{thread{{id}}}}}}", thread_id.replace('"', ""));
    gh(&["api", "graphql", "-f", &format!("query={query}")], 30).map(|_| ())
}

pub fn host_of(cwd: &Path) -> Option<(Host, Repo)> {
    github::repo_of(cwd).map(|r| (Host::GitHub, r))
}

// ── Commands ──

use crate::{contexts, PilotState};
use tauri::{AppHandle, State};

fn repo_for(app: &AppHandle, context_id: &str) -> Result<Repo, String> {
    let ctx = contexts::resolve(app, context_id).ok_or("unknown workspace")?;
    host_of(Path::new(&ctx.path)).map(|(_, r)| r).ok_or_else(|| format!("{} has no GitHub remote.", ctx.name))
}

#[tauri::command(async)]
pub fn pilot_review_load(app: AppHandle, state: State<'_, PilotState>, context_id: String, number: i64) -> Result<Loaded, String> {
    let repo = repo_for(&app, &context_id)?;
    let drafts = state.db.review_drafts(&repo.slug(), number).map_err(|e| e.to_string())?;
    let loaded = load(&repo, number, drafts)?;
    // A draft that moved with its line is saved at its new place.
    for d in loaded.drafts.iter().filter(|d| d.anchor == "moved") {
        let _ = state.db.save_review_draft(&repo.slug(), number, d);
    }
    Ok(loaded)
}

#[tauri::command(async)]
pub fn pilot_review_draft_save(app: AppHandle, state: State<'_, PilotState>, context_id: String, number: i64, mut draft: Draft) -> Result<Draft, String> {
    let repo = repo_for(&app, &context_id)?;
    if draft.body.len() > 64 * 1024 {
        return Err("A comment is at most 64 KB.".into());
    }
    if draft.id.is_empty() {
        draft.id = uuid::Uuid::new_v4().simple().to_string();
        draft.created_at = chrono::Utc::now().timestamp_millis();
    }
    if draft.side.is_empty() {
        draft.side = "RIGHT".into();
    }
    draft.anchor = "ok".into();
    state.db.save_review_draft(&repo.slug(), number, &draft).map_err(|e| e.to_string())?;
    Ok(draft)
}

#[tauri::command(async)]
pub fn pilot_review_draft_delete(app: AppHandle, state: State<'_, PilotState>, context_id: String, number: i64, id: String) -> Result<(), String> {
    let repo = repo_for(&app, &context_id)?;
    state.db.delete_review_drafts(&repo.slug(), number, Some(&id)).map_err(|e| e.to_string())
}

/// Submit the saved drafts as one review, against the PR's head right now.
#[tauri::command(async)]
pub fn pilot_review_submit(app: AppHandle, state: State<'_, PilotState>, context_id: String, number: i64, event: String) -> Result<String, String> {
    let repo = repo_for(&app, &context_id)?;
    let drafts = state.db.review_drafts(&repo.slug(), number).map_err(|e| e.to_string())?;
    // Re-read the head: a push since the page loaded must not shift comments.
    let fresh = load(&repo, number, drafts)?;
    if !fresh.actions.contains(&event) {
        return Err("GitHub does not let you do that on this pull request (your own, or closed).".into());
    }
    let summary = fresh.drafts.iter().find(|d| d.path.is_empty()).map(|d| d.body.clone()).unwrap_or_default();
    // The same content gives the same marker, so a retry finds what it posted.
    let mut ids: Vec<String> = fresh.drafts.iter().map(|d| format!("{}:{}:{:?}:{}", d.id, d.path, d.line, d.body)).collect();
    ids.sort();
    let marker = crate::skills::sha(format!("{}#{number}@{}:{event}:{summary}:{}", repo.slug(), fresh.head_sha, ids.join("|")).as_bytes())[..16].to_string();
    let review = payload(&fresh.head_sha, &event, &summary, &fresh.drafts, &marker)?;
    let url = submit(&repo, number, &review, &marker)?;
    state.db.delete_review_drafts(&repo.slug(), number, None).map_err(|e| e.to_string())?;
    Ok(url)
}

#[tauri::command(async)]
pub fn pilot_review_resolve(thread_id: String, resolved: bool) -> Result<(), String> {
    resolve(&thread_id, resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "@@ -1,4 +1,5 @@\n line one\n-old two\n+new two\n+added three\n line four\n@@ -20,2 +21,2 @@\n ctx twenty\n-gone\n+here\n";

    #[test]
    fn line_texts_follow_both_sides() {
        assert_eq!(line_text(PATCH, "RIGHT", 2).as_deref(), Some("new two"));
        assert_eq!(line_text(PATCH, "RIGHT", 3).as_deref(), Some("added three"));
        assert_eq!(line_text(PATCH, "LEFT", 2).as_deref(), Some("old two"));
        assert_eq!(line_text(PATCH, "RIGHT", 22).as_deref(), Some("here"));
        assert_eq!(line_text(PATCH, "RIGHT", 50), None);
    }

    fn draft(line: i64, text: &str) -> Draft {
        Draft { id: "d".into(), path: "a.rs".into(), side: "RIGHT".into(), line: Some(line), body: "nit".into(), commit: "old".into(), line_text: text.into(), ..Default::default() }
    }

    #[test]
    fn drafts_move_with_their_line_or_become_stale() {
        // The head moved: two lines were inserted above.
        let moved = "@@ -1,4 +1,7 @@\n line one\n+x\n+y\n-old two\n+new two\n+added three\n line four\n";
        let files = vec![FileDiff { path: "a.rs".into(), previous: None, status: "modified".into(), additions: 4, deletions: 1, patch: Some(moved.into()) }];
        let mut d = draft(3, "added three");
        reanchor(&mut d, "new", &files);
        assert_eq!((d.anchor.as_str(), d.line, d.commit.as_str()), ("moved", Some(5), "new"));
        let mut gone = draft(2, "text that is no longer there");
        reanchor(&mut gone, "new", &files);
        assert_eq!(gone.anchor, "stale");
        let mut same = draft(2, "new two");
        same.commit = "new".into();
        reanchor(&mut same, "new", &files);
        assert_eq!(same.anchor, "ok");
    }

    #[test]
    fn payloads_refuse_stale_anchors_and_carry_the_marker() {
        let mut a = draft(2, "new two");
        a.anchor = "ok".into();
        let mut r = draft(3, "added three");
        r.anchor = "ok".into();
        r.start_line = Some(2);
        let p = payload("sha", "comment", "Looks fine", &[a.clone(), r], "m-1").unwrap();
        assert_eq!(p["event"], "COMMENT");
        assert_eq!(p["comments"].as_array().unwrap().len(), 2);
        assert_eq!(p["comments"][1]["start_line"], 2);
        assert!(p["body"].as_str().unwrap().contains("quantpilot-review:m-1"));
        let mut stale = a.clone();
        stale.anchor = "stale".into();
        assert!(payload("sha", "comment", "x", &[stale], "m").is_err());
        assert!(payload("sha", "request_changes", "", &[a], "m").is_err(), "changes need a reason");
        assert!(payload("sha", "approve", "", &[], "m").is_ok(), "a bare approval is fine");
        assert!(payload("sha", "comment", "", &[], "m").is_err());
        assert!(payload("sha", "merge", "x", &[], "m").is_err(), "no merge from a review");
    }
}
