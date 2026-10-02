//! Per-workspace module settings + the crate's one door to the workspace
//! registry (PLAN-WORKSPACE-UNIFY).
//!
//! The old ProjectRegistry (projects.json) carried both the project list and
//! its per-project config. The list is now the core.db workspace registry
//! (`qs_core::workspaces`); the config lives in core.db `settings` under scope
//! `mcp`, keyed by the workspace id's b36 tail:
//!
//!   mcp / workspace.<b36>.approval_mode   "auto_apply" | "approval"
//!   mcp / workspace.<b36>.index           { mode, provider, model, baseUrl, filter }
//!
//! Not in the workspace entity payload on purpose: `toEntity()` in
//! `packages/core/src/workspaces.ts` rewrites the payload wholesale on every
//! open, so anything a module parks there is clobbered.

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::Manager;

/// Run a closure against the locked core.db connection. Errs while the app is
/// still starting (or in unit tests) — callers surface that as a tool error.
pub fn with_core_db<T>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
) -> Result<T, String> {
    let db = app
        .try_state::<qs_core::db::Db>()
        .ok_or("core.db not ready")?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    f(&conn)
}

/// Resolve the workspace a tool call means. Reads `workspace` first, then the
/// legacy aliases (`project_name` from the kanban tools, `codebase` from the
/// index tools) so pre-unify clients keep working; nothing given means the
/// active workspace.
pub fn resolve_workspace_arg(
    app: &tauri::AppHandle,
    arguments: &serde_json::Value,
) -> Result<qs_core::workspaces::WorkspaceEntry, String> {
    let ident = board_ident(arguments);
    with_core_db(app, |conn| qs_core::workspaces::resolve(conn, ident))
}

fn board_ident(arguments: &serde_json::Value) -> Option<&str> {
    arguments
        .get("workspace")
        .and_then(|v| v.as_str())
        .or_else(|| arguments.get("project_name").and_then(|v| v.as_str()))
        .or_else(|| arguments.get("codebase").and_then(|v| v.as_str()))
}

/// A kanban board identity (PLAN-KANBAN-UNIFY): either the suite-wide
/// General board or a registered workspace.
pub enum Board {
    General,
    Workspace(qs_core::workspaces::WorkspaceEntry),
}

impl Board {
    pub fn id(&self) -> &str {
        match self {
            Board::General => crate::kanban_db::GENERAL_BOARD_ID,
            Board::Workspace(ws) => &ws.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Board::General => "General",
            Board::Workspace(ws) => &ws.name,
        }
    }

    /// The per-board settings key tail (`workspace.<key>.…` in core.db) —
    /// the b36 id for a workspace, the sentinel itself for General.
    pub fn settings_key(&self) -> &str {
        match self {
            Board::General => crate::kanban_db::GENERAL_BOARD_ID,
            Board::Workspace(ws) => ws.b36(),
        }
    }
}

/// Resolve the board a kanban tool call means: `"general"` is the General
/// board, everything else (and the active-workspace default) resolves
/// through the registry like `resolve_workspace_arg`.
pub fn resolve_board_arg(
    app: &tauri::AppHandle,
    arguments: &serde_json::Value,
) -> Result<Board, String> {
    let ident = board_ident(arguments);
    if ident.is_some_and(|s| s.eq_ignore_ascii_case(crate::kanban_db::GENERAL_BOARD_ID)) {
        return Ok(Board::General);
    }
    with_core_db(app, |conn| qs_core::workspaces::resolve(conn, ident)).map(Board::Workspace)
}

/// Register a folder as a workspace if it isn't one yet, and return its entry.
///
/// `index_codebase` on an unregistered folder lands here (PLAN-WORKSPACE-UNIFY
/// D6): the id is path-derived (`workspace_id_for`), so this is idempotent and
/// converges with what the webview would register for the same folder. It does
/// NOT touch `core / workspace.active` — indexing a folder is not opening it.
pub fn ensure_registered_workspace(
    app: &tauri::AppHandle,
    path: &str,
) -> Result<qs_core::workspaces::WorkspaceEntry, String> {
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string())
        .unwrap_or_else(|_| path.to_string());
    if !std::path::Path::new(&canonical).is_dir() {
        return Err(format!("Not a directory: {canonical}"));
    }
    let id = qs_core::workspaces::workspace_id_for(&canonical);
    if let Ok(existing) = with_core_db(app, |conn| {
        qs_core::workspaces::by_id(conn, &id).ok_or_else(|| "not registered".into())
    }) {
        return Ok(existing);
    }
    let name = std::path::Path::new(&canonical)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Workspace")
        .to_string();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    // Same shape `toEntity()` writes, so the webview upserts over it cleanly.
    qs_core::runtime::upsert_entity(qs_core::db::Entity {
        id: id.clone(),
        module: "core".into(),
        kind: "workspace".into(),
        title: name.clone(),
        subtitle: Some(canonical.clone()),
        route: "/canvas".into(),
        icon: None,
        updated_at: now_ms,
        payload: Some(json!({ "path": canonical, "pinned": false, "lastOpenedAt": now_ms })),
    })?;
    Ok(qs_core::workspaces::WorkspaceEntry { id, name, path: canonical })
}

