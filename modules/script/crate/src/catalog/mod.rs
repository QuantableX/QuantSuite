//! The QuantScript Collection: install single indicators — with their
//! requirements and all their versions — from a catalog into the private
//! library (the format: FORMAT.md of QuantScript-Collection-Public / -Private).
//!
//! Every file is verified on the way in: the catalog names each manifest's
//! sha256, each manifest names its files' sha256. An install is planned
//! first (create / replace / skip / conflict per package), then staged — the
//! current library with the new files written in, verified by the engine
//! (`quantscript stage-check`) — and only then written. `<library>/collection.json`
//! remembers what the Collection installed, so an update can tell its own files
//! from the user's, and a removal can refuse while another item needs it.

pub mod fetch;
pub mod sources;
pub mod tokens;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::store::{self as history, sha256_hex};
use fetch::{safe_path, Fetch};
use sources::Source;

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ─── The catalog's documents ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRef {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub version: String,
    pub contract: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub scores: Value,
    pub manifest: ManifestRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub format: u32,
    #[serde(default)]
    pub generated_at: Option<String>,
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub key: String,
    pub name: String,
    pub version: String,
    pub contract: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub registers: Vec<String>,
    pub files: Vec<FileEntry>,
    /// Everything else (description, schema, versions, changelog …) for the UI.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A package with its files, every byte verified.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    /// (entry, bytes) — scripts, version files and the README.
    pub files: Vec<(FileEntry, Vec<u8>)>,
}

impl Package {
    /// Library-relative targets: the script at the root, versions under
    /// `versions/<key>/`. A README is shown, not installed.
    pub fn targets(&self) -> Vec<(String, &[u8])> {
        self.files
            .iter()
            .filter_map(|(entry, bytes)| target_of(&self.manifest.key, entry).map(|t| (t, bytes.as_slice())))
            .collect()
    }

    pub fn script(&self) -> String {
        format!("{}.py", self.manifest.key)
    }
}

fn target_of(key: &str, entry: &FileEntry) -> Option<String> {
    match entry.role.as_str() {
        "script" => Some(format!("{key}.py")),
        "version" => Path::new(&entry.path).file_name().map(|n| format!("versions/{key}/{}", n.to_string_lossy())),
        _ => None,
    }
}

pub fn load_catalog(fetch: &dyn Fetch, commit: &str) -> Result<Catalog, String> {
    let bytes = fetch.read(commit, "catalog.json")?;
    let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|e| format!("catalog.json is not a catalog: {e}"))?;
    if catalog.format != 1 {
        return Err(format!("catalog.json has format {} — this QuantSuite reads format 1.", catalog.format));
    }
    Ok(catalog)
}

fn contract_error(key: &str, contract: u32, engine: Option<u32>) -> Option<String> {
    match engine {
        Some(engine) if contract > engine => Some(format!(
            "{key} needs a newer QuantSuite (contract {contract}; this engine has {engine})."
        )),
        _ => None,
    }
}

/// The manifest and files of one catalog item, verified against the hashes.
pub fn load_package(fetch: &dyn Fetch, commit: &str, item: &CatalogItem, engine_contract: Option<u32>) -> Result<Package, String> {
    if !safe_path(&item.manifest.path) {
        return Err(format!("{}: '{}' is not a manifest path.", item.key, item.manifest.path));
    }
    let bytes = fetch.read(commit, &item.manifest.path)?;
    if sha256_hex(&bytes) != item.manifest.sha256 {
        return Err(format!("The manifest of {} does not match the catalog (sha256) — refusing it.", item.key));
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("The manifest of {} is not readable: {e}", item.key))?;
    if manifest.format != 1 || manifest.key != item.key {
        return Err(format!("The manifest at {} is not the package {}.", item.manifest.path, item.key));
    }
    if let Some(error) = contract_error(&manifest.key, manifest.contract, engine_contract) {
        return Err(error);
    }
    let dir = item.manifest.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut files = Vec::new();
    for entry in &manifest.files {
        if !safe_path(&entry.path) {
            return Err(format!("{}: '{}' is not a package path.", manifest.key, entry.path));
        }
        let path = if dir.is_empty() { entry.path.clone() } else { format!("{dir}/{}", entry.path) };
        let bytes = fetch.read(commit, &path)?;
        if sha256_hex(&bytes) != entry.sha256 {
            return Err(format!("{} of {} does not match its manifest (sha256) — refusing it.", entry.path, manifest.key));
        }
        files.push((entry.clone(), bytes));
    }
    if !files.iter().any(|(e, _)| e.role == "script" && e.path == format!("{}.py", manifest.key)) {
        return Err(format!("{} has no script {}.py.", manifest.key, manifest.key));
    }
    Ok(Package { manifest, files })
}

