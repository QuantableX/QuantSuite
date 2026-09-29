//! The Claude Code auto-memory switch (docs/MEMORY-HOOKS.md). QuantMemory is
//! the only memory (user decision 2026-09-29), so Claude Code's own
//! auto memory — `~/.claude/projects/<project>/memory/`, whose MEMORY.md
//! every session loads — can be turned off from QuantMCP.
//!
//! The setting is `autoMemoryEnabled` in Claude Code's user `settings.json`
//! (the file Claude Code's own `/memory` toggle writes; official memory docs,
//! and observed on 2.1.284: with `"autoMemoryEnabled": false` a session no
//! longer sees the MEMORY.md index). Only that one key is touched, through a
//! concrete syntax tree, and turning it back on restores the file byte for
//! byte while nothing else changed it — the same snapshot scheme as the
//! hooks. The memory files themselves are never touched here.

use super::install::{drop_snapshot, load_snapshot, read_optional, save_snapshot, write, Snapshot};
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SETTING: &str = "autoMemoryEnabled";
const FEATURE: &str = "auto-memory";

const OPTIONS: ParseOptions = ParseOptions {
    allow_comments: true,
    allow_trailing_commas: true,
    allow_loose_object_property_names: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};

/// Claude Code's user settings file.
pub fn settings_path() -> Result<PathBuf, String> {
    Ok(crate::clients::claude_config_dir()
        .ok_or("Could not resolve Claude Code's home")?
        .join("settings.json"))
}

/// The raw JSON text of `autoMemoryEnabled`, if the file sets it.
fn raw_value(text: &str) -> Result<Option<String>, String> {
    let root = CstRootNode::parse(text, &OPTIONS).map_err(|_| "not valid JSON; file left untouched".to_string())?;
    Ok(root
        .object_value()
        .and_then(|object| object.get(SETTING))
        .and_then(|prop| prop.value())
        .map(|value| value.to_string().trim().to_string()))
}

/// Auto memory is on unless the file says `false` (its default is on).
pub fn enabled_in(text: &str) -> Result<bool, String> {
    Ok(raw_value(text)?.as_deref() != Some("false"))
}

/// `text` with `autoMemoryEnabled` set to `false` (added when absent).
fn set_off(text: &str) -> Result<String, String> {
    let root = CstRootNode::parse(text, &OPTIONS).map_err(|_| "not valid JSON; file left untouched".to_string())?;
    let object = root
        .object_value_or_create()
        .ok_or("not a JSON object; file left untouched")?;
    match object.get(SETTING) {
        Some(prop) => prop.set_value(CstInputValue::Bool(false)),
        None => {
            object.append(SETTING, CstInputValue::Bool(false));
        }
    }
    Ok(root.to_string())
}

