//! The workspace registry, readable from plain Rust (PLAN-WORKSPACE-UNIFY).
//!
//! A workspace is one folder, registered as a `core:workspace:<b36>` entity in
//! core.db; the open one sits in the setting `core / workspace.active`
//! (PLAN-WORKSPACES §1). The webview owns all writes through
//! `packages/core/src/workspaces.ts` — this module is the read side for module
//! crates (memory vaults, QuantMCP's kanban and codebase index), so every
//! module resolves "which workspace?" through one function instead of five
//! private copies.
//!
//! `workspace_id_for` must stay bit-identical to `workspaceIdFor` in
//! `packages/core/src/workspaces.ts` — the b36 tail names memory vault
//! directories and index databases on disk, so a divergent hash would fork a
//! workspace's data. The parity test below pins vectors computed from the TS
//! implementation.

use rusqlite::Connection;

/// One registered workspace, as module crates need it.
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// Full entity id, `core:workspace:<b36>`.
    pub id: String,
    pub name: String,
    pub path: String,
}

impl WorkspaceEntry {
    /// The id's b36 tail — the on-disk key for per-workspace storage
    /// (memory vaults `workspaces/<b36>/`, index DBs `indexes/<b36>.db`).
    pub fn b36(&self) -> &str {
        self.id.rsplit(':').next().unwrap_or_default()
    }
}

/// Path in one shape — the same normalization `packages/core/src/workspaces.ts`
/// uses, so Rust and the webview agree on which folder is which.
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// The entity id for a folder — djb2 over the normalized path, base36.
///
/// JS hashes UTF-16 code units (`charCodeAt`) and truncates with `>>> 0`;
/// `encode_utf16` plus wrapping u32 arithmetic lands on the same low 32 bits.
pub fn workspace_id_for(path: &str) -> String {
    let norm = normalize_path(path);
    let mut h: u32 = 5381;
    for unit in norm.encode_utf16() {
        h = h.wrapping_shl(5).wrapping_add(h).wrapping_add(unit as u32);
    }
    format!("core:workspace:{}", to_base36(h))
}

/// Lowercase base36, matching JS `Number.prototype.toString(36)`.
fn to_base36(mut n: u32) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = [0u8; 7]; // u32 max is 6+1 base36 digits
    let mut i = out.len();
    while n > 0 {
        i -= 1;
        out[i] = DIGITS[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8_lossy(&out[i..]).into_owned()
}

/// Every workspace in the registry. Empty on any failure — a broken registry
/// read must degrade to "none registered", never to an error.
pub fn list(conn: &Connection) -> Vec<WorkspaceEntry> {
    let Ok(entities) = crate::db::list_entities(conn, Some("core"), Some("workspace"), 200) else {
        return Vec::new();
    };
    entities
        .into_iter()
        .filter_map(|e| {
            let path = e
                .payload
                .as_ref()
                .and_then(|p| p.get("path"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or(e.subtitle)?;
            if path.trim().is_empty() {
                return None;
            }
            Some(WorkspaceEntry { id: e.id, name: e.title, path })
        })
        .collect()
}

/// The suite's open workspace (setting `core / workspace.active`), if that
/// folder is in the registry.
pub fn active(conn: &Connection) -> Option<WorkspaceEntry> {
    let setting = crate::db::get_setting(conn, "core", "workspace.active").ok()??;
    let path = setting.get("path")?.as_str()?.to_string();
    let norm = normalize_path(&path);
    list(conn).into_iter().find(|w| normalize_path(&w.path) == norm)
}

/// Look up one workspace by its full entity id.
pub fn by_id(conn: &Connection, id: &str) -> Option<WorkspaceEntry> {
    list(conn).into_iter().find(|w| w.id == id)
}

/// Resolve a caller-supplied workspace identifier.
///
/// `None` or `"active"` means the open workspace; anything else matches the
/// registry by entity id, by name (case-insensitive) or by folder path. The
/// error names the registered workspaces so an agent can self-correct.
pub fn resolve(conn: &Connection, ident: Option<&str>) -> Result<WorkspaceEntry, String> {
    let ident = ident.map(str::trim).filter(|s| !s.is_empty());
    match ident {
        None | Some("active") => active(conn).ok_or_else(|| {
            format!(
                "No active workspace. Pass `workspace` (name, id, or folder path). Registered: {}",
                registered_names(conn)
            )
        }),
        Some(raw) => {
            let workspaces = list(conn);
            let norm = normalize_path(raw);
            workspaces
                .iter()
                .find(|w| w.id == raw)
                .or_else(|| workspaces.iter().find(|w| w.name.eq_ignore_ascii_case(raw)))
                .or_else(|| workspaces.iter().find(|w| normalize_path(&w.path) == norm))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Unknown workspace '{raw}'. Registered: {}",
                        registered_names(conn)
                    )
                })
        }
    }
}

fn registered_names(conn: &Connection) -> String {
    let names: Vec<String> = list(conn).into_iter().map(|w| w.name).collect();
    if names.is_empty() {
        "none — open a folder in QuantSuite first".into()
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors computed with `packages/core/src/workspaces.ts` `workspaceIdFor`
    /// (node). If this test fails, the Rust hash diverged from the webview's —
    /// never "fix" the vectors; fix the hash.
    #[test]
    fn id_matches_typescript_djb2() {
        for (path, expected) in [
            ("C:\\Projects\\QuantSuite", "core:workspace:78xw9p"),
            ("C:/Projects/QuantSuite/", "core:workspace:78xw9p"),
            ("C:\\Projects\\Lambo-Lounge", "core:workspace:1gfzoq4"),
            ("/home/user/wörk space", "core:workspace:1jm22wg"),
        ] {
            assert_eq!(workspace_id_for(path), expected, "path: {path}");
        }
    }

    #[test]
    fn base36_matches_js_to_string() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        assert_eq!(to_base36(u32::MAX), "1z141z3");
    }
}
