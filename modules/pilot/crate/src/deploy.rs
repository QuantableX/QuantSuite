//! Deploy (round 4, card 4, docs/PLAN-QUANTPILOT.md §53): preview, check, then
//! production, per project. Netlify, Vercel or the user's own commands.
//!
//! A preview is never called production and production always needs a person:
//! the button's confirmation, or an agent's request through the approval gate
//! (`external`). Production also wants a good preview of the same commit, unless
//! the user says otherwise. Tokens live in the vault and reach the CLI only
//! through its environment: never on a command line, never in a log.

use crate::db::Database;
use crate::{contexts, now, providers, PilotState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

const STEP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// What a run's log keeps, from its end.
const LOG_KEPT: usize = 64 * 1024;
const RUNS_SHOWN: i64 = 30;

/// One place a project deploys to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeployTarget {
    pub id: String,
    pub name: String,
    /// `netlify` | `vercel` | `custom`
    pub kind: String,
    /// Run first, in the project's folder; empty = nothing to build.
    pub build_command: String,
    /// What goes up, relative to the project.
    pub folder: String,
    /// Netlify's site id or name; empty = what the folder is linked to.
    pub site: String,
    pub preview_command: String,
    pub production_command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeployRun {
    pub id: String,
    pub project: String,
    pub target_id: String,
    pub target_name: String,
    /// `preview` | `production`
    pub stage: String,
    pub commit_id: String,
    /// Uncommitted changes went along: this is not exactly that commit.
    pub dirty: bool,
    /// `running` | `ok` | `failed`
    pub status: String,
    pub url: String,
    pub log: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

/// One command of a deploy.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub label: String,
    pub run: Run,
    /// The environment variable the token goes in; none = the step gets no token.
    pub token_env: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    /// A command line the user wrote, through the shell.
    Shell(String),
    /// A known CLI with exact arguments.
    Cli { program: &'static str, args: Vec<String> },
}

fn check_folder(folder: &str) -> Result<(), String> {
    let path = Path::new(folder.trim());
    if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        return Err("The folder is inside the project, like dist or build.".into());
    }
    Ok(())
}

pub fn validate(target: &DeployTarget) -> Result<(), String> {
    if target.name.trim().is_empty() {
        return Err("Give it a name.".into());
    }
    if !target.site.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        return Err("The site is an id or name, without spaces.".into());
    }
    check_folder(&target.folder)?;
    match target.kind.as_str() {
        "netlify" => {
            if target.folder.trim().is_empty() {
                return Err("Netlify needs the folder to publish, like dist.".into());
            }
            Ok(())
        }
        "vercel" => Ok(()),
        "custom" => {
            if target.preview_command.trim().is_empty() || target.production_command.trim().is_empty() {
                return Err("Your own target needs a preview command and a production command.".into());
            }
            Ok(())
        }
        other => Err(format!("Unknown kind of target: {other}")),
    }
}

/// The commands of a deploy, in order. A token never appears in here.
pub fn plan(target: &DeployTarget, production: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    if !target.build_command.trim().is_empty() {
        steps.push(Step { label: "Build".into(), run: Run::Shell(target.build_command.trim().to_string()), token_env: None });
    }
    let label = if production { "Deploy to production" } else { "Deploy a preview" }.to_string();
    let folder = target.folder.trim().to_string();
    match target.kind.as_str() {
        "netlify" => {
            let mut args = vec!["deploy".to_string(), "--json".to_string(), "--dir".to_string(), folder];
            if !target.site.trim().is_empty() {
                args.push("--site".into());
                args.push(target.site.trim().to_string());
            }
            if production {
                args.push("--prod".into());
            }
            steps.push(Step { label, run: Run::Cli { program: "netlify", args }, token_env: Some("NETLIFY_AUTH_TOKEN") });
        }
        "vercel" => {
            let mut args = vec!["deploy".to_string()];
            if !folder.is_empty() {
                args.push(folder);
            }
            args.push("--yes".into());
            if production {
                args.push("--prod".into());
            }
            steps.push(Step { label, run: Run::Cli { program: "vercel", args }, token_env: Some("VERCEL_TOKEN") });
        }
        _ => {
            let command = if production { &target.production_command } else { &target.preview_command };
            steps.push(Step { label, run: Run::Shell(command.trim().to_string()), token_env: Some("DEPLOY_TOKEN") });
        }
    }
    steps
}

