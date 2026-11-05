//! Move this installation straight to the server (PLAN-MIGRATION §2.3, card
//! Migration 4/4).
//!
//! On the PC that moves: export the bundle (card 2) into
//! `<data root>-move/`, send it over the paired connection to the server's
//! transfers folder in checksummed, resumable chunks (card Server 4), then
//! ask the server to check it and to import it with the path table entered
//! here. The passphrase travels only inside the encrypted connection and is
//! stored nowhere. Afterwards the frontend confirms the move here with the
//! report's code (this PC stops trading for good) and switches this PC to the
//! server.
//!
//! On the server: [`propose`] and [`import`] read the bundle from the
//! transfers folder; the import keeps the server's own gateway — its key, the
//! paired devices, where it listens, its role — and leaves the moving PC's
//! device key behind, then restarts QuantSuite to switch.

use crate::db::{self, Db};
use crate::gateway;
use crate::import::{Keep, PathRow};
use crate::server::{self, Role};
use crate::{paths, vault};
use qs_gateway::client::{self, CallOptions, Session};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Emitted with `{ phase, done, total }` while the move runs.
pub const PROGRESS: &str = "qs:move-progress";
/// The bundle of a move that has not finished (`{ path, connection, created_at }`).
const STATE: &str = "migration.move";

#[derive(Default)]
pub struct MoveState {
    session: tokio::sync::Mutex<Option<Arc<Session>>>,
    sending: Mutex<Option<tokio::task::AbortHandle>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn move_dir() -> PathBuf {
    let root = paths::root();
    root.with_file_name(format!("{}-move", root.file_name().and_then(|n| n.to_str()).unwrap_or("quantsuite")))
}

fn progress<R: Runtime>(app: &AppHandle<R>, phase: &str, done: u64, total: u64) {
    let _ = app.emit(PROGRESS, json!({ "phase": phase, "done": done, "total": total }));
}

// ── on the PC that moves ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MoveInfo {
    /// The bundle of an unfinished move, ready to be sent (again).
    pub bundle: Option<String>,
    pub bytes: Option<u64>,
    pub connection: Option<String>,
    pub created_at: Option<String>,
}

pub fn info(conn: &rusqlite::Connection) -> MoveInfo {
    let s = db::get_setting(conn, "core", STATE).ok().flatten().unwrap_or(Value::Null);
    let bundle = s.get("path").and_then(Value::as_str).map(String::from).filter(|p| std::path::Path::new(p).is_file());
    MoveInfo {
        bytes: bundle.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()),
        connection: s.get("connection").and_then(Value::as_str).map(String::from),
        created_at: s.get("created_at").and_then(Value::as_str).map(String::from),
        bundle,
    }
}

/// Step 1: the bundle, as an export (card 2) into `<data root>-move/`.
pub async fn export<R: Runtime>(app: &AppHandle<R>, connection: &str, passphrase: zeroize::Zeroizing<String>, include: std::collections::BTreeMap<String, bool>) -> Result<MoveInfo, String> {
    if server::role() != Role::Local {
        return Err("Only an installation with its own data can move (this PC is a server or uses one).".into());
    }
    let dir = move_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let a = app.clone();
    let exported = tauri::async_runtime::spawn_blocking(move || {
        crate::bundle::export_suite(&dir, &passphrase, include, &mut |p| progress(&a, "export", p.done_bytes, p.total_bytes))
    })
    .await
    .map_err(|e| e.to_string())??;
    let path = exported.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let state = json!({ "path": path, "connection": connection, "created_at": chrono::Utc::now().to_rfc3339() });
    db::set_setting(&conn, "core", STATE, &state).map_err(|e| e.to_string())?;
    Ok(info(&conn))
}

/// Step 2: connect, enter the server PIN (the move's server steps are
/// `external`), and send the bundle — continuing a part the server kept.
pub async fn send<R: Runtime>(app: &AppHandle<R>, connection: &str, pin: &str) -> Result<Value, String> {
    let (c, bundle) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let c = gateway::connections(&conn).into_iter().find(|c| c.id == connection).ok_or("No such server connection.")?;
        (c, info(&conn).bundle.ok_or("Export the bundle first.")?)
    };
    let session = Arc::new(gateway::open_session(app, &c).await?);
    session.step_up(pin).await?;
    *app.state::<MoveState>().session.lock().await = Some(session.clone());
    let path = PathBuf::from(&bundle);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("move.qsbundle").to_string();
    let a = app.clone();
    let task = tokio::spawn(async move { session.upload_with(&path, &name, None, &|done, total| progress(&a, "send", done, total)).await });
    *lock(&app.state::<MoveState>().sending) = Some(task.abort_handle());
    let report = task.await.map_err(|_| "The sending was stopped; it continues where it stopped next time.".to_string())??;
    *lock(&app.state::<MoveState>().sending) = None;
    Ok(json!({ "name": report.path.rsplit(['/', '\\']).next().unwrap_or_default(), "bytes": report.bytes, "resumed_from": report.resumed_from }))
}

