//! Import a `.qsbundle` on a new machine (PLAN-MIGRATION §2.2–§2.4, card 3).
//!
//! 1. **Check**: passphrase, every chunk and checksum, the version (this
//!    QuantSuite must be the same or newer than the one that wrote it), the
//!    free space.
//! 2. **Paths**: every stored absolute path is rewritten through one
//!    [`PathMap`] — the old data root to this one, each workspace to the
//!    folder chosen for it.
//! 3. **Write** into a staging folder next to the data root; **secrets** go
//!    into a new vault there (this machine's keystore).
//! 4. **Disarm**: bots stopped, Manual execution not armed (its settings
//!    kept), a notice "imported from …" for the Trading overview.
//! 5. **Verify**: `integrity_check` on every database, rows per table
//!    against the manifest, every secret decrypts, every mapped folder
//!    exists, each module's own check. The report is saved in the staging
//!    folder (`migration/`).
//! 6. **Switch**: on the next start, before anything opens the data root
//!    ([`apply_pending`]), the current root becomes
//!    `<root>.before-import-<time>` — kept, never deleted — and the staging
//!    folder becomes the root. A failure leaves both as they were.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::bundle::{self, Manifest};
use crate::inventory::{self, PathMap};
use crate::vault::{KeyStore, Vault};

const PENDING: &str = ".quantsuite-pending-import.json";

/// One row of the path table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathRow {
    /// `data-root` | `workspace`
    pub kind: String,
    pub name: String,
    pub old: String,
    /// The folder on this machine; `None` leaves it unlinked.
    pub new: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ok: bool,
    pub from_machine: String,
    pub created_at: String,
    pub suite_version: String,
    pub checks: Vec<Check>,
    /// What only the user can do (§2.4).
    pub manual_steps: Vec<String>,
    /// Entered on the old machine, it confirms the move there.
    pub move_code: String,
    pub staged: String,
    pub report_file: String,
}

