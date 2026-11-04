//! Paired devices and the audit log (PLAN-SERVER-CLIENT §4.1 layers 8–9), in
//! `gateway.db` on the server. A device is its Ed25519 public key; nothing
//! secret is stored here (the PIN only as an Argon2id hash).

use crate::authz::DeviceRole;
use crate::identity::fingerprint;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Entered the right code; waits for the server's user to compare the words.
    Pending,
    Paired,
    Revoked,
}

impl DeviceState {
    fn parse(s: &str) -> Self {
        match s {
            "paired" => Self::Paired,
            "revoked" => Self::Revoked,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub role: DeviceRole,
    pub state: DeviceState,
    pub fingerprint: String,
    pub created_at: String,
    pub paired_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub device_id: Option<String>,
    pub device: String,
    pub command: String,
    pub outcome: String,
    pub detail: Option<String>,
}

/// A device's id: the first 8 bytes of SHA-256 of its public key.
pub fn device_id(public: &[u8; 32]) -> String {
    hex::encode(&Sha256::digest(public)[..8])
}

/// Audit rows kept; older ones are dropped.
const AUDIT_KEEP: i64 = 20_000;

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn clean_name(name: &str) -> String {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).take(60).collect();
    if n.is_empty() {
        "Device".into()
    } else {
        n
    }
}

fn row_device(r: &Row) -> rusqlite::Result<Device> {
    let key: Vec<u8> = r.get("public_key")?;
    let key: [u8; 32] = key.try_into().unwrap_or([0u8; 32]);
    let role: String = r.get("role")?;
    let state: String = r.get("state")?;
    Ok(Device {
        id: r.get("id")?,
        name: r.get("name")?,
        role: DeviceRole::parse(&role).unwrap_or(DeviceRole::ReadOnly),
        state: DeviceState::parse(&state),
        fingerprint: fingerprint(&key),
        created_at: r.get("created_at")?,
        paired_at: r.get("paired_at")?,
        revoked_at: r.get("revoked_at")?,
        last_seen: r.get("last_seen")?,
    })
}

const COLUMNS: &str = "id, public_key, name, role, state, created_at, paired_at, revoked_at, last_seen";

pub struct Registry {
    conn: Connection,
}

impl Registry {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS devices (
                 id TEXT PRIMARY KEY,
                 public_key BLOB NOT NULL UNIQUE,
                 name TEXT NOT NULL,
                 role TEXT NOT NULL,
                 state TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 paired_at TEXT,
                 revoked_at TEXT,
                 last_seen TEXT
             );
             CREATE TABLE IF NOT EXISTS audit (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 at TEXT NOT NULL,
                 device_id TEXT,
                 device TEXT NOT NULL,
                 command TEXT NOT NULL,
                 outcome TEXT NOT NULL,
                 detail TEXT
             );
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             -- A pairing lives only as long as its connection: none survives a restart.
             DELETE FROM devices WHERE state = 'pending';",
        )
        .map_err(|e| format!("gateway.db: {e}"))?;
        Ok(Self { conn })
    }

    fn one(&self, sql: &str, p: impl rusqlite::Params) -> Result<Option<Device>, String> {
        self.conn.query_row(sql, p, row_device).optional().map_err(|e| format!("gateway.db: {e}"))
    }

    pub fn by_key(&self, public: &[u8; 32]) -> Result<Option<Device>, String> {
        self.one(&format!("SELECT {COLUMNS} FROM devices WHERE public_key = ?1"), params![public.as_slice()])
    }

    pub fn get(&self, id: &str) -> Result<Option<Device>, String> {
        self.one(&format!("SELECT {COLUMNS} FROM devices WHERE id = ?1"), params![id])
    }

    fn must(&self, id: &str) -> Result<Device, String> {
        self.get(id)?.ok_or_else(|| "no such device".to_string())
    }

    pub fn list(&self) -> Result<Vec<Device>, String> {
        let mut st = self
            .conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM devices ORDER BY CASE state WHEN 'paired' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END, name COLLATE NOCASE"
            ))
            .map_err(|e| e.to_string())?;
        let rows = st.query_map([], row_device).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// A device that entered the right code. A revoked key that pairs again
    /// (a new code on the server's screen) starts over as pending.
    pub fn add_pending(&self, public: &[u8; 32], name: &str) -> Result<Device, String> {
        let id = device_id(public);
        self.conn
            .execute(
                "INSERT INTO devices (id, public_key, name, role, state, created_at)
                 VALUES (?1, ?2, ?3, 'full', 'pending', ?4)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = 'pending', role = 'full',
                     paired_at = NULL, revoked_at = NULL",
                params![id, public.as_slice(), clean_name(name), now()],
            )
            .map_err(|e| format!("gateway.db: {e}"))?;
        self.must(&id)
    }

    pub fn confirm(&self, id: &str, role: DeviceRole) -> Result<Device, String> {
        let n = self
            .conn
            .execute(
                "UPDATE devices SET state = 'paired', role = ?2, paired_at = ?3 WHERE id = ?1 AND state = 'pending'",
                params![id, role.as_str(), now()],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("that device is not waiting for pairing".into());
        }
        self.must(id)
    }

    pub fn remove_pending(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM devices WHERE id = ?1 AND state = 'pending'", params![id])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<Device, String> {
        self.conn
            .execute("UPDATE devices SET name = ?2 WHERE id = ?1", params![id, clean_name(name)])
            .map_err(|e| e.to_string())?;
        self.must(id)
    }

    pub fn set_role(&self, id: &str, role: DeviceRole) -> Result<Device, String> {
        self.conn
            .execute("UPDATE devices SET role = ?2 WHERE id = ?1", params![id, role.as_str()])
            .map_err(|e| e.to_string())?;
        self.must(id)
    }

    pub fn revoke(&self, id: &str) -> Result<Device, String> {
        self.conn
            .execute(
                "UPDATE devices SET state = 'revoked', revoked_at = ?2 WHERE id = ?1 AND state != 'revoked'",
                params![id, now()],
            )
            .map_err(|e| e.to_string())?;
        self.must(id)
    }

    /// Removes a revoked device from the list (its key must pair again).
    pub fn forget(&self, id: &str) -> Result<(), String> {
        let n = self
            .conn
            .execute("DELETE FROM devices WHERE id = ?1 AND state = 'revoked'", params![id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("revoke the device first".into());
        }
        Ok(())
    }

    pub fn touch(&self, id: &str) {
        let _ = self.conn.execute("UPDATE devices SET last_seen = ?2 WHERE id = ?1", params![id, now()]);
    }

    /// One audit row. `device` is the device's name, or "server" for the
    /// server's own screen, or "unpaired" for a key that is not paired.
    pub fn audit(&self, device_id: Option<&str>, device: &str, command: &str, outcome: &str, detail: Option<&str>) {
        let detail = detail.map(|d| d.chars().take(300).collect::<String>());
        let r = self.conn.execute(
            "INSERT INTO audit (at, device_id, device, command, outcome, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![now(), device_id, device, command, outcome, detail],
        );
        match r {
            Ok(_) => {
                let id = self.conn.last_insert_rowid();
                if id % 1000 == 0 {
                    let _ = self.conn.execute("DELETE FROM audit WHERE id <= ?1", params![id - AUDIT_KEEP]);
                }
            }
            Err(e) => log::warn!("gateway audit: {e}"),
        }
    }

    pub fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, String> {
        let mut st = self
            .conn
            .prepare("SELECT id, at, device_id, device, command, outcome, detail FROM audit ORDER BY id DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![limit as i64], |r| {
                Ok(AuditEntry {
                    id: r.get(0)?,
                    at: r.get(1)?,
                    device_id: r.get(2)?,
                    device: r.get(3)?,
                    command: r.get(4)?,
                    outcome: r.get(5)?,
                    detail: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<(), String> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            ),
            None => self.conn.execute("DELETE FROM meta WHERE key = ?1", params![key]),
        }
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_goes_from_pending_to_paired_to_revoked() {
        let r = Registry::in_memory().unwrap();
        let key = [7u8; 32];
        let d = r.add_pending(&key, "  Laptop\n ").unwrap();
        assert_eq!((d.state, d.name.as_str()), (DeviceState::Pending, "Laptop"));
        assert_eq!(d.id, device_id(&key));
        let d = r.confirm(&d.id, DeviceRole::NoTrading).unwrap();
        assert_eq!((d.state, d.role), (DeviceState::Paired, DeviceRole::NoTrading));
        assert!(r.confirm(&d.id, DeviceRole::Full).is_err(), "only from pending");
        assert_eq!(r.by_key(&key).unwrap().unwrap().id, d.id);
        assert!(r.forget(&d.id).is_err(), "only revoked devices are forgotten");
        let d = r.revoke(&d.id).unwrap();
        assert_eq!(d.state, DeviceState::Revoked);
        assert!(d.revoked_at.is_some());
        // Paired again only through a new code: pending, not paired.
        assert_eq!(r.add_pending(&key, "Laptop").unwrap().state, DeviceState::Pending);
        r.remove_pending(&d.id).unwrap();
        assert!(r.get(&d.id).unwrap().is_none());
    }

    #[test]
    fn the_audit_keeps_the_newest_rows_first() {
        let r = Registry::in_memory().unwrap();
        r.audit(Some("abc"), "Laptop", "plugin:notes|list_notes", "ok", None);
        r.audit(None, "server", "revoke", "ok", Some(&"x".repeat(500)));
        let a = r.recent_audit(10).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].command, "revoke");
        assert_eq!(a[0].detail.as_ref().unwrap().len(), 300);
        assert_eq!(a[1].device, "Laptop");
    }
}
