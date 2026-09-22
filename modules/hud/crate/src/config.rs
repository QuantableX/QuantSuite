use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The standalone app also had a `NoConfigDir` variant, because it resolved the
/// directory with `dirs::config_dir()`, which returns an `Option`. The suite
/// uses [`qs_core::paths::module_dir`], which cannot fail, so that variant lost
/// its only construction site in the migration and is gone.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// `~/.quantsuite/modules/hud/config.json`.
///
/// The standalone app stored this in `<config_dir>/quanthub` — note the typo,
/// `quanthub` not `quanthud`. [`import_legacy_config`] reads that name once and
/// the misspelling ends there.
fn get_config_path() -> Result<PathBuf, ConfigError> {
    let app_dir = qs_core::paths::module_dir("hud");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir)?;
    }
    Ok(app_dir.join("config.json"))
}

/// One-time copy of the standalone app's config. Never moves it.
pub fn import_legacy_config() {
    let Ok(target) = get_config_path() else { return };
    let dir = target.parent().map(PathBuf::from).unwrap_or_default();
    let marker = dir.join(".migrated");
    if marker.exists() {
        return;
    }

    if let Some(legacy_dir) = dirs::config_dir() {
        let legacy = legacy_dir.join("quanthub").join("config.json");
        if legacy.exists() && !target.exists() {
            if let Err(e) = fs::copy(&legacy, &target) {
                eprintln!("hud: legacy config import failed: {e}");
                return;
            }
        }
    }

    let _ = fs::write(&marker, chrono::Utc::now().to_rfc3339());
}

pub fn load_config() -> Result<String, ConfigError> {
    load_config_from(&get_config_path()?)
}

/// The file's text when it is JSON, else the `.bak` [`save_config_to`] kept
/// before its last replace. A file that does not parse here is torn or empty
/// (a crash before atomic writes landed, a disk error): handed to the frontend
/// as is, `useConfig` runs on defaults and the next read-modify-write saves
/// `{}` over the user's clipboard, transcript and todo history. Without a
/// usable backup the text is returned unchanged, and the frontend logs the
/// parse failure as before.
fn load_config_from(path: &Path) -> Result<String, ConfigError> {
    if !path.exists() {
        return Ok("{}".to_string());
    }
    let text = fs::read_to_string(path)?;
    if is_json(&text) {
        return Ok(text);
    }
    let bak = qs_core::paths::backup_path(path);
    match fs::read_to_string(&bak) {
        Ok(fallback) if is_json(&fallback) => {
            log::warn!("hud: {} is not valid JSON — loading {}", path.display(), bak.display());
            Ok(fallback)
        }
        _ => Ok(text),
    }
}

fn is_json(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

pub fn save_config(config: &str) -> Result<(), ConfigError> {
    save_config_to(&get_config_path()?, config)
}

/// Atomic replace with the previous content kept as `.bak`. The frontend
/// queue rewrites this one file several times a minute (the clipboard poll),
/// so a plain `fs::write` left a truncate-then-fill window on the whole HUD
/// state every 1.5 s.
fn save_config_to(path: &Path, config: &str) -> Result<(), ConfigError> {
    qs_core::paths::write_atomic_with_backup(path, config.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qs-hud-config-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn torn_or_empty_config_falls_back_to_the_backup() {
        let dir = scratch("fallback");
        let path = dir.join("config.json");
        assert_eq!(load_config_from(&path).unwrap(), "{}");
        save_config_to(&path, r#"{"todos":[1]}"#).unwrap();
        save_config_to(&path, r#"{"todos":[1,2]}"#).unwrap();
        assert_eq!(load_config_from(&path).unwrap(), r#"{"todos":[1,2]}"#);
        // A write cut off half way, then the empty file `fs::write` used to leave.
        fs::write(&path, r#"{"todos":[1,"#).unwrap();
        assert_eq!(load_config_from(&path).unwrap(), r#"{"todos":[1]}"#);
        fs::write(&path, "").unwrap();
        assert_eq!(load_config_from(&path).unwrap(), r#"{"todos":[1]}"#);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparseable_config_without_a_backup_is_returned_as_is() {
        let dir = scratch("no-backup");
        let path = dir.join("config.json");
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_config_from(&path).unwrap(), "not json");
        let _ = fs::remove_dir_all(&dir);
    }
}

