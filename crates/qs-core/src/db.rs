//! `core.db` — the cross-module entity/link store, the settings table and the
//! event log. Schema per ARCHITECTURE.md §5.
//!
//! Modules own their real data in their own databases. This holds only what is
//! needed to find, name, link and route to a thing.

use crate::paths;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub const SCHEMA_VERSION: i64 = 1;

pub struct Db(pub Mutex<Connection>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// `<module>:<kind>:<ulid>`
    pub id: String,
    pub module: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Route that opens this thing, e.g. `/algo/backtests/01HXYZ`
    pub route: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub src: String,
    pub dst: String,
    /// `references` | `derived_from` | `annotates` | ...
    pub rel: String,
}

pub fn open() -> rusqlite::Result<Connection> {
    paths::ensure().ok();
    let conn = Connection::open(paths::core_db())?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    if let Err(e) = prune_events(&conn) {
        log::warn!("event log prune failed: {e}");
    }
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);

        CREATE TABLE IF NOT EXISTS entities (
          id          TEXT PRIMARY KEY,
          module      TEXT NOT NULL,
          kind        TEXT NOT NULL,
          title       TEXT NOT NULL,
          subtitle    TEXT,
          route       TEXT NOT NULL,
          icon        TEXT,
          updated_at  INTEGER NOT NULL,
          payload     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_entities_module_kind ON entities(module, kind);
        CREATE INDEX IF NOT EXISTS idx_entities_updated ON entities(updated_at DESC);

        CREATE TABLE IF NOT EXISTS links (
          src        TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
          dst        TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
          rel        TEXT NOT NULL,
          created_at INTEGER NOT NULL,
          PRIMARY KEY (src, dst, rel)
        );
        CREATE INDEX IF NOT EXISTS idx_links_dst ON links(dst, rel);

        CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
          title, subtitle, kind, module,
          content='entities', content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
          INSERT INTO entities_fts(rowid, title, subtitle, kind, module)
          VALUES (new.rowid, new.title, new.subtitle, new.kind, new.module);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
          INSERT INTO entities_fts(entities_fts, rowid, title, subtitle, kind, module)
          VALUES ('delete', old.rowid, old.title, old.subtitle, old.kind, old.module);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities BEGIN
          INSERT INTO entities_fts(entities_fts, rowid, title, subtitle, kind, module)
          VALUES ('delete', old.rowid, old.title, old.subtitle, old.kind, old.module);
          INSERT INTO entities_fts(rowid, title, subtitle, kind, module)
          VALUES (new.rowid, new.title, new.subtitle, new.kind, new.module);
        END;

        CREATE TABLE IF NOT EXISTS event_log (
          id       TEXT PRIMARY KEY,
          topic    TEXT NOT NULL,
          source   TEXT NOT NULL,
          ts       INTEGER NOT NULL,
          payload  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_event_log_ts ON event_log(ts DESC);

        CREATE TABLE IF NOT EXISTS settings (
          scope TEXT NOT NULL,
          key   TEXT NOT NULL,
          value TEXT NOT NULL,
          PRIMARY KEY (scope, key)
        );
        "#,
    )?;

    let current: Option<i64> = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| r.get(0))
        .optional()?;

    match current {
        None => {
            conn.execute("INSERT INTO schema_version (version) VALUES (?1)", params![SCHEMA_VERSION])?;
        }
        Some(v) if v < SCHEMA_VERSION => {
            // Migrations are forward-only and additive. Add steps here as the
            // schema grows; each bumps SCHEMA_VERSION.
            conn.execute("UPDATE schema_version SET version = ?1", params![SCHEMA_VERSION])?;
        }
        Some(_) => {}
    }

    Ok(())
}

// ── Entities ────────────────────────────────────────────────────────────

pub fn upsert_entity(conn: &Connection, e: &Entity) -> rusqlite::Result<()> {
    let ts = if e.updated_at > 0 { e.updated_at } else { now_ms() };
    conn.execute(
        r#"INSERT INTO entities (id, module, kind, title, subtitle, route, icon, updated_at, payload)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
           ON CONFLICT(id) DO UPDATE SET
             module=excluded.module, kind=excluded.kind, title=excluded.title,
             subtitle=excluded.subtitle, route=excluded.route, icon=excluded.icon,
             updated_at=excluded.updated_at, payload=excluded.payload"#,
        params![
            e.id,
            e.module,
            e.kind,
            e.title,
            e.subtitle,
            e.route,
            e.icon,
            ts,
            e.payload.as_ref().map(|p| p.to_string()),
        ],
    )?;
    Ok(())
}

pub fn delete_entity(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM entities WHERE id = ?1", params![id])?;
    Ok(())
}

fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
    let payload: Option<String> = row.get("payload")?;
    Ok(Entity {
        id: row.get("id")?,
        module: row.get("module")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        subtitle: row.get("subtitle")?,
        route: row.get("route")?,
        icon: row.get("icon")?,
        updated_at: row.get("updated_at")?,
        payload: payload.and_then(|p| serde_json::from_str(&p).ok()),
    })
}

/// Shared by [`list_entities`] and [`count_entities`] so a counter can never
/// disagree with the list it counts. `?1` = module, `?2` = kind, both optional.
const ENTITY_FILTER: &str = "(?1 IS NULL OR module = ?1) AND (?2 IS NULL OR kind = ?2)";