/// The chosen keys and everything they require, requirements first.
pub fn closure(catalog: &Catalog, keys: &[String]) -> Result<Vec<String>, String> {
    let by_key: BTreeMap<&str, &CatalogItem> = catalog.items.iter().map(|i| (i.key.as_str(), i)).collect();
    fn visit(key: &str, by_key: &BTreeMap<&str, &CatalogItem>, seen: &mut BTreeSet<String>, out: &mut Vec<String>, path: &mut Vec<String>) -> Result<(), String> {
        if seen.contains(key) || path.iter().any(|k| k == key) {
            return Ok(());
        }
        let item = by_key.get(key).ok_or_else(|| match path.last() {
            Some(parent) => format!("{parent} requires {key}, which the catalog does not have."),
            None => format!("{key} is not in the catalog."),
        })?;
        path.push(key.to_string());
        for needed in &item.requires {
            visit(needed, by_key, seen, out, path)?;
        }
        path.pop();
        seen.insert(key.to_string());
        out.push(key.to_string());
        Ok(())
    }
    let (mut seen, mut out) = (BTreeSet::new(), Vec::new());
    for key in keys {
        visit(key, &by_key, &mut seen, &mut out, &mut Vec::new())?;
    }
    Ok(out)
}

// ─── collection.json: what the Collection installed ──────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct InstalledItem {
    pub source: String,
    pub version: String,
    pub commit: String,
    pub installed_at: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub keys: Vec<String>,
    /// Library-relative path → sha256 of what the Collection wrote.
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Installed {
    pub format: u32,
    pub items: BTreeMap<String, InstalledItem>,
}

impl Default for Installed {
    fn default() -> Self {
        Self { format: 1, items: BTreeMap::new() }
    }
}

pub const STATE_FILE: &str = "collection.json";

/// The state file of the Store era, read until the next save replaces it.
const OLD_STATE_FILE: &str = "store.json";

pub fn load_installed(library: &Path) -> Result<Installed, String> {
    let path = if library.join(STATE_FILE).is_file() || !library.join(OLD_STATE_FILE).is_file() {
        library.join(STATE_FILE)
    } else {
        library.join(OLD_STATE_FILE)
    };
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{STATE_FILE} is not readable: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Installed::default()),
        Err(e) => Err(format!("Could not read {STATE_FILE}: {e}")),
    }
}

pub fn save_installed(library: &Path, installed: &Installed) -> Result<(), String> {
    let text = serde_json::to_string_pretty(installed).map_err(|e| e.to_string())? + "\n";
    qs_core::paths::write_atomic(&library.join(STATE_FILE), text.as_bytes()).map_err(|e| format!("Could not write {STATE_FILE}: {e}"))?;
    let _ = fs::remove_file(library.join(OLD_STATE_FILE));
    Ok(())
}

fn file_sha(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|b| sha256_hex(&b))
}

/// Files the Collection wrote that the user has changed since.
pub fn modified_files(library: &Path, item: &InstalledItem) -> Vec<String> {
    item.files
        .iter()
        .filter(|(path, sha)| file_sha(&library.join(path)).is_some_and(|now| &now != *sha))
        .map(|(path, _)| path.clone())
        .collect()
}

fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

pub fn newer(candidate: &str, installed: &str) -> bool {
    matches!((semver(candidate), semver(installed)), (Some(a), Some(b)) if a > b)
}

// ─── What the UI lists ─────────────────────────────────────────────────────

