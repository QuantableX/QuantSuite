//! `memory.db` — a rebuildable index over the vault, never the truth.
//!
//! Files are the source of record (vault.rs); this database exists so that
//! search, backlinks, the graph and the dashboard do not have to re-read the
//! whole vault on every question. Deleting it loses nothing: the next scan
//! rebuilds it from the files.
//!
//! Search is FTS5 with external content over `docs`, the same trigger
//! pattern core.db uses for `entities_fts` (crates/qs-core/src/db.rs).

use crate::vault;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// ─── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMeta {
    pub id: String,
    /// `'general'` or a workspace entity id (`core:workspace:<b36>`).
    pub scope: String,
    pub slug: String,
    pub rel_path: String,
    pub title: String,
    /// The frontmatter `type` — user | project | reference | decision | …
    pub kind: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub word_count: i64,
    pub outgoing_links: i64,
    pub incoming_links: i64,
    /// False for files the vault only adopted (no frontmatter id yet).
    pub managed: bool,
    pub quality: crate::quality::Quality,
    pub revision: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub scope: String,
    pub title: String,
    pub slug: String,
    pub snippet: String,
    /// bm25 — smaller is better.
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingLink {
    pub target: String,
    pub target_id: Option<String>,
    pub resolved_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedLink {
    pub scope: String,
    pub target: String,
    pub count: i64,
    pub sources: Vec<Backlink>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub scope: String,
    pub title: String,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub incoming: i64,
    pub outgoing: i64,
    /// True for a ghost node — a link target no memory answers to (yet).
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub shared_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub memories: i64,
    pub resolved_links: i64,
    pub unresolved_links: i64,
    pub orphans: i64,
    pub tags: i64,
    pub words: i64,
    pub last_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub total: usize,
    /// Ids whose rows changed — the caller mirrors these into core.db.
    pub changed_ids: Vec<String>,
    pub removed_ids: Vec<String>,
}

/// One vault file as the scanner sees it.
pub struct FileRecord<'a> {
    pub scope: &'a str,
    pub rel_path: &'a str,
    pub text: &'a str,
    pub mtime_ms: i64,
}

// ─── Schema ───────────────────────────────────────────────────────────────

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| format!("Read user_version: {e}"))?;
    if version == 2 {
        conn.execute_batch("BEGIN IMMEDIATE; ALTER TABLE memories ADD COLUMN quality_json TEXT NOT NULL DEFAULT '{}';
            UPDATE memories SET mtime_ms = -1, content_hash = ''; PRAGMA user_version = 3; COMMIT;")
            .map_err(|e| format!("Migrate provenance index: {e}"))?;
    }
    if version >= 2 {
        return crate::retrieval::init(conn);
    }
    if version == 1 {
        // v1 had no scope column. The index is a cache — drop and rebuild is
        // the whole migration; the vault files carry everything. Markers in
        // `settings` (seed, legacy import) survive.
        conn.execute_batch(
            "DROP TABLE IF EXISTS links;
             DROP TABLE IF EXISTS memories_fts;
             DROP TABLE IF EXISTS docs;
             DROP TABLE IF EXISTS memories;",
        )
        .map_err(|e| format!("Drop v1 index: {e}"))?;
    }
    conn.execute_batch(
        "
        -- One row per vault file. Everything here is derived from the file
        -- and can be rebuilt; `managed` marks files that carry a frontmatter
        -- id (written through QuantMemory) versus merely adopted ones.
        -- `scope` is 'general' or a workspace entity id — one index, many
        -- vaults, so the gigabrain view and cross-scope search are one query.
        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL DEFAULT 'general',
            slug TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            title TEXT NOT NULL,
            title_lc TEXT NOT NULL,
            kind TEXT,
            author TEXT,
            tags_json TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            word_count INTEGER NOT NULL DEFAULT 0,
            content_hash TEXT NOT NULL DEFAULT '',
            mtime_ms INTEGER NOT NULL DEFAULT 0,
            managed INTEGER NOT NULL DEFAULT 1,
            quality_json TEXT NOT NULL DEFAULT '{}'
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_scope_rel ON memories(scope, rel_path);
        CREATE INDEX IF NOT EXISTS idx_memories_title ON memories(title_lc);
        CREATE INDEX IF NOT EXISTS idx_memories_slug ON memories(slug);
        CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at DESC);

        -- Every [[wikilink]], one row per occurrence. `target_id` stays NULL
        -- while no memory answers to the target — the unresolved-links view
        -- and the graph's ghost nodes come straight from that. `scope` is the
        -- source memory's scope, denormalized so resolution (own scope, then
        -- general) stays one UPDATE.
        CREATE TABLE IF NOT EXISTS links (
            source_id TEXT NOT NULL,
            scope TEXT NOT NULL DEFAULT 'general',
            target TEXT NOT NULL,
            target_lc TEXT NOT NULL,
            target_id TEXT,
            alias TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_id);
        CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_id);
        CREATE INDEX IF NOT EXISTS idx_links_target_lc ON links(target_lc);

        -- Search substrate: the plain body, shadowed into FTS5 by triggers —
        -- the same external-content pattern core.db uses for entities_fts.
        CREATE TABLE IF NOT EXISTS docs (
            memory_id TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            tags TEXT NOT NULL DEFAULT ''
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            title, body, tags,
            content='docs', content_rowid='rowid'
        );
        CREATE TRIGGER IF NOT EXISTS docs_ai AFTER INSERT ON docs BEGIN
            INSERT INTO memories_fts(rowid, title, body, tags)
            VALUES (new.rowid, new.title, new.body, new.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_ad AFTER DELETE ON docs BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, title, body, tags)
            VALUES ('delete', old.rowid, old.title, old.body, old.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_au AFTER UPDATE ON docs BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, title, body, tags)
            VALUES ('delete', old.rowid, old.title, old.body, old.tags);
            INSERT INTO memories_fts(rowid, title, body, tags)
            VALUES (new.rowid, new.title, new.body, new.tags);
        END;

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL
        );

        PRAGMA user_version = 3;
        ",
    )
    .map_err(|e| format!("Init memory schema: {e}"))?;
    crate::retrieval::init(conn)
}

