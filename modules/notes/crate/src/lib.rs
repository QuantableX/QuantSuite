//! QuantNotes — a single-workspace, Notion-shaped note database.
//!
//! One database, `~/.quantsuite/modules/notes/notes.db`, holding one collection
//! of notes. There are no workspaces and no folder picking: the module owns its
//! storage the way every other suite module does (ARCHITECTURE.md §5).
//!
//! The model is Notion's, reduced to what a single collection needs:
//!
//!   * **notes** — the rows *and* the documents. A note has a title, an icon,
//!     a cover, a TipTap document, a parent (so the sidebar can show a tree)
//!     and a bag of property values.
//!   * **properties** — the collection's schema. Every note carries values for
//!     the same properties, keyed by property id in `properties_json`.
//!   * **views** — saved ways of rendering the same notes: table, board, list,
//!     gallery, calendar. A view owns its grouping, filters, sorts and which
//!     properties it shows; it never owns notes.
//!
//! Nothing here is per-view schema: a board view groups by a select property,
//! it does not have columns of its own. That is what makes "the same notes in
//! different views" true rather than three parallel data models.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Emitter, Manager, State, Wry,
};
use uuid::Uuid;

// ─── Types ────────────────────────────────────────────────────────────────

/// A note without its document. Everything a view needs to render a row —
/// including the property values, so a 500-row table is one call, not 501.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub icon: Option<String>,
    pub cover: Option<String>,
    pub properties: Value,
    pub word_count: i64,
    pub sort_index: f64,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(flatten)]
    pub summary: NoteSummary,
    pub content: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotePatch {
    pub title: Option<String>,
    pub icon: Option<Option<String>>,
    pub cover: Option<Option<String>>,
    pub parent_id: Option<Option<String>>,
    pub sort_index: Option<f64>,
    pub is_archived: Option<bool>,
}

/// One column of the collection. `kind` decides how a value is stored and
/// which editor a view renders; `config` carries the select options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: Value,
    pub sort_index: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: Value,
    pub sort_index: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub note_id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    pub note_id: String,
    pub title: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub font_size: i64,
    pub default_note_font: String,
    pub smart_quotes: bool,
    pub autoformat: bool,
    pub spellcheck_language: String,
    /// The view the collection opens on; empty means "the first one".
    pub default_view_id: String,
    pub sidebar_left_open: bool,
    pub sidebar_right_open: bool,
    pub focus_mode: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_size: 14,
            default_note_font: "sans".into(),
            smart_quotes: true,
            autoformat: true,
            spellcheck_language: "en-US".into(),
            default_view_id: String::new(),
            sidebar_left_open: true,
            sidebar_right_open: true,
            focus_mode: false,
        }
    }
}

pub struct AppState {
    data_dir: PathBuf,
    db: Arc<Mutex<Connection>>,
    app_settings: Mutex<AppSettings>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// `~/.quantsuite/modules/notes/` — the one place this module stores anything.
fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("notes")
}

fn db_path(base: &Path) -> PathBuf {
    base.join("notes.db")
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("config.json")
}

fn ensure_dirs(base: &Path) -> Result<(), String> {
    for sub in ["", "assets", "assets/covers", "assets/icons", "assets/attachments", "backups"] {
        let path = base.join(sub);
        fs::create_dir_all(&path).map_err(|e| format!("Create {}: {e}", path.display()))?;
    }
    Ok(())
}

fn empty_doc() -> Value {
    json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [] }] })
}

fn to_text(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("Serialize json: {e}"))
}

fn from_text(raw: Option<String>) -> Value {
    raw.and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}))
}

/// Every `text` leaf of a TipTap document, in order — the body the search
/// index matches on and the word count counts.
fn extract_text(value: &Value, sink: &mut String) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(t)) = map.get("text") {
                if !sink.is_empty() {
                    sink.push(' ');
                }
                sink.push_str(t);
            }
            for v in map.values() {
                extract_text(v, sink);
            }
        }
        Value::Array(items) => {
            for v in items {
                extract_text(v, sink);
            }
        }
        _ => {}
    }
}

fn plain_text(value: &Value) -> String {
    let mut buf = String::new();
    extract_text(value, &mut buf);
    buf
}

fn count_words(value: &Value) -> i64 {
    plain_text(value).split_whitespace().count() as i64
}

