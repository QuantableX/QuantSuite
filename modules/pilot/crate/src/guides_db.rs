//! Review guide storage (round 3, card 04): the latest guide per target (a
//! pull request, a thread's diff scope) with the key of the diff it was
//! written for — a guide whose key no longer matches is stale. Schema 23.

use super::Database;
use rusqlite::{params, OptionalExtension};

pub(crate) const GUIDE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS review_guides (
        target TEXT PRIMARY KEY,
        key TEXT NOT NULL,
        guide TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
";

impl Database {
    /// `(key, guide JSON)` of a target's latest guide.
    pub fn review_guide(&self, target: &str) -> rusqlite::Result<Option<(String, String)>> {
        let conn = self.lock();
        conn.query_row("SELECT key, guide FROM review_guides WHERE target = ?1", params![target], |r| Ok((r.get(0)?, r.get(1)?))).optional()
    }

    pub fn save_review_guide(&self, target: &str, key: &str, guide: &str, at: i64) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO review_guides (target, key, guide, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(target) DO UPDATE SET key = ?2, guide = ?3, created_at = ?4",
            params![target, key, guide, at],
        )?;
        Ok(())
    }
}