/// The catalog items with the library's state: `installed` (version),
/// `update`, `modified` (Collection files changed locally), `present` (a script
/// of that name the Collection did not install — installing it is a conflict
/// unless identical), `too_new` (the engine's contract is older).
pub fn catalog_view(library: &Path, installed: &Installed, catalog: &Catalog, engine_contract: Option<u32>) -> Vec<Value> {
    catalog
        .items
        .iter()
        .map(|item| {
            let record = installed.items.get(&item.key);
            let modified = record.map(|r| !modified_files(library, r).is_empty()).unwrap_or(false);
            let present = record.is_none() && library.join(format!("{}.py", item.key)).is_file();
            let mut value = serde_json::to_value(item).unwrap_or(Value::Null);
            value["local"] = json!({
                "installed_version": record.map(|r| r.version.clone()),
                "installed_commit": record.map(|r| r.commit.clone()),
                "update": record.is_some_and(|r| newer(&item.version, &r.version)),
                "modified": modified,
                "present": present,
                "conflict": present,
                "too_new": contract_error(&item.key, item.contract, engine_contract).is_some(),
            });
            value
        })
        .collect()
}

// ─── Planning ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanFile {
    pub path: String,
    /// `create` | `replace` | `same` | `remove`
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanItem {
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: String,
    pub installed_version: Option<String>,
    /// `create` | `replace` | `skip` | `conflict`
    pub action: String,
    pub reason: Option<String>,
    /// Chosen by the user (false: pulled in as a requirement).
    pub requested: bool,
    pub files: Vec<PlanFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub source: String,
    pub commit: String,
    pub items: Vec<PlanItem>,
    pub conflicts: usize,
}

pub fn plan_package(library: &Path, installed: &Installed, package: &Package, requested: bool) -> PlanItem {
    let key = &package.manifest.key;
    let record = installed.items.get(key);
    let modified = record.map(|r| modified_files(library, r)).unwrap_or_default();
    let mut files = Vec::new();
    let mut differs = false;
    let targets = package.targets();
    for (path, bytes) in &targets {
        let action = match fs::read(library.join(path)) {
            Ok(current) if current == *bytes => "same",
            Ok(_) => {
                differs = true;
                "replace"
            }
            Err(_) => "create",
        };
        files.push(PlanFile { path: path.clone(), action: action.into() });
    }
    if let Some(record) = record {
        for path in record.files.keys() {
            if !targets.iter().any(|(t, _)| t == path) && library.join(path).is_file() {
                files.push(PlanFile { path: path.clone(), action: "remove".into() });
            }
        }
    }
    let all_same = files.iter().all(|f| f.action == "same");
    let (action, reason) = if all_same {
        ("skip", Some(if record.is_some() { "installed — identical" } else { "already in the library — identical" }.to_string()))
    } else if record.is_some() && !modified.is_empty() {
        ("conflict", Some(format!("changed in the library since the Collection installed it: {}", modified.join(", "))))
    } else if record.is_some() {
        ("replace", None)
    } else if differs {
        ("conflict", Some("the library has files of this name the Collection did not install".to_string()))
    } else {
        ("create", None)
    };
    PlanItem {
        key: key.clone(),
        name: package.manifest.name.clone(),
        kind: package.manifest.kind.clone(),
        version: package.manifest.version.clone(),
        installed_version: record.map(|r| r.version.clone()),
        action: action.into(),
        reason,
        requested,
        files,
    }
}

/// Fetch, verify and plan the chosen keys with their requirements.
pub fn prepare(
    library: &Path,
    source: &Source,
    fetch: &dyn Fetch,
    commit: &str,
    keys: &[String],
    engine_contract: Option<u32>,
) -> Result<(Plan, Vec<Package>), String> {
    if keys.is_empty() {
        return Err("Pick at least one indicator.".into());
    }
    let catalog = load_catalog(fetch, commit)?;
    let order = closure(&catalog, keys)?;
    let installed = load_installed(library)?;
    let mut packages = Vec::new();
    let mut items = Vec::new();
    for key in &order {
        let item = catalog.items.iter().find(|i| &i.key == key).expect("closure keys are catalog keys");
        let package = load_package(fetch, commit, item, engine_contract)?;
        items.push(plan_package(library, &installed, &package, keys.contains(key)));
        packages.push(package);
    }
    let conflicts = items.iter().filter(|i| i.action == "conflict").count();
    Ok((Plan { source: source.id.clone(), commit: commit.to_string(), items, conflicts }, packages))
}

// ─── Installing ────────────────────────────────────────────────────────────

