//! The same private research service backs the app and the MCP capabilities.
use serde_json::{json, Value};
use std::io::Write;
use std::process::Stdio;
use tauri::{AppHandle, Manager};

async fn query(app: AppHandle, request: Value) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::ScriptState>();
        let sidecar = state.sidecar_dir.as_ref().ok_or("Python sidecar is unavailable")?;
        let (python, info) = crate::python(&state, false)?;
        if !info.ok {
            return Err(info.error.unwrap_or_else(|| "Python dependencies unavailable".into()));
        }
        let mut child = python.command(sidecar)
            .args(["-m", "smithery.lab", "query"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|e| format!("Research: {e}"))?;
        let input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&input).map_err(|e| e.to_string())?;
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        let doc = String::from_utf8_lossy(&output.stdout).lines().rev()
            .find_map(|line| serde_json::from_str::<Value>(line).ok());
        match doc {
            Some(value) if output.status.success() => Ok(value),
            Some(value) => Err(value["error"].as_str().unwrap_or("Research query failed").to_string()),
            None => Err(format!("Research: {}", String::from_utf8_lossy(&output.stderr))),
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_context(app_handle: AppHandle) -> Result<Value, String> {
    query(app_handle, json!({"action":"context"})).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_list(app_handle: AppHandle) -> Result<Value, String> {
    query(app_handle, json!({"action":"list"})).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_create(request: Value, app_handle: AppHandle) -> Result<Value, String> {
    let mut request = request.as_object().cloned().ok_or("Research request must be an object")?;
    request.insert("action".into(), json!("create"));
    query(app_handle, Value::Object(request)).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_candidate(run_id: String, file: String, content: String, keys: Vec<String>, app_handle: AppHandle) -> Result<Value, String> {
    query(app_handle, json!({"action":"candidate","run_id":run_id,"file":file,"content":content,"keys":keys})).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_import(source: String, title: Option<String>, app_handle: AppHandle) -> Result<Value, String> {
    query(app_handle, json!({"action":"import","source":source,"title":title})).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_read(run_id: String, section: Option<String>, offset: Option<u32>, limit: Option<u32>,
                                   field: Option<String>, compact: Option<bool>, app_handle: AppHandle) -> Result<Value, String> {
    // Agents get compact rows by default; the Research page asks for full curves.
    query(app_handle, json!({"action":"read","run_id":run_id,"section":section.unwrap_or_else(||"results".into()),
        "offset":offset.unwrap_or(0),"limit":limit.unwrap_or(100),"field":field,"compact":compact.unwrap_or(true)})).await
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn research_install(run_id: String, indicator: String, timeframe: String, app_handle: AppHandle) -> Result<Value, String> {
    let result = query(app_handle.clone(), json!({"action":"install","run_id":run_id,"indicator":indicator,"timeframe":timeframe})).await?;
    *app_handle.state::<crate::ScriptState>().listing.lock().map_err(|e| e.to_string())? = None;
    Ok(result)
}
