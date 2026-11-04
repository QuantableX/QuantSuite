//! The `.qsbundle` export (PLAN-MIGRATION §2.1, card Migration 2/4).
//!
//! One file, `<machine>-<date>.qsbundle`: a header, then a tar stream,
//! compressed with zstd, encrypted as a whole in authenticated chunks
//! (AES-256-GCM, the STREAM construction: a nonce prefix, a chunk counter
//! and a last-chunk flag, the header as associated data). The key comes
//! from the passphrase with Argon2id; a lost passphrase means an unreadable
//! bundle. A flipped, dropped, reordered or truncated chunk fails.
//!
//! Inside the tar: the data root's files the inventory says move
//! (`quantsuite/<path>`), the vault's secrets re-encrypted under a key
//! derived from the bundle key (`secrets.json`; never plaintext on disk),
//! and `manifest.json` last: the suite version, the source machine and root,
//! per file its size, SHA-256, class and (databases) schema version, and
//! what was left out and why. Databases are copied with SQLite's online
//! backup, a consistent snapshot while the app runs.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::inventory::{self, Class};
use crate::vault::KdfParams;

const MAGIC: &[u8; 10] = b"QSBUNDLE1\n";
/// Plaintext per chunk.
const CHUNK: usize = 1 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    version: u32,
    kdf: String,
    params: KdfParams,
    salt: String,
    nonce_prefix: String,
    chunk: usize,
}

fn derive(passphrase: &str, salt: &[u8], params: KdfParams) -> Result<Zeroizing<[u8; 32]>, String> {
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(32)).map_err(|e| e.to_string())?,
    );
    let mut key = Zeroizing::new([0u8; 32]);
    argon.hash_password_into(passphrase.as_bytes(), salt, &mut key[..]).map_err(|e| e.to_string())?;
    Ok(key)
}

fn chunk_nonce(prefix: &[u8; 7], counter: u32, last: bool) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..7].copy_from_slice(prefix);
    n[7..11].copy_from_slice(&counter.to_be_bytes());
    n[11] = last as u8;
    n
}

/// Encrypts what is written to it, chunk by chunk.
struct SealWriter<W: Write> {
    inner: W,
    cipher: Aes256Gcm,
    prefix: [u8; 7],
    counter: u32,
    buf: Vec<u8>,
    ad: Vec<u8>,
}

impl<W: Write> SealWriter<W> {
    fn seal(&mut self, last: bool) -> io::Result<()> {
        let nonce = chunk_nonce(&self.prefix, self.counter, last);
        let ct = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: &self.buf, aad: &self.ad })
            .map_err(|_| io::Error::other("encryption failed"))?;
        self.inner.write_all(&(ct.len() as u32).to_le_bytes())?;
        self.inner.write_all(&ct)?;
        self.buf.clear();
        self.counter = self.counter.checked_add(1).ok_or_else(|| io::Error::other("bundle too large"))?;
        Ok(())
    }

    fn finish(mut self) -> io::Result<W> {
        self.seal(true)?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for SealWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let room = CHUNK - self.buf.len();
        let take = room.min(data.len());
        self.buf.extend_from_slice(&data[..take]);
        if self.buf.len() == CHUNK {
            self.seal(false)?;
        }
        Ok(take)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Decrypts and authenticates chunk by chunk; a missing last chunk fails.
struct OpenReader<R: Read> {
    inner: R,
    cipher: Aes256Gcm,
    prefix: [u8; 7],
    counter: u32,
    buf: Zeroizing<Vec<u8>>,
    pos: usize,
    done: bool,
    ad: Vec<u8>,
}

impl<R: Read> OpenReader<R> {
    fn next_chunk(&mut self) -> io::Result<()> {
        let mut len = [0u8; 4];
        if let Err(e) = self.inner.read_exact(&mut len) {
            return Err(if e.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(io::ErrorKind::InvalidData, "the bundle is truncated")
            } else {
                e
            });
        }
        let len = u32::from_le_bytes(len) as usize;
        if len > CHUNK + 16 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the bundle is damaged"));
        }
        let mut ct = vec![0u8; len];
        self.inner
            .read_exact(&mut ct)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "the bundle is truncated"))?;
        for last in [false, true] {
            let nonce = chunk_nonce(&self.prefix, self.counter, last);
            if let Ok(pt) = self.cipher.decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: &self.ad }) {
                self.buf = Zeroizing::new(pt);
                self.pos = 0;
                self.done = last;
                self.counter += 1;
                return Ok(());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            if self.counter == 0 { "wrong passphrase, or not a QuantSuite bundle" } else { "the bundle was changed after it was written" },
        ))
    }
}