fn version_tuple(v: &str) -> (u64, u64, u64) {
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// The bundle's version against this QuantSuite: an older one refuses.
pub fn check_version(bundle_version: &str, this_version: &str) -> Result<(), String> {
    if version_tuple(bundle_version) > version_tuple(this_version) {
        return Err(format!(
            "The bundle was written by QuantSuite {bundle_version}; this is {this_version}. Update QuantSuite here first."
        ));
    }
    Ok(())
}

/// Read a bundle: check it completely, then stage it into `staging` to read
/// its workspace registry. Returns the manifest and the proposed path table
/// (each folder mapped to itself when it exists here). The staging folder
/// is removed again.
pub fn propose(bundle_file: &Path, passphrase: &str, staging: &Path, new_root: &Path) -> Result<(Manifest, Vec<PathRow>), String> {
    let _ = std::fs::remove_dir_all(staging);
    let opened = bundle::open(bundle_file, passphrase, Some(staging));
    let result = opened.and_then(|opened| {
        check_version(&opened.manifest.suite_version, env!("CARGO_PKG_VERSION"))?;
        let core = staging.join("quantsuite").join("core.db");
        let mut rows = vec![PathRow {
            kind: "data-root".into(),
            name: "QuantSuite data".into(),
            old: opened.manifest.source_root.clone(),
            new: Some(new_root.to_string_lossy().into_owned()),
        }];
        if let Ok(conn) = Connection::open_with_flags(&core, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            for ws in crate::workspaces::list(&conn) {
                let here = Path::new(&ws.path).is_dir();
                rows.push(PathRow { kind: "workspace".into(), name: ws.name, old: ws.path.clone(), new: here.then_some(ws.path) });
            }
        }
        Ok((opened.manifest, rows))
    });
    let _ = std::fs::remove_dir_all(staging);
    result
}

fn walk_files(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_files(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

/// Rewrite every stored path of a staged data root.
pub fn remap(staged_root: &Path, map: &PathMap) -> Result<usize, String> {
    let mut files = Vec::new();
    walk_files(staged_root, &mut files);
    let mut changed = 0;
    for f in files {
        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext == "json" {
            // A JSON file that does not parse is left as it is (and reported by its module check).
            changed += inventory::remap_json_file(&f, map).unwrap_or(0);
        } else if matches!(ext.as_str(), "db" | "sqlite") {
            let conn = Connection::open(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            changed += inventory::remap_sqlite(&conn, map)?;
        }
    }
    Ok(changed)
}

/// Executors arrive disarmed (§2.2 step 7): bots stopped, Manual execution
/// not armed, a notice for the Trading overview.
pub fn disarm(staged_root: &Path, from_machine: &str) -> Result<(), String> {
    let db = staged_root.join("modules").join("algo").join("quantalgo.db");
    if !db.exists() {
        return Ok(());
    }
    let conn = Connection::open(&db).map_err(|e| format!("quantalgo.db: {e}"))?;
    let exists = |t: &str| {
        conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)", params![t], |r| r.get::<_, bool>(0))
            .unwrap_or(false)
    };
    if exists("bots") {
        conn.execute(
            "UPDATE bots SET status = 'stopped', last_error = 'imported: start it on this machine when ready' WHERE status = 'running'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    if exists("manual_execution") {
        conn.execute("UPDATE manual_execution SET armed = 0, armed_at = NULL", []).map_err(|e| e.to_string())?;
    }
    if exists("execution_flags") {
        let notice = serde_json::json!({ "from": from_machine, "at": chrono::Utc::now().to_rfc3339() }).to_string();
        conn.execute(
            "INSERT INTO execution_flags (key, value) VALUES ('imported', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![notice],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The bundle's secrets into a new vault in the staged root.
pub fn store_secrets(staged_root: &Path, secrets: &[(String, zeroize::Zeroizing<Vec<u8>>)], keystore: &dyn KeyStore) -> Result<Vault, String> {
    let dir = staged_root.join("secrets");
    let _ = std::fs::remove_dir_all(&dir);
    let passphrase = std::env::var(crate::vault::PASSPHRASE_ENV).ok();
    let vault = Vault::open(&dir, keystore, passphrase.as_deref()).map_err(|e| e.to_string())?;
    for (id, value) in secrets {
        vault.set(id, value)?;
    }
    Ok(vault)
}

/// The checks of step 6.
pub fn verify(staged_root: &Path, manifest: &Manifest, vault: Option<&Vault>, rows: &[PathRow]) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut check = |name: &str, ok: bool, detail: String| checks.push(Check { name: name.into(), ok, detail });
    for f in manifest.files.iter().filter(|f| f.schema_version.is_some()) {
        let path = staged_root.join(&f.path);
        // Read-write: an FTS5 table's integrity check writes. The staging copy is ours.
        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                check("database", false, format!("{}: {e}", f.path));
                continue;
            }
        };
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap_or_else(|e| e.to_string());
        check("integrity", integrity == "ok", format!("{}: {integrity}", f.path));
        if let Some(expected) = &f.tables {
            let actual = bundle::table_counts(&conn);
            let diff: Vec<String> = expected
                .iter()
                .filter(|(t, n)| actual.get(*t) != Some(n))
                .map(|(t, n)| format!("{t}: {n} → {}", actual.get(t).map(|v| v.to_string()).unwrap_or_else(|| "missing".into())))
                .collect();
            check(
                "rows",
                diff.is_empty(),
                if diff.is_empty() { format!("{}: {} tables match", f.path, expected.len()) } else { format!("{}: {}", f.path, diff.join(", ")) },
            );
        }
    }
    match vault {
        Some(v) => {
            let bad: Vec<&String> = manifest.secrets.iter().filter(|id| !v.verify(id)).collect();
            check(
                "secrets",
                bad.is_empty(),
                if bad.is_empty() { format!("{} secret(s) decrypt in this machine's vault", manifest.secrets.len()) } else { format!("not readable: {bad:?}") },
            );
        }
        None => check("secrets", manifest.secrets.is_empty(), "the vault could not be opened".into()),
    }
    for row in rows.iter().filter(|r| r.kind == "workspace") {
        match &row.new {
            Some(new) => check("folder", Path::new(new).is_dir(), format!("{} → {new}", row.name)),
            None => check("folder", true, format!("{} is left unlinked (choose its folder later)", row.name)),
        }
    }
    // Each module's own check: its main store opens and has its tables.
    for (module, file, table) in [
        ("algo", "modules/algo/quantalgo.db", "bots"),
        ("notes", "modules/notes/notes.db", ""),
        ("mcp", "modules/mcp/kanban.db", ""),
        ("pilot", "modules/pilot/pilot.db", ""),
    ] {
        let path = staged_root.join(file);
        if !path.exists() {
            continue;
        }
        let ok = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .is_some_and(|c| table.is_empty() || c.prepare(&format!("SELECT COUNT(*) FROM {table}")).is_ok());
        check("module", ok, format!("{module}: {file}"));
    }
    let systems = staged_root.join("modules/systems/config.json");
    if systems.exists() {
        let ok = std::fs::read_to_string(&systems).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some();
        check("module", ok, "systems: config.json loads".into());
    }
    checks
}

/// The steps only the user can do on the new machine (§2.4).
pub fn manual_steps() -> Vec<String> {
    vec![
        "Log in to Claude Code / Codex on this machine (QuantPilot threads keep their history).".into(),
        "QuantMCP → Connect for each AI client (other apps' config files are not QuantSuite's to copy).".into(),
        "AgentOS import (re-creates ~/.claude/rules/AGENT.md).".into(),
        "QuantCanvas browser: log in to sites again.".into(),
        "Exchange API keys with an IP whitelist: add this machine's public IP.".into(),
        "Trading is off here: bots are stopped and Manual execution is disarmed. Arm it on this machine when ready.".into(),
        "On the old machine: enter the move code under Settings → Data, so it stops trading for good.".into(),
    ]
}

#[derive(Serialize, Deserialize)]
struct Pending {
    staged: PathBuf,
    target: PathBuf,
    at: String,
    #[serde(default)]
    error: Option<String>,
}

fn pending_file(target: &Path) -> PathBuf {
    target.parent().unwrap_or(target).join(PENDING)
}

/// Import a bundle into a staging folder beside `target_root` and schedule
/// the switch for the next start. Nothing in `target_root` is touched.
pub fn import(
    bundle_file: &Path,
    passphrase: &str,
    target_root: &Path,
    rows: &[PathRow],
    keystore: &dyn KeyStore,
) -> Result<Report, String> {
    let parent = target_root.parent().ok_or("the data root has no parent folder")?;
    let staging = parent.join(format!(".quantsuite-import-{}", chrono::Utc::now().format("%Y%m%d%H%M%S")));
    let fail = |e: String| {
        let _ = std::fs::remove_dir_all(&staging);
        e
    };
    // 1. Check: the whole bundle once without writing, then the space.
    let checked = bundle::open(bundle_file, passphrase, None)?;
    check_version(&checked.manifest.suite_version, env!("CARGO_PKG_VERSION"))?;
    let need: u64 = checked.manifest.files.iter().map(|f| f.size).sum();
    if let Ok(free) = fs4::available_space(parent) {
        if free < need + need / 10 + 64 * 1024 * 1024 {
            return Err(format!("Not enough free space: {:.1} GB needed, {:.1} GB free.", need as f64 / 1e9, free as f64 / 1e9));
        }
    }
    // 3. Write into staging.
    let opened = bundle::open(bundle_file, passphrase, Some(&staging)).map_err(fail)?;
    let staged_root = staging.join("quantsuite");
    // 2. Paths.
    let pairs: Vec<(&str, &str)> = rows.iter().filter_map(|r| r.new.as_deref().map(|n| (r.old.as_str(), n))).collect();
    let map = PathMap::new(&pairs);
    remap(&staged_root, &map).map_err(fail)?;
    // 5. Secrets.
    let vault = store_secrets(&staged_root, &opened.secrets, keystore);
    // 6. Verify what arrived, against the manifest.
    let checks = verify(&staged_root, &opened.manifest, vault.as_ref().ok(), rows);
    let ok = checks.iter().all(|c| c.ok) && vault.is_ok();
    // 7. Disarmed — a deliberate change after the check.
    disarm(&staged_root, &opened.manifest.machine).map_err(fail)?;
    let report_dir = staged_root.join("migration");
    std::fs::create_dir_all(&report_dir).map_err(|e| fail(e.to_string()))?;
    let report_file = report_dir.join(format!("import-report-{}.json", chrono::Utc::now().format("%Y%m%d-%H%M%S")));
    let report = Report {
        ok,
        from_machine: opened.manifest.machine.clone(),
        created_at: opened.manifest.created_at.clone(),
        suite_version: opened.manifest.suite_version.clone(),
        checks,
        manual_steps: manual_steps(),
        move_code: opened.move_code.clone(),
        staged: staging.to_string_lossy().into_owned(),
        report_file: report_file.to_string_lossy().into_owned(),
    };
    std::fs::write(&report_file, serde_json::to_vec_pretty(&report).unwrap_or_default()).map_err(|e| fail(e.to_string()))?;
    if !ok {
        // Kept for a look at what failed; never switched.
        return Ok(report);
    }
    let pending = Pending { staged: staged_root, target: target_root.to_path_buf(), at: chrono::Utc::now().to_rfc3339(), error: None };
    std::fs::write(pending_file(target_root), serde_json::to_vec_pretty(&pending).unwrap_or_default()).map_err(|e| fail(e.to_string()))?;
    Ok(report)
}

/// Whether a switch waits for the next start.
pub fn pending(target_root: &Path) -> Option<String> {
    std::fs::read_to_string(pending_file(target_root))
        .ok()
        .and_then(|t| serde_json::from_str::<Pending>(&t).ok())
        .map(|p| p.error.map(|e| format!("the last switch failed: {e}")).unwrap_or(p.at))
}

/// The switch, at start, before anything opens the data root: the current
/// root is kept as `<root>.before-import-<time>`, the staged one takes its
/// place. On a failure both stay as they were and the marker records why.
pub fn apply_pending(target_root: &Path) {
    let file = pending_file(target_root);
    let Ok(text) = std::fs::read_to_string(&file) else { return };
    let Ok(mut p) = serde_json::from_str::<Pending>(&text) else { return };
    if p.error.is_some() || p.target != target_root || !p.staged.is_dir() {
        return;
    }
    let keep = target_root.with_file_name(format!(
        "{}.before-import-{}",
        target_root.file_name().and_then(|n| n.to_str()).unwrap_or("quantsuite"),
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    let moved_old = !target_root.exists() || std::fs::rename(target_root, &keep).is_ok();
    let result = if moved_old {
        std::fs::rename(&p.staged, target_root).map_err(|e| {
            // Put the old root back.
            if keep.exists() {
                let _ = std::fs::rename(&keep, target_root);
            }
            e.to_string()
        })
    } else {
        Err("the data root is in use".to_string())
    };
    match result {
        Ok(()) => {
            let _ = std::fs::remove_file(&file);
            if let Some(staging) = p.staged.parent() {
                let _ = std::fs::remove_dir(staging);
            }
        }
        Err(e) => {
            p.error = Some(e);
            let _ = std::fs::write(&file, serde_json::to_vec_pretty(&p).unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::MemoryKeyStore;
    use std::sync::atomic::AtomicBool;

    const FAST: crate::vault::KdfParams = crate::vault::KdfParams { m_cost_kib: 256, t_cost: 1, p_cost: 1 };

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A populated profile at `<tmp>/old/quantsuite` with a workspace at `<tmp>/old/Projects/Q`.
    fn profile(tmp: &Path) -> (PathBuf, PathBuf) {
        let root = tmp.join("old").join("quantsuite");
        let project = tmp.join("old").join("Projects").join("Q");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(root.join("modules/algo")).unwrap();
        std::fs::create_dir_all(root.join("modules/canvas")).unwrap();
        let core = Connection::open(root.join("core.db")).unwrap();
        crate::db::migrate(&core).unwrap();
        let id = crate::workspaces::workspace_id_for(&project.to_string_lossy());
        crate::db::upsert_entity(
            &core,
            &crate::db::Entity {
                id,
                module: "core".into(),
                kind: "workspace".into(),
                title: "Q".into(),
                subtitle: Some(project.to_string_lossy().into_owned()),
                route: "/".into(),
                icon: None,
                updated_at: 0,
                payload: Some(serde_json::json!({ "path": project.to_string_lossy() })),
            },
        )
        .unwrap();
        drop(core);
        std::fs::write(
            root.join("modules/canvas/workspaces.json"),
            serde_json::to_vec(&serde_json::json!([{ "path": project.to_string_lossy(), "data": root.join("modules").to_string_lossy() }])).unwrap(),
        )
        .unwrap();
        let algo = Connection::open(root.join("modules/algo/quantalgo.db")).unwrap();
        algo.execute_batch(
            "CREATE TABLE bots (id TEXT, status TEXT, trading_mode TEXT, last_error TEXT);
             INSERT INTO bots VALUES ('b1', 'stopped', 'paper', NULL), ('b2', 'stopped', 'live', NULL);
             CREATE TABLE manual_execution (system_id TEXT, settings_json TEXT, armed INTEGER, armed_at TEXT);
             INSERT INTO manual_execution VALUES ('lces', '{}', 0, NULL);
             CREATE TABLE execution_flags (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        (root, project)
    }

    fn export(root: &Path, out: &Path) -> crate::bundle::Exported {
        let secrets = vec![("algo/exchange/e1".to_string(), zeroize::Zeroizing::new(b"{\"api_secret\":\"SK\"}".to_vec()))];
        crate::bundle::export(root, out, "correct horse battery", &Default::default(), &secrets, FAST, &AtomicBool::new(false), &mut |_| {}).unwrap()
    }

    #[test]
    fn a_round_trip_maps_paths_keeps_ids_verifies_and_switches_on_the_next_start() {
        let tmp = Tmp(std::env::temp_dir().join(format!("qs-import-{}", uuid::Uuid::new_v4())));
        let (old_root, old_project) = profile(&tmp.0);
        let exported = export(&old_root, &tmp.0.join("out"));

        // The new machine: another data root, the project in another folder.
        let new_root = tmp.0.join("new").join("quantsuite");
        let new_project = tmp.0.join("new").join("Work").join("Q");
        std::fs::create_dir_all(&new_project).unwrap();
        std::fs::create_dir_all(&new_root).unwrap();
        std::fs::write(new_root.join("core.db"), b"the fresh install's own").unwrap();
        let (manifest, mut rows) = propose(&exported.path, "correct horse battery", &tmp.0.join("probe"), &new_root).unwrap();
        assert_eq!(manifest.machine, exported.manifest.machine);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].old, old_project.to_string_lossy());
        assert_eq!(rows[1].new.as_deref(), Some(old_project.to_string_lossy().as_ref()), "the folder exists here too, so it maps to itself");
        rows[1].new = Some(new_project.to_string_lossy().into_owned());

        let report = import(&exported.path, "correct horse battery", &new_root, &rows, &MemoryKeyStore::default()).unwrap();
        assert!(report.ok, "{:#?}", report.checks);
        assert_eq!(report.move_code, exported.move_code, "both machines derive the same code");
        assert!(report.checks.iter().any(|c| c.name == "rows" && c.ok));
        assert!(report.checks.iter().any(|c| c.name == "secrets" && c.ok));
        assert!(Path::new(&report.report_file).exists());
        assert!(pending(&new_root).is_some());
        // Nothing in the live root was touched yet.
        assert_eq!(std::fs::read(new_root.join("core.db")).unwrap(), b"the fresh install's own");

        // The next start switches.
        apply_pending(&new_root);
        assert!(pending(&new_root).is_none());
        let core = Connection::open(new_root.join("core.db")).unwrap();
        let ws = crate::workspaces::list(&core);
        assert_eq!(ws[0].id, crate::workspaces::workspace_id_for(&old_project.to_string_lossy()), "the id survives");
        assert_eq!(crate::workspaces::normalize_path(&ws[0].path), crate::workspaces::normalize_path(&new_project.to_string_lossy()));
        let canvas = std::fs::read_to_string(new_root.join("modules/canvas/workspaces.json")).unwrap();
        assert!(!canvas.to_lowercase().contains(&crate::workspaces::normalize_path(&old_root.to_string_lossy())), "{canvas}");
        let kept: Vec<_> = std::fs::read_dir(new_root.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("before-import"))
            .collect();
        assert_eq!(kept.len(), 1, "the fresh install's root is kept");
        // Trading arrives disarmed, with a notice.
        let algo = Connection::open(new_root.join("modules/algo/quantalgo.db")).unwrap();
        let notice: String = algo.query_row("SELECT value FROM execution_flags WHERE key = 'imported'", [], |r| r.get(0)).unwrap();
        assert!(notice.contains(&exported.manifest.machine));
        let vault = Vault::open(&new_root.join("secrets"), &MemoryKeyStore::default(), None);
        assert!(vault.is_err(), "another keystore cannot open this machine's vault");
    }

    #[test]
    fn failures_say_why_and_write_nothing() {
        let tmp = Tmp(std::env::temp_dir().join(format!("qs-import-f-{}", uuid::Uuid::new_v4())));
        let (old_root, _) = profile(&tmp.0);
        let exported = export(&old_root, &tmp.0.join("out"));
        let new_root = tmp.0.join("new").join("quantsuite");
        std::fs::create_dir_all(&new_root).unwrap();
        let rows = vec![];
        let err = import(&exported.path, "wrong horse battery", &new_root, &rows, &MemoryKeyStore::default()).err().unwrap();
        assert!(err.contains("passphrase"), "{err}");
        let mut raw = std::fs::read(&exported.path).unwrap();
        let at = raw.len() / 2;
        raw[at] ^= 0x40;
        let bad = tmp.0.join("tampered.qsbundle");
        std::fs::write(&bad, raw).unwrap();
        assert!(import(&bad, "correct horse battery", &new_root, &rows, &MemoryKeyStore::default()).is_err());
        assert!(check_version("1.2.0", "1.0.45").unwrap_err().contains("Update QuantSuite"));
        assert!(check_version("1.0.45", "1.0.45").is_ok() && check_version("1.0.9", "1.0.45").is_ok());
        // A missing folder is reported, not switched.
        let (_, mut rows) = propose(&exported.path, "correct horse battery", &tmp.0.join("probe"), &new_root).unwrap();
        rows[1].new = Some(tmp.0.join("does-not-exist").to_string_lossy().into_owned());
        let report = import(&exported.path, "correct horse battery", &new_root, &rows, &MemoryKeyStore::default()).unwrap();
        assert!(!report.ok);
        assert!(report.checks.iter().any(|c| c.name == "folder" && !c.ok));
        assert!(pending(&new_root).is_none(), "a failed check never schedules the switch");
        let staged: Vec<_> = std::fs::read_dir(tmp.0.join("new")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(staged.iter().all(|n| !n.to_string_lossy().starts_with(".quantsuite-pending")), "{staged:?}");
    }
}