// ── Approval mode ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ApprovalMode {
    #[serde(rename = "auto_apply")]
    #[default]
    AutoApply,
    #[serde(rename = "approval")]
    Approval,
}

impl ApprovalMode {
    pub fn as_str(&self) -> &str {
        match self {
            ApprovalMode::AutoApply => "auto_apply",
            ApprovalMode::Approval => "approval",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "auto_apply" | "auto-apply" | "autoapply" => Ok(ApprovalMode::AutoApply),
            "approval" | "review" => Ok(ApprovalMode::Approval),
            _ => Err(format!(
                "Invalid approval mode '{}'. Use 'auto_apply' or 'approval'.",
                s
            )),
        }
    }
}

pub fn get_approval_mode(app: &tauri::AppHandle, b36: &str) -> ApprovalMode {
    get_approval_mode_checked(app, b36).unwrap_or_default()
}

/// Mutations must not silently become Auto Apply when settings cannot be read.
pub fn get_approval_mode_checked(app: &tauri::AppHandle, b36: &str) -> Result<ApprovalMode, String> {
    with_core_db(app, |conn| {
        qs_core::db::get_setting(conn, "mcp", &format!("workspace.{b36}.approval_mode"))
            .map_err(|e| e.to_string())
    })?.map(|value| {
        ApprovalMode::from_str(value.as_str().ok_or("Invalid board approval mode")?)
    }).unwrap_or(Ok(ApprovalMode::AutoApply))
}

pub fn set_approval_mode(
    app: &tauri::AppHandle,
    b36: &str,
    mode: &ApprovalMode,
) -> Result<(), String> {
    with_core_db(app, |conn| {
        qs_core::db::set_setting(
            conn,
            "mcp",
            &format!("workspace.{b36}.approval_mode"),
            &json!(mode.as_str()),
        )
        .map_err(|e| e.to_string())
    })
}

// ── Index settings ────────────────────────────────────────────────────────

/// What the projects.json rows used to remember per project: how this
/// workspace's code index is built. All optional — the CLI has its own
/// defaults (structural). Legacy provider fields are ignored when read.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexSettings {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    /// Switched off on the workspace's index card: nothing indexes or
    /// searches that half of the index until it is switched on again. The
    /// data stays.
    #[serde(default)]
    pub structural_off: bool,
    #[serde(default)]
    pub semantic_off: bool,
    /// Local embeddings are switched off suite-wide (Memory settings): the
    /// semantic half is off in every workspace, whatever it chose. Read from
    /// the engine, never stored.
    #[serde(skip)]
    pub embeddings_off: bool,
}

impl IndexSettings {
    fn semantic_on(&self) -> bool {
        !self.semantic_off && !self.embeddings_off
    }

    fn off_error(&self, half: &str, workspace: &str) -> String {
        if half == "semantic" && !self.semantic_off && self.embeddings_off {
            return format!(
                "Local embeddings are switched off for the whole suite (QuantMemory → Settings → Local semantic recall), so the semantic code index of workspace '{workspace}' cannot run."
            );
        }
        format!(
            "The {half} code index of workspace '{workspace}' is switched off (QuantMCP → Projects → Codebase Indexing)."
        )
    }

    /// The part of an index `mode` (`structural`, `semantic` or `both`) this
    /// workspace has switched on, or why nothing of it may run.
    pub fn allowed_mode(&self, mode: &str, workspace: &str) -> Result<String, String> {
        let structural = matches!(mode, "structural" | "both");
        let semantic = matches!(mode, "semantic" | "both");
        if !structural && !semantic {
            return Ok(mode.to_string());
        }
        match (structural && !self.structural_off, semantic && self.semantic_on()) {
            (true, true) => Ok("both".into()),
            (true, false) => Ok("structural".into()),
            (false, true) => Ok("semantic".into()),
            (false, false) if structural && semantic => Err(format!(
                "{} {}",
                self.off_error("structural", workspace),
                self.off_error("semantic", workspace)
            )),
            (false, false) => Err(self.off_error(if structural { "structural" } else { "semantic" }, workspace)),
        }
    }

