//! `pilot.db` — threads, their timeline and their turns
//! (docs/PLAN-QUANTPILOT.md §4.3).
//!
//! Schema v3 (`PRAGMA user_version = 3`): V2's `sessions` table is renamed
//! to `threads` and gains the V3 columns, so every V2 row — and the
//! provider session id it can resume — survives. V2 rows are `kind =
//! 'terminal'`; V3 threads are `kind = 'chat'`. `timeline` holds the items
//! the conversation view renders; `turns` one row per user message and
//! what came of it.

use crate::events::{Item, ItemKind};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

/// 3: threads, timeline, turns (card 2). 4: the follow-up queue (card 4).
/// 5: pin, order, settle, snooze (card 7). 6: workspace (card 10).
/// 7: checkpoints and turn changes (card 11). 8: links (card 12).
pub const SCHEMA_VERSION: i64 = 8;

/// A follow-up waiting for the running turn to end (docs/PLAN-QUANTPILOT.md, card 4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub thread_id: String,
    pub position: i64,
    pub text: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMeta {
    pub id: String,
    /// Adapter id: `claude`, `codex`, `pi`, `omp`, `opencode`, `gemini`, or a custom one.
    pub provider: String,
    /// What the CLI knows the conversation as (resume key).
    pub provider_session_id: Option<String>,
    pub title: String,
    pub context_id: String,
    pub context_name: String,
    pub cwd: String,
    pub model: String,
    /// `ask` | `edits` | `auto` | `plan` | `full`
    pub mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub last_error: Option<String>,
    /// `chat` (V3, rendered natively) | `terminal` (a V2 session row)
    pub kind: String,
    pub effort: String,
    /// `auto` until the user renames the thread, then `user`.
    pub title_source: String,
    /// `idle` | `running` | `waiting` | `error` | `interrupted` | `limited` — last known.
    pub state: String,
    pub context_tokens: Option<i64>,
    pub context_window: Option<i64>,
    /// The last time the agent answered or the user sent something.
    pub last_activity_at: i64,
    /// Kept above the active threads (card 7).
    pub pinned: bool,
    /// The user's order inside Pinned and Active: larger is higher. New
    /// threads take the current time, so they land on top.
    pub sort_key: f64,
    /// Out of the active list since then (manually or after inactivity).
    pub settled_at: Option<i64>,
    /// Hidden until then, unless it needs the user first.
    pub snoozed_until: Option<i64>,
    /// `false` keeps the thread out of automatic settlement (its menu).
    pub auto_settle: bool,
    /// `local` (the project's checkout) or `worktree` (its own, card 10).
    pub workspace_mode: String,
    /// What a worktree starts from; empty = the checkout's branch.
    pub base_branch: String,
    /// The worktree's branch — planned until the first message creates it.
    pub branch: String,
    /// Set once the worktree exists; `cwd` then points into it.
    pub worktree_path: Option<String>,
}

pub type SessionMeta = ThreadMeta;

/// What the sidebar changes on a thread; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrangement {
    pub pinned: Option<bool>,
    pub sort_key: Option<f64>,
    pub settled: Option<bool>,
    /// `Some(None)` wakes the thread.
    #[serde(default, deserialize_with = "some_option")]
    pub snoozed_until: Option<Option<i64>>,
    pub auto_settle: Option<bool>,
}

/// `null` in JSON is a value here ("wake"), not an absent field.
fn some_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

pub struct Database {
    conn: Mutex<Connection>,
}

const THREAD_COLS: &str = "id, provider, provider_session_id, title, context_id, context_name, cwd, model, mode, \
    created_at, updated_at, cost_usd, input_tokens, output_tokens, last_error, kind, effort, title_source, state, \
    context_tokens, context_window, last_activity_at, pinned, sort_key, settled_at, snoozed_until, auto_settle, workspace_mode, base_branch, \
    branch, worktree_path";

