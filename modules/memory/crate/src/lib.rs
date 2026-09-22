//! QuantMemory — the agents' shared second brain.
//!
//! An Obsidian-style vault of markdown memories, written by AI agents and by
//! the operator, for both. **Files are the source of truth**: every memory is
//! one `.md` with YAML frontmatter and `[[wikilinks]]`, the vault opens in
//! Obsidian unchanged, and `memory.db` is only a rebuildable index (index.rs).
//!
//! ## Scopes (V2, 2026-08-31)
//!
//! Knowledge is scoped the way the suite is: the **general** vault holds
//! cross-project knowledge (`memory / vault.path` core setting, default
//! `~/.quantsuite/modules/memory/vault/`), and every **workspace** from the
//! core.db registry (PLAN-WORKSPACES) has its own vault at
//! `~/.quantsuite/modules/memory/workspaces/<b36>/` — central, so project
//! folders stay untouched. General is the gigabrain: reads and search span
//! every scope by default, while writes without an explicit scope land in
//! the active workspace (general when none is open). Links resolve in their
//! own scope first, then general — never into a foreign project.
//!
//! Agents reach everything here through the `quantsuite.memory.*` capability
//! tools declared in module.json — this crate is also their backend.

mod base;
mod index;
mod quality;
mod retrieval;
mod intelligence;
mod vault;
mod watcher;

use index::{FileRecord, MemoryMeta};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, State, Wry,
};
use uuid::Uuid;
use watcher::{SelfWrites, WatcherHandle};

// ─── State and helpers ────────────────────────────────────────────────────

pub struct AppState {
    db: Arc<Mutex<Connection>>,
    /// The general vault's directory (the configurable one).
    vault: RwLock<PathBuf>,
    self_writes: SelfWrites,
    /// One watcher per scope whose vault directory exists.
    watchers: Mutex<HashMap<String, WatcherHandle>>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("memory")
}

fn default_vault() -> PathBuf {
    data_dir().join("vault")
}

/// The configured general vault, or the default. The setting lives in
/// core.db so the settings UI and this crate read the same value.
fn resolve_vault_path(app: &AppHandle) -> PathBuf {
    let configured = app
        .try_state::<qs_core::db::Db>()
        .and_then(|db| {
            let conn = db.0.lock().ok()?;
            qs_core::db::get_setting(&conn, "memory", "vault.path").ok().flatten()
        })
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|s| !s.trim().is_empty());
    configured.map(PathBuf::from).unwrap_or_else(default_vault)
}

fn db(state: &State<'_, AppState>) -> Arc<Mutex<Connection>> {
    state.db.clone()
}

fn trash_dir(vault: &Path) -> PathBuf {
    vault.join(".trash")
}

fn publish(app: &AppHandle, topic: &str, payload: Value) {
    let _ = qs_core::bus::publish(app, qs_core::bus::Event::new(topic, "memory", payload));
}

// ─── Scopes ───────────────────────────────────────────────────────────────

const GENERAL_SCOPE: &str = "general";

use qs_core::workspaces::{normalize_path, WorkspaceEntry};

/// Every workspace in the core.db registry (shared reader in
/// `qs_core::workspaces`). Empty on any failure — a broken registry read must
/// degrade to "general only", never to an error.
fn registered_workspaces(app: &AppHandle) -> Vec<WorkspaceEntry> {
    let Some(db) = app.try_state::<qs_core::db::Db>() else { return Vec::new() };
    let Ok(conn) = db.0.lock() else { return Vec::new() };
    qs_core::workspaces::list(&conn)
}

/// The scope of the suite's open workspace (setting `core / workspace.active`),
/// if that folder is in the registry.
fn active_workspace_scope(app: &AppHandle) -> Option<String> {
    let db = app.try_state::<qs_core::db::Db>()?;
    let conn = db.0.lock().ok()?;
    qs_core::workspaces::active(&conn).map(|w| w.id)
}

/// Where a scope's vault lives on disk. General is the configurable path;
/// workspace vaults are central (`workspaces/<b36>/`) so project folders
/// stay untouched (user decision 2026-08-31).
fn scope_vault_dir(state: &AppState, scope: &str) -> Result<PathBuf, String> {
    if scope == GENERAL_SCOPE {
        return state.vault.read().map(|p| p.clone()).map_err(|e| format!("Lock vault path: {e}"));
    }
    let b36 = scope.rsplit(':').next().unwrap_or_default();
    if b36.is_empty() || !b36.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("Not a scope id: {scope}"));
    }
    Ok(data_dir().join("workspaces").join(b36))
}

/// Turn whatever a caller said into a scope.
///
/// `None` follows the gigabrain defaults: writes land in the active
/// workspace (general when none is open), reads see everything (`None` out =
/// no filter). `'general'` and `'all'` are literal; `'active'` names the open
/// workspace; anything else matches a registered workspace by entity id,
/// name (case-insensitive) or folder path.
fn resolve_scope(
    app: &AppHandle,
    explicit: Option<&str>,
    for_write: bool,
) -> Result<Option<String>, String> {
    match explicit.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(if for_write {
            Some(active_workspace_scope(app).unwrap_or_else(|| GENERAL_SCOPE.into()))
        } else {
            None
        }),
        Some("all") => {
            if for_write {
                Err("Scope 'all' cannot be written to — name a workspace or 'general'".into())
            } else {
                Ok(None)
            }
        }
        Some("active") => {
            Ok(Some(active_workspace_scope(app).unwrap_or_else(|| GENERAL_SCOPE.into())))
        }
        Some(s) if s.eq_ignore_ascii_case(GENERAL_SCOPE) => Ok(Some(GENERAL_SCOPE.into())),
        Some(s) => {
            let workspaces = registered_workspaces(app);
            let norm = normalize_path(s);
            match workspaces.iter().find(|w| {
                w.id == s || w.name.eq_ignore_ascii_case(s) || normalize_path(&w.path) == norm
            }) {
                Some(w) => Ok(Some(w.id.clone())),
                None => Err(format!(
                    "Unknown scope \"{s}\". Use 'general', 'all', 'active', or a registered workspace: {}",
                    workspaces.iter().map(|w| w.name.as_str()).collect::<Vec<_>>().join(", ")
                )),
            }
        }
    }
}

/// The scope an identifier lookup should prefer: the explicit one, else the
/// active workspace, else general — so an agent inside a project finds its
/// own note first but still reaches every other scope by name.
fn resolve_read_hint(app: &AppHandle, explicit: Option<&str>) -> Result<Option<String>, String> {
    match explicit {
        None => Ok(Some(active_workspace_scope(app).unwrap_or_else(|| GENERAL_SCOPE.into()))),
        some => resolve_scope(app, some, false),
    }
}

/// A write target: always a concrete scope, its vault dir created.
fn write_scope(app: &AppHandle, state: &AppState, explicit: Option<&str>) -> Result<(String, PathBuf), String> {
    let scope = resolve_scope(app, explicit, true)?.unwrap_or_else(|| GENERAL_SCOPE.into());
    let vault = scope_vault_dir(state, &scope)?;
    fs::create_dir_all(&vault).map_err(|e| format!("Create vault: {e}"))?;
    Ok((scope, vault))
}

// ─── core.db mirror (best-effort, like QuantMCP's kanban cards) ───────────

fn mirror(meta: &MemoryMeta) {
    let _ = qs_core::runtime::upsert_entity(qs_core::db::Entity {
        id: format!("memory:note:{}", meta.id),
        module: "memory".into(),
        kind: "note".into(),
        title: meta.title.clone(),
        subtitle: meta.kind.clone(),
        route: format!("/memory/m/{}", meta.id),
        icon: None,
        updated_at: 0,
        payload: None,
    });
}

fn unmirror(id: &str) {
    let _ = qs_core::runtime::delete_entity(&format!("memory:note:{id}"));
}

