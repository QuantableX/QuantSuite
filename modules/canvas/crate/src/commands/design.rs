//! Design artboards (round 3, card 06; PandaCode's design canvas): HTML
//! mockups an agent writes and the user looks at side by side on the canvas,
//! with comments pinned to elements (a CSS selector) that go back to the
//! agent.
//!
//! Storage is the workspace's private QuantSuite folder — never the project:
//! `<workspace data>/canvas/design/<id>.html` and `<id>.comments.json` (the
//! artboard's name and its comments). "Export" copies a mockup into a folder
//! the user picks. The window renders a mockup in a sandboxed frame (scripts,
//! no same origin, no network), so a mockup cannot reach the app.
//!
//! The same commands serve the canvas and QuantMCP (`quantsuite.canvas.*`):
//! `workspace` is a registered workspace's name, id or folder; none means the
//! active one.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// A mockup is at most this big.
const HTML_MAX: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DesignComment {
    pub id: String,
    /// The element: a CSS selector inside the mockup.
    pub selector: String,
    /// The element's text when it was picked (a hint when the selector moves).
    pub quote: String,
    pub text: String,
    /// `open` | `resolved`.
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    /// What the agent (or you) said when resolving it.
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Sidecar {
    pub name: String,
    pub comments: Vec<DesignComment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artboard {
    pub id: String,
    pub name: String,
    pub updated_at: i64,
    pub open_comments: usize,
    pub resolved_comments: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtboardFull {
    #[serde(flatten)]
    pub artboard: Artboard,
    pub html: String,
    pub comments: Vec<DesignComment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentOut {
    pub artboard: String,
    pub artboard_name: String,
    #[serde(flatten)]
    pub comment: DesignComment,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignChanged {
    /// The workspace's folder.
    pub workspace: String,
    pub id: String,
    /// A new artboard: the canvas opens a window for it.
    pub created: bool,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// An artboard id: lowercase letters, digits and dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let s: String = out.trim_matches('-').chars().take(60).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "artboard".into()
    } else {
        s
    }
}

/// The folder and name of the workspace a call means.
fn workspace(app: &AppHandle, ident: Option<&str>) -> Result<(String, String), String> {
    let db = app.try_state::<qs_core::db::Db>().ok_or("core.db is not ready")?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let w = qs_core::workspaces::resolve(&conn, ident)?;
    Ok((w.path, w.name))
}

fn design_dir(folder: &str) -> Result<PathBuf, String> {
    let dir = qs_core::paths::workspace_dir(folder).join("canvas").join("design");
    fs::create_dir_all(&dir).map_err(|e| format!("could not create the design folder: {e}"))?;
    Ok(dir)
}

fn html_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.html"))
}

fn sidecar_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.comments.json"))
}

pub fn read_sidecar(dir: &Path, id: &str) -> Sidecar {
    fs::read_to_string(sidecar_path(dir, id)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn write_sidecar(dir: &Path, id: &str, s: &Sidecar) -> Result<(), String> {
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{id}.comments.json.tmp"));
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, sidecar_path(dir, id)).map_err(|e| e.to_string())
}

fn summary(dir: &Path, id: &str) -> Option<Artboard> {
    let meta = fs::metadata(html_path(dir, id)).ok()?;
    let side = read_sidecar(dir, id);
    let updated_at = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
    let open = side.comments.iter().filter(|c| c.status != "resolved").count();
    Some(Artboard {
        id: id.to_string(),
        name: if side.name.trim().is_empty() { id.to_string() } else { side.name },
        updated_at,
        open_comments: open,
        resolved_comments: side.comments.len() - open,
    })
}

/// Every artboard in a design folder, newest first.
pub fn list_in(dir: &Path) -> Vec<Artboard> {
    let mut out: Vec<Artboard> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".html")).map(str::to_string))
        .filter_map(|id| summary(dir, &id))
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    out
}

/// Creates or replaces a mockup; its comments stay.
pub fn write_in(dir: &Path, id: Option<&str>, name: &str, html: &str) -> Result<Artboard, String> {
    if html.trim().is_empty() {
        return Err("The mockup is empty.".into());
    }
    if html.len() > HTML_MAX {
        return Err(format!("The mockup is {} KB; at most {} KB.", html.len() / 1024, HTML_MAX / 1024));
    }
    let id = slug(id.filter(|i| !i.trim().is_empty()).unwrap_or(name));
    fs::write(html_path(dir, &id), html).map_err(|e| format!("could not save the mockup: {e}"))?;
    let mut side = read_sidecar(dir, &id);
    if !name.trim().is_empty() {
        side.name = name.trim().to_string();
    } else if side.name.is_empty() {
        side.name = id.clone();
    }
    write_sidecar(dir, &id, &side)?;
    summary(dir, &id).ok_or_else(|| "The mockup was not saved.".into())
}

