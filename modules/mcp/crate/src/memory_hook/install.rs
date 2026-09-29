//! Put the QuantMemory prompt hook into an agent's own hook config, and take
//! it out again (docs/MEMORY-HOOKS.md).
//!
//! Claude Code's `settings.json` and Codex's `hooks.json` share one shape —
//! `hooks.<Event>[] = { "hooks": [handler] }` — so one installer serves both.
//! It only ever appends its own group; foreign hooks (Orca, QuantPilot's
//! `--settings`, the user's) are never touched, and the edit goes through a
//! concrete syntax tree, so formatting and comments elsewhere survive.
//!
//! Uninstall restores the file byte for byte: install keeps a snapshot of the
//! file before and after its edit, and when the file is still exactly what
//! install wrote, the snapshot's "before" goes back (or the file goes away if
//! install created it). When something else changed the file since, only our
//! handlers are cut out and everything else stays as it now is.

use super::{HookClient, HookEvent, ROUTE_SEGMENT};
use jsonc_parser::cst::{CstInputValue, CstNode, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Seconds the agent grants the hook before cancelling it. curl already
/// gives up after 2 s (and after 0.5 s when nothing listens), so this only
/// bounds a wedged shell.
pub const HOOK_TIMEOUT_SECS: u32 = 5;

const OPTIONS: ParseOptions = ParseOptions {
    allow_comments: true,
    allow_trailing_commas: true,
    allow_loose_object_property_names: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};

/// Where the hook posts: loopback only, the port of this QuantMCP.
pub fn url(port: u16, client: HookClient, event: HookEvent) -> String {
    format!("http://127.0.0.1:{port}/{ROUTE_SEGMENT}/{}/{}", client.slug(), event.slug())
}

/// The hook command. One text for every shell the agents use — Git Bash and
/// PowerShell on Windows (Claude Code picks either), sh elsewhere: `;` and
/// `exit 0` mean the same in all of them. The event JSON goes in on stdin,
/// the answer comes out on stdout. `--connect-timeout 0.5` matters on
/// Windows, where a connect to a closed loopback port retries for ~2 s;
/// `-f` drops any error page, and `exit 0` keeps a stopped suite silent — a
/// non-zero exit would show a "hook error" notice on every prompt.
pub fn command(port: u16, client: HookClient, event: HookEvent) -> String {
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    format!(
        "{curl} -sf --connect-timeout 0.5 -m 2 --data-binary \"@-\" {}; exit 0",
        url(port, client, event)
    )
}

/// One of ours: a command that posts to the loopback QuantMemory route, on
/// any port (a dev build's 3101 as much as the release's 3100).
pub fn is_ours(command: &str) -> bool {
    command.contains("://127.0.0.1:") && command.contains(&format!("/{ROUTE_SEGMENT}/"))
}

fn handler(port: u16, client: HookClient, event: HookEvent) -> CstInputValue {
    CstInputValue::Object(vec![
        ("type".into(), "command".into()),
        ("command".into(), command(port, client, event).into()),
        ("timeout".into(), HOOK_TIMEOUT_SECS.into()),
    ])
}

/// `text` with one group of ours appended per event the client hooks into.
/// Creates `hooks` and the event arrays when missing; refuses (untouched)
/// anything that is not the documented shape.
pub fn add_hooks(text: &str, port: u16, client: HookClient) -> Result<String, String> {
    let root = CstRootNode::parse(text, &OPTIONS).map_err(|_| "not valid JSON; file left untouched".to_string())?;
    let object = root
        .object_value_or_create()
        .ok_or("not a JSON object; file left untouched")?;
    let hooks = object
        .object_value_or_create("hooks")
        .ok_or("`hooks` is not an object; file left untouched")?;
    for event in client.events() {
        let groups = hooks
            .array_value_or_create(event.name())
            .ok_or_else(|| format!("`hooks.{}` is not an array; file left untouched", event.name()))?;
        groups.append(CstInputValue::Object(vec![(
            "hooks".into(),
            CstInputValue::Array(vec![handler(port, client, *event)]),
        )]));
    }
    Ok(root.to_string())
}

fn handler_is_ours(node: &CstNode) -> bool {
    node.as_object()
        .and_then(|handler| handler.get("command"))
        .and_then(|prop| prop.value())
        .and_then(|value| value.as_string_lit())
        .and_then(|lit| lit.decoded_value().ok())
        .is_some_and(|command| is_ours(&command))
}

/// `text` without our handlers, or `None` when it holds none. A group or an
/// event list that only held ours goes with them, and so does a `hooks`
/// object left empty.
pub fn remove_hooks(text: &str) -> Result<Option<String>, String> {
    let root = CstRootNode::parse(text, &OPTIONS).map_err(|_| "not valid JSON; file left untouched".to_string())?;
    let Some(hooks) = root.object_value().and_then(|object| object.object_value("hooks")) else {
        return Ok(None);
    };
    let mut removed = false;
    for event in hooks.properties() {
        let Some(groups) = event.array_value() else { continue };
        let mut event_touched = false;
        for group in groups.elements() {
            let Some(handlers) = group.as_object().and_then(|g| g.array_value("hooks")) else { continue };
            let ours: Vec<CstNode> = handlers.elements().into_iter().filter(handler_is_ours).collect();
            if ours.is_empty() {
                continue;
            }
            ours.into_iter().for_each(CstNode::remove);
            if handlers.elements().is_empty() {
                group.remove();
            }
            event_touched = true;
        }
        if event_touched && groups.elements().is_empty() {
            event.remove();
        }
        removed |= event_touched;
    }
    if !removed {
        return Ok(None);
    }
    if hooks.properties().is_empty() {
        if let Some(prop) = root.object_value().and_then(|object| object.get("hooks")) {
            prop.remove();
        }
    }
    Ok(Some(root.to_string()))
}

/// What install / uninstall did to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Installed,
    UpToDate,
    /// Uninstalled, the file is byte for byte what it was before install
    /// (or gone again, when install created it).
    Restored,
    /// Uninstalled from a file that changed since install: only our handlers
    /// were cut out.
    Removed,
    NotInstalled,
}