// ─── Markers (one-time migrations) ────────────────────────────────────────

pub fn marker_set(conn: &Connection, key: &str) -> Result<bool, String> {
    let row: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Read marker {key}: {e}"))?;
    Ok(row.is_some())
}

pub fn set_marker(conn: &Connection, key: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, 'true')
         ON CONFLICT(key) DO UPDATE SET value_json = 'true'",
        params![key],
    )
    .map_err(|e| format!("Write marker {key}: {e}"))?;
    Ok(())
}

// ─── Row mapping ──────────────────────────────────────────────────────────

/// Shared by every meta query so the link counters can never disagree with
/// the rows they count.
const META_COLS: &str = "m.id, m.scope, m.slug, m.rel_path, m.title, m.kind, m.author, m.tags_json, \
     m.created_at, m.updated_at, m.word_count, m.managed, \
     (SELECT COUNT(*) FROM links l WHERE l.source_id = m.id AND l.target_id IS NOT NULL), \
     (SELECT COUNT(*) FROM links l WHERE l.target_id = m.id), m.quality_json, m.content_hash, \
     COALESCE((SELECT SUBSTR(d.body, 1, 600) FROM docs d WHERE d.memory_id = m.id), '')";

/// `NULL` scope parameter = every scope — the gigabrain filter, shared by all
/// scope-aware queries so 'all' can never mean different things.
const SCOPE_FILTER: &str = "(?1 IS NULL OR m.scope = ?1)";

fn read_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryMeta> {
    let tags_json: String = row.get(7)?;
    Ok(MemoryMeta {
        id: row.get(0)?,
        scope: row.get(1)?,
        slug: row.get(2)?,
        rel_path: row.get(3)?,
        title: row.get(4)?,
        kind: row.get(5)?,
        author: row.get(6)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        word_count: row.get(10)?,
        managed: row.get::<_, i64>(11)? != 0,
        outgoing_links: row.get(12)?,
        incoming_links: row.get(13)?,
        quality: crate::quality::from_value(serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(14)?).ok().as_ref(), &row.get::<_, String>(0)?),
        revision: row.get(15)?,
        preview: row.get(16)?,
    })
}

// ─── Upsert / remove ──────────────────────────────────────────────────────

fn file_stem(rel_path: &str) -> String {
    let name = rel_path.rsplit(['/', '\\']).next().unwrap_or(rel_path);
    name.strip_suffix(".md").or_else(|| name.strip_suffix(".MD")).unwrap_or(name).to_string()
}