impl<R: Read> Read for OpenReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.done {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn header_bytes(h: &Header) -> Vec<u8> {
    serde_json::to_vec(h).expect("header serializes")
}

// ── the manifest ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    /// `quantsuite` (the data root).
    pub root: String,
    /// `/`-separated, relative to the root.
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub class: String,
    /// SQLite `user_version` for a database copied with the backup API.
    #[serde(default)]
    pub schema_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeftOut {
    pub pattern: String,
    pub reason: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub suite_version: String,
    pub created_at: String,
    pub machine: String,
    pub source_root: String,
    pub files: Vec<FileEntry>,
    pub left_out: Vec<LeftOut>,
    /// Ids of the secrets in `secrets.json`, never their values.
    pub secrets: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct SealedSecret {
    id: String,
    nonce: String,
    ciphertext: String,
}

fn secrets_key(bundle_key: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(b"quantsuite-bundle:secrets:");
    h.update(bundle_key);
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&h.finalize());
    out
}

// ── export ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExportOptions {
    /// Optional rules to include or leave out against their default
    /// (pattern → included).
    #[serde(default)]
    pub include: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current: String,
}

/// Whether a rule's files go in, given the options.
fn included(rule: &inventory::Rule, opts: &ExportOptions) -> bool {
    match rule.class {
        Class::Must => true,
        Class::Optional { on } => opts.include.get(rule.pattern).copied().unwrap_or(on),
        Class::PerMachine | Class::Never => false,
    }
}

/// What trading is armed in the data root's QuantAlgo database, as a reason
/// to refuse; `None` when nothing trades live.
pub fn live_trading_armed(root: &Path) -> Option<String> {
    let db = root.join("modules").join("algo").join("quantalgo.db");
    let conn = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    let bots = count("SELECT COUNT(*) FROM bots WHERE status = 'running' AND trading_mode = 'live'");
    let systems = count("SELECT COUNT(*) FROM manual_execution WHERE armed = 1");
    (bots + systems > 0).then(|| {
        format!(
            "live trading is on: {bots} live bot(s) running, {systems} Manual system(s) armed. Stop them first \
             (\"Stop all trading\" on the Trading overview; it never sells)."
        )
    })
}

struct Plan {
    files: Vec<(PathBuf, String, &'static inventory::Rule)>,
    left_out: BTreeMap<&'static str, LeftOut>,
    total: u64,
    unclassified: Vec<String>,
}

/// What an export with `opts` would hold, before it starts.
#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub files: usize,
    pub bytes: u64,
    pub left_out: Vec<LeftOut>,
    pub unclassified: Vec<String>,
    /// The optional rules and whether each is in.
    pub optional: Vec<(String, String, bool)>,
}

pub fn plan_summary(root: &Path, opts: &ExportOptions) -> PlanSummary {
    let p = plan(root, opts, Path::new("\0no-skip"));
    PlanSummary {
        files: p.files.len(),
        bytes: p.total,
        left_out: p.left_out.into_values().collect(),
        unclassified: p.unclassified,
        optional: inventory::RULES
            .iter()
            .filter(|r| matches!(r.class, Class::Optional { .. }))
            .map(|r| (r.pattern.to_string(), r.reason.to_string(), included(r, opts)))
            .collect(),
    }
}