fn known(dir: &Path, id: &str) -> Result<String, String> {
    let id = slug(id);
    if html_path(dir, &id).exists() {
        Ok(id)
    } else {
        let names: Vec<String> = list_in(dir).into_iter().map(|a| a.id).collect();
        Err(format!("No artboard '{id}'. Artboards: {}", if names.is_empty() { "none yet".into() } else { names.join(", ") }))
    }
}

/// Sets a comment's status (and the note that came with it).
pub fn set_status_in(dir: &Path, id: &str, comment: &str, status: &str, note: Option<&str>) -> Result<DesignComment, String> {
    let id = known(dir, id)?;
    let mut side = read_sidecar(dir, &id);
    let c = side.comments.iter_mut().find(|c| c.id == comment).ok_or_else(|| format!("No comment '{comment}' on '{id}'."))?;
    c.status = if status == "resolved" { "resolved".into() } else { "open".into() };
    c.resolved_at = (c.status == "resolved").then(now_ms);
    if let Some(n) = note {
        c.note = n.trim().to_string();
    }
    let out = c.clone();
    write_sidecar(dir, &id, &side)?;
    Ok(out)
}

/// A free file name in `folder` for `id`: `<id>.html`, else `<id>-2.html`, ….
pub fn export_target(folder: &Path, id: &str) -> PathBuf {
    let first = folder.join(format!("{id}.html"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| folder.join(format!("{id}-{n}.html"))).find(|p| !p.exists()).unwrap_or(first)
}

fn changed(app: &AppHandle, folder: &str, id: &str) {
    emit(app, folder, id, false);
}

fn emit(app: &AppHandle, folder: &str, id: &str, created: bool) {
    let _ = app.emit("canvas-design-changed", DesignChanged { workspace: folder.to_string(), id: id.to_string(), created });
}

// ── Commands (the canvas and QuantMCP) ──

#[tauri::command(async)]
pub fn design_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<Artboard>, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    Ok(list_in(&design_dir(&folder)?))
}

#[tauri::command(async)]
pub fn design_read(app: AppHandle, workspace: Option<String>, artboard: String) -> Result<ArtboardFull, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&folder)?;
    let id = known(&dir, &artboard)?;
    let html = fs::read_to_string(html_path(&dir, &id)).map_err(|e| e.to_string())?;
    let artboard = summary(&dir, &id).ok_or("The artboard is gone.")?;
    Ok(ArtboardFull { artboard, html, comments: read_sidecar(&dir, &id).comments })
}

#[tauri::command(async)]
pub fn design_write(app: AppHandle, workspace: Option<String>, name: String, html: String, artboard: Option<String>) -> Result<Artboard, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&folder)?;
    let existed = html_path(&dir, &slug(artboard.as_deref().filter(|i| !i.trim().is_empty()).unwrap_or(&name))).exists();
    let a = write_in(&dir, artboard.as_deref(), &name, &html)?;
    emit(&app, &folder, &a.id, !existed);
    Ok(a)
}

#[tauri::command(async)]
pub fn design_comments(app: AppHandle, workspace: Option<String>, artboard: Option<String>, status: Option<String>) -> Result<Vec<CommentOut>, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&folder)?;
    let want = status.unwrap_or_else(|| "open".into());
    let boards: Vec<Artboard> = match artboard.as_deref() {
        Some(a) => vec![summary(&dir, &known(&dir, a)?).ok_or("The artboard is gone.")?],
        None => list_in(&dir),
    };
    Ok(boards
        .into_iter()
        .flat_map(|b| {
            read_sidecar(&dir, &b.id)
                .comments
                .into_iter()
                .filter(|c| want == "all" || (want == "resolved") == (c.status == "resolved"))
                .map(move |c| CommentOut { artboard: b.id.clone(), artboard_name: b.name.clone(), comment: c })
                .collect::<Vec<_>>()
        })
        .collect())
}

#[tauri::command(async)]
pub fn design_resolve(app: AppHandle, workspace: Option<String>, artboard: String, comment: String, note: Option<String>) -> Result<DesignComment, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let c = set_status_in(&design_dir(&folder)?, &artboard, &comment, "resolved", note.as_deref())?;
    changed(&app, &folder, &slug(&artboard));
    Ok(c)
}

