//! `settings.json` in the module's data dir: the shell a session opens
//! with, the permission mode the wrappers bake in, where a CLI is when PATH
//! is not enough, the model per adapter, and the user's own adapters for
//! CLIs the pilot has never heard of.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Bumped when a default changes for existing files: 2 = full access by
/// default (2026-09-02, the user's call). 3 = providers, environment,
/// appearance and project overrides (card 19); a V2 file keeps everything.
const VERSION: u32 = 3;

/// Per agent (card 19): extra launch arguments, environment variables, the
/// user's own model ids for the picker, and whether it is offered at all.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderPrefs {
    /// Shell-style; after Claude's own arguments, before Codex's `app-server`.
    pub args: String,
    pub env: BTreeMap<String, String>,
    pub models: Vec<String>,
    pub disabled: bool,
}

/// What a project changes for its new threads (card 19); `None` inherits.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectPrefs {
    pub provider: Option<String>,
    /// Adapter id → model.
    pub model: BTreeMap<String, String>,
    pub mode: Option<String>,
    pub workspace: Option<String>,
    /// The agents offered here; `None` = the global choice.
    pub enabled: Option<Vec<String>>,
}

/// The defaults a new thread in a project gets: default → project.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    pub provider: String,
    pub mode: String,
    pub workspace: String,
    pub enabled: Vec<String>,
}

/// The built-in agents a chat thread can run (card 20); a user's own ACP agents join them.
pub const CHAT_PROVIDERS: [&str; 7] = ["claude", "codex", "gemini", "opencode", "copilot", "pi", "omp"];

/// Variables that make Claude Code use something other than the account
/// saved by `claude auth login` — dropped from new Claude processes when
/// "Prefer saved Claude login" is on (PandaCode's list; Claude's env-var
/// reference). Variables set in the settings themselves still apply.
pub const CLAUDE_CREDENTIAL_VARS: [&str; 16] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
];

fn env_name(k: &str) -> bool {
    let mut c = k.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn clean_env(env: BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.into_iter().map(|(k, v)| (k.trim().to_string(), v)).filter(|(k, _)| env_name(k)).collect()
}

/// A CLI the pilot does not know: an executable and two argument templates.
///
/// Placeholders in the templates: `{cwd}`, `{session}`, `{title}`, `{mode}`
/// (`ask` | `auto` | `full`), `{model}`, `{vault}` (the general memory vault)
/// and `{providerSession}` (whatever the adapter reported, if anything).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomAdapter {
    /// Lowercase letters, digits and dashes; must not collide with a built-in.
    /// It is also the command the wrapper defines in the terminal.
    pub id: String,
    pub label: String,
    /// Executable name (resolved on PATH) or a full path.
    pub exe: String,
    /// Arguments for a new session, shell-style (double quotes group).
    pub args: String,
    /// Arguments when the session is reopened; empty = the same as `args`.
    pub resume_args: String,
    /// `acp`: a chat thread speaks the Agent Client Protocol with it (card
    /// 20). `terminal` (the V2 entries): it runs in a terminal session.
    #[serde(default = "terminal_protocol")]
    pub protocol: String,
}

fn terminal_protocol() -> String {
    "terminal".into()
}

fn no_version() -> u32 {
    0
}