fn row_thread(r: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadMeta> {
    Ok(ThreadMeta {
        id: r.get(0)?,
        provider: r.get(1)?,
        provider_session_id: r.get(2)?,
        title: r.get(3)?,
        context_id: r.get(4)?,
        context_name: r.get(5)?,
        cwd: r.get(6)?,
        model: r.get(7)?,
        mode: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        cost_usd: r.get(11)?,
        input_tokens: r.get(12)?,
        output_tokens: r.get(13)?,
        last_error: r.get(14)?,
        kind: r.get(15)?,
        effort: r.get(16)?,
        title_source: r.get(17)?,
        state: r.get(18)?,
        context_tokens: r.get(19)?,
        context_window: r.get(20)?,
        last_activity_at: r.get(21)?,
        pinned: r.get::<_, i64>(22)? != 0,
        sort_key: r.get(23)?,
        settled_at: r.get(24)?,
        snoozed_until: r.get(25)?,
        auto_settle: r.get::<_, i64>(26)? != 0,
        workspace_mode: r.get(27)?,
        base_branch: r.get(28)?,
        branch: r.get(29)?,
        worktree_path: r.get(30)?,
    })
}

const ITEM_COLS: &str = "id, thread_id, turn_id, seq, kind, parent, data, created_at, updated_at";

fn row_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let kind: String = r.get(4)?;
    let data: String = r.get(6)?;
    Ok(Item {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        turn_id: r.get(2)?,
        seq: r.get(3)?,
        kind: ItemKind::parse(&kind).unwrap_or(ItemKind::Notice),
        parent: r.get(5)?,
        data: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnRow {
    pub id: String,
    pub thread_id: String,
    pub status: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

const THREADS_TABLE: &str = "CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_session_id TEXT,
    title TEXT NOT NULL DEFAULT '',
    context_id TEXT NOT NULL,
    context_name TEXT NOT NULL DEFAULT '',
    cwd TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    mode TEXT NOT NULL DEFAULT 'full',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    cost_usd REAL NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    kind TEXT NOT NULL DEFAULT 'chat',
    effort TEXT NOT NULL DEFAULT '',
    title_source TEXT NOT NULL DEFAULT 'auto',
    state TEXT NOT NULL DEFAULT 'idle',
    context_tokens INTEGER,
    context_window INTEGER,
    last_activity_at INTEGER NOT NULL DEFAULT 0,
    queue_held INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    sort_key REAL NOT NULL DEFAULT 0,
    settled_at INTEGER,
    snoozed_until INTEGER,
    auto_settle INTEGER NOT NULL DEFAULT 1,
    unsettled_at INTEGER,
    workspace_mode TEXT NOT NULL DEFAULT 'local',
    base_branch TEXT NOT NULL DEFAULT '',
    branch TEXT NOT NULL DEFAULT '',
    worktree_path TEXT
)";

/// Card 11 (schema v7): checkpoints per turn, the files a turn's tools
/// wrote, the changes they made, and a pending rewind per thread.
const CHANGE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS checkpoints (
        turn_id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL,
        root TEXT NOT NULL,
        before_commit TEXT NOT NULL,
        after_commit TEXT
    );
    CREATE INDEX IF NOT EXISTS checkpoints_thread ON checkpoints (thread_id);
    CREATE TABLE IF NOT EXISTS turn_files (
        turn_id TEXT NOT NULL,
        path TEXT NOT NULL,
        PRIMARY KEY (turn_id, path)
    );
    CREATE TABLE IF NOT EXISTS turn_changes (
        turn_id TEXT NOT NULL,
        thread_id TEXT NOT NULL,
        path TEXT NOT NULL,
        status TEXT NOT NULL,
        added INTEGER NOT NULL DEFAULT 0,
        removed INTEGER NOT NULL DEFAULT 0,
        binary INTEGER NOT NULL DEFAULT 0,
        patch TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (turn_id, path)
    );";

/// What a thread links to (card 12: pull requests; card 21 adds board cards).
const LINK_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS links (
        thread_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        url TEXT NOT NULL,
        number INTEGER NOT NULL DEFAULT 0,
        title TEXT NOT NULL DEFAULT '',
        state TEXT NOT NULL DEFAULT 'open',
        checked_at INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (thread_id, url)
    );";

/// Columns the branch toolbar (card 10, schema v6) adds.
const V6_COLUMNS: [(&str, &str); 4] = [
    ("workspace_mode", "workspace_mode TEXT NOT NULL DEFAULT 'local'"),
    ("base_branch", "base_branch TEXT NOT NULL DEFAULT ''"),
    ("branch", "branch TEXT NOT NULL DEFAULT ''"),
    ("worktree_path", "worktree_path TEXT"),
];

/// Columns the sidebar (card 7, schema v5) adds to an older `threads` table.
const V5_COLUMNS: [(&str, &str); 6] = [
    ("pinned", "pinned INTEGER NOT NULL DEFAULT 0"),
    ("sort_key", "sort_key REAL NOT NULL DEFAULT 0"),
    ("settled_at", "settled_at INTEGER"),
    ("snoozed_until", "snoozed_until INTEGER"),
    ("auto_settle", "auto_settle INTEGER NOT NULL DEFAULT 1"),
    // Set by Un-settle: no automatic settlement until the user writes again.
    ("unsettled_at", "unsettled_at INTEGER"),
];

const QUEUE_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS queue (
        id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL,
        position INTEGER NOT NULL,
        text TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS queue_thread ON queue (thread_id, position);";

fn column_exists(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for n in names {
        if n? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Columns V3 adds to a V2 `sessions` table, with their defaults for V2 rows.
const V3_COLUMNS: [&str; 7] = [
    "kind TEXT NOT NULL DEFAULT 'terminal'",
    "effort TEXT NOT NULL DEFAULT ''",
    "title_source TEXT NOT NULL DEFAULT 'auto'",
    "state TEXT NOT NULL DEFAULT 'idle'",
    "context_tokens INTEGER",
    "context_window INTEGER",
    "last_activity_at INTEGER NOT NULL DEFAULT 0",
];

const TIMELINE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS timeline (
        id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL,
        turn_id TEXT NOT NULL,
        seq INTEGER NOT NULL,
        kind TEXT NOT NULL,
        parent TEXT,
        data TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS timeline_thread_seq ON timeline (thread_id, seq);
    CREATE TABLE IF NOT EXISTS turns (
        id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL,
        status TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER,
        duration_ms INTEGER,
        cost_usd REAL,
        error TEXT
    );
    CREATE INDEX IF NOT EXISTS turns_thread ON turns (thread_id, started_at);";

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", params![name], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n > 0)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    if table_exists(&tx, "sessions")? && !table_exists(&tx, "threads")? {
        tx.execute_batch("ALTER TABLE sessions RENAME TO threads;")?;
        for col in V3_COLUMNS {
            tx.execute_batch(&format!("ALTER TABLE threads ADD COLUMN {col};"))?;
        }
        tx.execute_batch("UPDATE threads SET last_activity_at = updated_at;")?;
    }
    tx.execute_batch(THREADS_TABLE)?;
    tx.execute_batch(TIMELINE_TABLES)?;
    if !column_exists(&tx, "threads", "queue_held")? {
        tx.execute_batch("ALTER TABLE threads ADD COLUMN queue_held INTEGER NOT NULL DEFAULT 0;")?;
    }
    tx.execute_batch(QUEUE_TABLE)?;
    let mut added_order = false;
    for (name, col) in V5_COLUMNS {
        if !column_exists(&tx, "threads", name)? {
            tx.execute_batch(&format!("ALTER TABLE threads ADD COLUMN {col};"))?;
            added_order |= name == "sort_key";
        }
    }
    for (name, col) in V6_COLUMNS {
        if !column_exists(&tx, "threads", name)? {
            tx.execute_batch(&format!("ALTER TABLE threads ADD COLUMN {col};"))?;
        }
    }
    tx.execute_batch(CHANGE_TABLES)?;
    tx.execute_batch(LINK_TABLE)?;
    if !column_exists(&tx, "turns", "provider_ref")? {
        tx.execute_batch("ALTER TABLE turns ADD COLUMN provider_ref TEXT;")?;
    }
    if !column_exists(&tx, "threads", "rewind")? {
        tx.execute_batch("ALTER TABLE threads ADD COLUMN rewind TEXT;")?;
    }
    if added_order {
        // Until the user drags, the order is the order of activity.
        tx.execute_batch("UPDATE threads SET sort_key = last_activity_at;")?;
    }
    // V1's transcript copy and key-value table, gone since V2.
    tx.execute_batch("DROP TABLE IF EXISTS items; DROP TABLE IF EXISTS kv;")?;
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    tx.commit()
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        migrate(&conn)?;
        let db = Self { conn: Mutex::new(conn) };
        // Nothing runs before the app does: what was live is over.
        db.settle_after_restart()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A process that was running when the app ended cannot answer a
    /// request any more, and its turn did not finish.
    fn settle_after_restart(&self) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute_batch(
            "UPDATE timeline SET data = json_set(data, '$.status', 'expired')
                WHERE kind IN ('approval', 'question', 'plan') AND json_extract(data, '$.status') = 'pending';
             UPDATE timeline SET data = json_set(data, '$.status', 'failed', '$.isError', json('true'))
                WHERE kind = 'tool' AND json_extract(data, '$.status') = 'running';
             UPDATE timeline SET data = json_set(data, '$.streaming', json('false'))
                WHERE kind IN ('assistant', 'reasoning') AND json_extract(data, '$.streaming') = 1;
             UPDATE turns SET status = 'interrupted' WHERE status = 'started';
             UPDATE threads SET state = 'interrupted' WHERE state IN ('running', 'waiting', 'starting');
             UPDATE threads SET queue_held = 1 WHERE id IN (SELECT thread_id FROM queue);",
        )
    }

    // ── Threads ──

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<ThreadMeta>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {THREAD_COLS} FROM threads ORDER BY updated_at DESC"))?;
        let rows = stmt.query_map([], row_thread)?;
        rows.collect()
    }

    pub fn get_session(&self, id: &str) -> rusqlite::Result<Option<ThreadMeta>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {THREAD_COLS} FROM threads WHERE id = ?1"), params![id], row_thread)
            .optional()
    }

    pub fn insert_session(&self, m: &ThreadMeta) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            &format!(
                "INSERT INTO threads ({THREAD_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, \
                 ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)"
            ),
            params![
                m.id,
                m.provider,
                m.provider_session_id,
                m.title,
                m.context_id,
                m.context_name,
                m.cwd,
                m.model,
                m.mode,
                m.created_at,
                m.updated_at,
                m.cost_usd,
                m.input_tokens,
                m.output_tokens,
                m.last_error,
                m.kind,
                m.effort,
                m.title_source,
                m.state,
                m.context_tokens,
                m.context_window,
                m.last_activity_at,
                m.pinned as i64,
                m.sort_key,
                m.settled_at,
                m.snoozed_until,
                m.auto_settle as i64,
                m.workspace_mode,
                m.base_branch,
                m.branch,
                m.worktree_path
            ],
        )?;
        Ok(())
    }

    /// Pin, order, settle, snooze (card 7). Settling unpins (T3 Code);
    /// un-settling puts the thread back on top of the active list and keeps
    /// it from settling again until the user writes in it.
    pub fn arrange(&self, id: &str, a: &Arrangement) -> rusqlite::Result<()> {
        let conn = self.lock();
        if let Some(p) = a.pinned {
            conn.execute("UPDATE threads SET pinned = ?2 WHERE id = ?1", params![id, p as i64])?;
        }
        if let Some(k) = a.sort_key {
            conn.execute("UPDATE threads SET sort_key = ?2 WHERE id = ?1", params![id, k])?;
        }
        match a.settled {
            Some(true) => {
                conn.execute("UPDATE threads SET settled_at = ?2, pinned = 0 WHERE id = ?1", params![id, now()])?;
            }
            // A drop or an undo names the spot; a plain Un-settle goes on top.
            Some(false) if a.sort_key.is_some() => {
                conn.execute("UPDATE threads SET settled_at = NULL, unsettled_at = ?2 WHERE id = ?1", params![id, now()])?;
            }
            Some(false) => {
                conn.execute(
                    "UPDATE threads SET settled_at = NULL, unsettled_at = ?2,
                        sort_key = (SELECT COALESCE(MAX(sort_key), 0) + 1 FROM threads) WHERE id = ?1",
                    params![id, now()],
                )?;
            }
            None => {}
        }
        if let Some(s) = a.snoozed_until {
            conn.execute("UPDATE threads SET snoozed_until = ?2 WHERE id = ?1", params![id, s])?;
        }
        if let Some(on) = a.auto_settle {
            conn.execute("UPDATE threads SET auto_settle = ?2 WHERE id = ?1", params![id, on as i64])?;
        }
        Ok(())
    }

    /// Settle threads nobody touched for `days` (0 = never). Threads that
    /// run, wait on the user or have queued follow-ups stay, and so do those
    /// with auto-settle off or un-settled since the user last wrote. Pinning
    /// does not prevent it; settling unpins (T3 Code).
    pub fn auto_settle(&self, days: u32, busy: &[String]) -> rusqlite::Result<Vec<String>> {
        if days == 0 {
            return Ok(Vec::new());
        }
        let cutoff = now() - i64::from(days) * 24 * 3600 * 1000;
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id FROM threads WHERE settled_at IS NULL AND kind = 'chat' AND auto_settle = 1
                AND unsettled_at IS NULL AND MAX(last_activity_at, created_at) < ?1 AND state != 'waiting'
                AND id NOT IN (SELECT thread_id FROM queue)",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![cutoff], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .into_iter()
            .filter(|id| !busy.contains(id))
            .collect();
        for id in &ids {
            conn.execute("UPDATE threads SET settled_at = ?2, pinned = 0 WHERE id = ?1", params![id, now()])?;
        }
        Ok(ids)
    }

    /// Local or Worktree, the base and the planned branch — before the first message.
    pub fn set_workspace(&self, id: &str, mode: &str, base: &str, branch: &str, worktree: Option<&str>, cwd: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET workspace_mode = ?2, base_branch = ?3, branch = ?4, worktree_path = ?5,
                cwd = COALESCE(?6, cwd), updated_at = ?7 WHERE id = ?1",
            params![id, mode, base, branch, worktree, cwd, now()],
        )?;
        Ok(())
    }

    /// The newest worktree of a repository a thread worked in (Reuse previous worktree).
    pub fn latest_worktree(&self, under: &str) -> rusqlite::Result<Option<ThreadMeta>> {
        let conn = self.lock();
        conn.query_row(
            &format!(
                "SELECT {THREAD_COLS} FROM threads WHERE worktree_path IS NOT NULL AND lower(worktree_path) LIKE lower(?1) || '%'
                    ORDER BY last_activity_at DESC LIMIT 1"
            ),
            params![under],
            row_thread,
        )
        .optional()
    }

    /// The user wrote in the thread: it is live work again — out of the
    /// settled and snoozed shelves, and under the usual settle rules.
    pub fn resume(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET settled_at = NULL, snoozed_until = NULL, unsettled_at = NULL WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// A regenerated title replaces any earlier one, the user's included.
    pub fn set_title_regenerated(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET title = ?2, title_source = 'auto', updated_at = ?3 WHERE id = ?1",
            params![id, title, now()],
        )?;
        Ok(())
    }

    /// The first thing the user asked and the latest answer — for a title.
    pub fn title_material(&self, id: &str) -> rusqlite::Result<(String, String)> {
        let conn = self.lock();
        let first: Option<String> = conn
            .query_row(
                "SELECT json_extract(data, '$.text') FROM timeline WHERE thread_id = ?1 AND kind = 'user' ORDER BY seq LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let last: Option<String> = conn
            .query_row(
                "SELECT json_extract(data, '$.text') FROM timeline WHERE thread_id = ?1 AND kind = 'assistant'
                    AND parent IS NULL ORDER BY seq DESC LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok((first.unwrap_or_default(), last.unwrap_or_default()))
    }

    pub fn set_provider_session(&self, id: &str, provider_session_id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET provider_session_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, provider_session_id, now()],
        )?;
        Ok(())
    }

    /// The adapter the row belongs to now. A different one than before
    /// forgets the old provider session: the new CLI starts its own.
    pub fn set_provider(&self, id: &str, provider: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let current: Option<String> = conn
            .query_row("SELECT provider FROM threads WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        let changed = current.as_deref() != Some(provider);
        if changed {
            conn.execute(
                "UPDATE threads SET provider = ?2, provider_session_id = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, provider, now()],
            )?;
        }
        Ok(changed)
    }

    pub fn set_title(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, title, now()])?;
        Ok(())
    }

    /// `source` = `auto` (first prompt, the agent) or `user` (a rename, which wins from then on).
    pub fn set_title_from(&self, id: &str, title: &str, source: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE threads SET title = ?2, title_source = ?3, updated_at = ?4
                WHERE id = ?1 AND (?3 = 'user' OR title_source != 'user')",
            params![id, title, source, now()],
        )?;
        Ok(n > 0)
    }

    /// A V2 terminal row continues as a chat thread with the same provider session.
    pub fn make_chat(&self, id: &str, provider: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET kind = 'chat', provider = ?2 WHERE id = ?1", params![id, provider])?;
        Ok(())
    }

    pub fn set_model(&self, id: &str, model: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET model = ?2 WHERE id = ?1", params![id, model])?;
        Ok(())
    }

    pub fn set_mode(&self, id: &str, mode: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET mode = ?2 WHERE id = ?1", params![id, mode])?;
        Ok(())
    }

    pub fn set_effort(&self, id: &str, effort: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET effort = ?2 WHERE id = ?1", params![id, effort])?;
        Ok(())
    }

    pub fn set_state(&self, id: &str, state: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET state = ?2 WHERE id = ?1", params![id, state])?;
        Ok(())
    }

    /// Absolute totals — V2's sources report cumulative usage.
    pub fn set_usage(&self, id: &str, input: Option<i64>, output: Option<i64>, cost: Option<f64>) -> rusqlite::Result<()> {
        let conn = self.lock();
        if let Some(i) = input {
            conn.execute("UPDATE threads SET input_tokens = ?2 WHERE id = ?1", params![id, i])?;
        }
        if let Some(o) = output {
            conn.execute("UPDATE threads SET output_tokens = ?2 WHERE id = ?1", params![id, o])?;
        }
        if let Some(c) = cost {
            conn.execute("UPDATE threads SET cost_usd = ?2 WHERE id = ?1", params![id, c])?;
        }
        Ok(())
    }

    /// One turn's usage added to the thread's totals (V3 drivers report per turn).
    pub fn add_usage(&self, id: &str, input: Option<i64>, output: Option<i64>, cost: Option<f64>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET input_tokens = input_tokens + ?2, output_tokens = output_tokens + ?3,
                cost_usd = cost_usd + ?4 WHERE id = ?1",
            params![id, input.unwrap_or(0), output.unwrap_or(0), cost.unwrap_or(0.0)],
        )?;
        Ok(())
    }

    pub fn set_context(&self, id: &str, tokens: Option<i64>, window: Option<i64>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE threads SET context_tokens = COALESCE(?2, context_tokens), context_window = COALESCE(?3, context_window)
                WHERE id = ?1",
            params![id, tokens, window],
        )?;
        Ok(())
    }

    pub fn set_error(&self, id: &str, error: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET last_error = ?2 WHERE id = ?1", params![id, error])?;
        Ok(())
    }

    pub fn touch(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        let t = now();
        conn.execute("UPDATE threads SET updated_at = ?2, last_activity_at = ?2 WHERE id = ?1", params![id, t])?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM timeline WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM turn_files WHERE turn_id IN (SELECT id FROM turns WHERE thread_id = ?1)", params![id])?;
        conn.execute("DELETE FROM turn_changes WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM checkpoints WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM turns WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM queue WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM links WHERE thread_id = ?1", params![id])?;
        conn.execute("DELETE FROM threads WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Checkpoints and turn changes (card 11) ──

    pub fn set_checkpoint(&self, turn_id: &str, thread_id: &str, root: &str, before: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT OR REPLACE INTO checkpoints (turn_id, thread_id, root, before_commit) VALUES (?1, ?2, ?3, ?4)",
            params![turn_id, thread_id, root, before],
        )?;
        Ok(())
    }

    pub fn finish_checkpoint(&self, turn_id: &str, after: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE checkpoints SET after_commit = ?2 WHERE turn_id = ?1", params![turn_id, after])?;
        Ok(())
    }

    /// `(root, before, after)` of a turn's checkpoints.
    pub fn checkpoint(&self, turn_id: &str) -> rusqlite::Result<Option<(String, String, Option<String>)>> {
        let conn = self.lock();
        conn.query_row(
            "SELECT root, before_commit, after_commit FROM checkpoints WHERE turn_id = ?1",
            params![turn_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
    }

    pub fn add_turn_file(&self, turn_id: &str, path: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("INSERT OR IGNORE INTO turn_files (turn_id, path) VALUES (?1, ?2)", params![turn_id, path])?;
        Ok(())
    }

    pub fn turn_files(&self, turn_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path FROM turn_files WHERE turn_id = ?1 ORDER BY path")?;
        let rows = stmt.query_map(params![turn_id], |r| r.get(0))?;
        rows.collect()
    }

    pub fn save_changes(&self, turn_id: &str, thread_id: &str, changes: &[(crate::checkpoints::FileChange, String)]) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM turn_changes WHERE turn_id = ?1", params![turn_id])?;
        for (c, patch) in changes {
            conn.execute(
                "INSERT INTO turn_changes (turn_id, thread_id, path, status, added, removed, binary, patch) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![turn_id, thread_id, c.path, c.status, c.added, c.removed, c.binary as i64, patch],
            )?;
        }
        Ok(())
    }

    pub fn turn_changes(&self, turn_id: &str) -> rusqlite::Result<Vec<crate::checkpoints::FileChange>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT path, status, added, removed, binary FROM turn_changes WHERE turn_id = ?1 ORDER BY path")?;
        let rows = stmt.query_map(params![turn_id], |r| {
            Ok(crate::checkpoints::FileChange { path: r.get(0)?, status: r.get(1)?, added: r.get(2)?, removed: r.get(3)?, binary: r.get::<_, i64>(4)? != 0 })
        })?;
        rows.collect()
    }

    pub fn set_provider_ref(&self, turn_id: &str, reference: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE turns SET provider_ref = ?2 WHERE id = ?1", params![turn_id, reference])?;
        Ok(())
    }

    /// A thread's turns, oldest first: `(id, provider_ref)`.
    pub fn turn_refs(&self, thread_id: &str) -> rusqlite::Result<Vec<(String, Option<String>)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, provider_ref FROM turns WHERE thread_id = ?1 ORDER BY started_at, rowid")?;
        let rows = stmt.query_map(params![thread_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Rewinding: the turn and every later one leave the timeline.
    pub fn drop_turns_from(&self, thread_id: &str, turn_ids: &[String]) -> rusqlite::Result<()> {
        let conn = self.lock();
        for t in turn_ids {
            conn.execute("DELETE FROM timeline WHERE thread_id = ?1 AND turn_id = ?2", params![thread_id, t])?;
            conn.execute("DELETE FROM turn_files WHERE turn_id = ?1", params![t])?;
            conn.execute("DELETE FROM turn_changes WHERE turn_id = ?1", params![t])?;
            conn.execute("DELETE FROM checkpoints WHERE turn_id = ?1", params![t])?;
            conn.execute("DELETE FROM turns WHERE id = ?1", params![t])?;
        }
        Ok(())
    }

    /// A rewind to before the first message: the next turn starts a new session.
    pub fn clear_provider_session(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET provider_session_id = NULL WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Links (card 12) ──

    /// Add a link, or refresh its title and state when the thread has it already.
    pub fn add_link(&self, l: &crate::gitops::Link) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO links (thread_id, kind, url, number, title, state, checked_at, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (thread_id, url) DO UPDATE SET
                title = CASE WHEN excluded.title = '' THEN links.title ELSE excluded.title END,
                state = excluded.state, checked_at = excluded.checked_at",
            params![l.thread_id, l.kind, l.url, l.number, l.title, l.state, l.checked_at, l.created_at],
        )?;
        Ok(())
    }

    /// One thread's links, or every thread's.
    pub fn links(&self, thread_id: Option<&str>) -> rusqlite::Result<Vec<crate::gitops::Link>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT thread_id, kind, url, number, title, state, checked_at, created_at FROM links
                WHERE ?1 IS NULL OR thread_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![thread_id], |r| {
            Ok(crate::gitops::Link {
                thread_id: r.get(0)?,
                kind: r.get(1)?,
                url: r.get(2)?,
                number: r.get(3)?,
                title: r.get(4)?,
                state: r.get(5)?,
                checked_at: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// What the next launch rewinds to (JSON), or nothing.
    pub fn set_rewind(&self, thread_id: &str, rewind: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET rewind = ?2 WHERE id = ?1", params![thread_id, rewind])?;
        Ok(())
    }

    pub fn rewind(&self, thread_id: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.lock();
        conn.query_row("SELECT rewind FROM threads WHERE id = ?1", params![thread_id], |r| r.get(0)).optional().map(Option::flatten)
    }

    // ── Timeline ──

    pub fn next_seq(&self, thread_id: &str) -> rusqlite::Result<i64> {
        let conn = self.lock();
        conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM timeline WHERE thread_id = ?1", params![thread_id], |r| {
            r.get(0)
        })
    }

    pub fn upsert_item(&self, item: &Item) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            &format!(
                "INSERT INTO timeline ({ITEM_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data, parent = excluded.parent,
                    updated_at = excluded.updated_at"
            ),
            params![
                item.id,
                item.thread_id,
                item.turn_id,
                item.seq,
                item.kind.as_str(),
                item.parent,
                item.data.to_string(),
                item.created_at,
                item.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_item(&self, id: &str) -> rusqlite::Result<Option<Item>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {ITEM_COLS} FROM timeline WHERE id = ?1"), params![id], row_item)
            .optional()
    }

    /// The newest `limit` items before `before_seq` (all when `None`), oldest first.
    pub fn items(&self, thread_id: &str, before_seq: Option<i64>, limit: i64) -> rusqlite::Result<Vec<Item>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM timeline WHERE thread_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![thread_id, before_seq.unwrap_or(i64::MAX), limit], row_item)?;
        let mut items: Vec<Item> = rows.collect::<rusqlite::Result<_>>()?;
        items.reverse();
        Ok(items)
    }

    /// Requests of a thread still marked pending — expired when their process is gone.
    pub fn expire_pending(&self, thread_id: &str) -> rusqlite::Result<Vec<Item>> {
        let ids: Vec<String> = {
            let conn = self.lock();
            let mut stmt = conn.prepare(
                "SELECT id FROM timeline WHERE thread_id = ?1 AND kind IN ('approval', 'question', 'plan')
                    AND json_extract(data, '$.status') = 'pending'
                    AND COALESCE(json_extract(data, '$.byMessage'), 0) = 0",
            )?;
            let rows = stmt.query_map(params![thread_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut changed = Vec::new();
        for id in ids {
            if let Some(mut item) = self.get_item(&id)? {
                item.data["status"] = "expired".into();
                item.updated_at = now();
                self.upsert_item(&item)?;
                changed.push(item);
            }
        }
        Ok(changed)
    }

    /// A request item by its request id (the newest, should one repeat).
    pub fn request_item(&self, thread_id: &str, request_id: &str) -> rusqlite::Result<Option<Item>> {
        let conn = self.lock();
        conn.query_row(
            &format!(
                "SELECT {ITEM_COLS} FROM timeline WHERE thread_id = ?1 AND kind IN ('approval', 'question', 'plan')
                    AND json_extract(data, '$.requestId') = ?2 ORDER BY seq DESC LIMIT 1"
            ),
            params![thread_id, request_id],
            row_item,
        )
        .optional()
    }

    /// One line for what the thread waits on — the newest pending request.
    pub fn pending_request_line(&self, thread_id: &str) -> Option<String> {
        let item = {
            let conn = self.lock();
            conn.query_row(
                &format!(
                    "SELECT {ITEM_COLS} FROM timeline WHERE thread_id = ?1 AND kind IN ('approval', 'question', 'plan')
                        AND json_extract(data, '$.status') = 'pending' ORDER BY seq DESC LIMIT 1"
                ),
                params![thread_id],
                row_item,
            )
            .optional()
            .ok()
            .flatten()?
        };
        let d = &item.data;
        let s = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        Some(match item.kind {
            ItemKind::Approval => {
                let tool = Some(s("toolName")).filter(|t| !t.is_empty()).unwrap_or_else(|| "a tool call".into());
                let title = s("title");
                if title.is_empty() || title == tool {
                    format!("Allow {tool}?")
                } else {
                    format!("Allow {tool}: {}?", title.chars().take(120).collect::<String>())
                }
            }
            ItemKind::Question => {
                let q = d["questions"][0]["question"].as_str().unwrap_or("A question for you");
                format!("Question: {}", q.chars().take(140).collect::<String>())
            }
            _ => "A plan is ready for your review".to_string(),
        })
    }

    // ── Queue ──

    pub fn queue(&self, thread_id: &str) -> rusqlite::Result<Vec<QueueItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, position, text, created_at, updated_at FROM queue
                WHERE thread_id = ?1 ORDER BY position, created_at",
        )?;
        let rows = stmt.query_map(params![thread_id], |r| {
            Ok(QueueItem {
                id: r.get(0)?,
                thread_id: r.get(1)?,
                position: r.get(2)?,
                text: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn queue_add(&self, thread_id: &str, text: &str) -> rusqlite::Result<QueueItem> {
        let conn = self.lock();
        let position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM queue WHERE thread_id = ?1",
            params![thread_id],
            |r| r.get(0),
        )?;
        let t = now();
        let item = QueueItem {
            id: uuid::Uuid::new_v4().to_string(),
            thread_id: thread_id.to_string(),
            position,
            text: text.to_string(),
            created_at: t,
            updated_at: t,
        };
        conn.execute(
            "INSERT INTO queue (id, thread_id, position, text, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item.id, item.thread_id, item.position, item.text, item.created_at, item.updated_at],
        )?;
        Ok(item)
    }

    /// The thread the item belongs to, or `None` when it is gone (already sent).
    pub fn queue_thread(&self, item_id: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.lock();
        conn.query_row("SELECT thread_id FROM queue WHERE id = ?1", params![item_id], |r| r.get(0))
            .optional()
    }

    pub fn queue_update(&self, item_id: &str, text: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let n = conn.execute("UPDATE queue SET text = ?2, updated_at = ?3 WHERE id = ?1", params![item_id, text, now()])?;
        Ok(n > 0)
    }

    /// Remove and return one item — the one that is about to be sent.
    pub fn queue_take(&self, item_id: &str) -> rusqlite::Result<Option<QueueItem>> {
        let conn = self.lock();
        let item = conn
            .query_row(
                "SELECT id, thread_id, position, text, created_at, updated_at FROM queue WHERE id = ?1",
                params![item_id],
                |r| {
                    Ok(QueueItem {
                        id: r.get(0)?,
                        thread_id: r.get(1)?,
                        position: r.get(2)?,
                        text: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                    })
                },
            )
            .optional()?;
        if item.is_some() {
            conn.execute("DELETE FROM queue WHERE id = ?1", params![item_id])?;
        }
        Ok(item)
    }

    /// Put an item before every other one of its thread (a send that failed goes back first).
    pub fn queue_reorder_first(&self, item_id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE queue SET position = (SELECT COALESCE(MIN(q.position), 1) - 1 FROM queue q
                WHERE q.thread_id = queue.thread_id) WHERE id = ?1",
            params![item_id],
        )?;
        Ok(())
    }

    pub fn queue_held(&self, thread_id: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        conn.query_row("SELECT queue_held FROM threads WHERE id = ?1", params![thread_id], |r| r.get::<_, i64>(0))
            .optional()
            .map(|v| v.unwrap_or(0) != 0)
    }

    pub fn set_queue_held(&self, thread_id: &str, held: bool) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET queue_held = ?2 WHERE id = ?1", params![thread_id, held as i64])?;
        Ok(())
    }

    // ── Turns ──

    pub fn insert_turn(&self, t: &TurnRow) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO turns (id, thread_id, status, started_at, ended_at, duration_ms, cost_usd, error)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![t.id, t.thread_id, t.status, t.started_at, t.ended_at, t.duration_ms, t.cost_usd, t.error],
        )?;
        Ok(())
    }

    pub fn finish_turn(
        &self,
        id: &str,
        status: &str,
        duration_ms: Option<i64>,
        cost: Option<f64>,
        error: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE turns SET status = ?2, ended_at = ?3, duration_ms = COALESCE(?4, ?3 - started_at), cost_usd = ?5,
                error = ?6 WHERE id = ?1",
            params![id, status, now(), duration_ms, cost, error],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn last_turn(&self, thread_id: &str) -> rusqlite::Result<Option<TurnRow>> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, thread_id, status, started_at, ended_at, duration_ms, cost_usd, error FROM turns
                WHERE thread_id = ?1 ORDER BY started_at DESC LIMIT 1",
            params![thread_id],
            |r| {
                Ok(TurnRow {
                    id: r.get(0)?,
                    thread_id: r.get(1)?,
                    status: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    duration_ms: r.get(5)?,
                    cost_usd: r.get(6)?,
                    error: r.get(7)?,
                })
            },
        )
        .optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn thread(id: &str) -> ThreadMeta {
        ThreadMeta {
            id: id.into(),
            provider: "claude".into(),
            provider_session_id: None,
            title: String::new(),
            context_id: "general".into(),
            context_name: "General".into(),
            cwd: "C:\\x".into(),
            model: String::new(),
            mode: "full".into(),
            created_at: 1,
            updated_at: 1,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            last_error: None,
            kind: "chat".into(),
            effort: String::new(),
            title_source: "auto".into(),
            state: "idle".into(),
            context_tokens: None,
            context_window: None,
            last_activity_at: 1,
            pinned: false,
            sort_key: 1.0,
            settled_at: None,
            snoozed_until: None,
            auto_settle: true,
            workspace_mode: "local".into(),
            base_branch: String::new(),
            branch: String::new(),
            worktree_path: None,
        }
    }

    #[test]
    fn links_are_kept_once_and_refreshed() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        let mut l = crate::gitops::Link {
            thread_id: "t1".into(),
            kind: "pr".into(),
            url: "https://github.com/o/r/pull/45".into(),
            number: 45,
            title: "Faster sidebar".into(),
            state: "open".into(),
            checked_at: 5,
            created_at: 5,
        };
        db.add_link(&l).unwrap();
        l.state = "merged".into();
        l.title = String::new();
        l.checked_at = 9;
        db.add_link(&l).unwrap();
        let all = db.links(None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].state.as_str(), all[0].title.as_str(), all[0].checked_at), ("merged", "Faster sidebar", 9));
        assert_eq!(db.links(Some("other")).unwrap().len(), 0);
        db.delete_session("t1").unwrap();
        assert!(db.links(None).unwrap().is_empty());
    }

    #[test]
    fn v2_sessions_migrate_into_threads() {
        let dir = std::env::temp_dir().join(format!("qp-migrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pilot.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (id TEXT PRIMARY KEY, provider TEXT NOT NULL, provider_session_id TEXT,
                    title TEXT NOT NULL DEFAULT '', context_id TEXT NOT NULL, context_name TEXT NOT NULL DEFAULT '',
                    cwd TEXT NOT NULL, model TEXT NOT NULL DEFAULT '', mode TEXT NOT NULL DEFAULT 'ask',
                    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, cost_usd REAL NOT NULL DEFAULT 0,
                    input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0, last_error TEXT);
                 INSERT INTO sessions (id, provider, provider_session_id, title, context_id, cwd, created_at, updated_at)
                    VALUES ('s1', 'claude', 'abc', 'Old one', 'general', 'C:\\v', 10, 20);",
            )
            .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let t = db.get_session("s1").unwrap().unwrap();
        assert_eq!(t.kind, "terminal");
        assert_eq!(t.provider_session_id.as_deref(), Some("abc"));
        assert_eq!(t.title, "Old one");
        assert_eq!(t.last_activity_at, 20);
        drop(db);
        // A second open is a no-op.
        let db = Database::open(&path).unwrap();
        assert_eq!(db.list_sessions().unwrap().len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn timeline_pages_and_expires() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        for i in 1..=5 {
            let kind = if i == 5 { ItemKind::Approval } else { ItemKind::User };
            db.upsert_item(&Item {
                id: format!("i{i}"),
                thread_id: "t1".into(),
                turn_id: "u1".into(),
                seq: i,
                kind,
                parent: None,
                data: json!({ "text": format!("m{i}"), "status": "pending" }),
                created_at: i,
                updated_at: i,
            })
            .unwrap();
        }
        assert_eq!(db.next_seq("t1").unwrap(), 6);
        let page = db.items("t1", None, 2).unwrap();
        assert_eq!(page.iter().map(|i| i.seq).collect::<Vec<_>>(), vec![4, 5]);
        let older = db.items("t1", Some(4), 10).unwrap();
        assert_eq!(older.len(), 3);
        assert_eq!(db.pending_request_line("t1").as_deref(), Some("Allow a tool call?"), "an approval without tool data still reads");
        let expired = db.expire_pending("t1").unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(db.get_item("i5").unwrap().unwrap().data["status"], "expired");
        assert_eq!(db.get_item("i1").unwrap().unwrap().data["status"], "pending", "user items untouched");
    }

    #[test]
    fn user_titles_win_and_usage_adds_up() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        assert!(db.set_title_from("t1", "First prompt", "auto").unwrap());
        assert!(db.set_title_from("t1", "Mine", "user").unwrap());
        assert!(!db.set_title_from("t1", "Agent idea", "auto").unwrap());
        assert_eq!(db.get_session("t1").unwrap().unwrap().title, "Mine");
        db.add_usage("t1", Some(10), Some(5), Some(0.5)).unwrap();
        db.add_usage("t1", Some(1), None, Some(0.25)).unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert_eq!((t.input_tokens, t.output_tokens), (11, 5));
        assert!((t.cost_usd - 0.75).abs() < 1e-9);
        db.set_context("t1", Some(1200), None).unwrap();
        db.set_context("t1", None, Some(200_000)).unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert_eq!((t.context_tokens, t.context_window), (Some(1200), Some(200_000)));
    }

    #[test]
    fn the_queue_keeps_order_and_holds_after_a_restart() {
        let dir = std::env::temp_dir().join(format!("qp-queue-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pilot.db");
        let (a, b) = {
            let db = Database::open(&path).unwrap();
            db.insert_session(&thread("t1")).unwrap();
            let a = db.queue_add("t1", "first").unwrap();
            let b = db.queue_add("t1", "second").unwrap();
            assert!(db.queue_update(&b.id, "second, edited").unwrap());
            assert_eq!(db.queue("t1").unwrap().iter().map(|q| q.text.as_str()).collect::<Vec<_>>(), vec!["first", "second, edited"]);
            assert!(!db.queue_held("t1").unwrap());
            (a, b)
        };
        let db = Database::open(&path).unwrap();
        assert!(db.queue_held("t1").unwrap(), "a queue that survived a restart waits for Resume");
        assert_eq!(db.queue_thread(&a.id).unwrap().as_deref(), Some("t1"));
        assert_eq!(db.queue_take(&a.id).unwrap().unwrap().text, "first");
        assert!(db.queue_take(&a.id).unwrap().is_none(), "taken once");
        assert_eq!(db.queue("t1").unwrap()[0].id, b.id);
        db.delete_session("t1").unwrap();
        assert!(db.queue("t1").unwrap().is_empty());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn arranging_pins_settles_and_snoozes() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        db.insert_session(&ThreadMeta { sort_key: 7.0, ..thread("t2") }).unwrap();
        db.arrange("t1", &Arrangement { pinned: Some(true), sort_key: Some(3.5), ..Default::default() }).unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert!(t.pinned);
        assert_eq!(t.sort_key, 3.5);
        db.arrange("t1", &Arrangement { settled: Some(true), ..Default::default() }).unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert!(t.settled_at.is_some() && !t.pinned, "settling unpins");
        db.arrange("t1", &Arrangement { settled: Some(false), ..Default::default() }).unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert!(t.settled_at.is_none());
        assert_eq!(t.sort_key, 8.0, "back on top of the active list");
        db.arrange("t1", &Arrangement { settled: Some(true), ..Default::default() }).unwrap();
        db.arrange("t1", &Arrangement { settled: Some(false), sort_key: Some(2.5), ..Default::default() }).unwrap();
        assert_eq!(db.get_session("t1").unwrap().unwrap().sort_key, 2.5, "a drop keeps its spot");
        let snooze: Arrangement = serde_json::from_value(json!({ "snoozedUntil": 99 })).unwrap();
        db.arrange("t1", &snooze).unwrap();
        assert_eq!(db.get_session("t1").unwrap().unwrap().snoozed_until, Some(99));
        let wake: Arrangement = serde_json::from_value(json!({ "snoozedUntil": null })).unwrap();
        assert_eq!(wake.snoozed_until, Some(None));
        db.arrange("t1", &wake).unwrap();
        assert_eq!(db.get_session("t1").unwrap().unwrap().snoozed_until, None);
        let untouched: Arrangement = serde_json::from_value(json!({ "pinned": false })).unwrap();
        assert_eq!(untouched.snoozed_until, None, "an absent field changes nothing");
        db.arrange("t1", &Arrangement { auto_settle: Some(false), ..Default::default() }).unwrap();
        assert!(!db.get_session("t1").unwrap().unwrap().auto_settle);
    }

    #[test]
    fn auto_settle_skips_waiting_queued_running_and_opted_out() {
        let db = Database::open_in_memory().unwrap();
        let fresh = now();
        for (id, auto_settle, state, activity) in [
            ("old", true, "idle", 1),
            ("off", false, "idle", 1),
            ("waiting", true, "waiting", 1),
            ("queued", true, "idle", 1),
            ("running", true, "idle", 1),
            ("fresh", true, "idle", fresh),
        ] {
            let meta = ThreadMeta { auto_settle, state: state.into(), last_activity_at: activity, created_at: activity, ..thread(id) };
            db.insert_session(&meta).unwrap();
        }
        db.insert_session(&ThreadMeta { pinned: true, ..thread("pinned") }).unwrap();
        db.insert_session(&ThreadMeta { kind: "terminal".into(), ..thread("term") }).unwrap();
        db.queue_add("queued", "later").unwrap();
        assert!(db.auto_settle(0, &[]).unwrap().is_empty(), "0 days = never");
        let mut settled = db.auto_settle(3, &["running".to_string()]).unwrap();
        settled.sort();
        assert_eq!(settled, vec!["old".to_string(), "pinned".to_string()], "pinning does not prevent it");
        assert!(!db.get_session("pinned").unwrap().unwrap().pinned, "settling unpins");
        assert!(db.auto_settle(3, &[]).unwrap().contains(&"running".to_string()), "settles once it stopped");
        // Un-settled by hand: stays until the user writes again.
        db.arrange("old", &Arrangement { settled: Some(false), ..Default::default() }).unwrap();
        assert!(db.auto_settle(3, &[]).unwrap().is_empty());
        db.resume("old").unwrap();
        assert_eq!(db.auto_settle(3, &[]).unwrap(), vec!["old".to_string()], "the usual rules again after a message");
        db.arrange("fresh", &Arrangement { snoozed_until: Some(Some(fresh + 1000)), ..Default::default() }).unwrap();
        db.resume("fresh").unwrap();
        assert_eq!(db.get_session("fresh").unwrap().unwrap().snoozed_until, None, "a message wakes it");
    }

    #[test]
    fn regenerated_titles_replace_the_users() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        db.set_title_from("t1", "Mine", "user").unwrap();
        for (seq, kind, text) in [(1, ItemKind::User, "Fix the login"), (2, ItemKind::Assistant, "Done, it works")] {
            db.upsert_item(&Item {
                id: format!("i{seq}"),
                thread_id: "t1".into(),
                turn_id: "u1".into(),
                seq,
                kind,
                parent: None,
                data: json!({ "text": text }),
                created_at: seq,
                updated_at: seq,
            })
            .unwrap();
        }
        assert_eq!(db.title_material("t1").unwrap(), ("Fix the login".to_string(), "Done, it works".to_string()));
        db.set_title_regenerated("t1", "Login fix").unwrap();
        let t = db.get_session("t1").unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.title_source.as_str()), ("Login fix", "auto"));
    }

    #[test]
    fn questions_answered_by_message_outlive_the_process() {
        let db = Database::open_in_memory().unwrap();
        db.insert_session(&thread("t1")).unwrap();
        for (seq, by_message) in [(1, true), (2, false)] {
            db.upsert_item(&Item {
                id: format!("q{seq}"),
                thread_id: "t1".into(),
                turn_id: "u1".into(),
                seq,
                kind: ItemKind::Question,
                parent: None,
                data: json!({ "requestId": format!("r{seq}"), "status": "pending", "byMessage": by_message, "questions": [] }),
                created_at: seq,
                updated_at: seq,
            })
            .unwrap();
        }
        let expired = db.expire_pending("t1").unwrap();
        assert_eq!(expired.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["q2"]);
        let kept = db.request_item("t1", "r1").unwrap().unwrap();
        assert_eq!(kept.data["status"], "pending", "an async question still waits for the next message");
        assert!(db.request_item("t1", "nope").unwrap().is_none());
    }

    #[test]
    fn turns_record_their_end() {
        let db = Database::open_in_memory().unwrap();
        db.insert_turn(&TurnRow {
            id: "u1".into(),
            thread_id: "t1".into(),
            status: "started".into(),
            started_at: 100,
            ended_at: None,
            duration_ms: None,
            cost_usd: None,
            error: None,
        })
        .unwrap();
        db.finish_turn("u1", "completed", Some(42), Some(0.01), None).unwrap();
        let t = db.last_turn("t1").unwrap().unwrap();
        assert_eq!(t.status, "completed");
        assert_eq!(t.duration_ms, Some(42));
    }
}
