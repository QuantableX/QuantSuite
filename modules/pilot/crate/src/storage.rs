//! Storage (parity 19): what QuantPilot keeps in its data folder, and rules
//! that keep it bounded without touching anything still in use.
//!
//! The inventory measures each category. A cleanup is planned first (the
//! dry run the Settings page shows) and carried out from the same plan, only
//! for the items the user saw. Every rule is off by default; folders of
//! deleted threads are the one thing a cleanup always offers. Kept: message
//! text and history (handoff and history reads depend on them), browser
//! profiles (Browser tab), worktrees (worktree cleanup with its own guards),
//! the skill library, anything referenced by a message, a queued message or
//! a draft, everything of a thread whose turn runs, and anything in a folder
//! that is not QuantPilot's own. Targets are resolved and must lie inside the
//! data folder's own subfolders.

use super::Database;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The cleanup rules (Settings → Storage); `None` keeps that category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoragePolicy {
    /// Saved patch bodies of turns older than this lose their text; the file
    /// list and line counts stay.
    pub patch_days: Option<u32>,
    /// Saved attachments that no message, queued message or draft uses
    /// (screenshots, pastes, files left behind), older than this.
    pub unsent_days: Option<u32>,
    /// Extracted document text not used for this long (read again on demand).
    pub document_days: Option<u32>,
    /// Folders left by deleted threads.
    pub orphans: bool,
    /// Apply the rules once a day by themselves.
    pub auto: bool,
    pub last_run: Option<i64>,
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self { patch_days: None, unsent_days: None, document_days: None, orphans: true, auto: false, last_run: None }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub key: String,
    pub label: String,
    pub bytes: u64,
    pub items: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// `orphans` | `unsent` | `documents` | `patches`
    pub category: String,
    /// What goes: a path, or `turn <id>` for saved patches.
    pub target: String,
    pub bytes: u64,
    pub reason: String,
    pub thread: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub category: String,
    pub reason: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub items: Vec<Item>,
    pub bytes: u64,
    pub kept: Vec<Kept>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub removed: u64,
    pub bytes: u64,
    pub failed: Vec<Failure>,
    pub skipped: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub target: String,
    pub error: String,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// A folder walk stops counting here; the size is then "at least".
const WALK_MAX: usize = 200_000;

fn is_thread_id(name: &str) -> bool {
    name.len() == 36 && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && name.matches('-').count() == 4
}

/// Bytes and files under `dir` (bounded).
pub fn dir_size(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut files, mut seen) = (0u64, 0u64, 0usize);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > WALK_MAX {
                return (bytes, files);
            }
            let Ok(meta) = e.metadata() else { continue };
            // Links (worktree junctions) are not followed.
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(e.path());
            } else {
                bytes += meta.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

fn age_days(path: &Path, now: SystemTime) -> Option<u64> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(now.duration_since(modified).unwrap_or(Duration::ZERO).as_secs() / 86_400)
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

#[derive(Default)]
struct KeptTally(HashMap<(String, String), (u64, u64)>);

impl KeptTally {
    fn add(&mut self, category: &str, reason: &str, bytes: u64) {
        let e = self.0.entry((category.to_string(), reason.to_string())).or_default();
        e.0 += 1;
        e.1 += bytes;
    }
    fn into_vec(self) -> Vec<Kept> {
        let mut v: Vec<Kept> = self.0.into_iter().map(|((category, reason), (count, bytes))| Kept { category, reason, count, bytes }).collect();
        v.sort_by(|a, b| a.category.cmp(&b.category).then(b.count.cmp(&a.count)));
        v
    }
}

impl Database {
    fn thread_titles(&self) -> HashMap<String, String> {
        let conn = self.lock();
        let Ok(mut stmt) = conn.prepare("SELECT id, title FROM threads") else { return HashMap::new() };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// A saved attachment is in use when a message (any of its data, also a
    /// question's files or a quote) or a queued message names it.
    fn attachment_used(&self, thread: &str, file_name: &str) -> bool {
        let conn = self.lock();
        let hit = |sql: &str| conn.query_row(sql, params![thread, file_name], |r| r.get::<_, bool>(0)).unwrap_or(true);
        hit("SELECT EXISTS(SELECT 1 FROM timeline WHERE thread_id = ?1 AND instr(data, ?2) > 0)")
            || hit("SELECT EXISTS(SELECT 1 FROM queue WHERE thread_id = ?1 AND (instr(COALESCE(payload, ''), ?2) > 0 OR instr(text, ?2) > 0))")
    }

    /// What QuantPilot keeps, by category.
    pub fn storage_inventory(&self, data_dir: &Path) -> Vec<Category> {
        let cat = |key: &str, label: &str, (bytes, items): (u64, u64), note: &str| Category { key: key.into(), label: label.into(), bytes, items, note: note.into() };
        let db_bytes: u64 = ["pilot.db", "pilot.db-wal", "pilot.db-shm"].iter().map(|f| file_size(&data_dir.join(f))).sum();
        let (patch_rows, patch_bytes): (u64, u64) = {
            let conn = self.lock();
            conn.query_row(
                "SELECT count(*), COALESCE(sum(length(patch) + COALESCE(length(before_text), 0) + COALESCE(length(after_text), 0)), 0)
                   FROM turn_changes WHERE patch != '' OR before_text IS NOT NULL OR after_text IS NOT NULL",
                [],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )
            .unwrap_or((0, 0))
        };
        let threads = self.thread_titles();
        let attachments = data_dir.join("attachments");
        let (mut att, mut orphan) = ((0u64, 0u64), (0u64, 0u64));
        for e in std::fs::read_dir(&attachments).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == ".documents" || !e.path().is_dir() {
                continue;
            }
            let (b, n) = dir_size(&e.path());
            if is_thread_id(&name) && !threads.contains_key(&name) {
                orphan = (orphan.0 + b, orphan.1 + n);
            }
            att = (att.0 + b, att.1 + n);
        }
        let worktrees = {
            let conn = self.lock();
            conn.query_row("SELECT count(*) FROM threads WHERE COALESCE(worktree_path, '') != ''", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as u64
        };
        vec![
            cat("database", "Messages and history", (db_bytes, 0), "Kept: message text and history stay for handoffs and history reads."),
            cat("patches", "Saved patches", (patch_bytes, patch_rows), "Each turn's file changes as text; an old one can lose its text and keep its file list."),
            cat(
                "attachments",
                "Attachments and screenshots",
                att,
                &if orphan.1 > 0 { format!("{} file{} ({}) belong{} to deleted threads.", orphan.1, if orphan.1 == 1 { "" } else { "s" }, human(orphan.0), if orphan.1 == 1 { "s" } else { "" }) } else { String::new() },
            ),
            cat("documents", "Extracted document text", dir_size(&attachments.join(".documents")), "A cache: read again when needed."),
            cat("sessions", "Session files", dir_size(&data_dir.join("sessions")), "Each thread's launch context."),
            cat("browserProfiles", "Browser profiles", dir_size(&data_dir.join("browser-profiles")), "Managed in the Browser tab's profile dialog."),
            cat("skills", "Skill library", dir_size(&data_dir.join("skills")), "Managed in Settings → Skills."),
            cat("worktrees", "Managed worktrees", (0, worktrees), "In their repositories; removed with Worktree cleanup, which checks for unsaved and unmerged work."),
        ]
    }

    /// The dry run: what `policy` would remove now, and what stays and why.
    /// `drafts` are paths the composer still holds; `busy` threads keep all.
    pub fn storage_plan(&self, data_dir: &Path, policy: &StoragePolicy, drafts: &[String], busy: &[String], now_ms: i64) -> Plan {
        let now = SystemTime::UNIX_EPOCH + Duration::from_millis(now_ms.max(0) as u64);
        let threads = self.thread_titles();
        let busy: HashSet<&str> = busy.iter().map(String::as_str).collect();
        let mut items = Vec::new();
        let mut kept = KeptTally::default();

        // Folders of threads that no longer exist, in attachments and sessions.
        for (root, what) in [(data_dir.join("attachments"), "attachments"), (data_dir.join("sessions"), "session files")] {
            for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name == ".documents" {
                    continue;
                }
                let path = e.path();
                let size = if path.is_dir() { dir_size(&path).0 } else { file_size(&path) };
                if !path.is_dir() || !is_thread_id(&name) {
                    kept.add("orphans", "not QuantPilot's own folder", size);
                    continue;
                }
                if threads.contains_key(&name) {
                    continue;
                }
                if policy.orphans {
                    items.push(Item { category: "orphans".into(), target: path.to_string_lossy().into_owned(), bytes: size, reason: format!("{what} of a deleted thread"), thread: None });
                } else {
                    kept.add("orphans", "the rule is off", size);
                }
            }
        }

        // Attachments no message, queued message or draft uses.
        if let Some(days) = policy.unsent_days {
            let drafted: HashSet<String> = drafts.iter().filter_map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned())).collect();
            for (id, title) in &threads {
                let dir = data_dir.join("attachments").join(id);
                for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let path = e.path();
                    if !path.is_file() {
                        continue;
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    let size = file_size(&path);
                    if busy.contains(id.as_str()) {
                        kept.add("unsent", "its thread is working", size);
                    } else if drafted.contains(&name) {
                        kept.add("unsent", "in a draft", size);
                    } else if self.attachment_used(id, &name) {
                        kept.add("unsent", "in a message or queued message", size);
                    } else if !age_days(&path, now).is_some_and(|a| a >= days as u64) {
                        kept.add("unsent", &format!("newer than {days} days"), size);
                    } else {
                        items.push(Item {
                            category: "unsent".into(),
                            target: path.to_string_lossy().into_owned(),
                            bytes: size,
                            reason: format!("in no message or draft, saved over {days} days ago"),
                            thread: Some(title.clone()),
                        });
                    }
                }
            }
        }

        // Extracted document text not used for a while.
        if let Some(days) = policy.document_days {
            let docs = data_dir.join("attachments").join(".documents");
            for e in std::fs::read_dir(&docs).into_iter().flatten().flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let text = path.with_extension("txt");
                let size = file_size(&path) + file_size(&text);
                if !age_days(&path, now).is_some_and(|a| a >= days as u64) {
                    kept.add("documents", &format!("used in the last {days} days"), size);
                } else {
                    items.push(Item {
                        category: "documents".into(),
                        target: path.to_string_lossy().into_owned(),
                        bytes: size,
                        reason: format!("not used for {days} days; read again when needed"),
                        thread: None,
                    });
                }
            }
        }

        // Saved patch bodies of old turns.
        if let Some(days) = policy.patch_days {
            let cutoff = now_ms - days as i64 * DAY_MS;
            let rows: Vec<(String, String, i64, i64, i64)> = {
                let conn = self.lock();
                let mut stmt = conn
                    .prepare(
                        "SELECT tc.turn_id, tc.thread_id, count(*),
                                sum(length(tc.patch) + COALESCE(length(tc.before_text), 0) + COALESCE(length(tc.after_text), 0)),
                                max(COALESCE(t.ended_at, t.started_at, 0))
                           FROM turn_changes tc LEFT JOIN turns t ON t.id = tc.turn_id
                          WHERE tc.patch != '' OR tc.before_text IS NOT NULL OR tc.after_text IS NOT NULL
                          GROUP BY tc.turn_id, tc.thread_id",
                    )
                    .expect("static SQL");
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map(|r| r.flatten().collect()).unwrap_or_default()
            };
            for (turn, thread, files, bytes, at) in rows {
                if busy.contains(thread.as_str()) {
                    kept.add("patches", "its thread is working", bytes as u64);
                } else if at == 0 || at >= cutoff {
                    kept.add("patches", &format!("newer than {days} days"), bytes as u64);
                } else {
                    items.push(Item {
                        category: "patches".into(),
                        target: format!("turn {turn}"),
                        bytes: bytes as u64,
                        reason: format!("{files} file{} of a turn over {days} days old; the file list and line counts stay", if files == 1 { "" } else { "s" }),
                        thread: threads.get(&thread).cloned(),
                    });
                }
            }
        }

        let bytes = items.iter().map(|i| i.bytes).sum();
        Plan { items, bytes, kept: kept.into_vec() }
    }

    /// Carries out `plan` — only the items named in `only` when given (the
    /// ones the user saw). Removing what is already gone counts as done.
    pub fn storage_clean(&self, data_dir: &Path, plan: &Plan, only: Option<&HashSet<String>>, today: &str) -> CleanResult {
        let mut out = CleanResult::default();
        let roots: Vec<PathBuf> = ["attachments", "sessions"].iter().filter_map(|r| std::fs::canonicalize(data_dir.join(r)).ok()).collect();
        let docs = std::fs::canonicalize(data_dir.join("attachments").join(".documents")).ok();
        for item in &plan.items {
            if only.is_some_and(|o| !o.contains(&item.target)) {
                out.skipped += 1;
                continue;
            }
            let done: Result<(), String> = match item.category.as_str() {
                "orphans" | "unsent" => remove_inside(Path::new(&item.target), &roots),
                "documents" => match &docs {
                    Some(root) => {
                        let json = Path::new(&item.target);
                        remove_inside(json, std::slice::from_ref(root)).and_then(|_| remove_inside(&json.with_extension("txt"), std::slice::from_ref(root)))
                    }
                    None => Ok(()),
                },
                "patches" => {
                    let turn = item.target.trim_start_matches("turn ");
                    let note = format!("The saved patch was removed by storage cleanup on {today}; the file list and line counts are kept.");
                    let conn = self.lock();
                    conn.execute(
                        "UPDATE turn_changes SET patch = '', before_text = NULL, after_text = NULL,
                                note = CASE WHEN COALESCE(note, '') = '' THEN ?2 ELSE note || ' ' || ?2 END
                          WHERE turn_id = ?1 AND (patch != '' OR before_text IS NOT NULL OR after_text IS NOT NULL)",
                        params![turn, note],
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
                }
                other => Err(format!("unknown category {other}")),
            };
            match done {
                Ok(()) => {
                    out.removed += 1;
                    out.bytes += item.bytes;
                }
                Err(error) => out.failed.push(Failure { target: item.target.clone(), error }),
            }
        }
        // Freed patch text: fold the write-ahead log back so it does not stay large.
        if plan.items.iter().any(|i| i.category == "patches") {
            let _ = self.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        out
    }
}

fn human(n: u64) -> String {
    match n {
        n if n >= 1 << 30 => format!("{:.1} GB", n as f64 / (1u64 << 30) as f64),
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n if n >= 1 << 10 => format!("{} KB", n / 1024),
        n => format!("{n} B"),
    }
}

/// Removes `target` (file or folder) only when it resolves strictly inside one of `roots`.
fn remove_inside(target: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let resolved = match std::fs::canonicalize(target) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    if !roots.iter().any(|r| resolved.starts_with(r) && resolved != *r) {
        return Err("outside QuantPilot's data folders — left alone".into());
    }
    let meta = std::fs::symlink_metadata(&resolved).map_err(|e| e.to_string())?;
    let r = if meta.is_dir() { std::fs::remove_dir_all(&resolved) } else { std::fs::remove_file(&resolved) };
    match r {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{e} (in use? it is tried again next time)")),
    }
}

/// One line per run in `storage-cleanup.log` (the newest 200 lines kept).
pub fn log_run(data_dir: &Path, when: &str, plan: &Plan, result: &CleanResult) {
    let mut per: HashMap<&str, u64> = HashMap::new();
    for i in &plan.items {
        *per.entry(i.category.as_str()).or_default() += 1;
    }
    let mut cats: Vec<String> = per.iter().map(|(k, v)| format!("{k} {v}")).collect();
    cats.sort();
    let line = format!("{when} removed {} ({} bytes), failed {}, skipped {}; planned: {}", result.removed, result.bytes, result.failed.len(), result.skipped, if cats.is_empty() { "nothing".into() } else { cats.join(", ") });
    let path = data_dir.join("storage-cleanup.log");
    let mut lines: Vec<String> = std::fs::read_to_string(&path).unwrap_or_default().lines().map(str::to_string).collect();
    lines.push(line);
    let start = lines.len().saturating_sub(200);
    let _ = std::fs::write(&path, lines[start..].join("\n") + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Item as TItem, ItemKind};
    use serde_json::json;

    const A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const B: &str = "bbbbbbbb-0000-0000-0000-000000000002";
    const GONE: &str = "cccccccc-0000-0000-0000-000000000003";

    fn real_now() -> i64 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as i64
    }

    fn old(path: &Path, days: u64) {
        let t = SystemTime::now() - Duration::from_secs(days * 86_400);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    fn fixture() -> (Database, PathBuf) {
        let data = std::env::temp_dir().join(format!("qp-storage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data).unwrap();
        let db = Database::open_in_memory().unwrap();
        for id in [A, B] {
            db.lock().execute("INSERT INTO threads(id,provider,context_id,cwd,created_at,updated_at,title) VALUES(?1,'claude',?1,'.',1,1,?1)", [id]).unwrap();
        }
        // Thread A: a sent screenshot, an unsent one, a drafted paste, a new file.
        let att = data.join("attachments");
        for (dir, files) in [(A, vec!["11111111-sent.png", "22222222-unsent.png", "33333333-draft.txt", "44444444-new.png"]), (B, vec!["55555555-busy.png"]), (GONE, vec!["66666666-left.png"])] {
            std::fs::create_dir_all(att.join(dir)).unwrap();
            for f in files {
                std::fs::write(att.join(dir).join(f), vec![7u8; 1000]).unwrap();
                if !f.contains("new") {
                    old(&att.join(dir).join(f), 40);
                }
            }
        }
        std::fs::create_dir_all(att.join("not-a-thread")).unwrap();
        std::fs::write(att.join("not-a-thread").join("keep.txt"), b"x").unwrap();
        std::fs::create_dir_all(data.join("sessions").join(GONE)).unwrap();
        std::fs::write(data.join("sessions").join(GONE).join("context.txt"), b"ctx").unwrap();
        let docs = att.join(".documents");
        std::fs::create_dir_all(&docs).unwrap();
        for (k, age) in [("old", 100), ("fresh", 1)] {
            std::fs::write(docs.join(format!("{k}-v1.json")), b"{}").unwrap();
            std::fs::write(docs.join(format!("{k}-v1.txt")), b"text").unwrap();
            old(&docs.join(format!("{k}-v1.json")), age);
        }
        let sent = TItem {
            id: "u1".into(),
            thread_id: A.into(),
            turn_id: "t-old".into(),
            seq: 1,
            kind: ItemKind::User,
            parent: None,
            data: json!({ "text": "look", "attachments": [{ "kind": "image", "path": att.join(A).join("11111111-sent.png").to_string_lossy() }] }),
            created_at: 1,
            updated_at: 1,
        };
        db.upsert_item(&sent).unwrap();
        let day = DAY_MS;
        let now = real_now();
        {
            let conn = db.lock();
            conn.execute("INSERT INTO turns(id,thread_id,status,started_at,ended_at) VALUES('t-old',?1,'completed',?2,?2)", params![A, now - 120 * day]).unwrap();
            conn.execute("INSERT INTO turns(id,thread_id,status,started_at,ended_at) VALUES('t-new',?1,'completed',?2,?2)", params![A, now - 2 * day]).unwrap();
            for (turn, path) in [("t-old", "src/a.rs"), ("t-old", "src/b.rs"), ("t-new", "src/c.rs")] {
                conn.execute(
                    "INSERT INTO turn_changes(turn_id,thread_id,path,status,added,removed,binary,patch,before_text,after_text) VALUES(?1,?2,?3,'modified',3,1,0,'@@ -1 +1 @@','old','new')",
                    params![turn, A, path],
                )
                .unwrap();
            }
        }
        (db, data)
    }

    #[test]
    fn the_dry_run_and_the_cleanup_agree_and_keep_what_is_in_use() {
        let (db, data) = fixture();
        let now = real_now();
        // Nothing but deleted threads' folders by default.
        let plan = db.storage_plan(&data, &StoragePolicy::default(), &[], &[], now);
        assert_eq!(plan.items.iter().map(|i| i.category.as_str()).collect::<Vec<_>>(), vec!["orphans", "orphans"], "{plan:?}");
        assert!(plan.kept.iter().any(|k| k.reason == "not QuantPilot's own folder"));

        let policy = StoragePolicy { unsent_days: Some(30), document_days: Some(30), patch_days: Some(90), ..Default::default() };
        let drafts = vec![data.join("attachments").join(A).join("33333333-draft.txt").to_string_lossy().into_owned()];
        let plan = db.storage_plan(&data, &policy, &drafts, &[B.to_string()], now);
        let targets: Vec<String> = plan.items.iter().map(|i| format!("{} {}", i.category, Path::new(&i.target).file_name().unwrap().to_string_lossy())).collect();
        assert!(targets.contains(&"unsent 22222222-unsent.png".to_string()), "{targets:?}");
        assert!(targets.contains(&"documents old-v1.json".to_string()));
        assert!(targets.contains(&"patches turn t-old".to_string()), "{targets:?}");
        for keep in ["11111111-sent.png", "33333333-draft.txt", "44444444-new.png", "55555555-busy.png", "fresh-v1.json", "turn t-new"] {
            assert!(!targets.iter().any(|t| t.ends_with(keep)), "{keep} must stay: {targets:?}");
        }
        let reasons: Vec<&str> = plan.kept.iter().map(|k| k.reason.as_str()).collect();
        for r in ["in a message or queued message", "in a draft", "newer than 30 days", "its thread is working"] {
            assert!(reasons.contains(&r), "{r}: {reasons:?}");
        }

        // The cleanup carries out exactly the items shown, and again changes nothing.
        let shown: HashSet<String> = plan.items.iter().map(|i| i.target.clone()).collect();
        let result = db.storage_clean(&data, &plan, Some(&shown), "2026-11-02");
        assert_eq!((result.removed as usize, result.failed.len()), (plan.items.len(), 0), "{result:?}");
        assert!(!data.join("attachments").join(GONE).exists() && !data.join("sessions").join(GONE).exists());
        assert!(!data.join("attachments").join(A).join("22222222-unsent.png").exists());
        for keep in ["11111111-sent.png", "33333333-draft.txt", "44444444-new.png"] {
            assert!(data.join("attachments").join(A).join(keep).exists(), "{keep}");
        }
        assert!(data.join("attachments").join(B).join("55555555-busy.png").exists());
        assert!(data.join("attachments").join("not-a-thread").join("keep.txt").exists());
        assert!(!data.join("attachments/.documents/old-v1.txt").exists() && data.join("attachments/.documents/fresh-v1.txt").exists());
        let changes = db.turn_changes("t-old").unwrap();
        assert_eq!(changes.len(), 2, "the file list stays");
        assert_eq!((changes[0].added, changes[0].removed), (3, 1));
        assert!(changes[0].note.as_deref().unwrap().contains("removed by storage cleanup on 2026-11-02"));
        assert_eq!(db.turn_change_saved("t-old", "src/a.rs").unwrap().unwrap(), (String::new(), None, None));
        assert!(db.turn_change_saved("t-new", "src/c.rs").unwrap().unwrap().1.is_some());
        let again = db.storage_clean(&data, &plan, Some(&shown), "2026-11-02");
        assert_eq!(again.failed.len(), 0, "an interrupted or repeated cleanup is harmless");
        assert!(db.storage_plan(&data, &policy, &drafts, &[B.to_string()], now).items.is_empty());

        // An item the user did not see is not removed.
        let plan = db.storage_plan(&data, &StoragePolicy { unsent_days: Some(30), ..Default::default() }, &[], &[], now);
        let none: HashSet<String> = HashSet::new();
        let r = db.storage_clean(&data, &plan, Some(&none), "2026-11-02");
        assert_eq!((r.removed, r.skipped as usize), (0, plan.items.len()));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn targets_outside_the_data_folders_are_never_removed() {
        let (db, data) = fixture();
        let outside = std::env::temp_dir().join(format!("qp-outside-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("precious.txt"), b"x").unwrap();
        let plan = Plan {
            items: vec![
                Item { category: "orphans".into(), target: outside.to_string_lossy().into_owned(), bytes: 1, reason: "test".into(), thread: None },
                Item { category: "unsent".into(), target: data.join("attachments").join("..").join("pilot.db").to_string_lossy().into_owned(), bytes: 1, reason: "test".into(), thread: None },
                Item { category: "orphans".into(), target: data.join("attachments").to_string_lossy().into_owned(), bytes: 1, reason: "test".into(), thread: None },
            ],
            bytes: 3,
            kept: vec![],
        };
        std::fs::write(data.join("pilot.db"), b"db").unwrap();
        let r = db.storage_clean(&data, &plan, None, "2026-11-02");
        assert_eq!((r.removed, r.failed.len()), (0, 3), "{r:?}");
        assert!(outside.join("precious.txt").exists() && data.join("pilot.db").exists() && data.join("attachments").is_dir());
        let inv = db.storage_inventory(&data);
        assert!(inv.iter().any(|c| c.key == "attachments" && c.note == "1 file (1000 B) belongs to deleted threads."), "{inv:?}");
        log_run(&data, "2026-11-02T10:00", &plan, &r);
        assert!(std::fs::read_to_string(data.join("storage-cleanup.log")).unwrap().contains("removed 0"));
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