fn emit(app: &AppHandle, event: &str, payload: Value) {
    let _ = app.emit(event, payload);
}

fn db(state: &State<'_, AppState>) -> Arc<Mutex<Connection>> {
    state.db.clone()
}

// ─── Schema ───────────────────────────────────────────────────────────────

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES notes(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            icon TEXT,
            cover TEXT,
            content_json TEXT,
            properties_json TEXT NOT NULL DEFAULT '{}',
            word_count INTEGER NOT NULL DEFAULT 0,
            sort_index REAL NOT NULL DEFAULT 0,
            is_archived INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_notes_parent ON notes(parent_id);
        CREATE INDEX IF NOT EXISTS idx_notes_archived ON notes(is_archived);

        CREATE TABLE IF NOT EXISTS note_backlinks (
            source_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            PRIMARY KEY (source_id, target_id)
        );

        -- The collection's schema. `kind` is one of text | number | select |
        -- multi_select | date | checkbox | url; `config_json` carries the
        -- select options as [{ id, name, color }].
        CREATE TABLE IF NOT EXISTS properties (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            config_json TEXT NOT NULL DEFAULT '{}',
            sort_index REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );

        -- `kind` is table | board | list | gallery | calendar. `config_json`
        -- holds { groupBy, dateBy, filters, sorts, visible, cardSize } — all
        -- optional, all view-local.
        CREATE TABLE IF NOT EXISTS views (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            config_json TEXT NOT NULL DEFAULT '{}',
            sort_index REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL
        );

        -- Rebuilt lazily by sync_search_index() whenever a note's updated_at
        -- no longer matches the stamp the doc was derived from. The foreign
        -- key means a deleted note takes its index row with it.
        CREATE TABLE IF NOT EXISTS search_docs (
            doc_id INTEGER PRIMARY KEY,
            note_id TEXT NOT NULL UNIQUE REFERENCES notes(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            title_lc TEXT NOT NULL,
            body TEXT NOT NULL,
            body_lc TEXT NOT NULL,
            is_archived INTEGER NOT NULL DEFAULT 0,
            source_stamp TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Init schema: {e}"))
}

/// `settings` key marking the seed below as done. Without it, a user who
/// deletes a seeded property gets it back on the next launch.
const SEED_KEY: &str = "schema.seeded";

/// The schema a brand-new collection starts with: four properties and the five
/// views, so the module is usable the moment it opens rather than being an
/// empty table with no columns.
fn seed_schema(conn: &Connection) -> Result<(), String> {
    let seeded: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![SEED_KEY], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Read seed marker: {e}"))?;
    if seeded.is_some() {
        return Ok(());
    }

    let now = now_iso();
    let status_id = new_id();
    let tags_id = new_id();
    let due_id = new_id();
    let priority_id = new_id();

    let select_options = |names: &[(&str, &str)]| -> Value {
        json!({
            "options": names
                .iter()
                .map(|(name, color)| json!({ "id": new_id(), "name": name, "color": color }))
                .collect::<Vec<_>>()
        })
    };

    let properties: Vec<(&str, &str, &str, Value)> = vec![
        (
            status_id.as_str(),
            "Status",
            "select",
            select_options(&[("To do", "slate"), ("In progress", "blue"), ("Done", "green")]),
        ),
        (tags_id.as_str(), "Tags", "multi_select", json!({ "options": [] })),
        (due_id.as_str(), "Due", "date", json!({})),
        (
            priority_id.as_str(),
            "Priority",
            "select",
            select_options(&[("Low", "slate"), ("Medium", "amber"), ("High", "red")]),
        ),
    ];

    for (i, (id, name, kind, config)) in properties.iter().enumerate() {
        conn.execute(
            "INSERT INTO properties (id, name, kind, config_json, sort_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, kind, to_text(config)?, i as f64, now],
        )
        .map_err(|e| format!("Seed property: {e}"))?;
    }

    let visible = json!([status_id, tags_id, due_id, priority_id]);
    let views: Vec<(&str, &str, Value)> = vec![
        ("Table", "table", json!({ "visible": visible })),
        ("Board", "board", json!({ "groupBy": status_id, "visible": visible })),
        ("List", "list", json!({ "visible": [due_id, status_id] })),
        ("Gallery", "gallery", json!({ "visible": [status_id, tags_id], "cardSize": "medium" })),
        ("Calendar", "calendar", json!({ "dateBy": due_id, "visible": [status_id] })),
    ];

    for (i, (name, kind, config)) in views.iter().enumerate() {
        conn.execute(
            "INSERT INTO views (id, name, kind, config_json, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![new_id(), name, kind, to_text(config)?, i as f64, now],
        )
        .map_err(|e| format!("Seed view: {e}"))?;
    }

    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, ?2)",
        params![SEED_KEY, "true"],
    )
    .map_err(|e| format!("Write seed marker: {e}"))?;
    Ok(())
}

