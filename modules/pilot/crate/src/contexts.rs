//! Where a session runs: the general memory vault, or one of the suite's
//! registered workspaces (PLAN-WORKSPACES: a workspace is a folder, the
//! registry is core.db).

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub const GENERAL_ID: &str = "general";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub id: String,
    pub name: String,
    pub path: String,
    /// `general` | `workspace`
    pub kind: String,
}

/// The general vault: the `memory / vault.path` setting when the user moved
/// it, else the memory module's default. Mirrors the memory crate's resolver
/// (a crate dependency would pull the whole memory plugin in).
pub fn general_vault(app: &AppHandle) -> PathBuf {
    let configured = app
        .try_state::<qs_core::db::Db>()
        .and_then(|db| {
            let conn = db.0.lock().ok()?;
            qs_core::db::get_setting(&conn, "memory", "vault.path").ok().flatten()
        })
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|s| !s.trim().is_empty());
    configured
        .map(PathBuf::from)
        .unwrap_or_else(|| qs_core::paths::module_dir("memory").join("vault"))
}

pub fn list(app: &AppHandle) -> Vec<Context> {
    let mut out = vec![Context {
        id: GENERAL_ID.into(),
        name: "General".into(),
        path: general_vault(app).to_string_lossy().into_owned(),
        kind: "general".into(),
    }];
    if let Some(db) = app.try_state::<qs_core::db::Db>() {
        if let Ok(conn) = db.0.lock() {
            for w in qs_core::workspaces::list(&conn) {
                out.push(Context {
                    id: w.id.clone(),
                    name: w.name.clone(),
                    path: w.path.clone(),
                    kind: "workspace".into(),
                });
            }
        }
    }
    out
}

pub fn resolve(app: &AppHandle, id: &str) -> Option<Context> {
    list(app).into_iter().find(|c| c.id == id)
}
