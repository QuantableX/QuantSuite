//! Preview-browser profiles (parity 18). The Browser tab is a temporary
//! (InPrivate) page by default; a named profile keeps its cookies, logins and
//! site data in its own WebView2 user-data folder in the pilot data folder
//! (`browser-profiles/<id>/`), so signing in to an app under test once is
//! enough. Each project remembers its profile; agents only ever use the page
//! the user's choice opened and cannot switch profiles. Clearing or deleting
//! touches only that profile's folder — never attachments or history.
//!
//! Importing a session from another browser: Chrome, Edge and Brave (127 and
//! later) protect their cookies with app-bound encryption that only the
//! browser itself can read, so they are listed as unsupported; Firefox is
//! not supported in this build. Nothing is read from another browser.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Project (context id) → profile id; absent means temporary.
    #[serde(default)]
    pub defaults: HashMap<String, String>,
}

pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("browser-profiles")
}

fn store_path(data_dir: &Path) -> PathBuf {
    root(data_dir).join("profiles.json")
}

/// A profile's WebView2 user-data folder.
pub fn data_dir_of(data_dir: &Path, id: &str) -> PathBuf {
    root(data_dir).join(id)
}

/// The profiles; a damaged list is set aside (`profiles.json.damaged`) rather
/// than overwritten, and the profile folders stay where they are.
pub fn load(data_dir: &Path) -> Store {
    let path = store_path(data_dir);
    let Ok(bytes) = std::fs::read(&path) else { return Store::default() };
    match serde_json::from_slice(&bytes) {
        Ok(store) => store,
        Err(_) => {
            let _ = std::fs::rename(&path, path.with_extension("json.damaged"));
            Store::default()
        }
    }
}

fn save(data_dir: &Path, store: &Store) -> Result<(), String> {
    std::fs::create_dir_all(root(data_dir)).map_err(|e| format!("browser profiles folder: {e}"))?;
    let tmp = store_path(data_dir).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, store_path(data_dir)).map_err(|e| e.to_string())
}

fn clean_name(store: &Store, name: &str, except: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err("A profile name has 1 to 40 characters.".into());
    }
    if store.profiles.iter().any(|p| Some(p.id.as_str()) != except && p.name.eq_ignore_ascii_case(name)) {
        return Err(format!("There is already a profile named \"{name}\"."));
    }
    Ok(name.to_string())
}

