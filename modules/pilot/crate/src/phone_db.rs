//! Phone access storage (round 4, card 1): paired phones and what they did.
//! Schema 25. A submodule of `db` so it shares the one connection. A phone's
//! token is stored as its hash only; a revoked row stays for the audit.

use super::Database;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

pub(crate) const PHONE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS phone_devices (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL,
        last_seen INTEGER NOT NULL,
        revoked_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS phone_audit (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        device TEXT NOT NULL,
        action TEXT NOT NULL,
        thread_id TEXT,
        ok INTEGER NOT NULL,
        note TEXT NOT NULL DEFAULT ''
    );
";

/// The audit keeps this many rows.
const AUDIT_ROWS: i64 = 500;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneDevice {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneAudit {
    pub at: i64,
    pub device: String,
    pub action: String,
    pub thread_id: Option<String>,
    pub ok: bool,
    pub note: String,
}

fn row_device(r: &Row) -> rusqlite::Result<PhoneDevice> {
    Ok(PhoneDevice { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)?, last_seen: r.get(3)? })
}

impl Database {
    pub fn phone_device_add(&self, device: &PhoneDevice, token_hash: &str) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO phone_devices (id, name, token_hash, created_at, last_seen) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![device.id, device.name, token_hash, device.created_at, device.last_seen],
        )?;
        Ok(())
    }

    /// The live device a token belongs to; a revoked one is nobody.
    pub fn phone_device_by_token(&self, token_hash: &str) -> Option<PhoneDevice> {
        self.lock()
            .query_row(
                "SELECT id, name, created_at, last_seen FROM phone_devices WHERE token_hash = ?1 AND revoked_at IS NULL",
                params![token_hash],
                row_device,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn phone_device_live(&self, id: &str) -> bool {
        self.lock()
            .query_row("SELECT 1 FROM phone_devices WHERE id = ?1 AND revoked_at IS NULL", params![id], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    pub fn phone_device_seen(&self, id: &str, at: i64) {
        let _ = self.lock().execute("UPDATE phone_devices SET last_seen = ?2 WHERE id = ?1", params![id, at]);
    }

    pub fn phone_devices(&self) -> rusqlite::Result<Vec<PhoneDevice>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at, last_seen FROM phone_devices WHERE revoked_at IS NULL ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_device)?;
        rows.collect()
    }

    pub fn phone_device_revoke(&self, id: &str, at: i64) -> rusqlite::Result<bool> {
        Ok(self
            .lock()
            .execute("UPDATE phone_devices SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL", params![id, at])?
            > 0)
    }

    pub fn phone_audit_add(&self, entry: &PhoneAudit) {
        let conn = self.lock();
        let _ = conn.execute(
            "INSERT INTO phone_audit (at, device, action, thread_id, ok, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![entry.at, entry.device, entry.action, entry.thread_id, entry.ok as i64, entry.note],
        );
        let _ = conn.execute(
            "DELETE FROM phone_audit WHERE id <= (SELECT MAX(id) FROM phone_audit) - ?1",
            params![AUDIT_ROWS],
        );
    }

    pub fn phone_audit_list(&self, limit: i64) -> rusqlite::Result<Vec<PhoneAudit>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT at, device, action, thread_id, ok, note FROM phone_audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(PhoneAudit {
                at: r.get(0)?,
                device: r.get(1)?,
                action: r.get(2)?,
                thread_id: r.get(3)?,
                ok: r.get::<_, i64>(4)? != 0,
                note: r.get(5)?,
            })
        })?;
        rows.collect()
    }
}