// ─── Row readers ──────────────────────────────────────────────────────────

const SUMMARY_COLUMNS: &str = "id, parent_id, title, icon, cover, properties_json, \
                               word_count, sort_index, is_archived, created_at, updated_at";

fn read_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    Ok(NoteSummary {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        title: row.get(2)?,
        icon: row.get(3)?,
        cover: row.get(4)?,
        properties: from_text(row.get::<_, Option<String>>(5)?),
        word_count: row.get(6)?,
        sort_index: row.get(7)?,
        is_archived: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn read_property(row: &rusqlite::Row<'_>) -> rusqlite::Result<Property> {
    Ok(Property {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        config: from_text(row.get::<_, Option<String>>(3)?),
        sort_index: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn read_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<View> {
    Ok(View {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        config: from_text(row.get::<_, Option<String>>(3)?),
        sort_index: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

// ─── Notes ────────────────────────────────────────────────────────────────

#[tauri::command(async)]
fn list_notes(include_archived: bool, state: State<'_, AppState>) -> Result<Vec<NoteSummary>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS} FROM notes {} ORDER BY sort_index, created_at",
        if include_archived { "" } else { "WHERE is_archived = 0" }
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare list notes: {e}"))?;
    let rows = stmt
        .query_map([], read_summary)
        .map_err(|e| format!("Query notes: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read notes: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn get_note(id: String, state: State<'_, AppState>) -> Result<Note, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let sql = format!("SELECT {SUMMARY_COLUMNS}, content_json FROM notes WHERE id = ?1");
    conn.query_row(&sql, params![id], |row| {
        Ok(Note {
            summary: read_summary(row)?,
            content: from_text(row.get::<_, Option<String>>(11)?),
        })
    })
    .map_err(|e| format!("Get note: {e}"))
}

#[tauri::command(async)]
fn create_note(
    title: Option<String>,
    parent_id: Option<String>,
    properties: Option<Value>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NoteSummary, String> {
    let summary = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let id = new_id();
        let title = title.unwrap_or_else(|| "Untitled".into());
        let props = properties.unwrap_or_else(|| json!({}));

        let next: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_index), -1) + 1 FROM notes WHERE parent_id IS ?1",
                params![parent_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("Next sort index: {e}"))?;

        conn.execute(
            "INSERT INTO notes (id, parent_id, title, icon, cover, content_json, properties_json,
                                word_count, sort_index, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, 0, ?6, 0, ?7, ?7)",
            params![id, parent_id, title, to_text(&empty_doc())?, to_text(&props)?, next, now],
        )
        .map_err(|e| format!("Create note: {e}"))?;

        NoteSummary {
            id,
            parent_id,
            title,
            icon: None,
            cover: None,
            properties: props,
            word_count: 0,
            sort_index: next,
            is_archived: false,
            created_at: now.clone(),
            updated_at: now,
        }
    };
    emit(&app, "note:created", json!({ "id": summary.id.clone() }));
    Ok(summary)
}

#[tauri::command(async)]
fn update_note_meta(
    id: String,
    patch: NotePatch,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NoteSummary, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();

        if let Some(title) = patch.title {
            conn.execute("UPDATE notes SET title = ?1, updated_at = ?2 WHERE id = ?3", params![title, now, id])
                .map_err(|e| format!("Update title: {e}"))?;
        }
        if let Some(icon) = patch.icon {
            conn.execute("UPDATE notes SET icon = ?1, updated_at = ?2 WHERE id = ?3", params![icon, now, id])
                .map_err(|e| format!("Update icon: {e}"))?;
        }
        if let Some(cover) = patch.cover {
            conn.execute("UPDATE notes SET cover = ?1, updated_at = ?2 WHERE id = ?3", params![cover, now, id])
                .map_err(|e| format!("Update cover: {e}"))?;
        }
        if let Some(parent) = patch.parent_id {
            conn.execute("UPDATE notes SET parent_id = ?1, updated_at = ?2 WHERE id = ?3", params![parent, now, id])
                .map_err(|e| format!("Update parent: {e}"))?;
        }
        if let Some(sort) = patch.sort_index {
            conn.execute("UPDATE notes SET sort_index = ?1, updated_at = ?2 WHERE id = ?3", params![sort, now, id])
                .map_err(|e| format!("Update sort index: {e}"))?;
        }
        if let Some(archived) = patch.is_archived {
            conn.execute(
                "UPDATE notes SET is_archived = ?1, updated_at = ?2 WHERE id = ?3",
                params![archived as i64, now, id],
            )
            .map_err(|e| format!("Update archived: {e}"))?;
        }
    }
    emit(&app, "note:updated", json!({ "id": id.clone() }));
    get_note(id, state).map(|n| n.summary)
}

/// Set one property value on one note. The whole `properties_json` bag is read
/// and rewritten under the lock, so two views editing different properties of
/// the same note cannot lose each other's write.
#[tauri::command(async)]
fn set_note_property(
    id: String,
    property_id: String,
    value: Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NoteSummary, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let raw: Option<String> = conn
            .query_row("SELECT properties_json FROM notes WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|e| format!("Read properties: {e}"))?;
        let mut props = match from_text(raw) {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        // Null clears rather than storing a null: an absent key is what every
        // view already treats as "empty", and it keeps the bags small.
        if value.is_null() {
            props.remove(&property_id);
        } else {
            props.insert(property_id, value);
        }
        conn.execute(
            "UPDATE notes SET properties_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![to_text(&Value::Object(props))?, now_iso(), id],
        )
        .map_err(|e| format!("Write properties: {e}"))?;
    }
    emit(&app, "note:updated", json!({ "id": id.clone() }));
    get_note(id, state).map(|n| n.summary)
}

#[tauri::command(async)]
fn save_note_content(
    id: String,
    content: Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NoteSummary, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute(
            "UPDATE notes SET content_json = ?1, word_count = ?2, updated_at = ?3 WHERE id = ?4",
            params![to_text(&content)?, count_words(&content), now_iso(), id],
        )
        .map_err(|e| format!("Save content: {e}"))?;
        rebuild_backlinks(&conn, &id, &content)?;
    }
    emit(&app, "note:updated", json!({ "id": id.clone() }));
    get_note(id, state).map(|n| n.summary)
}

/// Reorder and reparent in one call, so a drag in the sidebar tree is one write.
#[tauri::command(async)]
fn move_note(
    id: String,
    parent_id: Option<String>,
    sort_index: f64,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<NoteSummary, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        // A note cannot become its own ancestor; walking up from the target is
        // cheaper than any constraint SQLite could enforce for us, and the
        // cascade delete would otherwise take the whole cycle with it.
        let mut cursor = parent_id.clone();
        while let Some(current) = cursor {
            if current == id {
                return Err("A note cannot be moved inside itself".into());
            }
            cursor = conn
                .query_row("SELECT parent_id FROM notes WHERE id = ?1", params![current], |r| r.get(0))
                .optional()
                .map_err(|e| format!("Walk ancestors: {e}"))?
                .flatten();
        }
        conn.execute(
            "UPDATE notes SET parent_id = ?1, sort_index = ?2, updated_at = ?3 WHERE id = ?4",
            params![parent_id, sort_index, now_iso(), id],
        )
        .map_err(|e| format!("Move note: {e}"))?;
    }
    emit(&app, "note:updated", json!({ "id": id.clone() }));
    get_note(id, state).map(|n| n.summary)
}

#[tauri::command(async)]
fn archive_note(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    set_archived(&state, &id, true)?;
    emit(&app, "note:updated", json!({ "id": id }));
    Ok(true)
}

#[tauri::command(async)]
fn restore_note(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    set_archived(&state, &id, false)?;
    emit(&app, "note:updated", json!({ "id": id }));
    Ok(true)
}

/// Archiving a note archives its subtree — a child left visible under an
/// archived parent is unreachable in the tree yet still shows up in every view.
fn set_archived(state: &State<'_, AppState>, id: &str, archived: bool) -> Result<(), String> {
    let arc = db(state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    conn.execute(
        "WITH RECURSIVE subtree(id) AS (
             SELECT ?1
             UNION ALL
             SELECT n.id FROM notes n JOIN subtree s ON n.parent_id = s.id
         )
         UPDATE notes SET is_archived = ?2, updated_at = ?3 WHERE id IN (SELECT id FROM subtree)",
        params![id, archived as i64, now_iso()],
    )
    .map_err(|e| format!("Set archived: {e}"))?;
    Ok(())
}

#[tauri::command(async)]
fn delete_note(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete note: {e}"))?;
    }
    emit(&app, "note:deleted", json!({ "id": id }));
    Ok(true)
}

#[tauri::command(async)]
fn empty_trash(state: State<'_, AppState>, app: AppHandle) -> Result<usize, String> {
    let removed = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM notes WHERE is_archived = 1", [])
            .map_err(|e| format!("Empty trash: {e}"))?
    };
    emit(&app, "note:deleted", json!({ "id": Value::Null }));
    Ok(removed)
}

// ─── Backlinks ────────────────────────────────────────────────────────────

/// Collect every `noteMention` node id in a document. The editor writes them
/// as `{ type: "noteMention", attrs: { id } }`.
fn collect_mentions(value: &Value, sink: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("noteMention") {
                if let Some(id) = map.get("attrs").and_then(|a| a.get("id")).and_then(Value::as_str) {
                    sink.push(id.to_string());
                }
            }
            for v in map.values() {
                collect_mentions(v, sink);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_mentions(v, sink);
            }
        }
        _ => {}
    }
}

fn rebuild_backlinks(conn: &Connection, source_id: &str, content: &Value) -> Result<(), String> {
    let mut targets = Vec::new();
    collect_mentions(content, &mut targets);
    targets.sort();
    targets.dedup();

    conn.execute("DELETE FROM note_backlinks WHERE source_id = ?1", params![source_id])
        .map_err(|e| format!("Clear backlinks: {e}"))?;
    for target in targets {
        if target == source_id {
            continue;
        }
        // A mention of a note that has since been deleted is not an error: the
        // foreign key rejects the row and the link simply does not exist.
        let _ = conn.execute(
            "INSERT OR IGNORE INTO note_backlinks (source_id, target_id) VALUES (?1, ?2)",
            params![source_id, target],
        );
    }
    Ok(())
}

#[tauri::command(async)]
fn list_backlinks(id: String, state: State<'_, AppState>) -> Result<Vec<Backlink>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.icon
               FROM note_backlinks b JOIN notes n ON n.id = b.source_id
              WHERE b.target_id = ?1 AND n.is_archived = 0
              ORDER BY n.updated_at DESC",
        )
        .map_err(|e| format!("Prepare backlinks: {e}"))?;
    let rows = stmt
        .query_map(params![id], |row| {
            Ok(Backlink { note_id: row.get(0)?, title: row.get(1)?, icon: row.get(2)? })
        })
        .map_err(|e| format!("Query backlinks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read backlinks: {e}"))?;
    Ok(rows)
}