fn mtime_iso(mtime_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(mtime_ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

/// Index one file. Returns the id the file now lives under.
///
/// The id comes from the frontmatter when there is one; a file without it is
/// adopted under a path-derived id (vault::external_id) — reads never mutate.
/// A frontmatter id that already belongs to a *different* file (a duplicate
/// copied in the file manager) falls back to the path-derived id too, so two
/// files never fight over one identity.
pub fn upsert_from_file(conn: &Connection, file: &FileRecord<'_>) -> Result<String, String> {
    let (yaml, body) = vault::split_frontmatter(file.text);
    let fm = yaml.and_then(vault::parse_frontmatter);

    let fm_id = fm
        .as_ref()
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty());
    let managed = fm_id.is_some();
    // The path-derived id is scope-qualified: the same rel_path may exist in
    // two vaults and must be two identities.
    let derived_id = || vault::external_id(&format!("{}::{}", file.scope, file.rel_path));
    let mut id = fm_id.unwrap_or_else(derived_id);

    let claimed_by: Option<(String, String)> = conn
        .query_row("SELECT scope, rel_path FROM memories WHERE id = ?1", params![id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .map_err(|e| format!("Check id claim: {e}"))?;
    if claimed_by.is_some_and(|(s, p)| s != file.scope || p != file.rel_path) {
        id = derived_id();
    }

    // A file that changed identity leaves its old row behind — clean it up
    // (links and doc included) before inserting under the new id.
    let previous: Option<(String, String)> = conn
        .query_row(
            "SELECT id, created_at FROM memories WHERE scope = ?1 AND rel_path = ?2",
            params![file.scope, file.rel_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("Check previous row: {e}"))?;
    if let Some((old_id, _)) = &previous {
        if *old_id != id {
            remove_by_id(conn, old_id)?;
        }
    }

    let title = vault::first_h1(body).unwrap_or_else(|| file_stem(file.rel_path));
    let slug = file_stem(file.rel_path).to_lowercase();
    let kind = fm.as_ref().and_then(|m| m.get("type")).and_then(|v| v.as_str()).map(str::to_string);
    let author =
        fm.as_ref().and_then(|m| m.get("author")).and_then(|v| v.as_str()).map(str::to_string);
    let tags = fm.as_ref().and_then(|m| m.get("tags")).map(vault::tags_from_value).unwrap_or_default();
    let created_at = fm
        .as_ref()
        .and_then(|m| m.get("created"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| previous.as_ref().map(|(_, created)| created.clone()))
        .unwrap_or_else(|| mtime_iso(file.mtime_ms));
    let updated_at = fm
        .as_ref()
        .and_then(|m| m.get("updated"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| mtime_iso(file.mtime_ms));

    conn.execute(
        "INSERT INTO memories (id, scope, slug, rel_path, title, title_lc, kind, author, tags_json,
                               created_at, updated_at, word_count, content_hash, mtime_ms, managed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
           scope=excluded.scope, slug=excluded.slug, rel_path=excluded.rel_path,
           title=excluded.title, title_lc=excluded.title_lc, kind=excluded.kind,
           author=excluded.author, tags_json=excluded.tags_json,
           created_at=excluded.created_at, updated_at=excluded.updated_at,
           word_count=excluded.word_count, content_hash=excluded.content_hash,
           mtime_ms=excluded.mtime_ms, managed=excluded.managed",
        params![
            id,
            file.scope,
            slug,
            file.rel_path,
            title,
            title.to_lowercase(),
            kind,
            author,
            serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into()),
            created_at,
            updated_at,
            vault::word_count(body),
            format!("{:016x}", vault::fnv1a64(file.text)),
            file.mtime_ms,
            managed as i64,
        ],
    )
    .map_err(|e| format!("Upsert memory row: {e}"))?;

    let quality = crate::quality::for_body(fm.as_ref().and_then(|m| m.get("quality")), &id, body);
    conn.execute("UPDATE memories SET quality_json = ?2 WHERE id = ?1",
        params![id, serde_json::to_string(&quality).map_err(|e| e.to_string())?])
        .map_err(|e| format!("Index provenance: {e}"))?;

    conn.execute(
        "INSERT INTO docs (memory_id, title, body, tags) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(memory_id) DO UPDATE SET
           title=excluded.title, body=excluded.body, tags=excluded.tags",
        params![id, title, body, tags.join(" ")],
    )
    .map_err(|e| format!("Upsert search doc: {e}"))?;

    conn.execute("DELETE FROM links WHERE source_id = ?1", params![id])
        .map_err(|e| format!("Clear links: {e}"))?;
    for link in vault::extract_links(body) {
        conn.execute(
            "INSERT INTO links (source_id, scope, target, target_lc, target_id, alias)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![id, file.scope, link.target, link.target_lc, link.alias],
        )
        .map_err(|e| format!("Insert link: {e}"))?;
    }

    Ok(id)
}

pub fn remove_by_id(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM links WHERE source_id = ?1", params![id])
        .map_err(|e| format!("Remove links: {e}"))?;
    conn.execute("UPDATE links SET target_id = NULL WHERE target_id = ?1", params![id])
        .map_err(|e| format!("Unresolve inbound links: {e}"))?;
    conn.execute("DELETE FROM docs WHERE memory_id = ?1", params![id])
        .map_err(|e| format!("Remove search doc: {e}"))?;
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
        .map_err(|e| format!("Remove memory row: {e}"))?;
    Ok(())
}

/// Re-point every link at whatever memory now answers to its target. Links
/// resolve by title first, filename-slug second, **in their own scope first
/// and the general vault second** (user decision 2026-08-31): a project
/// memory may link to shared knowledge, but never into another project.
/// Ambiguity goes to the most recently updated memory. Cheap enough to run
/// after every batch of writes.
pub fn resolve_links(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        UPDATE links SET target_id = NULL
         WHERE target_id IS NOT NULL
           AND target_id NOT IN (SELECT id FROM memories);
        UPDATE links SET target_id = (
            SELECT m.id FROM memories m
             WHERE (m.title_lc = links.target_lc OR m.slug = links.target_lc)
               AND (m.scope = links.scope OR m.scope = 'general')
             -- The WHERE already restricts to the link's own scope or
             -- general, so ranking general last IS 'local first'. (The outer
             -- reference cannot appear here: SQLite resolves links.* in the
             -- subquery's WHERE, but not in its ORDER BY.)
             ORDER BY CASE WHEN m.scope = 'general' THEN 1 ELSE 0 END,
                      m.updated_at DESC
             LIMIT 1
        );
        ",
    )
    .map_err(|e| format!("Resolve links: {e}"))
}

// ─── Vault scan ───────────────────────────────────────────────────────────

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        // Hidden entries cover `.trash`, `.obsidian`, `.git` in one rule.
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            out.push(path);
        }
    }
}

pub fn rel_path_of(vault_root: &Path, path: &Path) -> String {
    path.strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn mtime_ms_of(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Reconcile the index with one scope's vault directory: new and changed
/// files are re-read (mtime first, content hash second), vanished files drop
/// out. Other scopes' rows are never touched.
pub fn scan_vault(conn: &Connection, vault_root: &Path, scope: &str) -> Result<ScanReport, String> {
    let mut files = Vec::new();
    collect_files(vault_root, &mut files);

    let mut known: HashMap<String, (String, i64, String)> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT rel_path, id, mtime_ms, content_hash FROM memories WHERE scope = ?1")
            .map_err(|e| format!("Prepare known rows: {e}"))?;
        let rows = stmt
            .query_map(params![scope], |r| {
                Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
            })
            .map_err(|e| format!("Query known rows: {e}"))?;
        for row in rows {
            let (rel, rest) = row.map_err(|e| format!("Read known row: {e}"))?;
            known.insert(rel, rest);
        }
    }

    let mut report = ScanReport { total: files.len(), ..Default::default() };
    let mut seen: HashSet<String> = HashSet::new();

    for path in files {
        let rel = rel_path_of(vault_root, &path);
        seen.insert(rel.clone());
        let mtime_ms = mtime_ms_of(&path);
        let existing = known.get(&rel);
        if existing.is_some_and(|(_, known_mtime, _)| *known_mtime == mtime_ms) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let hash = format!("{:016x}", vault::fnv1a64(&text));
        if let Some((id, _, known_hash)) = existing {
            if *known_hash == hash {
                conn.execute(
                    "UPDATE memories SET mtime_ms = ?1 WHERE id = ?2",
                    params![mtime_ms, id],
                )
                .map_err(|e| format!("Touch mtime: {e}"))?;
                continue;
            }
        }
        let was_known = existing.is_some();
        let id =
            upsert_from_file(conn, &FileRecord { scope, rel_path: &rel, text: &text, mtime_ms })?;
        if was_known {
            report.updated += 1;
        } else {
            report.added += 1;
        }
        report.changed_ids.push(id);
    }

    for (rel, (id, _, _)) in &known {
        if !seen.contains(rel) {
            remove_by_id(conn, id)?;
            report.removed += 1;
            report.removed_ids.push(id.clone());
        }
    }

    if report.added + report.updated + report.removed > 0 {
        resolve_links(conn)?;
    }
    Ok(report)
}

// ─── Queries ──────────────────────────────────────────────────────────────

/// Resolve an identifier to one memory. Ids win outright (globally unique);
/// titles and slugs prefer `scope_hint`, then the general vault, then
/// anything — so an agent working in a project finds its own note first but
/// can still reach every other scope by name (the gigabrain rule).
pub fn get_by_identifier(
    conn: &Connection,
    ident: &str,
    scope_hint: Option<&str>,
) -> Result<Option<MemoryMeta>, String> {
    let lc = ident.trim().to_lowercase();
    let slug_lc = lc.strip_suffix(".md").unwrap_or(&lc).to_string();
    let sql = format!(
        "SELECT {META_COLS} FROM memories m
          WHERE m.id = ?3 OR m.title_lc = ?1 OR m.slug = ?2 OR LOWER(m.rel_path) = ?4
          ORDER BY
            CASE WHEN m.id = ?3 THEN 0 ELSE 1 END,
            CASE WHEN ?5 IS NOT NULL AND m.scope = ?5 THEN 0
                 WHEN m.scope = 'general' THEN 1
                 ELSE 2 END,
            CASE WHEN m.title_lc = ?1 THEN 0 WHEN m.slug = ?2 THEN 1 ELSE 2 END,
            m.updated_at DESC
          LIMIT 1"
    );
    conn.query_row(
        &sql,
        params![lc, slug_lc, ident.trim(), lc.replace('\\', "/"), scope_hint],
        read_meta,
    )
    .optional()
    .map_err(|e| format!("Find memory: {e}"))
}

pub fn list(
    conn: &Connection,
    scope: Option<&str>,
    kind: Option<&str>,
    tag: Option<&str>,
    limit: i64,
) -> Result<Vec<MemoryMeta>, String> {
    let tag_like = tag.map(|t| format!("%\"{}\"%", t.trim()));
    let sql = format!(
        "SELECT {META_COLS} FROM memories m
          WHERE {SCOPE_FILTER} AND (?2 IS NULL OR m.kind = ?2) AND (?3 IS NULL OR m.tags_json LIKE ?3)
          ORDER BY m.updated_at DESC LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare list: {e}"))?;
    let rows = stmt
        .query_map(params![scope, kind, tag_like, limit], read_meta)
        .map_err(|e| format!("Query list: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read list: {e}"))?;
    Ok(rows)
}

/// Characters FTS5 would read as syntax are stripped, every remaining word
/// becomes a quoted prefix term (the core.db approach). `all` requires every
/// term, `any` settles for one — precision versus recall, the caller picks.
fn fts_query(query: &str, mode: &str) -> Option<String> {
    let cleaned: String = query
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();
    let terms: Vec<String> =
        cleaned.split_whitespace().map(|t| format!("\"{t}\"*")).collect();
    if terms.is_empty() {
        return None;
    }
    let joiner = if mode == "any" { " OR " } else { " " };
    Some(terms.join(joiner))
}

pub fn search(
    conn: &Connection,
    scope: Option<&str>,
    query: &str,
    mode: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, String> {
    let Some(match_expr) = fts_query(query, mode) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.scope, m.title, m.slug,
                    snippet(memories_fts, 1, '', '', ' … ', 18),
                    bm25(memories_fts, 5.0, 1.0, 3.0)
               FROM memories_fts
               JOIN docs d ON d.rowid = memories_fts.rowid
               JOIN memories m ON m.id = d.memory_id
              WHERE memories_fts MATCH ?1 AND (?2 IS NULL OR m.scope = ?2)
              ORDER BY bm25(memories_fts, 5.0, 1.0, 3.0)
              LIMIT ?3",
        )
        .map_err(|e| format!("Prepare search: {e}"))?;
    let rows = stmt
        .query_map(params![match_expr, scope, limit], |r| {
            Ok(SearchHit {
                id: r.get(0)?,
                scope: r.get(1)?,
                title: r.get(2)?,
                slug: r.get(3)?,
                snippet: r.get(4)?,
                score: r.get(5)?,
            })
        })
        .map_err(|e| format!("Query search: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read search: {e}"))?;
    Ok(rows)
}

pub fn body_of(conn: &Connection, id: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT body FROM docs WHERE memory_id = ?1", params![id], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Read doc body: {e}"))
}

pub fn outgoing_links(conn: &Connection, id: &str) -> Result<Vec<OutgoingLink>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.target, l.target_id, m.title
               FROM links l LEFT JOIN memories m ON m.id = l.target_id
              WHERE l.source_id = ?1",
        )
        .map_err(|e| format!("Prepare outgoing: {e}"))?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(OutgoingLink { target: r.get(0)?, target_id: r.get(1)?, resolved_title: r.get(2)? })
        })
        .map_err(|e| format!("Query outgoing: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read outgoing: {e}"))?;
    Ok(rows)
}