pub fn list_entities(
    conn: &Connection,
    module: Option<&str>,
    kind: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<Entity>> {
    let mut stmt = conn.prepare(&format!(
        r#"SELECT * FROM entities
           WHERE {ENTITY_FILTER}
           ORDER BY updated_at DESC LIMIT ?3"#
    ))?;
    let rows = stmt.query_map(params![module, kind, limit], row_to_entity)?;
    rows.collect()
}

/// How many entities match — for callers that want the number, not the rows.
pub fn count_entities(conn: &Connection, module: Option<&str>, kind: Option<&str>) -> rusqlite::Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM entities WHERE {ENTITY_FILTER}"),
        params![module, kind],
        |r| r.get(0),
    )
}

/// Command-palette search. Characters FTS5 would read as syntax are stripped
/// first, then every remaining word becomes a quoted prefix term.
pub fn search_entities(conn: &Connection, query: &str, limit: i64) -> rusqlite::Result<Vec<Entity>> {
    let cleaned: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return list_entities(conn, None, None, limit);
    }

    // split_whitespace collapses the runs of spaces the cleaning leaves behind:
    // one `* ` per space would emit a bare `*` token, an FTS5 syntax error.
    let fts_query = trimmed
        .split_whitespace()
        .map(|t| format!("\"{t}\"*"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut stmt = conn.prepare(
        r#"SELECT e.* FROM entities_fts f
           JOIN entities e ON e.rowid = f.rowid
           WHERE entities_fts MATCH ?1
           ORDER BY rank LIMIT ?2"#,
    )?;
    let rows = stmt.query_map(params![fts_query, limit], row_to_entity)?;
    rows.collect()
}

// ── Links ───────────────────────────────────────────────────────────────

pub fn link(conn: &Connection, l: &Link) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO links (src, dst, rel, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![l.src, l.dst, l.rel, now_ms()],
    )?;
    Ok(())
}

pub fn unlink(conn: &Connection, l: &Link) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM links WHERE src = ?1 AND dst = ?2 AND rel = ?3",
        params![l.src, l.dst, l.rel],
    )?;
    Ok(())
}

/// Entities linked from `id` (outgoing) or to `id` (incoming).
pub fn linked(conn: &Connection, id: &str, incoming: bool) -> rusqlite::Result<Vec<Entity>> {
    let sql = if incoming {
        "SELECT e.* FROM links l JOIN entities e ON e.id = l.src WHERE l.dst = ?1"
    } else {
        "SELECT e.* FROM links l JOIN entities e ON e.id = l.dst WHERE l.src = ?1"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![id], row_to_entity)?;
    rows.collect()
}

// ── Settings ────────────────────────────────────────────────────────────

pub fn get_setting(conn: &Connection, scope: &str, key: &str) -> rusqlite::Result<Option<serde_json::Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE scope = ?1 AND key = ?2",
            params![scope, key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn set_setting(conn: &Connection, scope: &str, key: &str, value: &serde_json::Value) -> rusqlite::Result<()> {
    conn.execute(
        r#"INSERT INTO settings (scope, key, value) VALUES (?1, ?2, ?3)
           ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value"#,
        params![scope, key, value.to_string()],
    )?;
    Ok(())
}

pub fn get_scope(conn: &Connection, scope: &str) -> rusqlite::Result<serde_json::Map<String, serde_json::Value>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE scope = ?1")?;
    let rows = stmt.query_map(params![scope], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut map = serde_json::Map::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, serde_json::from_str(&v).unwrap_or(serde_json::Value::Null));
    }
    Ok(map)
}

// ── Event log ───────────────────────────────────────────────────────────

/// How long an event stays in the log. It is for diagnosis, not for replay
/// (bus.rs), and the persisted topics include ones that fire per trade and per
/// agent tool call — without a cutoff the file grows across every session.
const EVENT_RETENTION_DAYS: i64 = 30;

/// How many appends pass between in-session sweeps. A startup sweep alone would
/// bound nothing here: the suite is tray-resident (`window.rs`), so one session
/// routinely spans weeks and the cutoff would first bite on the next launch.
const PRUNE_EVERY_APPENDS: u64 = 500;

static EVENTS_APPENDED: AtomicU64 = AtomicU64::new(0);

/// Drop events past the retention window. Called from [`open`] and every
/// `PRUNE_EVERY_APPENDS` rows from [`append_event`].
pub fn prune_events(conn: &Connection) -> rusqlite::Result<usize> {
    let cutoff = now_ms() - EVENT_RETENTION_DAYS * 24 * 60 * 60 * 1000;
    conn.execute("DELETE FROM event_log WHERE ts < ?1", params![cutoff])
}

pub fn append_event(
    conn: &Connection,
    id: &str,
    topic: &str,
    source: &str,
    ts: i64,
    payload: &serde_json::Value,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO event_log (id, topic, source, ts, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, topic, source, ts, payload.to_string()],
    )?;

    // The sweep rides along with the writes it bounds — see PRUNE_EVERY_APPENDS.
    // A failed prune is a diagnostic problem, never a reason to fail the append.
    if (EVENTS_APPENDED.fetch_add(1, Ordering::Relaxed) + 1) % PRUNE_EVERY_APPENDS == 0 {
        if let Err(e) = prune_events(conn) {
            log::warn!("event log prune failed: {e}");
        }
    }
    Ok(())
}

pub fn recent_events(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare("SELECT id, topic, source, ts, payload FROM event_log ORDER BY ts DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit], |r| {
        let payload: String = r.get(4)?;
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "topic": r.get::<_, String>(1)?,
            "source": r.get::<_, String>(2)?,
            "ts": r.get::<_, i64>(3)?,
            "payload": serde_json::from_str::<serde_json::Value>(&payload).unwrap_or(serde_json::Value::Null),
        }))
    })?;
    rows.collect()
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