// ─── Properties ───────────────────────────────────────────────────────────

const PROPERTY_KINDS: [&str; 7] =
    ["text", "number", "select", "multi_select", "date", "checkbox", "url"];

#[tauri::command(async)]
fn list_properties(state: State<'_, AppState>) -> Result<Vec<Property>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT id, name, kind, config_json, sort_index, created_at FROM properties ORDER BY sort_index")
        .map_err(|e| format!("Prepare list properties: {e}"))?;
    let rows = stmt
        .query_map([], read_property)
        .map_err(|e| format!("Query properties: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read properties: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn create_property(
    name: String,
    kind: String,
    config: Option<Value>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Property, String> {
    if !PROPERTY_KINDS.contains(&kind.as_str()) {
        return Err(format!("Unknown property kind: {kind}"));
    }
    let property = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let id = new_id();
        let config = config.unwrap_or_else(|| {
            if kind == "select" || kind == "multi_select" {
                json!({ "options": [] })
            } else {
                json!({})
            }
        });
        let next: f64 = conn
            .query_row("SELECT COALESCE(MAX(sort_index), -1) + 1 FROM properties", [], |r| r.get(0))
            .map_err(|e| format!("Next property index: {e}"))?;
        conn.execute(
            "INSERT INTO properties (id, name, kind, config_json, sort_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, kind, to_text(&config)?, next, now],
        )
        .map_err(|e| format!("Create property: {e}"))?;
        Property { id, name, kind, config, sort_index: next, created_at: now }
    };
    emit(&app, "schema:changed", json!({ "what": "properties" }));
    Ok(property)
}

#[tauri::command(async)]
fn update_property(
    id: String,
    name: Option<String>,
    config: Option<Value>,
    sort_index: Option<f64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Property, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        if let Some(name) = name {
            conn.execute("UPDATE properties SET name = ?1 WHERE id = ?2", params![name, id])
                .map_err(|e| format!("Rename property: {e}"))?;
        }
        if let Some(config) = config {
            conn.execute("UPDATE properties SET config_json = ?1 WHERE id = ?2", params![to_text(&config)?, id])
                .map_err(|e| format!("Update property config: {e}"))?;
        }
        if let Some(sort) = sort_index {
            conn.execute("UPDATE properties SET sort_index = ?1 WHERE id = ?2", params![sort, id])
                .map_err(|e| format!("Reorder property: {e}"))?;
        }
    }
    emit(&app, "schema:changed", json!({ "what": "properties" }));

    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    conn.query_row(
        "SELECT id, name, kind, config_json, sort_index, created_at FROM properties WHERE id = ?1",
        params![id],
        read_property,
    )
    .map_err(|e| format!("Get property: {e}"))
}

/// Dropping a property drops its values too. Left in the notes' bags they are
/// dead weight in every row, and they would resurrect the column the moment
/// someone recreated a property with the same id.
#[tauri::command(async)]
fn delete_property(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        conn.execute("DELETE FROM properties WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete property: {e}"))?;
        conn.execute(
            "UPDATE notes SET properties_json = json_remove(properties_json, '$.' || ?1)
              WHERE json_extract(properties_json, '$.' || ?1) IS NOT NULL",
            params![id],
        )
        .map_err(|e| format!("Strip property values: {e}"))?;
    }
    emit(&app, "schema:changed", json!({ "what": "properties" }));
    Ok(true)
}

