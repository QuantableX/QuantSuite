//! Persisted app switches shared by the shell, MCP discovery and the agent gate.
use crate::db::{self, Db};
use qs_mcp_bridge::AppAvailability;
use tauri::{AppHandle, Manager, Runtime};

pub fn read<R: Runtime>(app: &AppHandle<R>) -> Result<AppAvailability, String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let settings = db::get_scope(&conn, "core").map_err(|e| e.to_string())?;
    Ok(AppAvailability::from_settings(&settings))
}

pub fn require_module<R: Runtime>(app: &AppHandle<R>, module: &str) -> Result<(), String> {
    if read(app)?.module_enabled(module) {
        Ok(())
    } else {
        Err(format!("The app containing '{module}' is deactivated in Settings > Apps"))
    }
}