    /// The search mode to run: `auto` chooses among the switched-on indexes
    /// only, an explicit mode must be switched on.
    pub fn search_mode(&self, requested: &str, workspace: &str) -> Result<String, String> {
        match requested {
            "auto" => self.allowed_mode("both", workspace).map(|mode| match mode.as_str() {
                "both" => "auto".into(),
                _ => mode,
            }),
            "structural" | "semantic" => self.allowed_mode(requested, workspace),
            _ => Ok(requested.to_string()),
        }
    }
}

/// The settings a code index run follows: the workspace's own, with the
/// suite-wide embeddings switch applied.
pub fn get_effective_index_settings(app: &tauri::AppHandle, b36: &str) -> IndexSettings {
    IndexSettings {
        embeddings_off: qs_core::embeddings::info().off,
        ..get_index_settings(app, b36)
    }
}

pub fn get_index_settings(app: &tauri::AppHandle, b36: &str) -> IndexSettings {
    with_core_db(app, |conn| {
        qs_core::db::get_setting(conn, "mcp", &format!("workspace.{b36}.index"))
            .map_err(|e| e.to_string())
    })
    .ok()
    .flatten()
    .and_then(|v| serde_json::from_value(v).ok())
    .unwrap_or_default()
}

pub fn set_index_settings(
    app: &tauri::AppHandle,
    b36: &str,
    settings: &IndexSettings,
) -> Result<(), String> {
    let value = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    with_core_db(app, |conn| {
        qs_core::db::set_setting(conn, "mcp", &format!("workspace.{b36}.index"), &value)
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::IndexSettings;

    fn switched(structural_off: bool, semantic_off: bool) -> IndexSettings {
        IndexSettings { structural_off, semantic_off, ..Default::default() }
    }

    #[test]
    fn older_settings_read_as_both_halves_on_and_keep_their_switches() {
        let old: IndexSettings = serde_json::from_value(serde_json::json!({"mode": "both", "filter": "smart"})).unwrap();
        assert!(!old.structural_off && !old.semantic_off);
        let saved = serde_json::to_value(switched(false, true)).unwrap();
        assert_eq!(saved["semanticOff"], true);
        assert!(serde_json::from_value::<IndexSettings>(saved).unwrap().semantic_off);
    }

    #[test]
    fn index_runs_only_the_switched_on_halves() {
        let on = switched(false, false);
        assert_eq!(on.allowed_mode("both", "w").unwrap(), "both");
        assert_eq!(on.allowed_mode("semantic", "w").unwrap(), "semantic");
        let no_semantic = switched(false, true);
        assert_eq!(no_semantic.allowed_mode("both", "w").unwrap(), "structural");
        assert!(no_semantic.allowed_mode("semantic", "w").unwrap_err().contains("semantic code index of workspace 'w' is switched off"));
        let no_structural = switched(true, false);
        assert_eq!(no_structural.allowed_mode("both", "w").unwrap(), "semantic");
        assert!(no_structural.allowed_mode("structural", "w").is_err());
        let both_off = switched(true, true).allowed_mode("both", "w").unwrap_err();
        assert!(both_off.contains("structural code index") && both_off.contains("semantic code index"));
    }

    #[test]
    fn embeddings_off_suite_wide_turns_semantic_off_without_touching_the_workspace_choice() {
        let off = IndexSettings { embeddings_off: true, ..switched(false, false) };
        assert_eq!(off.allowed_mode("both", "w").unwrap(), "structural");
        assert_eq!(off.search_mode("auto", "w").unwrap(), "structural");
        let error = off.search_mode("semantic", "w").unwrap_err();
        assert!(error.contains("switched off for the whole suite"), "{error}");
        // Never stored: switching embeddings back on restores the workspace's own choice.
        let stored = serde_json::to_value(&off).unwrap();
        assert!(stored.get("embeddingsOff").is_none() && stored["semanticOff"] == false);
    }

    #[test]
    fn auto_search_picks_among_switched_on_indexes_only() {
        assert_eq!(switched(false, false).search_mode("auto", "w").unwrap(), "auto");
        assert_eq!(switched(false, true).search_mode("auto", "w").unwrap(), "structural");
        assert_eq!(switched(true, false).search_mode("auto", "w").unwrap(), "semantic");
        assert!(switched(true, true).search_mode("auto", "w").is_err());
        assert!(switched(false, true).search_mode("semantic", "w").is_err());
        assert_eq!(switched(false, true).search_mode("structural", "w").unwrap(), "structural");
    }
}
