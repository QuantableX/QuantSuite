//! The adapter boundary (docs/PLAN-QUANTPILOT.md, V2).
//!
//! Everything the user sees is generic — the terminal, the session list,
//! the face, the panel under it. Each CLI is one adapter that answers five
//! questions: how to launch in a folder, how to resume, where the CLI keeps
//! its sessions, how to hand it the context and QuantMCP, and which signals
//! it can send back. Six ship built in; a CLI that does not exist yet is a
//! [`CustomAdapter`] from settings.
//!
//! An adapter does not start anything itself: it produces the arguments the
//! session's terminal bakes into a wrapper command (`shell.rs`), and the
//! user starts the CLI by typing that command.

use crate::contexts::Context;
use crate::db::SessionMeta;
use crate::providers::Launchable;
use crate::settings::{CustomAdapter, Settings};
use crate::shell::Wrapper;
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Claude,
    Codex,
    Pi,
    Omp,
    Opencode,
    Gemini,
    Custom,
}

#[derive(Clone, Debug)]
pub struct Adapter {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub custom: Option<CustomAdapter>,
}

const BUILT_IN: [(&str, &str, Kind); 6] = [
    ("claude", "Claude Code", Kind::Claude),
    ("codex", "Codex", Kind::Codex),
    ("pi", "pi", Kind::Pi),
    ("omp", "omp", Kind::Omp),
    ("opencode", "OpenCode", Kind::Opencode),
    ("gemini", "Gemini CLI", Kind::Gemini),
];

pub fn is_built_in(id: &str) -> bool {
    BUILT_IN.iter().any(|(b, _, _)| *b == id)
}

pub fn all(settings: &Settings) -> Vec<Adapter> {
    let mut out: Vec<Adapter> = BUILT_IN
        .iter()
        .map(|(id, label, kind)| Adapter { id: (*id).into(), label: (*label).into(), kind: *kind, custom: None })
        .collect();
    for c in &settings.custom {
        out.push(Adapter { id: c.id.clone(), label: c.label.clone(), kind: Kind::Custom, custom: Some(c.clone()) });
    }
    out
}

pub fn find(settings: &Settings, id: &str) -> Option<Adapter> {
    all(settings).into_iter().find(|a| a.id == id)
}

/// Where a CLI writes its sessions, for reading the id back and tailing.
#[derive(Clone, Debug)]
pub enum Store {
    None,
    /// `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`
    Codex(PathBuf),
    /// `<root>/--<encoded cwd>--/<ts>_<uuid>.jsonl` (pi, and omp as its fork)
    PiLike(PathBuf),
}

/// How the pilot learns what the agent is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Signals {
    /// Claude Code hooks and statusLine, injected per launch.
    Hooks,
    /// The CLI's own session file, tailed.
    Transcript,
    /// Nothing structured: output activity and the bell.
    Activity,
}

impl Adapter {
    pub fn store(&self) -> Store {
        let home = dirs::home_dir().unwrap_or_default();
        match self.kind {
            Kind::Codex => Store::Codex(home.join(".codex").join("sessions")),
            Kind::Pi => Store::PiLike(home.join(".pi").join("agent").join("sessions")),
            Kind::Omp => Store::PiLike(home.join(".omp").join("agent").join("sessions")),
            _ => Store::None,
        }
    }

    pub fn signals(&self) -> Signals {
        match self.kind {
            Kind::Claude => Signals::Hooks,
            Kind::Codex | Kind::Pi | Kind::Omp => Signals::Transcript,
            _ => Signals::Activity,
        }
    }

    /// What the row is bound to the moment this adapter starts, when the
    /// adapter does not report an id of its own.
    pub fn immediate_binding(&self, session_id: &str) -> Option<String> {
        match self.kind {
            Kind::Pi | Kind::Custom => Some(session_id.to_string()),
            Kind::Opencode | Kind::Gemini => Some("latest".to_string()),
            Kind::Claude | Kind::Codex | Kind::Omp => None,
        }
    }
}

// ── Launching ──

/// The generated files a session's wrappers point at.
pub struct SessionFiles {
    /// The context prompt (`--append-system-prompt-file` and friends).
    pub context: PathBuf,
    /// The QuantMCP config for Claude; `None` when not attached.
    pub mcp: Option<PathBuf>,
    /// Claude's hooks and statusLine.
    pub claude_settings: PathBuf,
}

pub struct LaunchInput<'a> {
    pub meta: &'a SessionMeta,
    pub ctx: &'a Context,
    pub general_vault: &'a Path,
    pub mcp_url: Option<&'a str>,
    pub settings: &'a Settings,
    pub files: &'a SessionFiles,
}