/// A project action (card 13): a command run in a Run terminal of the
/// thread's checkout — from the header, by its key, when a worktree is
/// created, or when a thread settles (`cargo clean`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectAction {
    pub id: String,
    pub name: String,
    pub command: String,
    /// `play` | `test` | `lint` | `build` | `configure` | `debug`
    pub icon: String,
    /// A shortcut like `Ctrl+Shift+R`; empty = none.
    pub key: String,
    pub run_on_worktree_create: bool,
    pub run_on_settle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Absent in a file = 0, so the migration below sees it — the struct
    /// default would report the current version and skip it.
    #[serde(default = "no_version")]
    pub version: u32,
    /// Shell a session's terminal runs; empty = pwsh → Windows PowerShell → cmd.
    pub shell: String,
    /// `ask` | `auto` | `full` — what the wrappers pass the CLIs.
    pub default_mode: String,
    /// Adapter id → explicit executable path; absent = resolve the install.
    pub exe: BTreeMap<String, String>,
    /// Adapter id → model to launch with; absent = the CLI's own default.
    pub model: BTreeMap<String, String>,
    /// Claude `--effort`; empty = default.
    pub effort: String,
    /// Register QuantMCP with every session whose adapter can take it.
    pub attach_quantmcp: bool,
    /// Workspace sessions also get the general vault as a readable directory.
    pub include_general_vault: bool,
    pub custom: Vec<CustomAdapter>,
    /// The composer's send key: `enter` (Enter sends), `modEnterMultiline`
    /// (Ctrl+Enter once the message has several lines), `modEnter` (always Ctrl+Enter).
    pub send_shortcut: String,
    /// What a message sent while the agent works does: `queue` (waits for the
    /// turn to end) or `steer` (goes into the running turn).
    pub follow_up: String,
    /// A Windows toast when a thread needs you: `off`, `unfocused` (only
    /// while the suite is in the background) or `always`.
    pub notify: String,
    /// A short sound when a thread starts waiting on you.
    pub notify_sound: bool,
    /// Settle a thread nobody touched for this many days (0 = never).
    pub auto_settle_days: u32,
    /// Fold running threads into the sidebar's Working section; they come
    /// back on top of the active list when they finish or need you.
    pub working_section: bool,
    /// Where new threads work: `local` or `worktree` (card 10).
    pub new_thread_workspace: String,
    /// The start of a worktree thread's branch name.
    pub worktree_prefix: String,
    /// Generated commit messages: `repository` (the project's recent
    /// subjects and commit instructions) or `standard` (card 12).
    pub commit_style: String,
    /// Project actions by project (context id), in the user's order.
    pub actions: BTreeMap<String, Vec<ProjectAction>>,
    /// A usage-limit stop schedules the continuation for the reset (card 15).
    pub limit_resume: bool,
    /// A usage-limit stop snoozes the thread until the reset (card 15).
    pub limit_snooze: bool,
    /// The agent of a new thread (card 19).
    pub default_provider: String,
    /// Adapter id → its launch preferences (card 19).
    pub providers: BTreeMap<String, ProviderPrefs>,
    /// Variables for every agent process; an agent's own win.
    pub env: BTreeMap<String, String>,
    /// New Claude processes ignore inherited API keys, tokens and endpoints.
    pub prefer_saved_claude_login: bool,
    /// The conversation's text: `small`, `default`, `large`, `larger`.
    pub text_size: String,
    /// How long panels and menus take to open, 0–400 ms (reduced motion: none).
    pub panel_motion_ms: u32,
    /// `$…$` and `$$…$$` render as formulas.
    pub render_math: bool,
    /// Context id → what that project changes (card 19).
    pub projects: BTreeMap<String, ProjectPrefs>,
    /// GitHub through `gh` (card 23); off = no `gh` process at all.
    pub github: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            shell: String::new(),
            default_mode: "full".into(),
            exe: BTreeMap::new(),
            model: BTreeMap::new(),
            effort: String::new(),
            attach_quantmcp: true,
            include_general_vault: true,
            custom: Vec::new(),
            send_shortcut: "enter".into(),
            follow_up: "queue".into(),
            notify: "unfocused".into(),
            notify_sound: false,
            auto_settle_days: 3,
            working_section: true,
            new_thread_workspace: "local".into(),
            worktree_prefix: "quantpilot/".into(),
            commit_style: "repository".into(),
            actions: BTreeMap::new(),
            limit_resume: false,
            limit_snooze: false,
            default_provider: "claude".into(),
            providers: BTreeMap::new(),
            env: BTreeMap::new(),
            prefer_saved_claude_login: false,
            text_size: "default".into(),
            panel_motion_ms: 160,
            github: true,
            render_math: true,
            projects: BTreeMap::new(),
        }
    }
}

