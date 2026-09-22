//! Private workspace state. The registry remains in core.db; layouts, notes
//! and specs live in `<suite-home>/workspaces/<workspace-key>/canvas/`.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

// Migration and layout writes must not race across the suite's webviews.
static STORAGE_LOCK: Mutex<()> = Mutex::new(());

fn io_error(e: std::io::Error) -> String {
    format!("Workspace storage: {e}")
}

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

// Never follow a junction/symlink during migration. Leave unusual layouts in
// place with an error, instead of moving or deleting someone else's files.
fn inventory(dir: &Path, root: &Path, files: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) -> Result<(), String> {
    if is_link(&fs::symlink_metadata(dir).map_err(io_error)?) {
        return Err(format!("Workspace storage: cannot migrate linked directory {}", dir.display()));
    }
    dirs.push(dir.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf());
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        let meta = fs::symlink_metadata(&path).map_err(io_error)?;
        if is_link(&meta) {
            return Err(format!("Workspace storage: cannot migrate linked path {}", path.display()));
        }
        if meta.is_dir() {
            inventory(&path, root, files, dirs)?;
        } else if meta.is_file() {
            files.push(path.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf());
        } else {
            return Err(format!("Workspace storage: unsupported file {}", path.display()));
        }
    }
    Ok(())
}

/// Copy and verify the entire legacy directory before removing any source.
/// Existing internal files win; conflicting and unknown legacy data remain in
/// a separate complete archive. Interrupted migrations are safe to repeat.
fn migrate_legacy(folder: &Path, data: &Path) -> Result<(), String> {
    let legacy = folder.join(".quantcode");
    match fs::symlink_metadata(&legacy) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_error(e)),
        Ok(_) => {}
    }
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    inventory(&legacy, &legacy, &mut files, &mut dirs)?;
    let archive = data.parent().ok_or("Missing workspace storage parent")?
        .join("legacy").join(format!("quantcode-{}", uuid::Uuid::new_v4()));
    for rel in &dirs {
        fs::create_dir_all(archive.join(rel)).map_err(io_error)?;
    }
    for rel in &files {
        fs::copy(legacy.join(rel), archive.join(rel)).map_err(io_error)?;
        fs::OpenOptions::new().write(true).open(archive.join(rel)).map_err(io_error)?.sync_all().map_err(io_error)?;
    }
    for rel in &files {
        if fs::read(legacy.join(rel)).map_err(io_error)? != fs::read(archive.join(rel)).map_err(io_error)? {
            return Err(format!("Legacy data changed during migration: {}. Original retained.", rel.display()));
        }
    }
    for rel in &dirs {
        fs::create_dir_all(data.join(rel)).map_err(io_error)?;
    }
    for rel in &files {
        let dest = data.join(rel);
        if !dest.try_exists().map_err(io_error)? {
            let bytes = fs::read(archive.join(rel)).map_err(io_error)?;
            qs_core::paths::write_atomic(&dest, &bytes).map_err(io_error)?;
        }
    }
    for rel in &files {
        let src = legacy.join(rel);
        if is_link(&fs::symlink_metadata(&src).map_err(io_error)?)
            || fs::read(&src).map_err(io_error)? != fs::read(archive.join(rel)).map_err(io_error)? {
            return Err(format!("Legacy data changed during migration: {}. Archive: {}", src.display(), archive.display()));
        }
        fs::remove_file(src).map_err(io_error)?;
    }
    // Only remove empty directories. New files written by an older running
    // app are retained and cause an error; no recursive source deletion.
    for rel in dirs.iter().rev() {
        fs::remove_dir(legacy.join(rel)).map_err(io_error)?;
    }
    Ok(())
}

fn storage_dir(folder: &str) -> Result<PathBuf, String> {
    if !Path::new(folder).is_absolute() {
        return Err("Workspace storage requires an absolute workspace path".into());
    }
    let data = qs_core::paths::workspace_dir(folder).join("canvas");
    migrate_legacy(Path::new(folder), &data)?;
    fs::create_dir_all(&data).map_err(io_error)?;
    Ok(data)
}

#[tauri::command]
pub async fn workspace_storage_dir(folder_path: String) -> Result<String, String> {
    let _guard = STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    Ok(storage_dir(&folder_path)?.to_string_lossy().into_owned())
}