fn slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Claude Code's per-project transcript directory: every character of the
/// cwd that is not alphanumeric becomes `-`.
pub fn encode_claude_dir(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// pi's (and omp's) per-cwd session directory: the same rule, wrapped in `--`.
pub fn encode_pi_dir(cwd: &str) -> String {
    format!("--{}--", encode_claude_dir(cwd))
}

pub fn claude_transcript_path(cwd: &str, session_id: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("projects")
        .join(encode_claude_dir(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// The settings Claude Code gets on the command line: one hook command for
/// every event the face cares about, and a statusLine that reports vitals.
/// Both are a `curl` to the relay; the session id rides in the payload.
pub fn claude_settings_json(relay_port: u16) -> String {
    let hook = format!(
        "curl -s -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{relay_port}/claude/hook"
    );
    let status = format!(
        "curl -s -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{relay_port}/claude/status"
    );
    let entry = json!([{ "hooks": [{ "type": "command", "command": hook, "timeout": 5 }] }]);
    let mut hooks = serde_json::Map::new();
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
        "Notification",
        "Stop",
        "SessionEnd",
        "PreCompact",
    ] {
        hooks.insert(event.into(), entry.clone());
    }
    json!({
        "hooks": hooks,
        "statusLine": { "type": "command", "command": status, "padding": 0 },
    })
    .to_string()
}

/// Split a template the way a shell would, minus everything but quotes.
pub fn split_template(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut had_token = false;
    for c in template.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                had_token = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if had_token {
                    out.push(std::mem::take(&mut cur));
                    had_token = false;
                }
            }
            c => {
                cur.push(c);
                had_token = true;
            }
        }
    }
    if had_token {
        out.push(cur);
    }
    out
}

fn expand(token: &str, input: &LaunchInput<'_>, model: &str) -> String {
    token
        .replace("{cwd}", &input.meta.cwd)
        .replace("{session}", &input.meta.id)
        .replace("{title}", &input.meta.title)
        .replace("{mode}", &input.meta.mode)
        .replace("{model}", model)
        .replace("{vault}", &slashes(input.general_vault))
        .replace("{providerSession}", input.meta.provider_session_id.as_deref().unwrap_or(""))
}