/// The engine's verdict on a staged library (`quantscript stage-check`).
pub trait Stager {
    fn check(&self, staged_library: &Path, files: &[String]) -> Result<Value, String>;
}

#[derive(Debug, Clone, Serialize)]
pub struct Written {
    pub key: String,
    pub file: String,
    /// The script did not exist before (a `script.file.created` event).
    pub created: bool,
    pub version: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub plan: Plan,
    pub written: Vec<Written>,
    pub check: Option<Value>,
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Stage {}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("Stage {}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "__pycache__" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(&name))?;
        } else {
            fs::copy(&path, to.join(&name)).map_err(|e| format!("Stage {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn stage_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("quantscript-store-{}-{nanos}", std::process::id()))
}

/// Plan, stage, verify, write, record. Conflicts refuse the whole install
/// unless `overwrite`; a staged library the engine cannot load writes nothing.
#[allow(clippy::too_many_arguments)]
pub fn install(
    library: &Path,
    db: &Mutex<Connection>,
    source: &Source,
    fetch: &dyn Fetch,
    commit: &str,
    keys: &[String],
    overwrite: bool,
    engine_contract: Option<u32>,
    stager: &dyn Stager,
) -> Result<InstallOutcome, String> {
    let (plan, packages) = prepare(library, source, fetch, commit, keys, engine_contract)?;
    let conflicts: Vec<String> = plan
        .items
        .iter()
        .filter(|i| i.action == "conflict")
        .map(|i| format!("{} ({})", i.key, i.reason.clone().unwrap_or_default()))
        .collect();
    if !conflicts.is_empty() && !overwrite {
        return Err(format!("Not installed — conflicts: {}. Confirm overwriting to replace them.", conflicts.join("; ")));
    }
    let work: Vec<(&PlanItem, &Package)> = plan.items.iter().zip(&packages).filter(|(i, _)| i.action != "skip").collect();
    if work.is_empty() {
        return Ok(InstallOutcome { plan, written: Vec::new(), check: None });
    }

    // Stage: the library as it is, with the new files written in.
    let staged = stage_dir();
    let check = (|| {
        copy_tree(library, &staged)?;
        for (item, package) in &work {
            for (path, bytes) in package.targets() {
                let target = staged.join(&path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::write(&target, bytes).map_err(|e| format!("Stage {path}: {e}"))?;
            }
            for file in item.files.iter().filter(|f| f.action == "remove") {
                let _ = fs::remove_file(staged.join(&file.path));
            }
        }
        let scripts: Vec<String> = work.iter().map(|(_, p)| p.script()).collect();
        stager.check(&staged, &scripts)
    })();
    let _ = fs::remove_dir_all(&staged);
    let check = check?;
    if check.get("blocking").and_then(Value::as_bool).unwrap_or(true) {
        return Err(format!("Not installed — the library would not load with these files: {}", blocking_reason(&check)));
    }

    // Write: requirements first; version files before their script. The
    // history's lock is held for the writes only, not for fetch and check.
    let conn = db.lock().map_err(|e| format!("Lock: {e}"))?;
    let conn = &*conn;
    let mut installed = load_installed(library)?;
    let mut written = Vec::new();
    let sha7 = commit.chars().take(7).collect::<String>();
    for (item, package) in &work {
        let key = &package.manifest.key;
        let mut targets = package.targets();
        targets.sort_by_key(|(path, _)| path.ends_with(".py"));
        let mut record_files = BTreeMap::new();
        for (path, bytes) in &targets {
            let target = library.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
            }
            record_files.insert(path.clone(), sha256_hex(bytes));
            let existed = target.is_file();
            if path.ends_with(".py") {
                let content = String::from_utf8(bytes.to_vec()).map_err(|_| format!("{path} of {key} is not UTF-8 text."))?;
                // Whatever the file held before stays in the history.
                if let Ok(current) = fs::read_to_string(&target) {
                    let latest = history::latest(conn, path).map_err(|e| format!("Read versions: {e}"))?;
                    if current != content && latest.as_ref().map(|v| v.sha256.as_str()) != Some(sha256_hex(current.as_bytes()).as_str()) {
                        history::record(conn, path, &current, "Before the Collection replaced it", "external", false)
                            .map_err(|e| format!("Record version: {e}"))?;
                    }
                }
                qs_core::paths::write_atomic(&target, bytes).map_err(|e| format!("Could not write {path}: {e}"))?;
                let message = format!("Installed {key} {} from {}@{sha7}", package.manifest.version, source.name);
                let version = history::record(conn, path, &content, &message, "store", true).map_err(|e| format!("Record version: {e}"))?;
                written.push(Written { key: key.clone(), file: path.clone(), created: !existed, version: Some(version.version) });
            } else {
                qs_core::paths::write_atomic(&target, bytes).map_err(|e| format!("Could not write {path}: {e}"))?;
            }
        }
        for file in item.files.iter().filter(|f| f.action == "remove") {
            let _ = fs::remove_file(library.join(&file.path));
        }
        installed.items.insert(
            key.clone(),
            InstalledItem {
                source: source.id.clone(),
                version: package.manifest.version.clone(),
                commit: commit.to_string(),
                installed_at: now_iso(),
                requires: package.manifest.requires.clone(),
                keys: package.manifest.registers.clone(),
                files: record_files,
            },
        );
        save_installed(library, &installed)?;
    }
    Ok(InstallOutcome { plan, written, check: Some(check) })
}

fn blocking_reason(check: &Value) -> String {
    if let Some(message) = check.pointer("/import/message").and_then(Value::as_str) {
        return message.to_string();
    }
    let mut reasons = Vec::new();
    if let Some(files) = check.get("files").and_then(Value::as_object) {
        for (file, row) in files {
            if let Some(message) = row.pointer("/import/message").and_then(Value::as_str).filter(|_| row.pointer("/import/ok") != Some(&Value::Bool(true))) {
                reasons.push(format!("{file}: {message}"));
            }
            if let Some(errors) = row.get("discovery_errors").and_then(Value::as_object) {
                let stem = file.trim_end_matches(".py");
                if let Some(error) = errors.get(stem).and_then(Value::as_str) {
                    reasons.push(format!("{file}: {error}"));
                }
            }
        }
    }
    for u in check.get("unavailable").and_then(Value::as_array).into_iter().flatten() {
        reasons.push(format!(
            "{}: {}",
            u.get("key").and_then(Value::as_str).unwrap_or("?"),
            u.get("error").and_then(Value::as_str).unwrap_or("unavailable")
        ));
    }
    if reasons.is_empty() { "the engine refused the staged library".into() } else { reasons.join("; ") }
}

// ─── Removing ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Removed {
    pub key: String,
    pub file: String,
    pub keys: Vec<String>,
    pub version: Option<i64>,
    pub files: Vec<String>,
}

/// Remove an item the Collection installed — refused while another installed item
/// requires it. The script's last content becomes a `delete` version, so the
/// archive keeps it and a restore brings it back.
pub fn remove(library: &Path, conn: &Connection, key: &str) -> Result<Removed, String> {
    let mut installed = load_installed(library)?;
    let item = installed
        .items
        .get(key)
        .cloned()
        .ok_or_else(|| format!("{key} was not installed by the Collection — delete its script in the editor."))?;
    let dependents: Vec<String> = installed
        .items
        .iter()
        .filter(|(other, i)| other.as_str() != key && i.requires.iter().any(|r| r == key))
        .map(|(other, _)| other.clone())
        .collect();
    if !dependents.is_empty() {
        return Err(format!("{key} is required by {} — remove those first.", dependents.join(", ")));
    }
    let script = format!("{key}.py");
    let mut version = None;
    if let Ok(content) = fs::read_to_string(library.join(&script)) {
        let v = history::record(conn, &script, &content, "Removed by the Collection — the script as it was", "delete", false)
            .map_err(|e| format!("Record version: {e}"))?;
        version = Some(v.version);
    }
    let mut files = Vec::new();
    for path in item.files.keys() {
        let target = library.join(path);
        if target.is_file() {
            fs::remove_file(&target).map_err(|e| format!("Could not remove {path}: {e}"))?;
            files.push(path.clone());
        }
    }
    let _ = fs::remove_dir(library.join("versions").join(key));
    installed.items.remove(key);
    save_installed(library, &installed)?;
    Ok(Removed { key: key.to_string(), file: script, keys: item.keys, version, files })
}

#[cfg(test)]
mod tests;
