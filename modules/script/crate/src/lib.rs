//! QuantScript — the Indicator Smithery's scripts inside the suite
//! (docs/PLAN-QUANTSCRIPT.md).
//!
//! The scripts are private files in `QuantScript/indicators/`: the
//! files QuantSystems' engine and QuantAlgo's bots import through the forge's
//! registry. This crate lists them with what the registry knows about every
//! class in them (`python -m smithery.quantscript list`), reads and writes
//! them, keeps every saved version in `script.db`, and gates each write on a
//! check that runs in a sandbox copy of the package — a script that does not
//! import never lands in the registry every consumer loads. The forge itself
//! (gauntlet, walk-forward, reports) stays in QuantAlgo → Smithery.

mod python;
mod store;

use python::{PythonCommand, PythonInfo};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use store::{sha256_hex, VersionMeta};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Wry,
};

/// The package the scripts belong to, under the sidecar tree.
const PACKAGE: &str = "smithery";
const REGISTRY_FILE: &str = "__init__.py";
/// Read-only files shown next to the scripts: the contract every script obeys.
const REFERENCE_FILES: [&str; 1] = ["contract.py"];

pub struct ScriptState {
    db: Mutex<Connection>,
    /// `sidecars/python` — the tree holding the `smithery` package; `None`
    /// when no candidate location had it (the module then reports that).
    sidecar_dir: Option<PathBuf>,
    script_dir: PathBuf,
    /// Resolved on first use, re-probed on request.
    python: Mutex<Option<(PythonCommand, PythonInfo)>>,
    /// `python -m smithery.quantscript list`, cached until a write or a refresh.
    listing: Mutex<Option<Value>>,
}

// ─── Names and locations ──────────────────────────────────────────────────

/// One `.py` file name, no path: what the CLI accepts too.
fn valid_script_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".py") else { return false };
    !stem.is_empty()
        && stem.len() <= 80
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A registry key: a letter, then lowercase letters, digits, underscores.
fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && key.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_class_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn package_dir(state: &ScriptState) -> Result<PathBuf, String> {
    state.sidecar_dir.as_ref().map(|d| d.join(PACKAGE)).ok_or_else(|| {
        "The smithery package was not found: sidecars/python is missing next to the suite \
         (set QUANTSUITE_PYTHON_DIR for an unusual layout)."
            .to_string()
    })
}

fn indicators_dir(state: &ScriptState) -> Result<PathBuf, String> {
    Ok(state.script_dir.clone())
}

fn scripts_folder_path(dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    // Dev paths contain ../.. and forward slashes. Explorer does not resolve
    // those like the filesystem APIs; std::canonicalize's \\?\ prefix also
    // needs conversion to the ordinary Windows form when that is possible.
    dunce::canonicalize(dir).map_err(|e| format!("Could not resolve {}: {e}", dir.display()))
}

struct Located {
    path: PathBuf,
    editable: bool,
    kind: &'static str,
}

fn locate(state: &ScriptState, file: &str) -> Result<Located, String> {
    if file == REGISTRY_FILE {
        return Ok(Located { path: package_dir(state)?.join("indicators").join(file), editable: false, kind: "registry" });
    }
    if REFERENCE_FILES.contains(&file) {
        return Ok(Located { path: package_dir(state)?.join(file), editable: false, kind: "reference" });
    }
    if !valid_script_name(file) {
        return Err(format!("'{file}' is not a script name."));
    }
    Ok(Located {
        path: indicators_dir(state)?.join(file),
        editable: true,
        kind: if file == REGISTRY_FILE { "registry" } else { "script" },
    })
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn modified_iso(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

// ─── Python ───────────────────────────────────────────────────────────────

fn python(state: &ScriptState, refresh: bool) -> Result<(PythonCommand, PythonInfo), String> {
    let mut cached = state.python.lock().map_err(|e| format!("Lock: {e}"))?;
    if refresh || cached.is_none() {
        *cached = Some(python::resolve());
    }
    Ok(cached.clone().expect("resolved above"))
}

/// One `python -m smithery.quantscript …` query; the last JSON line on
/// stdout is the answer, an `event: error` line is the error.
fn run_query(state: &ScriptState, args: &[&str]) -> Result<Value, String> {
    let sidecar = state.sidecar_dir.as_ref().ok_or_else(|| package_dir(state).unwrap_err())?;
    let (cmd, info) = python(state, false)?;
    if !info.ok {
        return Err(info.error.unwrap_or_else(|| "No Python interpreter for the scripts.".into()));
    }
    let output = cmd
        .command(sidecar)
        .env("QUANTSCRIPT_INDICATORS_DIR", &state.script_dir)
        .arg("-m")
        .arg(format!("{PACKAGE}.quantscript"))
        .args(args)
        .output()
        .map_err(|e| format!("Could not run {} -m {PACKAGE}.quantscript: {e}", info.command))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok());
    match parsed {
        Some(doc) if doc.get("event").and_then(Value::as_str) == Some("error") => Err(doc
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("QuantScript's CLI reported an error")
            .to_string()),
        Some(doc) if output.status.success() => Ok(doc),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            Err(format!(
                "python -m {PACKAGE}.quantscript {} gave no answer: {}",
                args.first().copied().unwrap_or(""),
                if detail.is_empty() { "no JSON on stdout" } else { detail }
            ))
        }
    }
}

