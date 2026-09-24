//! Collection sources in script.db: where catalogs come from — a GitHub
//! repository at a branch (the published catalog) or a local folder (a
//! checkout, for authoring and tests).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::tokens::TokenStore;

/// The schema generation of the Collection tables (`PRAGMA user_version`):
/// 1 = `store_sources` (the feature was called the Store), 2 = `collection_sources`.
const SCHEMA: i64 = 2;

/// Create the table on first start and seed the user's own catalog once —
/// a source the user deletes stays deleted. A database of the Store era is
/// renamed, and its untouched seed follows the repository's new name.
pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA {
        return Ok(());
    }
    if version == 1 {
        conn.execute_batch("ALTER TABLE store_sources RENAME TO collection_sources;")?;
        conn.execute(
            "UPDATE collection_sources SET repo = 'QuantScript-Collection', last_commit = NULL, last_checked = NULL
             WHERE id = 'quantablex' AND kind = 'github' AND owner = 'QuantableX' AND repo = 'QuantScript-Indicators'",
            [],
        )?;
        return conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA};"));
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS collection_sources (
            id           TEXT PRIMARY KEY,
            kind         TEXT    NOT NULL,
            name         TEXT    NOT NULL,
            owner        TEXT    NOT NULL DEFAULT '',
            repo         TEXT    NOT NULL DEFAULT '',
            branch       TEXT    NOT NULL DEFAULT 'main',
            path         TEXT    NOT NULL DEFAULT '',
            enabled      INTEGER NOT NULL DEFAULT 1,
            last_commit  TEXT,
            last_checked TEXT,
            created_at   TEXT    NOT NULL
        );",
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO collection_sources (id, kind, name, owner, repo, branch, path, enabled, created_at)
         VALUES ('quantablex', 'github', 'QuantableX', 'QuantableX', 'QuantScript-Collection', 'main', '', 1, ?1)",
        params![super::now_iso()],
    )?;
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA};"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    /// `github` | `folder`
    pub kind: String,
    pub name: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    /// github: a folder inside the repository ('' = its root); folder: the checkout.
    pub path: String,
    pub enabled: bool,
    pub last_commit: Option<String>,
    pub last_checked: Option<String>,
}

/// What the UI sees of a source: never the token, only whether one is stored.
#[derive(Debug, Clone, Serialize)]
pub struct SourceView {
    #[serde(flatten)]
    pub source: Source,
    pub has_token: bool,
}

/// A source as the settings form sends it; `id` empty = a new source.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceInput {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub path: String,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

const COLUMNS: &str = "id, kind, name, owner, repo, branch, path, enabled, last_commit, last_checked";

fn row_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        owner: row.get(3)?,
        repo: row.get(4)?,
        branch: row.get(5)?,
        path: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        last_commit: row.get(8)?,
        last_checked: row.get(9)?,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<Source>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLUMNS} FROM collection_sources ORDER BY created_at, id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row_source).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn get(conn: &Connection, id: &str) -> Result<Source, String> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM collection_sources WHERE id = ?1"), params![id], row_source)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("There is no Collection source '{id}'."))
}

pub fn view(source: Source, tokens: &dyn TokenStore) -> SourceView {
    let has_token = source.kind == "github" && matches!(tokens.get(&source.id), Ok(Some(_)));
    SourceView { source, has_token }
}

fn plain(value: &str) -> bool {
    !value.is_empty() && value.len() <= 100 && value.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(40).collect::<String>();
    if out.is_empty() { "source".into() } else { out }
}

/// Insert or update a source after validating it; returns the stored row.
pub fn save(conn: &Connection, input: SourceInput) -> Result<Source, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("A source needs a name.".into());
    }
    let (owner, repo, branch, path) = match input.kind.as_str() {
        "github" => {
            let branch = if input.branch.trim().is_empty() { "main".to_string() } else { input.branch.trim().to_string() };
            if !plain(input.owner.trim()) || !plain(input.repo.trim()) {
                return Err("A GitHub source needs an owner and a repository (letters, digits, - _ .).".into());
            }
            if !branch.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) || branch.contains("..") {
                return Err(format!("'{branch}' is not a branch name."));
            }
            let path = input.path.trim().trim_matches('/').to_string();
            if path.split('/').any(|part| part == "..") || path.contains('\\') {
                return Err("The folder inside the repository must be a plain relative path.".into());
            }
            (input.owner.trim().to_string(), input.repo.trim().to_string(), branch, path)
        }
        "folder" => {
            let path = input.path.trim().to_string();
            if path.is_empty() || !std::path::Path::new(&path).is_absolute() {
                return Err("A folder source needs the absolute path of a catalog checkout.".into());
            }
            (String::new(), String::new(), String::new(), path)
        }
        other => return Err(format!("'{other}' is not a source kind — github or folder.")),
    };
    let id = if input.id.trim().is_empty() {
        let base = slug(&name);
        let mut id = base.clone();
        let mut n = 2;
        while get(conn, &id).is_ok() {
            id = format!("{base}-{n}");
            n += 1;
        }
        conn.execute(
            "INSERT INTO collection_sources (id, kind, name, owner, repo, branch, path, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, input.kind, name, owner, repo, branch, path, input.enabled as i64, super::now_iso()],
        )
        .map_err(|e| e.to_string())?;
        id
    } else {
        let id = input.id.trim().to_string();
        let before = get(conn, &id)?;
        // A different place is a different catalog: forget the pinned commit.
        let moved = before.kind != input.kind || before.owner != owner || before.repo != repo || before.branch != branch || before.path != path;
        conn.execute(
            "UPDATE collection_sources SET kind = ?2, name = ?3, owner = ?4, repo = ?5, branch = ?6, path = ?7, enabled = ?8,
             last_commit = CASE WHEN ?9 THEN NULL ELSE last_commit END,
             last_checked = CASE WHEN ?9 THEN NULL ELSE last_checked END
             WHERE id = ?1",
            params![id, input.kind, name, owner, repo, branch, path, input.enabled as i64, moved],
        )
        .map_err(|e| e.to_string())?;
        id
    };
    get(conn, &id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<(), String> {
    get(conn, id)?;
    conn.execute("DELETE FROM collection_sources WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remember_commit(conn: &Connection, id: &str, commit: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE collection_sources SET last_commit = ?2, last_checked = ?3 WHERE id = ?1",
        params![id, commit, super::now_iso()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
