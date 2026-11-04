//! What QuantSuite keeps where, and how a move rewrites its paths
//! (PLAN-MIGRATION §1 and §1.1, card Migration 1/4).
//!
//! - [`RULES`] classifies every path under the data root (`paths::root()`):
//!   must move / optional (default on or off) / per machine / never, with the
//!   reason. A test fails when a module or a top-level root has no rule, so a
//!   new module cannot be forgotten by the export (Migration 2/4).
//! - [`dry_run`] walks a data root and reports, per rule, files and bytes,
//!   and every file no rule covers.
//! - [`PathMap`] is the one path-remapping function: old prefix → new prefix,
//!   whatever separators or case a store wrote. [`remap_json`] and
//!   [`remap_sqlite`] apply it to every string in a JSON file or every TEXT
//!   cell of a SQLite database; [`find_paths`] lists what they would touch.
//! - Workspace ids survive a move: the registry keeps its ids and only the
//!   path inside the entity changes ([`crate::workspaces::storage_id`] reads
//!   the id from the registry before it hashes a path).

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

/// What happens to a path in a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Class {
    /// Always moves.
    Must,
    /// Moves when ticked; `on` is the default.
    Optional { on: bool },
    /// Belongs to this machine (environments, models, caches of binaries);
    /// the new machine builds its own.
    PerMachine,
    /// Never moves (old backups, legacy leftovers, the release signing key).
    Never,
}

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Relative to the data root, `/`-separated; a trailing `/` is a folder
    /// and everything in it; `*` matches within one segment.
    pub pattern: &'static str,
    pub class: Class,
    pub reason: &'static str,
}

const MUST: Class = Class::Must;
const ON: Class = Class::Optional { on: true };
const OFF: Class = Class::Optional { on: false };
const MACHINE: Class = Class::PerMachine;
const NEVER: Class = Class::Never;

