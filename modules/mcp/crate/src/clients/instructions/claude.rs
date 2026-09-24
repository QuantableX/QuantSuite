//! Claude Code (2.1.277+) reads `AGENTS.md` from a project's folder or any
//! folder above it when the project has no CLAUDE.md of its own. It has no
//! global AGENTS.md, and it does not expand an `@` import of a file outside
//! the project there. So General AgentOS reaches Claude Code as `AGENTS.md`
//! in the folder above each registered workspace, linked to
//! `~/.quantmcp/AGENT.md` itself: no copy, no CLAUDE.md, every edit live.
use super::{read_optional, result, START, END};
use crate::clients::{ClientSpec, ConnectResult, Outcome};
use std::fs;
use std::path::{Path, PathBuf};

const FILE: &str = "AGENTS.md";

/// The registered workspace folders, from core.db. Empty when the registry
/// cannot be read.
pub(super) fn registered_workspaces() -> Vec<PathBuf> {
    qs_core::db::open()
        .map(|conn| qs_core::workspaces::list(&conn).into_iter().map(|ws| PathBuf::from(ws.path)).collect())
        .unwrap_or_default()
}

/// `AGENTS.md` in the folder above each workspace, once per folder.
pub(super) fn link_targets(workspaces: &[PathBuf]) -> Vec<PathBuf> {
    // Compared as written, not canonicalised: an existing link would come
    // back as `\\?\C:\…` and show up that way in the UI.
    let same = |a: &Path, b: &Path| if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) } else { a == b };
    let mut targets: Vec<PathBuf> = Vec::new();
    for parent in workspaces.iter().filter_map(|ws| ws.parent()).filter(|p| !p.as_os_str().is_empty()) {
        let target = parent.join(FILE);
        if !targets.iter().any(|t| same(t, &target)) {
            targets.push(target);
        }
    }
    targets
}

/// The CLAUDE.md earlier imports wrote the AgentOS copy into.
pub(super) fn claude_md() -> Option<PathBuf> {
    super::env_path("CLAUDE_CONFIG_DIR")
        .or_else(|| dirs::home_dir().map(|p| p.join(".claude")))
        .map(|dir| dir.join("CLAUDE.md"))
}

pub(super) fn import(spec: &ClientSpec, source: &Path, workspaces: &[PathBuf], claude_md: Option<&Path>) -> Vec<ConnectResult> {
    let targets = link_targets(workspaces);
    if targets.is_empty() {
        return vec![result(spec, 0, Outcome::Failed, "No workspace is registered. Claude Code loads AGENTS.md from a project's folder or a folder above it; register a workspace and import again.".into())];
    }
    let mut rows: Vec<ConnectResult> = targets
        .iter()
        .enumerate()
        .map(|(i, target)| match install(target, source) {
            Ok(detail) => result(spec, i, Outcome::Written, detail),
            Err(detail) => result(spec, i, Outcome::Failed, detail),
        })
        .collect();
    // Only once Claude Code has AgentOS through a link does the old copy go.
    if !rows.iter().any(|r| r.outcome == Outcome::Written) {
        return rows;
    }
    match claude_md.map(retire_claude_md).transpose() {
        Ok(Some(Some(note))) => rows.push(result(spec, rows.len(), Outcome::Written, note)),
        Ok(_) => {}
        Err(e) => rows.push(result(spec, rows.len(), Outcome::Failed, e)),
    }
    rows
}

/// `target` becomes General AgentOS itself. An existing file that is not
/// AgentOS is never replaced.
fn install(target: &Path, source: &Path) -> Result<String, String> {
    if target.symlink_metadata().is_ok() {
        if same_file::is_same_file(target, source).unwrap_or(false) {
            return Ok(format!("Up to date: {} is {}", target.display(), source.display()));
        }
        return Err(format!(
            "{} already exists and is not General AgentOS; it was left untouched. Move it away and import again.",
            target.display()
        ));
    }
    let kind = link(target, source)?;
    Ok(format!("Linked: {} is {} ({kind})", target.display(), source.display()))
}

/// A symbolic link where the system allows one — it survives an editor that
/// saves by replacing the file — otherwise a hard link, which needs no rights
/// and stays live because the suite writes AGENT.md in place.
fn link(target: &Path, source: &Path) -> Result<&'static str, String> {
    #[cfg(windows)]
    let symlink = std::os::windows::fs::symlink_file(source, target);
    #[cfg(not(windows))]
    let symlink = std::os::unix::fs::symlink(source, target);
    if symlink.is_ok() {
        return Ok("symbolic link");
    }
    fs::hard_link(source, target)
        .map(|()| "hard link")
        .map_err(|e| format!("Cannot link {} to {}: {e}", target.display(), source.display()))
}

