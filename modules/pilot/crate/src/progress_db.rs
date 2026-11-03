//! Pinned reference images of a thread's visual progress (round 3, card 05).
//! Schema 24. A pin names an image by its progress key; it never copies it.

use super::Database;
use rusqlite::params;

pub(crate) const PIN_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS thread_pins (
        thread_id TEXT NOT NULL,
        key TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (thread_id, key)
    );
";

impl Database {
    /// A thread's pinned image keys, in the order they were pinned.
    pub fn pins(&self, thread_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT key FROM thread_pins WHERE thread_id = ?1 ORDER BY at, rowid")?;
        let rows = stmt.query_map(params![thread_id], |r| r.get(0))?;
        rows.collect()
    }

    pub fn set_pin(&self, thread_id: &str, key: &str, pinned: bool, at: i64) -> rusqlite::Result<()> {
        let conn = self.lock();
        if pinned {
            conn.execute("INSERT OR IGNORE INTO thread_pins (thread_id, key, at) VALUES (?1, ?2, ?3)", params![thread_id, key, at])?;
        } else {
            conn.execute("DELETE FROM thread_pins WHERE thread_id = ?1 AND key = ?2", params![thread_id, key])?;
        }
        Ok(())
    }
}