// ─── Views ────────────────────────────────────────────────────────────────

const VIEW_KINDS: [&str; 5] = ["table", "board", "list", "gallery", "calendar"];

#[tauri::command(async)]
fn list_views(state: State<'_, AppState>) -> Result<Vec<View>, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, kind, config_json, sort_index, created_at, updated_at
               FROM views ORDER BY sort_index",
        )
        .map_err(|e| format!("Prepare list views: {e}"))?;
    let rows = stmt
        .query_map([], read_view)
        .map_err(|e| format!("Query views: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read views: {e}"))?;
    Ok(rows)
}

#[tauri::command(async)]
fn create_view(
    name: String,
    kind: String,
    config: Option<Value>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<View, String> {
    if !VIEW_KINDS.contains(&kind.as_str()) {
        return Err(format!("Unknown view kind: {kind}"));
    }
    let view = {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        let id = new_id();
        let config = config.unwrap_or_else(|| json!({}));
        let next: f64 = conn
            .query_row("SELECT COALESCE(MAX(sort_index), -1) + 1 FROM views", [], |r| r.get(0))
            .map_err(|e| format!("Next view index: {e}"))?;
        conn.execute(
            "INSERT INTO views (id, name, kind, config_json, sort_index, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, name, kind, to_text(&config)?, next, now],
        )
        .map_err(|e| format!("Create view: {e}"))?;
        View { id, name, kind, config, sort_index: next, created_at: now.clone(), updated_at: now }
    };
    emit(&app, "schema:changed", json!({ "what": "views" }));
    Ok(view)
}

#[tauri::command(async)]
fn update_view(
    id: String,
    name: Option<String>,
    config: Option<Value>,
    sort_index: Option<f64>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<View, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let now = now_iso();
        if let Some(name) = name {
            conn.execute("UPDATE views SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, now, id])
                .map_err(|e| format!("Rename view: {e}"))?;
        }
        if let Some(config) = config {
            conn.execute(
                "UPDATE views SET config_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![to_text(&config)?, now, id],
            )
            .map_err(|e| format!("Update view config: {e}"))?;
        }
        if let Some(sort) = sort_index {
            conn.execute("UPDATE views SET sort_index = ?1, updated_at = ?2 WHERE id = ?3", params![sort, now, id])
                .map_err(|e| format!("Reorder view: {e}"))?;
        }
    }
    emit(&app, "schema:changed", json!({ "what": "views" }));

    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    conn.query_row(
        "SELECT id, name, kind, config_json, sort_index, created_at, updated_at FROM views WHERE id = ?1",
        params![id],
        read_view,
    )
    .map_err(|e| format!("Get view: {e}"))
}

#[tauri::command(async)]
fn delete_view(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<bool, String> {
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM views", [], |r| r.get(0))
            .map_err(|e| format!("Count views: {e}"))?;
        if count <= 1 {
            return Err("The collection needs at least one view".into());
        }
        conn.execute("DELETE FROM views WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete view: {e}"))?;
    }
    emit(&app, "schema:changed", json!({ "what": "views" }));
    Ok(true)
}

