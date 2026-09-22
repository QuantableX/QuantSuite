//! Per-workspace state (approval mode, index settings) and the codebase
//! indexer. The folder list itself is the core.db workspace registry, read
//! via @quantsuite/core in the webview.

use crate::indexing::{self, CodebaseIndexStatus, IndexCodebaseResult};
use crate::settings;

// â"€â"€ Projects â"€â"€

/// The id's b36 tail — the settings key for per-workspace state. Accepts the
/// full entity id so the frontend can pass workspace.id verbatim.
fn workspace_b36(workspace_id: &str) -> &str {
    workspace_id.rsplit(':').next().unwrap_or(workspace_id)
}

#[tauri::command(async)]
pub(crate) fn get_approval_mode(app: tauri::AppHandle, workspace_id: String) -> String {
    settings::get_approval_mode(&app, workspace_b36(&workspace_id))
        .as_str()
        .to_string()
}

#[tauri::command(async)]
pub(crate) fn set_approval_mode(app: tauri::AppHandle, workspace_id: String, mode: String) -> Result<String, String> {
    let approval_mode = settings::ApprovalMode::from_str(&mode)?;
    settings::set_approval_mode(&app, workspace_b36(&workspace_id), &approval_mode)?;
    Ok(approval_mode.as_str().to_string())
}

#[tauri::command(async)]
pub(crate) fn get_workspace_index_settings(
    app: tauri::AppHandle,
    workspace_id: String,
) -> settings::IndexSettings {
    settings::get_index_settings(&app, workspace_b36(&workspace_id))
}

#[tauri::command(async)]
pub(crate) fn set_workspace_index_settings(
    app: tauri::AppHandle,
    workspace_id: String,
    index_settings: settings::IndexSettings,
) -> Result<(), String> {
    settings::set_index_settings(&app, workspace_b36(&workspace_id), &index_settings)
}

// â"€â"€ Codebase Indexing â"€â"€

#[tauri::command]
pub(crate) async fn index_project_codebase(
    app: tauri::AppHandle,
    workspace_id: String,
    mode: String,
    embed_provider: Option<String>,
    embed_model: Option<String>,
    embed_base_url: Option<String>,
    force_reindex: Option<bool>,
    filter_mode: Option<String>,
) -> Result<IndexCodebaseResult, String> {
    let ws = settings::with_core_db(&app, |conn| {
        qs_core::workspaces::by_id(conn, &workspace_id)
            .ok_or_else(|| format!("Workspace '{}' not registered", workspace_id))
    })?;

    let mut args = if force_reindex.unwrap_or(false) {
        vec![
            "reindex".to_string(),
            "--codebase".to_string(),
            ws.b36().to_string(),
            "--mode".to_string(),
            mode.clone(),
        ]
    } else {
        vec![
            "index".to_string(),
            ws.path.clone(),
            "--name".to_string(),
            ws.b36().to_string(),
            "--mode".to_string(),
            mode.clone(),
        ]
    };
    if let Some(ref provider) = embed_provider {
        args.push("--provider".to_string());
        args.push(provider.clone());
    }
    if let Some(ref model) = embed_model {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    if let Some(ref url) = embed_base_url {
        args.push("--base-url".to_string());
        args.push(url.clone());
    }
    if let Some(ref fm) = filter_mode {
        args.push("--filter".to_string());
        args.push(fm.clone());
    }

    let payload = indexing::run_cli(&app, args).await?;
    let result: IndexCodebaseResult = serde_json::from_value(payload)
        .map_err(|e| format!("Failed to parse indexer output: {}", e))?;

    // Persist what worked, so the UI and the MCP tools agree on defaults.
    if result.error.is_none() {
        let _ = settings::set_index_settings(
            &app,
            ws.b36(),
            &settings::IndexSettings {
                mode: Some(mode),
                provider: embed_provider,
                model: embed_model,
                base_url: embed_base_url,
                filter: filter_mode,
            },
        );
    }

    Ok(result)
}

#[tauri::command]
pub(crate) async fn get_codebase_index_stats(
    app: tauri::AppHandle,
    workspace_id: String,
) -> Result<CodebaseIndexStatus, String> {
    let b36 = workspace_b36(&workspace_id).to_string();

    let stats = match indexing::run_cli(
        &app,
        vec!["stats".to_string(), "--codebase".to_string(), b36],
    )
    .await
    {
        Ok(stats) => stats,
        // "not found" and a missing CLI both read as not indexed in the UI.
        Err(_) => return Ok(CodebaseIndexStatus::not_indexed(workspace_id)),
    };

    let last_indexed = stats["last_indexed"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok());
    let structural_indexed_at = stats["structural_indexed_at"]
        .as_str()
        .and_then(|s| if s.is_empty() { None } else { s.parse::<u64>().ok() });
    let semantic_indexed_at = stats["semantic_indexed_at"]
        .as_str()
        .and_then(|s| if s.is_empty() { None } else { s.parse::<u64>().ok() });

    Ok(CodebaseIndexStatus {
        workspace_id,
        status: "indexed".to_string(),
        file_count: stats["file_count"].as_u64().unwrap_or(0) as u32,
        indexed_at: last_indexed,
        mode: stats["mode"].as_str().map(|s| s.to_string()),
        fts_entry_count: stats["fts_entry_count"].as_u64().map(|n| n as u32),
        chunk_count: stats["chunk_count"].as_u64().map(|n| n as u32),
        structural_indexed_at,
        semantic_indexed_at,
    })
}