pub fn backlinks(conn: &Connection, id: &str) -> Result<Vec<Backlink>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.title, m.slug, COUNT(*)
               FROM links l JOIN memories m ON m.id = l.source_id
              WHERE l.target_id = ?1
              GROUP BY m.id ORDER BY m.updated_at DESC",
        )
        .map_err(|e| format!("Prepare backlinks: {e}"))?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(Backlink { id: r.get(0)?, title: r.get(1)?, slug: r.get(2)?, count: r.get(3)? })
        })
        .map_err(|e| format!("Query backlinks: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read backlinks: {e}"))?;
    Ok(rows)
}

pub fn orphans(conn: &Connection, scope: Option<&str>) -> Result<Vec<MemoryMeta>, String> {
    let sql = format!(
        "SELECT {META_COLS} FROM memories m
          WHERE {SCOPE_FILTER}
            AND NOT EXISTS (SELECT 1 FROM links l WHERE l.source_id = m.id AND l.target_id IS NOT NULL)
            AND NOT EXISTS (SELECT 1 FROM links l WHERE l.target_id = m.id)
          ORDER BY m.updated_at DESC"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare orphans: {e}"))?;
    let rows = stmt
        .query_map(params![scope], read_meta)
        .map_err(|e| format!("Query orphans: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read orphans: {e}"))?;
    Ok(rows)
}