/// `ask` (Supervised), `edits` (Accept edits), `auto`, `plan`, `full` (Full
/// access) — docs/PLAN-QUANTPILOT.md §6. V2's terminal wrappers know the
/// first, third and fifth and treat the others as `ask`.
pub fn valid_mode(mode: &str) -> bool {
    matches!(mode, "ask" | "edits" | "auto" | "plan" | "full")
}

fn slug(id: &str) -> String {
    id.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        // A file from before "full by default" keeps everything but the mode.
        if self.version < 2 {
            self.default_mode = "full".into();
        }
        // A V2 file keeps everything; the new fields start at their defaults (card 19).
        self.version = VERSION;
        let chat = self.chat_providers();
        if !chat.contains(&self.default_provider) {
            self.default_provider = "claude".into();
        }
        if !matches!(self.text_size.as_str(), "small" | "default" | "large" | "larger") {
            self.text_size = "default".into();
        }
        self.panel_motion_ms = self.panel_motion_ms.min(400);
        self.env = clean_env(std::mem::take(&mut self.env));
        self.providers = std::mem::take(&mut self.providers)
            .into_iter()
            .map(|(id, mut p)| {
                p.args = p.args.trim().to_string();
                p.env = clean_env(p.env);
                let mut seen = std::collections::BTreeSet::new();
                p.models = p.models.into_iter().map(|m| m.trim().to_string()).filter(|m| !m.is_empty() && seen.insert(m.clone())).collect();
                (id.trim().to_string(), p)
            })
            .filter(|(id, p)| !id.is_empty() && *p != ProviderPrefs::default())
            .collect();
        self.projects = std::mem::take(&mut self.projects)
            .into_iter()
            .map(|(id, mut p)| {
                p.provider = p.provider.filter(|v| chat.contains(v));
                p.mode = p.mode.filter(|m| valid_mode(m));
                p.workspace = p.workspace.filter(|w| matches!(w.as_str(), "local" | "worktree"));
                p.model = p.model.into_iter().map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).filter(|(k, v)| !k.is_empty() && !v.is_empty()).collect();
                (id.trim().to_string(), p)
            })
            .filter(|(id, p)| !id.is_empty() && *p != ProjectPrefs::default())
            .collect();
        if !valid_mode(&self.default_mode) {
            self.default_mode = "full".into();
        }
        if !matches!(self.send_shortcut.as_str(), "enter" | "modEnterMultiline" | "modEnter") {
            self.send_shortcut = "enter".into();
        }
        if !matches!(self.notify.as_str(), "off" | "unfocused" | "always") {
            self.notify = "unfocused".into();
        }
        if !matches!(self.new_thread_workspace.as_str(), "local" | "worktree") {
            self.new_thread_workspace = "local".into();
        }
        self.worktree_prefix = self.worktree_prefix.trim().to_string();
        if !matches!(self.commit_style.as_str(), "repository" | "standard") {
            self.commit_style = "repository".into();
        }
        self.actions = std::mem::take(&mut self.actions)
            .into_iter()
            .map(|(project, list)| {
                let mut seen = std::collections::HashSet::new();
                let list: Vec<ProjectAction> = list
                    .into_iter()
                    .map(|mut a| {
                        a.name = a.name.trim().to_string();
                        a.command = a.command.trim().to_string();
                        a.key = a.key.trim().to_string();
                        if !matches!(a.icon.as_str(), "play" | "test" | "lint" | "build" | "configure" | "debug") {
                            a.icon = "play".into();
                        }
                        let base = if a.id.trim().is_empty() { slug(&a.name) } else { slug(&a.id) };
                        let base = if base.is_empty() { "action".to_string() } else { base };
                        a.id = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|c| !seen.contains(c)).unwrap_or(base);
                        seen.insert(a.id.clone());
                        a
                    })
                    .filter(|a| !a.name.is_empty() && !a.command.is_empty())
                    .collect();
                (project.trim().to_string(), list)
            })
            .filter(|(p, l)| !p.is_empty() && !l.is_empty())
            .collect();
        if !matches!(self.follow_up.as_str(), "queue" | "steer") {
            self.follow_up = "queue".into();
        }
        self.shell = self.shell.trim().to_string();
        self.effort = self.effort.trim().to_string();
        self.exe = self
            .exe
            .into_iter()
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect();
        self.model = self
            .model
            .into_iter()
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect();

        let mut seen = std::collections::BTreeSet::new();
        self.custom = self
            .custom
            .into_iter()
            .filter_map(|mut c| {
                c.id = slug(&c.id);
                c.exe = c.exe.trim().to_string();
                c.label = c.label.trim().to_string();
                c.args = c.args.trim().to_string();
                c.resume_args = c.resume_args.trim().to_string();
                if c.protocol != "acp" {
                    c.protocol = "terminal".into();
                }
                if c.id.is_empty() || c.exe.is_empty() || crate::adapters::is_built_in(&c.id) || !seen.insert(c.id.clone()) {
                    return None;
                }
                if c.label.is_empty() {
                    c.label = c.id.clone();
                }
                Some(c)
            })
            .collect();
        self
    }

    pub fn model_for(&self, adapter: &str) -> Option<&str> {
        self.model.get(adapter).map(String::as_str).filter(|m| !m.is_empty())
    }

    /// The model a project's threads of `adapter` start with: the project's, else the default.
    pub fn model_in(&self, project: &str, adapter: &str) -> Option<&str> {
        self.projects.get(project).and_then(|p| p.model.get(adapter)).map(String::as_str).filter(|m| !m.is_empty()).or_else(|| self.model_for(adapter))
    }

    /// Every agent a chat thread can run: the built-ins and the user's ACP agents.
    pub fn chat_providers(&self) -> Vec<String> {
        CHAT_PROVIDERS.iter().map(|c| c.to_string()).chain(self.custom.iter().filter(|c| c.protocol == "acp").map(|c| c.id.clone())).collect()
    }

    pub fn is_chat_provider(&self, id: &str) -> bool {
        self.chat_providers().iter().any(|c| c == id)
    }

    /// The agents offered: each not disabled, or the project's own list.
    pub fn enabled_in(&self, project: &str) -> Vec<String> {
        let all = self.chat_providers();
        if let Some(list) = self.projects.get(project).and_then(|p| p.enabled.as_ref()) {
            return all.into_iter().filter(|c| list.contains(c)).collect();
        }
        all.into_iter().filter(|c| !self.providers.get(c).is_some_and(|p| p.disabled)).collect()
    }

    /// A new thread's defaults in a project: default → project.
    pub fn defaults_for(&self, project: &str) -> Defaults {
        let p = self.projects.get(project);
        let enabled = self.enabled_in(project);
        let wanted = p.and_then(|p| p.provider.clone()).unwrap_or_else(|| self.default_provider.clone());
        let provider = if enabled.contains(&wanted) { wanted } else { enabled.first().cloned().unwrap_or(wanted) };
        Defaults {
            provider,
            mode: p.and_then(|p| p.mode.clone()).unwrap_or_else(|| self.default_mode.clone()),
            workspace: p.and_then(|p| p.workspace.clone()).unwrap_or_else(|| self.new_thread_workspace.clone()),
            enabled,
        }
    }

    /// Environment for an agent process: the global variables, the agent's
    /// own, and for Claude with "Prefer saved Claude login" the inherited
    /// credential variables removed first (`-NAME` entries).
    pub fn launch_env(&self, adapter: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let own = self.providers.get(adapter).map(|p| &p.env);
        if adapter == "claude" && self.prefer_saved_claude_login {
            for v in CLAUDE_CREDENTIAL_VARS {
                if !own.is_some_and(|e| e.contains_key(v)) && !self.env.contains_key(v) {
                    out.push((format!("-{v}"), String::new()));
                }
            }
        }
        for (k, v) in &self.env {
            if !own.is_some_and(|e| e.contains_key(k)) {
                out.push((k.clone(), v.clone()));
            }
        }
        if let Some(e) = own {
            out.extend(e.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        out
    }

    /// The agent's extra launch arguments.
    pub fn extra_args(&self, adapter: &str) -> Vec<String> {
        self.providers.get(adapter).map(|p| crate::adapters::split_template(&p.args)).unwrap_or_default()
    }

    /// What a warm process was started with beyond the thread: a change starts a fresh one.
    pub fn launch_fingerprint(&self, adapter: &str) -> String {
        format!("{:?}|{:?}", self.extra_args(adapter), self.launch_env(adapter))
    }

    pub fn exe_for(&self, adapter: &str) -> &str {
        self.exe.get(adapter).map(String::as_str).unwrap_or("")
    }
}

pub fn read(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
        .normalized()
}

pub fn write(dir: &Path, settings: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    qs_core::paths::write_atomic(&dir.join("settings.json"), text.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_actions_are_cleaned_and_get_unique_ids() {
        let action = |name: &str, command: &str, icon: &str| ProjectAction { name: name.into(), command: command.into(), icon: icon.into(), ..Default::default() };
        let mut actions = BTreeMap::new();
        actions.insert(
            "ws1".to_string(),
            vec![action(" Dev server ", " npm run dev ", "play"), action("Dev server", "npm run dev:2", "rocket"), action("Empty", "  ", "test")],
        );
        actions.insert("ws2".to_string(), vec![action("", "x", "play")]);
        let s = Settings { actions, ..Default::default() }.normalized();
        let list = &s.actions["ws1"];
        assert_eq!(list.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["dev-server", "dev-server-2"]);
        assert_eq!((list[0].name.as_str(), list[0].command.as_str()), ("Dev server", "npm run dev"));
        assert_eq!(list[1].icon, "play", "an unknown icon falls back");
        assert!(!s.actions.contains_key("ws2"), "a project without valid actions is dropped");
    }

    #[test]
    fn custom_adapters_are_slugged_and_deduped() {
        let s = Settings {
            custom: vec![
                CustomAdapter { id: " My Agent ".into(), exe: "agent".into(), ..Default::default() },
                CustomAdapter { id: "my-agent".into(), exe: "agent2".into(), ..Default::default() },
                CustomAdapter { id: "claude".into(), exe: "x".into(), ..Default::default() },
                CustomAdapter { id: "noexe".into(), exe: "  ".into(), ..Default::default() },
            ],
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.custom.len(), 1);
        assert_eq!(s.custom[0].id, "my-agent");
        assert_eq!(s.custom[0].label, "my-agent");
    }

    /// Card 19: a V2 file keeps its executables, models, mode, attachments and own agents.
    #[test]
    fn a_v2_file_keeps_everything() {
        let v2: Settings = serde_json::from_str(
            r#"{ "version": 2, "defaultMode": "ask", "exe": { "codex": "C:\\tools\\codex.exe" }, "model": { "claude": "opus" },
                 "attachQuantmcp": false, "includeGeneralVault": false, "custom": [{ "id": "mine", "exe": "mine.exe" }] }"#,
        )
        .unwrap();
        let s = v2.normalized();
        assert_eq!(s.version, VERSION);
        assert_eq!((s.default_mode.as_str(), s.exe_for("codex"), s.model_for("claude")), ("ask", r"C:\tools\codex.exe", Some("opus")));
        assert!(!s.attach_quantmcp && !s.include_general_vault);
        assert_eq!((s.custom[0].id.as_str(), s.custom[0].protocol.as_str()), ("mine", "terminal"), "a V2 agent stays a terminal agent");
        assert_eq!((s.default_provider.as_str(), s.panel_motion_ms, s.render_math), ("claude", 160, true));
    }

    /// Card 19: default → project, per row.
    #[test]
    fn project_overrides_layer_over_the_defaults() {
        let mut s = Settings { default_mode: "edits".into(), default_provider: "codex".into(), ..Default::default() };
        s.model.insert("claude".into(), "opus".into());
        s.projects.insert(
            "ws1".into(),
            ProjectPrefs { provider: Some("claude".into()), mode: Some("plan".into()), model: [("claude".to_string(), "sonnet".to_string())].into(), ..Default::default() },
        );
        s.projects.insert("ws2".into(), ProjectPrefs { enabled: Some(vec!["claude".into()]), workspace: Some("worktree".into()), ..Default::default() });
        let s = s.normalized();
        let d = s.defaults_for("ws1");
        assert_eq!((d.provider.as_str(), d.mode.as_str(), d.workspace.as_str()), ("claude", "plan", "local"));
        assert_eq!(s.model_in("ws1", "claude"), Some("sonnet"));
        assert_eq!(s.model_in("other", "claude"), Some("opus"), "elsewhere the default model");
        let d = s.defaults_for("ws2");
        assert_eq!((d.provider.as_str(), d.workspace.as_str()), ("claude", "worktree"), "Codex is off here, so the default falls to Claude");
        assert_eq!(d.enabled, vec!["claude".to_string()]);
        assert_eq!(s.defaults_for("other").provider, "codex");
        // A disabled default agent hands over to the next one.
        let mut off = s.clone();
        off.providers.insert("codex".into(), ProviderPrefs { disabled: true, ..Default::default() });
        assert_eq!(off.defaults_for("other").provider, "claude");
        // Invalid overrides fall away.
        let mut bad = Settings::default();
        bad.projects.insert("ws3".into(), ProjectPrefs { provider: Some("no-such-agent".into()), mode: Some("yolo".into()), ..Default::default() });
        assert!(bad.normalized().projects.is_empty());
    }

    /// Card 19: the environment of an agent process, and "Prefer saved Claude login".
    #[test]
    fn launch_env_layers_and_strips_inherited_credentials() {
        let mut s = Settings::default();
        s.env.insert("HTTPS_PROXY".into(), "http://proxy:8080".into());
        s.env.insert("bad name".into(), "x".into());
        s.providers.insert("claude".into(), ProviderPrefs { env: [("ANTHROPIC_BASE_URL".to_string(), "https://gw".to_string())].into(), args: "--verbose \"a b\"".into(), ..Default::default() });
        let s = s.normalized();
        assert!(!s.env.contains_key("bad name"));
        assert_eq!(s.launch_env("codex"), vec![("HTTPS_PROXY".to_string(), "http://proxy:8080".to_string())]);
        assert!(!s.launch_env("claude").iter().any(|(k, _)| k.starts_with('-')), "off by default");
        let on = Settings { prefer_saved_claude_login: true, ..s.clone() };
        let env = on.launch_env("claude");
        assert!(env.contains(&("-ANTHROPIC_API_KEY".to_string(), String::new())));
        assert!(env.contains(&("-CLAUDE_CODE_OAUTH_TOKEN".to_string(), String::new())));
        assert!(!env.iter().any(|(k, _)| k == "-ANTHROPIC_BASE_URL"), "a variable set here is the user's choice");
        assert!(env.contains(&("ANTHROPIC_BASE_URL".to_string(), "https://gw".to_string())));
        assert_eq!(on.launch_env("codex").len(), 1, "Codex keeps its environment");
        assert_eq!(s.extra_args("claude"), vec!["--verbose".to_string(), "a b".to_string()]);
        assert_ne!(s.launch_fingerprint("claude"), on.launch_fingerprint("claude"));
    }

    #[test]
    fn old_files_move_to_full_access_once() {
        let old: Settings = serde_json::from_str(r#"{ "defaultMode": "ask", "effort": "high" }"#).unwrap();
        let s = old.normalized();
        assert_eq!(s.default_mode, "full");
        assert_eq!(s.version, VERSION);
        assert_eq!(s.effort, "high");
        // A current file keeps a deliberate "ask".
        let kept = Settings { default_mode: "ask".into(), ..Default::default() }.normalized();
        assert_eq!(kept.default_mode, "ask");
    }
}
