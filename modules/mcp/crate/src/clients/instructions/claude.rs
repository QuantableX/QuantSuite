//! Claude Code loads every Markdown file in its user rules folder
//! (`~/.claude/rules/`, or `CLAUDE_CONFIG_DIR/rules/`) in every session,
//! whatever folder it starts in (observed on 2.1.283). So General AgentOS
//! reaches Claude Code as `rules/AGENT.md` there, linked to
//! `~/.quantmcp/AGENT.md` itself: no copy, no CLAUDE.md, no workspace, every
//! edit live.
//!
//! Earlier imports linked `AGENTS.md` into the folder above each registered
//! workspace, and before that copied the brief into CLAUDE.md. Both go once
//! the central link is in place.
use super::{read_optional, result, START, END};
use crate::clients::{ClientSpec, ConnectResult, Outcome};
use std::fs;
use std::path::{Path, PathBuf};

const FILE: &str = "AGENT.md";
const OLD_FILE: &str = "AGENTS.md";

fn config_dir() -> Option<PathBuf> {
    super::env_path("CLAUDE_CONFIG_DIR").or_else(|| dirs::home_dir().map(|p| p.join(".claude")))
}

/// The one link: `rules/AGENT.md` in Claude Code's home.
pub(super) fn target() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("rules").join(FILE))
}

/// The CLAUDE.md earlier imports wrote the AgentOS copy into.
pub(super) fn claude_md() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("CLAUDE.md"))
}

/// The registered workspace folders, from core.db. Only used to find the
/// links earlier imports left above them; empty when the registry cannot be
/// read.
pub(super) fn registered_workspaces() -> Vec<PathBuf> {
    qs_core::db::open()
        .map(|conn| qs_core::workspaces::list(&conn).into_iter().map(|ws| PathBuf::from(ws.path)).collect())
        .unwrap_or_default()
}

/// Where earlier imports linked `AGENTS.md`: the folder above each
/// workspace, once per folder.
fn old_links(workspaces: &[PathBuf]) -> Vec<PathBuf> {
    // Compared as written, not canonicalised: a link would come back as
    // `\\?\C:\…` and show up that way in the UI.
    let same = |a: &Path, b: &Path| if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) } else { a == b };
    let mut links: Vec<PathBuf> = Vec::new();
    for parent in workspaces.iter().filter_map(|ws| ws.parent()).filter(|p| !p.as_os_str().is_empty()) {
        let link = parent.join(OLD_FILE);
        if !links.iter().any(|l| same(l, &link)) {
            links.push(link);
        }
    }
    links
}

