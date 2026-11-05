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

/// What the user chose last (round 2, card 03). New threads start with it
/// unless the project pins a value; Settings → General gives the values
/// before the first choice.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Sticky {
    /// The agent picked last.
    pub provider: String,
    /// Per agent: the model picked last (`""` = the CLI's default, chosen).
    pub model: BTreeMap<String, String>,
    /// Per agent: the reasoning effort picked last.
    pub effort: BTreeMap<String, String>,
    /// The permission mode picked last.
    pub mode: String,
    /// `local` or `worktree`, picked last.
    pub workspace: String,
    /// Per project: the base branch of a worktree thread (`""` = the checkout's branch).
    pub base_branch: BTreeMap<String, String>,
}

/// An agent's model and effort for a new thread in a project.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetup {
    pub model: String,
    pub effort: String,
    /// The project pins this model.
    pub model_pinned: bool,
}

/// ONE answer to "what does a new thread start with here" (round 2, card
/// 03): the project's pin, else the user's last choice, else Settings.
/// Every way to start a thread asks it; the menus show it.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NewSetup {
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub mode: String,
    pub workspace: String,
    pub base_branch: String,
    /// The values the project pins: `provider`, `model`, `mode`, `workspace`.
    pub pinned: Vec<String>,
    /// The agent wanted is not available here: what happens instead.
    pub notice: Option<String>,
    /// Every agent's model and effort here (the agent menu's rows).
    pub agents: BTreeMap<String, AgentSetup>,
}

impl NewSetup {
    /// The model and effort `provider` starts with (the resolved agent's, or another's).
    pub fn agent(&self, provider: &str) -> AgentSetup {
        self.agents.get(provider).cloned().unwrap_or_default()
    }
}

/// The agents whose CLI takes a reasoning effort.
fn takes_effort(provider: &str) -> bool {
    matches!(provider, "claude" | "codex")
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
    /// After a turn that changed files: run it, and when it fails queue one
    /// repair for the agent (round 2, card 12).
    #[serde(default)]
    pub check_after_turn: bool,
}

/// Another account of an agent (round 3, card 02): the CLI pointed at its
/// own config home, so its login, sessions and usage stay apart from the
/// default account's. QuantPilot never reads or writes the credentials in it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Instance {
    /// Lowercase letters, digits and dashes; unique per agent.
    pub id: String,
    /// `claude` or `codex` — the agents whose CLI takes a config home.
    pub provider: String,
    pub label: String,
    /// The account's config home (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`).
    pub home: String,
}

/// What the default account's login folder variable must be, given what the
/// suite inherited and what the system has: nothing to do when they agree;
/// else the system's value, or the inherited one removed (`-NAME`). A
/// terminal that started the suite (Orca sets `CODEX_HOME` to its own
/// account folders) must not decide which login an agent uses.
pub fn home_fix(var: &str, inherited: Option<&str>, system: Option<&str>) -> Option<(String, String)> {
    let inherited = inherited.map(str::trim).filter(|v| !v.is_empty());
    let system = system.map(str::trim).filter(|v| !v.is_empty());
    if inherited.map(|v| v.replace('/', "\\").to_lowercase()) == system.map(|v| v.replace('/', "\\").to_lowercase()) {
        return None;
    }
    Some(match system {
        Some(v) => (var.to_string(), v.to_string()),
        None => (format!("-{var}"), String::new()),
    })
}

/// [`home_fix`] for this process, both variables (agents started without
/// the settings' environment: helper runs, title writing).
pub fn system_homes() -> Vec<(String, String)> {
    ["CODEX_HOME", "CLAUDE_CONFIG_DIR"]
        .into_iter()
        .filter_map(|v| home_fix(v, std::env::var(v).ok().as_deref(), qs_core::system_path::system_var(v).as_deref()))
        .collect()
}