/// The data root, rule by rule (PLAN-MIGRATION §1). The most specific rule
/// that matches a path wins.
pub const RULES: &[Rule] = &[
    Rule { pattern: "core.db", class: MUST, reason: "suite database: entities, settings, workspace registry, event log" },
    Rule { pattern: "core.db-wal", class: MUST, reason: "core.db's write-ahead log (the export takes a backup instead)" },
    Rule { pattern: "core.db-shm", class: MUST, reason: "core.db's shared memory (the export takes a backup instead)" },
    Rule { pattern: "core.db.bak*", class: NEVER, reason: "old core.db backups" },
    Rule { pattern: "last-launch-version", class: MACHINE, reason: "which version last started on this machine" },
    Rule { pattern: "logs/", class: OFF, reason: "log files" },
    Rule { pattern: "secrets/", class: MUST, reason: "the vault — its secrets are re-encrypted under the bundle passphrase, never copied as they are" },
    Rule { pattern: "signing/", class: NEVER, reason: "the release signing key; opt-in only when releases are built on the new machine" },
    Rule { pattern: "market-data/", class: ON, reason: "shared candle archive; re-downloading takes hours and hits rate limits" },
    Rule { pattern: "workspaces/*/", class: MUST, reason: "per-workspace private data (canvas, memory bench), small" },
    Rule { pattern: "workspaces/*/artifacts/", class: OFF, reason: "agent scratch, mostly re-creatable; ticked per workspace or subfolder" },
    // modules/<id>/ — one per module, the ids of modules/*/module.json.
    Rule { pattern: "modules/algo/", class: MUST, reason: "QuantAlgo: bots, trades, exchanges, strategies, backtests, settings" },
    Rule { pattern: "modules/algo/logs/", class: OFF, reason: "bot logs" },
    Rule { pattern: "modules/canvas/", class: MUST, reason: "QuantCanvas workspaces and browser data" },
    Rule { pattern: "modules/canvas/bin/", class: MACHINE, reason: "downloaded binaries for this machine" },
    Rule { pattern: "modules/code/", class: MUST, reason: "QuantCode settings" },
    Rule { pattern: "modules/console/", class: MUST, reason: "console sessions and settings" },
    Rule { pattern: "modules/console/console.bash", class: MACHINE, reason: "shell integration, written again at start" },
    Rule { pattern: "modules/console/console.fish", class: MACHINE, reason: "shell integration, written again at start" },
    Rule { pattern: "modules/console/console.ps1", class: MACHINE, reason: "shell integration, written again at start" },
    Rule { pattern: "modules/console/console.zsh", class: MACHINE, reason: "shell integration, written again at start" },
    Rule { pattern: "modules/console/zdotdir/", class: MACHINE, reason: "shell integration, written again at start" },
    Rule { pattern: "modules/console/pasted/", class: ON, reason: "images pasted into the console" },
    Rule { pattern: "modules/control/", class: MUST, reason: "QuantControl's database (legacy, still read)" },
    Rule { pattern: "modules/finance/", class: MUST, reason: "Finance" },
    Rule { pattern: "modules/finance/backups/", class: OFF, reason: "Finance backups" },
    Rule { pattern: "modules/flow/", class: MUST, reason: "Flow settings" },
    Rule { pattern: "modules/habit/", class: MUST, reason: "Habits" },
    Rule { pattern: "modules/hud/", class: MUST, reason: "HUD and dictation settings" },
    Rule { pattern: "modules/hud/pyenv/", class: MACHINE, reason: "the dictation Python environment" },
    Rule { pattern: "modules/hud/models/", class: OFF, reason: "Whisper models, downloaded again on first use" },
    Rule { pattern: "modules/hud/*.bak", class: NEVER, reason: "old settings backups" },
    Rule { pattern: "modules/mcp/", class: MUST, reason: "QuantMCP: kanban boards, agent runs, server list" },
    Rule { pattern: "modules/mcp/indexes/", class: OFF, reason: "code indexes, rebuildable" },
    Rule { pattern: "modules/mcp/pyenv/", class: MACHINE, reason: "a Python environment" },
    Rule { pattern: "modules/mcp/pending-worktree-cleanup/", class: MACHINE, reason: "cleanup queue of this machine's worktree folders" },
    Rule { pattern: "modules/mcp/*.legacy*", class: NEVER, reason: "pre-migration leftovers" },
    Rule { pattern: "modules/mcp/*.bak", class: NEVER, reason: "old backups" },
    Rule { pattern: "modules/memory/", class: MUST, reason: "memory vaults" },
    Rule { pattern: "modules/memory/engine/", class: MACHINE, reason: "the local embedding engine" },
    Rule { pattern: "modules/notes/", class: MUST, reason: "Notes" },
    Rule { pattern: "modules/notes/backups/", class: OFF, reason: "Notes backups" },
    Rule { pattern: "modules/notes/exports/", class: OFF, reason: "Notes exports" },
    Rule { pattern: "modules/pilot/", class: MUST, reason: "QuantPilot threads and settings" },
    Rule { pattern: "modules/plan/", class: MUST, reason: "Plan" },
    Rule { pattern: "modules/plan/backups/", class: OFF, reason: "Plan backups" },
    Rule { pattern: "modules/script/", class: MUST, reason: "QuantScript: library database, Collection, research runs" },
    Rule { pattern: "modules/systems/", class: MUST, reason: "QuantSystems: systems, Ranks, research" },
    Rule { pattern: "modules/systems/cache/", class: ON, reason: "candle and ranking cache; rebuilding takes hours" },
    Rule { pattern: "modules/terminal/", class: MUST, reason: "QuantTerminal" },
];

/// Roots outside the data root (PLAN-MIGRATION §1, second table).
pub const EXTERNAL: &[(&str, Class, &str)] = &[
    ("~/.quantmcp/", MUST, "global AgentOS and the QuantMCP registry"),
    ("~/.quantmcp/indexes/", OFF, "code indexes, rebuildable"),
    ("~/.claude/rules/AGENT.md", NEVER, "re-created by the AgentOS import"),
    ("the private indicator library (paths::indicators_dir)", OFF, "opt-in copy or a clone; its path is remapped"),
    ("project folders (workspaces)", OFF, "git; opt-in per workspace with its size"),
    ("~/.quantsuite-dev/", NEVER, "dev builds' data"),
    ("the WebView2 profile", NEVER, "bound to the machine; log in again"),
    ("Python environments, CLI logins (~/.claude, ~/.codex)", NEVER, "set up on the new machine; listed in the import report"),
];

fn glob_segment(pattern: &str, segment: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') => (0..=s.len()).any(|k| go(&p[1..], &s[k..])),
            Some(c) => s.first().is_some_and(|d| d.eq_ignore_ascii_case(c)) && go(&p[1..], &s[1..]),
        }
    }
    go(pattern.as_bytes(), segment.as_bytes())
}