#[tauri::command(async)]
pub fn design_comment_add(app: AppHandle, workspace: Option<String>, artboard: String, selector: String, text: String, quote: Option<String>) -> Result<DesignComment, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&folder)?;
    let id = known(&dir, &artboard)?;
    if selector.trim().is_empty() || text.trim().is_empty() {
        return Err("A comment needs an element and a text.".into());
    }
    let mut side = read_sidecar(&dir, &id);
    let c = DesignComment {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        selector: selector.trim().to_string(),
        quote: quote.unwrap_or_default().trim().chars().take(160).collect(),
        text: text.trim().to_string(),
        status: "open".into(),
        created_at: now_ms(),
        resolved_at: None,
        note: String::new(),
    };
    side.comments.push(c.clone());
    write_sidecar(&dir, &id, &side)?;
    changed(&app, &folder, &id);
    Ok(c)
}

#[tauri::command(async)]
pub fn design_comment_status(app: AppHandle, workspace: Option<String>, artboard: String, comment: String, status: String) -> Result<DesignComment, String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let c = set_status_in(&design_dir(&folder)?, &artboard, &comment, &status, None)?;
    changed(&app, &folder, &slug(&artboard));
    Ok(c)
}

#[tauri::command(async)]
pub fn design_comment_delete(app: AppHandle, workspace: Option<String>, artboard: String, comment: String) -> Result<(), String> {
    let (folder, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&folder)?;
    let id = known(&dir, &artboard)?;
    let mut side = read_sidecar(&dir, &id);
    side.comments.retain(|c| c.id != comment);
    write_sidecar(&dir, &id, &side)?;
    changed(&app, &folder, &id);
    Ok(())
}

/// Copies a mockup into a folder the user picked; never overwrites a file.
#[tauri::command(async)]
pub fn design_export(app: AppHandle, workspace: Option<String>, artboard: String, folder: String) -> Result<String, String> {
    let (ws, _) = self::workspace(&app, workspace.as_deref())?;
    let dir = design_dir(&ws)?;
    let id = known(&dir, &artboard)?;
    let target = export_target(Path::new(&folder), &id);
    fs::copy(html_path(&dir, &id), &target).map_err(|e| format!("could not copy the mockup: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qs-design-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ids_are_slugs() {
        assert_eq!(slug("  Landing Page — v2! "), "landing-page-v2");
        assert_eq!(slug("***"), "artboard");
        assert_eq!(slug(&"x".repeat(80)).len(), 60);
    }

    #[test]
    fn a_mockup_and_its_comments_live_side_by_side() {
        let dir = temp("store");
        let a = write_in(&dir, None, "Landing page", "<h1>Hi</h1>").unwrap();
        assert_eq!((a.id.as_str(), a.name.as_str(), a.open_comments), ("landing-page", "Landing page", 0));
        assert!(dir.join("landing-page.html").exists() && dir.join("landing-page.comments.json").exists());
        let mut side = read_sidecar(&dir, "landing-page");
        side.comments.push(DesignComment { id: "c1".into(), selector: "main > h1:nth-of-type(1)".into(), text: "Bigger".into(), status: "open".into(), ..Default::default() });
        side.comments.push(DesignComment { id: "c2".into(), selector: "#cta".into(), text: "Green".into(), status: "open".into(), ..Default::default() });
        write_sidecar(&dir, "landing-page", &side).unwrap();
        // A rewrite keeps the comments; the stored format reads back.
        let a = write_in(&dir, Some("landing-page"), "", "<h1>Hello</h1>").unwrap();
        assert_eq!((a.name.as_str(), a.open_comments), ("Landing page", 2));
        let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("landing-page.comments.json")).unwrap()).unwrap();
        assert_eq!(raw["comments"][0]["selector"], "main > h1:nth-of-type(1)");
        assert_eq!(raw["comments"][1]["createdAt"], 0);
        let c = set_status_in(&dir, "landing-page", "c2", "resolved", Some("Made it green")).unwrap();
        assert_eq!((c.status.as_str(), c.note.as_str()), ("resolved", "Made it green"));
        assert!(c.resolved_at.is_some());
        let listed = list_in(&dir);
        assert_eq!((listed[0].open_comments, listed[0].resolved_comments), (1, 1));
        assert!(set_status_in(&dir, "landing-page", "nope", "resolved", None).is_err());
        assert!(set_status_in(&dir, "other", "c1", "resolved", None).unwrap_err().contains("Artboards: landing-page"));
        assert!(write_in(&dir, None, "x", "  ").is_err());
        assert!(write_in(&dir, None, "x", &"a".repeat(HTML_MAX + 1)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_never_overwrites() {
        let dir = temp("export");
        assert_eq!(export_target(&dir, "landing"), dir.join("landing.html"));
        fs::write(dir.join("landing.html"), "x").unwrap();
        fs::write(dir.join("landing-2.html"), "x").unwrap();
        assert_eq!(export_target(&dir, "landing"), dir.join("landing-3.html"));
        let _ = fs::remove_dir_all(&dir);
    }
}
