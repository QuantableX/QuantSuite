//! Visual progress (round 3, card 05; PandaCode's visual-progress): every
//! image of a thread in order — what you attached (and the browser captures
//! you attached), the images in pages the agent rendered, and image files
//! its tools read or wrote — each with its turn and source, for a filmstrip,
//! a before/after compare and pinned references.
//!
//! Nothing is copied: an image is named by a key (`<item id>#<n>`) and read
//! from where the thread already has it — the file, or the page's embedded
//! data. A file that is gone is listed as gone. The pixels a tool returned
//! inline are not kept by the timeline, so they cannot be shown.

use crate::events::{Item, ItemKind};
use crate::{context, now, PilotState};
use base64::Engine;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;

/// Images up to this size are read for the strip and the viewer.
const READ_MAX: u64 = 32 * 1024 * 1024;
/// The strip's thumbnails fit in this box.
const THUMB: u32 = 360;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressImage {
    /// `<item id>#<n>`: the nth image of that timeline item.
    pub key: String,
    pub item_id: String,
    /// 1-based turn number; 0 when the turn is unknown.
    pub turn: usize,
    /// `you` | `browser` | `page` | `tool`.
    pub source: String,
    pub name: String,
    /// The file, for file images (pages' images live in the page).
    pub path: Option<String>,
    pub gone: bool,
    pub at: i64,
}

fn page_images() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r#"(?is)<img\b[^>]*?\bsrc\s*=\s*["'](data:image/[a-z0-9.+-]+;base64,[^"']+)["']"#).expect("page image pattern"))
}

fn tool_paths() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r#"(?i)"(?:file_path|filePath|path|filename|fileName|output|outputPath|savePath|screenshot)"\s*:\s*"((?:[^"\\]|\\.)+?\.(?:png|jpe?g|gif|webp|bmp|svg))""#).expect("tool image pattern")
    })
}

fn file_name(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

/// The thread's images in timeline order. `turns` are the turn ids in order;
/// `exists` says whether a file is still there (a seam for tests).
pub fn collect(items: &[Item], turns: &[String], cwd: &Path, exists: &dyn Fn(&Path) -> bool) -> Vec<ProgressImage> {
    let turn_no = |t: &str| turns.iter().position(|x| x == t).map(|i| i + 1).unwrap_or(0);
    let mut out = Vec::new();
    let mut seen_paths: Vec<String> = Vec::new();
    for item in items {
        let base = |n: usize, source: &str, name: String, path: Option<String>, gone: bool| ProgressImage {
            key: format!("{}#{n}", item.id),
            item_id: item.id.clone(),
            turn: turn_no(&item.turn_id),
            source: source.into(),
            name,
            path,
            gone,
            at: item.created_at,
        };
        match item.kind {
            ItemKind::User => {
                for (n, a) in item.data["attachments"].as_array().into_iter().flatten().enumerate() {
                    if a["kind"].as_str() != Some("image") {
                        continue;
                    }
                    let Some(path) = a["path"].as_str().filter(|p| !p.is_empty()) else {
                        continue;
                    };
                    let name = a["name"].as_str().map(str::to_string).unwrap_or_else(|| file_name(path));
                    // The Browser tab's capture is saved as "screenshot.png".
                    let source = if name == "screenshot.png" { "browser" } else { "you" };
                    out.push(base(n, source, name, Some(path.to_string()), !exists(Path::new(path))));
                }
            }
            ItemKind::Page => {
                let html = item.data["html"].as_str().unwrap_or_default();
                let title = item.data["title"].as_str().unwrap_or("Page").to_string();
                for (n, _) in page_images().captures_iter(html).enumerate() {
                    out.push(base(n, "page", format!("{title} · image {}", n + 1), None, false));
                }
            }
            ItemKind::Tool => {
                let input = item.data["input"].as_str().map(str::to_string).unwrap_or_else(|| item.data["input"].to_string());
                for (n, c) in tool_paths().captures_iter(&input).enumerate() {
                    let raw = c[1].replace("\\\\", "\\").replace("\\/", "/");
                    let p = Path::new(&raw);
                    let full: PathBuf = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
                    let shown = full.to_string_lossy().into_owned();
                    // The same file read again later is one image, at its first use.
                    let norm = shown.replace('\\', "/").to_lowercase();
                    if seen_paths.contains(&norm) {
                        continue;
                    }
                    seen_paths.push(norm);
                    let gone = !exists(&full);
                    out.push(base(n, "tool", file_name(&raw), Some(shown), gone));
                }
            }
            _ => {}
        }
    }
    out
}

/// The image bytes behind a key: its file, or the nth image of a page.
fn bytes_of(items: &[Item], list: &[ProgressImage], key: &str) -> Result<(Vec<u8>, String), String> {
    let img = list.iter().find(|i| i.key == key).ok_or("That image is not in this thread.")?;
    if let Some(path) = &img.path {
        let p = Path::new(path);
        let meta = std::fs::metadata(p).map_err(|_| format!("The image is no longer at {path}."))?;
        if meta.len() > READ_MAX {
            return Err(format!("The image is {} MB — open it in an app instead.", meta.len() / 1024 / 1024));
        }
        let bytes = std::fs::read(p).map_err(|e| format!("The image could not be read ({e})."))?;
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
        return Ok((bytes, ext));
    }
    let item = items.iter().find(|i| i.id == img.item_id).ok_or("That page is gone.")?;
    let n: usize = key.rsplit('#').next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let html = item.data["html"].as_str().unwrap_or_default();
    let uri = page_images().captures_iter(html).nth(n).map(|c| c[1].to_string()).ok_or("That page image is gone.")?;
    let (head, data) = uri.split_once(',').ok_or("A broken page image.")?;
    let ext = head.trim_start_matches("data:image/").split(';').next().unwrap_or("png").replace("svg+xml", "svg").replace("jpeg", "jpg");
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|_| "A broken page image.".to_string())?;
    Ok((bytes, ext))
}

