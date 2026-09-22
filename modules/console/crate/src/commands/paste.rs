//! Getting a pasted image out of the webview and onto the disk.
//!
//! A terminal moves bytes, and a PTY carries no attachments. What a program like
//! `claude` accepts instead is a **path**, so pasting a screenshot into a pane
//! means writing the clipboard's image somewhere and typing its name.
//!
//! The webview cannot do that half on its own: `plugin:canvas|write_file` takes a
//! `String`, and PNG bytes are not UTF-8. So the frontend hands the image over as
//! base64 and this puts it in the module's own directory.

use base64::Engine as _;
use std::path::PathBuf;

/// How many pasted images are kept before the oldest are dropped.
///
/// They are a side effect of a keystroke, not documents anybody named, and a
/// screenshot is a megabyte. Without a ceiling this directory is a slow leak that
/// nobody thinks to look at.
const KEEP: usize = 40;

/// The largest image accepted, in decoded bytes.
///
/// A guard against a paste that is not what it claims: the frontend already
/// filters by MIME type, and a webview is not a trust boundary this crate should
/// rely on.
const MAX_BYTES: usize = 24 * 1024 * 1024;

fn paste_dir() -> Result<PathBuf, String> {
    let dir = qs_core::paths::module_dir("console").join("pasted");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Delete all but the newest `KEEP` files, oldest first.
///
/// Best effort throughout: a file that cannot be read or removed is skipped
/// rather than failing the paste. Trimming is housekeeping, and housekeeping that
/// can break the thing it tidies is worse than a stale file.
fn trim(dir: &PathBuf) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let modified = entry.metadata().ok()?.modified().ok()?;
            path.is_file().then_some((modified, path))
        })
        .collect();
    if files.len() <= KEEP {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in files.iter().take(files.len() - KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

/// Write a pasted image and return the path to type.
///
/// `extension` is taken from the clipboard's MIME type and sanitised here rather
/// than trusted: it ends up in a filename, and a webview that sends `../..` must
/// not be able to choose where this writes.
#[tauri::command]
pub async fn save_pasted_image(data: String, extension: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("pasted image is not valid base64: {e}"))?;

    if bytes.is_empty() {
        return Err("pasted image is empty".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "pasted image is {} MB; the limit is {} MB",
            bytes.len() / (1024 * 1024),
            MAX_BYTES / (1024 * 1024)
        ));
    }

    let ext: String = extension
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_lowercase();
    let ext = if ext.is_empty() { "png".to_string() } else { ext };

    let dir = paste_dir()?;

    // The name is a timestamp so the file sorts, reads and trims by age without a
    // sidecar index. Nanoseconds because two pastes in one second are ordinary.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("system clock is before the epoch: {e}"))?;
    let name = format!("paste-{}-{:09}.{ext}", stamp.as_secs(), stamp.subsec_nanos());
    let path = dir.join(name);

    std::fs::write(&path, &bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    trim(&dir);

    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1×1 PNG, the smallest real image there is.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    #[tokio::test]
    async fn writes_the_image_and_returns_a_path_that_exists() {
        let path = save_pasted_image(PNG.to_string(), "png".into())
            .await
            .expect("saved");
        let written = std::fs::read(&path).expect("readable");
        assert!(path.ends_with(".png"), "kept the extension: {path}");
        // The PNG signature, so this is the file we were handed and not an
        // accidental re-encode.
        assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");
        let _ = std::fs::remove_file(&path);
    }

    /// The extension lands in a filename, so it is sanitised rather than trusted.
    #[tokio::test]
    async fn refuses_to_take_a_path_from_the_extension() {
        let path = save_pasted_image(PNG.to_string(), "../../evil".into())
            .await
            .expect("saved");
        assert!(!path.contains(".."), "no traversal survives: {path}");
        assert!(path.ends_with(".evil"), "letters kept, separators dropped: {path}");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rejects_input_that_is_not_base64() {
        assert!(save_pasted_image("not base64!!".into(), "png".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rejects_an_empty_image() {
        assert!(save_pasted_image(String::new(), "png".into()).await.is_err());
    }
}