/// The adapter's arguments for this session: `(args, resumes)`. The row is
/// bound to one adapter; every other adapter starts fresh.
pub fn adapter_args(adapter: &Adapter, input: &LaunchInput<'_>) -> (Vec<String>, bool) {
    let meta = input.meta;
    let model = Some(meta.model.trim())
        .filter(|m| !m.is_empty())
        .or_else(|| input.settings.model_for(&adapter.id))
        .unwrap_or("")
        .to_string();
    let bound = (meta.provider == adapter.id)
        .then_some(meta.provider_session_id.as_deref())
        .flatten()
        .filter(|s| !s.is_empty());
    let vault_readable = input.settings.include_general_vault && input.ctx.kind != "general";
    let vault = slashes(input.general_vault);
    let mut args: Vec<String> = Vec::new();
    let mut resumed = false;

    match adapter.kind {
        Kind::Claude => {
            // Ours at first; `/clear` inside the CLI moves the session to a
            // new id, which the relay reports back as the one to resume.
            let resume_id = bound.unwrap_or(&meta.id);
            let transcript = claude_transcript_path(&meta.cwd, resume_id);
            if bound.is_some() && transcript.is_file() {
                args.extend(["--resume".into(), resume_id.to_string()]);
                resumed = true;
            } else {
                args.extend(["--session-id".into(), meta.id.clone()]);
            }
            if !meta.title.trim().is_empty() {
                args.extend(["-n".into(), meta.title.trim().to_string()]);
            }
            if !model.is_empty() {
                args.extend(["--model".into(), model.clone()]);
            }
            match meta.mode.as_str() {
                "auto" => args.extend(["--permission-mode".into(), "auto".into()]),
                "full" => args.extend(["--permission-mode".into(), "bypassPermissions".into(), "--dangerously-skip-permissions".into()]),
                _ => args.extend(["--permission-mode".into(), "default".into()]),
            }
            if !input.settings.effort.is_empty() {
                args.extend(["--effort".into(), input.settings.effort.clone()]);
            }
            if let Some(mcp) = input.files.mcp.as_deref() {
                args.extend(["--mcp-config".into(), path_arg(mcp)]);
            }
            if vault_readable {
                args.extend(["--add-dir".into(), input.general_vault.to_string_lossy().into_owned()]);
            }
            args.extend(["--append-system-prompt-file".into(), path_arg(&input.files.context)]);
            args.extend(["--settings".into(), path_arg(&input.files.claude_settings)]);
        }
        Kind::Codex => {
            if let Some(id) = bound {
                args.extend(["resume".into(), id.to_string()]);
                resumed = true;
            }
            if !model.is_empty() {
                args.extend(["-m".into(), model.clone()]);
            }
            match meta.mode.as_str() {
                "auto" => args.push("--full-auto".into()),
                "full" => args.push("--dangerously-bypass-approvals-and-sandbox".into()),
                _ => args.extend(["-a".into(), "untrusted".into(), "-s".into(), "workspace-write".into()]),
            }
            if let Some(url) = input.mcp_url {
                args.extend(["-c".into(), format!("mcp_servers.quantsuite.url=\"{url}\"")]);
            }
            if vault_readable && meta.mode != "full" {
                args.extend(["-c".into(), format!("sandbox_workspace_write.writable_roots=[\"{vault}\"]")]);
            }
        }
        Kind::Pi => {
            args.extend(["--session-id".into(), meta.id.clone()]);
            resumed = bound.is_some();
            if !meta.title.trim().is_empty() {
                args.extend(["-n".into(), meta.title.trim().to_string()]);
            }
            if !model.is_empty() {
                args.extend(["--model".into(), model.clone()]);
            }
            // pi takes text or a file's contents here.
            args.extend(["--append-system-prompt".into(), path_arg(&input.files.context)]);
        }
        Kind::Omp => {
            if let Some(id) = bound {
                args.extend(["--resume".into(), id.to_string()]);
                resumed = true;
            }
            if !model.is_empty() {
                args.extend(["--model".into(), model.clone()]);
            }
            let approval = match meta.mode.as_str() {
                "auto" => "write",
                "full" => "yolo",
                _ => "always-ask",
            };
            args.extend(["--approval-mode".into(), approval.into()]);
            if vault_readable {
                args.extend(["--add-dir".into(), vault.clone()]);
            }
            args.extend(["--append-system-prompt".into(), path_arg(&input.files.context)]);
        }
        Kind::Opencode => {
            if bound.is_some() {
                args.push("--continue".into());
                resumed = true;
            }
            if !model.is_empty() {
                args.extend(["-m".into(), model.clone()]);
            }
        }
        Kind::Gemini => {
            if bound.is_some() {
                args.extend(["--resume".into(), "latest".into()]);
                resumed = true;
            }
            if !model.is_empty() {
                args.extend(["-m".into(), model.clone()]);
            }
            let approval = match meta.mode.as_str() {
                "auto" => "auto_edit",
                "full" => "yolo",
                _ => "default",
            };
            args.extend(["--approval-mode".into(), approval.into()]);
            if vault_readable {
                args.extend(["--include-directories".into(), vault.clone()]);
            }
        }
        Kind::Custom => {
            let custom = adapter.custom.clone().unwrap_or_default();
            let template = if bound.is_some() && !custom.resume_args.is_empty() {
                resumed = true;
                custom.resume_args
            } else {
                custom.args
            };
            args.extend(split_template(&template).iter().map(|t| expand(t, input, &model)));
        }
    }
    (args, resumed)
}