pub fn unresolved(conn: &Connection, scope: Option<&str>) -> Result<Vec<UnresolvedLink>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.scope, l.target_lc, l.target, m.id, m.title, m.slug
               FROM links l JOIN memories m ON m.id = l.source_id
              WHERE l.target_id IS NULL AND (?1 IS NULL OR l.scope = ?1)
              ORDER BY l.target_lc, m.updated_at DESC",
        )
        .map_err(|e| format!("Prepare unresolved: {e}"))?;
    let rows = stmt
        .query_map(params![scope], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| format!("Query unresolved: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read unresolved: {e}"))?;

    // Grouped per (scope, target): the same missing title in two projects is
    // two missing memories, because links never resolve across projects.
    let mut grouped: BTreeMap<(String, String), UnresolvedLink> = BTreeMap::new();
    for (link_scope, target_lc, target, src_id, src_title, src_slug) in rows {
        let entry = grouped.entry((link_scope.clone(), target_lc)).or_insert_with(|| {
            UnresolvedLink { scope: link_scope, target, count: 0, sources: Vec::new() }
        });
        entry.count += 1;
        if !entry.sources.iter().any(|s| s.id == src_id) {
            entry.sources.push(Backlink { id: src_id, title: src_title, slug: src_slug, count: 0 });
        }
    }
    Ok(grouped.into_values().collect())
}

pub fn graph(conn: &Connection, scope: Option<&str>) -> Result<Graph, String> {
    let mut nodes = Vec::new();
    {
        // A workspace view includes the general memories its links resolved
        // into — without them, cross-scope edges would dangle. Their `scope`
        // field says where they live; the frontend tints them accordingly.
        let sql = format!(
            "SELECT {META_COLS} FROM memories m
              WHERE ?1 IS NULL OR m.scope = ?1
                 OR m.id IN (SELECT l.target_id FROM links l
                              WHERE l.scope = ?1 AND l.target_id IS NOT NULL)"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare graph nodes: {e}"))?;
        let metas = stmt
            .query_map(params![scope], read_meta)
            .map_err(|e| format!("Query graph nodes: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read graph nodes: {e}"))?;
        for m in metas {
            nodes.push(GraphNode {
                id: m.id,
                scope: m.scope,
                title: m.title,
                kind: m.kind,
                tags: m.tags,
                incoming: m.incoming_links,
                outgoing: m.outgoing_links,
                missing: false,
            });
        }
    }
    // Ghost nodes: targets nobody answers to, one per (scope, target) — the
    // same missing title in two projects is two missing memories.
    {
        let mut stmt = conn
            .prepare(
                "SELECT l.scope, l.target_lc, MIN(l.target), COUNT(*)
                   FROM links l
                  WHERE l.target_id IS NULL AND (?1 IS NULL OR l.scope = ?1)
                  GROUP BY l.scope, l.target_lc",
            )
            .map_err(|e| format!("Prepare ghost nodes: {e}"))?;
        let rows = stmt
            .query_map(params![scope], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("Query ghost nodes: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read ghost nodes: {e}"))?;
        for (link_scope, target_lc, target, count) in rows {
            nodes.push(GraphNode {
                id: format!("ghost:{link_scope}:{target_lc}"),
                scope: link_scope,
                title: target,
                kind: None,
                tags: Vec::new(),
                incoming: count,
                outgoing: 0,
                missing: true,
            });
        }
    }

    let mut edges = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT source_id,
                        COALESCE(target_id, 'ghost:' || scope || ':' || target_lc),
                        COUNT(*)
                   FROM links
                  WHERE ?1 IS NULL OR scope = ?1
                  GROUP BY source_id, COALESCE(target_id, 'ghost:' || scope || ':' || target_lc)",
            )
            .map_err(|e| format!("Prepare edges: {e}"))?;
        let rows = stmt
            .query_map(params![scope], |r| {
                Ok(GraphEdge { source: r.get(0)?, target: r.get(1)?, count: r.get(2)? })
            })
            .map_err(|e| format!("Query edges: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Read edges: {e}"))?;
        edges.extend(rows);
    }
    Ok(Graph { nodes, edges })
}

pub fn stats(conn: &Connection, scope: Option<&str>) -> Result<Stats, String> {
    let one = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, params![scope], |r| r.get(0)).map_err(|e| format!("Stats ({sql}): {e}"))
    };
    let orphan_count = orphans(conn, scope)?.len() as i64;
    let tag_count = list_tags(conn, scope)?.len() as i64;
    let last: Option<String> = conn
        .query_row(
            "SELECT MAX(updated_at) FROM memories m WHERE ?1 IS NULL OR m.scope = ?1",
            params![scope],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Stats last update: {e}"))?
        .flatten();
    Ok(Stats {
        memories: one("SELECT COUNT(*) FROM memories m WHERE ?1 IS NULL OR m.scope = ?1")?,
        resolved_links: one(
            "SELECT COUNT(*) FROM links WHERE target_id IS NOT NULL AND (?1 IS NULL OR scope = ?1)",
        )?,
        unresolved_links: one(
            "SELECT COUNT(*) FROM links WHERE target_id IS NULL AND (?1 IS NULL OR scope = ?1)",
        )?,
        orphans: orphan_count,
        tags: tag_count,
        words: one(
            "SELECT COALESCE(SUM(word_count), 0) FROM memories m WHERE ?1 IS NULL OR m.scope = ?1",
        )?,
        last_updated_at: last,
    })
}

