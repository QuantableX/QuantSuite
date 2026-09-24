//! The Collection against a SYNTHETIC folder source: helper_lib (a library),
//! alpha (requires helper_lib, one version file) and beta (requires alpha).

use super::fetch::{github_error, safe_path, token_problem, Folder, TokenFacts};
use super::sources::{self, SourceInput};
use super::tokens::{MemoryTokens, TokenStore};
use super::*;
use std::cell::RefCell;

struct Tree {
    root: PathBuf,
    repo: PathBuf,
    library: PathBuf,
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn tree(name: &str) -> Tree {
    let root = std::env::temp_dir().join(format!("qs-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let repo = root.join("repo");
    let library = root.join("library");
    fs::create_dir_all(&library).unwrap();
    let t = Tree { root, repo, library };
    write_package(&t.repo, "library", "helper_lib", "1.0.0", 2, &[], "def smooth(x, n):\n    return x\n", None);
    write_package(&t.repo, "indicator", "alpha", "1.0.0", 2, &["helper_lib"], "ALPHA = 1\n", Some(r#"{"key": "alpha_opt", "params": {"length": 30}}"#));
    write_package(&t.repo, "indicator", "beta", "1.0.0", 2, &["alpha"], "BETA = 1\n", None);
    write_catalog(&t.repo);
    t
}

fn folder(kind: &str) -> &'static str {
    if kind == "indicator" { "indicators" } else { "libraries" }
}

#[allow(clippy::too_many_arguments)]
fn write_package(repo: &Path, kind: &str, key: &str, version: &str, contract: u32, requires: &[&str], script: &str, version_file: Option<&str>) {
    let dir = repo.join(folder(kind)).join(key);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("versions")).unwrap();
    fs::write(dir.join(format!("{key}.py")), script).unwrap();
    let mut files = vec![json!({"path": format!("{key}.py"), "sha256": sha256_hex(script.as_bytes()), "role": "script"})];
    if let Some(doc) = version_file {
        let name = format!("versions/{key}_opt.json");
        fs::write(dir.join(&name), doc).unwrap();
        files.push(json!({"path": name, "sha256": sha256_hex(doc.as_bytes()), "role": "version"}));
    }
    fs::write(dir.join("README.md"), "# readme\n").unwrap();
    files.push(json!({"path": "README.md", "sha256": sha256_hex(b"# readme\n"), "role": "readme"}));
    let manifest = json!({
        "format": 1, "type": kind, "key": key, "name": key.to_uppercase(), "summary": "synthetic", "description": "",
        "tags": [kind], "version": version, "released_at": "2026-09-24", "author": "test", "license": "test",
        "contract": contract, "requires": requires, "registers": if kind == "indicator" { json!([key]) } else { json!([]) },
        "files": files, "changelog": [],
    });
    fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
}

fn write_catalog(repo: &Path) {
    let mut items = Vec::new();
    for kind in ["indicator", "library"] {
        let Ok(entries) = fs::read_dir(repo.join(folder(kind))) else { continue };
        for entry in entries.flatten() {
            let path = entry.path().join("manifest.json");
            let bytes = fs::read(&path).unwrap();
            let m: Value = serde_json::from_slice(&bytes).unwrap();
            items.push(json!({
                "type": m["type"], "key": m["key"], "name": m["name"], "summary": m["summary"], "tags": m["tags"],
                "version": m["version"], "contract": m["contract"], "requires": m["requires"], "scores": {},
                "manifest": {"path": format!("{}/{}/manifest.json", folder(kind), m["key"].as_str().unwrap()), "sha256": sha256_hex(&bytes)},
            }));
        }
    }
    items.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    fs::write(repo.join("catalog.json"), serde_json::to_vec_pretty(&json!({"format": 1, "generated_at": "2026-09-24", "items": items})).unwrap()).unwrap();
}

fn source() -> Source {
    Source {
        id: "test".into(),
        kind: "folder".into(),
        name: "Test".into(),
        owner: String::new(),
        repo: String::new(),
        branch: String::new(),
        path: String::new(),
        enabled: true,
        last_commit: None,
        last_checked: None,
    }
}

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    history::init_schema(&conn).unwrap();
    sources::init_schema(&conn).unwrap();
    conn
}

/// Passes, and remembers what it was asked to check.
#[derive(Default)]
struct Pass(RefCell<Vec<Vec<String>>>);

impl Stager for Pass {
    fn check(&self, staged: &Path, files: &[String]) -> Result<Value, String> {
        for file in files {
            assert!(staged.join(file).is_file(), "{file} is staged");
        }
        self.0.borrow_mut().push(files.to_vec());
        Ok(json!({"blocking": false, "ok": true}))
    }
}

struct Block;

impl Stager for Block {
    fn check(&self, _: &Path, _: &[String]) -> Result<Value, String> {
        Ok(json!({"blocking": true, "ok": false, "import": {"ok": false, "message": "ImportError: synthetic"}}))
    }
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| k.to_string()).collect()
}

fn run(t: &Tree, conn: &Mutex<Connection>, list: &[&str], overwrite: bool, stager: &dyn Stager) -> Result<InstallOutcome, String> {
    let fetch = Folder::new(&t.repo);
    let commit = fetch.commit().unwrap();
    install(&t.library, conn, &source(), &fetch, &commit, &keys(list), overwrite, Some(2), stager)
}

#[test]
fn an_install_brings_the_requirements_first_and_records_everything() {
    let t = tree("closure");
    let conn = Mutex::new(db());
    let stager = Pass::default();
    let outcome = run(&t, &conn, &["beta"], false, &stager).unwrap();
    let order: Vec<&str> = outcome.plan.items.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(order, ["helper_lib", "alpha", "beta"]);
    assert_eq!(outcome.plan.items.iter().map(|i| i.requested).collect::<Vec<_>>(), [false, false, true]);
    assert!(outcome.plan.items.iter().all(|i| i.action == "create"));
    assert_eq!(stager.0.borrow().as_slice(), [keys(&["helper_lib.py", "alpha.py", "beta.py"])]);
    for file in ["helper_lib.py", "alpha.py", "beta.py", "versions/alpha/alpha_opt.json"] {
        assert!(t.library.join(file).is_file(), "{file} installed");
    }
    assert!(!t.library.join("README.md").exists(), "a README is shown, not installed");
    let installed = load_installed(&t.library).unwrap();
    assert_eq!(installed.items.keys().collect::<Vec<_>>(), ["alpha", "beta", "helper_lib"]);
    assert_eq!(installed.items["beta"].requires, ["alpha"]);
    assert_eq!(installed.items["alpha"].files.len(), 2);
    assert!(outcome.written.iter().all(|w| w.created));
    let latest = history::latest(&conn.lock().unwrap(), "alpha.py").unwrap().unwrap();
    assert_eq!(latest.author, "store");
    assert!(latest.checked);
    assert!(latest.message.starts_with("Installed alpha 1.0.0 from Test@local-"), "{}", latest.message);
}

#[test]
fn identical_files_are_skipped_and_nothing_is_rewritten() {
    let t = tree("identical");
    let conn = Mutex::new(db());
    run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap();
    let again = run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap();
    assert!(again.plan.items.iter().all(|i| i.action == "skip"), "{:?}", again.plan.items);
    assert!(again.written.is_empty());
    assert_eq!(history::latest(&conn.lock().unwrap(), "alpha.py").unwrap().unwrap().version, 1);

    // The same bytes the user put there themselves: skipped too, not a conflict.
    let t2 = tree("identical-own");
    fs::copy(t2.repo.join("libraries/helper_lib/helper_lib.py"), t2.library.join("helper_lib.py")).unwrap();
    let outcome = run(&t2, &conn, &["helper_lib"], false, &Pass::default()).unwrap();
    assert_eq!(outcome.plan.items[0].action, "skip");
    assert_eq!(outcome.plan.items[0].reason.as_deref(), Some("already in the library — identical"));
}

#[test]
fn a_file_the_store_did_not_install_is_a_conflict_and_nothing_is_written() {
    let t = tree("conflict");
    let conn = Mutex::new(db());
    fs::write(t.library.join("alpha.py"), "MY_OWN = 1\n").unwrap();
    let err = run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap_err();
    assert!(err.contains("conflicts: alpha"), "{err}");
    assert_eq!(fs::read_to_string(t.library.join("alpha.py")).unwrap(), "MY_OWN = 1\n");
    assert!(!t.library.join("helper_lib.py").exists(), "no partial install");
    assert!(!t.library.join(STATE_FILE).exists());
}

#[test]
fn overwriting_keeps_the_replaced_content_in_the_history() {
    let t = tree("overwrite");
    let conn = Mutex::new(db());
    fs::write(t.library.join("alpha.py"), "MY_OWN = 1\n").unwrap();
    run(&t, &conn, &["alpha"], true, &Pass::default()).unwrap();
    assert_eq!(fs::read_to_string(t.library.join("alpha.py")).unwrap(), "ALPHA = 1\n");
    let versions = history::list(&conn.lock().unwrap(), "alpha.py").unwrap();
    let authors: Vec<&str> = versions.iter().map(|v| v.author.as_str()).collect();
    assert_eq!(authors, ["store", "external"], "newest first");
    let (_, before) = history::read(&conn.lock().unwrap(), "alpha.py", versions[1].version).unwrap().unwrap();
    assert_eq!(before, "MY_OWN = 1\n");
}

#[test]
fn an_update_replaces_the_stores_files_and_a_local_edit_is_a_conflict() {
    let t = tree("update");
    let conn = Mutex::new(db());
    run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap();

    write_package(&t.repo, "indicator", "alpha", "1.1.0", 2, &["helper_lib"], "ALPHA = 2\n", None);
    write_catalog(&t.repo);
    let fetch = Folder::new(&t.repo);
    let commit = fetch.commit().unwrap();
    let catalog = load_catalog(&fetch, &commit).unwrap();
    let view = catalog_view(&t.library, &load_installed(&t.library).unwrap(), &catalog, Some(2));
    let alpha = view.iter().find(|v| v["key"] == "alpha").unwrap();
    assert_eq!(alpha["local"]["installed_version"], "1.0.0");
    assert_eq!(alpha["local"]["update"], true);

    let outcome = run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap();
    let item = outcome.plan.items.iter().find(|i| i.key == "alpha").unwrap();
    assert_eq!(item.action, "replace");
    assert!(item.files.contains(&PlanFile { path: "versions/alpha/alpha_opt.json".into(), action: "remove".into() }));
    assert!(!t.library.join("versions/alpha/alpha_opt.json").exists(), "a version the package dropped goes");
    assert_eq!(load_installed(&t.library).unwrap().items["alpha"].version, "1.1.0");
    assert!(!outcome.written.iter().find(|w| w.key == "alpha").unwrap().created);

    fs::write(t.library.join("alpha.py"), "ALPHA = 'edited'\n").unwrap();
    write_package(&t.repo, "indicator", "alpha", "1.2.0", 2, &["helper_lib"], "ALPHA = 3\n", None);
    write_catalog(&t.repo);
    let err = run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap_err();
    assert!(err.contains("changed in the library since the Collection installed it: alpha.py"), "{err}");
}

#[test]
fn a_required_item_cannot_be_removed_and_removal_is_archived() {
    let t = tree("remove");
    let conn = Mutex::new(db());
    run(&t, &conn, &["beta"], false, &Pass::default()).unwrap();
    let err = remove(&t.library, &conn.lock().unwrap(), "alpha").unwrap_err();
    assert_eq!(err, "alpha is required by beta — remove those first.");
    let removed = remove(&t.library, &conn.lock().unwrap(), "beta").unwrap();
    assert_eq!(removed.keys, ["beta"]);
    assert!(!t.library.join("beta.py").exists());
    let removed = remove(&t.library, &conn.lock().unwrap(), "alpha").unwrap();
    assert_eq!(removed.files, ["alpha.py", "versions/alpha/alpha_opt.json"]);
    assert!(!t.library.join("versions/alpha").exists());
    assert_eq!(history::latest(&conn.lock().unwrap(), "alpha.py").unwrap().unwrap().author, "delete");
    assert!(remove(&t.library, &conn.lock().unwrap(), "alpha").unwrap_err().contains("not installed by the Collection"));
    assert_eq!(load_installed(&t.library).unwrap().items.keys().collect::<Vec<_>>(), ["helper_lib"]);
}

#[test]
fn a_file_or_manifest_that_does_not_match_its_hash_is_refused() {
    let t = tree("hash");
    let conn = Mutex::new(db());
    fs::write(t.repo.join("indicators/alpha/alpha.py"), "TAMPERED = 1\n").unwrap();
    let err = run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap_err();
    assert_eq!(err, "alpha.py of alpha does not match its manifest (sha256) — refusing it.");

    let t = tree("hash-manifest");
    let path = t.repo.join("indicators/beta/manifest.json");
    let text = fs::read_to_string(&path).unwrap().replace("synthetic", "changed");
    fs::write(&path, text).unwrap();
    let err = run(&t, &conn, &["beta"], false, &Pass::default()).unwrap_err();
    assert_eq!(err, "The manifest of beta does not match the catalog (sha256) — refusing it.");
    assert!(!t.library.join(STATE_FILE).exists());
}

#[test]
fn a_package_for_a_newer_contract_is_refused() {
    let t = tree("contract");
    write_package(&t.repo, "indicator", "beta", "1.0.0", 3, &["alpha"], "BETA = 1\n", None);
    write_catalog(&t.repo);
    let conn = Mutex::new(db());
    let err = run(&t, &conn, &["beta"], false, &Pass::default()).unwrap_err();
    assert_eq!(err, "beta needs a newer QuantSuite (contract 3; this engine has 2).");
    let fetch = Folder::new(&t.repo);
    let catalog = load_catalog(&fetch, &fetch.commit().unwrap()).unwrap();
    let view = catalog_view(&t.library, &Installed::default(), &catalog, Some(2));
    assert_eq!(view.iter().find(|v| v["key"] == "beta").unwrap()["local"]["too_new"], true);
    assert_eq!(view.iter().find(|v| v["key"] == "alpha").unwrap()["local"]["too_new"], false);
}

#[test]
fn a_staged_library_the_engine_refuses_writes_nothing() {
    let t = tree("blocked");
    let conn = Mutex::new(db());
    let err = run(&t, &conn, &["alpha"], false, &Block).unwrap_err();
    assert_eq!(err, "Not installed — the library would not load with these files: ImportError: synthetic");
    assert!(fs::read_dir(&t.library).unwrap().next().is_none(), "the library is untouched");
}

#[test]
fn requirements_must_exist_and_paths_must_stay_inside() {
    let t = tree("closure-missing");
    fs::remove_dir_all(t.repo.join("libraries/helper_lib")).unwrap();
    write_catalog(&t.repo);
    let fetch = Folder::new(&t.repo);
    let catalog = load_catalog(&fetch, &fetch.commit().unwrap()).unwrap();
    assert_eq!(closure(&catalog, &keys(&["beta"])).unwrap_err(), "alpha requires helper_lib, which the catalog does not have.");
    assert_eq!(closure(&catalog, &keys(&["gamma"])).unwrap_err(), "gamma is not in the catalog.");
    assert!(safe_path("indicators/alpha/manifest.json"));
    for bad in ["", "/etc/passwd", "../x", "a/../b", "a//b", "a\\b", "C:/x"] {
        assert!(!safe_path(bad), "{bad}");
    }
}

#[test]
fn github_errors_say_what_to_do() {
    assert_eq!(github_error("o/r", 404, None, "", false), "o/r: not found or no access — a private repository needs a token.");
    assert_eq!(github_error("o/r", 404, None, "", true), "o/r: not found, or the token has no access to it.");
    assert_eq!(github_error("o/r", 403, Some("0"), "{}", false), "GitHub rate limit reached — add a token.");
    assert_eq!(github_error("o/r", 401, None, "", true), "GitHub refused the token for o/r — set a new one.");
    assert_eq!(github_error("o/r", 500, None, r#"{"message":"boom"}"#, false), "GitHub answered 500 for o/r: boom");
}

#[test]
fn a_token_is_never_serialized() {
    let conn = db();
    let tokens = MemoryTokens::default();
    let secret = "ghp_synthetic_secret_0123456789";
    tokens.set("quantablex", secret).unwrap();
    let seeded = sources::get(&conn, "quantablex").unwrap();
    let view = sources::view(seeded, &tokens);
    let text = serde_json::to_string(&view).unwrap();
    assert!(text.contains(r#""has_token":true"#), "{text}");
    assert!(!text.contains(secret));
    let all: Vec<_> = sources::list(&conn).unwrap().into_iter().map(|s| sources::view(s, &tokens)).collect();
    assert!(!serde_json::to_string(&all).unwrap().contains(secret));
    tokens.clear("quantablex").unwrap();
    assert!(!sources::view(sources::get(&conn, "quantablex").unwrap(), &tokens).has_token);
}

#[test]
fn sources_are_seeded_once_and_validated() {
    let conn = db();
    let seeded = sources::list(&conn).unwrap();
    assert_eq!(seeded.len(), 1);
    assert_eq!((seeded[0].owner.as_str(), seeded[0].repo.as_str(), seeded[0].branch.as_str()), ("QuantableX", "QuantScript-Collection", "main"));
    sources::delete(&conn, "quantablex").unwrap();
    sources::init_schema(&conn).unwrap();
    assert!(sources::list(&conn).unwrap().is_empty(), "a deleted seed stays deleted");

    let input = |kind: &str, owner: &str, path: &str| SourceInput {
        id: String::new(), kind: kind.into(), name: "My catalog".into(), owner: owner.into(), repo: "Repo".into(),
        branch: String::new(), path: path.into(), enabled: true,
    };
    let saved = sources::save(&conn, input("github", "someone", "")).unwrap();
    assert_eq!((saved.id.as_str(), saved.branch.as_str()), ("my-catalog", "main"));
    assert_eq!(sources::save(&conn, input("github", "someone", "")).unwrap().id, "my-catalog-2");
    assert!(sources::save(&conn, input("github", "bad owner", "")).is_err());
    assert!(sources::save(&conn, input("github", "o", "../up")).is_err());
    assert!(sources::save(&conn, input("folder", "", "relative/path")).is_err());
    assert!(sources::save(&conn, input("ftp", "o", "")).is_err());
    sources::remember_commit(&conn, "my-catalog", "abc").unwrap();
    let mut moved = input("github", "someone-else", "");
    moved.id = "my-catalog".into();
    assert_eq!(sources::save(&conn, moved).unwrap().last_commit, None, "a moved source forgets its commit");
}

#[test]
fn a_store_era_database_is_renamed_and_its_seed_follows_the_repository() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE store_sources (id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, owner TEXT NOT NULL DEFAULT '',
            repo TEXT NOT NULL DEFAULT '', branch TEXT NOT NULL DEFAULT 'main', path TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1, last_commit TEXT, last_checked TEXT, created_at TEXT NOT NULL);
         INSERT INTO store_sources (id, kind, name, owner, repo, branch, path, enabled, last_commit, created_at)
           VALUES ('quantablex', 'github', 'QuantableX', 'QuantableX', 'QuantScript-Indicators', 'main', '', 1, 'abc', 'x'),
                  ('mine', 'github', 'Mine', 'someone', 'QuantScript-Indicators', 'main', '', 1, 'def', 'y');
         PRAGMA user_version = 1;",
    )
    .unwrap();
    sources::init_schema(&conn).unwrap();
    let all = sources::list(&conn).unwrap();
    let seed = all.iter().find(|s| s.id == "quantablex").unwrap();
    assert_eq!((seed.repo.as_str(), seed.last_commit.as_deref()), ("QuantScript-Collection", None));
    let mine = all.iter().find(|s| s.id == "mine").unwrap();
    assert_eq!((mine.repo.as_str(), mine.last_commit.as_deref()), ("QuantScript-Indicators", Some("def")), "only the untouched seed moves");
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 2);
    sources::init_schema(&conn).unwrap();
    assert_eq!(sources::list(&conn).unwrap().len(), 2, "idempotent");
}

#[test]
fn a_store_era_state_file_is_read_and_replaced() {
    let t = tree("old-state");
    let conn = Mutex::new(db());
    run(&t, &conn, &["helper_lib"], false, &Pass::default()).unwrap();
    fs::rename(t.library.join(STATE_FILE), t.library.join("store.json")).unwrap();
    assert!(load_installed(&t.library).unwrap().items.contains_key("helper_lib"), "the old file is read");
    run(&t, &conn, &["alpha"], false, &Pass::default()).unwrap();
    assert!(t.library.join(STATE_FILE).is_file());
    assert!(!t.library.join("store.json").exists(), "and replaced on the next save");
    assert_eq!(load_installed(&t.library).unwrap().items.keys().collect::<Vec<_>>(), ["alpha", "helper_lib"]);
}

#[test]
fn a_refused_token_is_explained_without_showing_it() {
    let facts = |login: Option<&str>, user: u16, repo: u16| TokenFacts {
        kind: "fine-grained",
        login: login.map(str::to_string),
        user_status: Some(user),
        repo_status: Some(repo),
        accepted: Some("contents=read".into()),
        expires: Some("2026-10-24 00:00:00 UTC".into()),
    };
    let msg = token_problem("QuantableX/QS", "QuantableX", 404, &facts(Some("QuantableX"), 200, 404));
    assert!(msg.starts_with("The fine-grained token belongs to QuantableX but GitHub does not show it QuantableX/QS (answer 404)."), "{msg}");
    assert!(msg.contains("Only select repositories → QuantableX/QS"), "{msg}");
    assert!(msg.contains("Contents: Read-only"), "{msg}");
    assert!(msg.contains("GitHub wants: contents=read."), "{msg}");
    assert!(msg.contains("It expires 2026-10-24"), "{msg}");
    assert!(!msg.contains("not QuantableX's token"));

    let other = token_problem("QuantableX/QS", "QuantableX", 404, &facts(Some("someone-else"), 200, 404));
    assert!(other.contains("It is not QuantableX's token"), "{other}");

    let visible = token_problem("QuantableX/QS", "QuantableX", 404, &facts(Some("QuantableX"), 200, 200));
    assert!(visible.contains("sees QuantableX/QS, but may not read its files"), "{visible}");

    let refused = token_problem("QuantableX/QS", "QuantableX", 404, &facts(None, 401, 404));
    assert_eq!(refused, "GitHub refused the token (fine-grained token): it is wrong, revoked or expired — set a new one.");
}