/// The command the terminal defines for this adapter: `(wrapper, resumes)`.
pub fn wrapper(adapter: &Adapter, exe: &Launchable, input: &LaunchInput<'_>) -> (Wrapper, bool) {
    let (adapter_args, resumed) = adapter_args(adapter, input);
    let mut args = exe.prefix.clone();
    args.extend(adapter_args);
    (Wrapper { command: adapter.id.clone(), program: exe.program.clone(), args }, resumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(provider: &str, bound: Option<&str>) -> SessionMeta {
        SessionMeta {
            id: "11111111-2222-3333-4444-555555555555".into(),
            provider: provider.into(),
            provider_session_id: bound.map(String::from),
            title: "Fix the thing".into(),
            context_id: "general".into(),
            context_name: "General".into(),
            cwd: "C:\\Projects\\QuantSuite".into(),
            model: String::new(),
            mode: "full".into(),
            created_at: 0,
            updated_at: 0,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            last_error: None,
        }
    }

    fn files() -> SessionFiles {
        SessionFiles {
            context: PathBuf::from("C:\\d\\s\\context.txt"),
            mcp: Some(PathBuf::from("C:\\d\\s\\mcp.json")),
            claude_settings: PathBuf::from("C:\\d\\s\\claude-settings.json"),
        }
    }

    fn input<'a>(m: &'a SessionMeta, ctx: &'a Context, settings: &'a Settings, vault: &'a Path, files: &'a SessionFiles) -> LaunchInput<'a> {
        LaunchInput { meta: m, ctx, general_vault: vault, mcp_url: Some("http://localhost:3101/mcp"), settings, files }
    }

    #[test]
    fn encodes_dirs_like_the_clis_do() {
        assert_eq!(encode_claude_dir("C:\\Projects\\QuantSuite"), "C--Projects-QuantSuite");
        assert_eq!(
            encode_claude_dir("C:\\Users\\root\\.quantsuite-dev\\modules\\memory\\vault"),
            "C--Users-root--quantsuite-dev-modules-memory-vault"
        );
        assert_eq!(encode_pi_dir("C:\\Projects\\QuantCode"), "--C--Projects-QuantCode--");
    }

    #[test]
    fn claude_new_session_carries_id_mode_files_and_hooks() {
        let m = meta("", None);
        let ctx = Context { id: "general".into(), name: "General".into(), path: m.cwd.clone(), kind: "general".into() };
        let s = Settings::default();
        let vault = PathBuf::from("C:/vault");
        let f = files();
        let a = find(&s, "claude").unwrap();
        let (args, resumed) = adapter_args(&a, &input(&m, &ctx, &s, &vault, &f));
        assert!(!resumed);
        let joined = args.join(" ");
        assert!(joined.contains("--session-id 11111111-2222-3333-4444-555555555555"));
        assert!(joined.contains("-n Fix the thing"));
        assert!(joined.contains("--permission-mode bypassPermissions --dangerously-skip-permissions"));
        assert!(joined.contains("--mcp-config C:\\d\\s\\mcp.json"));
        assert!(joined.contains("--append-system-prompt-file C:\\d\\s\\context.txt"));
        assert!(joined.contains("--settings C:\\d\\s\\claude-settings.json"));
        assert!(!joined.contains("--add-dir"), "general context needs no add-dir");
        assert!(!args.iter().any(|a| a.contains('{') || a.contains('"')), "nothing long or quoted rides on the command line");
    }

    #[test]
    fn a_row_bound_to_codex_resumes_codex_and_starts_others_fresh() {
        let mut m = meta("codex", Some("abc"));
        m.mode = "ask".into();
        let ctx = Context { id: "w".into(), name: "W".into(), path: m.cwd.clone(), kind: "workspace".into() };
        let s = Settings::default();
        let vault = PathBuf::from("C:/vault");
        let f = files();
        let (args, resumed) = adapter_args(&find(&s, "codex").unwrap(), &input(&m, &ctx, &s, &vault, &f));
        assert!(resumed);
        assert_eq!(&args[..2], &["resume".to_string(), "abc".to_string()]);
        assert!(args.iter().any(|a| a.starts_with("mcp_servers.quantsuite.url=")));
        assert!(args.iter().any(|a| a.contains("writable_roots")), "ask mode gets the vault as a writable root");
        let (claude_args, claude_resumed) = adapter_args(&find(&s, "claude").unwrap(), &input(&m, &ctx, &s, &vault, &f));
        assert!(!claude_resumed);
        assert!(claude_args.join(" ").contains("--session-id"));
    }

    #[test]
    fn wrapper_prefixes_the_launchable() {
        let m = meta("", None);
        let ctx = Context { id: "general".into(), name: "General".into(), path: m.cwd.clone(), kind: "general".into() };
        let s = Settings { model: [("pi".to_string(), "gpt-5.4".to_string())].into_iter().collect(), ..Default::default() };
        let vault = PathBuf::from("C:/vault");
        let f = files();
        let exe = Launchable { program: "node.exe".into(), prefix: vec!["cli.js".into()], found: "pi.cmd".into() };
        let (w, _) = wrapper(&find(&s, "pi").unwrap(), &exe, &input(&m, &ctx, &s, &vault, &f));
        assert_eq!(w.command, "pi");
        assert_eq!(w.program, "node.exe");
        assert_eq!(&w.args[..3], &["cli.js".to_string(), "--session-id".to_string(), m.id.clone()]);
        assert!(w.args.windows(2).any(|p| p == ["--model", "gpt-5.4"]), "the per-adapter model from settings applies");
    }

    #[test]
    fn custom_template_expands() {
        let m = meta("", None);
        let ctx = Context { id: "general".into(), name: "General".into(), path: m.cwd.clone(), kind: "general".into() };
        let s = Settings {
            custom: vec![CustomAdapter {
                id: "mine".into(),
                label: "Mine".into(),
                exe: "agent".into(),
                args: "--dir {cwd} --name \"{title}\" --id {session}".into(),
                resume_args: String::new(),
            }],
            ..Default::default()
        }
        .normalized();
        let vault = PathBuf::from("C:/vault");
        let f = files();
        let (args, _) = adapter_args(&find(&s, "mine").unwrap(), &input(&m, &ctx, &s, &vault, &f));
        assert_eq!(
            args,
            vec!["--dir", "C:\\Projects\\QuantSuite", "--name", "Fix the thing", "--id", "11111111-2222-3333-4444-555555555555"]
        );
    }

    #[test]
    fn split_template_groups_quotes() {
        assert_eq!(split_template("a \"b c\" d"), vec!["a", "b c", "d"]);
        assert_eq!(split_template("  "), Vec::<String>::new());
        assert_eq!(split_template("x \"\" y"), vec!["x", "", "y"]);
    }
}