// ─── The listing ──────────────────────────────────────────────────────────

fn file_entry(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok(json!({
        "file": file,
        "stem": file.strip_suffix(".py").unwrap_or(&file),
        "path": path.to_string_lossy(),
        "size": bytes.len(),
        "modified": modified_iso(path),
        "sha256": sha256_hex(&bytes),
    }))
}

/// The files alone — what the module shows when the registry cannot be
/// read (no interpreter, or a script that breaks the import): viewing,
/// editing and versioning keep working, the verdicts and checks stand down.
fn fallback_listing(state: &ScriptState, error: &str) -> Result<Value, String> {
    let dir = indicators_dir(state)?;
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("Could not list {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    paths.retain(|p| !p.file_name().is_some_and(|f| f == REGISTRY_FILE));
    let registry = package_dir(state)?.join("indicators").join(REGISTRY_FILE);
    if registry.is_file() {
        paths.push(registry);
    }
    paths.sort();
    let mut scripts = Vec::new();
    for path in paths {
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if file.starts_with('_') && file != REGISTRY_FILE {
            continue;
        }
        let mut entry = file_entry(&path)?;
        entry["kind"] = json!(if file == REGISTRY_FILE { "registry" } else { "script" });
        entry["editable"] = json!(file != REGISTRY_FILE);
        entry["summary"] = json!("");
        entry["syntax_error"] = Value::Null;
        entry["classes"] = json!([]);
        entry["registered"] = json!(0);
        entry["discovery_error"] = Value::Null;
        scripts.push(entry);
    }
    let mut reference = Vec::new();
    for name in REFERENCE_FILES {
        let path = package_dir(state)?.join(name);
        if path.is_file() {
            let mut entry = file_entry(&path)?;
            entry["kind"] = json!("reference");
            entry["editable"] = json!(false);
            entry["summary"] = json!("");
            entry["syntax_error"] = Value::Null;
            entry["classes"] = json!([]);
            entry["registered"] = json!(0);
            entry["discovery_error"] = Value::Null;
            reference.push(entry);
        }
    }
    Ok(json!({
        "generated_at": now_iso(),
        "package_dir": package_dir(state)?.to_string_lossy(),
        "indicators_dir": dir.to_string_lossy(),
        "registry_error": error,
        "registry_keys": 0,
        "certified": [],
        "discovery_errors": {},
        "scripts": scripts,
        "reference": reference,
        "fallback": true,
    }))
}

fn python_listing(state: &ScriptState, refresh: bool) -> Result<Value, String> {
    if !refresh {
        if let Some(cached) = state.listing.lock().map_err(|e| format!("Lock: {e}"))?.as_ref() {
            return Ok(cached.clone());
        }
    }
    let doc = run_query(state, &["list"])?;
    *state.listing.lock().map_err(|e| format!("Lock: {e}"))? = Some(doc.clone());
    Ok(doc)
}

fn forget_listing(state: &ScriptState) {
    if let Ok(mut cached) = state.listing.lock() {
        *cached = None;
    }
}

/// Strip the listing to what an agent (or a tree) needs: every class keeps
/// its key, name, line and the verdict's score / grade / certified; the
/// hypotheses, defaults, parameter spaces and per-track verdicts — the bulk
/// of the document — go, and summaries are cut short.
fn compact_listing(doc: &mut Value) {
    const KEEP_CERT: [&str; 5] = ["score", "grade", "certified", "perm_p", "source"];
    for list in ["scripts", "reference"] {
        let Some(entries) = doc.get_mut(list).and_then(Value::as_array_mut) else { continue };
        for entry in entries {
            if let Some(classes) = entry.get_mut("classes").and_then(Value::as_array_mut) {
                for class in classes {
                    let Some(obj) = class.as_object_mut() else { continue };
                    for key in ["hypothesis", "params", "param_space", "timeframes", "bases"] {
                        obj.remove(key);
                    }
                    if let Some(cert) = obj.get_mut("certification").and_then(Value::as_object_mut) {
                        cert.retain(|k, _| KEEP_CERT.contains(&k.as_str()));
                    }
                }
            }
            let short = entry
                .get("summary")
                .and_then(Value::as_str)
                .filter(|s| s.chars().count() > 160)
                .map(|s| format!("{}…", s.chars().take(157).collect::<String>()));
            if let Some(short) = short {
                entry["summary"] = json!(short);
            }
        }
    }
}

/// The scripts, the reference files and the interpreter — the registry's
/// view when it can be read, the files alone when it cannot — each joined
/// with its version history, plus the archive: scripts that only the
/// version history still knows (deleted, or gone from disk).
fn build_listing(state: &ScriptState, refresh: bool, compact: bool) -> Result<Value, String> {
    let (_, info) = python(state, refresh)?;
    let mut doc = if info.ok {
        match python_listing(state, refresh) {
            Ok(doc) => doc,
            Err(e) => fallback_listing(state, &e)?,
        }
    } else {
        fallback_listing(state, info.error.as_deref().unwrap_or("no interpreter"))?
    };
    let counts = {
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        store::counts(&conn).map_err(|e| format!("Read versions: {e}"))?
    };
    let mut listed: Vec<String> = Vec::new();
    for list in ["scripts", "reference"] {
        if let Some(entries) = doc.get_mut(list).and_then(Value::as_array_mut) {
            for entry in entries {
                let file = entry.get("file").and_then(Value::as_str).unwrap_or_default().to_string();
                let sha = entry.get("sha256").and_then(Value::as_str).unwrap_or_default().to_string();
                let c = counts.get(&file).cloned().unwrap_or_default();
                entry["versions"] = json!({
                    "latest": c.latest,
                    "count": c.count,
                    // The file no longer matches its latest recorded version:
                    // an agent, git or an editor changed it outside QuantScript.
                    "external": c.latest_sha256.as_deref().is_some_and(|s| s != sha),
                });
                listed.push(file);
            }
        }
    }
    let dir = indicators_dir(state)?;
    let mut archived: Vec<Value> = counts
        .iter()
        .filter(|(file, _)| !listed.contains(file) && valid_script_name(file) && !dir.join(file).exists())
        .map(|(file, c)| {
            json!({
                "file": file,
                "latest": c.latest,
                "count": c.count,
                "deleted": c.latest_author.as_deref() == Some("delete"),
                "at": c.latest_at,
            })
        })
        .collect();
    archived.sort_by(|a, b| b["at"].as_str().cmp(&a["at"].as_str()));
    doc["archived"] = json!(archived);
    if compact {
        compact_listing(&mut doc);
    }
    doc["python"] = serde_json::to_value(&info).map_err(|e| e.to_string())?;
    doc["db_path"] = json!(db_path().to_string_lossy());
    Ok(doc)
}

// ─── Documents ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ScriptDocument {
    pub file: String,
    pub path: String,
    pub kind: String,
    pub editable: bool,
    pub content: String,
    pub sha256: String,
    pub size: usize,
    pub modified: Option<String>,
    /// The latest recorded version — the content on disk, after `recorded`.
    pub version: Option<VersionMeta>,
    /// Opening the file recorded a version: `baseline` (first sight) or
    /// `external` (changed outside QuantScript since the last one).
    pub recorded: Option<String>,
}