fn matches(rule: &str, rel: &str) -> bool {
    let folder = rule.ends_with('/');
    let rule_segs: Vec<&str> = rule.trim_end_matches('/').split('/').collect();
    let path_segs: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    if path_segs.len() < rule_segs.len() || (!folder && path_segs.len() != rule_segs.len()) {
        return false;
    }
    rule_segs.iter().zip(&path_segs).all(|(r, p)| glob_segment(r, p))
}

/// The most specific rule for a path relative to the data root.
pub fn classify(rel: &str) -> Option<&'static Rule> {
    let rel = rel.replace('\\', "/");
    RULES
        .iter()
        .filter(|r| matches(r.pattern, &rel))
        .max_by_key(|r| (r.pattern.trim_end_matches('/').split('/').count(), !r.pattern.ends_with('/'), r.pattern.len()))
}

#[derive(Debug, Default, Serialize)]
pub struct RuleTotal {
    pub pattern: String,
    pub class: Option<Class>,
    pub reason: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct DryRun {
    pub root: String,
    pub totals: Vec<RuleTotal>,
    /// Files no rule covers (a new module or root) — the export refuses them
    /// until they are classified.
    pub unclassified: Vec<String>,
    pub must_bytes: u64,
    pub optional_on_bytes: u64,
}

/// Walk `root` (read-only, sizes and names only) and total it per rule.
pub fn dry_run(root: &Path) -> DryRun {
    let mut report = DryRun { root: root.to_string_lossy().into_owned(), ..DryRun::default() };
    let mut totals: std::collections::BTreeMap<&'static str, RuleTotal> = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            match classify(&rel) {
                Some(rule) => {
                    let t = totals.entry(rule.pattern).or_insert_with(|| RuleTotal {
                        pattern: rule.pattern.into(),
                        class: Some(rule.class),
                        reason: rule.reason.into(),
                        ..RuleTotal::default()
                    });
                    t.files += 1;
                    t.bytes += meta.len();
                    match rule.class {
                        Class::Must => report.must_bytes += meta.len(),
                        Class::Optional { on: true } => report.optional_on_bytes += meta.len(),
                        _ => {}
                    }
                }
                None => report.unclassified.push(rel),
            }
        }
    }
    report.totals = totals.into_values().collect();
    report.unclassified.sort();
    report
}

// ── paths ───────────────────────────────────────────────────────────────────

/// Old prefix → new prefix, for every stored absolute path.
#[derive(Debug, Clone)]
pub struct PathMap {
    pairs: Vec<(String, String)>,
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

impl PathMap {
    /// The longest old prefix wins, so a workspace inside a remapped parent
    /// can take its own target.
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        let mut pairs: Vec<(String, String)> = pairs
            .iter()
            .filter(|(old, _)| !old.trim().is_empty())
            .map(|(old, new)| (norm(old), new.trim_end_matches(['/', '\\']).to_string()))
            .collect();
        pairs.sort_by_key(|(old, _)| std::cmp::Reverse(old.len()));
        Self { pairs }
    }