fn mime_of(bytes: &[u8], ext: &str) -> Option<&'static str> {
    Some(match (ext, bytes.get(..4)) {
        (_, Some([0x89, b'P', b'N', b'G'])) => "image/png",
        (_, Some([0xFF, 0xD8, 0xFF, _])) => "image/jpeg",
        (_, Some([b'G', b'I', b'F', b'8'])) => "image/gif",
        (_, Some([b'R', b'I', b'F', b'F'])) => "image/webp",
        ("svg", _) => "image/svg+xml",
        ("bmp", _) => "image/bmp",
        _ => return None,
    })
}

/// A data URL of the image scaled into the strip's box (SVGs as they are).
pub fn thumbnail(bytes: &[u8], ext: &str) -> Option<String> {
    let mime = mime_of(bytes, ext)?;
    if mime == "image/svg+xml" {
        return (bytes.len() < 512 * 1024).then(|| format!("data:{mime};base64,{}", context::b64(bytes)));
    }
    let img = image::load_from_memory(bytes).ok()?;
    let small = if img.width() > THUMB || img.height() > THUMB { img.thumbnail(THUMB, THUMB) } else { img };
    let mut png = Vec::new();
    small.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", context::b64(&png)))
}

fn scan(state: &PilotState, id: &str) -> Result<(Vec<Item>, Vec<ProgressImage>), String> {
    let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let items = state.db.items(id, None, 5000).map_err(|e| e.to_string())?;
    let turns: Vec<String> = state.db.turn_refs(id).unwrap_or_default().into_iter().map(|(t, _)| t).collect();
    let list = collect(&items, &turns, Path::new(&meta.cwd), &|p| p.exists());
    Ok((items, list))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub images: Vec<ProgressImage>,
    /// Pinned keys that still name an image, in pin order.
    pub pins: Vec<String>,
}

#[tauri::command(async)]
pub fn pilot_progress(state: State<'_, PilotState>, id: String) -> Result<Progress, String> {
    let (_, images) = scan(&state, &id)?;
    let pins = state.db.pins(&id).map_err(|e| e.to_string())?.into_iter().filter(|k| images.iter().any(|i| &i.key == k)).collect();
    Ok(Progress { images, pins })
}

