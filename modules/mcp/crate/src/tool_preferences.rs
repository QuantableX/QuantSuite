//! Installation-wide MCP exposure. Group gates preserve individual choices.
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{config_gen, settings};

pub const SETTINGS_KEY: &str = "tool_preferences";

fn builtin_groups() -> &'static HashMap<String, String> {
    static GROUPS: OnceLock<HashMap<String, String>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        let mut groups = HashMap::new();
        for (group, tools) in [
            ("codebase", config_gen::codebase_index_tools()),
            ("agentos", config_gen::agentos_tools()),
            ("kanban", config_gen::kanban_tools()),
            ("worktree", config_gen::worktree_tools()),
        ] {
            for tool in tools {
                groups.insert(tool.name, group.into());
            }
        }
        for tool in qs_mcp_bridge::capabilities() {
            groups.insert(tool.tool.clone(), format!("suite.{}", tool.module));
        }
        groups
    })
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolPreferences {
    pub disabled_groups: BTreeSet<String>,
    pub disabled_tools: BTreeSet<String>,
}

impl ToolPreferences {
    pub fn group_enabled(&self, group: &str) -> bool {
        !self.disabled_groups.contains(group)
            && (!group.starts_with("suite.") || !self.disabled_groups.contains("suite"))
    }

    pub fn tool_enabled(&self, name: &str, native: bool) -> bool {
        let group = builtin_groups()
            .get(name)
            .map(String::as_str)
            .unwrap_or(if native { "native" } else { "custom" });
        self.group_enabled(group) && !self.disabled_tools.contains(name)
    }

    fn set(&mut self, kind: &str, name: &str, enabled: bool) -> Result<(), String> {
        let values = match kind {
            "group"
                if matches!(
                    name,
                    "codebase" | "agentos" | "kanban" | "worktree" | "suite" | "native" | "custom"
                ) || builtin_groups().values().any(|group| group == name) =>
            {
                &mut self.disabled_groups
            }
            "tool" if builtin_groups().contains_key(name) => &mut self.disabled_tools,
            _ => return Err(format!("Unknown tool preference: {kind} '{name}'")),
        };
        if enabled {
            values.remove(name);
        } else {
            values.insert(name.into());
        }
        Ok(())
    }
}

pub fn read_from(conn: &Connection) -> Result<ToolPreferences, String> {
    // Do not treat corrupt saved preferences as permission to expose everything.
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE scope = 'mcp' AND key = ?1",
            [SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|e| format!("Invalid tool preferences: {e}"))
    })
    .transpose()
    .map(Option::unwrap_or_default)
}

pub fn read(app: &tauri::AppHandle) -> Result<ToolPreferences, String> {
    settings::with_core_db(app, read_from)
}

fn write_to(
    conn: &Connection,
    kind: &str,
    name: &str,
    enabled: bool,
) -> Result<ToolPreferences, String> {
    let mut prefs = read_from(conn)?;
    prefs.set(kind, name, enabled)?;
    qs_core::db::set_setting(
        conn,
        "mcp",
        SETTINGS_KEY,
        &serde_json::to_value(&prefs).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(prefs)
}

pub fn set(
    app: &tauri::AppHandle,
    kind: &str,
    name: &str,
    enabled: bool,
) -> Result<ToolPreferences, String> {
    // Read-modify-write under one DB lock: concurrent windows retain each other's choices.
    let prefs = settings::with_core_db(app, |conn| write_to(conn, kind, name, enabled))?;
    let _ = qs_core::bus::emit(
        app,
        "core.setting.changed",
        serde_json::json!({
            "scope": "mcp", "key": SETTINGS_KEY, "value": prefs,
        }),
    );
    Ok(prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE settings(scope TEXT, key TEXT, value TEXT, PRIMARY KEY(scope, key))",
        )
        .unwrap();
        conn
    }

    #[test]
    fn all_catalogued_tools_have_a_group_and_default_to_enabled() {
        let prefs = ToolPreferences::default();
        assert!(!builtin_groups().is_empty());
        for (tool, group) in builtin_groups().iter() {
            assert!(prefs.tool_enabled(tool, false), "{tool}");
            let mut disabled = prefs.clone();
            disabled.set("group", group, false).unwrap();
            assert!(!disabled.tool_enabled(tool, false), "{tool} in {group}");
        }
    }

    #[test]
    fn group_round_trip_preserves_individual_and_nested_choices() {
        let conn = db();
        write_to(&conn, "tool", "list_kanban_cards", false).unwrap();
        let prefs = write_to(&conn, "group", "kanban", false).unwrap();
        assert!(!prefs.tool_enabled("create_kanban_card", false));
        let prefs = write_to(&conn, "group", "kanban", true).unwrap();
        assert!(!prefs.tool_enabled("list_kanban_cards", false));
        assert!(prefs.tool_enabled("create_kanban_card", false));
        write_to(&conn, "group", "suite.memory", false).unwrap();
        write_to(&conn, "group", "suite", false).unwrap();
        let prefs = write_to(&conn, "group", "suite.memory", true).unwrap();
        assert!(!prefs.tool_enabled("quantsuite.memory.search", false));
        let prefs = write_to(&conn, "group", "suite", true).unwrap();
        assert!(prefs.tool_enabled("quantsuite.memory.search", false));
        assert_eq!(read_from(&conn).unwrap(), prefs);
    }

    #[test]
    fn native_and_custom_groups_are_independent() {
        let mut prefs = ToolPreferences::default();
        prefs.set("group", "native", false).unwrap();
        assert!(!prefs.tool_enabled("native_example", true));
        assert!(prefs.tool_enabled("custom_example", false));
        assert!(prefs.tool_enabled("search_code", false));
        prefs.set("group", "custom", false).unwrap();
        assert!(!prefs.tool_enabled("custom_example", false));
    }

    #[test]
    fn preferences_survive_closing_and_reopening_the_database() {
        let path =
            std::env::temp_dir().join(format!("qs-tool-settings-{}.db", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE settings(scope TEXT, key TEXT, value TEXT, PRIMARY KEY(scope, key))",
            )
            .unwrap();
            write_to(&conn, "group", "kanban", false).unwrap();
            write_to(&conn, "tool", "quantsuite.memory.search", false).unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            let prefs = read_from(&conn).unwrap();
            assert!(!prefs.tool_enabled("list_kanban_cards", false));
            assert!(!prefs.tool_enabled("quantsuite.memory.search", false));
            assert!(prefs.tool_enabled("quantsuite.memory.read", false));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_changes_and_failed_writes_preserve_saved_state() {
        let conn = db();
        let prefs = write_to(&conn, "group", "worktree", false).unwrap();
        assert!(write_to(&conn, "group", "suite.typo", false).is_err());
        assert!(write_to(&conn, "tool", "unknown_tool", false).is_err());
        assert_eq!(read_from(&conn).unwrap(), prefs);
        conn.execute_batch("PRAGMA query_only = ON").unwrap();
        assert!(write_to(&conn, "group", "worktree", true).is_err());
        assert_eq!(read_from(&conn).unwrap(), prefs);
    }

    #[test]
    fn malformed_settings_fail_closed() {
        let conn = db();
        assert_eq!(read_from(&conn).unwrap(), ToolPreferences::default());
        conn.execute(
            "INSERT INTO settings VALUES ('mcp', ?1, 'broken')",
            [SETTINGS_KEY],
        )
        .unwrap();
        assert!(read_from(&conn).is_err());
        assert!(write_to(&conn, "group", "kanban", true).is_err());
    }
}
