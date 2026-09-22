//! Operator-defined Markdown recipients, persisted in the suite's settings.
//! No executable discovery or MCP connection is implied by registering a file.
use super::{install, selected_clients, ClientSpec, ConnectResult, Outcome};
use crate::clients::ClientScan;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRecipient {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

pub fn validate_custom_recipients(recipients: &[CustomRecipient]) -> Result<(), String> {
    if recipients.len() > 100 {
        return Err("At most 100 custom agents are supported.".into());
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for recipient in recipients {
        if recipient
            .id
            .strip_prefix("custom:")
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .is_none()
            || !ids.insert(&recipient.id)
        {
            return Err("Custom agent IDs must be unique custom UUIDs.".into());
        }
        if recipient.name.trim().is_empty()
            || recipient.name.chars().count() > 80
            || recipient.name.chars().any(char::is_control)
        {
            return Err("Custom agents need a name of 1–80 characters.".into());
        }
        if !recipient.path.is_absolute()
            || !recipient
                .path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            return Err(format!(
                "{} needs an absolute Markdown (.md) instruction-file path.",
                recipient.name
            ));
        }
        let path =
            std::fs::canonicalize(&recipient.path).unwrap_or_else(|_| recipient.path.clone());
        let key = if cfg!(windows) {
            path.to_string_lossy().replace('\\', "/").to_lowercase()
        } else {
            path.to_string_lossy().into_owned()
        };
        if !paths.insert(key) {
            return Err("This custom instruction file is already registered.".into());
        }
    }
    Ok(())
}

pub fn load_custom_recipients(app: &tauri::AppHandle) -> Result<Vec<CustomRecipient>, String> {
    let value = crate::settings::with_core_db(app, |conn| {
        qs_core::db::get_setting(conn, "mcp", "agentos.custom_recipients")
            .map_err(|e| e.to_string())
    })?;
    let recipients: Vec<CustomRecipient> = match value {
        Some(value) => {
            serde_json::from_value(value).map_err(|_| "Custom agent settings are invalid")?
        }
        None => Vec::new(),
    };
    validate_custom_recipients(&recipients)?;
    Ok(recipients)
}

impl CustomRecipient {
    fn ready(&self) -> bool {
        self.path.is_file() || (!self.path.exists() && self.path.parent().is_some_and(Path::is_dir))
    }

    pub fn scan(&self) -> ClientScan {
        let ready = self.ready();
        ClientScan {
            id: self.id.clone(), name: self.name.clone(), installed: ready,
            configured: false, config_path: None, restart: false, manual: true,
            mcp_selection_supported: false,
            note: Some("MCP setup is managed by this agent, separately from instruction import.".into()),
            instructions_manual: false,
            instructions_note: Some(if ready {
                "Imports a managed section into the Markdown file you chose. Make sure your agent loads this file. Existing text is preserved."
            } else {
                "The instruction file or its parent folder is unavailable. Edit this custom target to choose an existing folder."
            }.into()),
            instructions_paths: vec![self.path.display().to_string()], custom: true,
            detection: Some("Added by you; application installation has not been verified.".into()),
            mcp_support: "custom".into(),
        }
    }

    pub(super) fn import(&self, source: &Path, text: &str) -> ConnectResult {
        let body = format!("# QuantMCP AgentOS\n\nSource: {}\nManaged by QuantMCP. Re-import after changing General AgentOS.\n\n{text}", source.display());
        let outcome = if self.ready() {
            install(&self.path, source, &body, None)
        } else {
            Err("Instruction file or its parent folder is unavailable; nothing was written.".into())
        };
        let (outcome, detail) = match outcome {
            Ok(detail) => (Outcome::Written, detail),
            Err(detail) => (Outcome::Failed, detail),
        };
        ConnectResult {
            id: format!("{}:agentos:0", self.id),
            name: format!("{} · AgentOS", self.name),
            outcome,
            detail,
            restart: false,
        }
    }
}

type Selection<'a> = (Vec<&'static ClientSpec>, Vec<&'a CustomRecipient>);

pub(super) fn resolve_selection<'a>(
    ids: &[String],
    custom: &'a [CustomRecipient],
) -> Result<Selection<'a>, String> {
    validate_custom_recipients(custom)?;
    if ids.is_empty() {
        return Err("Select at least one agent before importing.".into());
    }
    let mut built_in = Vec::new();
    let mut selected = Vec::new();
    for id in ids {
        if id.starts_with("custom:") {
            let recipient = custom
                .iter()
                .find(|recipient| recipient.id == *id)
                .ok_or("Custom agent is no longer registered. Refresh the agent list.")?;
            if !selected
                .iter()
                .any(|item: &&CustomRecipient| item.id == *id)
            {
                selected.push(recipient);
            }
        } else {
            built_in.push(id.clone());
        }
    }
    // Resolve all IDs before the caller starts writing any destination.
    let built_in = if built_in.is_empty() {
        Vec::new()
    } else {
        selected_clients(&built_in)?
    };
    Ok((built_in, selected))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    fn recipient(path: PathBuf) -> CustomRecipient {
        CustomRecipient {
            id: format!("custom:{}", uuid::Uuid::new_v4()),
            name: "My agent".into(),
            path,
        }
    }

    #[test]
    fn custom_targets_are_validated_before_any_import_and_only_selected_are_resolved() {
        let temp = Scratch::new();
        let custom = vec![
            recipient(temp.0.join("one.md")),
            recipient(temp.0.join("two.md")),
        ];
        let (built_in, selected) = resolve_selection(
            &["pi".into(), custom[1].id.clone(), custom[1].id.clone()],
            &custom,
        )
        .unwrap();
        assert_eq!(built_in[0].id, "pi");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path, custom[1].path);
        assert!(resolve_selection(&[], &custom).is_err());
        assert!(resolve_selection(&[custom[0].id.clone(), "unknown".into()], &custom).is_err());
        assert!(resolve_selection(&[format!("custom:{}", uuid::Uuid::new_v4())], &custom).is_err());
        assert!(validate_custom_recipients(&[recipient("relative.md".into())]).is_err());
        assert!(validate_custom_recipients(&[recipient(temp.0.join("settings.json"))]).is_err());
        assert!(validate_custom_recipients(&[
            custom[0].clone(),
            recipient(custom[0].path.clone())
        ])
        .is_err());
    }

    #[test]
    fn custom_import_preserves_backs_up_and_never_modifies_unselected_files() {
        let temp = Scratch::new();
        let source = temp.0.join("source.md");
        let custom = vec![
            recipient(temp.0.join("one.md")),
            recipient(temp.0.join("two.md")),
        ];
        std::fs::write(&source, "Shared policy").unwrap();
        std::fs::write(&custom[0].path, "My preferences").unwrap();
        std::fs::write(&custom[1].path, "Unselected").unwrap();
        let (_, selected) = resolve_selection(&[custom[0].id.clone()], &custom).unwrap();
        assert_eq!(
            selected[0].import(&source, "Shared policy").outcome,
            Outcome::Written
        );
        assert_eq!(
            std::fs::read_to_string(qs_core::paths::backup_path(&custom[0].path)).unwrap(),
            "My preferences"
        );
        assert!(selected[0]
            .import(&source, "Shared policy")
            .detail
            .starts_with("Up to date"));
        assert_eq!(
            std::fs::read_to_string(&custom[1].path).unwrap(),
            "Unselected"
        );
        assert_eq!(
            recipient(source.clone()).import(&source, "rules").outcome,
            Outcome::Failed
        );
        let missing = recipient(temp.0.join("missing/AGENTS.md"));
        assert!(!missing.scan().installed);
        assert_eq!(missing.import(&source, "rules").outcome, Outcome::Failed);
        assert!(!missing.path.exists());
    }
}