pub fn list_tags(conn: &Connection, scope: Option<&str>) -> Result<Vec<TagCount>, String> {
    let mut stmt = conn
        .prepare("SELECT tags_json FROM memories m WHERE ?1 IS NULL OR m.scope = ?1")
        .map_err(|e| format!("Prepare tags: {e}"))?;
    let rows = stmt
        .query_map(params![scope], |r| r.get::<_, String>(0))
        .map_err(|e| format!("Query tags: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read tags: {e}"))?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for raw in rows {
        for tag in serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default() {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    let mut out: Vec<TagCount> =
        counts.into_iter().map(|(tag, count)| TagCount { tag, count }).collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.tag.cmp(&b.tag)));
    Ok(out)
}

// ─── Connection suggestions ───────────────────────────────────────────────

/// Words too common to signal a shared topic, English and German mixed —
/// the vault is written in both.
const STOPWORDS: [&str; 58] = [
    "about", "after", "also", "been", "before", "being", "between", "code", "could", "does",
    "file", "files", "from", "have", "here", "into", "just", "like", "more", "most", "only",
    "other", "over", "should", "some", "than", "that", "their", "them", "then", "there", "these",
    "they", "this", "toward", "under", "until", "user", "were", "what", "when", "which", "will",
    "with", "would", "your", "aber", "auch", "dann", "dass", "eine", "haben", "nach", "nicht",
    "oder", "sich", "sind", "wird",
];

fn keywords_of(text: &str, limit: usize) -> Vec<String> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.chars().count() < 4 || raw.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let word = raw.to_lowercase();
        if STOPWORDS.contains(&word.as_str()) {
            continue;
        }
        *freq.entry(word).or_insert(0) += 1;
    }
    let mut ranked: Vec<(String, usize)> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().take(limit).map(|(w, _)| w).collect()
}