/// The file before and after our edit, so uninstall can put it back exactly.
#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    path: String,
    /// `None`: the file did not exist.
    original: Option<String>,
    installed: String,
}

fn fnv1a64(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

fn snapshot_file(state_dir: &Path, target: &Path) -> PathBuf {
    let key = qs_core::workspaces::normalize_path(&target.to_string_lossy());
    state_dir.join(format!("{:016x}.json", fnv1a64(&key)))
}

fn load_snapshot(state_dir: &Path, target: &Path) -> Option<Snapshot> {
    let text = std::fs::read_to_string(snapshot_file(state_dir, target)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_snapshot(state_dir: &Path, target: &Path, snapshot: &Snapshot) -> Result<(), String> {
    std::fs::create_dir_all(state_dir).map_err(|e| format!("Cannot create {}: {e}", state_dir.display()))?;
    let text = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
    let file = snapshot_file(state_dir, target);
    qs_core::paths::write_atomic(&file, text.as_bytes()).map_err(|e| format!("Cannot write {}: {e}", file.display()))
}

fn drop_snapshot(state_dir: &Path, target: &Path) {
    let _ = std::fs::remove_file(snapshot_file(state_dir, target));
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    qs_core::paths::write_atomic_with_backup(path, text.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

/// The file as it would be without our hooks: the snapshot's original while
/// the file is untouched since install, else the file minus our handlers.
fn without_ours(current: Option<&str>, snapshot: Option<&Snapshot>) -> Result<Option<String>, String> {
    match (current, snapshot) {
        (Some(text), Some(snap)) if snap.installed == text => Ok(snap.original.clone()),
        (Some(text), _) => Ok(Some(remove_hooks(text)?.unwrap_or_else(|| text.to_string()))),
        (None, _) => Ok(None),
    }
}

/// Whether `path` holds a hook of ours.
pub fn installed_in(path: &Path) -> Result<bool, String> {
    Ok(match read_optional(path)? {
        Some(text) => remove_hooks(&text)?.is_some(),
        None => false,
    })
}

/// Install (or re-point, after a port change) our hook in `path`.
pub fn install_file(path: &Path, state_dir: &Path, port: u16, client: HookClient) -> Result<Change, String> {
    let current = read_optional(path)?;
    let snapshot = load_snapshot(state_dir, path);
    let base = without_ours(current.as_deref(), snapshot.as_ref())?;
    let desired = add_hooks(base.as_deref().unwrap_or(""), port, client)?;
    let change = if current.as_deref() == Some(desired.as_str()) {
        Change::UpToDate
    } else {
        write(path, &desired)?;
        Change::Installed
    };
    // Kept (or refreshed) either way: a snapshot lost to a crash must not
    // cost the exact restore of a file we still own unchanged.
    save_snapshot(
        state_dir,
        path,
        &Snapshot { path: path.display().to_string(), original: base, installed: desired },
    )?;
    Ok(change)
}

/// Take our hook out of `path` — byte for byte when nothing else changed it.
pub fn uninstall_file(path: &Path, state_dir: &Path) -> Result<Change, String> {
    let current = read_optional(path)?;
    let snapshot = load_snapshot(state_dir, path);
    let Some(text) = current else {
        drop_snapshot(state_dir, path);
        return Ok(Change::NotInstalled);
    };
    if let Some(snap) = snapshot.filter(|snap| snap.installed == text) {
        match &snap.original {
            Some(original) => write(path, original)?,
            None => std::fs::remove_file(path).map_err(|e| format!("Cannot remove {}: {e}", path.display()))?,
        }
        drop_snapshot(state_dir, path);
        return Ok(Change::Restored);
    }
    let change = match remove_hooks(&text)? {
        Some(stripped) => {
            write(path, &stripped)?;
            Change::Removed
        }
        None => Change::NotInstalled,
    };
    drop_snapshot(state_dir, path);
    Ok(change)
}

/// The hook config files of `client` on this machine: Claude Code's user
/// settings, every Codex home's `hooks.json`.
pub fn targets(client: HookClient) -> Result<Vec<PathBuf>, String> {
    match client {
        HookClient::ClaudeCode => Ok(vec![crate::clients::claude_config_dir()
            .ok_or("Could not resolve Claude Code's home")?
            .join("settings.json")]),
        HookClient::Codex => Ok(crate::clients::current_codex_homes()?
            .into_iter()
            .map(|home| home.join("hooks.json"))
            .collect()),
    }
}

/// Where the snapshots live: QuantMCP's own module folder.
pub fn state_dir() -> PathBuf {
    qs_core::paths::module_dir("mcp").join("memory-hooks")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("qs-memory-hook-{name}-{}", std::process::id()));
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

    /// A settings.json the way Claude Code writes it, with a foreign hook on
    /// the same event (Orca's) and unrelated settings around it.
    const CLAUDE: &str = "{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(git status)\"\n    ]\n  },\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"UserPromptSubmit\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"C:/Users/root/.orca/agent-hooks/claude-hook.cmd || echo {}\",\n            \"timeout\": 10\n          }\n        ]\n      }\n    ]\n  },\n  \"theme\": \"dark\"\n}\n";

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn command_posts_stdin_to_the_loopback_route_and_always_exits_zero() {
        let command = command(3100, HookClient::ClaudeCode, HookEvent::UserPromptSubmit);
        assert!(command.contains("--data-binary \"@-\" http://127.0.0.1:3100/quantmemory/claude-code/user-prompt-submit; exit 0"));
        assert!(command.contains("--connect-timeout 0.5 -m 2"));
        assert!(is_ours(&command));
        assert!(is_ours(&command.replace("3100", "3101")));
        assert!(!is_ours("curl -s -X POST --data-binary @- http://127.0.0.1:4545/claude/hook"));
        assert!(!is_ours("C:/Users/root/.orca/agent-hooks/claude-hook.cmd || echo {}"));
    }

    #[test]
    fn add_appends_one_group_beside_foreign_hooks_and_keeps_everything_else() {
        let added = add_hooks(CLAUDE, 3100, HookClient::ClaudeCode).unwrap();
        let value = parsed(&added);
        let groups = value["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["hooks"][0]["command"], "C:/Users/root/.orca/agent-hooks/claude-hook.cmd || echo {}");
        assert_eq!(groups[1]["hooks"][0]["type"], "command");
        assert_eq!(groups[1]["hooks"][0]["timeout"], HOOK_TIMEOUT_SECS);
        assert!(is_ours(groups[1]["hooks"][0]["command"].as_str().unwrap()));
        assert_eq!(value["model"], "opus");
        assert_eq!(value["theme"], "dark");
        assert!(added.starts_with("{\n  \"permissions\": {\n    \"allow\": [\n      \"Bash(git status)\"\n    ]\n  },\n  \"model\": \"opus\","));
        // Removing restores the original text exactly.
        assert_eq!(remove_hooks(&added).unwrap().as_deref(), Some(CLAUDE));
        assert_eq!(remove_hooks(CLAUDE).unwrap(), None);
    }

    #[test]
    fn add_creates_hooks_and_refuses_foreign_shapes() {
        for (before, expect_ok) in [
            ("", true),
            ("{}", true),
            ("{\n  \"model\": \"opus\"\n}\n", true),
            ("// comment\n{ \"hooks\": { \"Stop\": [] }, }\n", true),
            ("[]", false),
            ("{ \"hooks\": [] }", false),
            ("{ \"hooks\": { \"UserPromptSubmit\": {} } }", false),
            ("{ not json", false),
        ] {
            let result = add_hooks(before, 3100, HookClient::Codex);
            assert_eq!(result.is_ok(), expect_ok, "{before:?} → {result:?}");
            if let Ok(after) = result {
                assert!(remove_hooks(&after).unwrap().is_some(), "{before:?}");
                assert!(after.contains("/quantmemory/codex-cli/user-prompt-submit"));
            }
        }
    }

    #[test]
    fn remove_only_takes_our_handlers_out_of_a_shared_group() {
        let ours = command(3101, HookClient::ClaudeCode, HookEvent::UserPromptSubmit);
        let mixed = format!(
            "{{\"hooks\":{{\"UserPromptSubmit\":[{{\"hooks\":[{{\"type\":\"command\",\"command\":\"echo keep\"}},{{\"type\":\"command\",\"command\":{}}}]}}]}}}}",
            serde_json::to_string(&ours).unwrap()
        );
        let stripped = remove_hooks(&mixed).unwrap().unwrap();
        let value = parsed(&stripped);
        assert_eq!(value["hooks"]["UserPromptSubmit"][0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(value["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], "echo keep");
    }

    #[test]
    fn install_then_uninstall_restores_the_file_byte_for_byte() {
        let scratch = Scratch::new("exact");
        let state = scratch.0.join("state");
        for (name, original) in [
            ("claude.json", Some(CLAUDE.to_string())),
            ("crlf.json", Some("{\r\n  \"model\": \"opus\"\r\n}".to_string())),
            ("empty-object.json", Some("{}".to_string())),
            ("absent.json", None),
        ] {
            let path = scratch.0.join(name);
            if let Some(text) = &original {
                std::fs::write(&path, text).unwrap();
            }
            assert_eq!(install_file(&path, &state, 3100, HookClient::ClaudeCode).unwrap(), Change::Installed, "{name}");
            assert!(installed_in(&path).unwrap(), "{name}");
            // A second install is a no-op; a port change re-points in place.
            assert_eq!(install_file(&path, &state, 3100, HookClient::ClaudeCode).unwrap(), Change::UpToDate, "{name}");
            assert_eq!(install_file(&path, &state, 3101, HookClient::ClaudeCode).unwrap(), Change::Installed, "{name}");
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains("127.0.0.1:3101") && !text.contains("127.0.0.1:3100"), "{name}");
            assert_eq!(uninstall_file(&path, &state).unwrap(), Change::Restored, "{name}");
            match &original {
                Some(text) => assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes(), "{name}"),
                None => assert!(!path.exists(), "{name}"),
            }
            assert!(!installed_in(&path).unwrap(), "{name}");
            assert_eq!(uninstall_file(&path, &state).unwrap(), Change::NotInstalled, "{name}");
        }
    }

    #[test]
    fn uninstall_after_foreign_edits_keeps_them_and_drops_only_ours() {
        let scratch = Scratch::new("edited");
        let state = scratch.0.join("state");
        let path = scratch.0.join("settings.json");
        std::fs::write(&path, CLAUDE).unwrap();
        install_file(&path, &state, 3100, HookClient::ClaudeCode).unwrap();
        // Claude Code (or the user) changes a setting after the install.
        let edited = std::fs::read_to_string(&path).unwrap().replace("\"theme\": \"dark\"", "\"theme\": \"light\"");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(uninstall_file(&path, &state).unwrap(), Change::Removed);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, CLAUDE.replace("\"theme\": \"dark\"", "\"theme\": \"light\""));
        assert!(!installed_in(&path).unwrap());
    }

    #[test]
    fn uninstall_without_a_snapshot_still_finds_our_entries() {
        let scratch = Scratch::new("no-snapshot");
        let path = scratch.0.join("hooks.json");
        std::fs::write(&path, add_hooks("{}", 3100, HookClient::Codex).unwrap()).unwrap();
        assert_eq!(uninstall_file(&path, &scratch.0.join("state")).unwrap(), Change::Removed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn a_file_that_is_not_json_is_left_untouched() {
        let scratch = Scratch::new("invalid");
        let state = scratch.0.join("state");
        let path = scratch.0.join("settings.json");
        std::fs::write(&path, "{ broken").unwrap();
        assert!(install_file(&path, &state, 3100, HookClient::ClaudeCode).is_err());
        assert!(uninstall_file(&path, &state).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }
}