fn plan(root: &Path, opts: &ExportOptions, skip: &Path) -> Plan {
    let mut p = Plan { files: Vec::new(), left_out: BTreeMap::new(), total: 0, unclassified: Vec::new() };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.starts_with(skip) {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // A folder rule that leaves its whole folder out is not walked
                // (artifacts: hundreds of GB of scratch).
                if let Some(rule) = inventory::classify(&format!("{rel}/")) {
                    if (rule.pattern.trim_end_matches('/') == rel.as_str() || glob_dir(rule.pattern, &rel)) && !included(rule, opts) {
                        {
                            // Not walked, so not measured.
                            p.left_out.entry(rule.pattern).or_insert_with(|| LeftOut {
                                pattern: rule.pattern.into(),
                                reason: rule.reason.into(),
                                files: 0,
                                bytes: 0,
                            });
                            continue;
                        }
                    }
                }
                stack.push(path);
                continue;
            }
            // The vault moves as re-encrypted secrets, the WAL inside the backup.
            if rel.starts_with("secrets/") || rel.ends_with("-wal") || rel.ends_with("-shm") {
                continue;
            }
            match inventory::classify(&rel) {
                Some(rule) if included(rule, opts) => {
                    p.total += meta.len();
                    p.files.push((path, rel, rule));
                }
                Some(rule) => {
                    let lo = p.left_out.entry(rule.pattern).or_insert_with(|| LeftOut {
                        pattern: rule.pattern.into(),
                        reason: rule.reason.into(),
                        files: 0,
                        bytes: 0,
                    });
                    lo.files += 1;
                    lo.bytes += meta.len();
                }
                None => p.unclassified.push(rel),
            }
        }
    }
    p.files.sort_by(|a, b| a.1.cmp(&b.1));
    p
}

/// A folder rule pattern (`workspaces/*/artifacts/`) naming exactly `rel`.
fn glob_dir(pattern: &str, rel: &str) -> bool {
    let pat: Vec<&str> = pattern.trim_end_matches('/').split('/').collect();
    let segs: Vec<&str> = rel.split('/').collect();
    pat.len() == segs.len() && pat.iter().zip(&segs).all(|(p, s)| *p == "*" || p.eq_ignore_ascii_case(s))
}

/// Free bytes where `path` lives.
fn available_space(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

struct HashRead<R: Read> {
    inner: R,
    hash: Sha256,
    n: u64,
}

impl<R: Read> Read for HashRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hash.update(&buf[..n]);
        self.n += n as u64;
        Ok(n)
    }
}

