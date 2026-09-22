//! AGENT.md management for the AgentOS pages: the global file under
//! `~/.quantmcp` and the per-workspace one. The templates and the merge the
//! MCP tools serve live in `mcp_server`.

use crate::mcp_server;

// ── AGENT.md Management ──

/// Overwrite a scope's AGENT.md with the template the suite ships — the
/// editor's "Reset to default". Unlike `init_agent_md` it replaces an
/// existing file (the UI confirms first) and returns the new content.
#[tauri::command]
pub(crate) async fn reset_agent_md(scope: String, project_path: Option<String>) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let (path, content) = match scope.as_str() {
        "global" => (
            home.join(".quantmcp").join("AGENT.md"),
            mcp_server::default_global_agent_md_template(),
        ),
        "project" => {
            let p = project_path
                .as_ref()
                .ok_or("project_path required for project scope")?;
            let proj_name = std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            (
                std::path::PathBuf::from(p).join("AGENT.md"),
                mcp_server::default_project_agent_md_template(proj_name),
            )
        }
        _ => return Err(format!("Invalid scope: {}", scope)),
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    tokio::fs::write(&path, &content)
        .await
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(content)
}

#[derive(serde::Serialize)]
pub(crate) struct AgentMdPaths {
    global: String,
    project: Option<String>,
    global_exists: bool,
    project_exists: bool,
}

#[tauri::command(async)]
pub(crate) fn get_agent_md_paths(project_path: Option<String>) -> AgentMdPaths {
    let home = dirs::home_dir().unwrap_or_default();
    let global_path = home.join(".quantmcp").join("AGENT.md");

    let project_md_path = project_path.as_ref().map(|p| std::path::PathBuf::from(p).join("AGENT.md"));

    AgentMdPaths {
        global: global_path.to_string_lossy().to_string(),
        project: project_md_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        global_exists: global_path.exists(),
        project_exists: project_md_path.as_ref().is_some_and(|p| p.exists()),
    }
}

#[tauri::command]
pub(crate) async fn read_agent_md(scope: String, project_path: Option<String>) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let path = match scope.as_str() {
        "global" => home.join(".quantmcp").join("AGENT.md"),
        "project" => {
            let p = project_path.ok_or("project_path required for project scope")?;
            std::path::PathBuf::from(p).join("AGENT.md")
        }
        _ => return Err(format!("Invalid scope: {}", scope)),
    };

    if path.exists() {
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    } else {
        Ok(String::new())
    }
}

#[tauri::command]
pub(crate) async fn write_agent_md(
    scope: String,
    content: String,
    project_path: Option<String>,
) -> Result<(), String> {
    let home = dirs::home_dir().unwrap_or_default();
    let path = match scope.as_str() {
        "global" => home.join(".quantmcp").join("AGENT.md"),
        "project" => {
            let p = project_path.ok_or("project_path required for project scope")?;
            std::path::PathBuf::from(p).join("AGENT.md")
        }
        _ => return Err(format!("Invalid scope: {}", scope)),
    };

    // Create parent dirs if needed
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

#[tauri::command]
pub(crate) async fn init_agent_md(scope: String, project_path: Option<String>) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let (path, default_content) = match scope.as_str() {
        "global" => (
            home.join(".quantmcp").join("AGENT.md"),
            mcp_server::default_global_agent_md_template(),
        ),
        "project" => {
            let p = project_path
                .as_ref()
                .ok_or("project_path required for project scope")?;
            let proj_name = std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project");
            (
                std::path::PathBuf::from(p).join("AGENT.md"),
                mcp_server::default_project_agent_md_template(proj_name),
            )
        }
        _ => return Err(format!("Invalid scope: {}", scope)),
    };

    if path.exists() {
        return Ok("already_exists".to_string());
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    tokio::fs::write(&path, default_content)
        .await
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok("created".to_string())
}

// ── MEMORY.md Management ── (removed 2026-08-31: persistent memory moved to
// QuantMemory's vault — modules/memory. The old ~/.quantmcp/MEMORY.md files
// stay on disk untouched; QuantMemory imported the global one on first run.)

// ── CONCEPT.md Management ── (removed 2026-08-31: AgentOS is AGENT.md only.
// Existing CONCEPT.md files stay on disk untouched.)
