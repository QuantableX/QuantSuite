//! The suite's secret vault (PLAN-AUTO-EXECUTION §2.1).
//!
//! One store for every secret QuantSuite keeps: exchange API keys today; wallet
//! keys (PLAN-EXECUTION-VENUES), the server identity and paired device keys
//! (PLAN-SERVER-CLIENT) and the bundle re-encryption of PLAN-MIGRATION later.
//!
//! - A random 256-bit **master key** lives in the OS keystore through
//!   `keyring` (Windows Credential Manager, macOS Keychain, Linux Secret
//!   Service), one entry per vault. Where no keystore exists (a headless Linux
//!   server), the master key is wrapped in `secrets/vault.key` under a key
//!   derived from a passphrase with Argon2id; the passphrase comes from
//!   [`PASSPHRASE_ENV`] or [`unlock`].
//! - Each **secret** is AES-256-GCM under the master key with its id as
//!   associated data, in `secrets/vault.db`: a ciphertext copied onto another
//!   id does not decrypt.
//! - **Write-only** towards the frontend: set, replace, delete and a masked
//!   fingerprint. A value leaves this module only to Rust code that uses it
//!   (signing a request) — never into a command result, an event or a log line.
//!
//! A copy of `vault.db` alone decrypts nothing: the master key stays in the
//! keystore of the machine (or behind the passphrase).

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use zeroize::Zeroizing;

/// The keystore service every vault's master key is filed under.
pub const KEYSTORE_SERVICE: &str = "QuantSuite Vault";
/// The passphrase for a vault without an OS keystore (headless servers).
pub const PASSPHRASE_ENV: &str = "QUANTSUITE_VAULT_PASSPHRASE";

const DB_FILE: &str = "vault.db";
const KEY_FILE: &str = "vault.key";
const CHECK_PLAINTEXT: &[u8] = b"quantsuite-vault-check-v1";
const AD_PREFIX: &[u8] = b"quantsuite-vault:v1:";
const MIN_PASSPHRASE_CHARS: usize = 12;

type MasterKey = Zeroizing<[u8; 32]>;

// ── keystores ───────────────────────────────────────────────────────────────

/// Why a keystore call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStoreError {
    /// There is no usable keystore on this machine (none installed, locked,
    /// not supported). Only at creation does this switch a vault to the
    /// passphrase file.
    Unavailable(String),
    /// The keystore answered with an error.
    Failed(String),
}

impl fmt::Display for KeyStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(e) => write!(f, "no OS keystore is available ({e})"),
            Self::Failed(e) => write!(f, "the OS keystore failed ({e})"),
        }
    }
}

/// Where master keys live. The app uses [`OsKeyStore`]; tests use their own.
pub trait KeyStore: Send + Sync {
    /// The key filed under `account`; `Ok(None)` when there is none.
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError>;
    fn store(&self, account: &str, key: &[u8]) -> Result<(), KeyStoreError>;
}

/// The OS credential store through `keyring`.
pub struct OsKeyStore;

fn classify(e: keyring::Error) -> KeyStoreError {
    match e {
        keyring::Error::NoDefaultStore
        | keyring::Error::NoStorageAccess(_)
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NotSupportedByStore(_) => KeyStoreError::Unavailable(e.to_string()),
        other => KeyStoreError::Failed(other.to_string()),
    }
}

impl KeyStore for OsKeyStore {
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
        let entry = keyring::Entry::new(KEYSTORE_SERVICE, account).map_err(classify)?;
        match entry.get_password() {
            Ok(text) => {
                let text = Zeroizing::new(text);
                hex::decode(text.as_str())
                    .map(|bytes| Some(Zeroizing::new(bytes)))
                    .map_err(|_| KeyStoreError::Failed("the stored master key is not hex".into()))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(classify(e)),
        }
    }