/// Export the data root `root` into `dest_dir`. `secrets` are the vault's
/// (id, value) pairs, held in memory only. Returns the bundle's path and its
/// manifest. A cancelled or failed export leaves no file behind.
#[allow(clippy::too_many_arguments)]
pub fn export(
    root: &Path,
    dest_dir: &Path,
    passphrase: &str,
    opts: &ExportOptions,
    secrets: &[(String, Zeroizing<Vec<u8>>)],
    params: KdfParams,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<(PathBuf, Manifest), String> {
    if passphrase.chars().count() < 12 {
        return Err("The passphrase needs at least 12 characters.".into());
    }
    if let Some(why) = live_trading_armed(root) {
        return Err(format!("The export refuses while {why}"));
    }
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("{}: {e}", dest_dir.display()))?;
    let plan = plan(root, opts, dest_dir);
    if !plan.unclassified.is_empty() {
        return Err(format!(
            "{} file(s) have no rule in the data inventory, e.g. {}; classify them first (qs_core::inventory::RULES)",
            plan.unclassified.len(),
            plan.unclassified.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if let Some(free) = available_space(dest_dir) {
        let need = plan.total + plan.total / 10 + 64 * 1024 * 1024;
        if free < need {
            return Err(format!(
                "Not enough free space: the export needs up to {:.1} GB, {} has {:.1} GB.",
                need as f64 / 1e9,
                dest_dir.display(),
                free as f64 / 1e9
            ));
        }
    }

    let machine = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "machine".into());
    let stamp = chrono::Utc::now();
    let name = format!("{}-{}.qsbundle", machine.to_lowercase(), stamp.format("%Y-%m-%d"));
    let target = dest_dir.join(&name);
    let part = dest_dir.join(format!("{name}.part"));
    let temp_db = dest_dir.join(format!(".{name}.db-snapshot"));

    let result = (|| -> Result<Manifest, String> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let mut prefix = [0u8; 7];
        OsRng.fill_bytes(&mut prefix);
        let header = Header {
            version: 1,
            kdf: "argon2id".into(),
            params,
            salt: hex::encode(salt),
            nonce_prefix: hex::encode(prefix),
            chunk: CHUNK,
        };
        let key = derive(passphrase, &salt, params)?;
        let head = header_bytes(&header);
        let mut file = io::BufWriter::new(std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?);
        file.write_all(MAGIC).map_err(|e| e.to_string())?;
        file.write_all(&(head.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&head).map_err(|e| e.to_string())?;
        let sealer = SealWriter {
            inner: file,
            cipher: Aes256Gcm::new_from_slice(&key[..]).map_err(|e| e.to_string())?,
            prefix,
            counter: 0,
            buf: Vec::with_capacity(CHUNK),
            ad: head.clone(),
        };
        let zst = zstd::stream::write::Encoder::new(sealer, 3).map_err(|e| e.to_string())?;
        let mut tar = tar::Builder::new(zst);
        tar.mode(tar::HeaderMode::Deterministic);

        let mut files = Vec::new();
        let mut done = 0u64;
        for (path, rel, rule) in &plan.files {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            progress(ExportProgress { done_bytes: done, total_bytes: plan.total, current: rel.clone() });
            let is_db = rel.ends_with(".db") || rel.ends_with(".sqlite");
            let (source, schema_version) = if is_db {
                // A consistent snapshot of a live database.
                let _ = std::fs::remove_file(&temp_db);
                let src = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(|e| format!("{rel}: {e}"))?;
                let mut dst = rusqlite::Connection::open(&temp_db).map_err(|e| format!("{rel}: {e}"))?;
                {
                    let backup = rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| format!("{rel}: {e}"))?;
                    backup.run_to_completion(256, std::time::Duration::from_millis(5), None).map_err(|e| format!("{rel}: {e}"))?;
                }
                let version: i64 = dst.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);
                drop(dst);
                (temp_db.clone(), Some(version))
            } else {
                (path.clone(), None)
            };
            let size = std::fs::metadata(&source).map_err(|e| format!("{rel}: {e}"))?.len();
            let mut header = tar::Header::new_gnu();
            header.set_size(size);
            header.set_mode(0o644);
            header.set_cksum();
            let reader = std::fs::File::open(&source).map_err(|e| format!("{rel}: {e}"))?;
            let mut hashed = HashRead { inner: reader, hash: Sha256::new(), n: 0 };
            tar.append_data(&mut header, format!("quantsuite/{rel}"), &mut hashed).map_err(|e| format!("{rel}: {e}"))?;
            if hashed.n != size {
                return Err(format!("{rel} changed size while it was exported"));
            }
            files.push(FileEntry {
                root: "quantsuite".into(),
                path: rel.clone(),
                size,
                sha256: hex::encode(hashed.hash.finalize()),
                class: format!("{:?}", rule.class),
                schema_version,
            });
            done += size;
        }
        let _ = std::fs::remove_file(&temp_db);

        // The secrets, re-encrypted in memory under the bundle's own key.
        let skey = secrets_key(&key);
        let cipher = Aes256Gcm::new_from_slice(&skey[..]).map_err(|e| e.to_string())?;
        let mut sealed = Vec::new();
        for (id, value) in secrets {
            let mut nonce = [0u8; 12];
            OsRng.fill_bytes(&mut nonce);
            let ct = cipher
                .encrypt(Nonce::from_slice(&nonce), Payload { msg: value, aad: id.as_bytes() })
                .map_err(|_| "encrypting a secret failed".to_string())?;
            sealed.push(SealedSecret { id: id.clone(), nonce: hex::encode(nonce), ciphertext: hex::encode(ct) });
        }
        let body = serde_json::to_vec(&sealed).map_err(|e| e.to_string())?;
        let mut h = tar::Header::new_gnu();
        h.set_size(body.len() as u64);
        h.set_mode(0o600);
        h.set_cksum();
        tar.append_data(&mut h, "secrets.json", body.as_slice()).map_err(|e| e.to_string())?;

        let manifest = Manifest {
            format: 1,
            suite_version: env!("CARGO_PKG_VERSION").into(),
            created_at: stamp.to_rfc3339(),
            machine: machine.clone(),
            source_root: root.to_string_lossy().into_owned(),
            files,
            left_out: plan.left_out.values().cloned().collect(),
            secrets: secrets.iter().map(|(id, _)| id.clone()).collect(),
        };
        let body = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        let mut h = tar::Header::new_gnu();
        h.set_size(body.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        tar.append_data(&mut h, "manifest.json", body.as_slice()).map_err(|e| e.to_string())?;

        let zst = tar.into_inner().map_err(|e| e.to_string())?;
        let sealer = zst.finish().map_err(|e| e.to_string())?;
        let mut file = sealer.finish().map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        progress(ExportProgress { done_bytes: plan.total, total_bytes: plan.total, current: "done".into() });
        Ok(manifest)
    })();

    match result {
        Ok(manifest) => {
            std::fs::rename(&part, &target).map_err(|e| format!("{}: {e}", target.display()))?;
            Ok((target, manifest))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            let _ = std::fs::remove_file(&temp_db);
            Err(e)
        }
    }
}

/// Cancels the running export of [`export_suite`].
pub static CANCEL: AtomicBool = AtomicBool::new(false);

/// Export this suite's data root with the vault's secrets into `dest_dir` —
/// what Settings → Data and the agent tool run. `progress` gets every file.
pub fn export_suite(
    dest_dir: &Path,
    passphrase: &str,
    include: BTreeMap<String, bool>,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<serde_json::Value, String> {
    CANCEL.store(false, Ordering::Relaxed);
    // The vault's secrets, in memory only.
    let secrets = crate::vault::with_vault(|v| {
        let mut out = Vec::new();
        for info in v.list("")? {
            if let Some(value) = v.get(&info.id)? {
                out.push((info.id, value));
            }
        }
        Ok(out)
    })
    .map_err(|e| format!("The vault cannot be read, so its secrets cannot go into the bundle: {e}"))?;
    let opts = ExportOptions { include };
    let (path, manifest) = export(
        &crate::paths::root(),
        dest_dir,
        passphrase,
        &opts,
        &secrets,
        KdfParams::default(),
        &CANCEL,
        progress,
    )?;
    Ok(serde_json::json!({
        "path": path,
        "files": manifest.files.len(),
        "bytes": manifest.files.iter().map(|f| f.size).sum::<u64>(),
        "secrets": manifest.secrets.len(),
        "left_out": manifest.left_out,
    }))
}

// ── open ────────────────────────────────────────────────────────────────────

/// What opening a bundle found.
pub struct Opened {
    pub manifest: Manifest,
    /// The secrets, decrypted, in memory.
    pub secrets: Vec<(String, Zeroizing<Vec<u8>>)>,
}

/// Open and verify a bundle: the passphrase, every chunk, every file's
/// checksum against the manifest. With `unpack_to`, the files are written
/// there (`<root>/<path>`) — a staging folder, never a live data root.
pub fn open(bundle: &Path, passphrase: &str, unpack_to: Option<&Path>) -> Result<Opened, String> {
    let mut file = io::BufReader::new(std::fs::File::open(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?);
    let mut magic = [0u8; 10];
    file.read_exact(&mut magic).map_err(|_| "not a QuantSuite bundle".to_string())?;
    if &magic != MAGIC {
        return Err("not a QuantSuite bundle".into());
    }
    let mut len = [0u8; 4];
    file.read_exact(&mut len).map_err(|_| "the bundle is damaged".to_string())?;
    let mut head = vec![0u8; u32::from_le_bytes(len) as usize];
    file.read_exact(&mut head).map_err(|_| "the bundle is damaged".to_string())?;
    let header: Header = serde_json::from_slice(&head).map_err(|_| "the bundle is damaged".to_string())?;
    if header.version != 1 || header.kdf != "argon2id" {
        return Err("this bundle was written by a newer QuantSuite".into());
    }
    let salt = hex::decode(&header.salt).map_err(|_| "the bundle is damaged".to_string())?;
    let prefix: [u8; 7] = hex::decode(&header.nonce_prefix)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| "the bundle is damaged".to_string())?;
    let key = derive(passphrase, &salt, header.params)?;
    let opener = OpenReader {
        inner: file,
        cipher: Aes256Gcm::new_from_slice(&key[..]).map_err(|e| e.to_string())?,
        prefix,
        counter: 0,
        buf: Zeroizing::new(Vec::new()),
        pos: 0,
        done: false,
        ad: head,
    };
    let zst = zstd::stream::read::Decoder::new(opener).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(zst);
    let mut hashes: BTreeMap<String, (u64, String)> = BTreeMap::new();
    let mut manifest: Option<Manifest> = None;
    let mut sealed: Vec<SealedSecret> = Vec::new();
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry.path().map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        if name == "manifest.json" {
            let mut body = Vec::new();
            entry.read_to_end(&mut body).map_err(|e| e.to_string())?;
            manifest = Some(serde_json::from_slice(&body).map_err(|e| format!("manifest: {e}"))?);
            continue;
        }
        if name == "secrets.json" {
            let mut body = Vec::new();
            entry.read_to_end(&mut body).map_err(|e| e.to_string())?;
            sealed = serde_json::from_slice(&body).map_err(|e| format!("secrets: {e}"))?;
            continue;
        }
        if name.contains("..") {
            return Err(format!("unsafe path in the bundle: {name}"));
        }
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut out = match unpack_to {
            Some(dir) => {
                let target = dir.join(&name);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                Some(io::BufWriter::new(std::fs::File::create(&target).map_err(|e| e.to_string())?))
            }
            None => None,
        };
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf).map_err(|e| format!("{name}: {e}"))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            size += n as u64;
            if let Some(w) = out.as_mut() {
                w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            }
        }
        hashes.insert(name, (size, hex::encode(hash.finalize())));
    }
    let manifest = manifest.ok_or("the bundle has no manifest (truncated?)")?;
    for f in &manifest.files {
        let key = format!("{}/{}", f.root, f.path);
        match hashes.get(&key) {
            Some((size, sha)) if *size == f.size && *sha == f.sha256 => {}
            Some(_) => return Err(format!("{key} does not match its checksum")),
            None => return Err(format!("{key} is missing from the bundle")),
        }
    }
    let skey = secrets_key(&key);
    let cipher = Aes256Gcm::new_from_slice(&skey[..]).map_err(|e| e.to_string())?;
    let mut secrets = Vec::new();
    for s in sealed {
        let nonce = hex::decode(&s.nonce).map_err(|_| "secrets damaged".to_string())?;
        let ct = hex::decode(&s.ciphertext).map_err(|_| "secrets damaged".to_string())?;
        let value = cipher
            .decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: s.id.as_bytes() })
            .map_err(|_| format!("secret {} does not decrypt", s.id))?;
        secrets.push((s.id, Zeroizing::new(value)));
    }
    Ok(Opened { manifest, secrets })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: KdfParams = KdfParams { m_cost_kib: 256, t_cost: 1, p_cost: 1 };

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn root() -> Tmp {
        let dir = std::env::temp_dir().join(format!("qs-bundle-{}", uuid::Uuid::new_v4()));
        let write = |rel: &str, body: &[u8]| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        };
        write("modules/algo/config.json", br#"{"strategy_dir":"C:/Projects/x"}"#);
        write("modules/notes/notes.md", b"my note");
        write("logs/quantsuite.log", b"log line");
        write("workspaces/abc/artifacts/big.bin", &[7u8; 4096]);
        write("workspaces/abc/canvas/canvas.json", b"{}");
        write("modules/hud/pyenv/python.exe", b"binary");
        let conn = rusqlite::Connection::open(dir.join("core.db")).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=7; CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1),(2),(3);").unwrap();
        drop(conn);
        Tmp(dir)
    }

    fn secrets() -> Vec<(String, Zeroizing<Vec<u8>>)> {
        vec![("algo/exchange/e1".into(), Zeroizing::new(b"{\"api_secret\":\"SK-very-secret\"}".to_vec()))]
    }

    fn run(root: &Path, dest: &Path, pass: &str, opts: &ExportOptions) -> Result<(PathBuf, Manifest), String> {
        export(root, dest, pass, opts, &secrets(), FAST, &AtomicBool::new(false), &mut |_| {})
    }

    #[test]
    fn a_bundle_round_trips_and_its_manifest_follows_the_inventory() {
        let r = root();
        let dest = Tmp(r.0.with_extension("out"));
        let (path, manifest) = run(&r.0, &dest.0, "correct horse battery", &ExportOptions::default()).unwrap();
        let names: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, vec!["core.db", "modules/algo/config.json", "modules/notes/notes.md", "workspaces/abc/canvas/canvas.json"]);
        assert_eq!(manifest.files[0].schema_version, Some(7));
        let left: Vec<&str> = manifest.left_out.iter().map(|l| l.pattern.as_str()).collect();
        assert!(left.contains(&"logs/") && left.contains(&"workspaces/*/artifacts/") && left.contains(&"modules/hud/pyenv/"), "{left:?}");
        assert_eq!(manifest.secrets, vec!["algo/exchange/e1"]);
        // Nothing readable in the file: not the note, not the secret.
        let raw = std::fs::read(&path).unwrap();
        for needle in [&b"my note"[..], b"SK-very-secret", b"strategy_dir"] {
            assert!(!raw.windows(needle.len()).any(|w| w == needle));
        }
        let stage = Tmp(r.0.with_extension("stage"));
        let opened = open(&path, "correct horse battery", Some(&stage.0)).unwrap();
        assert_eq!(opened.manifest.files, manifest.files);
        assert_eq!(&opened.secrets[0].1[..], b"{\"api_secret\":\"SK-very-secret\"}");
        assert_eq!(std::fs::read(stage.0.join("quantsuite/modules/notes/notes.md")).unwrap(), b"my note");
        let db = rusqlite::Connection::open(stage.0.join("quantsuite/core.db")).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM t", [], |r| r.get::<_, i64>(0)).unwrap(), 3);
        // Ticking an optional part puts it in.
        let mut opts = ExportOptions::default();
        opts.include.insert("logs/".into(), true);
        let (_, m) = run(&r.0, &dest.0, "correct horse battery", &opts).unwrap();
        assert!(m.files.iter().any(|f| f.path == "logs/quantsuite.log"));
    }

    #[test]
    fn a_wrong_passphrase_a_changed_chunk_and_a_cut_file_all_fail() {
        let r = root();
        let dest = Tmp(r.0.with_extension("out"));
        let (path, _) = run(&r.0, &dest.0, "correct horse battery", &ExportOptions::default()).unwrap();
        assert!(open(&path, "wrong horse battery", None).err().unwrap().contains("wrong passphrase"));
        let raw = std::fs::read(&path).unwrap();
        let mut tampered = raw.clone();
        let at = tampered.len() - 40;
        tampered[at] ^= 0x01;
        let bad = dest.0.join("tampered.qsbundle");
        std::fs::write(&bad, &tampered).unwrap();
        let err = open(&bad, "correct horse battery", None).err().unwrap();
        assert!(err.contains("changed") || err.contains("passphrase"), "{err}");
        std::fs::write(&bad, &raw[..raw.len() - 30]).unwrap();
        assert!(open(&bad, "correct horse battery", None).is_err(), "a cut bundle fails");
        assert!(run(&r.0, &dest.0, "short", &ExportOptions::default()).is_err(), "a short passphrase is refused");
    }

    #[test]
    fn unclassified_files_and_armed_live_trading_refuse_the_export() {
        let r = root();
        let dest = Tmp(r.0.with_extension("out"));
        std::fs::write(r.0.join("surprise.dat"), b"?").unwrap();
        assert!(run(&r.0, &dest.0, "correct horse battery", &ExportOptions::default()).err().unwrap().contains("surprise.dat"));
        std::fs::remove_file(r.0.join("surprise.dat")).unwrap();
        let algo = r.0.join("modules/algo/quantalgo.db");
        let conn = rusqlite::Connection::open(&algo).unwrap();
        conn.execute_batch(
            "CREATE TABLE bots (id TEXT, status TEXT, trading_mode TEXT); INSERT INTO bots VALUES ('b', 'running', 'live');
             CREATE TABLE manual_execution (system_id TEXT, armed INTEGER);",
        )
        .unwrap();
        drop(conn);
        let err = run(&r.0, &dest.0, "correct horse battery", &ExportOptions::default()).err().unwrap();
        assert!(err.contains("1 live bot"), "{err}");
        assert!(std::fs::read_dir(&dest.0).unwrap().next().is_none(), "nothing half-written");
    }

    #[test]
    fn a_database_written_during_the_export_is_a_consistent_snapshot() {
        let r = root();
        let dest = Tmp(r.0.with_extension("out"));
        let db = r.0.join("core.db");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let writer = {
            let (db, stop) = (db.clone(), stop.clone());
            std::thread::spawn(move || {
                let conn = rusqlite::Connection::open(&db).unwrap();
                let mut n = 0;
                while !stop.load(Ordering::Relaxed) && n < 2_000 {
                    conn.execute("INSERT INTO t VALUES (?1)", [n]).unwrap();
                    n += 1;
                }
            })
        };
        let (path, _) = run(&r.0, &dest.0, "correct horse battery", &ExportOptions::default()).unwrap();
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        let stage = Tmp(r.0.with_extension("stage"));
        open(&path, "correct horse battery", Some(&stage.0)).unwrap();
        let copy = rusqlite::Connection::open(stage.0.join("quantsuite/core.db")).unwrap();
        let ok: String = copy.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(ok, "ok");
        assert!(copy.query_row("SELECT COUNT(*) FROM t", [], |r| r.get::<_, i64>(0)).unwrap() >= 3);
    }
}

