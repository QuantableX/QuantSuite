//! Review guides (round 3, card 04; T3 Code's "Write guide"): a walkthrough
//! of a change written by one read-only agent run — a summary, the files in
//! the order to read them grouped by concern, per file what changed, how
//! risky it is and what to check, and questions for the author.
//!
//! The diff goes into the prompt (no tools, no session, no repository
//! access); each file's patch is cut at a size and the whole prompt has a
//! budget — a file cut or left out is still listed by name and marked, so
//! the guide's file list is always the diff's. A file the agent forgot is
//! added under "Not covered by the guide", never dropped. Guides are kept
//! per target (a pull request, a thread's diff scope) with a key over the
//! diff; when the diff changes, the guide is stale until written again.
//! Nothing is posted anywhere.

use crate::settings::Settings;
use crate::{adapters, lock, now, providers, review, threads, PilotState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, State};

/// A file's patch is cut after this many characters.
const PER_FILE: usize = 12_000;
/// The diffs in one prompt together.
const TOTAL: usize = 160_000;
const TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GuideTarget {
    /// A thread's Diff tab: `turn` (with `turn`), `uncommitted` or `branch`.
    Thread { id: String, scope: String, turn: Option<String> },
    /// A pull request in the GitHub view.
    Pr { context_id: String, number: i64 },
}

impl GuideTarget {
    fn storage_key(&self) -> String {
        match self {
            GuideTarget::Thread { id, scope, turn } => format!("thread:{id}:{scope}:{}", turn.as_deref().unwrap_or_default()),
            GuideTarget::Pr { context_id, number } => format!("pr:{context_id}#{number}"),
        }
    }
}

/// One changed file as the prompt and the guide see it.
#[derive(Debug, Clone, Default)]
pub struct DiffFile {
    pub path: String,
    pub status: String,
    pub added: i64,
    pub removed: i64,
    pub patch: String,
}

/// What a guide is written about.
#[derive(Debug, Clone, Default)]
pub struct Material {
    /// "Pull request #12: Add schedules" / "Uncommitted changes in hello world qa".
    pub label: String,
    /// A PR's description.
    pub description: String,
    /// The PR head or the compared range — part of the key.
    pub identity: String,
    pub files: Vec<DiffFile>,
}