    /// Every occurrence of an old prefix inside `text` — whatever separators
    /// (`/`, `\`, or a JSON-escaped `\\`) or case the store wrote, only on a
    /// whole-segment boundary — replaced. The rest of each path takes the new
    /// prefix's separator; inside JSON-escaped text a backslash stays escaped.
    pub fn remap_text(&self, text: &str) -> (String, usize) {
        let chars: Vec<char> = text.chars().collect();
        let is_sep = |c: char| c == '/' || c == '\\';
        let stops = |c: char| matches!(c, '"' | '\'' | ' ' | ';' | ',' | ')' | '\n' | '<' | '>' | '|');
        let mut out = String::with_capacity(text.len());
        let mut count = 0;
        let mut i = 0;
        'scan: while i < chars.len() {
            let start_ok = i == 0 || !chars[i - 1].is_alphanumeric();
            if start_ok {
                for (old, new) in &self.pairs {
                    // Match `old` (normalized, `/`-separated) at i.
                    let mut j = i;
                    let mut escaped = false;
                    let mut ok = true;
                    for oc in old.chars() {
                        if oc == '/' {
                            let run_start = j;
                            while j < chars.len() && is_sep(chars[j]) {
                                j += 1;
                            }
                            if j == run_start {
                                ok = false;
                                break;
                            }
                            escaped |= j - run_start == 2 && chars[run_start] == '\\';
                        } else if j < chars.len() && chars[j].to_lowercase().eq(oc.to_lowercase()) {
                            j += 1;
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if !ok || !(j == chars.len() || is_sep(chars[j]) || stops(chars[j])) {
                        continue;
                    }
                    if j + 1 < chars.len() && chars[j] == '\\' && chars[j + 1] == '\\' {
                        escaped = true;
                    }
                    let sep = if new.contains('\\') { '\\' } else { '/' };
                    let write_sep = |out: &mut String| {
                        out.push(sep);
                        if sep == '\\' && escaped {
                            out.push('\\');
                        }
                    };
                    for c in new.chars() {
                        if c == '\\' {
                            write_sep(&mut out);
                        } else {
                            out.push(c);
                        }
                    }
                    // The tail of this path, separators in the new style.
                    while j < chars.len() && !stops(chars[j]) {
                        if is_sep(chars[j]) {
                            while j < chars.len() && is_sep(chars[j]) {
                                j += 1;
                            }
                            write_sep(&mut out);
                        } else {
                            out.push(chars[j]);
                            j += 1;
                        }
                    }
                    i = j;
                    count += 1;
                    continue 'scan;
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        (out, count)
    }
}

/// Remap every string in a JSON value; returns how many paths changed.
pub fn remap_json(value: &mut Value, map: &PathMap) -> usize {
    match value {
        Value::String(s) => {
            let (new, n) = map.remap_text(s);
            if n > 0 {
                *s = new;
            }
            n
        }
        Value::Array(items) => items.iter_mut().map(|v| remap_json(v, map)).sum(),
        Value::Object(obj) => obj.values_mut().map(|v| remap_json(v, map)).sum(),
        _ => 0,
    }
}

/// Remap a JSON file in place (write-then-rename).
pub fn remap_json_file(path: &Path, map: &PathMap) -> Result<usize, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let n = remap_json(&mut value, map);
    if n > 0 {
        let out = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        crate::paths::write_atomic(path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(n)
}

/// Virtual tables (full-text indexes) and their shadow tables: their content
/// follows the real tables through triggers, and writing them directly
/// would break the index.
pub fn virtual_and_shadow_tables(conn: &Connection) -> std::collections::BTreeSet<String> {
    let virtuals: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%'")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .unwrap_or_default();
    let mut out: std::collections::BTreeSet<String> = virtuals.iter().cloned().collect();
    for v in &virtuals {
        for suffix in ["_data", "_idx", "_docsize", "_config", "_content", "_segments", "_segdir", "_stat"] {
            out.insert(format!("{v}{suffix}"));
        }
    }
    out
}

fn text_columns(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let skip = virtual_and_shadow_tables(conn);
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>())
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|t| !skip.contains(t))
        .collect();
    let mut out = Vec::new();
    for table in tables {
        let cols: Vec<(String, String)> = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\"")))
            .and_then(|mut s| s.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?.collect())
            .map_err(|e| e.to_string())?;
        for (col, ty) in cols {
            let ty = ty.to_uppercase();
            if ty.is_empty() || ty.contains("TEXT") || ty.contains("CHAR") || ty.contains("CLOB") || ty.contains("JSON") {
                out.push((table.clone(), col));
            }
        }
    }
    Ok(out)
}

/// Remap every TEXT cell of a SQLite database (rows found by rowid; tables
/// without one are skipped and reported). Returns cells changed.
pub fn remap_sqlite(conn: &Connection, map: &PathMap) -> Result<usize, String> {
    let mut changed = 0;
    for (table, col) in text_columns(conn)? {
        let q = |s: &str| s.replace('"', "\"\"");
        let select = format!("SELECT rowid, \"{}\" FROM \"{}\" WHERE typeof(\"{}\") = 'text'", q(&col), q(&table), q(&col));
        let Ok(mut stmt) = conn.prepare(&select) else { continue };
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        for (rowid, text) in rows {
            let (new, n) = map.remap_text(&text);
            if n > 0 {
                conn.execute(
                    &format!("UPDATE \"{}\" SET \"{}\" = ?1 WHERE rowid = ?2", q(&table), q(&col)),
                    params![new, rowid],
                )
                .map_err(|e| format!("{table}.{col}: {e}"))?;
                changed += 1;
            }
        }
    }
    Ok(changed)
}