fn read_document(state: &ScriptState, file: &str) -> Result<ScriptDocument, String> {
    let loc = locate(state, file)?;
    let content = fs::read_to_string(&loc.path).map_err(|e| format!("Could not read {file}: {e}"))?;
    let sha = sha256_hex(content.as_bytes());
    let (version, recorded) = if loc.editable {
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        match store::latest(&conn, file).map_err(|e| format!("Read versions: {e}"))? {
            None => {
                let v = store::record(&conn, file, &content, "Baseline — the script as found on disk", "baseline", false)
                    .map_err(|e| format!("Record version: {e}"))?;
                (Some(v), Some("baseline".to_string()))
            }
            Some(latest) if latest.sha256 != sha => {
                let v = store::record(&conn, file, &content, "Changed outside QuantScript", "external", false)
                    .map_err(|e| format!("Record version: {e}"))?;
                (Some(v), Some("external".to_string()))
            }
            Some(latest) => (Some(latest), None),
        }
    } else {
        (None, None)
    };
    Ok(ScriptDocument {
        file: file.to_string(),
        path: loc.path.to_string_lossy().to_string(),
        kind: loc.kind.to_string(),
        editable: loc.editable,
        size: content.len(),
        modified: modified_iso(&loc.path),
        content,
        sha256: sha,
        version,
        recorded,
    })
}

// ─── Checks and writes ────────────────────────────────────────────────────

fn candidate_file(file: &str, content: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!("quantscript-{}-{nanos}-{file}", std::process::id()));
    fs::write(&path, content.as_bytes()).map_err(|e| format!("Could not write the candidate: {e}"))?;
    Ok(path)
}