// ─── Search ───────────────────────────────────────────────────────────────

/// Bring `search_docs` back in line with `notes`.
///
/// A doc is rebuilt when its `source_stamp` — the note's `updated_at` at the
/// time the doc was derived — no longer matches, and disappears with its note
/// through `ON DELETE CASCADE`. Reconciling here, on every query rather than in
/// every write path, is what keeps the index from silently aging: a write that
/// forgets to touch it is repaired by the next search instead of leaving a note
/// findable that no longer exists, or lost that still does.
fn sync_search_index(conn: &Connection) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Begin search index tx: {e}"))?;
    let stale = {
        let mut stmt = tx
            .prepare(
                "SELECT n.id, n.title, n.content_json, n.is_archived, n.updated_at
                   FROM notes n LEFT JOIN search_docs d ON d.note_id = n.id
                  WHERE d.doc_id IS NULL OR d.source_stamp <> n.updated_at",
            )
            .map_err(|e| format!("Prepare stale docs: {e}"))?;
        // Bound to a local rather than left as the block's tail expression:
        // the tail's temporary outlives `stmt`, which still borrows `tx`.
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| format!("Query stale docs: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read stale docs: {e}"))?;
        rows
    };

    for (id, title, content, is_archived, stamp) in stale {
        let body = plain_text(&from_text(content));
        tx.execute(
            "INSERT INTO search_docs (note_id, title, title_lc, body, body_lc, is_archived, source_stamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(note_id) DO UPDATE SET
                 title = excluded.title, title_lc = excluded.title_lc,
                 body = excluded.body, body_lc = excluded.body_lc,
                 is_archived = excluded.is_archived, source_stamp = excluded.source_stamp",
            params![id, title, title.to_lowercase(), body, body.to_lowercase(), is_archived, stamp],
        )
        .map_err(|e| format!("Insert search doc: {e}"))?;
    }
    tx.commit().map_err(|e| format!("Commit search index tx: {e}"))
}

