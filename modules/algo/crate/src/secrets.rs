//! Exchange credentials in the suite vault (PLAN-AUTO-EXECUTION §2.1).
//!
//! One vault secret per connected exchange, id `algo/exchange/<exchange id>`,
//! value = the JSON of [`ExchangeSecret`]. Before the vault they sat in
//! `exchanges.config_encrypted`, encrypted with a key anyone could recompute
//! (the username and data dir, or a constant in the public source). Every such
//! row moves into the vault once — on start, after an import, or when the row
//! is first read — and the column is cleared with SQLite's secure delete.
//!
//! The two old keys live on only in [`legacy`], to read rows written before
//! the vault. Nothing is encrypted with them any more.

use qs_core::vault::{SecretInfo, Vault};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::ExchangeConfig;

pub(crate) const PREFIX: &str = "algo/exchange/";

pub(crate) fn secret_id(exchange_id: &str) -> String {
    format!("{PREFIX}{exchange_id}")
}

/// An exchange's credentials. Wiped from memory when dropped; no `Debug`.
#[derive(Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExchangeSecret {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_endpoint: Option<String>,
}

impl Drop for ExchangeSecret {
    fn drop(&mut self) {
        for field in self.fields_mut() {
            field.zeroize();
        }
    }
}

fn present(value: &Option<String>) -> Option<String> {
    value.as_ref().filter(|v| !v.trim().is_empty()).cloned()
}

impl ExchangeSecret {
    /// The credentials typed into the exchange form; blank fields are absent.
    pub fn from_config(config: &ExchangeConfig) -> Self {
        Self {
            api_key: present(&config.api_key),
            api_secret: present(&config.api_secret),
            passphrase: present(&config.passphrase),
            wallet_address: present(&config.wallet_address),
            private_key: present(&config.private_key),
            rpc_endpoint: present(&config.rpc_endpoint),
        }
    }

    fn fields_mut(&mut self) -> [&mut Option<String>; 6] {
        [
            &mut self.api_key,
            &mut self.api_secret,
            &mut self.passphrase,
            &mut self.wallet_address,
            &mut self.private_key,
            &mut self.rpc_endpoint,
        ]
    }

    /// `self` with every field `update` carries replaced: a blank field in the
    /// edit form keeps the stored value ("Leave blank to keep the stored key").
    pub fn merged(&self, update: &ExchangeSecret) -> ExchangeSecret {
        let pick = |new: &Option<String>, old: &Option<String>| present(new).or_else(|| present(old));
        ExchangeSecret {
            api_key: pick(&update.api_key, &self.api_key),
            api_secret: pick(&update.api_secret, &self.api_secret),
            passphrase: pick(&update.passphrase, &self.passphrase),
            wallet_address: pick(&update.wallet_address, &self.wallet_address),
            private_key: pick(&update.private_key, &self.private_key),
            rpc_endpoint: pick(&update.rpc_endpoint, &self.rpc_endpoint),
        }
    }

    pub fn is_empty(&self) -> bool {
        let mut copy = self.clone();
        copy.fields_mut().iter().all(|f| present(f).is_none())
    }

    pub fn text(value: &Option<String>) -> String {
        value.clone().unwrap_or_default()
    }
}

/// Store `secret` for an exchange; an empty one removes the entry.
pub(crate) fn store(vault: &Vault, exchange_id: &str, secret: &ExchangeSecret) -> Result<Option<SecretInfo>, String> {
    let id = secret_id(exchange_id);
    if secret.is_empty() {
        vault.delete(&id)?;
        return Ok(None);
    }
    let json = Zeroizing::new(serde_json::to_string(secret).map_err(|e| format!("Serialize credentials: {e}"))?);
    vault.set(&id, json.as_bytes()).map(Some)
}

/// The stored credentials of an exchange; empty when it has none.
pub(crate) fn load(vault: &Vault, exchange_id: &str) -> Result<ExchangeSecret, String> {
    match vault.get_string(&secret_id(exchange_id))? {
        None => Ok(ExchangeSecret::default()),
        Some(json) => serde_json::from_str(&json).map_err(|_| "The stored credentials are damaged; enter them again.".to_string()),
    }
}