fn mirror_ids(conn: &Connection, ids: &[String]) {
    for id in ids {
        if let Ok(Some(meta)) = index::get_by_identifier(conn, id, None) {
            mirror(&meta);
        }
    }
}

// ─── Vault file operations ────────────────────────────────────────────────

fn write_vault_file(state: &AppState, path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create {}: {e}", parent.display()))?;
    }
    state.self_writes.mark(path);
    fs::write(path, text).map_err(|e| format!("Write {}: {e}", path.display()))
}

fn read_vault_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Read {}: {e}", path.display()))
}

/// Re-index one file after a write and re-point links vault-wide.
fn index_file(conn: &Connection, vault: &Path, abs: &Path, scope: &str) -> Result<String, String> {
    let text = read_vault_file(abs)?;
    let rel = index::rel_path_of(vault, abs);
    let mtime = fs::metadata(abs)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let id = index::upsert_from_file(
        conn,
        &FileRecord { scope, rel_path: &rel, text: &text, mtime_ms: mtime },
    )?;
    index::resolve_links(conn)?;
    Ok(id)
}

fn require_meta(
    conn: &Connection,
    identifier: &str,
    scope_hint: Option<&str>,
) -> Result<MemoryMeta, String> {
    index::get_by_identifier(conn, identifier, scope_hint)?
        .ok_or_else(|| format!("No memory matches \"{identifier}\" (by title, filename or id)"))
}

/// `# Title` on top unless the body already opens with exactly that heading.
fn ensure_h1(title: &str, body: &str) -> String {
    let trimmed = body.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("");
    if first_line.trim().to_lowercase() == format!("# {}", title.to_lowercase()) {
        return body.to_string();
    }
    if trimmed.is_empty() {
        return format!("# {title}\n");
    }
    format!("# {title}\n\n{body}")
}

/// The frontmatter for a write: the file's existing map (foreign keys
/// preserved), or a fresh one carrying the id the index knows the file by.
fn frontmatter_for_write(existing: Option<Map<String, Value>>, id: &str, created: &str) -> Map<String, Value> {
    let mut map = existing.unwrap_or_default();
    map.entry("id".to_string()).or_insert_with(|| json!(id));
    map.entry("created".to_string()).or_insert_with(|| json!(created));
    map.insert("updated".to_string(), json!(now_iso()));
    map
}

struct NewMemory<'a> {
    title: &'a str,
    body: Option<&'a str>,
    tags: Vec<String>,
    kind: Option<&'a str>,
    author: Option<&'a str>,
}

/// The one write path for a brand-new memory — used by the command, the
/// welcome seed and the legacy import, so they cannot drift apart.
fn create_in_vault(
    conn: &Connection,
    state: &AppState,
    vault: &Path,
    scope: &str,
    new: &NewMemory<'_>,
) -> Result<MemoryMeta, String> {
    let title = new.title.trim();
    if title.is_empty() {
        return Err("A memory needs a title".into());
    }
    let base = vault::slugify(title);
    let slug = vault::unique_slug(&base, |candidate| vault.join(format!("{candidate}.md")).exists());
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    let mut fm = Map::new();
    fm.insert("id".into(), json!(id));
    if let Some(kind) = new.kind.map(str::trim).filter(|k| !k.is_empty()) {
        fm.insert("type".into(), json!(kind));
    }
    if !new.tags.is_empty() {
        fm.insert("tags".into(), json!(new.tags));
    }
    if let Some(author) = new.author.map(str::trim).filter(|a| !a.is_empty()) {
        fm.insert("author".into(), json!(author));
    }
    fm.insert("created".into(), json!(now));
    fm.insert("updated".into(), json!(now));

    let body = ensure_h1(title, new.body.unwrap_or_default());
    let abs = vault.join(format!("{slug}.md"));
    write_vault_file(state, &abs, &vault::compose(&fm, &body))?;
    let indexed_id = index_file(conn, vault, &abs, scope)?;
    require_meta(conn, &indexed_id, None)
}

// ─── Read commands ────────────────────────────────────────────────────────