/// The variable that points an agent's CLI at another config home.
pub fn home_var(provider: &str) -> Option<&'static str> {
    match provider {
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        "codex" => Some("CODEX_HOME"),
        _ => None,
    }
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
    /// More accounts of Claude Code and Codex (round 3, card 02).
    pub instances: Vec<Instance>,
    /// Phone access (round 4, card 1). Changed only through its own commands,
    /// which check the address.
    pub phone: crate::phone::PhoneSettings,
    /// The Telegram bot (round 4, card 2), changed the same way.
    pub telegram: crate::telegram::TelegramSettings,
    /// The webhook listener (round 4, card 3), changed the same way.
    pub triggers: crate::phone::PhoneSettings,
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
    /// Settings → Storage (parity 19): cleanup rules, all off by default.
    pub storage: crate::db::StoragePolicy,
    /// What the user chose last; new threads start with it (round 2, card 03).
    pub sticky: Sticky,
    /// While recording the screen, threads with one of these tags are
    /// hidden from the lists (round 2, card 12).
    pub hide_tags_while_recording: Vec<String>,
    /// Crews (round 2, card 07): workers running at once per crew.
    pub crew_parallel: u32,
    /// Minutes a crew worker may work on its task before it is stopped; 0 = no limit.
    pub crew_timeout_minutes: u32,
    /// Crew workers start in their own git worktree.
    pub crew_worktrees: bool,
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
            instances: Vec::new(),
            phone: Default::default(),
            telegram: Default::default(),
            triggers: Default::default(),
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
            storage: crate::db::StoragePolicy::default(),
            render_math: true,
            projects: BTreeMap::new(),
            sticky: Sticky::default(),
            hide_tags_while_recording: vec!["private".into()],
            crew_parallel: 4,
            crew_timeout_minutes: 60,
            crew_worktrees: true,
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
        if !valid_mode(&self.sticky.mode) {
            self.sticky.mode.clear();
        }
        if !matches!(self.sticky.workspace.as_str(), "local" | "worktree") {
            self.sticky.workspace.clear();
        }
        let trim_map = |m: BTreeMap<String, String>| m.into_iter().map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).filter(|(k, _)| !k.is_empty()).collect();
        self.sticky.model = trim_map(std::mem::take(&mut self.sticky.model));
        self.sticky.effort = trim_map(std::mem::take(&mut self.sticky.effort));
        self.sticky.base_branch = trim_map(std::mem::take(&mut self.sticky.base_branch));
        self.hide_tags_while_recording = crate::db::clean_tags(&self.hide_tags_while_recording);
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

        let mut seen_instances = std::collections::BTreeSet::new();
        self.instances = self
            .instances
            .into_iter()
            .filter_map(|mut i| {
                i.id = slug(&i.id);
                i.provider = i.provider.trim().to_string();
                i.label = i.label.trim().to_string();
                i.home = i.home.trim().to_string();
                if i.id.is_empty() || i.home.is_empty() || home_var(&i.provider).is_none() || !seen_instances.insert((i.provider.clone(), i.id.clone())) {
                    return None;
                }
                if i.label.is_empty() {
                    i.label = i.id.clone();
                }
                Some(i)
            })
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
                // A user's own agent named like one round 2 adds keeps
                // working under its own id instead of vanishing (card 11).
                if crate::adapters::extra(&c.id).is_some() {
                    c.id = format!("{}-own", c.id);
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

    /// Every agent a chat thread can run: the built-ins, the round-2 agents
    /// that speak ACP (card 11) and the user's ACP agents.
    pub fn chat_providers(&self) -> Vec<String> {
        CHAT_PROVIDERS
            .iter()
            .map(|c| c.to_string())
            .chain(crate::adapters::EXTRAS.iter().filter(|e| e.acp.is_some()).map(|e| e.id.to_string()))
            .chain(self.custom.iter().filter(|c| c.protocol == "acp").map(|c| c.id.clone()))
            .collect()
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

    /// An agent's model and effort for a new thread in `project`: the
    /// project's model pin, else the last pick for that agent, else the
    /// remembered model of earlier versions; the effort only where the
    /// agent takes one.
    pub fn agent_setup(&self, project: &str, provider: &str) -> AgentSetup {
        let pin = self.projects.get(project).and_then(|p| p.model.get(provider)).filter(|m| !m.is_empty());
        let model = match pin {
            Some(m) => m.clone(),
            None => self.sticky.model.get(provider).cloned().or_else(|| self.model_for(provider).map(str::to_string)).unwrap_or_default(),
        };
        let effort = if takes_effort(provider) {
            self.sticky.effort.get(provider).cloned().unwrap_or_else(|| self.effort.trim().to_string())
        } else {
            String::new()
        };
        AgentSetup { model, effort, model_pinned: pin.is_some() }
    }

    /// What a new thread in `project` starts with (round 2, card 03). The
    /// agent: the project's pin, else the last pick, else the default — and
    /// when that one is not available here (`available` says whether its
    /// CLI is installed) the default or the first offered one, with a notice.
    pub fn new_thread_setup(&self, project: &str, available: &dyn Fn(&str) -> bool, label: &dyn Fn(&str) -> String) -> NewSetup {
        let p = self.projects.get(project);
        let offered = self.enabled_in(project);
        let usable: Vec<String> = offered.iter().filter(|id| available(id)).cloned().collect();
        let mut pinned = Vec::new();
        let pin = p.and_then(|p| p.provider.clone()).filter(|id| self.is_chat_provider(id));
        let last = Some(self.sticky.provider.clone()).filter(|id| !id.is_empty() && self.is_chat_provider(id));
        let wanted = pin.clone().or(last).unwrap_or_else(|| self.default_provider.clone());
        let provider = if usable.contains(&wanted) {
            wanted.clone()
        } else if usable.contains(&self.default_provider) {
            self.default_provider.clone()
        } else {
            usable.first().or(offered.first()).cloned().unwrap_or_else(|| wanted.clone())
        };
        if pin.as_deref() == Some(provider.as_str()) {
            pinned.push("provider".to_string());
        }
        let notice = (provider != wanted).then(|| {
            let why = if !self.is_chat_provider(&wanted) {
                "is gone"
            } else if !offered.contains(&wanted) {
                if p.and_then(|p| p.enabled.as_ref()).is_some() { "is off in this project" } else { "is turned off in Settings" }
            } else {
                "is not installed"
            };
            format!("{} {why} — this thread starts with {}.", label(&wanted), label(&provider))
        });
        let agents: BTreeMap<String, AgentSetup> = self.chat_providers().into_iter().map(|id| (id.clone(), self.agent_setup(project, &id))).collect();
        let own = agents.get(&provider).cloned().unwrap_or_default();
        if own.model_pinned {
            pinned.push("model".to_string());
        }
        let mode = match p.and_then(|p| p.mode.clone()).filter(|m| valid_mode(m)) {
            Some(m) => {
                pinned.push("mode".to_string());
                m
            }
            None => Some(self.sticky.mode.clone()).filter(|m| valid_mode(m)).unwrap_or_else(|| self.default_mode.clone()),
        };
        let workspace = match p.and_then(|p| p.workspace.clone()).filter(|w| matches!(w.as_str(), "local" | "worktree")) {
            Some(w) => {
                pinned.push("workspace".to_string());
                w
            }
            None => Some(self.sticky.workspace.clone()).filter(|w| matches!(w.as_str(), "local" | "worktree")).unwrap_or_else(|| self.new_thread_workspace.clone()),
        };
        NewSetup {
            model: own.model,
            effort: own.effort,
            provider,
            mode,
            workspace,
            base_branch: self.sticky.base_branch.get(project).cloned().unwrap_or_default(),
            pinned,
            notice,
            agents,
        }
    }

    /// Settings saved from the app (round 2, card 03): the last choices are
    /// the app's own record, never taken from the (possibly older) copy the
    /// page sends; a default the user changes in Settings becomes the start
    /// again, so the change shows on the next new thread.
    pub fn merged_into(mut self, current: &Settings) -> Settings {
        // The settings page holds a copy: it can never turn phone access on.
        self.phone = current.phone.clone();
        self.telegram = current.telegram.clone();
        self.triggers = current.triggers.clone();
        let mut sticky = current.sticky.clone();
        if self.default_provider != current.default_provider {
            sticky.provider.clear();
        }
        if self.default_mode != current.default_mode {
            sticky.mode.clear();
        }
        if self.new_thread_workspace != current.new_thread_workspace {
            sticky.workspace.clear();
        }
        for id in current.model.keys().chain(self.model.keys()) {
            if self.model.get(id) != current.model.get(id) {
                sticky.model.remove(id);
            }
        }
        if self.effort != current.effort {
            sticky.effort.remove("claude");
        }
        self.sticky = sticky;
        self
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

    /// An account saved without a folder gets its own under QuantPilot's
    /// data folder: `accounts/<agent>-<id>`.
    pub fn with_account_homes(mut self, data_dir: &Path) -> Settings {
        for i in &mut self.instances {
            let id = slug(&i.id);
            if i.home.trim().is_empty() && !id.is_empty() {
                i.home = data_dir.join("accounts").join(format!("{}-{id}", i.provider.trim())).to_string_lossy().into_owned();
            }
        }
        self
    }

    /// An account of an agent; `""` is the default account (no instance).
    pub fn instance(&self, provider: &str, id: &str) -> Option<&Instance> {
        if id.is_empty() {
            return None;
        }
        self.instances.iter().find(|i| i.provider == provider && i.id == id)
    }

    /// A thread's environment: the agent's, plus its account's config home.
    /// The default account signs in as the PC does: its login folder variable
    /// as the system has it (see [`home_fix`]), unless set in these settings.
    pub fn launch_env_for(&self, provider: &str, instance: &str) -> Vec<(String, String)> {
        let mut env = self.launch_env(provider);
        let Some(var) = home_var(provider) else {
            return env;
        };
        if let Some(i) = self.instance(provider, instance) {
            env.retain(|(k, _)| k != var);
            env.push((var.to_string(), i.home.clone()));
        } else if !env.iter().any(|(k, _)| k == var) {
            env.extend(home_fix(var, std::env::var(var).ok().as_deref(), qs_core::system_path::system_var(var).as_deref()));
        }
        env
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
    fn the_default_account_signs_in_as_the_pc_does() {
        // A terminal that started the suite pointed CODEX_HOME at an empty folder: the agent gets it removed.
        assert_eq!(home_fix("CODEX_HOME", Some("C:/orca/acc/home"), None), Some(("-CODEX_HOME".into(), String::new())));
        // The user set it for Windows: the agent gets that, whatever the terminal had.
        assert_eq!(home_fix("CODEX_HOME", Some("C:/orca/acc/home"), Some("D:/codex")), Some(("CODEX_HOME".into(), "D:/codex".into())));
        assert_eq!(home_fix("CODEX_HOME", None, Some("D:/codex")), Some(("CODEX_HOME".into(), "D:/codex".into())));
        // Inherited and system agree (slashes and case aside), or neither is set: nothing to change.
        assert_eq!(home_fix("CODEX_HOME", Some("d:\\Codex"), Some("D:/codex")), None);
        assert_eq!(home_fix("CLAUDE_CONFIG_DIR", None, None), None);
        assert_eq!(home_fix("CLAUDE_CONFIG_DIR", Some("  "), None), None);
        // A value set in QuantPilot's settings wins over the system's.
        let mut s = Settings::default();
        s.env.insert("CODEX_HOME".into(), "E:/mine".into());
        let env = s.launch_env_for("codex", "");
        assert_eq!(env.iter().filter(|(k, _)| k.trim_start_matches('-') == "CODEX_HOME").count(), 1);
        assert!(env.contains(&("CODEX_HOME".to_string(), "E:/mine".to_string())));
    }

    #[test]
    fn instances_need_an_agent_with_a_home_and_point_it_there() {
        let s = Settings {
            instances: vec![
                Instance { id: " Work ".into(), provider: "claude".into(), label: "".into(), home: " C:/claude-work ".into() },
                Instance { id: "work".into(), provider: "claude".into(), label: "dup".into(), home: "C:/x".into() },
                Instance { id: "work".into(), provider: "codex".into(), label: "Work".into(), home: "C:/codex-work".into() },
                Instance { id: "gem".into(), provider: "gemini".into(), label: "".into(), home: "C:/g".into() },
                Instance { id: "nohome".into(), provider: "claude".into(), label: "".into(), home: "  ".into() },
            ],
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.instances.len(), 2, "a duplicate, an agent without a home variable and an empty home go");
        assert_eq!(s.instances[0], Instance { id: "work".into(), provider: "claude".into(), label: "work".into(), home: "C:/claude-work".into() });
        assert!(s.launch_env_for("claude", "work").contains(&("CLAUDE_CONFIG_DIR".to_string(), "C:/claude-work".to_string())));
        assert!(s.launch_env_for("codex", "work").contains(&("CODEX_HOME".to_string(), "C:/codex-work".to_string())));
        assert!(!s.launch_env_for("claude", "").iter().any(|(k, _)| k == "CLAUDE_CONFIG_DIR"), "the default account keeps the system's login folder");
        assert!(!s.launch_env_for("claude", "gone").iter().any(|(k, _)| k == "CLAUDE_CONFIG_DIR"), "an unknown account is the default one");
        assert_eq!(home_var("pi"), None);

        let homed = Settings { instances: vec![Instance { id: "Side".into(), provider: "codex".into(), label: "".into(), home: " ".into() }], ..Default::default() }
            .with_account_homes(Path::new("D:/pilot"))
            .normalized();
        assert_eq!(Path::new(&homed.instances[0].home), Path::new("D:/pilot").join("accounts").join("codex-side"), "an account without a folder gets its own");
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

    fn all_there(_: &str) -> bool {
        true
    }
    fn name(id: &str) -> String {
        match id {
            "claude" => "Claude Code".into(),
            "codex" => "Codex".into(),
            other => other.into(),
        }
    }

    /// Round 2, card 11: the ACP agents round 2 adds run chat threads; a
    /// user's own agent with one of their ids is kept under another.
    #[test]
    fn round_two_agents_chat_and_own_agents_keep_their_place() {
        let s = Settings {
            custom: vec![CustomAdapter { id: "kilo".into(), exe: "my-kilo".into(), protocol: "acp".into(), ..Default::default() }],
            ..Default::default()
        }
        .normalized();
        for id in ["kilo", "hermes", "openclaw"] {
            assert!(s.is_chat_provider(id), "{id}");
        }
        for id in ["cursor-agent", "freebuff", "grok", "agy", "kiro-cli", "droid", "cline", "devin"] {
            assert!(!s.is_chat_provider(id), "{id} runs as a terminal session");
        }
        assert_eq!(s.custom[0].id, "kilo-own");
        assert!(s.is_chat_provider("kilo-own"));
    }

    /// Round 2, card 03: project pin → last choice → Settings, per value.
    #[test]
    fn a_new_thread_starts_with_the_last_choice_unless_the_project_pins_it() {
        let mut s = Settings { default_provider: "claude".into(), default_mode: "full".into(), ..Default::default() };
        s.model.insert("claude".into(), "opus".into());
        // Before any choice: Settings, and the remembered model of earlier versions.
        let d = s.new_thread_setup("ws", &all_there, &name);
        assert_eq!((d.provider.as_str(), d.model.as_str(), d.mode.as_str(), d.workspace.as_str()), ("claude", "opus", "full", "local"));
        assert!(d.pinned.is_empty() && d.notice.is_none());
        // The user picks Codex, a model and effort per agent, Plan, a worktree off main.
        s.sticky = Sticky {
            provider: "codex".into(),
            model: [("codex".to_string(), "gpt-a".to_string()), ("claude".to_string(), "haiku".to_string())].into(),
            effort: [("codex".to_string(), "high".to_string()), ("pi".to_string(), "low".to_string())].into(),
            mode: "plan".into(),
            workspace: "worktree".into(),
            base_branch: [("ws".to_string(), "main".to_string())].into(),
        };
        let s = s.normalized();
        let d = s.new_thread_setup("ws", &all_there, &name);
        assert_eq!((d.provider.as_str(), d.model.as_str(), d.effort.as_str()), ("codex", "gpt-a", "high"));
        assert_eq!((d.mode.as_str(), d.workspace.as_str(), d.base_branch.as_str()), ("plan", "worktree", "main"));
        assert_eq!(d.agent("claude").model, "haiku", "each agent keeps its own model");
        assert_eq!(d.agent("pi").effort, "", "pi takes no effort");
        assert_eq!(s.new_thread_setup("other", &all_there, &name).base_branch, "", "the base branch is per project");
        // A project pin wins and is named.
        let mut p = s.clone();
        p.projects.insert(
            "ws".into(),
            ProjectPrefs { provider: Some("claude".into()), mode: Some("ask".into()), model: [("claude".to_string(), "sonnet".to_string())].into(), ..Default::default() },
        );
        let d = p.new_thread_setup("ws", &all_there, &name);
        assert_eq!((d.provider.as_str(), d.model.as_str(), d.mode.as_str()), ("claude", "sonnet", "ask"));
        assert_eq!(d.pinned, vec!["provider", "model", "mode"]);
        assert!(d.agent("claude").model_pinned);
        assert_eq!(p.new_thread_setup("other", &all_there, &name).provider, "codex", "elsewhere the last choice");
    }

    /// Round 2, card 03: saving Settings keeps the app's record of the last
    /// choices, and a changed default starts over from it.
    #[test]
    fn saving_settings_keeps_the_last_choices_unless_a_default_changes() {
        let mut current = Settings { sticky: Sticky { provider: "codex".into(), mode: "plan".into(), workspace: "worktree".into(), ..Default::default() }, ..Default::default() };
        current.sticky.model.insert("claude".into(), "haiku".into());
        current.sticky.model.insert("codex".into(), "gpt-a".into());
        current.sticky.effort.insert("claude".into(), "low".into());
        // The page's copy is older (no sticky) and only changes the toast setting.
        let page = Settings { notify: "off".into(), ..Default::default() };
        let saved = page.clone().merged_into(&current);
        assert_eq!(saved.sticky, current.sticky);
        assert_eq!(saved.notify, "off");
        // A new default agent, mode and Claude model: those start over.
        let mut changed = page;
        changed.default_provider = "gemini".into();
        changed.default_mode = "ask".into();
        changed.model.insert("claude".into(), "opus".into());
        changed.effort = "high".into();
        let saved = changed.merged_into(&current);
        assert_eq!((saved.sticky.provider.as_str(), saved.sticky.mode.as_str(), saved.sticky.workspace.as_str()), ("", "", "worktree"));
        assert_eq!(saved.sticky.model.get("claude"), None);
        assert_eq!(saved.sticky.model.get("codex").map(String::as_str), Some("gpt-a"));
        assert_eq!(saved.sticky.effort.get("claude"), None);
        let d = saved.new_thread_setup("ws", &all_there, &name);
        assert_eq!((d.provider.as_str(), d.mode.as_str(), d.model.as_str(), d.effort.as_str()), ("gemini", "ask", "", ""));
        assert_eq!(saved.agent_setup("ws", "claude").model, "opus");
    }

    /// Round 2, card 03: an agent that is not available falls back, and says so.
    #[test]
    fn an_unavailable_agent_falls_back_with_a_notice() {
        let mut s = Settings { default_provider: "claude".into(), ..Default::default() };
        s.sticky.provider = "codex".into();
        let no_codex = |id: &str| id != "codex";
        let d = s.new_thread_setup("ws", &no_codex, &name);
        assert_eq!(d.provider, "claude");
        assert_eq!(d.notice.as_deref(), Some("Codex is not installed — this thread starts with Claude Code."));
        s.providers.insert("codex".into(), ProviderPrefs { disabled: true, ..Default::default() });
        let d = s.new_thread_setup("ws", &all_there, &name);
        assert_eq!(d.notice.as_deref(), Some("Codex is turned off in Settings — this thread starts with Claude Code."));
        s.providers.clear();
        s.projects.insert("ws".into(), ProjectPrefs { enabled: Some(vec!["claude".into()]), ..Default::default() });
        let d = s.new_thread_setup("ws", &all_there, &name);
        assert_eq!(d.notice.as_deref(), Some("Codex is off in this project — this thread starts with Claude Code."));
        // The default itself unavailable: the first offered one.
        let mut t = Settings { default_provider: "claude".into(), ..Default::default() };
        t.providers.insert("claude".into(), ProviderPrefs { disabled: true, ..Default::default() });
        let d = t.new_thread_setup("x", &all_there, &name);
        assert_eq!(d.provider, "codex");
        assert!(d.notice.is_some());
        // A removed custom agent as the last choice.
        let mut u = Settings::default();
        u.sticky.provider = "gone-agent".into();
        let d = u.normalized().new_thread_setup("x", &all_there, &name);
        assert_eq!(d.provider, "claude");
        assert!(d.notice.is_none(), "an id that is no agent any more is ignored, the default is no fallback");
        // Invalid sticky values fall away on load.
        let mut v = Settings::default();
        v.sticky.mode = "yolo".into();
        v.sticky.workspace = "cloud".into();
        let v = v.normalized();
        assert_eq!((v.sticky.mode.as_str(), v.sticky.workspace.as_str()), ("", ""));
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