/// Memories that share distinctive keywords with the target but are not yet
/// linked to it in either direction — candidates for a fresh `[[link]]`.
/// Only linkable scopes qualify (the target's own, plus general): suggesting
/// a link that could never resolve would be advice to break the vault.
pub fn suggest_connections(
    conn: &Connection,
    id: &str,
    limit: i64,
) -> Result<Vec<Suggestion>, String> {
    let target_scope: Option<String> = conn
        .query_row("SELECT scope FROM memories WHERE id = ?1", params![id], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Read target scope: {e}"))?;
    let Some(target_scope) = target_scope else { return Ok(Vec::new()) };
    let doc: Option<(String, String, String)> = conn
        .query_row(
            "SELECT title, body, tags FROM docs WHERE memory_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("Read target doc: {e}"))?;
    let Some((title, body, tags)) = doc else { return Ok(Vec::new()) };

    let keywords = keywords_of(&format!("{title}\n{tags}\n{body}"), 12);
    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    let mut linked: HashSet<String> = HashSet::new();
    linked.insert(id.to_string());
    {
        let mut stmt = conn
            .prepare(
                "SELECT target_id FROM links WHERE source_id = ?1 AND target_id IS NOT NULL
                 UNION SELECT source_id FROM links WHERE target_id = ?1",
            )
            .map_err(|e| format!("Prepare linked set: {e}"))?;
        let rows = stmt
            .query_map(params![id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("Query linked set: {e}"))?;
        for row in rows {
            linked.insert(row.map_err(|e| format!("Read linked set: {e}"))?);
        }
    }

    let match_expr =
        keywords.iter().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(" OR ");
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.title, m.slug, LOWER(d.title || ' ' || d.body || ' ' || d.tags)
               FROM memories_fts
               JOIN docs d ON d.rowid = memories_fts.rowid
               JOIN memories m ON m.id = d.memory_id
              WHERE memories_fts MATCH ?1 AND (m.scope = ?3 OR m.scope = 'general')
              ORDER BY bm25(memories_fts, 5.0, 1.0, 3.0)
              LIMIT ?2",
        )
        .map_err(|e| format!("Prepare suggestions: {e}"))?;
    let candidates = stmt
        .query_map(params![match_expr, limit * 4, target_scope], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("Query suggestions: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read suggestions: {e}"))?;

    let mut out: Vec<Suggestion> = candidates
        .into_iter()
        .filter(|(cid, _, _, _)| !linked.contains(cid))
        .map(|(cid, ctitle, cslug, haystack)| {
            let shared: Vec<String> =
                keywords.iter().filter(|k| haystack.contains(*k)).cloned().collect();
            Suggestion { id: cid, title: ctitle, slug: cslug, shared_terms: shared }
        })
        .filter(|s| !s.shared_terms.is_empty())
        .collect();
    out.sort_by(|a, b| b.shared_terms.len().cmp(&a.shared_terms.len()));
    out.truncate(limit as usize);
    Ok(out)
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        init_schema(&conn).expect("schema");
        conn
    }

    fn put_in(conn: &Connection, scope: &str, rel: &str, text: &str, mtime: i64) -> String {
        let id = upsert_from_file(conn, &FileRecord { scope, rel_path: rel, text, mtime_ms: mtime })
            .expect("upsert");
        resolve_links(conn).expect("resolve");
        id
    }

    fn put(conn: &Connection, rel: &str, text: &str, mtime: i64) -> String {
        put_in(conn, "general", rel, text, mtime)
    }

    #[test]
    fn a_managed_file_keeps_its_frontmatter_id() {
        let conn = mem_conn();
        let id = put(&conn, "a.md", "---\nid: fixed-id\n---\n\n# Alpha\n\nBody", 1);
        assert_eq!(id, "fixed-id");
        let meta = get_by_identifier(&conn, "Alpha", None).expect("query").expect("found");
        assert!(meta.managed);
        assert_eq!(meta.title, "Alpha");
    }

    #[test]
    fn metadata_previews_are_bounded_body_only_and_follow_index_updates() {
        let conn = mem_conn();
        let text = format!("---\nid: preview-id\nprivate-key: hidden\n---\n# Preview\n\n{}", "知識 ".repeat(300));
        put(&conn, "preview.md", &text, 1);
        let meta = get_by_identifier(&conn, "preview-id", None).unwrap().unwrap();
        assert!(!meta.preview.contains("private-key"));
        assert!(meta.preview.contains("知識"));
        assert_eq!(meta.preview.chars().count(), 600);
        put(&conn, "preview.md", "---\nid: preview-id\n---\n# Preview\n\nUpdated body", 2);
        let listed = list(&conn, None, None, None, 10).unwrap();
        assert!(listed[0].preview.ends_with("Updated body"));
    }

    #[test]
    fn an_adopted_file_gets_a_stable_path_id_and_reads_never_mutate() {
        let conn = mem_conn();
        let first = put(&conn, "plain.md", "# Plain\n\nNo frontmatter here", 1);
        let second = put(&conn, "plain.md", "# Plain\n\nNo frontmatter here, edited", 2);
        assert_eq!(first, second);
        assert!(first.starts_with("ext-"));
        assert!(!get_by_identifier(&conn, "Plain", None).unwrap().unwrap().managed);
    }

    /// Two files claiming one frontmatter id (a duplicate made in the file
    /// manager) must not fight over one identity.
    #[test]
    fn a_duplicated_id_falls_back_to_a_path_id() {
        let conn = mem_conn();
        let original = put(&conn, "a.md", "---\nid: same\n---\n\n# One", 1);
        let copy = put(&conn, "b.md", "---\nid: same\n---\n\n# Two", 1);
        assert_eq!(original, "same");
        assert_ne!(copy, "same");
        assert_eq!(list(&conn, None, None, None, 10).unwrap().len(), 2);
    }

    #[test]
    fn links_resolve_by_title_and_by_filename_slug() {
        let conn = mem_conn();
        let a = put(&conn, "a.md", "# Alpha\n\nSee [[Beta]] and [[gamma-note]]", 1);
        let b = put(&conn, "beta.md", "# Beta\n\nBack to [[Alpha]]", 1);
        put(&conn, "gamma-note.md", "# The Gamma\n\nText", 1);
        let outgoing = outgoing_links(&conn, &a).expect("outgoing");
        assert!(outgoing.iter().all(|l| l.target_id.is_some()));
        let back = backlinks(&conn, &a).expect("backlinks");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, b);
    }

    #[test]
    fn creating_the_missing_memory_resolves_old_links() {
        let conn = mem_conn();
        let a = put(&conn, "a.md", "# Alpha\n\nSee [[Future Note]]", 1);
        assert_eq!(unresolved(&conn, None).expect("unresolved").len(), 1);
        let f = put(&conn, "future-note.md", "# Future Note\n\nNow it exists", 2);
        assert!(unresolved(&conn, None).expect("unresolved").is_empty());
        let outgoing = outgoing_links(&conn, &a).expect("outgoing");
        assert_eq!(outgoing[0].target_id.as_deref(), Some(f.as_str()));
    }

    #[test]
    fn deleting_a_memory_unresolves_its_backlinks() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Alpha\n\nSee [[Beta]]", 1);
        let b = put(&conn, "beta.md", "# Beta\n\nText", 1);
        remove_by_id(&conn, &b).expect("remove");
        resolve_links(&conn).expect("resolve");
        assert_eq!(unresolved(&conn, None).expect("unresolved").len(), 1);
    }

    #[test]
    fn search_finds_by_body_and_ranks_title_hits_first() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Deployment Rules\n\nAlways behind a flag.", 1);
        put(&conn, "b.md", "# Unrelated\n\nMentions deployment once in the body.", 1);
        let hits = search(&conn, None, "deployment", "all", 10).expect("search");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Deployment Rules");
        assert!(search(&conn, None, "deployment flag", "all", 10).expect("all").len() == 1);
        assert!(search(&conn, None, "deployment missingword", "any", 10).expect("any").len() == 2);
    }

    #[test]
    fn orphans_are_memories_with_no_resolved_links_either_way() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Alpha\n\nSee [[Beta]]", 1);
        put(&conn, "beta.md", "# Beta\n\nText", 1);
        put(&conn, "lonely.md", "# Lonely\n\nNo one links here. [[Nowhere]] is unresolved.", 1);
        let orphan_titles: Vec<String> =
            orphans(&conn, None).expect("orphans").into_iter().map(|m| m.title).collect();
        assert_eq!(orphan_titles, vec!["Lonely"]);
    }

    #[test]
    fn the_graph_shows_ghost_nodes_for_unresolved_targets() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Alpha\n\nSee [[Missing Piece]]", 1);
        let graph = graph(&conn, None).expect("graph");
        let ghost = graph.nodes.iter().find(|n| n.missing).expect("ghost node");
        assert_eq!(ghost.title, "Missing Piece");
        assert!(graph.edges.iter().any(|e| e.target == ghost.id));
    }

    #[test]
    fn a_vanished_file_drops_out_on_scan() {
        let conn = mem_conn();
        let dir = std::env::temp_dir().join(format!("qm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).expect("mkdir");
        fs::write(dir.join("one.md"), "# One\n\nText [[Two]]").expect("write");
        fs::write(dir.join("sub/two.md"), "# Two\n\nText").expect("write");
        fs::write(dir.join(".hidden.md"), "# Hidden").expect("write");

        let report = scan_vault(&conn, &dir, "general").expect("scan");
        assert_eq!(report.added, 2);
        assert!(get_by_identifier(&conn, "Hidden", None).expect("q").is_none());

        fs::remove_file(dir.join("sub/two.md")).expect("rm");
        let report = scan_vault(&conn, &dir, "general").expect("rescan");
        assert_eq!(report.removed, 1);
        assert_eq!(unresolved(&conn, None).expect("unresolved").len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn suggestions_share_terms_and_skip_already_linked() {
        let conn = mem_conn();
        let a = put(
            &conn,
            "a.md",
            "# Alpha\n\nKubernetes deployment rollout strategies for kubernetes clusters. [[Linked]]",
            1,
        );
        put(&conn, "linked.md", "# Linked\n\nkubernetes deployment", 1);
        put(&conn, "c.md", "# Candidate\n\nNotes on kubernetes rollout timing.", 1);
        put(&conn, "d.md", "# Unrelated\n\nCooking pasta properly.", 1);
        let suggestions = suggest_connections(&conn, &a, 5).expect("suggest");
        let titles: Vec<String> = suggestions.iter().map(|s| s.title.clone()).collect();
        assert!(titles.contains(&"Candidate".to_string()));
        assert!(!titles.contains(&"Linked".to_string()));
        assert!(!titles.contains(&"Unrelated".to_string()));
    }

    #[test]
    fn links_resolve_locally_first_then_general() {
        let conn = mem_conn();
        let ws = "core:workspace:abc";
        let general = put(&conn, "setup.md", "# Setup\n\nShared setup", 1);
        let src = put_in(&conn, ws, "note.md", "# Note\n\nSee [[Setup]]", 1);
        // While the workspace has no Setup of its own, the general one answers.
        let out = outgoing_links(&conn, &src).expect("outgoing");
        assert_eq!(out[0].target_id.as_deref(), Some(general.as_str()));
        // A local Setup takes over the moment it exists.
        let local = put_in(&conn, ws, "setup.md", "# Setup\n\nProject setup", 2);
        let out = outgoing_links(&conn, &src).expect("outgoing");
        assert_eq!(out[0].target_id.as_deref(), Some(local.as_str()));
    }

    /// Project knowledge stays project knowledge: a link may fall back to
    /// general, never into another workspace.
    #[test]
    fn links_never_resolve_into_a_foreign_workspace() {
        let conn = mem_conn();
        put_in(&conn, "core:workspace:aaa", "target.md", "# Target\n\nA's knowledge", 1);
        let src = put_in(&conn, "core:workspace:bbb", "src.md", "# Src\n\nSee [[Target]]", 1);
        assert!(outgoing_links(&conn, &src).expect("outgoing")[0].target_id.is_none());
        assert_eq!(unresolved(&conn, Some("core:workspace:bbb")).expect("unresolved").len(), 1);
        assert!(unresolved(&conn, Some("core:workspace:aaa")).expect("unresolved").is_empty());
    }

    #[test]
    fn the_same_rel_path_in_two_scopes_is_two_memories() {
        let conn = mem_conn();
        let a = put_in(&conn, "core:workspace:aaa", "notes.md", "# Notes\n\nA", 1);
        let b = put_in(&conn, "core:workspace:bbb", "notes.md", "# Notes\n\nB", 1);
        assert_ne!(a, b);
        assert_eq!(list(&conn, None, None, None, 10).expect("all").len(), 2);
        assert_eq!(
            list(&conn, Some("core:workspace:aaa"), None, None, 10).expect("scoped").len(),
            1
        );
    }

    #[test]
    fn identifier_resolution_prefers_the_hinted_scope_then_general() {
        let conn = mem_conn();
        let general = put(&conn, "config.md", "# Config\n\nshared", 5);
        let ws = put_in(&conn, "core:workspace:aaa", "config.md", "# Config\n\nproject", 1);
        assert_eq!(
            get_by_identifier(&conn, "Config", Some("core:workspace:aaa")).expect("q").expect("hit").id,
            ws
        );
        assert_eq!(get_by_identifier(&conn, "Config", None).expect("q").expect("hit").id, general);
        // Ids win outright, whatever the hint says.
        assert_eq!(
            get_by_identifier(&conn, &ws, Some("somewhere-else")).expect("q").expect("hit").id,
            ws
        );
    }

    #[test]
    fn scoped_search_filters_and_all_sees_everything() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Shared\n\nkubernetes", 1);
        put_in(&conn, "core:workspace:aaa", "b.md", "# ProjectNote\n\nkubernetes", 1);
        assert_eq!(search(&conn, None, "kubernetes", "all", 10).expect("all").len(), 2);
        assert_eq!(search(&conn, Some("general"), "kubernetes", "all", 10).expect("general").len(), 1);
        let scoped = search(&conn, Some("core:workspace:aaa"), "kubernetes", "all", 10).expect("ws");
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].scope, "core:workspace:aaa");
    }

    #[test]
    fn a_workspace_graph_includes_the_general_nodes_its_links_reach() {
        let conn = mem_conn();
        let shared = put(&conn, "shared.md", "# Shared\n\nText", 1);
        put_in(&conn, "core:workspace:aaa", "note.md", "# Note\n\nSee [[Shared]]", 1);
        let g = graph(&conn, Some("core:workspace:aaa")).expect("graph");
        assert!(g.nodes.iter().any(|n| n.id == shared && n.scope == "general"));
        assert!(g.edges.iter().any(|e| e.target == shared));
    }

    #[test]
    fn stats_add_up() {
        let conn = mem_conn();
        put(&conn, "a.md", "# Alpha\n\nSee [[Beta]] and [[Missing]]", 1);
        put(&conn, "beta.md", "---\ntags: [x]\n---\n\n# Beta\n\nText", 1);
        let s = stats(&conn, None).expect("stats");
        assert_eq!(s.memories, 2);
        assert_eq!(s.resolved_links, 1);
        assert_eq!(s.unresolved_links, 1);
        assert_eq!(s.tags, 1);
    }
}