/// The address a deploy step printed.
pub fn url_from(kind: &str, production: bool, stdout: &str) -> String {
    if kind == "netlify" {
        if let Some(start) = stdout.find('{') {
            if let Ok(v) = serde_json::from_str::<Value>(stdout[start..].trim()) {
                let key = if production { "url" } else { "deploy_url" };
                if let Some(url) = v.get(key).and_then(Value::as_str) {
                    return url.to_string();
                }
            }
        }
    }
    stdout
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>')
        .rfind(|w| w.starts_with("https://") || w.starts_with("http://"))
        .unwrap_or_default()
        .trim_end_matches([',', '.', ')'])
        .to_string()
}

fn scrub(text: &str, token: Option<&str>) -> String {
    match token.filter(|t| !t.is_empty()) {
        Some(t) => text.replace(t, "***"),
        None => text.to_string(),
    }
}

/// Production's rule: a person said yes, and either a good preview of exactly
/// this commit exists or that person said to go without one.
pub fn check_production(runs: &[DeployRun], target_id: &str, head: Option<&Head>, confirmed: bool, without_preview: bool) -> Result<(), String> {
    if !confirmed {
        return Err("Production needs your confirmation.".into());
    }
    if without_preview {
        return Ok(());
    }
    let Some(head) = head else {
        return Err("This folder is not a git repository, so no preview can be matched to it. Deploy without a preview if you mean to.".into());
    };
    if head.dirty {
        return Err("There are uncommitted changes, so no preview can vouch for this. Commit them, or deploy without a preview if you mean to.".into());
    }
    let previewed = runs.iter().any(|r| r.target_id == target_id && r.stage == "preview" && r.status == "ok" && r.commit_id == head.commit && !r.dirty);
    if !previewed {
        return Err(format!("No good preview of commit {} yet. Deploy a preview first, or deploy without one if you mean to.", short(&head.commit)));
    }
    Ok(())
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Head {
    pub commit: String,
    pub dirty: bool,
}

fn head_of(cwd: &Path) -> Option<Head> {
    let commit = crate::gitops::run("git", cwd, &["rev-parse", "HEAD"], Duration::from_secs(20)).ok()?;
    let status = crate::gitops::run("git", cwd, &["status", "--porcelain"], Duration::from_secs(30)).ok()?;
    Some(Head { commit: commit.trim().to_string(), dirty: !status.trim().is_empty() })
}

fn shell_command(command: &str, cwd: &Path) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.raw_arg("/S /C \"").raw_arg(command).raw_arg("\"");
        c.creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.env("PATH", qs_core::system_path::current());
    cmd.current_dir(cwd);
    cmd
}

fn install_hint(program: &str) -> String {
    match program {
        "netlify" => "The Netlify CLI is not installed. Install it with: npm install -g netlify-cli".to_string(),
        "vercel" => "The Vercel CLI is not installed. Install it with: npm install -g vercel".to_string(),
        other => format!("{other} is not installed."),
    }
}