/// Take the copy earlier imports wrote out of CLAUDE.md: the file goes when
/// nothing else is in it (a backup stays), the user's own rules stay
/// otherwise. `None` when there is nothing to take out.
fn retire_claude_md(path: &Path) -> Result<Option<String>, String> {
    let text = read_optional(path)?;
    let (Some(start), Some(end)) = (text.find(START), text.find(END)) else {
        return Ok(None);
    };
    if end < start || text.matches(START).count() > 1 || text.matches(END).count() > 1 {
        return Err(format!("The AgentOS markers in {} are damaged or duplicated; it was left untouched.", path.display()));
    }
    let rest = format!("{}{}", &text[..start], text[end + END.len()..].trim_start_matches(['\r', '\n']));
    if rest.trim().is_empty() {
        fs::copy(path, qs_core::paths::backup_path(path))
            .and_then(|_| fs::remove_file(path))
            .map_err(|e| format!("Cannot remove {}: {e}", path.display()))?;
        return Ok(Some(format!("Removed {}: Claude Code no longer needs it.", path.display())));
    }
    qs_core::paths::write_atomic_with_backup(path, rest.as_bytes())
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(Some(format!("Removed the old AgentOS copy from {}; your own rules there stay.", path.display())))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    #[test]
    fn one_link_per_folder_above_the_workspaces() {
        let root = PathBuf::from(if cfg!(windows) { r"C:\Projects" } else { "/projects" });
        let targets = link_targets(&[root.join("a"), root.join("b"), root.join("other").join("c")]);
        assert_eq!(targets.len(), 2, "{targets:?}");
        assert!(targets[0].ends_with(Path::new(FILE)) && targets[0].parent() == Some(root.as_path()));
        assert_eq!(targets[1], root.join("other").join(FILE));
    }

    /// The link is AGENT.md itself: in-place edits show through it, a second
    /// import finds it up to date, and a foreign AGENTS.md is never touched.
    #[test]
    fn links_the_live_source_and_never_replaces_a_foreign_file() {
        let temp = Scratch::new();
        let source = temp.0.join("home/.quantmcp/AGENT.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "rules v1").unwrap();
        let target = temp.0.join("projects").join(FILE);
        fs::create_dir_all(target.parent().unwrap()).unwrap();

        assert!(install(&target, &source).unwrap().starts_with("Linked"));
        fs::write(&source, "rules v2").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "rules v2", "live, not a copy");
        assert!(install(&target, &source).unwrap().starts_with("Up to date"));

        let foreign = temp.0.join("mine").join(FILE);
        fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        fs::write(&foreign, "my own AGENTS.md").unwrap();
        assert!(install(&foreign, &source).is_err());
        assert_eq!(fs::read_to_string(&foreign).unwrap(), "my own AGENTS.md");
    }

    #[test]
    fn retires_the_old_claude_md_copy_but_keeps_user_rules() {
        let temp = Scratch::new();
        let only_ours = temp.0.join("a/CLAUDE.md");
        fs::create_dir_all(only_ours.parent().unwrap()).unwrap();
        fs::write(&only_ours, format!("{START}\n# QuantMCP AgentOS\n\nold copy\n{END}\n")).unwrap();
        assert!(retire_claude_md(&only_ours).unwrap().is_some());
        assert!(!only_ours.exists(), "nothing else was in it");
        assert!(qs_core::paths::backup_path(&only_ours).is_file());

        let mixed = temp.0.join("b/CLAUDE.md");
        fs::create_dir_all(mixed.parent().unwrap()).unwrap();
        fs::write(&mixed, format!("# Mine\r\nKeep me.\r\n\r\n{START}\nold copy\n{END}\n# Also mine\n")).unwrap();
        assert!(retire_claude_md(&mixed).unwrap().is_some());
        let text = fs::read_to_string(&mixed).unwrap();
        assert!(text.contains("Keep me.") && text.contains("# Also mine") && !text.contains("old copy") && !text.contains(START), "{text}");

        assert!(retire_claude_md(&mixed).unwrap().is_none(), "nothing left to take out");
        assert!(retire_claude_md(&temp.0.join("missing/CLAUDE.md")).unwrap().is_none());
        let damaged = temp.0.join("c/CLAUDE.md");
        fs::create_dir_all(damaged.parent().unwrap()).unwrap();
        fs::write(&damaged, format!("{END}\n{START}")).unwrap();
        assert!(retire_claude_md(&damaged).is_err());
        assert_eq!(fs::read_to_string(&damaged).unwrap(), format!("{END}\n{START}"));
    }

    #[test]
    fn import_reports_each_link_and_the_retired_copy() {
        let temp = Scratch::new();
        let source = temp.0.join("AGENT.md");
        fs::write(&source, "rules").unwrap();
        let claude_md = temp.0.join("claude/CLAUDE.md");
        fs::create_dir_all(claude_md.parent().unwrap()).unwrap();
        fs::write(&claude_md, format!("{START}\nold copy\n{END}\n")).unwrap();
        let spec = crate::clients::spec("claude-code").unwrap();

        let none = import(spec, &source, &[], Some(&claude_md));
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].outcome, Outcome::Failed);
        assert!(claude_md.exists(), "nothing retired while Claude Code would be left without AgentOS");

        let blocked = temp.0.join("blocked");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join(FILE), "someone else's").unwrap();
        let rows = import(spec, &source, &[blocked.join("app")], Some(&claude_md));
        assert_eq!(rows.len(), 1);
        assert!(claude_md.exists(), "no link, so the old copy stays");

        let ws = temp.0.join("projects/app");
        fs::create_dir_all(&ws).unwrap();
        let rows = import(spec, &source, &[ws], Some(&claude_md));
        assert_eq!(rows.len(), 2, "{:?}", rows.iter().map(|r| &r.detail).collect::<Vec<_>>());
        assert!(rows.iter().all(|r| r.outcome == Outcome::Written));
        assert!(same_file::is_same_file(temp.0.join("projects").join(FILE), &source).unwrap());
        assert!(!claude_md.exists());
    }
}
