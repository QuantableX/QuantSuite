//! Moodboards (round 3, card 07; PandaCode's moodboards): reference images on
//! the canvas — dropped or pasted, captioned, ordered by drag — that agents
//! can see (`quantsuite.canvas.moodboard_list` / `moodboard_read` give the
//! image files and captions; an agent reads the file itself).
//!
//! Storage is the workspace's private QuantSuite folder, never the project:
//! `<workspace data>/canvas/moodboards/<id>/board.json` plus a copy of each
//! image. Nothing is downloaded: images come only from what the user drops
//! or pastes.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// One image is at most this big.
const IMAGE_MAX: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MoodItem {
    pub id: String,
    /// The copy's file name inside the board's folder.
    pub file: String,
    pub caption: String,
    /// The name it came with.
    pub source: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BoardFile {
    pub name: String,
    /// In board order.
    pub items: Vec<MoodItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    pub id: String,
    pub name: String,
    pub images: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardImage {
    pub id: String,
    /// The image file, for an agent to read.
    pub path: String,
    pub caption: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub id: String,
    pub name: String,
    pub images: Vec<BoardImage>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

use super::design::slug;

fn workspace(app: &AppHandle, ident: Option<&str>) -> Result<String, String> {
    let db = app.try_state::<qs_core::db::Db>().ok_or("core.db is not ready")?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    Ok(qs_core::workspaces::resolve(&conn, ident)?.path)
}

fn boards_dir(folder: &str) -> Result<PathBuf, String> {
    let dir = qs_core::paths::workspace_dir(folder).join("canvas").join("moodboards");
    fs::create_dir_all(&dir).map_err(|e| format!("could not create the moodboards folder: {e}"))?;
    Ok(dir)
}

pub fn read_board(dir: &Path) -> Option<BoardFile> {
    fs::read_to_string(dir.join("board.json")).ok().and_then(|s| serde_json::from_str(&s).ok())
}

pub fn write_board(dir: &Path, b: &BoardFile) -> Result<(), String> {
    let tmp = dir.join("board.json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join("board.json")).map_err(|e| e.to_string())
}

/// The image kind from its first bytes; anything else is refused.
pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes.get(..12)? {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P'] => Some("webp"),
        _ => None,
    }
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        _ => "image/webp",
    }
}

pub fn list_in(root: &Path) -> Vec<BoardSummary> {
    let mut out: Vec<BoardSummary> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_str()?.to_string();
            let b = read_board(&e.path())?;
            Some(BoardSummary { id, name: if b.name.is_empty() { e.file_name().to_string_lossy().into_owned() } else { b.name }, images: b.items.len() })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn create_in(root: &Path, name: &str) -> Result<BoardSummary, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A moodboard needs a name.".into());
    }
    let mut id = slug(name);
    let mut n = 2;
    while root.join(&id).exists() {
        id = format!("{}-{n}", slug(name));
        n += 1;
    }
    let dir = root.join(&id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_board(&dir, &BoardFile { name: name.to_string(), items: Vec::new() })?;
    Ok(BoardSummary { id, name: name.to_string(), images: 0 })
}

fn open(root: &Path, id: &str) -> Result<(PathBuf, BoardFile), String> {
    let dir = root.join(slug(id));
    let b = read_board(&dir).ok_or_else(|| {
        let names: Vec<String> = list_in(root).into_iter().map(|b| b.id).collect();
        format!("No moodboard '{id}'. Moodboards: {}", if names.is_empty() { "none yet".into() } else { names.join(", ") })
    })?;
    Ok((dir, b))
}

/// Adds a copy of an image to the end of the board.
pub fn add_in(root: &Path, board: &str, source: &str, bytes: &[u8]) -> Result<MoodItem, String> {
    if bytes.len() > IMAGE_MAX {
        return Err(format!("The image is {} MB; at most {} MB.", bytes.len() / 1024 / 1024, IMAGE_MAX / 1024 / 1024));
    }
    let ext = image_ext(bytes).ok_or("Only PNG, JPEG, GIF and WebP images go on a moodboard.")?;
    let (dir, mut b) = open(root, board)?;
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let file = format!("{id}.{ext}");
    fs::write(dir.join(&file), bytes).map_err(|e| format!("could not save the image: {e}"))?;
    let item = MoodItem { id, file, caption: String::new(), source: source.trim().to_string(), added_at: now_ms() };
    b.items.push(item.clone());
    write_board(&dir, &b)?;
    Ok(item)
}

/// The board in a new order: the ids given first, in that order; any left out keep theirs after.
pub fn reorder_in(root: &Path, board: &str, order: &[String]) -> Result<(), String> {
    let (dir, mut b) = open(root, board)?;
    let mut items = std::mem::take(&mut b.items);
    let mut out = Vec::new();
    for id in order {
        if let Some(i) = items.iter().position(|x| &x.id == id) {
            out.push(items.remove(i));
        }
    }
    out.extend(items);
    b.items = out;
    write_board(&dir, &b)
}

pub fn caption_in(root: &Path, board: &str, item: &str, caption: &str) -> Result<(), String> {
    let (dir, mut b) = open(root, board)?;
    let it = b.items.iter_mut().find(|x| x.id == item).ok_or("That image is not on the board.")?;
    it.caption = caption.trim().chars().take(500).collect();
    write_board(&dir, &b)
}

pub fn remove_in(root: &Path, board: &str, item: &str) -> Result<(), String> {
    let (dir, mut b) = open(root, board)?;
    if let Some(i) = b.items.iter().position(|x| x.id == item) {
        let gone = b.items.remove(i);
        let _ = fs::remove_file(dir.join(gone.file));
    }
    write_board(&dir, &b)
}

fn board_of(root: &Path, id: &str) -> Result<Board, String> {
    let (dir, b) = open(root, id)?;
    Ok(Board {
        id: slug(id),
        name: b.name,
        images: b
            .items
            .into_iter()
            .map(|i| BoardImage { id: i.id, path: dir.join(&i.file).to_string_lossy().into_owned(), caption: i.caption, source: i.source })
            .collect(),
    })
}

// ── Commands (the canvas and QuantMCP) ──

#[tauri::command(async)]
pub fn moodboard_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<BoardSummary>, String> {
    Ok(list_in(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?))
}

#[tauri::command(async)]
pub fn moodboard_read(app: AppHandle, workspace: Option<String>, board: String) -> Result<Board, String> {
    board_of(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?, &board)
}

#[tauri::command(async)]
pub fn moodboard_create(app: AppHandle, workspace: Option<String>, name: String) -> Result<BoardSummary, String> {
    create_in(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?, &name)
}

/// An image from the user: base64 data (paste, drop) or a file path.
#[tauri::command(async)]
pub fn moodboard_add(app: AppHandle, workspace: Option<String>, board: String, name: String, data: Option<String>, path: Option<String>) -> Result<MoodItem, String> {
    let root = boards_dir(&self::workspace(&app, workspace.as_deref())?)?;
    let bytes = match (data, path) {
        (Some(d), _) => {
            let raw = d.split_once(',').map(|(_, b)| b).unwrap_or(&d);
            base64::engine::general_purpose::STANDARD.decode(raw.trim()).map_err(|_| "The image data is broken.".to_string())?
        }
        (None, Some(p)) => fs::read(&p).map_err(|e| format!("could not read {p}: {e}"))?,
        (None, None) => return Err("No image given.".into()),
    };
    add_in(&root, &board, &name, &bytes)
}

#[tauri::command(async)]
pub fn moodboard_caption(app: AppHandle, workspace: Option<String>, board: String, item: String, caption: String) -> Result<(), String> {
    caption_in(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?, &board, &item, &caption)
}

#[tauri::command(async)]
pub fn moodboard_reorder(app: AppHandle, workspace: Option<String>, board: String, order: Vec<String>) -> Result<(), String> {
    reorder_in(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?, &board, &order)
}

#[tauri::command(async)]
pub fn moodboard_remove(app: AppHandle, workspace: Option<String>, board: String, item: String) -> Result<(), String> {
    remove_in(&boards_dir(&self::workspace(&app, workspace.as_deref())?)?, &board, &item)
}

/// An image as a data URL, for the window.
#[tauri::command(async)]
pub fn moodboard_image(app: AppHandle, workspace: Option<String>, board: String, item: String) -> Result<String, String> {
    let root = boards_dir(&self::workspace(&app, workspace.as_deref())?)?;
    let (dir, b) = open(&root, &board)?;
    let it = b.items.iter().find(|x| x.id == item).ok_or("That image is not on the board.")?;
    let bytes = fs::read(dir.join(&it.file)).map_err(|_| "The image file is gone.".to_string())?;
    let ext = it.file.rsplit('.').next().unwrap_or("png");
    Ok(format!("data:{};base64,{}", mime(ext), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, 1, 2, 3];

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("qs-mood-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_board_keeps_copies_captions_and_order() {
        let root = temp();
        let b = create_in(&root, "Calm dashboards").unwrap();
        assert_eq!((b.id.as_str(), b.images), ("calm-dashboards", 0));
        assert_eq!(create_in(&root, "Calm dashboards").unwrap().id, "calm-dashboards-2", "a name taken twice gets a number");
        let a = add_in(&root, "calm-dashboards", "linear.png", PNG).unwrap();
        let c = add_in(&root, "calm-dashboards", "stripe.png", PNG).unwrap();
        let d = add_in(&root, "calm-dashboards", "vercel.png", PNG).unwrap();
        assert!(root.join("calm-dashboards").join(&a.file).exists(), "an image is copied into the board");
        caption_in(&root, "calm-dashboards", &c.id, "  Dense table, quiet colours  ").unwrap();
        reorder_in(&root, "calm-dashboards", &[d.id.clone(), a.id.clone()]).unwrap();
        let file: BoardFile = serde_json::from_str(&fs::read_to_string(root.join("calm-dashboards/board.json")).unwrap()).unwrap();
        assert_eq!(file.name, "Calm dashboards");
        assert_eq!(file.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(), vec![d.id.clone(), a.id.clone(), c.id.clone()], "listed first, the rest after");
        assert_eq!(file.items[2].caption, "Dense table, quiet colours");
        let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(root.join("calm-dashboards/board.json")).unwrap()).unwrap();
        assert!(raw["items"][0]["addedAt"].is_i64(), "the stored format is camelCase");
        let board = board_of(&root, "calm-dashboards").unwrap();
        assert_eq!(board.images.len(), 3);
        assert!(board.images[0].path.ends_with(&d.file), "agents get the file to read");
        remove_in(&root, "calm-dashboards", &a.id).unwrap();
        assert!(!root.join("calm-dashboards").join(&a.file).exists(), "removing deletes the copy");
        assert_eq!(list_in(&root).iter().map(|b| (b.id.clone(), b.images)).collect::<Vec<_>>(), vec![("calm-dashboards".to_string(), 2), ("calm-dashboards-2".to_string(), 0)]);
        assert!(add_in(&root, "calm-dashboards", "notes.txt", b"hello, not an image").is_err());
        assert!(open(&root, "nope").unwrap_err().contains("Moodboards: calm-dashboards"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn only_images_are_taken() {
        assert_eq!(image_ext(PNG), Some("png"));
        assert_eq!(image_ext(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]), Some("jpg"));
        assert_eq!(image_ext(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(b"GIF89a......"), Some("gif"));
        assert_eq!(image_ext(b"<svg></svg>.."), None);
    }
}