    fn store(&self, account: &str, key: &[u8]) -> Result<(), KeyStoreError> {
        let entry = keyring::Entry::new(KEYSTORE_SERVICE, account).map_err(classify)?;
        let text = Zeroizing::new(hex::encode(key));
        entry.set_password(&text).map_err(classify)
    }
}

// ── public types ────────────────────────────────────────────────────────────

/// Where a vault's master key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MasterSource {
    Keystore,
    PassphraseFile,
}

impl MasterSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Keystore => "keystore",
            Self::PassphraseFile => "passphrase-file",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "keystore" => Some(Self::Keystore),
            "passphrase-file" => Some(Self::PassphraseFile),
            _ => None,
        }
    }
}

/// What the frontend may know about a secret: never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretInfo {
    pub id: String,
    /// `…7f3a`: four hex digits of an HMAC of the value under the master key.
    /// It changes when the value changes and reveals nothing about it.
    pub fingerprint: String,
    /// Unix milliseconds.
    pub created_at: i64,
    pub updated_at: i64,
}

/// Why a vault could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The vault (or a new vault on a machine without a keystore) needs its
    /// passphrase.
    NeedsPassphrase(String),
    /// The passphrase does not open the key file.
    WrongPassphrase,
    Failed(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedsPassphrase(why) => write!(f, "The secret vault is locked: {why}"),
            Self::WrongPassphrase => f.write_str("The passphrase does not open the secret vault."),
            Self::Failed(why) => write!(f, "The secret vault cannot be opened: {why}"),
        }
    }
}

/// The vault's state for Settings and the exchange page.
#[derive(Debug, Clone, Serialize)]
pub struct VaultStatus {
    /// `ready` | `needs-passphrase` | `error`
    pub state: &'static str,
    pub source: Option<MasterSource>,
    /// Secrets stored; 0 while the vault is not open.
    pub secrets: i64,
    pub message: Option<String>,
}

/// Argon2id cost of the passphrase file. Stored in the file, so a stronger
/// default later still opens older files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    /// 64 MiB, 3 passes: about half a second on a desktop CPU, once per start.
    fn default() -> Self {
        Self { m_cost_kib: 64 * 1024, t_cost: 3, p_cost: 1 }
    }
}

#[derive(Serialize, Deserialize)]
struct KeyFile {
    version: u32,
    kdf: String,
    params: KdfParams,
    salt: String,
    nonce: String,
    wrapped: String,
}

// ── the vault ───────────────────────────────────────────────────────────────

/// An open vault. Deliberately not `Debug`: it holds the master key.
pub struct Vault {
    conn: Connection,
    dir: PathBuf,
    vault_id: String,
    source: MasterSource,
    master: MasterKey,
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    OsRng.fill_bytes(&mut buf);
    buf
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn account_for(vault_id: &str) -> String {
    format!("master-key:{vault_id}")
}

fn associated_data(id: &str) -> Vec<u8> {
    let mut ad = AD_PREFIX.to_vec();
    ad.extend_from_slice(id.as_bytes());
    ad
}

fn seal(key: &[u8; 32], plaintext: &[u8], ad: &[u8]) -> Result<([u8; 12], Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("cipher init: {e}"))?;
    let nonce = random_bytes::<12>();
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: ad })
        .map_err(|_| "encryption failed".to_string())?;
    Ok((nonce, ciphertext))
}

fn open_sealed(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8], ad: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    if nonce.len() != 12 {
        return Err("bad nonce length".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("cipher init: {e}"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: ad })
        .map(Zeroizing::new)
        .map_err(|_| "the ciphertext does not authenticate".to_string())
}

fn derive_kek(passphrase: &str, salt: &[u8], params: KdfParams) -> Result<MasterKey, String> {
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(32))
            .map_err(|e| format!("Argon2 parameters: {e}"))?,
    );
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out[..])
        .map_err(|e| format!("Argon2: {e}"))?;
    Ok(out)
}