/// Runs the steps in order, stopping at the first failure. `locate` finds a
/// known CLI. Returns the address, or the failure; `log` gets what happened,
/// with the token taken out.
#[allow(clippy::too_many_arguments)]
pub fn run_steps(steps: &[Step], kind: &str, production: bool, cwd: &Path, token: Option<&str>, stage: &str, locate: &dyn Fn(&str) -> Option<std::path::PathBuf>, log: &mut String, timeout: Duration) -> Result<String, String> {
    let mut url = String::new();
    for step in steps {
        log.push_str(&format!("── {}\n", step.label));
        let mut cmd = match &step.run {
            Run::Shell(command) => shell_command(command, cwd),
            Run::Cli { program, args } => {
                let Some(exe) = locate(program) else {
                    let hint = install_hint(program);
                    log.push_str(&hint);
                    log.push('\n');
                    return Err(hint);
                };
                let mut c = providers::base_cmd(&exe);
                c.args(args).current_dir(cwd);
                c
            }
        };
        cmd.env("QS_DEPLOY_STAGE", stage).env("CI", "1").env("NO_COLOR", "1");
        if let (Some(name), Some(value)) = (step.token_env, token.filter(|t| !t.is_empty())) {
            cmd.env(name, value);
        }
        let (ok, out, err) = providers::run_capture_split(cmd, timeout)?;
        // Windows line ends read as stray characters in the page's log.
        log.push_str(&scrub(&out, token).replace("\r\n", "\n"));
        if !err.trim().is_empty() {
            log.push_str(&scrub(&err, token).replace("\r\n", "\n"));
        }
        if !log.ends_with('\n') {
            log.push('\n');
        }
        if !ok {
            return Err(format!("{} failed.", step.label));
        }
        if step.token_env.is_some() {
            url = url_from(kind, production, &out);
        }
    }
    Ok(url)
}

fn kept(log: String) -> String {
    if log.len() <= LOG_KEPT {
        return log;
    }
    let mut from = log.len() - LOG_KEPT;
    while !log.is_char_boundary(from) {
        from += 1;
    }
    format!("[the start was cut]\n{}", &log[from..])
}

// ── In the app ──────────────────────────────────────────────────────────────

fn secret_id(target: &str) -> String {
    format!("pilot.deploy.{target}")
}

