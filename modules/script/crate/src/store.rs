//! The version history — `script.db`, one row per saved version of a script.
//!
//! The files stay the source of truth (the forge imports them, not this
//! database); the history is append-only, like the forge's ledger. Version 1
//! of every script is the content as first found on disk, an edit made
//! outside QuantScript (an agent, git) is recorded as its own version the
//! next time the script is opened, and a restore is a new version, never a
//! rewind.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct VersionMeta {
    pub id: i64,
    pub file: String,
    pub version: i64,
    pub sha256: String,
    pub size: i64,
    pub message: String,
    /// `baseline` | `user` | `external` | `restore` | `template`.
    pub author: String,
    /// The content passed the sandbox check before it was written.
    pub checked: bool,
    pub created_at: String,
}

/// Per script: the latest version number, how many there are, and the
/// latest version's hash (what the listing compares the file against),
/// author and date (what tells a deleted script's archive entry apart).
#[derive(Debug, Clone, Serialize, Default)]
pub struct VersionCounts {
    pub latest: Option<i64>,
    pub count: i64,
    pub latest_sha256: Option<String>,
    pub latest_author: Option<String>,
    pub latest_at: Option<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS versions (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            file       TEXT    NOT NULL,
            version    INTEGER NOT NULL,
            content    TEXT    NOT NULL,
            sha256     TEXT    NOT NULL,
            size       INTEGER NOT NULL,
            message    TEXT    NOT NULL DEFAULT '',
            author     TEXT    NOT NULL DEFAULT 'user',
            checked    INTEGER NOT NULL DEFAULT 0,
            created_at TEXT    NOT NULL,
            UNIQUE(file, version)
        );
        CREATE INDEX IF NOT EXISTS versions_file ON versions(file, version DESC);",
    )
}

const COLUMNS: &str = "id, file, version, sha256, size, message, author, checked, created_at";

fn row_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<VersionMeta> {
    Ok(VersionMeta {
        id: row.get(0)?,
        file: row.get(1)?,
        version: row.get(2)?,
        sha256: row.get(3)?,
        size: row.get(4)?,
        message: row.get(5)?,
        author: row.get(6)?,
        checked: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
    })
}

pub fn latest(conn: &Connection, file: &str) -> rusqlite::Result<Option<VersionMeta>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM versions WHERE file = ?1 ORDER BY version DESC LIMIT 1"),
        params![file],
        row_meta,
    )
    .optional()
}

/// Newest first.
pub fn list(conn: &Connection, file: &str) -> rusqlite::Result<Vec<VersionMeta>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM versions WHERE file = ?1 ORDER BY version DESC"))?;
    let rows = stmt.query_map(params![file], row_meta)?;
    rows.collect()
}

pub fn read(conn: &Connection, file: &str, version: i64) -> rusqlite::Result<Option<(VersionMeta, String)>> {
    conn.query_row(
        &format!("SELECT {COLUMNS}, content FROM versions WHERE file = ?1 AND version = ?2"),
        params![file, version],
        |row| Ok((row_meta(row)?, row.get::<_, String>(9)?)),
    )
    .optional()
}

/// Append the next version of a script.
pub fn record(
    conn: &Connection,
    file: &str,
    content: &str,
    message: &str,
    author: &str,
    checked: bool,
) -> rusqlite::Result<VersionMeta> {
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM versions WHERE file = ?1",
        params![file],
        |row| row.get(0),
    )?;
    let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let sha = sha256_hex(content.as_bytes());
    conn.execute(
        "INSERT INTO versions (file, version, content, sha256, size, message, author, checked, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            file,
            next,
            content,
            sha,
            content.len() as i64,
            message,
            author,
            i64::from(checked),
            created_at
        ],
    )?;
    let id = conn.last_insert_rowid();
    Ok(VersionMeta {
        id,
        file: file.to_string(),
        version: next,
        sha256: sha,
        size: content.len() as i64,
        message: message.to_string(),
        author: author.to_string(),
        checked,
        created_at,
    })
}

/// Every script's counts in one query — the listing joins these onto the
/// files it found.
pub fn counts(conn: &Connection) -> rusqlite::Result<HashMap<String, VersionCounts>> {
    let mut stmt = conn.prepare(
        "SELECT v.file, v.version, v.sha256, c.n, v.author, v.created_at
         FROM versions v
         JOIN (SELECT file, MAX(version) AS latest, COUNT(*) AS n FROM versions GROUP BY file) c
           ON c.file = v.file AND c.latest = v.version",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            VersionCounts {
                latest: Some(row.get(1)?),
                count: row.get(3)?,
                latest_sha256: Some(row.get(2)?),
                latest_author: Some(row.get(4)?),
                latest_at: Some(row.get(5)?),
            },
        ))
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init_schema(&c).unwrap();
        c
    }

    #[test]
    fn versions_append_and_read_back_newest_first() {
        let c = conn();
        assert!(latest(&c, "a.py").unwrap().is_none());
        let v1 = record(&c, "a.py", "x = 1\n", "Baseline", "baseline", false).unwrap();
        let v2 = record(&c, "a.py", "x = 2\n", "Saved", "user", true).unwrap();
        record(&c, "b.py", "y = 1\n", "Baseline", "baseline", false).unwrap();
        assert_eq!((v1.version, v2.version), (1, 2));
        assert_eq!(v2.sha256, sha256_hex(b"x = 2\n"));
        assert!(v2.checked && !v1.checked);
        assert_eq!(latest(&c, "a.py").unwrap().unwrap().version, 2);
        let listed = list(&c, "a.py").unwrap();
        assert_eq!(listed.iter().map(|v| v.version).collect::<Vec<_>>(), vec![2, 1]);
        let (meta, content) = read(&c, "a.py", 1).unwrap().unwrap();
        assert_eq!((meta.author.as_str(), content.as_str()), ("baseline", "x = 1\n"));
        assert!(read(&c, "a.py", 3).unwrap().is_none());
        let counts = counts(&c).unwrap();
        assert_eq!(counts["a.py"].latest, Some(2));
        assert_eq!(counts["a.py"].count, 2);
        assert_eq!(counts["a.py"].latest_sha256.as_deref(), Some(v2.sha256.as_str()));
        assert_eq!(counts["a.py"].latest_author.as_deref(), Some("user"));
        assert_eq!(counts["a.py"].latest_at.as_deref(), Some(v2.created_at.as_str()));
        assert_eq!(counts["b.py"].count, 1);
    }

    #[test]
    fn the_hash_is_the_lowercase_sha256_of_the_bytes() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