/// What a migration did. Holds ids and counts only, never a value.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MigrationReport {
    pub migrated: usize,
    /// Exchange ids whose old row decrypts with neither old key.
    pub unreadable: Vec<String>,
}

fn legacy_rows(conn: &Connection, only: Option<&str>) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, config_encrypted FROM exchanges
             WHERE config_encrypted IS NOT NULL AND config_encrypted != '' AND (?1 IS NULL OR id = ?1)",
        )
        .map_err(|e| format!("Prepare: {e}"))?;
    let rows = stmt
        .query_map(params![only], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("Query: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("Row: {e}"))
}

/// Move every old `config_encrypted` row (or only `only`) into the vault:
/// decrypt with today's derivation or the old constant, store, read back and
/// compare, then clear the column. A row neither key opens stays as it is and
/// is reported; its keys have to be entered again.
pub(crate) fn migrate_legacy_rows(conn: &Connection, vault: &Vault, only: Option<&str>) -> Result<MigrationReport, String> {
    migrate_with_keys(conn, vault, only, &legacy::keys())
}

fn migrate_with_keys(
    conn: &Connection,
    vault: &Vault,
    only: Option<&str>,
    keys: &[Zeroizing<[u8; 32]>],
) -> Result<MigrationReport, String> {
    let rows = legacy_rows(conn, only)?;
    let mut report = MigrationReport::default();
    if rows.is_empty() {
        return Ok(report);
    }
    // Overwrite the old ciphertext on disk instead of leaving it in a free page.
    let secure_before: i64 = conn
        .query_row("PRAGMA secure_delete", [], |row| row.get(0))
        .map_err(|e| format!("secure_delete: {e}"))?;
    conn.execute_batch("PRAGMA secure_delete = ON;").map_err(|e| format!("secure_delete: {e}"))?;

    let result = (|| -> Result<(), String> {
        for (exchange_id, hex_text) in rows {
            let Some(plain) = legacy::decrypt(&hex_text, keys) else {
                report.unreadable.push(exchange_id);
                continue;
            };
            let secret: ExchangeSecret = match serde_json::from_str(&plain) {
                Ok(secret) => secret,
                Err(_) => {
                    report.unreadable.push(exchange_id);
                    continue;
                }
            };
            store(vault, &exchange_id, &secret)?;
            if load(vault, &exchange_id)? != secret {
                return Err(format!("The vault returned different credentials for exchange {exchange_id}; nothing was cleared."));
            }
            conn.execute("UPDATE exchanges SET config_encrypted = NULL WHERE id = ?1", params![exchange_id])
                .map_err(|e| format!("Clear old credentials: {e}"))?;
            report.migrated += 1;
        }
        Ok(())
    })();

    if report.migrated > 0 {
        // The WAL still holds the old page images until a checkpoint.
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    }
    let _ = conn.execute_batch(&format!("PRAGMA secure_delete = {secure_before};"));
    result.map(|()| report)
}

/// Remove vault entries of exchanges that no longer exist.
pub(crate) fn prune_orphans(conn: &Connection, vault: &Vault) -> Result<usize, String> {
    let mut removed = 0;
    for info in vault.list(PREFIX)? {
        let exchange_id = &info.id[PREFIX.len()..];
        let exists: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM exchanges WHERE id = ?1)", params![exchange_id], |row| row.get(0))
            .map_err(|e| format!("Query: {e}"))?;
        if !exists && vault.delete(&info.id)? {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Whether any row still holds old credentials (cheap; decides at start
/// whether the vault has to be opened at all).
pub(crate) fn has_legacy_rows(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exchanges WHERE config_encrypted IS NOT NULL AND config_encrypted != '')",
        [],
        |row| row.get(0),
    )
    .unwrap_or(false)
}

/// Reading the credentials QuantAlgo stored before the vault. Decrypt only.
pub(crate) mod legacy {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use sha2::{Digest, Sha256};
    use zeroize::Zeroizing;

    /// The constant key of the first QuantAlgo versions. Public in the source
    /// history, so a row under it was never protected.
    const LEGACY_CONSTANT: &[u8; 32] = b"QuantAlgo_AES256_Key_2024!@#$%^&";

    /// The second scheme: SHA-256 of the username and the data dir.
    pub(crate) fn derived(user: &str, data_dir: &str) -> Zeroizing<[u8; 32]> {
        let digest = Sha256::digest(format!("quantalgo:v2:{user}:{data_dir}").as_bytes());
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&digest);
        key
    }

    /// The keys rows on this machine were written with, newest first.
    pub(crate) fn keys() -> Vec<Zeroizing<[u8; 32]>> {
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "local-user".to_string());
        let data_dir = crate::settings::get_data_dir();
        vec![derived(&user, &data_dir.to_string_lossy()), Zeroizing::new(*LEGACY_CONSTANT)]
    }

    /// `nonce ‖ ciphertext` as hex → the plaintext, with the first key that
    /// authenticates.
    pub(crate) fn decrypt(hex_text: &str, keys: &[Zeroizing<[u8; 32]>]) -> Option<Zeroizing<String>> {
        let data = hex::decode(hex_text).ok()?;
        if data.len() < 13 {
            return None;
        }
        let (nonce, ciphertext) = data.split_at(12);
        keys.iter().find_map(|key| {
            let cipher = Aes256Gcm::new_from_slice(&key[..]).ok()?;
            let plain = cipher.decrypt(Nonce::from_slice(nonce), ciphertext).ok()?;
            String::from_utf8(plain).ok().map(Zeroizing::new)
        })
    }

    /// Rows as the old code wrote them — for the migration tests only.
    #[cfg(test)]
    pub(crate) fn encrypt(plain: &str, key: &[u8; 32]) -> String {
        let cipher = Aes256Gcm::new_from_slice(key).unwrap();
        let nonce = [7u8; 12];
        let mut out = nonce.to_vec();
        out.extend(cipher.encrypt(Nonce::from_slice(&nonce), plain.as_bytes()).unwrap());
        hex::encode(out)
    }

    #[cfg(test)]
    pub(crate) fn constant() -> Zeroizing<[u8; 32]> {
        Zeroizing::new(*LEGACY_CONSTANT)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use qs_core::vault::{KeyStore, KeyStoreError};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct MemoryKeyStore(Mutex<HashMap<String, Vec<u8>>>);

    impl KeyStore for MemoryKeyStore {
        fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
            Ok(self.0.lock().unwrap().get(account).cloned().map(Zeroizing::new))
        }
        fn store(&self, account: &str, key: &[u8]) -> Result<(), KeyStoreError> {
            self.0.lock().unwrap().insert(account.to_string(), key.to_vec());
            Ok(())
        }
    }

    /// A vault in a temporary folder that is removed with it.
    pub(crate) struct TestVault(Option<Vault>, std::path::PathBuf);

    impl std::ops::Deref for TestVault {
        type Target = Vault;
        fn deref(&self) -> &Vault {
            self.0.as_ref().unwrap()
        }
    }

    impl Drop for TestVault {
        fn drop(&mut self) {
            drop(self.0.take());
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }

    pub(crate) fn test_vault() -> TestVault {
        let dir = std::env::temp_dir().join(format!("qs-algo-vault-{}", uuid::Uuid::new_v4()));
        let vault = Vault::open(&dir, &MemoryKeyStore::default(), None).unwrap();
        TestVault(Some(vault), dir)
    }

    pub(crate) fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, provider: &str, config_encrypted: Option<&str>) {
        conn.execute(
            "INSERT INTO exchanges (id, name, exchange_type, provider, config_encrypted, is_active, sandbox, created_at, updated_at)
             VALUES (?1, ?1, 'cex', ?2, ?3, 1, 0, 't', 't')",
            params![id, provider, config_encrypted],
        )
        .unwrap();
    }

    fn column(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row("SELECT config_encrypted FROM exchanges WHERE id = ?1", params![id], |row| row.get(0))
            .unwrap()
    }

    const OLD_JSON: &str = r#"{"api_key":"AK-binance-1234","api_secret":"SK-binance-9876","passphrase":null,"wallet_address":null,"private_key":null,"rpc_endpoint":null}"#;

    #[test]
    fn rows_written_with_both_old_keys_move_into_the_vault() {
        let conn = test_db();
        let vault = test_vault();
        let derived = legacy::derived("alice", "C:/Users/alice/.quantsuite/modules/algo");
        let constant = legacy::constant();
        insert(&conn, "e-derived", "binance", Some(&legacy::encrypt(OLD_JSON, &derived)));
        insert(&conn, "e-constant", "bybit", Some(&legacy::encrypt(OLD_JSON.replace("binance", "bybit").as_str(), &constant)));
        insert(&conn, "e-foreign", "binance", Some(&legacy::encrypt(OLD_JSON, &legacy::derived("bob", "elsewhere"))));
        insert(&conn, "e-none", "kraken", None);

        let keys = vec![derived, constant];
        let report = migrate_with_keys(&conn, &vault, None, &keys).unwrap();
        assert_eq!(report.migrated, 2);
        assert_eq!(report.unreadable, vec!["e-foreign".to_string()]);

        let a = load(&vault, "e-derived").unwrap();
        assert_eq!(a.api_key.as_deref(), Some("AK-binance-1234"));
        assert_eq!(a.api_secret.as_deref(), Some("SK-binance-9876"));
        assert_eq!(a.passphrase, None);
        let b = load(&vault, "e-constant").unwrap();
        assert_eq!(b.api_key.as_deref(), Some("AK-bybit-1234"));

        assert_eq!(column(&conn, "e-derived"), None);
        assert_eq!(column(&conn, "e-constant"), None);
        assert!(column(&conn, "e-foreign").is_some(), "an unreadable row is left alone");
        assert!(load(&vault, "e-none").unwrap().is_empty());

        // Idempotent: a second run has nothing left to move.
        let again = migrate_with_keys(&conn, &vault, None, &keys).unwrap();
        assert_eq!(again.migrated, 0);
        assert_eq!(again.unreadable, vec!["e-foreign".to_string()]);
    }

    #[test]
    fn one_row_can_be_migrated_alone() {
        let conn = test_db();
        let vault = test_vault();
        let constant = legacy::constant();
        insert(&conn, "a", "binance", Some(&legacy::encrypt(OLD_JSON, &constant)));
        insert(&conn, "b", "binance", Some(&legacy::encrypt(OLD_JSON, &constant)));
        let report = migrate_with_keys(&conn, &vault, Some("b"), &[constant]).unwrap();
        assert_eq!(report.migrated, 1);
        assert!(column(&conn, "a").is_some());
        assert_eq!(column(&conn, "b"), None);
    }

    fn keys(api_key: Option<&str>, api_secret: Option<&str>, passphrase: Option<&str>) -> ExchangeSecret {
        ExchangeSecret {
            api_key: api_key.map(String::from),
            api_secret: api_secret.map(String::from),
            passphrase: passphrase.map(String::from),
            wallet_address: None,
            private_key: None,
            rpc_endpoint: None,
        }
    }

    #[test]
    fn a_blank_form_field_keeps_the_stored_value() {
        let stored = keys(Some("AK"), Some("SK"), Some("PP"));
        let update = keys(None, Some("SK2"), Some("  "));
        let merged = stored.merged(&update);
        assert_eq!(merged.api_key.as_deref(), Some("AK"));
        assert_eq!(merged.api_secret.as_deref(), Some("SK2"));
        assert_eq!(merged.passphrase.as_deref(), Some("PP"));
        assert!(ExchangeSecret::default().is_empty());
        assert!(!merged.is_empty());
    }

    #[test]
    fn an_empty_secret_removes_the_entry_and_orphans_are_pruned() {
        let conn = test_db();
        let vault = test_vault();
        insert(&conn, "kept", "binance", None);
        let secret = keys(Some("AK"), Some("SK"), None);
        store(&vault, "kept", &secret).unwrap();
        store(&vault, "gone", &secret).unwrap();
        assert_eq!(prune_orphans(&conn, &vault).unwrap(), 1);
        assert!(vault.info(&secret_id("kept")).unwrap().is_some());
        assert!(vault.info(&secret_id("gone")).unwrap().is_none());
        assert!(store(&vault, "kept", &ExchangeSecret::default()).unwrap().is_none());
        assert!(vault.info(&secret_id("kept")).unwrap().is_none());
    }

    #[test]
    fn the_old_ciphertext_does_not_survive_on_disk() {
        let dir = std::env::temp_dir().join(format!("qs-algo-secure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quantalgo.db");
        let vault = test_vault();
        let constant = legacy::constant();
        let old = legacy::encrypt(OLD_JSON, &constant);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            crate::db::init_db(&conn).unwrap();
            insert(&conn, "e1", "binance", Some(&old));
            let report = migrate_with_keys(&conn, &vault, None, &[constant]).unwrap();
            assert_eq!(report.migrated, 1);
        }
        for file in ["quantalgo.db", "quantalgo.db-wal"] {
            let raw = std::fs::read(dir.join(file)).unwrap_or_default();
            assert!(!raw.windows(32).any(|w| w == &old.as_bytes()[..32]), "{file} still holds the old ciphertext");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The acceptance run on a COPY of a real quantalgo.db (never the live file):
/// `QS_VAULT_REAL_COPY=<copy> QUANTSUITE_HOME=<the root that wrote it>
/// cargo test -p tauri-plugin-algo --lib real_copy -- --ignored --nocapture`.
/// A throwaway vault with an in-memory keystore; `QS_VAULT_REAL_PROBE=1`
/// also runs the read-only credential test of each migrated exchange.
/// Prints counts, providers and test results — never a key.
#[cfg(test)]
mod real_copy {
    use super::tests::test_vault;
    use super::*;

    #[test]
    #[ignore = "needs a copy of a real quantalgo.db"]
    fn real_copy_migrates_every_row() {
        let path = std::env::var("QS_VAULT_REAL_COPY").expect("QS_VAULT_REAL_COPY");
        let conn = Connection::open(&path).unwrap();
        let vault = test_vault();
        let before: Vec<(String, String)> = conn
            .prepare("SELECT id, provider FROM exchanges WHERE config_encrypted IS NOT NULL AND config_encrypted != ''")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let report = migrate_legacy_rows(&conn, &vault, None).unwrap();
        println!("rows with old keys: {}, migrated: {}, unreadable: {:?}", before.len(), report.migrated, report.unreadable);
        assert!(report.unreadable.is_empty(), "rows no old key opens: {:?}", report.unreadable);
        assert_eq!(report.migrated, before.len());
        assert!(!has_legacy_rows(&conn));
        for (id, provider) in before {
            let secret = load(&vault, &id).unwrap();
            let has = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.is_empty());
            println!("{provider}: api_key {} api_secret {}", has(&secret.api_key), has(&secret.api_secret));
            if std::env::var("QS_VAULT_REAL_PROBE").as_deref() == Ok("1") {
                let sandbox: i32 =
                    conn.query_row("SELECT sandbox FROM exchanges WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
                let result = crate::exchanges::test_provider_credentials(
                    &provider,
                    &ExchangeSecret::text(&secret.api_key),
                    &ExchangeSecret::text(&secret.api_secret),
                    &ExchangeSecret::text(&secret.passphrase),
                    sandbox != 0,
                )
                .unwrap();
                // Reported, not asserted: the venue may refuse a key for its own
                // reasons (expired, IP whitelist) that say nothing about the vault.
                println!("{provider}: credential test success={} ({})", result.success, result.message);
            }
        }
    }
}