fn read_token(target: &str) -> Result<Option<zeroize::Zeroizing<String>>, String> {
    qs_core::vault::with_vault(|v| v.get_string(&secret_id(target)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetView {
    #[serde(flatten)]
    target: DeployTarget,
    has_token: bool,
    cli_found: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployView {
    targets: Vec<TargetView>,
    runs: Vec<DeployRun>,
    head: Option<Head>,
}

fn project_path(app: &AppHandle, project: &str) -> Result<std::path::PathBuf, String> {
    let ctx = contexts::resolve(app, project).ok_or("Unknown project.")?;
    if ctx.kind == "general" || ctx.path.is_empty() {
        return Err("Deploy targets belong to a project.".into());
    }
    Ok(std::path::PathBuf::from(ctx.path))
}

fn targets_of(state: &PilotState, project: &str) -> Vec<DeployTarget> {
    state.settings.lock().map(|s| s.deploy.get(project).cloned().unwrap_or_default()).unwrap_or_default()
}

fn view_of(app: &AppHandle, state: &PilotState, project: &str) -> Result<DeployView, String> {
    let cwd = project_path(app, project)?;
    let targets = targets_of(state, project)
        .into_iter()
        .map(|target| TargetView {
            has_token: read_token(&target.id).ok().flatten().is_some(),
            cli_found: match target.kind.as_str() {
                "netlify" | "vercel" => providers::which(&target.kind).is_some(),
                _ => true,
            },
            target,
        })
        .collect();
    Ok(DeployView { targets, runs: state.db.deploy_runs(project, RUNS_SHOWN).map_err(|e| e.to_string())?, head: head_of(&cwd) })
}

#[tauri::command(async)]
pub fn pilot_deploy_view(app: AppHandle, state: State<'_, PilotState>, project: String) -> Result<DeployView, String> {
    view_of(&app, &state, &project)
}

/// Save a target. A token, when given, goes to the vault and is never shown again.
#[tauri::command(async)]
pub fn pilot_deploy_save(app: AppHandle, state: State<'_, PilotState>, project: String, target: DeployTarget, token: Option<String>) -> Result<DeployView, String> {
    project_path(&app, &project)?;
    let mut target = target;
    target.name = target.name.trim().to_string();
    validate(&target)?;
    if target.id.is_empty() {
        target.id = uuid::Uuid::new_v4().to_string();
    }
    if let Some(token) = token.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        qs_core::vault::with_vault(|v| v.set(&secret_id(&target.id), token.as_bytes()).map(|_| ()))?;
    }
    {
        let mut s = state.settings.lock().map_err(|_| "settings lock")?;
        let list = s.deploy.entry(project.clone()).or_default();
        match list.iter_mut().find(|t| t.id == target.id) {
            Some(existing) => *existing = target,
            None => list.push(target),
        }
        crate::settings::write(&state.data_dir, &s)?;
    }
    view_of(&app, &state, &project)
}

#[tauri::command(async)]
pub fn pilot_deploy_delete(app: AppHandle, state: State<'_, PilotState>, project: String, id: String) -> Result<DeployView, String> {
    {
        let mut s = state.settings.lock().map_err(|_| "settings lock")?;
        if let Some(list) = s.deploy.get_mut(&project) {
            list.retain(|t| t.id != id);
        }
        crate::settings::write(&state.data_dir, &s)?;
    }
    let _ = qs_core::vault::with_vault(|v| v.delete(&secret_id(&id)).map(|_| ()));
    view_of(&app, &state, &project)
}

fn execute(db: &Database, mut run: DeployRun, target: &DeployTarget, cwd: &Path, token: Option<&str>) -> DeployRun {
    let production = run.stage == "production";
    let steps = plan(target, production);
    let mut log = String::new();
    let stage = run.stage.clone();
    let result = run_steps(&steps, &target.kind, production, cwd, token, &stage, &|p| providers::which(p), &mut log, STEP_TIMEOUT);
    match result {
        Ok(url) => {
            run.status = "ok".into();
            run.url = url;
        }
        Err(e) => {
            run.status = "failed".into();
            log.push_str(&format!("{e}\n"));
        }
    }
    run.log = kept(log);
    run.finished_at = Some(now());
    let _ = db.deploy_save(&run);
    run
}

/// Checks the rules and records the run before anything runs.
fn begin(app: &AppHandle, state: &PilotState, project: &str, target_id: &str, stage: &str, confirmed: bool, without_preview: bool) -> Result<(DeployRun, DeployTarget, std::path::PathBuf), String> {
    if !matches!(stage, "preview" | "production") {
        return Err("A deploy is a preview or production.".into());
    }
    let cwd = project_path(app, project)?;
    let target = targets_of(state, project).into_iter().find(|t| t.id == target_id).ok_or("Unknown deploy target.")?;
    let runs = state.db.deploy_runs(project, 200).map_err(|e| e.to_string())?;
    if runs.iter().any(|r| r.target_id == target_id && r.status == "running") {
        return Err("A deploy to this target is still running.".into());
    }
    let head = head_of(&cwd);
    if stage == "production" {
        check_production(&runs, target_id, head.as_ref(), confirmed, without_preview)?;
    }
    let run = DeployRun {
        id: uuid::Uuid::new_v4().to_string(),
        project: project.to_string(),
        target_id: target_id.to_string(),
        target_name: target.name.clone(),
        stage: stage.to_string(),
        commit_id: head.as_ref().map(|h| h.commit.clone()).unwrap_or_default(),
        dirty: head.as_ref().is_some_and(|h| h.dirty),
        status: "running".into(),
        started_at: now(),
        ..Default::default()
    };
    state.db.deploy_save(&run).map_err(|e| e.to_string())?;
    Ok((run, target, cwd))
}

/// The buttons: starts in the background; the page follows the history.
#[tauri::command(async)]
pub fn pilot_deploy_run(app: AppHandle, state: State<'_, PilotState>, project: String, target_id: String, stage: String, confirmed: bool, without_preview: bool) -> Result<DeployRun, String> {
    let (run, target, cwd) = begin(&app, &state, &project, &target_id, &stage, confirmed, without_preview)?;
    let token = read_token(&target_id)?;
    let db = state.db.clone();
    let started = run.clone();
    std::thread::spawn(move || {
        execute(&db, run, &target, &cwd, token.as_ref().map(|t| t.as_str()));
    });
    Ok(started)
}

/// What an agent may do: the approval gate stands in for the button (`external`),
/// and production still wants a good preview of this commit.
#[tauri::command]
pub async fn pilot_agent_deploy(app: AppHandle, pilot_token: Option<String>, target: String, stage: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PilotState>();
        let me = crate::agent_tools::caller(&state, pilot_token)?;
        let wanted = target.trim().to_lowercase();
        let found = targets_of(&state, &me.context_id)
            .into_iter()
            .find(|t| t.id == target.trim() || t.name.to_lowercase() == wanted)
            .ok_or_else(|| format!("No deploy target \"{target}\" in {}. The user adds them on the Deploy page.", me.context_name))?;
        let stage = stage.unwrap_or_else(|| "preview".into());
        let (run, spec, cwd) = begin(&app, &state, &me.context_id, &found.id, &stage, true, false)?;
        let token = read_token(&found.id)?;
        let done = execute(&state.db, run, &spec, &cwd, token.as_ref().map(|t| t.as_str()));
        if done.status == "ok" {
            let what = if stage == "production" { "Live in production" } else { "Preview ready, not production" };
            Ok(format!("{what}: {}", if done.url.is_empty() { "(the host printed no address)" } else { done.url.as_str() }))
        } else {
            let tail: String = done.log.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
            Err(format!("The deploy failed:\n{tail}"))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// At the app's start.
pub fn after_restart(db: &Database) {
    let _ = db.deploy_mark_interrupted(now());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn netlify() -> DeployTarget {
        DeployTarget { id: "t1".into(), name: "Site".into(), kind: "netlify".into(), folder: "dist".into(), site: "my-site".into(), build_command: "npm run build".into(), ..Default::default() }
    }

    fn cli(step: &Step) -> (&str, Vec<&str>) {
        match &step.run {
            Run::Cli { program, args } => (program, args.iter().map(String::as_str).collect()),
            Run::Shell(_) => panic!("not a CLI step"),
        }
    }

    #[test]
    fn each_target_has_its_command_lines_and_preview_is_never_production() {
        let preview = plan(&netlify(), false);
        assert_eq!(preview[0].run, Run::Shell("npm run build".into()));
        assert_eq!(preview[0].token_env, None, "the build gets no token");
        assert_eq!(cli(&preview[1]), ("netlify", vec!["deploy", "--json", "--dir", "dist", "--site", "my-site"]));
        assert_eq!(preview[1].token_env, Some("NETLIFY_AUTH_TOKEN"));
        assert_eq!(cli(&plan(&netlify(), true)[1]).1.last(), Some(&"--prod"));

        let vercel = DeployTarget { kind: "vercel".into(), folder: String::new(), build_command: String::new(), ..netlify() };
        assert_eq!(cli(&plan(&vercel, false)[0]), ("vercel", vec!["deploy", "--yes"]));
        assert_eq!(cli(&plan(&vercel, true)[0]), ("vercel", vec!["deploy", "--yes", "--prod"]));
        assert_eq!(plan(&vercel, true)[0].token_env, Some("VERCEL_TOKEN"));

        let own = DeployTarget { kind: "custom".into(), preview_command: "./up preview".into(), production_command: "./up live".into(), build_command: String::new(), ..netlify() };
        assert_eq!(plan(&own, false)[0].run, Run::Shell("./up preview".into()));
        assert_eq!(plan(&own, true)[0].run, Run::Shell("./up live".into()));
    }

    #[test]
    fn targets_are_checked() {
        assert!(validate(&netlify()).is_ok());
        assert!(validate(&DeployTarget { name: " ".into(), ..netlify() }).is_err());
        assert!(validate(&DeployTarget { folder: String::new(), ..netlify() }).is_err(), "Netlify needs a folder");
        assert!(validate(&DeployTarget { folder: "../secrets".into(), ..netlify() }).is_err());
        assert!(validate(&DeployTarget { folder: "C:/Users/me".into(), ..netlify() }).is_err());
        assert!(validate(&DeployTarget { site: "a b".into(), ..netlify() }).is_err());
        assert!(validate(&DeployTarget { kind: "ftp".into(), ..netlify() }).is_err());
        assert!(validate(&DeployTarget { kind: "custom".into(), ..netlify() }).is_err(), "needs both commands");
        assert!(validate(&DeployTarget { kind: "vercel".into(), folder: String::new(), ..netlify() }).is_ok());
    }

    fn run(stage: &str, commit: &str, status: &str, dirty: bool) -> DeployRun {
        DeployRun { target_id: "t1".into(), stage: stage.into(), commit_id: commit.into(), status: status.into(), dirty, ..Default::default() }
    }

    #[test]
    fn production_wants_a_person_and_a_good_preview_of_this_commit() {
        let head = Head { commit: "abc1234def".into(), dirty: false };
        let good = [run("preview", "abc1234def", "ok", false)];
        assert!(check_production(&good, "t1", Some(&head), true, false).is_ok());
        assert!(check_production(&good, "t1", Some(&head), false, false).unwrap_err().contains("confirmation"));
        assert!(check_production(&good, "t1", Some(&head), false, true).unwrap_err().contains("confirmation"), "going without a preview still needs a person");

        for (name, runs) in [
            ("no preview", vec![]),
            ("another commit", vec![run("preview", "old", "ok", false)]),
            ("a failed preview", vec![run("preview", "abc1234def", "failed", false)]),
            ("still running", vec![run("preview", "abc1234def", "running", false)]),
            ("with uncommitted changes", vec![run("preview", "abc1234def", "ok", true)]),
            ("a production run", vec![run("production", "abc1234def", "ok", false)]),
        ] {
            let e = check_production(&runs, "t1", Some(&head), true, false).unwrap_err();
            assert!(e.contains("abc1234"), "{name}: {e}");
            assert!(check_production(&runs, "t1", Some(&head), true, true).is_ok(), "{name}: the user may say so");
        }
        assert!(check_production(&good, "t2", Some(&head), true, false).is_err(), "another target's preview");
        let dirty = Head { dirty: true, ..head.clone() };
        assert!(check_production(&good, "t1", Some(&dirty), true, false).unwrap_err().contains("uncommitted"));
        assert!(check_production(&good, "t1", None, true, false).unwrap_err().contains("not a git repository"));
    }

    #[test]
    fn addresses_are_read_from_what_the_host_prints() {
        let json = r#"{"site_id":"1","deploy_url":"https://abc--site.netlify.app","url":"https://site.netlify.app"}"#;
        assert_eq!(url_from("netlify", false, json), "https://abc--site.netlify.app");
        assert_eq!(url_from("netlify", true, json), "https://site.netlify.app");
        assert_eq!(url_from("netlify", false, &format!("Building...\n{json}")), "https://abc--site.netlify.app");
        assert_eq!(url_from("vercel", false, "Vercel CLI 39\nInspect: https://vercel.com/x/1\nhttps://app-abc.vercel.app\n"), "https://app-abc.vercel.app");
        assert_eq!(url_from("custom", true, "Live at https://example.com/app, done."), "https://example.com/app");
        assert_eq!(url_from("custom", true, "no address here"), "");
    }

    #[test]
    fn a_long_log_keeps_its_end() {
        let log = format!("{}THE END", "x".repeat(LOG_KEPT + 10));
        let cut = kept(log);
        assert!(cut.starts_with("[the start was cut]") && cut.ends_with("THE END"));
        assert_eq!(kept("short".into()), "short");
    }

    // Real processes, with a stand-in for the CLI.

    #[cfg(windows)]
    struct StandIn {
        dir: std::path::PathBuf,
        cli: std::path::PathBuf,
    }

    #[cfg(windows)]
    fn stand_in(fail: bool) -> StandIn {
        let dir = std::env::temp_dir().join(format!("qs-deploy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cli = dir.join("netlify.cmd");
        let body = format!(
            "@echo off\r\necho args=%*\r\necho seen-token=%NETLIFY_AUTH_TOKEN%\r\necho stage=%QS_DEPLOY_STAGE%\r\necho ran> \"{}\"\r\n{}\r\n",
            dir.join("deployed.txt").display(),
            if fail { "exit /b 3" } else { "echo {\"deploy_url\":\"https://preview.example\",\"url\":\"https://live.example\"}" }
        );
        std::fs::write(&cli, body).unwrap();
        StandIn { dir, cli }
    }

    #[cfg(windows)]
    fn go(s: &StandIn, target: &DeployTarget, production: bool, token: Option<&str>) -> (Result<String, String>, String) {
        let mut log = String::new();
        let stage = if production { "production" } else { "preview" };
        let cli = s.cli.clone();
        let result = run_steps(&plan(target, production), &target.kind, production, &s.dir, token, stage, &move |_| Some(cli.clone()), &mut log, Duration::from_secs(60));
        (result, log)
    }

    #[cfg(windows)]
    #[test]
    fn the_token_only_travels_in_the_environment_and_never_into_the_log() {
        let s = stand_in(false);
        let target = DeployTarget { build_command: String::new(), ..netlify() };
        let token = "nfp_SECRETSECRET";
        let (result, log) = go(&s, &target, false, Some(token));
        assert_eq!(result.unwrap(), "https://preview.example");
        assert!(!log.contains(token), "{log}");
        assert!(log.contains("seen-token=***"), "the CLI got it, the log did not: {log}");
        assert!(log.contains("args=deploy --json --dir dist --site my-site"), "{log}");
        assert!(!log.contains("--prod"), "{log}");
        assert!(log.contains("stage=preview"), "{log}");
        let (result, log) = go(&s, &target, true, Some(token));
        assert_eq!(result.unwrap(), "https://live.example");
        assert!(log.contains("--prod") && log.contains("stage=production"), "{log}");
        let _ = std::fs::remove_dir_all(&s.dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_build_stops_before_anything_goes_up() {
        let s = stand_in(false);
        let target = DeployTarget { build_command: "echo building & exit /b 7".into(), ..netlify() };
        let (result, log) = go(&s, &target, false, Some("nfp_SECRETSECRET"));
        assert_eq!(result.unwrap_err(), "Build failed.");
        assert!(log.contains("building"), "{log}");
        assert!(!s.dir.join("deployed.txt").exists(), "the deploy never ran");

        // The build does not get the token.
        let target = DeployTarget { build_command: "echo build-token=%NETLIFY_AUTH_TOKEN%".into(), ..netlify() };
        let (result, log) = go(&s, &target, false, Some("nfp_SECRETSECRET"));
        assert!(result.is_ok());
        assert!(log.contains("build-token=%NETLIFY_AUTH_TOKEN%"), "cmd leaves an unset variable as it is: {log}");
        assert!(s.dir.join("deployed.txt").exists());
        let _ = std::fs::remove_dir_all(&s.dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_deploy_is_a_failure_and_a_missing_cli_says_how_to_install_it() {
        let s = stand_in(true);
        let target = DeployTarget { build_command: String::new(), ..netlify() };
        let (result, _) = go(&s, &target, false, None);
        assert_eq!(result.unwrap_err(), "Deploy a preview failed.");

        let mut log = String::new();
        let e = run_steps(&plan(&target, false), "netlify", false, &s.dir, None, "preview", &|_| None, &mut log, Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("npm install -g netlify-cli"), "{e}");
        let _ = std::fs::remove_dir_all(&s.dir);
    }

    #[test]
    fn a_run_cut_off_by_a_closed_app_is_not_a_good_preview() {
        let db = Database::open_in_memory().unwrap();
        let mut r = run("preview", "abc", "running", false);
        r.id = "r1".into();
        r.project = "p".into();
        db.deploy_save(&r).unwrap();
        db.deploy_mark_interrupted(5).unwrap();
        let after = db.deploy_runs("p", 10).unwrap();
        assert_eq!(after[0].status, "failed");
        assert!(after[0].log.contains("result is unknown"));
        assert_eq!(after[0].finished_at, Some(5));
    }
}