pub fn create(data_dir: &Path, name: &str, now: i64) -> Result<Profile, String> {
    let mut store = load(data_dir);
    if store.profiles.len() >= 20 {
        return Err("At most 20 browser profiles.".into());
    }
    let name = clean_name(&store, name, None)?;
    let profile = Profile { id: format!("p-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]), name, created_at: now };
    std::fs::create_dir_all(data_dir_of(data_dir, &profile.id)).map_err(|e| format!("could not make the profile's folder: {e}"))?;
    store.profiles.push(profile.clone());
    save(data_dir, &store)?;
    Ok(profile)
}

pub fn rename(data_dir: &Path, id: &str, name: &str) -> Result<Profile, String> {
    let mut store = load(data_dir);
    let name = clean_name(&store, name, Some(id))?;
    let p = store.profiles.iter_mut().find(|p| p.id == id).ok_or("That profile no longer exists.")?;
    p.name = name;
    let out = p.clone();
    save(data_dir, &store)?;
    Ok(out)
}

/// The project's profile; `None` is temporary.
pub fn set_default(data_dir: &Path, context: &str, id: Option<&str>) -> Result<(), String> {
    let mut store = load(data_dir);
    match id {
        Some(id) if !store.profiles.iter().any(|p| p.id == id) => return Err("That profile no longer exists.".into()),
        Some(id) => {
            store.defaults.insert(context.to_string(), id.to_string());
        }
        None => {
            store.defaults.remove(context);
        }
    }
    save(data_dir, &store)
}

pub fn exists(data_dir: &Path, id: &str) -> bool {
    load(data_dir).profiles.iter().any(|p| p.id == id)
}

/// Removes a folder, waiting a while for WebView2's browser processes to
/// let go of it (they end shortly after their last page closes).
fn remove_folder(dir: &Path, wait: Duration) -> Result<(), String> {
    let start = Instant::now();
    loop {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) if start.elapsed() >= wait => {
                return Err(format!("Windows still has the profile's files open ({e}). Close the Browser tab, wait a moment and try again."));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Cookies, logins, storage and cache of one profile go; the profile stays.
pub fn clear(data_dir: &Path, id: &str, wait: Duration) -> Result<(), String> {
    if !exists(data_dir, id) {
        return Err("That profile no longer exists.".into());
    }
    let dir = data_dir_of(data_dir, id);
    remove_folder(&dir, wait)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())
}

/// The profile and its folder go; projects that used it go back to temporary.
pub fn delete(data_dir: &Path, id: &str, wait: Duration) -> Result<(), String> {
    if !exists(data_dir, id) {
        return Err("That profile no longer exists.".into());
    }
    remove_folder(&data_dir_of(data_dir, id), wait)?;
    let mut store = load(data_dir);
    store.profiles.retain(|p| p.id != id);
    store.defaults.retain(|_, v| v != id);
    save(data_dir, &store)
}

// ── Importing a session ──

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSource {
    pub browser: String,
    /// `unsupported` | `not_found`
    pub status: String,
    pub reason: String,
}

/// What other browsers are here, and why their sessions cannot be imported.
pub fn import_sources(local: &Path, roaming: &Path) -> Vec<ImportSource> {
    let chromium = [
        ("Google Chrome", local.join("Google/Chrome/User Data")),
        ("Microsoft Edge", local.join("Microsoft/Edge/User Data")),
        ("Brave", local.join("BraveSoftware/Brave-Browser/User Data")),
    ];
    let mut out = Vec::new();
    for (name, dir) in chromium {
        let state = std::fs::read_to_string(dir.join("Local State")).ok();
        let (status, reason) = match state {
            None => ("not_found", format!("{name} is not installed for this user.")),
            Some(s) if s.contains("app_bound_encrypted_key") => (
                "unsupported",
                format!("{name} protects its cookies with app-bound encryption that only {name} itself can read; QuantPilot does not work around it. Sign in once in a QuantPilot profile instead — it stays signed in."),
            ),
            Some(_) => (
                "unsupported",
                format!("Importing from {name} is not supported: its cookies are encrypted for {name}. Sign in once in a QuantPilot profile instead."),
            ),
        };
        out.push(ImportSource { browser: name.into(), status: status.into(), reason });
    }
    let firefox = roaming.join("Mozilla/Firefox/Profiles");
    out.push(if firefox.is_dir() {
        ImportSource { browser: "Mozilla Firefox".into(), status: "unsupported".into(), reason: "Importing from Firefox is not supported in this version.".into() }
    } else {
        ImportSource { browser: "Mozilla Firefox".into(), status: "not_found".into(), reason: "Firefox is not installed for this user.".into() }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("qp-profiles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn profiles_are_named_kept_apart_and_removed_alone() {
        let data = temp();
        let work = create(&data, " Work ", 1).unwrap();
        assert_eq!(work.name, "Work");
        assert!(data_dir_of(&data, &work.id).is_dir());
        let other = create(&data, "Other", 2).unwrap();
        assert_eq!(create(&data, "work", 3).unwrap_err(), "There is already a profile named \"work\".");
        assert!(create(&data, "", 3).is_err() && create(&data, &"x".repeat(41), 3).is_err());
        assert_eq!(rename(&data, &other.id, "Second").unwrap().name, "Second");
        assert!(rename(&data, &other.id, "WORK").is_err());
        assert!(rename(&data, "p-missing", "x").is_err());

        set_default(&data, "proj-a", Some(&work.id)).unwrap();
        set_default(&data, "proj-b", Some(&other.id)).unwrap();
        assert!(set_default(&data, "proj-c", Some("p-missing")).is_err());
        assert_eq!(load(&data).defaults.get("proj-a"), Some(&work.id));

        // Site data of one profile: clearing it leaves the other alone.
        std::fs::write(data_dir_of(&data, &work.id).join("Cookies"), b"w").unwrap();
        std::fs::write(data_dir_of(&data, &other.id).join("Cookies"), b"o").unwrap();
        clear(&data, &work.id, Duration::ZERO).unwrap();
        assert!(!data_dir_of(&data, &work.id).join("Cookies").exists());
        assert!(data_dir_of(&data, &work.id).is_dir() && data_dir_of(&data, &other.id).join("Cookies").exists());

        // Deleting one: its folder and its project defaults go, nothing else.
        let attachments = data.join("attachments");
        std::fs::create_dir_all(&attachments).unwrap();
        delete(&data, &other.id, Duration::ZERO).unwrap();
        let store = load(&data);
        assert_eq!(store.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Work"]);
        assert_eq!(store.defaults.get("proj-b"), None);
        assert_eq!(store.defaults.get("proj-a"), Some(&work.id));
        assert!(!data_dir_of(&data, &other.id).exists() && attachments.is_dir());
        assert!(delete(&data, &other.id, Duration::ZERO).is_err());
        set_default(&data, "proj-a", None).unwrap();
        assert!(load(&data).defaults.is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_damaged_list_is_set_aside_not_overwritten() {
        let data = temp();
        let p = create(&data, "Kept", 1).unwrap();
        std::fs::write(store_path(&data), b"{ not json").unwrap();
        assert!(load(&data).profiles.is_empty());
        assert_eq!(std::fs::read(store_path(&data).with_extension("json.damaged")).unwrap(), b"{ not json");
        assert!(data_dir_of(&data, &p.id).is_dir(), "the profile's site data stays");
        create(&data, "New", 2).unwrap();
        assert_eq!(load(&data).profiles.len(), 1);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_locked_profile_folder_is_reported_and_kept_listed() {
        let data = temp();
        let p = create(&data, "Locked", 1).unwrap();
        let file = data_dir_of(&data, &p.id).join("Cookies");
        std::fs::write(&file, b"x").unwrap();
        // Windows does not delete a file another handle keeps open without sharing.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
            let err = delete(&data, &p.id, Duration::from_millis(300)).unwrap_err();
            assert!(err.contains("still has the profile's files open"), "{err}");
            assert!(exists(&data, &p.id), "the profile stays listed when its folder could not go");
        }
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn other_browsers_are_reported_never_read() {
        let local = temp();
        let roaming = temp();
        std::fs::create_dir_all(local.join("Google/Chrome/User Data")).unwrap();
        std::fs::write(local.join("Google/Chrome/User Data/Local State"), r#"{"os_crypt":{"app_bound_encrypted_key":"x","encrypted_key":"y"}}"#).unwrap();
        std::fs::create_dir_all(local.join("Microsoft/Edge/User Data")).unwrap();
        std::fs::write(local.join("Microsoft/Edge/User Data/Local State"), r#"{"os_crypt":{"encrypted_key":"y"}}"#).unwrap();
        let s = import_sources(&local, &roaming);
        let by = |b: &str| s.iter().find(|x| x.browser == b).unwrap().clone();
        assert_eq!(by("Google Chrome").status, "unsupported");
        assert!(by("Google Chrome").reason.contains("app-bound encryption"));
        assert_eq!(by("Microsoft Edge").status, "unsupported");
        assert_eq!(by("Brave").status, "not_found");
        assert_eq!(by("Mozilla Firefox").status, "not_found");
        std::fs::create_dir_all(roaming.join("Mozilla/Firefox/Profiles")).unwrap();
        assert_eq!(import_sources(&local, &roaming).iter().find(|x| x.browser == "Mozilla Firefox").unwrap().status, "unsupported");
        let _ = std::fs::remove_dir_all(&local);
        let _ = std::fs::remove_dir_all(&roaming);
    }
}