/// One stored absolute path.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub store: String,
    /// `table.column#rowid` or a JSON pointer.
    pub location: String,
    pub value: String,
    /// What the map would make of it (`None`: no prefix applies).
    pub remapped: Option<String>,
}

/// Whether text holds an absolute path: `C:\…`, `C:/…`, `\\?\…`, `/home/…`, `/Users/…`.
pub fn has_absolute_path(text: &str) -> bool {
    let b = text.as_bytes();
    (0..b.len().saturating_sub(2)).any(|i| {
        b[i].is_ascii_alphabetic()
            && b[i + 1] == b':'
            && matches!(b[i + 2], b'\\' | b'/')
            && (i == 0 || !b[i - 1].is_ascii_alphanumeric())
    }) || text.contains(r"\\?\")
        || text.contains("/home/")
        || text.contains("/Users/")
}

fn clip(text: &str) -> String {
    if text.chars().count() > 200 {
        format!("{}…", text.chars().take(200).collect::<String>())
    } else {
        text.to_string()
    }
}

fn json_findings(store: &str, pointer: &str, value: &Value, map: &PathMap, out: &mut Vec<Finding>) {
    match value {
        Value::String(s) if has_absolute_path(s) => {
            let (new, n) = map.remap_text(s);
            out.push(Finding { store: store.into(), location: pointer.into(), value: clip(s), remapped: (n > 0).then(|| clip(&new)) });
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                json_findings(store, &format!("{pointer}/{i}"), v, map, out);
            }
        }
        Value::Object(obj) => {
            for (k, v) in obj {
                json_findings(store, &format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")), v, map, out);
            }
        }
        _ => {}
    }
}

/// Every absolute path in the JSON files and SQLite databases under `root`
/// (read-only), with what `map` would make of it.
pub fn find_paths(root: &Path, map: &PathMap) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if ext == "json" {
                if let Ok(value) = std::fs::read_to_string(&path).map_err(|_| ()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|_| ())) {
                    json_findings(&rel, "", &value, map, &mut out);
                }
            } else if matches!(ext.as_str(), "db" | "sqlite" | "sqlite3") {
                let Ok(conn) = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
                let Ok(cols) = text_columns(&conn) else { continue };
                for (table, col) in cols {
                    let q = |s: &str| s.replace('"', "\"\"");
                    let sql = format!("SELECT rowid, \"{}\" FROM \"{}\" WHERE typeof(\"{}\") = 'text'", q(&col), q(&table), q(&col));
                    let Ok(mut stmt) = conn.prepare(&sql) else { continue };
                    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) else { continue };
                    for (rowid, text) in rows.flatten() {
                        if has_absolute_path(&text) {
                            let (new, n) = map.remap_text(&text);
                            out.push(Finding {
                                store: rel.clone(),
                                location: format!("{table}.{col}#{rowid}"),
                                value: clip(&text),
                                remapped: (n > 0).then(|| clip(&new)),
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// The data root's top-level names the suite creates (`paths.rs`).
pub const TOP_LEVEL: &[&str] =
    &["core.db", "logs/", "secrets/", "signing/", "market-data/", "workspaces/x/", "modules/", "last-launch-version"];

/// A report directory under `root` for a dry run's JSON.
pub fn report_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}-{}.json", chrono::Utc::now().format("%Y%m%d-%H%M%S")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_module_and_root_has_a_rule() {
        let modules = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../modules");
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&modules).unwrap().flatten() {
            let manifest = entry.path().join("module.json");
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                let id = serde_json::from_str::<Value>(&text).unwrap()["id"].as_str().unwrap().to_string();
                ids.push(id);
            }
        }
        assert!(ids.len() >= 15, "{ids:?}");
        for id in &ids {
            let rule = classify(&format!("modules/{id}/settings.json"));
            assert!(rule.is_some_and(|r| r.pattern == format!("modules/{id}/")), "module {id} has no rule in RULES");
        }
        for top in TOP_LEVEL {
            let probe = if top.ends_with('/') { format!("{top}file") } else { top.to_string() };
            if *top == "modules/" {
                continue; // covered per module above
            }
            assert!(classify(&probe).is_some(), "top-level {top} has no rule");
        }
    }

    #[test]
    fn the_most_specific_rule_wins() {
        let class = |p: &str| classify(p).map(|r| r.class);
        assert_eq!(class("core.db"), Some(Class::Must));
        assert_eq!(class("core.db.bak-rename"), Some(Class::Never));
        assert_eq!(class("modules/algo/quantalgo.db"), Some(Class::Must));
        assert_eq!(class("modules/algo/logs/bot.log"), Some(Class::Optional { on: false }));
        assert_eq!(class("modules/hud/pyenv/Lib/site.py"), Some(Class::PerMachine));
        assert_eq!(class("modules/hud/config.json.bak"), Some(Class::Never));
        assert_eq!(class("modules/mcp/kanban.legacy.db"), Some(Class::Never));
        assert_eq!(class("modules/mcp/kanban.db"), Some(Class::Must));
        assert_eq!(class("modules/systems/cache/x.sqlite"), Some(Class::Optional { on: true }));
        assert_eq!(class("workspaces/78xw9p/canvas/canvas.json"), Some(Class::Must));
        assert_eq!(class("workspaces/78xw9p/artifacts/run/a.png"), Some(Class::Optional { on: false }));
        assert_eq!(class("modules/newthing/x"), None, "a new module is unclassified until it gets a rule");
        assert_eq!(class("something-new.db"), None);
    }

    #[test]
    fn a_dry_run_totals_and_names_what_has_no_rule() {
        let root = std::env::temp_dir().join(format!("qs-inventory-{}", uuid::Uuid::new_v4()));
        for (rel, size) in [("core.db", 10), ("modules/algo/quantalgo.db", 20), ("modules/algo/logs/a.log", 5), ("newdir/x.bin", 7)] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![0u8; size]).unwrap();
        }
        let report = dry_run(&root);
        assert_eq!(report.must_bytes, 30);
        assert_eq!(report.unclassified, vec!["newdir/x.bin"]);
        assert!(report.totals.iter().any(|t| t.pattern == "modules/algo/logs/" && t.bytes == 5));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn paths_remap_whatever_the_separators_and_case() {
        let map = PathMap::new(&[(r"C:\Projects", "D:/work"), (r"C:\Users\root\.quantsuite", "/srv/quantsuite")]);
        assert_eq!(map.remap_text(r"C:\Projects\QuantSuite").0, "D:/work/QuantSuite");
        assert_eq!(map.remap_text("c:/projects/QuantSuite/.qs-worktrees/ab").0, "D:/work/QuantSuite/.qs-worktrees/ab");
        assert_eq!(map.remap_text(r"C:\Users\root\.quantsuite\modules\algo").0, "/srv/quantsuite/modules/algo");
        // Embedded, several, and only on segment boundaries.
        let (text, n) = map.remap_text(r#"{"cwd":"C:\\Projects\\A","x":"see C:/Projects/B; C:/ProjectsOld/C"}"#);
        assert_eq!(n, 2, "{text}");
        assert!(text.contains("D:/work/B") && text.contains("C:/ProjectsOld/C"), "{text}");
        let map = PathMap::new(&[("C:/Projects/Quant", "E:/Q")]);
        assert_eq!(map.remap_text("C:/Projects/QuantSuite").1, 0, "a longer folder name is a different folder");
        assert!(has_absolute_path(r"C:\x") && has_absolute_path("cwd=C:/x") && !has_absolute_path("12:30/x"));
    }

    #[test]
    fn every_audited_store_kind_remaps_and_workspace_ids_survive() {
        let map = PathMap::new(&[("C:/Projects", "D:/work")]);
        // JSON stores: canvas/notes workspaces.json, hud/algo config.json, canvas.json.
        let mut canvas = serde_json::json!({"workspaces": [{"id": "core:workspace:78xw9p", "path": "C:\\Projects\\QuantSuite"}], "nested": {"files": ["C:/Projects/QuantSuite/README.md"]}});
        assert_eq!(remap_json(&mut canvas, &map), 2);
        assert_eq!(canvas["workspaces"][0]["id"], "core:workspace:78xw9p", "ids are never rewritten");
        assert_eq!(canvas["workspaces"][0]["path"], "D:/work/QuantSuite");

        // SQLite: the core.db registry, a kanban worktree path, a thread folder in JSON text.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        let id = crate::workspaces::workspace_id_for("C:/Projects/QuantSuite");
        crate::db::upsert_entity(
            &conn,
            &crate::db::Entity {
                id: id.clone(),
                module: "core".into(),
                kind: "workspace".into(),
                title: "QuantSuite".into(),
                subtitle: Some("C:\\Projects\\QuantSuite".into()),
                route: "/".into(),
                icon: None,
                updated_at: 0,
                payload: Some(serde_json::json!({"path": "C:\\Projects\\QuantSuite"})),
            },
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE cards (id TEXT PRIMARY KEY, worktree_path TEXT, meta TEXT, n INTEGER);
             INSERT INTO cards VALUES ('c1', 'C:\\Projects\\QuantSuite\\.qs-worktrees\\c1', '{\"cwd\":\"C:/Projects/QuantGames\"}', 3);",
        )
        .unwrap();
        let changed = remap_sqlite(&conn, &map).unwrap();
        assert!(changed >= 4, "{changed}");
        let (wt, meta): (String, String) = conn.query_row("SELECT worktree_path, meta FROM cards", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(wt, "D:/work/QuantSuite/.qs-worktrees/c1");
        assert!(meta.contains("D:/work/QuantGames"));
        // The registry keeps its id: lookups by the new path find it, and the
        // per-workspace data folder keeps its name.
        let ws = crate::workspaces::list(&conn);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].id, id);
        assert_eq!(ws[0].path, "D:/work/QuantSuite");
        assert_eq!(crate::workspaces::storage_id(Some(&conn), "D:\\work\\QuantSuite"), id);
        assert_ne!(crate::workspaces::workspace_id_for("D:/work/QuantSuite"), id, "a hash of the new path would fork the data");
        assert_eq!(crate::workspaces::containing(&conn, "D:/work/QuantSuite/.qs-worktrees/x").map(|w| w.id), Some(id));
        // find_paths sees what is left (nothing under the old prefix).
        assert!(map.remap_text(&wt).1 == 0);
    }
}

