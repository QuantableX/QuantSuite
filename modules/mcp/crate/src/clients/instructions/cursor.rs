//! Cursor keeps its User Rules in a private settings database and reads no
//! global rules file. It does load local plugins from
//! `~/.cursor/plugins/local/<name>/`, and a plugin rule marked `alwaysApply`
//! is a global rule in every chat (observed in Cursor 3.21.18). So General
//! AgentOS reaches Cursor as the one rule of a small plugin of our own.
//!
//! A rule needs its frontmatter, so it is a copy, not a link: re-import after
//! changing AgentOS, then reload Cursor's windows. A `sessionStart` hook is no
//! alternative — Cursor caps hook context at 10,000 characters.
use super::read_optional;
use std::fs;
use std::path::{Path, PathBuf};

const NAME: &str = "quantmcp-agentos";

/// `~/.cursor/plugins/local/quantmcp-agentos`
pub(super) fn plugin_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".cursor").join("plugins").join("local").join(NAME))
}

pub(super) fn rule_path(dir: &Path) -> PathBuf {
    dir.join("rules").join("agentos.mdc")
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join(".cursor-plugin").join("plugin.json")
}

fn manifest() -> String {
    let manifest = serde_json::json!({
        "name": NAME,
        "description": "General AgentOS from QuantSuite (~/.quantmcp/AGENT.md), applied to every Cursor chat. Managed by QuantMCP.",
        "version": "1.0.0",
        "rules": "./rules/",
    });
    format!("{}\n", serde_json::to_string_pretty(&manifest).expect("a literal JSON value"))
}

/// The rule file: frontmatter that makes Cursor apply it everywhere, then the
/// brief as every other client receives it.
pub(super) fn rule(brief: &str) -> String {
    format!("---\ndescription: General AgentOS, the operator's standing brief for every agent (QuantMCP)\nalwaysApply: true\n---\n\n{brief}")
}

/// Write the plugin into `dir`. A plugin of another name in that folder is
/// never replaced.
pub(super) fn install(dir: &Path, rule: &str) -> Result<String, String> {
    let manifest_path = manifest_path(dir);
    let existing = read_optional(&manifest_path)?;
    if !existing.is_empty() {
        let name = serde_json::from_str::<serde_json::Value>(&existing)
            .ok()
            .and_then(|value| value.get("name").and_then(|name| name.as_str()).map(str::to_owned));
        if name.as_deref() != Some(NAME) {
            return Err(format!(
                "{} belongs to another Cursor plugin; it was left untouched. Move it away and import again.",
                dir.display()
            ));
        }
    }
    let rule_path = rule_path(dir);
    let manifest = manifest();
    if existing == manifest && read_optional(&rule_path)? == rule {
        return Ok(format!("Up to date: {}", rule_path.display()));
    }
    for (path, text) in [(&manifest_path, manifest.as_str()), (&rule_path, rule)] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
        qs_core::paths::write_atomic(path, text.as_bytes()).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    }
    Ok(format!("Imported: {} (Cursor plugin {NAME}; reload Cursor windows)", rule_path.display()))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    #[test]
    fn writes_an_always_applied_plugin_rule_and_is_idempotent() {
        let temp = Scratch::new();
        let dir = temp.0.join(NAME);
        let rule = rule("# QuantMCP AgentOS\n\nrules v1\n");
        assert!(rule.starts_with("---\n") && rule.contains("\nalwaysApply: true\n---\n\n# QuantMCP AgentOS"));

        assert!(install(&dir, &rule).unwrap().starts_with("Imported"));
        let manifest: serde_json::Value = serde_json::from_str(&fs::read_to_string(manifest_path(&dir)).unwrap()).unwrap();
        assert_eq!(manifest["name"], NAME);
        assert_eq!(manifest["rules"], "./rules/");
        assert_eq!(fs::read_to_string(rule_path(&dir)).unwrap(), rule);
        assert!(install(&dir, &rule).unwrap().starts_with("Up to date"));

        let changed = super::rule("# QuantMCP AgentOS\n\nrules v2\n");
        assert!(install(&dir, &changed).unwrap().starts_with("Imported"));
        assert!(fs::read_to_string(rule_path(&dir)).unwrap().contains("rules v2"));
    }

    #[test]
    fn never_replaces_another_plugin_in_the_folder() {
        let temp = Scratch::new();
        for (case, text) in [("other", r#"{"name":"someone-else"}"#), ("broken", "not json")] {
            let dir = temp.0.join(case);
            fs::create_dir_all(manifest_path(&dir).parent().unwrap()).unwrap();
            fs::write(manifest_path(&dir), text).unwrap();
            assert!(install(&dir, &rule("rules")).is_err(), "{case}");
            assert_eq!(fs::read_to_string(manifest_path(&dir)).unwrap(), text);
            assert!(!rule_path(&dir).exists());
        }
    }
}