/// A window of the body around the first match, so a hit reads as a sentence
/// rather than as the first 120 characters of the note.
fn snippet_around(body: &str, needle: &str) -> String {
    let lower = body.to_lowercase();
    let Some(at) = lower.find(needle) else {
        return body.chars().take(120).collect();
    };
    let start = body[..at].char_indices().rev().nth(40).map(|(i, _)| i).unwrap_or(0);
    let end = body[at..].char_indices().nth(100).map(|(i, _)| at + i).unwrap_or(body.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(body[start..end].trim());
    if end < body.len() {
        out.push('…');
    }
    out
}

#[tauri::command(async)]
fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    sync_search_index(&conn)?;

    let pattern = format!("%{needle}%");
    let mut stmt = conn
        .prepare(
            "SELECT note_id, title, body
               FROM search_docs
              WHERE is_archived = 0 AND (title_lc LIKE ?1 OR body_lc LIKE ?1)
              ORDER BY CASE WHEN title_lc LIKE ?1 THEN 0 ELSE 1 END, title
              LIMIT 50",
        )
        .map_err(|e| format!("Prepare search: {e}"))?;
    let rows = stmt
        .query_map(params![pattern], |row| {
            let body: String = row.get(2)?;
            Ok(SearchHit {
                note_id: row.get(0)?,
                title: row.get(1)?,
                snippet: snippet_around(&body, &needle),
            })
        })
        .map_err(|e| format!("Query search: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read search hits: {e}"))?;
    Ok(rows)
}