/// Thumbnails for the strip, key → data URL (none: gone or unreadable).
#[tauri::command(async)]
pub fn pilot_progress_thumbs(state: State<'_, PilotState>, id: String, keys: Vec<String>) -> Result<HashMap<String, Option<String>>, String> {
    if keys.len() > 60 {
        return Err("at most 60 thumbnails at a time".into());
    }
    let (items, list) = scan(&state, &id)?;
    Ok(keys.into_iter().map(|k| {
        let thumb = bytes_of(&items, &list, &k).ok().and_then(|(b, ext)| thumbnail(&b, &ext));
        (k, thumb)
    })
    .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub mime: String,
    pub data: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// One image at full size, for the viewer and the compare.
#[tauri::command(async)]
pub fn pilot_progress_image(state: State<'_, PilotState>, id: String, key: String) -> Result<Media, String> {
    let (items, list) = scan(&state, &id)?;
    let (bytes, ext) = bytes_of(&items, &list, &key)?;
    let mime = mime_of(&bytes, &ext).ok_or("This file is not an image the viewer shows; open it in an app.")?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map(|(w, h)| (Some(w), Some(h)))
        .unwrap_or((None, None));
    Ok(Media { mime: mime.into(), size: bytes.len() as u64, data: context::b64(&bytes), width, height })
}

/// Pins an image as a reference of the thread (shown first), or unpins it.
#[tauri::command(async)]
pub fn pilot_progress_pin(state: State<'_, PilotState>, id: String, key: String, pinned: bool) -> Result<Vec<String>, String> {
    state.db.set_pin(&id, &key, pinned, now()).map_err(|e| e.to_string())?;
    state.db.pins(&id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, turn: &str, kind: ItemKind, data: serde_json::Value, at: i64) -> Item {
        Item { id: id.into(), thread_id: "t".into(), turn_id: turn.into(), seq: at, kind, parent: None, data, created_at: at, updated_at: at }
    }

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(800, 400, image::Rgba([10, 120, 200, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png
    }

    #[test]
    fn images_come_from_attachments_pages_and_tools_in_order() {
        let png = context::b64(&tiny_png());
        let items = vec![
            item("u1", "turn-a", ItemKind::User, json!({ "text": "look", "attachments": [
                { "kind": "image", "name": "mock.png", "path": "C:/att/mock.png" },
                { "kind": "file", "name": "notes.md", "path": "C:/att/notes.md" },
                { "kind": "image", "name": "screenshot.png", "path": "C:/att/screenshot-1a2b.png" }
            ] }), 1),
            item("p1", "turn-a", ItemKind::Page, json!({ "title": "Mock", "html": format!("<p>x</p><img alt=a src=\"data:image/png;base64,{png}\"><IMG SRC='data:image/png;base64,{png}'><img src=\"https://x/y.png\">") }), 2),
            item("k1", "turn-b", ItemKind::Tool, json!({ "name": "Read", "input": "{\n  \"file_path\": \"shots\\\\after.png\"\n}" }), 3),
            item("k2", "turn-b", ItemKind::Tool, json!({ "name": "Read", "input": "{\"file_path\": \"C:\\\\proj\\\\shots\\\\after.png\"}" }), 4),
            item("k3", "turn-b", ItemKind::Tool, json!({ "name": "Bash", "input": "{\"command\": \"ls *.png\"}" }), 5),
            item("a1", "turn-b", ItemKind::Assistant, json!({ "text": "done ![x](y.png)" }), 6),
        ];
        let turns = vec!["turn-a".to_string(), "turn-b".to_string()];
        let list = collect(&items, &turns, Path::new("C:/proj"), &|p| !p.to_string_lossy().contains("mock"));
        let got: Vec<(String, usize, String, bool)> = list.iter().map(|i| (i.key.clone(), i.turn, i.source.clone(), i.gone)).collect();
        assert_eq!(
            got,
            vec![
                ("u1#0".into(), 1, "you".into(), true),
                ("u1#2".into(), 1, "browser".into(), false),
                ("p1#0".into(), 1, "page".into(), false),
                ("p1#1".into(), 1, "page".into(), false),
                ("k1#0".into(), 2, "tool".into(), false),
            ],
            "attachments, a page's embedded images (not remote ones), a tool's image file once"
        );
        assert_eq!(list[0].name, "mock.png");
        assert_eq!(list[2].name, "Mock · image 1");
        assert!(list[4].path.as_deref().unwrap().replace('\\', "/").ends_with("proj/shots/after.png"), "relative to the thread's folder");
        // A page image is read from the page itself.
        let (bytes, ext) = bytes_of(&items, &list, "p1#1").unwrap();
        assert_eq!(ext, "png");
        assert_eq!(bytes, tiny_png());
        assert!(bytes_of(&items, &list, "u1#1").is_err(), "a file attachment is no image");
    }

    #[test]
    fn thumbnails_fit_the_box() {
        let t = thumbnail(&tiny_png(), "png").unwrap();
        let b = base64::engine::general_purpose::STANDARD.decode(t.trim_start_matches("data:image/png;base64,")).unwrap();
        let img = image::load_from_memory(&b).unwrap();
        assert_eq!((img.width(), img.height()), (360, 180));
        assert!(thumbnail(b"<svg xmlns='http://www.w3.org/2000/svg'/>", "svg").unwrap().starts_with("data:image/svg+xml;base64,"));
        assert_eq!(thumbnail(b"not an image", "txt"), None);
    }

    #[test]
    fn pins_are_kept_per_thread_in_order() {
        let db = crate::db::Database::open_in_memory().unwrap();
        db.set_pin("t1", "a#0", true, 2).unwrap();
        db.set_pin("t1", "b#1", true, 1).unwrap();
        db.set_pin("t1", "a#0", true, 9).unwrap();
        db.set_pin("t2", "c#0", true, 1).unwrap();
        assert_eq!(db.pins("t1").unwrap(), vec!["b#1".to_string(), "a#0".to_string()]);
        db.set_pin("t1", "b#1", false, 3).unwrap();
        assert_eq!(db.pins("t1").unwrap(), vec!["a#0".to_string()]);
    }
}
