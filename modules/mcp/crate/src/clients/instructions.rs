//! Import General AgentOS into the native startup files of detected clients.
//! MCP initialization instructions alone are not a client's standing brief.
use super::{ClientSpec, ConfigPath, ConnectResult, Outcome, CLIENTS};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(crate) mod custom;
mod kilo;
pub(super) mod pi;

const START: &str = "<!-- quantmcp:agentos:start -->";
const END: &str = "<!-- quantmcp:agentos:end -->";
static IMPORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub enum Instructions {
    File(ConfigPath),
    LimitedFile(ConfigPath, usize),
    Codex,
    Claude,
    OpenCode,
    Kilo,
    Pi,
    Omp,
    Manual(&'static str),
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let path = fs::canonicalize(&path).unwrap_or(path);
    if !paths.iter().any(|p| {
        if cfg!(windows) {
            p.to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        } else {
            *p == path
        }
    }) {
        paths.push(path);
    }
}

/// Discover explicit profiles and only existing Orca account homes. Never
/// enumerate sessions, credentials, or every folder under the user's home.
fn codex_homes(
    home: &Path,
    overrides: &[PathBuf],
    orca_roots: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let default = home.join(".codex");
    if overrides.is_empty() || default.is_dir() {
        push_unique(&mut paths, default);
    }
    for path in overrides {
        if !path.is_absolute() {
            return Err(format!("Codex home must be absolute: {}", path.display()));
        }
        push_unique(&mut paths, path.clone());
    }
    for root in orca_roots {
        let accounts = root.join("codex-accounts");
        let entries = match fs::read_dir(&accounts) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Cannot scan {}: {e}", accounts.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|e| format!("Cannot scan {}: {e}", accounts.display()))?;
            let path = entry.path().join("home");
            if path.is_dir() {
                push_unique(&mut paths, path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn current_codex_homes() -> Result<Vec<PathBuf>, String> {
    let home = dirs::home_dir().ok_or("Could not resolve the user home")?;
    let overrides: Vec<_> = [env_path("CODEX_HOME"), env_path("ORCA_CODEX_HOME")]
        .into_iter()
        .flatten()
        .collect();
    let roots: Vec<_> = [
        env_path("ORCA_USER_DATA_PATH"),
        dirs::config_dir().map(|p| p.join("orca")),
    ]
    .into_iter()
    .flatten()
    .collect();
    codex_homes(&home, &overrides, &roots)
}

pub(super) fn has_codex_profile() -> bool {
    current_codex_homes().is_ok_and(|paths| paths.iter().any(|p| p.is_dir()))
}

fn read_optional(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}

fn codex_file(home: &Path) -> Result<PathBuf, String> {
    let override_file = home.join("AGENTS.override.md");
    if !read_optional(&override_file)?.trim().is_empty() {
        Ok(override_file)
    } else {
        Ok(home.join("AGENTS.md"))
    }
}

impl Instructions {
    pub(super) fn note(&self) -> Option<&'static str> {
        match self {
            Self::Manual(note) => Some(note),
            Self::Kilo => Some("Kilo Code and Kilo CLI share global instructions. Selecting either enables AgentOS for both."),
            Self::Pi => Some("Pi loads instructions directly. MCP access requires a separate Pi extension."),
            Self::Omp => Some("Imports into the active OMP profile. Its native instructions take priority over other agents’ global context files."),
            _ => None,
        }
    }

    pub(super) fn paths(&self) -> Result<Vec<PathBuf>, String> {
        let resolve = |path: &ConfigPath| {
            path.resolve()
                .ok_or_else(|| "Could not resolve the instruction directory".to_string())
        };
        match self {
            Self::File(path) | Self::LimitedFile(path, _) => Ok(vec![resolve(path)?]),
            Self::Pi => Ok(vec![pi::instruction_file(&pi::pi_home()?)]),
            Self::Omp => Ok(vec![pi::omp_home()?.join("AGENTS.md")]),
            Self::Kilo => Ok(vec![kilo::config_path()?]),
            Self::Codex => current_codex_homes()?
                .iter()
                .map(|home| codex_file(home))
                .collect(),
            Self::Claude => {
                let dir = env_path("CLAUDE_CONFIG_DIR")
                    .or_else(|| dirs::home_dir().map(|p| p.join(".claude")))
                    .ok_or("Could not resolve Claude's home")?;
                Ok(vec![dir.join("CLAUDE.md")])
            }
            Self::OpenCode => {
                let dir = env_path("XDG_CONFIG_HOME")
                    .or_else(|| dirs::home_dir().map(|p| p.join(".config")))
                    .ok_or("Could not resolve OpenCode's home")?;
                let mut paths = vec![dir.join("opencode/AGENTS.md")];
                if let Some(extra) = env_path("OPENCODE_CONFIG_DIR") {
                    push_unique(&mut paths, extra.join("AGENTS.md"));
                }
                Ok(paths)
            }
            Self::Manual(_) => Ok(Vec::new()),
        }
    }
}

/// Replace our section only. Ambiguous/damaged markers are an error, never a
/// reason to discard user text or append a second competing instruction set.
fn merge(existing: &str, body: &str) -> Result<String, String> {
    if body.contains(START) || body.contains(END) {
        return Err(
            "General AgentOS contains reserved import markers; remove them before importing".into(),
        );
    }
    let block = format!("{START}\n{body}\n{END}");
    let starts: Vec<_> = existing.match_indices(START).collect();
    let ends: Vec<_> = existing.match_indices(END).collect();
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => {
            let separator = if existing.is_empty() || existing.ends_with("\n\n") { "" } else if existing.ends_with('\n') { "\n" } else { "\n\n" };
            Ok(format!("{existing}{separator}{block}\n"))
        }
        ([(start, _)], [(end, _)]) if start < end => {
            Ok(format!("{}{block}{}", &existing[..*start], &existing[*end + END.len()..]))
        }
        _ => Err("Existing AgentOS import markers are damaged or duplicated; the file was left untouched".into()),
    }
}

fn install(path: &Path, source: &Path, body: &str, limit: Option<usize>) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(format!(
            "Instruction path must be absolute: {}",
            path.display()
        ));
    }
    // Preserve symlinks by updating their destination, rather than replacing
    // the link with the atomic write's regular file.
    let destination = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if destination == fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()) {
        return Err(
            "Instruction file points at the General AgentOS source; refusing to modify the source"
                .into(),
        );
    }
    let old = read_optional(&destination)?;
    let new = merge(&old, body)?;
    if limit.is_some_and(|limit| new.chars().count() > limit) {
        return Err(format!("{} would exceed its instruction size limit; existing rules were kept. Shorten the rules before importing.", path.display()));
    }
    if old == new {
        return Ok(format!("Up to date: {}", path.display()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    qs_core::paths::write_atomic_with_backup(&destination, new.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(format!("Imported: {}", path.display()))
}

fn result(spec: &ClientSpec, index: usize, outcome: Outcome, detail: String) -> ConnectResult {
    ConnectResult {
        id: format!("{}:agentos:{index}", spec.id),
        name: format!("{} · AgentOS", spec.name),
        outcome,
        detail,
        restart: false,
    }
}

fn import_client(spec: &ClientSpec, source: &Path, text: &str) -> Vec<ConnectResult> {
    if matches!(spec.instructions, Instructions::Kilo) {
        return vec![
            match kilo::config_path().and_then(|path| kilo::install(&path, source)) {
                Ok(detail) => result(spec, 0, Outcome::Written, detail),
                Err(detail) => result(spec, 0, Outcome::Failed, detail),
            },
        ];
    }
    if let Instructions::Manual(note) = &spec.instructions {
        return vec![result(spec, 0, Outcome::Manual, note.to_string())];
    }
    let paths = match spec.instructions.paths() {
        Ok(paths) => paths,
        Err(e) => return vec![result(spec, 0, Outcome::Failed, e)],
    };
    import_paths(spec, source, text, &paths)
}

fn import_paths(
    spec: &ClientSpec,
    source: &Path,
    text: &str,
    paths: &[PathBuf],
) -> Vec<ConnectResult> {
    // A short startup directive fits Windsurf's 6,000-character cap and loads
    // the same live source. Other clients receive the complete instructions.
    let preface = format!("# QuantMCP AgentOS\n\nSource: {}\nManaged by QuantMCP. Re-import from General > AgentOS after changing the source.\n", source.display());
    let (body, limit) = match &spec.instructions {
        Instructions::LimitedFile(_, limit) => (format!("{preface}\nBefore any task, read and follow the complete instructions in `{}`. If QuantMCP is connected, call get_instructions to load the current global and workspace instructions. If neither is available, report that AgentOS could not be loaded.\n", source.display()), Some(*limit)),
        _ => (format!("{preface}\n{text}"), None),
    };
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| match install(path, source, &body, limit) {
            Ok(detail) => result(spec, i, Outcome::Written, detail),
            Err(detail) => result(spec, i, Outcome::Failed, detail),
        })
        .collect()
}

/// Resolve and validate the entire selection before any destination is touched.
fn selected_clients(ids: &[String]) -> Result<Vec<&'static ClientSpec>, String> {
    if ids.is_empty() {
        return Err("Select at least one detected agent before importing.".into());
    }
    let mut selected = Vec::new();
    for id in ids {
        let spec = CLIENTS
            .iter()
            .find(|spec| spec.id == id)
            .ok_or_else(|| format!("Unknown agent: {id}. Refresh detection and select again."))?;
        if !selected.iter().any(|s: &&ClientSpec| s.id == spec.id) {
            selected.push(spec);
        }
    }
    Ok(selected)
}

fn import_selected(
    selected: &[&ClientSpec],
    installed: impl Fn(&ClientSpec) -> bool,
    mut import: impl FnMut(&ClientSpec) -> Vec<ConnectResult>,
) -> Vec<ConnectResult> {
    selected
        .iter()
        .flat_map(|spec| {
            if installed(spec) {
                import(spec)
            } else {
                vec![result(
                    spec,
                    0,
                    Outcome::Failed,
                    "No longer detected. Refresh detection and select again.".into(),
                )]
            }
        })
        .collect()
}

pub fn import_agent_instructions(
    ids: &[String],
    custom: &[custom::CustomRecipient],
) -> Result<Vec<ConnectResult>, String> {
    let (selected, custom_selected) = custom::resolve_selection(ids, custom)?;
    let _lock = IMPORT_LOCK.lock().map_err(|e| e.to_string())?;
    let source = dirs::home_dir()
        .ok_or("Could not resolve the user home")?
        .join(".quantmcp/AGENT.md");
    let text = fs::read_to_string(&source).map_err(|e| {
        format!(
            "Cannot import {}: {e}. Save General AgentOS first.",
            source.display()
        )
    })?;
    if text.trim().is_empty() {
        return Err("General AgentOS is empty. Save instructions before importing.".into());
    }
    let mut report = import_selected(&selected, ClientSpec::installed, |spec| {
        import_client(spec, &source, &text)
    });
    for recipient in custom_selected {
        report.push(recipient.import(&source, &text));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) struct Scratch(pub(super) PathBuf);
    impl Scratch {
        pub(super) fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("qs-agentos-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn selection_is_explicit_validated_deduplicated_and_rechecked() {
        assert!(selected_clients(&[]).is_err());
        assert!(selected_clients(&["codex-cli".into(), "unknown".into()]).is_err());
        let selected =
            selected_clients(&["kilo-cli".into(), "codex-cli".into(), "kilo-cli".into()]).unwrap();
        let mut imported = Vec::new();
        let report = import_selected(
            &selected,
            |spec| spec.id != "codex-cli",
            |spec| {
                imported.push(spec.id);
                vec![result(spec, 0, Outcome::Written, "test import".into())]
            },
        );
        assert_eq!(imported, ["kilo-cli"]);
        assert_eq!(report.len(), 2);
        assert_eq!(report[1].outcome, Outcome::Failed);
        assert!(report[1].detail.contains("No longer detected"));
        assert!(matches!(
            super::super::spec("kilo-code").unwrap().instructions,
            Instructions::Kilo
        ));
    }

    #[test]
    fn preserves_user_rules_and_replaces_only_managed_section() {
        let existing = "\u{feff}# Personal rules\r\nKeep my preferences.\r\n";
        let once = merge(existing, "First instructions").unwrap();
        let once = format!("{once}\n# Other rules\r\nKeep these too.");
        let twice = merge(&once, "Updated instructions — UTF-8").unwrap();
        assert!(twice.starts_with(existing));
        assert!(twice.ends_with("\n# Other rules\r\nKeep these too."));
        assert!(!twice.contains("First instructions"));
        assert!(twice.contains("Updated instructions — UTF-8"));
        assert_eq!(twice.matches(START).count(), 1);
        assert_eq!(
            merge(&twice, "Updated instructions — UTF-8").unwrap(),
            twice
        );
    }

    #[test]
    fn malformed_markers_and_source_markers_are_rejected() {
        for text in [
            START.to_string(),
            END.to_string(),
            format!("{END}{START}"),
            format!("{START}{START}{END}"),
            format!("{START}{END}{END}"),
        ] {
            assert!(merge(&text, "rules").is_err());
        }
        assert!(merge("personal", START).is_err());
    }

    #[test]
    fn import_is_idempotent_backs_up_and_updates_actual_source_text() {
        let temp = Scratch::new();
        let source = temp.0.join("AGENT.md");
        let target = temp.0.join("profile/AGENTS.md");
        fs::write(&source, "user rules").unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "personal preferences").unwrap();
        install(&target, &source, "user rules", None).unwrap();
        let backup = qs_core::paths::backup_path(&target);
        assert_eq!(fs::read_to_string(&backup).unwrap(), "personal preferences");
        assert!(install(&target, &source, "user rules", None)
            .unwrap()
            .starts_with("Up to date"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "personal preferences");
        install(&target, &source, "new rules", None).unwrap();
        let current = fs::read_to_string(&target).unwrap();
        assert!(current.contains("personal preferences"));
        assert!(current.contains("new rules"));
        assert!(!current.contains("user rules"));
        assert_eq!(fs::read_to_string(source).unwrap(), "user rules");
    }

    #[test]
    fn errors_never_overwrite_existing_instructions() {
        let temp = Scratch::new();
        let source = temp.0.join("AGENT.md");
        let target = temp.0.join("AGENTS.md");
        fs::write(&source, "source").unwrap();
        fs::write(&target, START).unwrap();
        assert!(install(&target, &source, "new rules", None).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), START);
        fs::write(&target, "personal rules").unwrap();
        assert!(install(&target, &source, "new rules", Some(10)).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "personal rules");
        assert!(install(&source, &source, "new rules", None).is_err());
        fs::write(&target, [0xff, 0xfe]).unwrap();
        assert!(install(&target, &source, "new rules", None).is_err());
        assert_eq!(fs::read(&target).unwrap(), [0xff, 0xfe]);
        assert!(install(&temp.0, &source, "new rules", None).is_err());
    }

    #[test]
    fn codex_discovers_default_custom_and_all_existing_orca_homes_once() {
        let temp = Scratch::new();
        let default = temp.0.join(".codex");
        let custom = temp.0.join("custom-codex");
        let orca = temp.0.join("orca");
        let profile = orca.join("codex-accounts/account-one/home");
        let second = orca.join("codex-accounts/account-two/home");
        for dir in [&default, &custom, &profile, &second] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::create_dir_all(orca.join("codex-accounts/not-a-profile")).unwrap();
        let paths = codex_homes(
            &temp.0,
            &[custom.clone(), profile.clone()],
            &[orca.clone(), orca],
        )
        .unwrap();
        assert_eq!(paths.len(), 4);
        for dir in [default, custom, profile, second] {
            assert!(paths.contains(&fs::canonicalize(dir).unwrap()));
        }
        assert!(codex_homes(&temp.0, &[PathBuf::from("relative")], &[]).is_err());
    }

    #[test]
    fn codex_import_uses_nonempty_override_that_would_shadow_agents_md() {
        let temp = Scratch::new();
        let override_file = temp.0.join("AGENTS.override.md");
        assert_eq!(codex_file(&temp.0).unwrap(), temp.0.join("AGENTS.md"));
        fs::write(&override_file, " \n").unwrap();
        assert_eq!(codex_file(&temp.0).unwrap(), temp.0.join("AGENTS.md"));
        fs::write(&override_file, "private overrides").unwrap();
        let path = codex_file(&temp.0).unwrap();
        assert_eq!(path, override_file);
        install(&path, &temp.0.join("source.md"), "AgentOS rules", None).unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("private overrides"));
        assert!(text.contains("AgentOS rules"));
        assert!(!temp.0.join("AGENTS.md").exists());
    }

    #[test]
    fn unsupported_clients_are_manual_and_never_claim_an_import() {
        let spec = super::super::spec("cursor").unwrap();
        let rows = import_client(spec, Path::new("unused"), "rules");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, Outcome::Manual);
        assert!(rows[0].detail.contains("User Rules"));
        assert!(
            super::super::spec(&rows[0].id).is_none(),
            "instruction imports must not be recorded as MCP connection installs"
        );
    }

    #[test]
    fn imports_the_full_brief_and_reports_each_profile_independently() {
        let temp = Scratch::new();
        let source = temp.0.join("source.md");
        let brief = include_str!("../../templates/AGENT.global.md");
        fs::write(&source, brief).unwrap();
        let bad = temp.0.join("bad/AGENTS.md");
        let good = temp.0.join("good/AGENTS.md");
        fs::create_dir_all(&bad).unwrap();
        let rows = import_paths(
            super::super::spec("codex-cli").unwrap(),
            &source,
            brief,
            &[bad, good.clone()],
        );
        assert_eq!(rows[0].outcome, Outcome::Failed);
        assert_eq!(rows[1].outcome, Outcome::Written);
        assert!(fs::read_to_string(&good).unwrap().contains(brief));
        assert_eq!(fs::read_to_string(&source).unwrap(), brief);
        let rows = import_paths(
            super::super::spec("codex-cli").unwrap(),
            &source,
            brief,
            &[good],
        );
        assert!(rows[0].detail.starts_with("Up to date"));
    }
}