pub(super) fn import(spec: &ClientSpec, source: &Path, target: &Path, workspaces: &[PathBuf], claude_md: Option<&Path>) -> Vec<ConnectResult> {
    let linked = install(target, source);
    let mut rows = vec![match &linked {
        Ok(detail) => result(spec, 0, Outcome::Written, detail.clone()),
        Err(detail) => result(spec, 0, Outcome::Failed, detail.clone()),
    }];
    // Only once Claude Code has AgentOS through the central link do the
    // earlier links and the old copy go.
    if linked.is_err() {
        return rows;
    }
    let mut report = |outcome: Result<Option<String>, String>| match outcome {
        Ok(Some(note)) => rows.push(result(spec, rows.len(), Outcome::Written, note)),
        Ok(None) => {}
        Err(e) => rows.push(result(spec, rows.len(), Outcome::Failed, e)),
    };
    for link in old_links(workspaces) {
        report(retire_link(&link, source));
    }
    if let Some(path) = claude_md {
        report(retire_claude_md(path));
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
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
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

/// Remove an `AGENTS.md` an earlier import linked above a workspace. Only a
/// link to the source goes — removing a link leaves AGENT.md itself alone —
/// and any other AGENTS.md stays. `None` when there is nothing to remove.
fn retire_link(link: &Path, source: &Path) -> Result<Option<String>, String> {
    if link.symlink_metadata().is_err() || !same_file::is_same_file(link, source).unwrap_or(false) {
        return Ok(None);
    }
    fs::remove_file(link)
        .map(|()| Some(format!("Removed {}: the link above a workspace is no longer needed.", link.display())))
        .map_err(|e| format!("Cannot remove {}: {e}", link.display()))
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
    fn one_old_link_per_folder_above_the_workspaces() {
        let root = PathBuf::from(if cfg!(windows) { r"C:\Projects" } else { "/projects" });
        let links = old_links(&[root.join("a"), root.join("b"), root.join("other").join("c")]);
        assert_eq!(links, [root.join(OLD_FILE), root.join("other").join(OLD_FILE)]);
    }

    /// The link is AGENT.md itself: in-place edits show through it, a second
    /// import finds it up to date, and a foreign file is never touched.
    #[test]
    fn links_the_live_source_and_never_replaces_a_foreign_file() {
        let temp = Scratch::new();
        let source = temp.0.join("home/.quantmcp/AGENT.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "rules v1").unwrap();
        let target = temp.0.join("home/.claude/rules").join(FILE);

        assert!(install(&target, &source).unwrap().starts_with("Linked"), "creates the rules folder");
        fs::write(&source, "rules v2").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "rules v2", "live, not a copy");
        assert!(install(&target, &source).unwrap().starts_with("Up to date"));

        let foreign = temp.0.join("mine").join(FILE);
        fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        fs::write(&foreign, "my own rules").unwrap();
        assert!(install(&foreign, &source).is_err());
        assert_eq!(fs::read_to_string(&foreign).unwrap(), "my own rules");
    }

    #[test]
    fn retires_only_links_to_the_source() {
        let temp = Scratch::new();
        let source = temp.0.join("AGENT.md");
        fs::write(&source, "rules").unwrap();
        let ours = temp.0.join("projects").join(OLD_FILE);
        fs::create_dir_all(ours.parent().unwrap()).unwrap();
        link(&ours, &source).unwrap();
        let foreign = temp.0.join("games").join(OLD_FILE);
        fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        fs::write(&foreign, "project rules").unwrap();

        assert!(retire_link(&ours, &source).unwrap().is_some());
        assert!(ours.symlink_metadata().is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "rules", "the source stays");
        assert!(retire_link(&foreign, &source).unwrap().is_none());
        assert_eq!(fs::read_to_string(&foreign).unwrap(), "project rules");
        assert!(retire_link(&temp.0.join("missing").join(OLD_FILE), &source).unwrap().is_none());
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
    fn imports_without_any_workspace_and_then_retires_the_old_setup() {
        let temp = Scratch::new();
        let source = temp.0.join("AGENT.md");
        fs::write(&source, "rules").unwrap();
        let spec = crate::clients::spec("claude-code").unwrap();

        let target = temp.0.join("claude/rules").join(FILE);
        let rows = import(spec, &source, &target, &[], None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, Outcome::Written, "{}", rows[0].detail);
        assert!(same_file::is_same_file(&target, &source).unwrap());

        // A foreign file in the way: nothing old is retired either.
        let claude_md = temp.0.join("old/CLAUDE.md");
        fs::create_dir_all(claude_md.parent().unwrap()).unwrap();
        fs::write(&claude_md, format!("{START}\nold copy\n{END}\n")).unwrap();
        let blocked = temp.0.join("blocked/rules").join(FILE);
        fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        fs::write(&blocked, "someone else's").unwrap();
        let rows = import(spec, &source, &blocked, &[], Some(&claude_md));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, Outcome::Failed);
        assert!(claude_md.exists(), "Claude Code would be left without AgentOS");

        let ws = temp.0.join("projects/app");
        fs::create_dir_all(&ws).unwrap();
        let old = temp.0.join("projects").join(OLD_FILE);
        link(&old, &source).unwrap();
        let rows = import(spec, &source, &target, &[ws], Some(&claude_md));
        assert_eq!(rows.len(), 3, "{:?}", rows.iter().map(|r| &r.detail).collect::<Vec<_>>());
        assert!(rows.iter().all(|r| r.outcome == Outcome::Written));
        assert!(rows[0].detail.starts_with("Up to date"));
        assert!(old.symlink_metadata().is_err());
        assert!(!claude_md.exists());
    }
}