impl Material {
    /// The diff's key: a changed file, patch or head makes the guide stale.
    pub fn key(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.identity.as_bytes());
        for f in &self.files {
            h.update([0]);
            h.update(f.path.as_bytes());
            h.update([0]);
            h.update(f.patch.as_bytes());
        }
        h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GuideFile {
    pub path: String,
    pub what: String,
    /// `low` | `medium` | `high`; "" when the agent did not cover the file.
    pub risk: String,
    pub check: Vec<String>,
    /// A new-side line to look at first.
    pub line: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GuideGroup {
    pub title: String,
    pub files: Vec<GuideFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Guide {
    pub summary: String,
    /// In reading order.
    pub groups: Vec<GuideGroup>,
    pub questions: Vec<String>,
    /// Diff files the agent left out — listed under "Not covered by the guide".
    pub missing: Vec<String>,
    /// Files whose patch was cut or left out of the prompt.
    pub cut: Vec<String>,
    pub key: String,
    pub provider: String,
    pub model: String,
    pub created_at: i64,
}

// ── The prompt ──

/// The prompt and the files whose diff it cut or left out.
pub fn build_prompt(m: &Material) -> (String, Vec<String>) {
    let mut cut = Vec::new();
    let mut out = String::from(
        "You are writing a review guide for a code change, so that a human reviews it well. Read the change below and answer with JSON only — \
         nothing before or after it — in exactly this shape:\n\
         {\"summary\": \"<2-4 sentences: what the change does and why it matters>\",\n \
         \"groups\": [{\"title\": \"<one concern, e.g. 'Schedule engine'>\", \"files\": [{\"path\": \"<a path from the file list, exactly as written>\", \
         \"what\": \"<what changed in this file, 1-2 sentences>\", \"risk\": \"low|medium|high\", \"check\": [\"<what the reviewer should verify>\"], \
         \"line\": <the new-side line number to look at first, or null>}]}],\n \
         \"questions\": [\"<questions for the author>\"]}\n\
         Order the groups the way a reviewer should read them: foundations first, then what builds on them, tests and docs last. \
         Every file in the file list belongs in exactly one group. Risk: high = can break behaviour, data or security; \
         medium = logic that needs a careful look; low = mechanical changes, docs, tests. \
         Files marked as cut or left out: judge them from their name and the rest of the change, and say so in \"what\".\n\n",
    );
    out.push_str(&format!("Change: {}\n", m.label));
    if !m.description.trim().is_empty() {
        let d: String = m.description.trim().chars().take(4000).collect();
        out.push_str(&format!("Description:\n{d}\n"));
    }
    out.push_str(&format!("\nFiles ({}):\n", m.files.len()));
    for f in &m.files {
        out.push_str(&format!("{} {} (+{} -{})\n", f.status, f.path, f.added, f.removed));
    }
    out.push_str("\nDiffs:\n");
    let mut used = 0usize;
    for f in &m.files {
        if f.patch.trim().is_empty() {
            out.push_str(&format!("=== {} ===\n(no text diff: binary, too large or only renamed)\n", f.path));
            continue;
        }
        if used >= TOTAL {
            out.push_str(&format!("=== {} ===\n(left out: the prompt's budget is used up)\n", f.path));
            cut.push(f.path.clone());
            continue;
        }
        let room = PER_FILE.min(TOTAL - used);
        let n = f.patch.chars().count();
        if n > room {
            let part: String = f.patch.chars().take(room).collect();
            out.push_str(&format!("=== {} ===\n{part}\n(cut: {} of {n} characters shown)\n", f.path, room));
            cut.push(f.path.clone());
            used += room;
        } else {
            out.push_str(&format!("=== {} ===\n{}\n", f.path, f.patch));
            used += n;
        }
    }
    (out, cut)
}

// ── The answer ──

fn norm(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let p = p.trim_start_matches("./");
    p.strip_prefix("a/").or_else(|| p.strip_prefix("b/")).unwrap_or(p).to_string()
}

/// The diff path a guide's path means: exact, else the one diff path it ends.
fn match_path(raw: &str, paths: &[String]) -> Option<String> {
    let p = norm(raw);
    if p.is_empty() {
        return None;
    }
    if let Some(hit) = paths.iter().find(|x| **x == p) {
        return Some(hit.clone());
    }
    let tail: Vec<&String> = paths.iter().filter(|x| x.ends_with(&format!("/{p}")) || p.ends_with(&format!("/{}", x))).collect();
    (tail.len() == 1).then(|| tail[0].clone())
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

/// The agent's answer as a guide over `paths` (the diff's files, in order).
pub fn parse_guide(text: &str, paths: &[String]) -> Result<Guide, String> {
    let body: String = text.lines().filter(|l| !l.trim_start().starts_with("```")).collect::<Vec<_>>().join("\n");
    let json = body
        .find('{')
        .zip(body.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Value>(&body[a..=b]).ok())
        .ok_or("The agent's answer was not a guide (no JSON in it).")?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut groups = Vec::new();
    for g in json["groups"].as_array().into_iter().flatten() {
        let mut files = Vec::new();
        for f in g["files"].as_array().into_iter().flatten() {
            let Some(path) = f["path"].as_str().and_then(|p| match_path(p, paths)) else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let risk = f["risk"].as_str().unwrap_or_default().trim().to_lowercase();
            files.push(GuideFile {
                path,
                what: f["what"].as_str().unwrap_or_default().trim().to_string(),
                risk: if matches!(risk.as_str(), "low" | "medium" | "high") { risk } else { "medium".into() },
                check: strings(&f["check"]),
                line: f["line"].as_i64().filter(|l| *l > 0),
            });
        }
        if !files.is_empty() {
            let title = g["title"].as_str().unwrap_or_default().trim();
            groups.push(GuideGroup { title: if title.is_empty() { "Changes".into() } else { title.to_string() }, files });
        }
    }
    let summary = json["summary"].as_str().unwrap_or_default().trim().to_string();
    if groups.is_empty() && summary.is_empty() {
        return Err("The agent's answer was not a guide (no summary, no files).".into());
    }
    let missing: Vec<String> = paths.iter().filter(|p| !seen.contains(*p)).cloned().collect();
    if !missing.is_empty() {
        groups.push(GuideGroup {
            title: "Not covered by the guide".into(),
            files: missing.iter().map(|p| GuideFile { path: p.clone(), ..Default::default() }).collect(),
        });
    }
    Ok(Guide { summary, groups, questions: strings(&json["questions"]), missing, ..Default::default() })
}

// ── Material ──

fn material(app: &AppHandle, state: &PilotState, target: &GuideTarget) -> Result<Material, String> {
    match target {
        GuideTarget::Thread { id, scope, turn } => {
            let list = threads::diff_list(state, id, scope, turn.clone())?;
            let paths: Vec<String> = list.files.iter().filter(|f| !f.binary).map(|f| f.path.clone()).collect();
            let patches = threads::diff_patches(state, id, &list.from, list.to.clone(), paths)?;
            let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
            let what = match scope.as_str() {
                "uncommitted" => "Uncommitted changes",
                "branch" => "The branch's changes against its base",
                _ => "One turn's changes",
            };
            Ok(Material {
                label: format!("{what} in {}{}", meta.context_name, if meta.title.trim().is_empty() { String::new() } else { format!(" (thread \u{201c}{}\u{201d})", meta.title.trim()) }),
                description: String::new(),
                identity: format!("{scope}:{}..{}", list.from, list.to),
                files: list
                    .files
                    .iter()
                    .map(|f| DiffFile {
                        path: f.path.clone(),
                        status: f.status.clone(),
                        added: f.added,
                        removed: f.removed,
                        patch: patches.iter().find(|p| p.path == f.path).map(|p| p.patch.clone()).unwrap_or_default(),
                    })
                    .collect(),
            })
        }
        GuideTarget::Pr { context_id, number } => {
            let pr = review::load_pr(app, context_id, *number)?;
            Ok(Material {
                label: format!("Pull request #{} \u{201c}{}\u{201d} ({} \u{2192} {})", pr.number, pr.title, pr.head, pr.base),
                description: pr.body.clone(),
                identity: format!("pr:{}", pr.head_sha),
                files: pr
                    .files
                    .iter()
                    .map(|f| DiffFile {
                        path: f.path.clone(),
                        status: match f.status.as_str() {
                            "added" => "A",
                            "removed" => "D",
                            "renamed" => "R",
                            _ => "M",
                        }
                        .to_string(),
                        added: f.additions,
                        removed: f.deletions,
                        patch: f.patch.clone().unwrap_or_default(),
                    })
                    .collect(),
            })
        }
    }
}

// ── The agent run ──

/// One read-only run of Claude Code or Codex with the prompt on stdin; its answer.
fn ask(settings: &Settings, provider: &str, model: &str, instance: &str, prompt: &str) -> Result<String, String> {
    if provider != "claude" && provider != "codex" {
        return Err("Guides are written by Claude Code or Codex.".into());
    }
    let adapter = adapters::find(settings, provider).ok_or("unknown agent")?;
    let exe = providers::resolve(&adapter, settings).ok_or_else(|| format!("{} was not found", adapter.label))?;
    let tmp = std::env::temp_dir();
    let out_file = tmp.join(format!("qp-guide-{}.txt", uuid::Uuid::new_v4()));
    let model = model.trim();
    let mut args: Vec<String> = Vec::new();
    if provider == "codex" {
        args.extend(["exec", "--ephemeral", "--skip-git-repo-check", "--ignore-rules", "-s", "read-only"].map(String::from));
        if !model.is_empty() && model != "default" {
            args.extend(["-m".to_string(), model.to_string()]);
        }
        args.extend(["-C".to_string(), tmp.to_string_lossy().into_owned(), "-o".into(), out_file.to_string_lossy().into_owned(), "-".into()]);
    } else {
        args.push("-p".into());
        if !model.is_empty() && model != "default" {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        args.extend(["--tools", "", "--output-format", "text", "--no-session-persistence", "--strict-mcp-config", "--settings", r#"{"disableAllHooks":true}"#].map(String::from));
    }
    let mut cmd = providers::base_cmd(Path::new(&exe.program));
    cmd.args(&exe.prefix).args(&args).current_dir(&tmp);
    let mut cmd = providers::with_env(cmd, &settings.launch_env_for(provider, instance));
    cmd.env("PATH", qs_core::system_path::current());
    let r = providers::run_with_input(cmd, Some(prompt.to_string()), TIMEOUT);
    let answer = if provider == "codex" { std::fs::read_to_string(&out_file).unwrap_or_default() } else { String::new() };
    let _ = std::fs::remove_file(&out_file);
    let (ok, out, err) = r?;
    let text = if provider == "codex" { answer } else { out };
    if !ok || text.trim().is_empty() {
        let why = err.lines().chain(text.lines()).map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("no answer").to_string();
        return Err(format!("{} wrote no guide: {why}", adapter.label));
    }
    Ok(text)
}

// ── Commands ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideState {
    pub guide: Option<Guide>,
    /// The diff changed since the guide was written.
    pub stale: bool,
    pub files: usize,
}

/// The saved guide of a target and whether the diff moved on since.
#[tauri::command(async)]
pub fn pilot_guide_get(app: AppHandle, state: State<'_, PilotState>, target: GuideTarget) -> Result<GuideState, String> {
    let m = material(&app, &state, &target)?;
    let saved = state.db.review_guide(&target.storage_key()).map_err(|e| e.to_string())?;
    let guide = saved.as_ref().and_then(|(_, g)| serde_json::from_str::<Guide>(g).ok());
    let stale = saved.is_some_and(|(k, _)| k != m.key());
    Ok(GuideState { guide, stale, files: m.files.len() })
}

/// Writes (or rewrites) the guide: one read-only agent run over the diff.
#[tauri::command(async)]
pub fn pilot_guide_write(
    app: AppHandle,
    state: State<'_, PilotState>,
    target: GuideTarget,
    provider: String,
    model: Option<String>,
    instance: Option<String>,
) -> Result<Guide, String> {
    let m = material(&app, &state, &target)?;
    if m.files.is_empty() {
        return Err("There are no changes to guide through.".into());
    }
    let (prompt, cut) = build_prompt(&m);
    let settings = lock(&state.settings).clone();
    let model = model.unwrap_or_default();
    let answer = ask(&settings, provider.trim(), &model, instance.as_deref().unwrap_or_default(), &prompt)?;
    let paths: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
    let mut guide = parse_guide(&answer, &paths)?;
    guide.cut = cut;
    guide.key = m.key();
    guide.provider = provider.trim().to_string();
    guide.model = model;
    guide.created_at = now();
    let json = serde_json::to_string(&guide).map_err(|e| e.to_string())?;
    state.db.save_review_guide(&target.storage_key(), &guide.key, &json, guide.created_at).map_err(|e| e.to_string())?;
    Ok(guide)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, patch: &str) -> DiffFile {
        DiffFile { path: path.into(), status: "M".into(), added: 1, removed: 0, patch: patch.into() }
    }
    fn paths() -> Vec<String> {
        vec!["src/engine.rs".into(), "src/db.rs".into(), "app/View.vue".into(), "docs/PLAN.md".into()]
    }

    #[test]
    fn a_guide_is_read_from_the_answer_and_keeps_every_file() {
        let answer = r#"Here is the guide:
```json
{"summary": "Adds schedules.", "groups": [
  {"title": "Storage", "files": [{"path": "./src/db.rs", "what": "Two tables.", "risk": "LOW", "check": ["the unique index"], "line": 12}]},
  {"title": "Engine", "files": [{"path": "engine.rs", "what": "The loop.", "risk": "severe", "check": "DST"},
                               {"path": "src/db.rs", "what": "again"}, {"path": "src/elsewhere.rs", "what": "not in the diff"}]},
  {"title": "", "files": [{"path": "b/app/View.vue", "what": "The list.", "risk": "medium", "line": -3}]}
], "questions": ["Why a minute?", ""]}
```"#;
        let g = parse_guide(answer, &paths()).unwrap();
        assert_eq!(g.summary, "Adds schedules.");
        let titles: Vec<&str> = g.groups.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, ["Storage", "Engine", "Changes", "Not covered by the guide"]);
        assert_eq!(g.groups[0].files[0], GuideFile { path: "src/db.rs".into(), what: "Two tables.".into(), risk: "low".into(), check: vec!["the unique index".into()], line: Some(12) });
        assert_eq!(g.groups[1].files.len(), 1, "a repeated file and one outside the diff are not listed twice or invented");
        assert_eq!(g.groups[1].files[0].path, "src/engine.rs", "a bare name means the one diff path it ends");
        assert_eq!(g.groups[1].files[0].risk, "medium", "an unknown risk reads as medium");
        assert_eq!(g.groups[1].files[0].check, vec!["DST".to_string()]);
        assert_eq!(g.groups[2].files[0].line, None);
        assert_eq!(g.missing, vec!["docs/PLAN.md".to_string()], "a file the agent forgot is listed, never dropped");
        assert_eq!(g.groups[3].files[0].risk, "");
        assert_eq!(g.questions, vec!["Why a minute?".to_string()]);
    }

    #[test]
    fn an_answer_without_a_guide_is_an_error() {
        assert!(parse_guide("I cannot review this.", &paths()).is_err());
        assert!(parse_guide("{\"groups\": []}", &paths()).is_err());
    }

    #[test]
    fn the_prompt_cuts_big_patches_and_lists_every_file() {
        let big = "+x\n".repeat(PER_FILE);
        let m = Material {
            label: "Uncommitted changes in qa".into(),
            identity: "uncommitted:HEAD..working tree".into(),
            files: vec![file("a.rs", "+one\n"), file("big.rs", &big), file("bin.png", "")],
            ..Default::default()
        };
        let (prompt, cut) = build_prompt(&m);
        assert!(prompt.contains("Files (3):\nM a.rs (+1 -0)\nM big.rs"));
        assert!(prompt.contains("=== a.rs ===\n+one\n"));
        assert!(prompt.contains(&format!("(cut: {PER_FILE} of {} characters shown)", big.len())));
        assert!(prompt.contains("=== bin.png ===\n(no text diff"));
        assert_eq!(cut, vec!["big.rs".to_string()]);
        // Past the budget a file is named and marked, not dropped.
        let many: Vec<DiffFile> = (0..20).map(|i| file(&format!("f{i}.rs"), &big)).collect();
        let (prompt, cut) = build_prompt(&Material { files: many, ..Default::default() });
        assert!(prompt.contains("=== f19.rs ===\n(left out: the prompt's budget is used up)"));
        assert_eq!(cut.len(), 20);
        assert!(prompt.chars().count() < TOTAL + 20_000);
    }

    #[test]
    fn the_key_follows_the_diff() {
        let m = Material { identity: "pr:abc".into(), files: vec![file("a.rs", "+1")], ..Default::default() };
        let same = m.clone();
        let mut edited = m.clone();
        edited.files[0].patch = "+2".into();
        let mut pushed = m.clone();
        pushed.identity = "pr:def".into();
        assert_eq!(m.key(), same.key());
        assert_ne!(m.key(), edited.key());
        assert_ne!(m.key(), pushed.key());
        assert_eq!(GuideTarget::Pr { context_id: "core:workspace:x".into(), number: 7 }.storage_key(), "pr:core:workspace:x#7");
    }

    #[test]
    fn a_target_reads_from_the_frontend() {
        let t: GuideTarget = serde_json::from_str(r#"{"kind":"thread","id":"t1","scope":"uncommitted","turn":null}"#).unwrap();
        assert_eq!(t.storage_key(), "thread:t1:uncommitted:");
        let p: GuideTarget = serde_json::from_str(r#"{"kind":"pr","contextId":"core:workspace:x","number":3}"#).unwrap();
        assert_eq!(p.storage_key(), "pr:core:workspace:x#3");
    }
}
