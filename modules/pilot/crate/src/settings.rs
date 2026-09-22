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
        }
    }
}

pub fn valid_mode(mode: &str) -> bool {
    matches!(mode, "ask" | "auto" | "full")
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