fn to_master(bytes: &[u8]) -> Result<MasterKey, String> {
    if bytes.len() != 32 {
        return Err(format!("the master key has {} bytes, not 32", bytes.len()));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(bytes);
    Ok(key)
}

fn key_file_ad(vault_id: &str) -> Vec<u8> {
    format!("quantsuite-vault:key-file:{vault_id}").into_bytes()
}

fn write_key_file(dir: &Path, vault_id: &str, master: &[u8; 32], passphrase: &str, params: KdfParams) -> Result<(), String> {
    let salt = random_bytes::<16>();
    let kek = derive_kek(passphrase, &salt, params)?;
    let (nonce, wrapped) = seal(&kek, master, &key_file_ad(vault_id))?;
    let file = KeyFile {
        version: 1,
        kdf: "argon2id".into(),
        params,
        salt: hex::encode(salt),
        nonce: hex::encode(nonce),
        wrapped: hex::encode(wrapped),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|e| format!("key file: {e}"))?;
    crate::paths::write_atomic(&dir.join(KEY_FILE), &json).map_err(|e| format!("write {KEY_FILE}: {e}"))
}

fn read_key_file(dir: &Path, vault_id: &str, passphrase: &str) -> Result<MasterKey, OpenError> {
    let raw = std::fs::read(dir.join(KEY_FILE))
        .map_err(|e| OpenError::Failed(format!("{KEY_FILE} cannot be read: {e}")))?;
    let file: KeyFile =
        serde_json::from_slice(&raw).map_err(|e| OpenError::Failed(format!("{KEY_FILE} is damaged: {e}")))?;
    if file.version != 1 || file.kdf != "argon2id" {
        return Err(OpenError::Failed(format!("{KEY_FILE} has an unknown format")));
    }
    let decode = |text: &str| hex::decode(text).map_err(|_| OpenError::Failed(format!("{KEY_FILE} is damaged")));
    let salt = decode(&file.salt)?;
    let nonce = decode(&file.nonce)?;
    let wrapped = decode(&file.wrapped)?;
    let kek = derive_kek(passphrase, &salt, file.params).map_err(OpenError::Failed)?;
    let master = open_sealed(&kek, &nonce, &wrapped, &key_file_ad(vault_id)).map_err(|_| OpenError::WrongPassphrase)?;
    to_master(&master).map_err(OpenError::Failed)
}

fn migrate_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS secrets (
            id TEXT PRIMARY KEY,
            nonce BLOB NOT NULL,
            ciphertext BLOB NOT NULL,
            fingerprint TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        ",
    )
}

fn meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| row.get(0))
        .optional()
}

fn count_secrets(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM secrets", [], |row| row.get(0))
}

impl Vault {
    /// Open the vault in `dir`, creating it on first use.
    ///
    /// A new vault files a fresh master key in `keystore`. Only when the
    /// keystore is unavailable does it use the passphrase file — and then only
    /// with a `passphrase`, never silently without one.
    pub fn open(dir: &Path, keystore: &dyn KeyStore, passphrase: Option<&str>) -> Result<Self, OpenError> {
        Self::open_with(dir, keystore, passphrase, KdfParams::default())
    }

