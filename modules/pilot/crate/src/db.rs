//! `pilot.db` — the session list.
//!
//! Only the list. The transcript is the CLI's own (Claude's project
//! folder, Codex's rollout, pi's session file) and comes back when the
//! session is resumed inside its terminal; the pilot stores what it needs
//! to launch again and what the panel shows when nothing is running.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    /// Adapter id: `claude`, `codex`, `pi`, `omp`, `opencode`, `gemini`, or a custom one.
    pub provider: String,
    /// What the CLI knows the session as: Claude and pi take our id, Codex
    /// and omp report theirs, OpenCode and Gemini only know "the latest".
    pub provider_session_id: Option<String>,
    pub title: String,
    pub context_id: String,
    pub context_name: String,
    pub cwd: String,
    pub model: String,
    /// `ask` | `auto` | `full`
    pub mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub last_error: Option<String>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

const SESSION_COLS: &str = "id, provider, provider_session_id, title, context_id, context_name, cwd, model, mode, \
    created_at, updated_at, cost_usd, input_tokens, output_tokens, last_error";

fn row_session(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMeta> {
    Ok(SessionMeta {
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
    })
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                provider_session_id TEXT,
                title TEXT NOT NULL DEFAULT '',
                context_id TEXT NOT NULL,
                context_name TEXT NOT NULL DEFAULT '',
                cwd TEXT NOT NULL,
                model TEXT NOT NULL DEFAULT '',
                mode TEXT NOT NULL DEFAULT 'ask',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                cost_usd REAL NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            );
            -- V1's transcript copy. The CLIs keep their own; nothing reads this.
            DROP TABLE IF EXISTS items;
            DROP TABLE IF EXISTS kv;",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<SessionMeta>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {SESSION_COLS} FROM sessions ORDER BY updated_at DESC"))?;
        let rows = stmt.query_map([], row_session)?;
        rows.collect()
    }

    pub fn get_session(&self, id: &str) -> rusqlite::Result<Option<SessionMeta>> {
        let conn = self.lock();
        conn.query_row(&format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"), params![id], row_session)
            .optional()
    }

    pub fn insert_session(&self, m: &SessionMeta) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO sessions (id, provider, provider_session_id, title, context_id, context_name, cwd, model, mode,
                created_at, updated_at, cost_usd, input_tokens, output_tokens, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                m.last_error
            ],
        )?;
        Ok(())
    }

    pub fn set_provider_session(&self, id: &str, provider_session_id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE sessions SET provider_session_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, provider_session_id, now()],
        )?;
        Ok(())
    }

    /// The adapter the row belongs to now. A different one than before
    /// forgets the old provider session: the new CLI starts its own.
    pub fn set_provider(&self, id: &str, provider: &str) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let current: Option<String> = conn
            .query_row("SELECT provider FROM sessions WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        let changed = current.as_deref() != Some(provider);
        if changed {
            conn.execute(
                "UPDATE sessions SET provider = ?2, provider_session_id = NULL, updated_at = ?3 WHERE id = ?1",
                params![id, provider, now()],
            )?;
        }
        Ok(changed)
    }

    pub fn set_title(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, title, now()])?;
        Ok(())
    }

    pub fn set_model(&self, id: &str, model: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE sessions SET model = ?2 WHERE id = ?1", params![id, model])?;
        Ok(())
    }

    pub fn set_mode(&self, id: &str, mode: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE sessions SET mode = ?2 WHERE id = ?1", params![id, mode])?;
        Ok(())
    }

    /// Absolute totals — every source reports cumulative usage.
    pub fn set_usage(&self, id: &str, input: Option<i64>, output: Option<i64>, cost: Option<f64>) -> rusqlite::Result<()> {
        let conn = self.lock();
        if let Some(i) = input {
            conn.execute("UPDATE sessions SET input_tokens = ?2 WHERE id = ?1", params![id, i])?;
        }
        if let Some(o) = output {
            conn.execute("UPDATE sessions SET output_tokens = ?2 WHERE id = ?1", params![id, o])?;
        }
        if let Some(c) = cost {
            conn.execute("UPDATE sessions SET cost_usd = ?2 WHERE id = ?1", params![id, c])?;
        }
        Ok(())
    }

    pub fn set_error(&self, id: &str, error: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE sessions SET last_error = ?2 WHERE id = ?1", params![id, error])?;
        Ok(())
    }

    pub fn touch(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE sessions SET updated_at = ?2 WHERE id = ?1", params![id, now()])?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }
}