/// Stops the sending; the server keeps what arrived.
pub fn cancel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(h) = lock(&app.state::<MoveState>().sending).take() {
        h.abort();
    }
}

async fn session<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Session>, String> {
    let s = app.state::<MoveState>().session.lock().await.clone();
    s.filter(|s| !s.is_closed()).ok_or_else(|| "The connection to the server is gone; send the bundle again (it continues).".to_string())
}

fn bundle_name(conn: &rusqlite::Connection) -> Result<String, String> {
    let b = info(conn).bundle.ok_or("No bundle to move.")?;
    Ok(PathBuf::from(b).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string())
}

/// Step 3: the server checks the bundle and proposes its path table.
pub async fn remote_propose<R: Runtime>(app: &AppHandle<R>, passphrase: &str) -> Result<Value, String> {
    let name = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        bundle_name(&conn)?
    };
    let s = session(app).await?;
    let opts = CallOptions { key: None, timeout: Duration::from_secs(30 * 60) };
    s.call_with("plugin:qs|move_propose", json!({ "name": name, "passphrase": passphrase }), opts).await.map_err(|e| e.message)
}

/// Step 4: the server imports with the path table chosen here; its report.
pub async fn remote_import<R: Runtime>(app: &AppHandle<R>, passphrase: &str, rows: Vec<PathRow>) -> Result<Value, String> {
    let name = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        bundle_name(&conn)?
    };
    let s = session(app).await?;
    let opts = CallOptions { key: Some(client::new_key()), timeout: Duration::from_secs(60 * 60) };
    s.call_with("plugin:qs|move_import", json!({ "name": name, "passphrase": passphrase, "rows": rows }), opts).await.map_err(|e| e.message)
}

/// Step 5, after the frontend confirmed the move here: the local bundle goes
/// (it holds every secret), and this PC uses the server from the next start.
pub fn finish<R: Runtime>(app: &AppHandle<R>, connection: &str) -> Result<(), String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(move_dir());
    db::set_setting(&conn, "core", STATE, &Value::Null).map_err(|e| e.to_string())?;
    gateway::use_connection(&conn, Some(connection))
}

// ── on the server ──────────────────────────────────────────────────────────

fn received(name: &str) -> Result<PathBuf, String> {
    if server::role() != Role::Server {
        return Err("This PC is not a server.".into());
    }
    let p = gateway::file_roots().upload_target(name)?;
    if !p.is_file() {
        return Err(format!("{name} has not arrived on the server."));
    }
    Ok(p)
}

/// The server checks a bundle that arrived and proposes the path table.
pub fn propose(name: &str, passphrase: &str) -> Result<Value, String> {
    let bundle = received(name)?;
    let root = paths::root();
    let probe = root.with_file_name(".quantsuite-import-probe");
    let (manifest, rows) = crate::import::propose(&bundle, passphrase, &probe, &root)?;
    Ok(json!({
        "machine": manifest.machine,
        "created_at": manifest.created_at,
        "suite_version": manifest.suite_version,
        "files": manifest.files.len(),
        "bytes": manifest.files.iter().map(|f| f.size).sum::<u64>(),
        "secrets": manifest.secrets.len(),
        "rows": rows,
    }))
}

/// What the server keeps through the switch.
fn keep(conn: &rusqlite::Connection) -> Result<Keep, String> {
    let settings = db::get_scope(conn, "core")
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|(k, _)| k.starts_with(Keep::SETTINGS))
        .collect();
    let secrets = vault::with_vault(|v| Ok(v.get(gateway::SERVER_IDENTITY)?.map(|k| vec![(gateway::SERVER_IDENTITY.to_string(), k)]).unwrap_or_default()))?;
    Ok(Keep { settings, secrets, root: Some(paths::root()) })
}

/// The server imports a bundle that arrived, keeping its own gateway, and
/// restarts a few seconds after a green report to switch to the moved data.
pub async fn import<R: Runtime>(app: &AppHandle<R>, name: String, passphrase: zeroize::Zeroizing<String>, rows: Vec<PathRow>) -> Result<crate::import::Report, String> {
    let bundle = received(&name)?;
    let keep = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        keep(&conn)?
    };
    let b = bundle.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        crate::import::import_keeping(&b, &passphrase, &paths::root(), &rows, &crate::vault::OsKeyStore, Some(&keep))
    })
    .await
    .map_err(|e| e.to_string())??;
    if report.ok {
        // The bundle holds every secret; it is not needed any more.
        let _ = std::fs::remove_file(&bundle);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            app.restart();
        });
    }
    Ok(report)
}