/// Verify a candidate version of a script in a sandbox copy of the package
/// (`python -m smithery.quantscript check`). The package is never touched.
fn run_check(state: &ScriptState, file: &str, content: &str, depth: &str) -> Result<Value, String> {
    let loc = locate(state, file)?;
    if !loc.editable {
        return Err(format!("{file} is reference, not a script — there is nothing to check."));
    }
    let depth = if depth == "full" { "full" } else { "quick" };
    let candidate = candidate_file(file, content)?;
    let result = run_query(
        state,
        &["check", "--file", file, "--candidate", &candidate.to_string_lossy(), "--depth", depth],
    );
    let _ = fs::remove_file(&candidate);
    result
}

/// Diagnostics for the editor while the user types (`quantscript lint`):
/// syntax with its span, the parser's warnings, the contract's shape. No
/// import, no sandbox — an interpreter start, nothing more.
fn run_lint(state: &ScriptState, file: &str, content: &str) -> Result<Value, String> {
    let loc = locate(state, file)?;
    if !loc.editable {
        return Err(format!("{file} is reference, not a script — there is nothing to lint."));
    }
    let candidate = candidate_file(file, content)?;
    let result = run_query(state, &["lint", "--candidate", &candidate.to_string_lossy()]);
    let _ = fs::remove_file(&candidate);
    result
}

#[derive(Debug, Serialize)]
pub struct SaveResult {
    pub saved: bool,
    pub unchanged: bool,
    /// The content went through the sandbox check (an interpreter was there).
    pub checked: bool,
    pub check: Option<Value>,
    pub version: Option<VersionMeta>,
    pub sha256: String,
    pub note: Option<String>,
}

fn publish(app: &AppHandle, topic: &str, payload: Value) {
    if let Err(e) = qs_core::bus::publish(app, qs_core::bus::Event::new(topic, "script", payload)) {
        log::warn!(target: "script", "publish {topic}: {e}");
    }
}

/// Write a script — checked first, then atomically, then recorded. A
/// candidate that would not import, or that cannot register, is not
/// written; its check comes back for the editor to show.
fn write_version(app: &AppHandle, file: &str, content: &str, message: &str, author: &str) -> Result<SaveResult, String> {
    let state = app.state::<ScriptState>();
    let loc = locate(&state, file)?;
    if !loc.editable {
        return Err(format!("{file} is read-only — the contract is reference, not a script."));
    }
    let sha = sha256_hex(content.as_bytes());
    let current = fs::read_to_string(&loc.path).unwrap_or_default();
    if current == content {
        let version = {
            let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            store::latest(&conn, file).map_err(|e| format!("Read versions: {e}"))?
        };
        return Ok(SaveResult {
            saved: false,
            unchanged: true,
            checked: false,
            check: None,
            version,
            sha256: sha,
            note: Some("Nothing to save — the file already has this content.".into()),
        });
    }
    let (_, info) = python(&state, false)?;
    let (checked, check) = if info.ok {
        let doc = run_check(&state, file, content, "quick")?;
        if doc.get("blocking").and_then(Value::as_bool).unwrap_or(true) {
            return Ok(SaveResult {
                saved: false,
                unchanged: false,
                checked: true,
                check: Some(doc),
                version: None,
                sha256: sha,
                note: Some("Not saved — the script would not load into the registry.".into()),
            });
        }
        (true, Some(doc))
    } else {
        (false, None)
    };
    qs_core::paths::write_atomic(&loc.path, content.as_bytes()).map_err(|e| format!("Could not write {file}: {e}"))?;
    let version = {
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        store::record(&conn, file, content, message, author, checked).map_err(|e| format!("Record version: {e}"))?
    };
    forget_listing(&state);
    publish(app, "script.file.saved", json!({ "file": file, "version": version.version, "checked": checked, "author": author }));
    Ok(SaveResult {
        saved: true,
        unchanged: false,
        checked,
        check,
        version: Some(version),
        sha256: sha,
        note: if checked {
            None
        } else {
            Some("Saved without a check — no Python interpreter with numpy and pandas was found.".into())
        },
    })
}

#[derive(Debug)]
pub struct NewScript {
    pub key: String,
    pub class_name: String,
    pub name: Option<String>,
}

