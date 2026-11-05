//! "Export everything" as an agent tool (PLAN-MIGRATION §2.1): the suite's
//! export behind QuantMCP's `export_everything` capability, which is
//! `external` — the file holds every key, so only on the user's request.

use std::collections::BTreeMap;

use tauri::Emitter;

#[tauri::command(rename_all = "snake_case")]
pub async fn export_everything(
    app: tauri::AppHandle,
    dest_dir: String,
    passphrase: String,
    include: Option<BTreeMap<String, bool>>,
) -> Result<serde_json::Value, String> {
    let passphrase = zeroize::Zeroizing::new(passphrase);
    tauri::async_runtime::spawn_blocking(move || {
        qs_core::bundle::export_suite(std::path::Path::new(&dest_dir), &passphrase, include.unwrap_or_default(), &mut |p| {
            if p.current == "done" {
                let _ = app.emit("qs:bundle-progress", &p);
            }
        })
    })
    .await
    .map_err(|e| format!("Export failed: {e}"))?
}