    /// [`Vault::open`] with explicit Argon2 cost for a new passphrase file.
    pub fn open_with(
        dir: &Path,
        keystore: &dyn KeyStore,
        passphrase: Option<&str>,
        params: KdfParams,
    ) -> Result<Self, OpenError> {
        let fail = |what: &str, e: &dyn fmt::Display| OpenError::Failed(format!("{what}: {e}"));
        std::fs::create_dir_all(dir).map_err(|e| fail("create the secrets folder", &e))?;
        let conn = Connection::open(dir.join(DB_FILE)).map_err(|e| fail("open vault.db", &e))?;
        conn.pragma_update(None, "journal_mode", "WAL").map_err(|e| fail("vault.db", &e))?;
        conn.pragma_update(None, "secure_delete", "ON").map_err(|e| fail("vault.db", &e))?;
        migrate_schema(&conn).map_err(|e| fail("vault.db schema", &e))?;

        let vault_id = meta(&conn, "vault_id").map_err(|e| fail("vault.db", &e))?;
        let source = meta(&conn, "master_source").map_err(|e| fail("vault.db", &e))?;
        match (vault_id, source.as_deref().and_then(MasterSource::parse)) {
            (Some(vault_id), Some(source)) => {
                let master = match source {
                    MasterSource::Keystore => match keystore.load(&account_for(&vault_id)) {
                        Ok(Some(bytes)) => to_master(&bytes).map_err(OpenError::Failed)?,
                        Ok(None) => {
                            let stored = count_secrets(&conn).map_err(|e| fail("vault.db", &e))?;
                            if stored == 0 {
                                // Nothing to lose: start over with a new key.
                                return Self::create(conn, dir, keystore, passphrase, params);
                            }
                            return Err(OpenError::Failed(format!(
                                "its master key is missing from the OS keystore, so the {stored} stored \
                                 secret(s) cannot be read. Enter the keys again."
                            )));
                        }
                        Err(e) => return Err(OpenError::Failed(e.to_string())),
                    },
                    MasterSource::PassphraseFile => {
                        let Some(passphrase) = passphrase else {
                            return Err(OpenError::NeedsPassphrase(format!(
                                "enter its passphrase (or set {PASSPHRASE_ENV})."
                            )));
                        };
                        read_key_file(dir, &vault_id, passphrase)?
                    }
                };
                let vault = Self { conn, dir: dir.to_path_buf(), vault_id, source, master };
                vault.verify_check()?;
                Ok(vault)
            }
            _ => Self::create(conn, dir, keystore, passphrase, params),
        }
    }

    fn create(
        conn: Connection,
        dir: &Path,
        keystore: &dyn KeyStore,
        passphrase: Option<&str>,
        params: KdfParams,
    ) -> Result<Self, OpenError> {
        let vault_id = uuid::Uuid::new_v4().to_string();
        let master: MasterKey = Zeroizing::new(random_bytes::<32>());
        let account = account_for(&vault_id);

        // The keystore counts only when the key reads back identical.
        let in_keystore = keystore
            .store(&account, &master[..])
            .and_then(|()| keystore.load(&account))
            .map(|loaded| loaded.map(|b| b.as_slice() == &master[..]).unwrap_or(false));
        let source = match in_keystore {
            Ok(true) => MasterSource::Keystore,
            Ok(false) => {
                return Err(OpenError::Failed("the OS keystore did not return the key it stored".into()));
            }
            Err(KeyStoreError::Failed(e)) => return Err(OpenError::Failed(format!("the OS keystore failed ({e})"))),
            Err(KeyStoreError::Unavailable(why)) => {
                let Some(passphrase) = passphrase else {
                    return Err(OpenError::NeedsPassphrase(format!(
                        "this machine has no OS keystore ({why}); choose a passphrase for the vault \
                         (or set {PASSPHRASE_ENV})."
                    )));
                };
                if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
                    return Err(OpenError::NeedsPassphrase(format!(
                        "the passphrase needs at least {MIN_PASSPHRASE_CHARS} characters."
                    )));
                }
                write_key_file(dir, &vault_id, &master, passphrase, params).map_err(OpenError::Failed)?;
                MasterSource::PassphraseFile
            }
        };