/// A new script from the template: `<key>.py`, registering `key`.
fn create(app: &AppHandle, request: NewScript) -> Result<Value, String> {
    let state = app.state::<ScriptState>();
    let key = request.key.trim().to_string();
    let class_name = request.class_name.trim().to_string();
    if !valid_key(&key) {
        return Err("The key is the registry name: a letter, then lowercase letters, digits and underscores (at most 40).".into());
    }
    if !valid_class_name(&class_name) {
        return Err("The class name is CapitalizedWords: a capital letter, then letters, digits and underscores.".into());
    }
    let file = format!("{key}.py");
    let path = indicators_dir(&state)?.join(&file);
    if path.exists() {
        return Err(format!("{file} exists already — pick another key."));
    }
    let name = request.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&class_name).to_string();
    let doc = run_query(&state, &["template", "--key", &key, "--class", &class_name, "--name", &name])?;
    let source = doc
        .get("source")
        .and_then(Value::as_str)
        .ok_or("The template came back empty.")?
        .to_string();
    let check = run_check(&state, &file, &source, "quick")?;
    if check.get("blocking").and_then(Value::as_bool).unwrap_or(true) {
        let reason = check
            .get("discovery_errors")
            .and_then(|d| d.get(&key))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| check.get("import").and_then(|i| i.get("message")).and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| "the script would not load".into());
        return Err(format!("Not created — {reason}."));
    }
    qs_core::paths::write_atomic(&path, source.as_bytes()).map_err(|e| format!("Could not write {file}: {e}"))?;
    let version = {
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        store::record(&conn, &file, &source, "Created from the QuantScript template", "template", true)
            .map_err(|e| format!("Record version: {e}"))?
    };
    forget_listing(&state);
    publish(app, "script.file.created", json!({ "file": file, "key": key, "class_name": class_name }));
    Ok(json!({ "file": file, "path": path.to_string_lossy(), "key": key, "version": version }))
}

/// Delete a script the user made. The forge's explicit modules — the ones
/// `indicators/__init__.py` imports — are refused: deleting one breaks the
/// registry for every bot and backtest. The last content is recorded as a
/// final `delete` version first, so the archive keeps it and a restore
/// brings it back.
fn delete(app: &AppHandle, file: &str) -> Result<Value, String> {
    let state = app.state::<ScriptState>();
    let loc = locate(&state, file)?;
    if loc.kind != "script" {
        return Err(format!(
            "{file} is {} — not a script to delete.",
            if loc.kind == "registry" { "the registry" } else { "reference" }
        ));
    }
    if !loc.path.is_file() {
        return Err(format!("{file} does not exist."));
    }
    let (_, info) = python(&state, false)?;
    if !info.ok {
        return Err("Deleting needs the interpreter: QuantScript has to confirm that the forge's explicit registry does not import this file.".into());
    }
    let listing = python_listing(&state, false)?;
    let entry = listing
        .get("scripts")
        .and_then(Value::as_array)
        .and_then(|list| list.iter().find(|e| e.get("file").and_then(Value::as_str) == Some(file)))
        .cloned()
        .ok_or_else(|| format!("{file} is not in the registry's listing — reload and try again."))?;
    match entry.get("registration").and_then(Value::as_str).unwrap_or("unknown") {
        "explicit" => {
            return Err(format!(
                "{file} is one of the forge's explicit modules — indicators/__init__.py imports it. \
                 Remove that import first; deleting the file would break the registry."
            ))
        }
        "unknown" => {
            return Err("The registry could not be read, so nothing confirms this file is safe to delete — fix the registry first.".into())
        }
        _ => {}
    }
    let keys: Vec<String> = entry
        .get("classes")
        .and_then(Value::as_array)
        .map(|classes| {
            classes
                .iter()
                .filter_map(|c| c.get("key").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let content = fs::read_to_string(&loc.path).map_err(|e| format!("Could not read {file}: {e}"))?;
    let version = {
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        store::record(&conn, file, &content, "Deleted — the script as it was", "delete", false)
            .map_err(|e| format!("Record version: {e}"))?
    };
    fs::remove_file(&loc.path).map_err(|e| format!("Could not delete {file}: {e}"))?;
    forget_listing(&state);
    publish(app, "script.file.deleted", json!({ "file": file, "keys": keys, "version": version.version }));
    Ok(json!({ "file": file, "keys": keys, "version": version }))
}

// ─── Commands ─────────────────────────────────────────────────────────────

/// Open the same directory used for script reads and writes, even without Python.
#[tauri::command]
async fn open_scripts_folder(app_handle: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        let dir = scripts_folder_path(&indicators_dir(&state)?)?;
        #[cfg(target_os = "windows")]
        let program = "explorer.exe";
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let program = "xdg-open";
        let mut child = std::process::Command::new(program)
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Could not open {}: {e}", dir.display()))?;
        // Reap the launcher without making the UI wait for a file-manager window.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|e| format!("Open folder task failed: {e}"))?
}

/// Every script with what the registry knows about it, plus versions, the
/// archive and the interpreter. `refresh` re-reads the registry and
/// re-probes Python. Compact unless told otherwise — the agents' default;
/// the workbench asks for the full view.
#[tauri::command]
async fn list_scripts(refresh: Option<bool>, compact: Option<bool>, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        build_listing(&state, refresh.unwrap_or(false), compact.unwrap_or(true))
    })
    .await
    .map_err(|e| format!("Listing task failed: {e}"))?
}

/// Syntax and contract-shape diagnostics for the buffer — no sandbox.
#[tauri::command]
async fn lint_script(file: String, content: String, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        run_lint(&state, &file, &content)
    })
    .await
    .map_err(|e| format!("Lint task failed: {e}"))?
}