#[tauri::command(async)]
fn list_memories(
    scope: Option<String>,
    kind: Option<String>,
    tag: Option<String>,
    limit: Option<i64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<MemoryMeta>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::list(&conn, sc.as_deref(), kind.as_deref(), tag.as_deref(), limit.unwrap_or(500).clamp(1, 5000))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDoc {
    meta: MemoryMeta,
    body: String,
    frontmatter: Value,
    outgoing: Vec<index::OutgoingLink>,
    backlinks: Vec<index::Backlink>,
    revision: Option<String>,
}

#[tauri::command(async)]
fn get_memory(
    identifier: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryDoc, String> {
    let hint = resolve_read_hint(&app, scope.as_deref())?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut meta = require_meta(&conn, &identifier, hint.as_deref())?;
    let vault = scope_vault_dir(&state, &meta.scope)?;
    // The file is the truth; the indexed doc only fills in if it vanished
    // between the query and the read.
    let (frontmatter, body, revision) = match read_vault_file(&vault.join(&meta.rel_path)) {
        Ok(text) => {
            let (yaml, body) = vault::split_frontmatter(&text);
            let map = yaml.and_then(vault::parse_frontmatter);
            // A watcher may not have indexed an external edit yet. Never show
            // the previous body's review status beside freshly read text.
            meta.quality = quality::for_body(map.as_ref().and_then(|m| m.get("quality")), &meta.id, body);
            (map.map(Value::Object).unwrap_or(Value::Null), body.to_string(), Some(format!("{:016x}", vault::fnv1a64(&text))))
        }
        Err(_) => (Value::Null, index::body_of(&conn, &meta.id)?.unwrap_or_default(), None),
    };
    Ok(MemoryDoc {
        outgoing: index::outgoing_links(&conn, &meta.id)?,
        backlinks: index::backlinks(&conn, &meta.id)?,
        meta,
        body,
        frontmatter,
        revision,
    })
}

#[tauri::command(async)]
fn search_memories(
    query: String,
    mode: Option<String>,
    limit: Option<i64>,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<index::SearchHit>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::search(
        &conn,
        sc.as_deref(),
        &query,
        mode.as_deref().unwrap_or("all"),
        limit.unwrap_or(20).clamp(1, 200),
    )
}

#[tauri::command(async)]
fn get_backlinks(
    identifier: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<index::Backlink>, String> {
    let hint = resolve_read_hint(&app, scope.as_deref())?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let meta = require_meta(&conn, &identifier, hint.as_deref())?;
    index::backlinks(&conn, &meta.id)
}

#[tauri::command(async)]
fn get_graph(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<index::Graph, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::graph(&conn, sc.as_deref())
}

#[tauri::command(async)]
fn get_orphans(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<MemoryMeta>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::orphans(&conn, sc.as_deref())
}

#[tauri::command(async)]
fn get_unresolved_links(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<index::UnresolvedLink>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::unresolved(&conn, sc.as_deref())
}

#[tauri::command(async)]
fn suggest_connections(
    identifier: String,
    limit: Option<i64>,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<index::Suggestion>, String> {
    let hint = resolve_read_hint(&app, scope.as_deref())?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let meta = require_meta(&conn, &identifier, hint.as_deref())?;
    index::suggest_connections(&conn, &meta.id, limit.unwrap_or(5).clamp(1, 20))
}

#[tauri::command(async)]
fn get_stats(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<index::Stats, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::stats(&conn, sc.as_deref())
}

#[tauri::command(async)]
fn list_tags(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<index::TagCount>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    index::list_tags(&conn, sc.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeInfo {
    scope: String,
    name: String,
    /// The workspace folder — absent for general.
    path: Option<String>,
    vault_dir: String,
    memory_count: i64,
    is_active: bool,
}

/// General first, then every registered workspace — the memory module's own
/// scope picker and the agents' `workspaces` tool both read this.
#[tauri::command(async)]
fn list_scopes(state: State<'_, AppState>, app: AppHandle) -> Result<Vec<ScopeInfo>, String> {
    let active = active_workspace_scope(&app);
    let count = |scope: &str| -> i64 {
        let arc = db(&state);
        let Ok(conn) = arc.lock() else { return 0 };
        conn.query_row("SELECT COUNT(*) FROM memories WHERE scope = ?1", [scope], |r| r.get(0))
            .unwrap_or(0)
    };
    let general_vault = scope_vault_dir(&state, GENERAL_SCOPE)?;
    let mut out = vec![ScopeInfo {
        scope: GENERAL_SCOPE.into(),
        name: "General".into(),
        path: None,
        vault_dir: general_vault.to_string_lossy().to_string(),
        memory_count: count(GENERAL_SCOPE),
        is_active: false,
    }];
    for ws in registered_workspaces(&app) {
        let vault_dir = scope_vault_dir(&state, &ws.id)?;
        out.push(ScopeInfo {
            memory_count: count(&ws.id),
            is_active: active.as_deref() == Some(ws.id.as_str()),
            scope: ws.id,
            name: ws.name,
            path: Some(ws.path),
            vault_dir: vault_dir.to_string_lossy().to_string(),
        });
    }
    Ok(out)
}

// ─── Write commands ───────────────────────────────────────────────────────

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)] // Flat args on purpose: MCP agents call this with named JSON arguments.
fn create_memory(
    title: String,
    body: Option<String>,
    tags: Option<Vec<String>>,
    kind: Option<String>,
    author: Option<String>,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    let (sc, vault) = write_scope(&app, &state, scope.as_deref())?;
    let meta = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        create_in_vault(
            &conn,
            &state,
            &vault,
            &sc,
            &NewMemory {
                title: &title,
                body: body.as_deref(),
                tags: tags.unwrap_or_default(),
                kind: kind.as_deref(),
                author: author.as_deref(),
            },
        )?
    };
    ensure_watcher(&app, &sc);
    mirror(&meta);
    publish(
        &app,
        "memory.note.created",
        json!({ "id": meta.id, "title": meta.title, "scope": meta.scope }),
    );
    Ok(meta)
}

/// Shared tail of every body-changing command: stamp `updated`, write the
/// file, re-index it, mirror it, announce it.
fn save_body(
    state: &AppState,
    app: &AppHandle,
    meta: &MemoryMeta,
    frontmatter: Option<Map<String, Value>>,
    body: &str,
) -> Result<MemoryMeta, String> {
    let vault = scope_vault_dir(state, &meta.scope)?;
    let abs = vault.join(&meta.rel_path);
    let mut fm = frontmatter_for_write(frontmatter, &meta.id, &meta.created_at);
    let saved = {
        let conn = state.db.lock().map_err(|e| format!("Lock db: {e}"))?;
        let original = read_vault_file(&abs)?;
        if format!("{:016x}", vault::fnv1a64(&original)) != meta.revision {
            return Err("Memory changed during this write. Reload and retry to preserve concurrent edits.".into());
        }
        // Reviewing yesterday's text does not verify today's edit.
        if vault::split_frontmatter(&original).1 != body {
            let mut q = quality::from_value(fm.get("quality"), &meta.id);
            q.reviewed = false;
            q.last_verified = None;
            fm.insert("quality".into(), json!(q));
        }
        write_vault_file(state, &abs, &vault::compose(&fm, body))?;
        let id = index_file(&conn, &vault, &abs, &meta.scope)?;
        require_meta(&conn, &id, None)?
    };
    mirror(&saved);
    publish(
        app,
        "memory.note.updated",
        json!({ "id": saved.id, "title": saved.title, "scope": saved.scope }),
    );
    Ok(saved)
}

/// A memory as a write sees it: its meta, its file's frontmatter, its body.
type LoadedMemory = (MemoryMeta, Option<Map<String, Value>>, String);

/// Find the memory a write names, plus its file's frontmatter and body.
fn load_for_write(
    state: &AppState,
    app: &AppHandle,
    identifier: &str,
    scope: Option<&str>,
) -> Result<LoadedMemory, String> {
    let hint = resolve_read_hint(app, scope)?;
    let conn = state.db.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut meta = require_meta(&conn, identifier, hint.as_deref())?;
    drop(conn);
    let vault = scope_vault_dir(state, &meta.scope)?;
    let text = read_vault_file(&vault.join(&meta.rel_path))?;
    meta.revision = format!("{:016x}", vault::fnv1a64(&text));
    let (yaml, body) = vault::split_frontmatter(&text);
    Ok((meta, yaml.and_then(vault::parse_frontmatter), body.to_string()))
}

#[tauri::command(async)]
fn update_memory(
    identifier: String,
    body: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    let (meta, fm, _) = load_for_write(&state, &app, &identifier, scope.as_deref())?;
    save_body(&state, &app, &meta, fm, &body)
}

#[tauri::command(async)]
fn append_memory(
    identifier: String,
    addition: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    if addition.trim().is_empty() {
        return Err("Nothing to append".into());
    }
    let (meta, fm, body) = load_for_write(&state, &app, &identifier, scope.as_deref())?;
    let new_body = format!("{}\n\n{}", body.trim_end(), addition.trim());
    save_body(&state, &app, &meta, fm, &new_body)
}

#[tauri::command(async)]
fn set_memory_meta(
    identifier: String,
    kind: Option<String>,
    tags: Option<Vec<String>>,
    author: Option<String>,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    let (meta, fm, body) = load_for_write(&state, &app, &identifier, scope.as_deref())?;
    let mut fm = fm.unwrap_or_default();
    // An empty string clears the field; None leaves it alone.
    if let Some(kind) = kind {
        if kind.trim().is_empty() {
            fm.remove("type");
        } else {
            fm.insert("type".into(), json!(kind.trim()));
        }
    }
    if let Some(tags) = tags {
        let clean: Vec<String> = tags
            .iter()
            .map(|t| t.trim().trim_start_matches('#').to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if clean.is_empty() {
            fm.remove("tags");
        } else {
            fm.insert("tags".into(), json!(clean));
        }
    }
    if let Some(author) = author {
        if author.trim().is_empty() {
            fm.remove("author");
        } else {
            fm.insert("author".into(), json!(author.trim()));
        }
    }
    save_body(&state, &app, &meta, Some(fm), &body)
}

#[tauri::command(async)]
fn rename_memory(
    identifier: String,
    new_title: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    let new_title = new_title.trim().to_string();
    if new_title.is_empty() {
        return Err("A memory needs a title".into());
    }
    let hint = resolve_read_hint(&app, scope.as_deref())?;
    let renamed = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let meta = require_meta(&conn, &identifier, hint.as_deref())?;
        let vault = scope_vault_dir(&state, &meta.scope)?;
        let old_abs = vault.join(&meta.rel_path);
        let old_title = meta.title.clone();

        // New filename beside the old one (same subfolder), collision-safe.
        let dir = Path::new(&meta.rel_path).parent().unwrap_or_else(|| Path::new(""));
        let base = vault::slugify(&new_title);
        let slug = vault::unique_slug(&base, |candidate| {
            let candidate_path = vault.join(dir).join(format!("{candidate}.md"));
            candidate_path.exists() && candidate_path != old_abs
        });
        let new_abs = vault.join(dir).join(format!("{slug}.md"));

        let text = read_vault_file(&old_abs)?;
        let (yaml, body) = vault::split_frontmatter(&text);
        let fm = frontmatter_for_write(yaml.and_then(vault::parse_frontmatter), &meta.id, &meta.created_at);
        let new_body = vault::replace_h1(body, &new_title);
        write_vault_file(&state, &new_abs, &vault::compose(&fm, &new_body))?;
        if new_abs != old_abs {
            state.self_writes.mark(&old_abs);
            fs::remove_file(&old_abs).map_err(|e| format!("Remove {}: {e}", old_abs.display()))?;
        }
        let id = index_file(&conn, &vault, &new_abs, &meta.scope)?;

        // Every memory pointing at the old name follows along — Obsidian's
        // rename behaviour, without which a rename silently orphans links.
        // Sources may live in ANY scope: a workspace memory can point at a
        // general one via the fallback, so the unresolved sweep is global.
        let sources: Vec<String> = index::backlinks(&conn, &id)?
            .into_iter()
            .map(|b| b.id)
            .chain(
                index::unresolved(&conn, None)?
                    .into_iter()
                    .filter(|u| {
                        u.target.to_lowercase() == old_title.to_lowercase()
                            || vault::slugify(&u.target) == vault::slugify(&old_title)
                    })
                    .flat_map(|u| u.sources.into_iter().map(|s| s.id)),
            )
            .filter(|source| *source != id)
            .collect();
        for source in sources {
            let Some(source_meta) = index::get_by_identifier(&conn, &source, None)? else { continue };
            let source_vault = scope_vault_dir(&state, &source_meta.scope)?;
            let source_abs = source_vault.join(&source_meta.rel_path);
            let Ok(text) = read_vault_file(&source_abs) else { continue };
            let (yaml, body) = vault::split_frontmatter(&text);
            let (rewritten, changed) = vault::rewrite_links(body, &old_title, &new_title);
            if changed == 0 {
                continue;
            }
            let out = match yaml {
                Some(yaml) => format!("---\n{yaml}---\n\n{rewritten}"),
                None => rewritten,
            };
            write_vault_file(&state, &source_abs, &out)?;
            index_file(&conn, &source_vault, &source_abs, &source_meta.scope)?;
        }
        index::resolve_links(&conn)?;
        require_meta(&conn, &id, None)?
    };
    mirror(&renamed);
    publish(
        &app,
        "memory.note.updated",
        json!({ "id": renamed.id, "title": renamed.title, "scope": renamed.scope, "renamed": true }),
    );
    Ok(renamed)
}

#[tauri::command(async)]
fn delete_memory(
    identifier: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<bool, String> {
    let hint = resolve_read_hint(&app, scope.as_deref())?;
    let (id, title, meta_scope) = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let meta = require_meta(&conn, &identifier, hint.as_deref())?;
        let vault = scope_vault_dir(&state, &meta.scope)?;
        let abs = vault.join(&meta.rel_path);
        let trash = trash_dir(&vault);
        fs::create_dir_all(&trash).map_err(|e| format!("Create trash: {e}"))?;
        let name = Path::new(&meta.rel_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| meta.slug.clone());
        let stem = vault::unique_slug(&name, |candidate| trash.join(format!("{candidate}.md")).exists());
        state.self_writes.mark(&abs);
        fs::rename(&abs, trash.join(format!("{stem}.md")))
            .map_err(|e| format!("Move to trash: {e}"))?;
        index::remove_by_id(&conn, &meta.id)?;
        index::resolve_links(&conn)?;
        (meta.id, meta.title, meta.scope)
    };
    unmirror(&id);
    publish(&app, "memory.note.deleted", json!({ "id": id, "title": title, "scope": meta_scope }));
    Ok(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashEntry {
    file_name: String,
    size: u64,
    deleted_at_ms: i64,
}

#[tauri::command(async)]
fn list_trash(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<TrashEntry>, String> {
    let sc = resolve_scope(&app, scope.as_deref(), true)?.unwrap_or_else(|| GENERAL_SCOPE.into());
    let trash = trash_dir(&scope_vault_dir(&state, &sc)?);
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&trash) {
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            out.push(TrashEntry {
                file_name: entry.file_name().to_string_lossy().to_string(),
                size: meta.len(),
                deleted_at_ms: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
            });
        }
    }
    out.sort_by_key(|entry| std::cmp::Reverse(entry.deleted_at_ms));
    Ok(out)
}

#[tauri::command(async)]
fn restore_memory(
    file_name: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    if file_name.contains(['/', '\\']) || file_name.starts_with('.') {
        return Err("Not a trash file name".into());
    }
    let (sc, vault) = write_scope(&app, &state, scope.as_deref())?;
    let src = trash_dir(&vault).join(&file_name);
    if !src.is_file() {
        return Err(format!("No such file in trash: {file_name}"));
    }
    let restored = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let stem = Path::new(&file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone());
        let slug = vault::unique_slug(&stem, |candidate| vault.join(format!("{candidate}.md")).exists());
        let dest = vault.join(format!("{slug}.md"));
        state.self_writes.mark(&dest);
        fs::rename(&src, &dest).map_err(|e| format!("Restore: {e}"))?;
        let id = index_file(&conn, &vault, &dest, &sc)?;
        require_meta(&conn, &id, None)?
    };
    mirror(&restored);
    publish(
        &app,
        "memory.note.created",
        json!({ "id": restored.id, "title": restored.title, "scope": restored.scope, "restored": true }),
    );
    Ok(restored)
}

#[tauri::command(async)]
fn purge_trash(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<i64, String> {
    let sc = resolve_scope(&app, scope.as_deref(), true)?.unwrap_or_else(|| GENERAL_SCOPE.into());
    let trash = trash_dir(&scope_vault_dir(&state, &sc)?);
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(&trash) {
        for entry in entries.flatten() {
            if entry.path().is_file() && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

// ─── Base: the folder that describes the codebase (base.rs) ──────────────

/// The workspace a base call means: the explicit scope or the active one —
/// never general, which has no folder to describe.
fn base_workspace(
    app: &AppHandle,
    state: &AppState,
    explicit: Option<&str>,
) -> Result<(WorkspaceEntry, PathBuf), String> {
    let scope = resolve_scope(app, explicit, true)?.unwrap_or_else(|| GENERAL_SCOPE.into());
    if scope == GENERAL_SCOPE {
        return Err("The general vault has no codebase — pass a workspace as scope (its name or \
                    path), or open one in the suite"
            .into());
    }
    let ws = registered_workspaces(app)
        .into_iter()
        .find(|w| w.id == scope)
        .ok_or_else(|| format!("Workspace {scope} is not registered"))?;
    let vault = scope_vault_dir(state, &scope)?;
    fs::create_dir_all(vault.join(base::DIR)).map_err(|e| format!("Create base folder: {e}"))?;
    Ok((ws, vault))
}

/// One base document as its file holds it.
struct BaseDoc {
    meta: MemoryMeta,
    fm: Map<String, Value>,
    body: String,
}

fn fm_str(fm: &Map<String, Value>, key: &str) -> Option<String> {
    fm.get(key).and_then(Value::as_str).map(str::to_string)
}

fn fm_flag(fm: &Map<String, Value>, key: &str) -> bool {
    fm.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Every base document of a workspace, keyed by its `base_path`. The
/// frontmatter is the truth — a file renamed in Obsidian keeps its unit.
fn load_base_docs(conn: &Connection, vault: &Path, scope: &str) -> Result<BTreeMap<String, BaseDoc>, String> {
    let mut docs = BTreeMap::new();
    for meta in index::list(conn, Some(scope), Some(base::KIND), None, 5000)? {
        let Ok(text) = read_vault_file(&vault.join(&meta.rel_path)) else { continue };
        let (yaml, body) = vault::split_frontmatter(&text);
        let Some(fm) = yaml.and_then(vault::parse_frontmatter) else { continue };
        let Some(path) = fm_str(&fm, "base_path") else { continue };
        docs.insert(path, BaseDoc { meta, fm, body: body.to_string() });
    }
    Ok(docs)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseEntry {
    /// Workspace-relative unit path, `.` for the root.
    path: String,
    title: String,
    id: String,
    rel_path: String,
    /// First line of the hand-written description, if any.
    summary: Option<String>,
    /// The directory vanished from the workspace.
    missing: bool,
    described_files: usize,
    synced_at: Option<String>,
    updated_at: String,
}

fn base_entry(path: &str, doc: &BaseDoc) -> BaseEntry {
    BaseEntry {
        path: path.to_string(),
        title: doc.meta.title.clone(),
        id: doc.meta.id.clone(),
        rel_path: doc.meta.rel_path.clone(),
        summary: base::summary_of(&doc.body),
        missing: fm_flag(&doc.fm, "base_missing"),
        described_files: base::file_descriptions(&doc.body).len(),
        synced_at: fm_str(&doc.fm, "base_synced"),
        updated_at: doc.meta.updated_at.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseSyncReport {
    scope: String,
    workspace: String,
    root: String,
    depth: usize,
    units: usize,
    created: usize,
    updated: usize,
    unchanged: usize,
    missing: usize,
    files: usize,
    lines: usize,
}

/// Write one base document and index it.
fn save_base_doc(
    conn: &Connection,
    state: &AppState,
    vault: &Path,
    scope: &str,
    abs: &Path,
    fm: &Map<String, Value>,
    body: &str,
) -> Result<MemoryMeta, String> {
    write_vault_file(state, abs, &vault::compose(fm, body))?;
    let id = index_file(conn, vault, abs, scope)?;
    require_meta(conn, &id, None)
}

/// The sync proper, under the db lock: discover the units, create the
/// documents that are missing, regenerate the block of the ones that exist,
/// flag the ones whose directory is gone. `forced_paths` become units no
/// matter what discovery thinks. Returns the report and the changed ids.
fn sync_base_inner(
    state: &AppState,
    conn: &Connection,
    ws: &WorkspaceEntry,
    vault: &Path,
    depth: Option<usize>,
    forced_paths: &[String],
) -> Result<(BaseSyncReport, Vec<String>), String> {
    let root = PathBuf::from(&ws.path);
    if !root.is_dir() {
        return Err(format!("Workspace folder {} does not exist", ws.path));
    }
    let existing = load_base_docs(conn, vault, &ws.id)?;
    let stored_depth = existing
        .get(base::ROOT)
        .and_then(|d| d.fm.get("base_depth"))
        .and_then(Value::as_u64)
        .map(|d| d as usize);
    let depth = depth.or(stored_depth).unwrap_or(2).clamp(1, 6);

    // A described directory stays a unit for good; a requested one joins.
    let mut forced: HashSet<String> = existing
        .keys()
        .filter(|p| p.as_str() != base::ROOT && root.join(p).is_dir())
        .cloned()
        .collect();
    for p in forced_paths {
        let rel = base::normalize_rel(p, &root);
        if rel == base::ROOT {
            continue;
        }
        if !root.join(&rel).is_dir() {
            return Err(format!("No directory {rel} in {}", ws.path));
        }
        forced.insert(rel);
    }

    let units = base::discover(&root, &base::Discovery { depth, forced, ..Default::default() });
    let root_title = base::root_title(&ws.name);
    let now = now_iso();
    let summaries: BTreeMap<String, String> = existing
        .iter()
        .filter_map(|(p, d)| base::summary_of(&d.body).map(|s| (p.clone(), s)))
        .collect();
    let ctx = base::RenderCtx { root_title: &root_title, workspace_path: &ws.path, summaries: &summaries };

    let mut report = BaseSyncReport {
        scope: ws.id.clone(),
        workspace: ws.name.clone(),
        root: ws.path.clone(),
        depth,
        units: units.len(),
        created: 0,
        updated: 0,
        unchanged: 0,
        missing: 0,
        files: units.first().map(|u| u.totals.files).unwrap_or(0),
        lines: units.first().map(|u| u.totals.lines).unwrap_or(0),
    };
    let mut changed_ids = Vec::new();

    for unit in &units {
        let title = base::unit_title(&unit.rel_path, &root_title);
        let is_root = unit.rel_path == base::ROOT;
        let previous = existing
            .get(&unit.rel_path)
            .map(|d| base::file_descriptions(&d.body))
            .unwrap_or_default();
        let generated = base::render_auto(unit, &units, &ctx, &previous);
        match existing.get(&unit.rel_path) {
            Some(doc) => {
                let body = base::compose_body(&doc.body, &generated);
                let depth_kept = !is_root || stored_depth == Some(depth);
                if body == doc.body && !fm_flag(&doc.fm, "base_missing") && depth_kept {
                    report.unchanged += 1;
                    continue;
                }
                let mut fm = doc.fm.clone();
                fm.remove("base_missing");
                fm.insert("base_synced".into(), json!(now));
                fm.insert("updated".into(), json!(now));
                if is_root {
                    fm.insert("base_depth".into(), json!(depth));
                }
                let meta = save_base_doc(conn, state, vault, &ws.id, &vault.join(&doc.meta.rel_path), &fm, &body)?;
                changed_ids.push(meta.id);
                report.updated += 1;
            }
            None => {
                let mut fm = Map::new();
                fm.insert("id".into(), json!(Uuid::new_v4().to_string()));
                fm.insert("type".into(), json!(base::KIND));
                fm.insert("tags".into(), json!([base::KIND]));
                fm.insert("author".into(), json!("quantmemory"));
                fm.insert("created".into(), json!(now));
                fm.insert("updated".into(), json!(now));
                fm.insert("base_path".into(), json!(unit.rel_path));
                fm.insert("base_synced".into(), json!(now));
                if is_root {
                    fm.insert("base_depth".into(), json!(depth));
                } else {
                    // Obsidian resolves `[[modules/memory]]` through the alias;
                    // QuantMemory through the title.
                    fm.insert("aliases".into(), json!([title]));
                }
                let stem = base::unit_slug(&unit.rel_path);
                let base_dir = vault.join(base::DIR);
                let slug = vault::unique_slug(&stem, |c| base_dir.join(format!("{c}.md")).exists());
                let abs = base_dir.join(format!("{slug}.md"));
                let body = base::fresh_body(&title, &generated);
                let meta = save_base_doc(conn, state, vault, &ws.id, &abs, &fm, &body)?;
                changed_ids.push(meta.id);
                report.created += 1;
            }
        }
    }

    let unit_paths: HashSet<&str> = units.iter().map(|u| u.rel_path.as_str()).collect();
    for (path, doc) in &existing {
        if unit_paths.contains(path.as_str()) {
            continue;
        }
        report.missing += 1;
        if fm_flag(&doc.fm, "base_missing") {
            continue;
        }
        let mut fm = doc.fm.clone();
        fm.insert("base_missing".into(), json!(true));
        fm.insert("updated".into(), json!(now));
        let body = base::compose_body(&doc.body, &base::render_missing(&now));
        let meta = save_base_doc(conn, state, vault, &ws.id, &vault.join(&doc.meta.rel_path), &fm, &body)?;
        changed_ids.push(meta.id);
    }

    index::resolve_links(conn)?;
    Ok((report, changed_ids))
}

#[tauri::command(async)]
fn sync_base(
    scope: Option<String>,
    depth: Option<usize>,
    paths: Option<Vec<String>>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<BaseSyncReport, String> {
    let (ws, vault) = base_workspace(&app, &state, scope.as_deref())?;
    let (report, changed) = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let out = sync_base_inner(&state, &conn, &ws, &vault, depth, &paths.unwrap_or_default())?;
        mirror_ids(&conn, &out.1);
        out
    };
    ensure_watcher(&app, &ws.id);
    publish(
        &app,
        "memory.vault.changed",
        json!({ "reason": "base", "scope": ws.id, "units": report.units, "changed": changed.len() }),
    );
    Ok(report)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseListing {
    scope: String,
    workspace: String,
    root: String,
    units: Vec<BaseEntry>,
}

#[tauri::command(async)]
fn list_base(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<BaseListing, String> {
    let (ws, vault) = base_workspace(&app, &state, scope.as_deref())?;
    let docs = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        load_base_docs(&conn, &vault, &ws.id)?
    };
    let mut units: Vec<BaseEntry> = docs.iter().map(|(p, d)| base_entry(p, d)).collect();
    units.sort_by(|a, b| {
        (a.path != base::ROOT).cmp(&(b.path != base::ROOT)).then_with(|| a.path.cmp(&b.path))
    });
    Ok(BaseListing { scope: ws.id, workspace: ws.name, root: ws.path, units })
}

/// The unit responsible for a workspace path and the remainder inside it.
fn resolve_base_target(docs: &BTreeMap<String, BaseDoc>, path: &str) -> Result<(String, Option<String>), String> {
    if docs.is_empty() {
        return Err("No base documents for this workspace yet — run quantsuite.memory.base_sync first".into());
    }
    let paths: Vec<&str> = docs.keys().map(String::as_str).collect();
    base::owner_of(path, &paths)
        .map(|(unit, rest)| (unit.to_string(), rest))
        .ok_or_else(|| format!("No base document covers {path} — run quantsuite.memory.base_sync"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseFileHit {
    /// Relative to the unit.
    path: String,
    description: Option<String>,
    /// Whether the file has a line in the unit's file list.
    listed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseReadResult {
    unit: BaseEntry,
    body: String,
    file: Option<BaseFileHit>,
    outgoing: Vec<index::OutgoingLink>,
    backlinks: Vec<index::Backlink>,
}

#[tauri::command(async)]
fn get_base(
    path: Option<String>,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<BaseReadResult, String> {
    let (ws, vault) = base_workspace(&app, &state, scope.as_deref())?;
    let rel = base::normalize_rel(path.as_deref().unwrap_or(""), Path::new(&ws.path));
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let docs = load_base_docs(&conn, &vault, &ws.id)?;
    let (unit_path, rest) = resolve_base_target(&docs, &rel)?;
    let doc = &docs[&unit_path];
    let file = rest.map(|file_rel| {
        let line = base::file_lines(&doc.body).into_iter().find(|l| l.path == file_rel);
        BaseFileHit { listed: line.is_some(), description: line.and_then(|l| l.desc), path: file_rel }
    });
    Ok(BaseReadResult {
        unit: base_entry(&unit_path, doc),
        body: doc.body.clone(),
        file,
        outgoing: index::outgoing_links(&conn, &doc.meta.id)?,
        backlinks: index::backlinks(&conn, &doc.meta.id)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaseDescribeResult {
    /// `unit` or `file`.
    target: String,
    unit: BaseEntry,
    /// The described file, relative to the unit.
    file: Option<String>,
    /// The directory had no document and got one first.
    created_unit: bool,
}

#[tauri::command(async)]
fn describe_base(
    path: String,
    description: String,
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<BaseDescribeResult, String> {
    let (ws, vault) = base_workspace(&app, &state, scope.as_deref())?;
    let root = PathBuf::from(&ws.path);
    let rel = base::normalize_rel(&path, &root);
    let target = if rel == base::ROOT { root.clone() } else { root.join(&rel) };
    let (result, saved) = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let mut docs = load_base_docs(&conn, &vault, &ws.id)?;
        let mut created_unit = false;
        // A directory without a document of its own becomes a unit first
        // (and stays one), so a description always has a place to live.
        if !docs.contains_key(&rel) && target.is_dir() {
            let (_, changed) = sync_base_inner(&state, &conn, &ws, &vault, None, std::slice::from_ref(&rel))?;
            mirror_ids(&conn, &changed);
            docs = load_base_docs(&conn, &vault, &ws.id)?;
            created_unit = true;
        }
        let (unit_path, rest) = resolve_base_target(&docs, &rel)?;
        let doc = &docs[&unit_path];
        let body = match &rest {
            None => base::set_description(&doc.body, &doc.meta.title, &description),
            Some(file_rel) => {
                if !target.is_file() {
                    return Err(format!("{rel} is not a file in {}", ws.path));
                }
                let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let lang = base::lang_of(&name);
                let meta_text = match base::count_lines(&target) {
                    Some(n) => format!("{lang}, {n} lines"),
                    None => lang.to_string(),
                };
                base::set_file_description(&doc.body, file_rel, Some(&meta_text), &description)
            }
        };
        let mut fm = doc.fm.clone();
        fm.insert("updated".into(), json!(now_iso()));
        let saved = save_base_doc(&conn, &state, &vault, &ws.id, &vault.join(&doc.meta.rel_path), &fm, &body)?;
        index::resolve_links(&conn)?;
        let updated = BaseDoc { meta: saved.clone(), fm, body };
        let result = BaseDescribeResult {
            target: if rest.is_some() { "file".into() } else { "unit".into() },
            unit: base_entry(&unit_path, &updated),
            file: rest,
            created_unit,
        };
        (result, saved)
    };
    mirror(&saved);
    publish(
        &app,
        "memory.note.updated",
        json!({ "id": saved.id, "title": saved.title, "scope": saved.scope, "base": true }),
    );
    Ok(result)
}

// ─── Vault management ─────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultInfo {
    path: String,
    is_default: bool,
    watching: bool,
    trash_count: i64,
}

/// The GENERAL vault's info — workspace vaults are fixed and central; only
/// the general one is configurable.
#[tauri::command(async)]
fn get_vault_info(state: State<'_, AppState>) -> Result<VaultInfo, String> {
    let vault = scope_vault_dir(&state, GENERAL_SCOPE)?;
    let trash_count = fs::read_dir(trash_dir(&vault))
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count() as i64)
        .unwrap_or(0);
    let watching = state
        .watchers
        .lock()
        .map(|w| w.contains_key(GENERAL_SCOPE))
        .unwrap_or(false);
    Ok(VaultInfo {
        path: vault.to_string_lossy().to_string(),
        is_default: vault == default_vault(),
        watching,
        trash_count,
    })
}

/// Drop one scope's rows (unmirroring them) so a rescan starts clean.
fn wipe_scope(conn: &Connection, scope: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id FROM memories WHERE scope = ?1")
        .map_err(|e| format!("Prepare wipe: {e}"))?;
    let ids = stmt
        .query_map([scope], |r| r.get::<_, String>(0))
        .map_err(|e| format!("Query wipe: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read wipe: {e}"))?;
    for id in &ids {
        index::remove_by_id(conn, id)?;
        unmirror(id);
    }
    Ok(())
}

/// Every scope that can have a vault right now: general plus the registry.
fn all_scopes(app: &AppHandle) -> Vec<String> {
    let mut scopes = vec![GENERAL_SCOPE.to_string()];
    scopes.extend(registered_workspaces(app).into_iter().map(|w| w.id));
    scopes
}

fn rescan_scope_inner(app: &AppHandle, conn: &Connection, scope: &str) -> Result<index::ScanReport, String> {
    let state = app.state::<AppState>();
    let vault = scope_vault_dir(&state, scope)?;
    if !vault.is_dir() {
        return Ok(index::ScanReport::default());
    }
    let report = index::scan_vault(conn, &vault, scope)?;
    mirror_ids(conn, &report.changed_ids);
    for id in &report.removed_ids {
        unmirror(id);
    }
    Ok(report)
}

#[tauri::command(async)]
fn reindex_vault(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<index::ScanReport, String> {
    let sc = resolve_scope(&app, scope.as_deref(), false)?;
    let scopes = match &sc {
        Some(s) => vec![s.clone()],
        None => all_scopes(&app),
    };
    let mut total = index::ScanReport::default();
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        for s in &scopes {
            wipe_scope(&conn, s)?;
            let report = rescan_scope_inner(&app, &conn, s)?;
            total.added += report.added;
            total.updated += report.updated;
            total.removed += report.removed;
            total.total += report.total;
            total.changed_ids.extend(report.changed_ids);
            total.removed_ids.extend(report.removed_ids);
        }
        index::resolve_links(&conn)?;
    }
    publish(
        &app,
        "memory.vault.changed",
        json!({ "reason": "reindex", "scope": sc, "total": total.total }),
    );
    Ok(total)
}

#[tauri::command(async)]
fn set_vault_path(
    path: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<VaultInfo, String> {
    let new_vault = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => {
            let candidate = PathBuf::from(p);
            if !candidate.is_absolute() {
                return Err("The vault path must be absolute".into());
            }
            candidate
        }
        None => default_vault(),
    };
    fs::create_dir_all(&new_vault).map_err(|e| format!("Create vault: {e}"))?;

    // The setting lives in core.db; null means "back to the default".
    let core_db = app.try_state::<qs_core::db::Db>().ok_or("core.db not ready")?;
    {
        let conn = core_db.0.lock().map_err(|e| format!("Lock core.db: {e}"))?;
        let value = if new_vault == default_vault() { Value::Null } else { json!(new_vault.to_string_lossy()) };
        qs_core::db::set_setting(&conn, "memory", "vault.path", &value)
            .map_err(|e| format!("Save vault path: {e}"))?;
    }

    {
        let mut vault = state.vault.write().map_err(|e| format!("Lock vault path: {e}"))?;
        *vault = new_vault.clone();
    }
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        wipe_scope(&conn, GENERAL_SCOPE)?;
        rescan_scope_inner(&app, &conn, GENERAL_SCOPE)?;
        index::resolve_links(&conn)?;
    }
    if let Ok(mut watchers) = state.watchers.lock() {
        watchers.remove(GENERAL_SCOPE);
    }
    ensure_watcher(&app, GENERAL_SCOPE);
    publish(
        &app,
        "memory.vault.changed",
        json!({ "reason": "path", "scope": GENERAL_SCOPE, "path": new_vault.to_string_lossy() }),
    );
    get_vault_info(app.state::<AppState>())
}

// ─── Watcher, seed and legacy import ──────────────────────────────────────

/// Watch one scope's vault if its directory exists and no watcher runs yet.
/// Replacing/removing a handle drops the old watcher, which stops its thread.
fn ensure_watcher(app: &AppHandle, scope: &str) {
    let state = app.state::<AppState>();
    let Ok(vault) = scope_vault_dir(&state, scope) else { return };
    if !vault.is_dir() {
        return;
    }
    if state.watchers.lock().map(|w| w.contains_key(scope)).unwrap_or(true) {
        return;
    }
    let on_change = {
        let app = app.clone();
        let scope = scope.to_string();
        Box::new(move || {
            let state = app.state::<AppState>();
            let Ok(conn) = state.db.lock() else { return };
            match rescan_scope_inner(&app, &conn, &scope) {
                Ok(report) if report.added + report.updated + report.removed > 0 => {
                    publish(
                        &app,
                        "memory.vault.changed",
                        json!({
                            "reason": "external",
                            "scope": scope,
                            "added": report.added,
                            "updated": report.updated,
                            "removed": report.removed,
                        }),
                    );
                }
                Ok(_) => {}
                Err(e) => log::warn!("memory vault rescan ({scope}) failed: {e}"),
            }
        }) as Box<dyn Fn() + Send>
    };
    match watcher::start(vault, state.self_writes.clone(), on_change) {
        Ok(handle) => {
            if let Ok(mut watchers) = state.watchers.lock() {
                watchers.insert(scope.to_string(), handle);
            }
        }
        Err(e) => log::warn!("memory vault watcher ({scope}) not started: {e}"),
    }
}

fn ensure_all_watchers(app: &AppHandle) {
    for scope in all_scopes(app) {
        ensure_watcher(app, &scope);
    }
}

const WELCOME_TITLE: &str = "Welcome to QuantMemory";
const WELCOME_BODY: &str = "\
This vault is the shared second brain of this machine — written by AI agents \
and by the operator, readable by both. Every memory is one markdown file; the \
vault opens unchanged in Obsidian.

## Conventions

- **One memory per fact or topic**, with a title that still makes sense weeks \
later. Search before creating — extend an existing memory instead of \
duplicating it.
- **Connect liberally** with `[[Title]]` or `[[Title|alias]]` wikilinks. A \
link to a memory that does not exist yet is fine — it marks something worth \
writing, and resolves the moment it is created.
- **Append, don't overwrite.** Adding a fact to an existing memory should \
keep what is already there.
- Use the frontmatter `type` (user | project | reference | decision | \
insight) and `tags` so the vault stays filterable, and `author` to say which \
agent — or which human — wrote it.
- **Scopes:** this is the *general* vault — cross-project knowledge. Every \
suite workspace has its own vault for project knowledge; links resolve in \
their own scope first, then here. Search spans everything.
- This is durable knowledge, not a scratchpad: decisions, project state, \
why-it-is-this-way facts, pointers to external systems.";

/// First-run work that must not block the window: welcome seed, the one-time
/// import of the old flat MEMORY.md, the initial scans, the watchers.
fn initialize(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(general_vault) = scope_vault_dir(&state, GENERAL_SCOPE) else { return };
    {
        let Ok(conn) = state.db.lock() else { return };

        // Welcome memory — only into a genuinely empty general vault.
        let seed_due = !index::marker_set(&conn, "vault.seeded").unwrap_or(true);
        if seed_due {
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(&general_vault) {
                files.extend(entries.flatten().filter(|e| {
                    e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))
                }));
            }
            if files.is_empty() {
                let _ = create_in_vault(
                    &conn,
                    &state,
                    &general_vault,
                    GENERAL_SCOPE,
                    &NewMemory {
                        title: WELCOME_TITLE,
                        body: Some(WELCOME_BODY),
                        tags: vec!["meta".into()],
                        kind: Some("reference"),
                        author: Some("quantmemory"),
                    },
                );
            }
            let _ = index::set_marker(&conn, "vault.seeded");
        }

        // One-time import of the legacy global MEMORY.md (the flat-file
        // system QuantMemory replaces). The old file itself stays untouched.
        if !index::marker_set(&conn, "legacy.import.done").unwrap_or(true) {
            let legacy = dirs::home_dir().unwrap_or_default().join(".quantmcp").join("MEMORY.md");
            if let Ok(content) = fs::read_to_string(&legacy) {
                if !content.trim().is_empty() {
                    let body = format!(
                        "Imported one-to-one from `~/.quantmcp/MEMORY.md` when QuantMemory \
                         replaced the flat memory file. Split what is still true into \
                         focused memories, then thin this one out.\n\n---\n\n{content}"
                    );
                    let _ = create_in_vault(
                        &conn,
                        &state,
                        &general_vault,
                        GENERAL_SCOPE,
                        &NewMemory {
                            title: "Legacy Memory (global)",
                            body: Some(&body),
                            tags: vec!["legacy".into()],
                            kind: Some("reference"),
                            author: Some("quantmemory"),
                        },
                    );
                }
            }
            let _ = index::set_marker(&conn, "legacy.import.done");
        }

        for scope in all_scopes(app) {
            if let Err(e) = rescan_scope_inner(app, &conn, &scope) {
                log::warn!("memory vault initial scan ({scope}) failed: {e}");
            }
        }
        let _ = index::resolve_links(&conn);
    }
    ensure_all_watchers(app);
    publish(app, "memory.vault.changed", json!({ "reason": "startup" }));
}

// ─── Entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `memory` plugin. Pinned to `Wry` like the other module plugins:
/// bare `tauri::AppHandle` already means `AppHandle<Wry>`.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("memory")
        .setup(|app, _api| {
            let dir = data_dir();
            fs::create_dir_all(&dir)?;
            fs::create_dir_all(dir.join("workspaces"))?;

            let conn = Connection::open(dir.join("memory.db"))?;
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
            index::init_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let vault = resolve_vault_path(app);
            fs::create_dir_all(&vault)?;

            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                vault: RwLock::new(vault),
                self_writes: SelfWrites::default(),
                watchers: Mutex::new(HashMap::new()),
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "memory".into(),
                    label: "QuantMemory".into(),
                    route: "/memory".into(),
                    toggle_window: None,
                },
            );

            // A newly opened folder may be a workspace this module has never
            // seen — pick up its vault (and watcher) the moment it registers.
            if let Some(bus) = app.try_state::<qs_core::bus::Bus>() {
                let handle = app.clone();
                bus.subscribe(
                    "core.workspace.opened",
                    Box::new(move |_event| {
                        let app = handle.clone();
                        // Bus handlers run on the publisher's thread — do the
                        // filesystem work elsewhere.
                        std::thread::spawn(move || ensure_all_watchers(&app));
                    }),
                );
            }

            // Seed, legacy import, first scans and the watchers all touch the
            // filesystem — off the main thread, the window must not wait.
            let handle = app.clone();
            std::thread::Builder::new()
                .name("qm-init".into())
                .spawn(move || initialize(&handle))?;

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_memories,
            get_memory,
            create_memory,
            update_memory,
            append_memory,
            rename_memory,
            set_memory_meta,
            delete_memory,
            restore_memory,
            list_trash,
            purge_trash,
            search_memories,
            get_backlinks,
            get_graph,
            get_orphans,
            get_unresolved_links,
            suggest_connections,
            get_stats,
            get_vault_info,
            set_vault_path,
            reindex_vault,
            list_tags,
            list_scopes,
            sync_base,
            list_base,
            get_base,
            describe_base,
            intelligence::memory_context,
            intelligence::get_embedding_config,
            intelligence::set_embedding_config,
            intelligence::index_embeddings,
            intelligence::set_memory_quality,
            intelligence::review_memory_quality,
            intelligence::get_memory_review_queue,
        ]))
        .build()
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qm-lib-{name}-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn test_state(vault: &Path) -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        index::init_schema(&conn).unwrap();
        AppState {
            db: Arc::new(Mutex::new(conn)),
            vault: RwLock::new(vault.to_path_buf()),
            self_writes: SelfWrites::default(),
            watchers: Mutex::new(HashMap::new()),
        }
    }

    /// The base round trip without Tauri: sync creates the documents, a
    /// description survives the next sync, a vanished directory is flagged,
    /// and the units resolve paths the way the tools need them to.
    #[test]
    fn base_sync_round_trip() {
        let workspace = temp_dir("ws");
        touch(&workspace, "package.json", "{}\n");
        touch(&workspace, "README.md", "# hi\n");
        touch(&workspace, "modules/memory/module.json", "{}\n");
        touch(&workspace, "modules/memory/app/a.vue", "<template/>\n");
        touch(&workspace, "modules/memory/crate/Cargo.toml", "[package]\n");
        touch(&workspace, "modules/memory/crate/src/lib.rs", "fn a() {}\n");
        let vault = temp_dir("vault");
        let ws = WorkspaceEntry {
            id: "core:workspace:test01".into(),
            name: "Demo".into(),
            path: workspace.to_string_lossy().to_string(),
        };
        let state = test_state(&vault);
        fs::create_dir_all(vault.join(base::DIR)).unwrap();

        let (report, changed) = {
            let conn = state.db.lock().unwrap();
            sync_base_inner(&state, &conn, &ws, &vault, None, &[]).unwrap()
        };
        assert_eq!(report.created, 4, "root, modules, modules/memory, modules/memory/crate");
        assert_eq!(report.units, 4);
        assert_eq!(changed.len(), 4);
        assert!(vault.join("base/codebase.md").is_file());
        assert!(vault.join("base/modules-memory.md").is_file());
        assert!(vault.join("base/modules-memory-crate.md").is_file());

        // Every document is an ordinary memory of type base, found by title.
        {
            let conn = state.db.lock().unwrap();
            let root = index::get_by_identifier(&conn, "Demo codebase", Some(&ws.id)).unwrap().unwrap();
            assert_eq!(root.kind.as_deref(), Some("base"));
            assert_eq!(root.rel_path, "base/codebase.md");
            let unit = index::get_by_identifier(&conn, "modules/memory", Some(&ws.id)).unwrap().unwrap();
            assert_eq!(unit.tags, vec!["base"]);
            // The unit links to the root by slug, the root to the unit.
            assert_eq!(unit.outgoing_links, 2, "Part of [[codebase]] + [[modules-memory-crate]]");
            assert_eq!(root.outgoing_links, 3);
        }

        // Second sync: nothing changes.
        let (again, changed) = {
            let conn = state.db.lock().unwrap();
            sync_base_inner(&state, &conn, &ws, &vault, None, &[]).unwrap()
        };
        assert_eq!((again.created, again.updated, again.unchanged), (0, 0, 4));
        assert!(changed.is_empty());

        // A description written into the file survives a sync that changes
        // the structure; the root listing picks up its summary.
        {
            let conn = state.db.lock().unwrap();
            let docs = load_base_docs(&conn, &vault, &ws.id).unwrap();
            let doc = &docs["modules/memory"];
            let described = base::set_description(&doc.body, &doc.meta.title, "The memory module.\n\nMore.");
            let described = base::set_file_description(&described, "module.json", None, "the manifest");
            save_base_doc(&conn, &state, &vault, &ws.id, &vault.join(&doc.meta.rel_path), &doc.fm, &described).unwrap();
        }
        touch(&workspace, "modules/memory/app/b.ts", "export {}\n");
        let (third, _) = {
            let conn = state.db.lock().unwrap();
            sync_base_inner(&state, &conn, &ws, &vault, None, &[]).unwrap()
        };
        assert_eq!(third.updated, 3, "the unit (new file), modules and the root (new summary)");
        let unit_text = fs::read_to_string(vault.join("base/modules-memory.md")).unwrap();
        assert!(unit_text.contains("# modules/memory\n\nThe memory module.\n\nMore.\n\n<!-- base:auto -->"));
        assert!(unit_text.contains("- `app/b.ts` (TypeScript, 1 lines)\n"));
        assert!(unit_text.contains("- `module.json` (JSON, 1 lines) — the manifest\n"));
        let root_text = fs::read_to_string(vault.join("base/codebase.md")).unwrap();
        assert!(root_text.contains("- [[modules-memory|modules/memory/]] — 5 files · 5 lines — The memory module.\n"));
        assert!(root_text.contains("  - [[modules-memory-crate|modules/memory/crate/]]"));

        // Path resolution: exact unit, containing unit + remainder, root.
        {
            let conn = state.db.lock().unwrap();
            let docs = load_base_docs(&conn, &vault, &ws.id).unwrap();
            assert_eq!(resolve_base_target(&docs, "modules/memory").unwrap(), ("modules/memory".into(), None));
            assert_eq!(
                resolve_base_target(&docs, "modules/memory/crate/src/lib.rs").unwrap(),
                ("modules/memory/crate".into(), Some("src/lib.rs".into()))
            );
            assert_eq!(resolve_base_target(&docs, "README.md").unwrap(), (".".into(), Some("README.md".into())));
        }

        // A forced directory becomes a unit; a vanished one is flagged, not deleted.
        let (forced, _) = {
            let conn = state.db.lock().unwrap();
            sync_base_inner(&state, &conn, &ws, &vault, None, &["modules/memory/app".to_string()]).unwrap()
        };
        assert_eq!(forced.created, 1);
        fs::remove_dir_all(workspace.join("modules/memory/crate")).unwrap();
        let (gone, _) = {
            let conn = state.db.lock().unwrap();
            sync_base_inner(&state, &conn, &ws, &vault, None, &[]).unwrap()
        };
        assert_eq!(gone.missing, 1);
        let crate_text = fs::read_to_string(vault.join("base/modules-memory-crate.md")).unwrap();
        assert!(crate_text.contains("base_missing: true"));
        assert!(crate_text.contains("_Directory not found in the workspace"));
        {
            let conn = state.db.lock().unwrap();
            let docs = load_base_docs(&conn, &vault, &ws.id).unwrap();
            assert!(base_entry("modules/memory/crate", &docs["modules/memory/crate"]).missing);
            assert!(docs.contains_key("modules/memory/app"));
        }

        fs::remove_dir_all(&workspace).unwrap();
        fs::remove_dir_all(&vault).unwrap();
    }
}