/// Saved tabs and linked specs can still contain legacy absolute paths. Resolve
/// those too, so saving an old tab cannot recreate .quantcode.
pub(super) fn resolve_legacy_path(path: &str) -> Result<String, String> {
    let original = Path::new(path);
    for ancestor in original.ancestors() {
        if ancestor.file_name().is_some_and(|n| n == ".quantcode") {
            let folder = ancestor.parent().ok_or("Missing legacy workspace")?;
            let rel = original.strip_prefix(ancestor).map_err(|e| e.to_string())?;
            if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
                return Err("Invalid relative workspace storage path".into());
            }
            let _guard = STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
            return Ok(storage_dir(&folder.to_string_lossy())?.join(rel).to_string_lossy().into_owned());
        }
    }
    Ok(path.to_string())
}

#[tauri::command]
pub async fn load_canvas_state(folder_path: String) -> Result<String, String> {
    let _guard = STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let file = storage_dir(&folder_path)?.join("canvas.json");
    match fs::read_to_string(file) {
        Ok(data) => Ok(data),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(r#"{"tabs":[],"activeTab":null}"#.into()),
        Err(e) => Err(io_error(e)),
    }
}

#[tauri::command]
pub async fn save_canvas_state(folder_path: String, data: String) -> Result<(), String> {
    let _guard = STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let file = storage_dir(&folder_path)?.join("canvas.json");
    qs_core::paths::write_atomic_with_backup(&file, data.as_bytes()).map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_all_files_conflicts_and_is_repeatable() {
        let root = std::env::temp_dir().join(format!("qs-canvas-migrate-{}", uuid::Uuid::new_v4()));
        let folder = root.join("project");
        let legacy = folder.join(".quantcode");
        let data = root.join("private/canvas");
        fs::create_dir_all(legacy.join("specs")).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(legacy.join("canvas.json"), "old layout").unwrap();
        fs::write(legacy.join("NOTES.md"), "my notes").unwrap();
        fs::write(legacy.join("specs/one.spec.md"), "my spec").unwrap();
        fs::write(legacy.join("unknown.bin"), [0, 255, 10]).unwrap();
        fs::write(data.join("canvas.json"), "new layout").unwrap();
        migrate_legacy(&folder, &data).unwrap();
        assert!(!legacy.exists());
        assert_eq!(fs::read_to_string(data.join("canvas.json")).unwrap(), "new layout");
        assert_eq!(fs::read_to_string(data.join("NOTES.md")).unwrap(), "my notes");
        assert_eq!(fs::read_to_string(data.join("specs/one.spec.md")).unwrap(), "my spec");
        assert_eq!(fs::read(data.join("unknown.bin")).unwrap(), [0, 255, 10]);
        let archives: Vec<_> = fs::read_dir(root.join("private/legacy")).unwrap().collect();
        assert_eq!(archives.len(), 1);
        let backup = archives[0].as_ref().unwrap().path();
        assert_eq!(fs::read_to_string(backup.join("canvas.json")).unwrap(), "old layout");
        migrate_legacy(&folder, &data).unwrap();
        assert_eq!(fs::read_dir(root.join("private/legacy")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_import_retains_originals_and_verified_archive() {
        let root = std::env::temp_dir().join(format!("qs-canvas-fail-{}", uuid::Uuid::new_v4()));
        let legacy = root.join("project/.quantcode");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("NOTES.md"), "irreplaceable").unwrap();
        let data = root.join("private/canvas");
        fs::create_dir_all(data.parent().unwrap()).unwrap();
        fs::write(&data, "not a directory").unwrap();
        assert!(migrate_legacy(&root.join("project"), &data).is_err());
        assert_eq!(fs::read_to_string(legacy.join("NOTES.md")).unwrap(), "irreplaceable");
        assert_eq!(fs::read_dir(root.join("private/legacy")).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn migration_does_not_follow_legacy_junctions() {
        let root = std::env::temp_dir().join(format!("qs-canvas-link-{}", uuid::Uuid::new_v4()));
        let folder = root.join("project");
        let shared = root.join("shared");
        fs::create_dir_all(&folder).unwrap();
        fs::create_dir(&shared).unwrap();
        fs::write(shared.join("NOTES.md"), "shared notes").unwrap();
        let link = folder.join(".quantcode");
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"]).arg(&link).arg(&shared).output().unwrap();
        assert!(result.status.success());
        assert!(migrate_legacy(&folder, &root.join("private/canvas")).is_err());
        assert_eq!(fs::read_to_string(shared.join("NOTES.md")).unwrap(), "shared notes");
        fs::remove_dir(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

}