/// Delete a script the user made; its content stays in the archive.
#[tauri::command]
async fn delete_script(file: String, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || delete(&app_handle, &file))
        .await
        .map_err(|e| format!("Delete task failed: {e}"))?
}

/// The file's content; opening records a baseline or an external change.
#[tauri::command]
async fn read_script(file: String, app_handle: AppHandle) -> Result<ScriptDocument, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        read_document(&state, &file)
    })
    .await
    .map_err(|e| format!("Read task failed: {e}"))?
}

/// Verify a candidate in the sandbox: `quick` (syntax, import, one signal
/// pass) or `full` (plus causality and scale invariance). Nothing is written.
#[tauri::command]
async fn check_script(
    file: String,
    content: String,
    depth: Option<String>,
    app_handle: AppHandle,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        run_check(&state, &file, &content, depth.as_deref().unwrap_or("quick"))
    })
    .await
    .map_err(|e| format!("Check task failed: {e}"))?
}

/// Save a script as a new version — after the quick check passed.
#[tauri::command]
async fn save_script(
    file: String,
    content: String,
    message: Option<String>,
    app_handle: AppHandle,
) -> Result<SaveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let message = message.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or_else(|| "Saved".into());
        write_version(&app_handle, &file, &content, &message, "user")
    })
    .await
    .map_err(|e| format!("Save task failed: {e}"))?
}

/// A script's version history, newest first (no content).
#[tauri::command]
async fn list_versions(file: String, app_handle: AppHandle) -> Result<Vec<VersionMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        locate(&state, &file)?;
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        store::list(&conn, &file).map_err(|e| format!("Read versions: {e}"))
    })
    .await
    .map_err(|e| format!("Versions task failed: {e}"))?
}

/// One version with its content — for the diff against the buffer.
#[tauri::command]
async fn read_version(file: String, version: i64, app_handle: AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        locate(&state, &file)?;
        let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
        let (meta, content) = store::read(&conn, &file, version)
            .map_err(|e| format!("Read version: {e}"))?
            .ok_or_else(|| format!("{file} has no version {version}."))?;
        Ok(json!({ "meta": meta, "content": content }))
    })
    .await
    .map_err(|e| format!("Version task failed: {e}"))?
}

/// Write an earlier version back — as a new version, checked like a save.
#[tauri::command]
async fn restore_version(file: String, version: i64, app_handle: AppHandle) -> Result<SaveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let content = {
            let state = app_handle.state::<ScriptState>();
            locate(&state, &file)?;
            let conn = state.db.lock().map_err(|e| format!("Lock: {e}"))?;
            store::read(&conn, &file, version)
                .map_err(|e| format!("Read version: {e}"))?
                .ok_or_else(|| format!("{file} has no version {version}."))?
                .1
        };
        write_version(&app_handle, &file, &content, &format!("Restored v{version}"), "restore")
    })
    .await
    .map_err(|e| format!("Restore task failed: {e}"))?
}

/// A new script from the template — `<key>.py`, registering `key`. Flat,
/// snake_case arguments: the same call serves the workbench and the
/// `quantsuite.script.create` agent tool.
#[tauri::command(rename_all = "snake_case")]
async fn create_script(
    key: String,
    class_name: String,
    name: Option<String>,
    app_handle: AppHandle,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || create(&app_handle, NewScript { key, class_name, name }))
        .await
        .map_err(|e| format!("Create task failed: {e}"))?
}

/// The interpreter the scripts run with; `refresh` probes again.
#[tauri::command]
async fn script_python(refresh: Option<bool>, app_handle: AppHandle) -> Result<PythonInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<ScriptState>();
        python(&state, refresh.unwrap_or(false)).map(|(_, info)| info)
    })
    .await
    .map_err(|e| format!("Python task failed: {e}"))?
}

// ─── Plugin ───────────────────────────────────────────────────────────────

fn data_dir() -> PathBuf {
    qs_core::paths::module_dir("script")
}

fn db_path() -> PathBuf {
    data_dir().join("script.db")
}

pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("script")
        .setup(|app, _api| {
            let dir = data_dir();
            fs::create_dir_all(&dir)?;
            let conn = Connection::open(db_path())?;
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            store::init_schema(&conn)?;

            // Repo checkout and installed bundle both resolve through qs-core
            // (it also checks the Tauri resource dir).
            let sidecar_dir = qs_core::paths::python_sidecar_dir(app, PACKAGE);
            if let Err(error) = qs_core::paths::ensure_indicator_scripts() {
                // A read-only installation must not prevent the suite opening.
                // Script operations report the folder error when used.
                log::warn!(target: "script", "Could not create the script folder {}: {error}", qs_core::paths::indicators_dir().display());
            }
            match &sidecar_dir {
                Some(_) => log::info!(target: "script", "scripts in {}", qs_core::paths::indicators_dir().display()),
                None => log::warn!(target: "script", "sidecars/python with the smithery package was not found"),
            }

            app.manage(ScriptState {
                db: Mutex::new(conn),
                sidecar_dir,
                script_dir: qs_core::paths::indicators_dir(),
                python: Mutex::new(None),
                listing: Mutex::new(None),
            });

            qs_core::tray::register_entry(
                app,
                qs_core::tray::TrayEntry {
                    module: "script".into(),
                    label: "QuantScript".into(),
                    route: "/script".into(),
                    toggle_window: None,
                },
            );
            Ok(())
        })
        .invoke_handler(qs_core::diagnostics::traced(tauri::generate_handler![
            list_scripts,
            open_scripts_folder,
            read_script,
            check_script,
            save_script,
            list_versions,
            read_version,
            restore_version,
            create_script,
            delete_script,
            lint_script,
            script_python,
        ]))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qs-script-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(PACKAGE).join("indicators")).unwrap();
        dir
    }

    fn state_for(sidecar: &Path) -> ScriptState {
        let conn = Connection::open_in_memory().unwrap();
        store::init_schema(&conn).unwrap();
        ScriptState {
            db: Mutex::new(conn),
            sidecar_dir: Some(sidecar.to_path_buf()),
            script_dir: sidecar.join(PACKAGE).join("indicators"),
            python: Mutex::new(None),
            listing: Mutex::new(None),
        }
    }

    #[test]
    fn names_are_one_python_file_and_nothing_else() {
        assert!(valid_script_name("extremes.py"));
        assert!(valid_script_name("__init__.py"));
        assert!(valid_script_name("my-script_2.py"));
        assert!(!valid_script_name("extremes"));
        assert!(!valid_script_name("../evil.py"));
        assert!(!valid_script_name("dir/x.py"));
        assert!(!valid_script_name(".hidden.py"));
        assert!(!valid_script_name(""));
    }

    #[test]
    fn keys_and_class_names_follow_the_registry_rules() {
        assert!(valid_key("extremes"));
        assert!(valid_key("bq_adaptive_envelope"));
        assert!(!valid_key("Extremes"));
        assert!(!valid_key("1st"));
        assert!(!valid_key("a-b"));
        assert!(!valid_key(""));
        assert!(valid_class_name("ExtremeFlow"));
        assert!(!valid_class_name("extremeFlow"));
        assert!(!valid_class_name("Extreme Flow"));
    }

    #[test]
    fn the_explorer_path_resolves_parents_separators_and_custom_folders() {
        let tree = temp_tree("explorer-path");
        let mut state = state_for(&tree);
        state.script_dir = tree.join("smithery/../Custom Skripte ä/indicators");
        let expected = tree.join("Custom Skripte ä").join("indicators");
        let path = scripts_folder_path(&indicators_dir(&state).unwrap()).unwrap();

        assert!(path.is_absolute());
        assert!(path.is_dir());
        assert_eq!(fs::canonicalize(&path).unwrap(), fs::canonicalize(&expected).unwrap());
        assert!(!path.components().any(|c| c == std::path::Component::ParentDir));
        #[cfg(windows)]
        {
            assert!(!path.to_string_lossy().contains('/'));
            assert!(!path.to_string_lossy().starts_with(r"\\?\"));
            // Also accept paths from Rust's normal canonicalize API.
            assert_eq!(scripts_folder_path(&fs::canonicalize(&expected).unwrap()).unwrap(), path);
        }
        fs::remove_dir_all(tree).unwrap();
    }

    #[test]
    fn reference_files_are_read_only_and_scripts_live_in_the_indicators_dir() {
        let tree = temp_tree("locate");
        let state = state_for(&tree);
        let contract = locate(&state, "contract.py").unwrap();
        assert!(!contract.editable);
        assert_eq!(contract.kind, "reference");
        assert_eq!(contract.path, tree.join(PACKAGE).join("contract.py"));
        let script = locate(&state, "extremes.py").unwrap();
        assert!(script.editable);
        assert_eq!(script.kind, "script");
        assert_eq!(script.path, tree.join(PACKAGE).join("indicators").join("extremes.py"));
        assert_eq!(locate(&state, REGISTRY_FILE).unwrap().kind, "registry");
        assert!(!locate(&state, REGISTRY_FILE).unwrap().editable);
        assert!(locate(&state, "../contract.py").is_err());
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn opening_a_script_records_its_baseline_and_later_external_changes() {
        let tree = temp_tree("read");
        let path = tree.join(PACKAGE).join("indicators").join("a.py");
        fs::write(&path, "x = 1\n").unwrap();
        let state = state_for(&tree);

        let first = read_document(&state, "a.py").unwrap();
        assert_eq!(first.recorded.as_deref(), Some("baseline"));
        assert_eq!(first.version.as_ref().unwrap().version, 1);
        assert_eq!(first.content, "x = 1\n");

        let again = read_document(&state, "a.py").unwrap();
        assert_eq!(again.recorded, None);
        assert_eq!(again.version.as_ref().unwrap().version, 1);

        fs::write(&path, "x = 2\n").unwrap();
        let changed = read_document(&state, "a.py").unwrap();
        assert_eq!(changed.recorded.as_deref(), Some("external"));
        assert_eq!(changed.version.as_ref().unwrap().version, 2);
        assert_eq!(changed.version.as_ref().unwrap().author, "external");
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn the_fallback_listing_names_the_files_and_the_reason() {
        let tree = temp_tree("fallback");
        let ind = tree.join(PACKAGE).join("indicators");
        fs::write(ind.join("__init__.py"), "REGISTRY = {}\n").unwrap();
        fs::write(ind.join("b.py"), "b = 1\n").unwrap();
        fs::write(ind.join("a.py"), "a = 1\n").unwrap();
        fs::write(ind.join("_discover.py"), "\n").unwrap();
        fs::write(tree.join(PACKAGE).join("contract.py"), "class TrendIndicator: ...\n").unwrap();
        let state = state_for(&tree);
        {
            let conn = state.db.lock().unwrap();
            store::record(&conn, "a.py", "a = 0\n", "Baseline", "baseline", false).unwrap();
            // A script only the history knows: deleted, its last content kept.
            store::record(&conn, "gone.py", "g = 1\n", "Baseline", "baseline", false).unwrap();
            store::record(&conn, "gone.py", "g = 2\n", "Deleted — the script as it was", "delete", false).unwrap();
        }
        let doc = build_listing(&state, false, false);
        // No interpreter is resolved for the test tree either way; the
        // listing must still come back with the files.
        let doc = match doc {
            Ok(doc) => doc,
            Err(e) => panic!("listing failed: {e}"),
        };
        let files: Vec<&str> = doc["scripts"].as_array().unwrap().iter().map(|s| s["file"].as_str().unwrap()).collect();
        assert_eq!(files, vec!["__init__.py", "a.py", "b.py"]);
        assert_eq!(doc["scripts"][0]["kind"], "registry");
        assert_eq!(doc["reference"][0]["file"], "contract.py");
        assert_eq!(doc["reference"][0]["editable"], false);
        // a.py's latest version holds different bytes than the file: external.
        assert_eq!(doc["scripts"][1]["versions"]["external"], true);
        assert_eq!(doc["scripts"][1]["versions"]["latest"], 1);
        assert_eq!(doc["scripts"][2]["versions"]["count"], 0);
        assert!(doc["python"].is_object());
        // The archive lists what has no file any more, newest first.
        let archived = doc["archived"].as_array().unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0]["file"], "gone.py");
        assert_eq!(archived[0]["latest"], 2);
        assert_eq!(archived[0]["deleted"], true);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn a_compact_listing_keeps_the_verdict_and_drops_the_bulk() {
        let mut doc = json!({
            "scripts": [{
                "file": "x.py",
                "summary": "s".repeat(300),
                "classes": [{
                    "class_name": "X", "line": 3, "key": "x", "name": "X", "bases": ["TrendIndicator"],
                    "hypothesis": "long", "params": {"a": 1}, "param_space": {"a": [0, 1]},
                    "warmup_bars": 400,
                    "certification": {"score": 71, "grade": "A", "certified": false, "perm_p": 0.05, "date": "2026-09-07", "report": "r", "tracks_run": 3},
                    "timeframes": {"1d": {"score": 71}}
                }]
            }],
            "reference": []
        });
        compact_listing(&mut doc);
        let class = &doc["scripts"][0]["classes"][0];
        assert_eq!(class["key"], "x");
        assert_eq!(class["warmup_bars"], 400);
        assert!(class.get("hypothesis").is_none());
        assert!(class.get("params").is_none());
        assert!(class.get("timeframes").is_none());
        assert_eq!(class["certification"]["score"], 71);
        assert!(class["certification"].get("report").is_none());
        assert_eq!(doc["scripts"][0]["summary"].as_str().unwrap().chars().count(), 158);
    }
}
