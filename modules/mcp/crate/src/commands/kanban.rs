//! The kanban board commands for the drawer and the module's board page:
//! kanban.db is the one store, one board per workspace plus General.

use crate::kanban_db::{self, KanbanCard, KanbanColumn};
use crate::mcp_server;
use crate::settings;
use crate::AppState;
use std::str::FromStr;

#[tauri::command(async)]
pub(crate) fn list_kanban_cards(state: tauri::State<'_, AppState>, workspace_id: String) -> Result<Vec<KanbanCard>, String> {
    state.kanban_db.list_cards(&workspace_id)
}

// ── Archive ──
//
// The board page's "Archive all" on the Done column and its archive modal.
// Archived cards stay in kanban.db (agents list them with
// `list_kanban_cards archived=true`); they only leave the board.

#[tauri::command(async)]
pub(crate) fn list_archived_kanban_cards(state: tauri::State<'_, AppState>, workspace_id: String) -> Result<Vec<KanbanCard>, String> {
    state.kanban_db.list_archived_cards(&workspace_id)
}

/// Archive one card; the database rejects cards outside Done.
#[tauri::command(async)]
pub(crate) fn archive_kanban_card(state: tauri::State<'_, AppState>, id: String) -> Result<KanbanCard, String> {
    state.kanban_db.archive_card(&id)
}

/// Archive every card in the board's Done column; returns how many moved.
#[tauri::command(async)]
pub(crate) fn archive_done_kanban_cards(state: tauri::State<'_, AppState>, workspace_id: String) -> Result<usize, String> {
    Ok(state.kanban_db.archive_done_cards(&workspace_id)?.len())
}

#[tauri::command(async)]
pub(crate) fn unarchive_kanban_card(state: tauri::State<'_, AppState>, id: String) -> Result<KanbanCard, String> {
    state.kanban_db.unarchive_card(&id)
}

#[tauri::command(async)]
pub(crate) fn add_kanban_card(
    state: tauri::State<'_, AppState>,
    workspace_id: String,
    title: String,
    description: String,
    column: KanbanColumn,
    priority: Option<String>,
    blocked_by: Option<String>,
) -> Result<KanbanCard, String> {
    let pri = priority
        .as_deref()
        .map(kanban_db::CardPriority::from_str)
        .transpose()?
        .unwrap_or(kanban_db::CardPriority::Medium);
    let deps: Vec<String> = blocked_by
        .as_deref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();
    let card_id = uuid::Uuid::new_v4().to_string();
    state.kanban_db.add_card(&card_id, &workspace_id, &title, &description, &column, &pri, &deps)
}

#[tauri::command(async)]
pub(crate) fn update_kanban_card(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
    description: String,
    priority: Option<String>,
    blocked_by: Option<String>,
) -> Result<KanbanCard, String> {
    let pri = priority
        .as_deref()
        .map(kanban_db::CardPriority::from_str)
        .transpose()?;
    let deps: Option<Vec<String>> = blocked_by.as_deref().map(|s| {
        if s.is_empty() { vec![] }
        else { s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect() }
    });
    state.kanban_db.update_card(&id, &title, &description, pri.as_ref(), deps.as_deref())
}

#[tauri::command(async)]
pub(crate) fn move_kanban_card(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    column: KanbanColumn,
    before_id: Option<String>,
) -> Result<(), String> {
    let card = state.kanban_db.get_card(&id)?.ok_or("Card not found")?;
    if card.workspace_id != kanban_db::GENERAL_BOARD_ID && card.column != column
        && mcp_server::approval_mode_for_card(&app, &card)? == settings::ApprovalMode::Approval
    {
        return Err("Use 'Approve work' or 'Approve result' in the card dialog; dragging cannot skip approval stages.".into());
    }
    state.kanban_db.move_card(&id, &column, before_id.as_deref())
}

/// Human-only actions from the shared card dialog. Not exposed as MCP tools.
#[tauri::command(async)]
pub(crate) fn review_kanban_card(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    action: String,
    reason: Option<String>,
) -> Result<String, String> {
    match action.as_str() {
        "start" => {
            state.kanban_db.approve_start(&id)?;
            Ok("Work approved. The agent may now claim this card.".into())
        }
        "approve" => mcp_server::approve_card_from_ui(&app, &state.kanban_db, &id),
        "reject" => {
            let reason = reason.as_deref().unwrap_or("").trim();
            if reason.is_empty() { return Err("Describe the changes you want.".into()); }
            state.kanban_db.reject_card(&id, reason)?;
            Ok("Changes requested. The worktree is preserved.".into())
        }
        _ => Err("Unknown card review action".into()),
    }
}

/// Deleting a claimed card removes its worktree and branch first — the same
/// path as the MCP tool (`mcp_server::delete_card_with_cleanup`). The UI has
/// no line for the cleanup notes, so a warning goes to the log instead.
#[tauri::command(async)]
pub(crate) fn delete_kanban_card(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let note = mcp_server::delete_card_with_cleanup(&app, &state.kanban_db, &id)?;
    if note.contains("WARNING") {
        eprintln!("mcp: delete_kanban_card {id}:{note}");
    }
    Ok(())
}

/// Re-home a card onto another board — `workspace_id` is a workspace entity
/// id or the General sentinel. The drawer's hand-over uses this.
#[tauri::command(async)]
pub(crate) fn move_kanban_card_to_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    workspace_id: String,
) -> Result<KanbanCard, String> {
    if workspace_id != kanban_db::GENERAL_BOARD_ID {
        settings::with_core_db(&app, |conn| {
            qs_core::workspaces::by_id(conn, &workspace_id)
                .ok_or_else(|| format!("Workspace '{}' not registered", workspace_id))
        })?;
    }
    state.kanban_db.move_card_to_board(&id, &workspace_id)
}