// ─── Settings ─────────────────────────────────────────────────────────────

fn load_app_settings(base: &Path) -> AppSettings {
    fs::read_to_string(settings_path(base))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_app_settings(base: &Path, settings: &AppSettings) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(settings).map_err(|e| format!("Serialize settings: {e}"))?;
    qs_core::paths::write_atomic(&settings_path(base), raw.as_bytes()).map_err(|e| format!("Write settings: {e}"))
}

#[tauri::command(async)]
fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let guard = state.app_settings.lock().map_err(|e| format!("Lock settings: {e}"))?;
    Ok(guard.clone())
}

/// Async because it writes `config.json`: a sync command runs inline on the
/// main thread and a slow disk freezes the window (ARCHITECTURE.md §1). The
/// guard lives in its own scope so the future stays `Send`.
#[tauri::command]
async fn update_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let saved = {
        let mut guard = state.app_settings.lock().map_err(|e| format!("Lock settings: {e}"))?;
        *guard = settings;
        save_app_settings(&state.data_dir, &guard)?;
        guard.clone()
    };
    Ok(saved)
}

#[tauri::command(async)]
fn get_setting(key: String, state: State<'_, AppState>) -> Result<Value, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Get setting: {e}"))?;
    Ok(raw.map(|s| from_text(Some(s))).unwrap_or(Value::Null))
}

#[tauri::command(async)]
fn set_setting(key: String, value: Value, state: State<'_, AppState>) -> Result<bool, String> {
    let arc = db(&state);
    let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, to_text(&value)?],
    )
    .map_err(|e| format!("Set setting: {e}"))?;
    Ok(true)
}

// ─── Backup ───────────────────────────────────────────────────────────────

/// Async: copying the whole database is unbounded work, and on the main thread
/// it stalls the window for as long as the copy takes.
#[tauri::command]
async fn backup_database(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    let target = state
        .data_dir
        .join("backups")
        .join(format!("notes-{}.db", Utc::now().format("%Y%m%d-%H%M%S")));
    {
        let arc = db(&state);
        let conn = arc.lock().map_err(|e| format!("Lock db: {e}"))?;
        let mut dst = Connection::open(&target).map_err(|e| format!("Open backup target: {e}"))?;
        let backup =
            rusqlite::backup::Backup::new(&conn, &mut dst).map_err(|e| format!("Start backup: {e}"))?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(50), None)
            .map_err(|e| format!("Run backup: {e}"))?;
    }
    let path = target.to_string_lossy().to_string();
    let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    emit(&app, "backup:complete", json!({ "path": path.clone(), "size": size }));
    Ok(path)
}

// ─── Entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Build the `notes` plugin.
///
/// Pinned to `Wry` for the same reason as `systems`: bare `tauri::AppHandle`
/// already means `AppHandle<Wry>`. The dialog / fs / clipboard / shell plugins
/// are registered once by the suite binary, not here.
pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("notes")
        .setup(|app, _api| {
            let dir = data_dir();
            ensure_dirs(&dir).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let conn = Connection::open(db_path(&dir))?;
            // Foreign keys are per-connection and off by default; the archive
            // cascade and the search index's ON DELETE both depend on them.
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
            init_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            seed_schema(&conn).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let settings = load_app_settings(&dir);
            app.manage(AppState {
                data_dir: dir,
                db: Arc::new(Mutex::new(conn)),
                app_settings: Mutex::new(settings),
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "notes".into(),
                    label: "QuantNotes".into(),
                    route: "/notes".into(),
                    toggle_window: None,
                },
            );

            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_notes,
            get_note,
            create_note,
            update_note_meta,
            set_note_property,
            save_note_content,
            move_note,
            archive_note,
            restore_note,
            delete_note,
            empty_trash,
            list_backlinks,
            list_properties,
            create_property,
            update_property,
            delete_property,
            list_views,
            create_view,
            update_view,
            delete_view,
            search,
            get_setting,
            set_setting,
            get_app_settings,
            update_app_settings,
            backup_database,
        ]))
        .build()
}