/// Acceptance: export a populated profile (a copy of a real data root, never
/// the live one), open it with the passphrase, compare the manifest.
/// `QS_BUNDLE_ROOT=<copy> QS_BUNDLE_OUT=<dir> cargo test -p tauri-plugin-qs --lib profile_export -- --ignored --nocapture`
#[cfg(test)]
mod profile_export {
    use super::*;

    #[test]
    #[ignore = "exports a copy of a real data root"]
    fn profile_export_round_trip() {
        let root = PathBuf::from(std::env::var("QS_BUNDLE_ROOT").expect("QS_BUNDLE_ROOT"));
        let out = PathBuf::from(std::env::var("QS_BUNDLE_OUT").expect("QS_BUNDLE_OUT"));
        let secrets = vec![("qa/secret".to_string(), Zeroizing::new(b"qa-value".to_vec()))];
        let t = std::time::Instant::now();
        let (path, manifest) = export(&root, &out, "qa passphrase 2026", &ExportOptions::default(), &secrets, KdfParams::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        let bytes: u64 = manifest.files.iter().map(|f| f.size).sum();
        println!("exported {} files, {:.1} MB in, bundle {:.1} MB, {:.1}s", manifest.files.len(), bytes as f64 / 1e6, size as f64 / 1e6, t.elapsed().as_secs_f64());
        let dbs = manifest.files.iter().filter(|f| f.schema_version.is_some()).count();
        println!("  {} databases via the backup API; left out: {:?}", dbs, manifest.left_out.iter().map(|l| l.pattern.as_str()).collect::<Vec<_>>());
        let t = std::time::Instant::now();
        let opened = open(&path, "qa passphrase 2026", None).unwrap();
        println!("opened and verified every checksum in {:.1}s", t.elapsed().as_secs_f64());
        assert_eq!(opened.manifest.files, manifest.files);
        assert_eq!(&opened.secrets[0].1[..], b"qa-value");
        assert!(open(&path, "not the passphrase", None).is_err());
        // Every file of the profile the inventory moves is in the manifest.
        let expected = plan(&root, &ExportOptions::default(), &out).files.len();
        assert_eq!(manifest.files.len(), expected);
        std::fs::remove_file(&path).unwrap();
    }
}