/// The acceptance dry run (PLAN-MIGRATION card 1): the classification of a
/// data root (read-only, names and sizes) and every absolute path in a COPY
/// of its stores with what a sample map makes of it. Reports go to
/// `QS_DRYRUN_OUT`. `cargo test -p tauri-plugin-qs --lib dry_run_report -- --ignored`
#[cfg(test)]
mod dry_run_report {
    use super::*;

    #[test]
    #[ignore = "reads a real data root and a copy of its stores"]
    fn dry_run_report() {
        let root = PathBuf::from(std::env::var("QS_DRYRUN_ROOT").expect("QS_DRYRUN_ROOT"));
        let copy = PathBuf::from(std::env::var("QS_DRYRUN_COPY").expect("QS_DRYRUN_COPY"));
        let out = PathBuf::from(std::env::var("QS_DRYRUN_OUT").expect("QS_DRYRUN_OUT"));
        let report = dry_run(&root);
        std::fs::write(report_path(&out, "classification"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        let map = PathMap::new(&[
            (r"C:\Projects", "/srv/projects"),
            (r"C:\Users\root\.quantsuite", "/srv/quantsuite"),
            (r"C:\Users\root\.quantmcp", "/srv/quantmcp"),
            (r"C:\Users\root", "/home/quant"),
        ]);
        let findings = find_paths(&copy, &map);
        std::fs::write(report_path(&out, "paths"), serde_json::to_vec_pretty(&findings).unwrap()).unwrap();
        let unmapped: Vec<&Finding> = findings.iter().filter(|f| f.remapped.is_none()).collect();
        let mut stores: Vec<&str> = findings.iter().map(|f| f.store.as_str()).collect();
        stores.sort();
        stores.dedup();
        println!(
            "classified: must {:.1} GB, optional-on {:.1} GB, {} rule totals, {} unclassified files",
            report.must_bytes as f64 / 1e9,
            report.optional_on_bytes as f64 / 1e9,
            report.totals.len(),
            report.unclassified.len()
        );
        for u in report.unclassified.iter().take(20) {
            println!("  unclassified: {u}");
        }
        println!("paths: {} findings in {} stores, {} not covered by the sample map", findings.len(), stores.len(), unmapped.len());
        for s in &stores {
            println!("  store: {s} ({})", findings.iter().filter(|f| f.store == *s).count());
        }
        for f in unmapped.iter().take(15) {
            println!("  not mapped: {} {} {}", f.store, f.location, f.value);
        }
    }
}
