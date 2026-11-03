//! Images for the user's own agents (round 2, card 01). The built-in agents
//! carry bundled logos in the app; a custom agent can take an image file
//! instead of its letter. The file is checked (a PNG, JPEG or WebP that
//! decodes, at most 256 KB, at most 1024 px a side), copied into the pilot
//! data dir as `agent-icons/<id>.<ext>` and handed to the app as a data URL.

use base64::Engine;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MAX_BYTES: u64 = 256 * 1024;
const MAX_SIDE: u32 = 1024;
const DIR: &str = "agent-icons";
const KINDS: &[(&str, &str)] = &[("png", "image/png"), ("jpg", "image/jpeg"), ("webp", "image/webp")];

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

/// Agent ids are slugs (settings normalize them); anything else never names a file.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The stored format of a file's bytes, after decoding them once.
fn check(bytes: &[u8]) -> Result<&'static str, String> {
    let ext = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::Jpeg) => "jpg",
        Ok(image::ImageFormat::WebP) => "webp",
        _ => return Err("the icon must be a PNG, JPEG or WebP image".into()),
    };
    let img = image::load_from_memory(bytes).map_err(|e| format!("the image does not open: {e}"))?;
    if img.width() == 0 || img.height() == 0 || img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        return Err(format!("the image is {}×{} px; at most {MAX_SIDE} px a side", img.width(), img.height()));
    }
    Ok(ext)
}

fn data_url(ext: &str, bytes: &[u8]) -> String {
    let mime = KINDS.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m).unwrap_or("image/png");
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Remove every stored image of `id`, whatever its extension.
pub fn clear(data_dir: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("not an agent id".into());
    }
    for (ext, _) in KINDS {
        let path = dir(data_dir).join(format!("{id}.{ext}"));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Check `source` and store it as `id`'s icon; returns the data URL.
pub fn set(data_dir: &Path, id: &str, source: &Path) -> Result<String, String> {
    if !valid_id(id) {
        return Err("give the agent an id first (letters, digits, dashes)".into());
    }
    let len = std::fs::metadata(source).map_err(|e| format!("{}: {e}", source.display()))?.len();
    if len > MAX_BYTES {
        return Err(format!("the image is {} KB; at most {} KB", len.div_ceil(1024), MAX_BYTES / 1024));
    }
    let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let ext = check(&bytes)?;
    std::fs::create_dir_all(dir(data_dir)).map_err(|e| e.to_string())?;
    clear(data_dir, id)?;
    let path = dir(data_dir).join(format!("{id}.{ext}"));
    let tmp = path.with_extension(format!("{ext}.tmp"));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(data_url(ext, &bytes))
}

/// Every stored icon whose agent still exists: id → data URL. Files of
/// removed agents, unreadable or no longer valid files are skipped.
pub fn all(data_dir: &Path, ids: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for id in ids.iter().filter(|id| valid_id(id)) {
        for (ext, _) in KINDS {
            let path = dir(data_dir).join(format!("{id}.{ext}"));
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.len() > MAX_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if check(&bytes).ok() == Some(*ext) {
                out.insert(id.clone(), data_url(ext, &bytes));
                break;
            }
        }
    }
    out
}

/// After a settings save: drop the images of agents that no longer exist.
pub fn prune(data_dir: &Path, ids: &[String]) {
    let Ok(entries) = std::fs::read_dir(dir(data_dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stem = name.split('.').next().unwrap_or_default();
        if !ids.iter().any(|id| id == stem) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([217, 119, 87, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qp-agent-icons-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn stores_a_valid_image_and_lists_it() {
        let data = temp();
        let src = data.join("logo.png");
        std::fs::write(&src, png(32, 32)).unwrap();
        let url = set(&data, "my-agent", &src).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(data.join("agent-icons/my-agent.png").exists());
        let listed = all(&data, &["my-agent".into(), "other".into()]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed["my-agent"], url);
        // Removed agents lose their image on the next save.
        prune(&data, &["other".into()]);
        assert!(!data.join("agent-icons/my-agent.png").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn refuses_what_is_not_a_small_image() {
        let data = temp();
        let text = data.join("notes.png");
        std::fs::write(&text, b"not an image at all").unwrap();
        assert!(set(&data, "a", &text).unwrap_err().contains("PNG, JPEG or WebP"));
        let big = data.join("big.png");
        std::fs::write(&big, vec![0u8; (MAX_BYTES + 1) as usize]).unwrap();
        assert!(set(&data, "a", &big).unwrap_err().contains("at most 256 KB"));
        let huge = data.join("huge.png");
        std::fs::write(&huge, png(1100, 4)).unwrap();
        assert!(set(&data, "a", &huge).unwrap_err().contains("at most 1024 px"));
        // Ids never become paths outside the folder.
        let ok = data.join("ok.png");
        std::fs::write(&ok, png(8, 8)).unwrap();
        assert!(set(&data, "../x", &ok).is_err());
        assert!(set(&data, "", &ok).is_err());
        assert!(clear(&data, "..").is_err());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_new_image_replaces_the_old_one_of_another_format() {
        let data = temp();
        let a = data.join("a.png");
        std::fs::write(&a, png(8, 8)).unwrap();
        set(&data, "x", &a).unwrap();
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]));
        let mut jpg = std::io::Cursor::new(Vec::new());
        img.write_to(&mut jpg, image::ImageFormat::Jpeg).unwrap();
        let b = data.join("b.jpg");
        std::fs::write(&b, jpg.into_inner()).unwrap();
        let url = set(&data, "x", &b).unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(!data.join("agent-icons/x.png").exists());
        assert!(data.join("agent-icons/x.jpg").exists());
        clear(&data, "x").unwrap();
        assert!(all(&data, &["x".into()]).is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }
}