/// `text` with the setting as it was before we turned it off: its previous
/// raw value, or no key at all.
fn set_back(text: &str, previous: Option<&str>) -> Result<String, String> {
    let root = CstRootNode::parse(text, &OPTIONS).map_err(|_| "not valid JSON; file left untouched".to_string())?;
    let Some(object) = root.object_value() else { return Ok(text.to_string()) };
    let Some(prop) = object.get(SETTING) else { return Ok(text.to_string()) };
    match previous {
        Some("true") => prop.set_value(CstInputValue::Bool(true)),
        // Anything else we did not write ourselves goes back to the default.
        _ => prop.remove(),
    }
    Ok(root.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    TurnedOff,
    AlreadyOff,
    /// Back on, the file byte for byte as before.
    Restored,
    /// Back on in a file that changed since: only the key was put back.
    TurnedOn,
    AlreadyOn,
}

/// Turn Claude Code's auto memory off in `path`.
pub fn turn_off(path: &Path, state_dir: &Path) -> Result<Change, String> {
    let current = read_optional(path)?;
    let text = current.as_deref().unwrap_or("");
    if !enabled_in(text)? {
        return Ok(Change::AlreadyOff);
    }
    let previous = raw_value(text)?;
    let installed = set_off(text)?;
    write(path, &installed)?;
    save_snapshot(
        state_dir,
        path,
        FEATURE,
        &Snapshot { path: path.display().to_string(), original: current, installed, previous },
    )?;
    Ok(Change::TurnedOff)
}

/// Turn it back on — byte for byte when nothing else changed the file.
pub fn turn_on(path: &Path, state_dir: &Path) -> Result<Change, String> {
    let Some(text) = read_optional(path)? else {
        drop_snapshot(state_dir, path, FEATURE);
        return Ok(Change::AlreadyOn);
    };
    let snapshot = load_snapshot(state_dir, path, FEATURE);
    if let Some(snap) = snapshot.as_ref().filter(|snap| snap.installed == text) {
        match &snap.original {
            Some(original) => write(path, original)?,
            None => std::fs::remove_file(path).map_err(|e| format!("Cannot remove {}: {e}", path.display()))?,
        }
        drop_snapshot(state_dir, path, FEATURE);
        return Ok(Change::Restored);
    }
    if enabled_in(&text)? {
        drop_snapshot(state_dir, path, FEATURE);
        return Ok(Change::AlreadyOn);
    }
    write(path, &set_back(&text, snapshot.as_ref().and_then(|s| s.previous.as_deref()))?)?;
    drop_snapshot(state_dir, path, FEATURE);
    Ok(Change::TurnedOn)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("qs-auto-memory-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const SETTINGS: &str = "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"Stop\": []\n  },\n  \"theme\": \"dark\"\n}\n";

    #[test]
    fn off_then_on_restores_the_file_byte_for_byte() {
        let scratch = Scratch::new("exact");
        let state = scratch.0.join("state");
        for (name, original) in [
            ("plain.json", Some(SETTINGS.to_string())),
            ("explicit-true.json", Some("{\r\n  \"autoMemoryEnabled\": true\r\n}".to_string())),
            ("absent.json", None),
        ] {
            let path = scratch.0.join(name);
            if let Some(text) = &original {
                std::fs::write(&path, text).unwrap();
            }
            assert_eq!(turn_off(&path, &state).unwrap(), Change::TurnedOff, "{name}");
            let off = std::fs::read_to_string(&path).unwrap();
            assert!(!enabled_in(&off).unwrap(), "{name}");
            assert_eq!(serde_json::from_str::<serde_json::Value>(&off).unwrap()[SETTING], false, "{name}");
            assert_eq!(turn_off(&path, &state).unwrap(), Change::AlreadyOff, "{name}");
            assert_eq!(turn_on(&path, &state).unwrap(), Change::Restored, "{name}");
            match &original {
                Some(text) => assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes(), "{name}"),
                None => assert!(!path.exists(), "{name}"),
            }
            assert_eq!(turn_on(&path, &state).unwrap(), Change::AlreadyOn, "{name}");
        }
    }

    #[test]
    fn on_after_foreign_edits_puts_back_only_the_key() {
        let scratch = Scratch::new("edited");
        let state = scratch.0.join("state");
        let path = scratch.0.join("settings.json");
        std::fs::write(&path, SETTINGS).unwrap();
        turn_off(&path, &state).unwrap();
        let edited = std::fs::read_to_string(&path).unwrap().replace("\"dark\"", "\"light\"");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(turn_on(&path, &state).unwrap(), Change::TurnedOn);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SETTINGS.replace("\"dark\"", "\"light\""));
    }

    #[test]
    fn a_previous_true_comes_back_as_true() {
        let scratch = Scratch::new("true");
        let state = scratch.0.join("state");
        let path = scratch.0.join("settings.json");
        std::fs::write(&path, "{\"autoMemoryEnabled\": true, \"theme\": \"dark\"}").unwrap();
        turn_off(&path, &state).unwrap();
        let edited = std::fs::read_to_string(&path).unwrap().replace("dark", "light");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(turn_on(&path, &state).unwrap(), Change::TurnedOn);
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value[SETTING], true);
        assert_eq!(value["theme"], "light");
    }

    #[test]
    fn an_invalid_file_is_left_untouched() {
        let scratch = Scratch::new("invalid");
        let path = scratch.0.join("settings.json");
        std::fs::write(&path, "{ broken").unwrap();
        assert!(turn_off(&path, &scratch.0.join("state")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }
}