        let (nonce, check) = seal(&master, CHECK_PLAINTEXT, &associated_data("vault:check")).map_err(OpenError::Failed)?;
        let write = || -> rusqlite::Result<()> {
            conn.execute("DELETE FROM meta", [])?;
            conn.execute("INSERT INTO meta (key, value) VALUES ('vault_id', ?1)", params![vault_id])?;
            conn.execute("INSERT INTO meta (key, value) VALUES ('master_source', ?1)", params![source.as_str()])?;
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('check', ?1)",
                params![format!("{}:{}", hex::encode(nonce), hex::encode(&check))],
            )?;
            conn.execute("INSERT INTO meta (key, value) VALUES ('created_at', ?1)", params![now_ms().to_string()])?;
            Ok(())
        };
        write().map_err(|e| OpenError::Failed(format!("vault.db: {e}")))?;
        Ok(Self { conn, dir: dir.to_path_buf(), vault_id, source, master })
    }

    fn verify_check(&self) -> Result<(), OpenError> {
        let stored = meta(&self.conn, "check")
            .map_err(|e| OpenError::Failed(format!("vault.db: {e}")))?
            .ok_or_else(|| OpenError::Failed("vault.db has no key check".into()))?;
        let (nonce, check) = stored
            .split_once(':')
            .ok_or_else(|| OpenError::Failed("vault.db key check is damaged".into()))?;
        let (nonce, check) = (
            hex::decode(nonce).map_err(|_| OpenError::Failed("vault.db key check is damaged".into()))?,
            hex::decode(check).map_err(|_| OpenError::Failed("vault.db key check is damaged".into()))?,
        );
        match open_sealed(&self.master, &nonce, &check, &associated_data("vault:check")) {
            Ok(plain) if plain.as_slice() == CHECK_PLAINTEXT => Ok(()),
            _ if self.source == MasterSource::PassphraseFile => Err(OpenError::WrongPassphrase),
            _ => Err(OpenError::Failed("the master key in the OS keystore does not match this vault".into())),
        }
    }

    pub fn source(&self) -> MasterSource {
        self.source
    }

    /// The vault's random id; its keystore entry is `master-key:<id>`.
    pub fn id(&self) -> &str {
        &self.vault_id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn fingerprint(&self, value: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.master[..]).expect("HMAC takes any key length");
        mac.update(b"quantsuite-vault:fingerprint:");
        mac.update(value);
        let digest = mac.finalize().into_bytes();
        format!("…{}", &hex::encode(digest)[..4])
    }

    /// Store `value` under `id`, replacing what was there.
    pub fn set(&self, id: &str, value: &[u8]) -> Result<SecretInfo, String> {
        if id.trim().is_empty() {
            return Err("a secret needs an id".into());
        }
        let (nonce, ciphertext) = seal(&self.master, value, &associated_data(id))?;
        let fingerprint = self.fingerprint(value);
        let now = now_ms();
        self.conn
            .execute(
                "INSERT INTO secrets (id, nonce, ciphertext, fingerprint, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(id) DO UPDATE SET nonce = excluded.nonce, ciphertext = excluded.ciphertext,
                   fingerprint = excluded.fingerprint, updated_at = excluded.updated_at",
                params![id, nonce.to_vec(), ciphertext, fingerprint, now],
            )
            .map_err(|e| format!("vault.db: {e}"))?;
        self.info(id)?.ok_or_else(|| "the secret was not stored".into())
    }

    /// The value under `id`, for Rust code that uses it. Never return it from a
    /// command.
    pub fn get(&self, id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .query_row("SELECT nonce, ciphertext FROM secrets WHERE id = ?1", params![id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(|e| format!("vault.db: {e}"))?;
        match row {
            None => Ok(None),
            Some((nonce, ciphertext)) => open_sealed(&self.master, &nonce, &ciphertext, &associated_data(id))
                .map(Some)
                .map_err(|e| format!("secret {id}: {e}")),
        }
    }

    /// [`Vault::get`] for UTF-8 values.
    pub fn get_string(&self, id: &str) -> Result<Option<Zeroizing<String>>, String> {
        match self.get(id)? {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes.to_vec())
                .map(|text| Some(Zeroizing::new(text)))
                .map_err(|_| format!("secret {id} is not text")),
        }
    }

    /// Whether the secret under `id` exists and decrypts.
    pub fn verify(&self, id: &str) -> bool {
        matches!(self.get(id), Ok(Some(_)))
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        self.conn
            .execute("DELETE FROM secrets WHERE id = ?1", params![id])
            .map(|n| n > 0)
            .map_err(|e| format!("vault.db: {e}"))
    }

    pub fn info(&self, id: &str) -> Result<Option<SecretInfo>, String> {
        self.conn
            .query_row(
                "SELECT id, fingerprint, created_at, updated_at FROM secrets WHERE id = ?1",
                params![id],
                |row| {
                    Ok(SecretInfo {
                        id: row.get(0)?,
                        fingerprint: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("vault.db: {e}"))
    }

    /// Every secret whose id starts with `prefix`, without values.
    pub fn list(&self, prefix: &str) -> Result<Vec<SecretInfo>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, fingerprint, created_at, updated_at FROM secrets ORDER BY id")
            .map_err(|e| format!("vault.db: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SecretInfo {
                    id: row.get(0)?,
                    fingerprint: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("vault.db: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            let info = row.map_err(|e| format!("vault.db: {e}"))?;
            if info.id.starts_with(prefix) {
                out.push(info);
            }
        }
        Ok(out)
    }

    pub fn count(&self) -> Result<i64, String> {
        count_secrets(&self.conn).map_err(|e| format!("vault.db: {e}"))
    }
}

// ── the suite's vault ───────────────────────────────────────────────────────

static GLOBAL: OnceLock<Mutex<Option<Vault>>> = OnceLock::new();

fn global() -> &'static Mutex<Option<Vault>> {
    GLOBAL.get_or_init(|| Mutex::new(None))
}

fn env_passphrase() -> Option<Zeroizing<String>> {
    std::env::var(PASSPHRASE_ENV).ok().filter(|p| !p.is_empty()).map(Zeroizing::new)
}

/// Run `f` on the suite's vault (`~/.quantsuite/secrets/`), opening it on
/// first use.
pub fn with_vault<R>(f: impl FnOnce(&Vault) -> Result<R, String>) -> Result<R, String> {
    let mut slot = global().lock().map_err(|_| "the secret vault lock is poisoned".to_string())?;
    if slot.is_none() {
        let passphrase = env_passphrase();
        let vault = Vault::open(&crate::paths::secrets_dir(), &OsKeyStore, passphrase.as_deref().map(String::as_str))
            .map_err(|e| e.to_string())?;
        *slot = Some(vault);
    }
    match slot.as_ref() {
        Some(vault) => f(vault),
        None => Err("the secret vault is not open".into()),
    }
}

/// Open (or, without a keystore, create) the suite's vault with a passphrase.
pub fn unlock(passphrase: &str) -> VaultStatus {
    let opened = Vault::open(&crate::paths::secrets_dir(), &OsKeyStore, Some(passphrase));
    match (global().lock(), opened) {
        (Ok(mut slot), Ok(vault)) => {
            *slot = Some(vault);
            drop(slot);
            status()
        }
        (_, Err(e)) => status_from_error(&e),
        (Err(_), Ok(_)) => VaultStatus {
            state: "error",
            source: None,
            secrets: 0,
            message: Some("the secret vault lock is poisoned".into()),
        },
    }
}

fn status_from_error(e: &OpenError) -> VaultStatus {
    VaultStatus {
        state: match e {
            OpenError::NeedsPassphrase(_) | OpenError::WrongPassphrase => "needs-passphrase",
            OpenError::Failed(_) => "error",
        },
        source: None,
        secrets: 0,
        message: Some(e.to_string()),
    }
}

/// The suite vault's state; opens it when it can.
pub fn status() -> VaultStatus {
    let mut slot = match global().lock() {
        Ok(slot) => slot,
        Err(_) => {
            return VaultStatus {
                state: "error",
                source: None,
                secrets: 0,
                message: Some("the secret vault lock is poisoned".into()),
            }
        }
    };
    if slot.is_none() {
        let passphrase = env_passphrase();
        match Vault::open(&crate::paths::secrets_dir(), &OsKeyStore, passphrase.as_deref().map(String::as_str)) {
            Ok(vault) => *slot = Some(vault),
            Err(e) => return status_from_error(&e),
        }
    }
    match slot.as_ref() {
        Some(vault) => VaultStatus {
            state: "ready",
            source: Some(vault.source()),
            secrets: vault.count().unwrap_or(0),
            message: None,
        },
        None => VaultStatus { state: "error", source: None, secrets: 0, message: None },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A keystore in memory.
    #[derive(Default)]
    pub struct MemoryKeyStore(pub Mutex<HashMap<String, Vec<u8>>>);

    impl KeyStore for MemoryKeyStore {
        fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
            Ok(self.0.lock().unwrap().get(account).cloned().map(Zeroizing::new))
        }
        fn store(&self, account: &str, key: &[u8]) -> Result<(), KeyStoreError> {
            self.0.lock().unwrap().insert(account.to_string(), key.to_vec());
            Ok(())
        }
    }

    /// A machine without a keystore.
    pub struct NoKeyStore;

    impl KeyStore for NoKeyStore {
        fn load(&self, _: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
            Err(KeyStoreError::Unavailable("no secret service".into()))
        }
        fn store(&self, _: &str, _: &[u8]) -> Result<(), KeyStoreError> {
            Err(KeyStoreError::Unavailable("no secret service".into()))
        }
    }

    pub const FAST: KdfParams = KdfParams { m_cost_kib: 256, t_cost: 1, p_cost: 1 };

    /// A temporary folder, removed when dropped (declare it before the vault
    /// so the vault closes first).
    struct TempDir(PathBuf);

    impl std::ops::Deref for TempDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("qs-vault-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    #[test]
    fn round_trip_and_replace() {
        let dir = temp_dir("round");
        let store = MemoryKeyStore::default();
        let vault = Vault::open(&dir, &store, None).unwrap();
        assert_eq!(vault.source(), MasterSource::Keystore);

        let first = vault.set("algo/exchange/a", b"secret-one").unwrap();
        assert_eq!(vault.get_string("algo/exchange/a").unwrap().unwrap().as_str(), "secret-one");
        assert!(first.fingerprint.starts_with('…') && first.fingerprint.chars().count() == 5);

        let second = vault.set("algo/exchange/a", b"secret-two").unwrap();
        assert_ne!(first.fingerprint, second.fingerprint);
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(vault.get_string("algo/exchange/a").unwrap().unwrap().as_str(), "secret-two");
        assert_eq!(vault.count().unwrap(), 1);

        // Reopened with the same keystore: still readable.
        drop(vault);
        let vault = Vault::open(&dir, &store, None).unwrap();
        assert_eq!(vault.get_string("algo/exchange/a").unwrap().unwrap().as_str(), "secret-two");
        assert!(vault.delete("algo/exchange/a").unwrap());
        assert!(vault.get("algo/exchange/a").unwrap().is_none());
        assert!(!vault.delete("algo/exchange/a").unwrap());
    }

    #[test]
    fn wrong_associated_data_fails() {
        let dir = temp_dir("ad");
        let store = MemoryKeyStore::default();
        let vault = Vault::open(&dir, &store, None).unwrap();
        vault.set("algo/exchange/a", b"for a").unwrap();
        vault.set("algo/exchange/b", b"for b").unwrap();
        // Copy a's ciphertext onto b's row: it must not decrypt there.
        vault
            .conn
            .execute(
                "UPDATE secrets SET nonce = (SELECT nonce FROM secrets WHERE id = 'algo/exchange/a'),
                 ciphertext = (SELECT ciphertext FROM secrets WHERE id = 'algo/exchange/a')
                 WHERE id = 'algo/exchange/b'",
                [],
            )
            .unwrap();
        assert!(vault.get("algo/exchange/b").is_err());
        assert!(!vault.verify("algo/exchange/b"));
        assert!(vault.verify("algo/exchange/a"));
    }

    #[test]
    fn a_copy_of_the_database_alone_decrypts_nothing() {
        let dir = temp_dir("copy");
        let vault = Vault::open(&dir, &MemoryKeyStore::default(), None).unwrap();
        vault.set("algo/exchange/a", b"hunter2").unwrap();
        drop(vault);
        // Another machine: same vault.db, a keystore without this vault's key.
        let err = Vault::open(&dir, &MemoryKeyStore::default(), None).err().unwrap();
        assert!(matches!(err, OpenError::Failed(_)), "{err:?}");
        for file in [DB_FILE, "vault.db-wal"] {
            let raw = std::fs::read(dir.join(file)).unwrap_or_default();
            assert!(!raw.windows(7).any(|w| w == b"hunter2"), "{file}");
        }
    }

    #[test]
    fn missing_keystore_falls_back_to_the_passphrase_file() {
        let dir = temp_dir("fallback");
        // No passphrase: the vault is not created silently.
        let err = Vault::open_with(&dir, &NoKeyStore, None, FAST).err().unwrap();
        assert!(matches!(err, OpenError::NeedsPassphrase(_)), "{err:?}");
        // Too short.
        let err = Vault::open_with(&dir, &NoKeyStore, Some("short"), FAST).err().unwrap();
        assert!(matches!(err, OpenError::NeedsPassphrase(_)), "{err:?}");

        let vault = Vault::open_with(&dir, &NoKeyStore, Some("correct horse battery"), FAST).unwrap();
        assert_eq!(vault.source(), MasterSource::PassphraseFile);
        vault.set("server/identity", b"identity-key").unwrap();
        drop(vault);
        assert!(dir.join(KEY_FILE).exists());

        let err = Vault::open_with(&dir, &NoKeyStore, None, FAST).err().unwrap();
        assert!(matches!(err, OpenError::NeedsPassphrase(_)), "{err:?}");
        let err = Vault::open_with(&dir, &NoKeyStore, Some("wrong horse battery"), FAST).err().unwrap();
        assert_eq!(err, OpenError::WrongPassphrase);

        let vault = Vault::open_with(&dir, &NoKeyStore, Some("correct horse battery"), FAST).unwrap();
        assert_eq!(vault.get_string("server/identity").unwrap().unwrap().as_str(), "identity-key");
        // A keystore appearing later does not change an existing passphrase vault.
        drop(vault);
        let vault = Vault::open_with(&dir, &MemoryKeyStore::default(), Some("correct horse battery"), FAST).unwrap();
        assert_eq!(vault.source(), MasterSource::PassphraseFile);
    }

    #[test]
    fn a_lost_key_with_nothing_stored_starts_a_new_vault() {
        let dir = temp_dir("lost");
        drop(Vault::open(&dir, &MemoryKeyStore::default(), None).unwrap());
        let vault = Vault::open(&dir, &MemoryKeyStore::default(), None).unwrap();
        assert_eq!(vault.count().unwrap(), 0);
        vault.set("x", b"y").unwrap();
    }

    #[test]
    fn nothing_public_carries_a_value() {
        let dir = temp_dir("public");
        let vault = Vault::open(&dir, &MemoryKeyStore::default(), None).unwrap();
        let info = vault.set("algo/exchange/a", b"sk-very-secret-value").unwrap();
        let listed = vault.list("algo/exchange/").unwrap();
        assert_eq!(listed, vec![info.clone()]);
        let json = serde_json::to_string(&(info, listed)).unwrap();
        assert!(!json.contains("sk-very-secret-value"));
        assert!(vault.list("server/").unwrap().is_empty());
    }
}
