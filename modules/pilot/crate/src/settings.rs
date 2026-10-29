//! `settings.json` in the module's data dir: the shell a session opens
//! with, the permission mode the wrappers bake in, where a CLI is when PATH
//! is not enough, the model per adapter, and the user's own adapters for
//! CLIs the pilot has never heard of.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Bumped when a default changes for existing files: 2 = full access by
/// default (2026-09-02, the user's call).
const VERSION: u32 = 2;

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
            self.version = VERSION;
        }
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
