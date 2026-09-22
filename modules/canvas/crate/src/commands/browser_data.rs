use std::fs;
use std::path::PathBuf;

/// The module's own data directory, `~/.quantsuite/modules/canvas/`.
///
/// Was `~/.quantcode/` in the standalone app; the plugin's setup imports that
/// on first launch. This is the global browser store. Per-workspace layouts,
/// notes and specs use private workspace storage (commands::workspace).
fn quantcode_home() -> Result<PathBuf, String> {
    Ok(qs_core::paths::module_dir("canvas"))
}

// ---------------------------------------------------------------------------
// load_browser_data
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn load_browser_data() -> Result<String, String> {
    let dir = quantcode_home()?;
    let file = dir.join("browser-data.json");

    if !file.exists() {
        return Ok(r#"{"history":[],"bookmarks":[]}"#.to_string());
    }

    fs::read_to_string(&file)
        .map_err(|e| format!("Failed to read browser-data.json: {}", e))
}

// ---------------------------------------------------------------------------
// save_browser_data
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_browser_data(data: String) -> Result<(), String> {
    let dir = quantcode_home()?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create browser data directory: {}", e))?;
    }

    let file = dir.join("browser-data.json");
    fs::write(&file, data)
        .map_err(|e| format!("Failed to write browser-data.json: {}", e))
}
