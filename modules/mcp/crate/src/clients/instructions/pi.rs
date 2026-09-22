//! Native instruction discovery is independent of MCP support.
use super::env_path;
use std::path::{Path, PathBuf};

fn absolute_override(path: PathBuf, home: &Path) -> Result<PathBuf, String> {
    let path = path
        .strip_prefix("~")
        .map(|tail| home.join(tail))
        .unwrap_or(path.clone());
    if !path.is_absolute() {
        return Err("Agent directory override must be an absolute path (or start with ~/).".into());
    }
    Ok(path)
}

pub(in crate::clients) fn pi_home() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not resolve the user home")?;
    match env_path("PI_CODING_AGENT_DIR") {
        Some(path) => absolute_override(path, &home),
        None => Ok(home.join(".pi/agent")),
    }
}

fn omp_directory(
    home: &Path,
    root: Option<PathBuf>,
    agent: Option<PathBuf>,
    profile: Option<&str>,
) -> Result<PathBuf, String> {
    let root = root.unwrap_or_else(|| PathBuf::from(".omp"));
    // OMP documents PI_CONFIG_DIR as a name relative to home. An absolute
    // value has different Node join semantics; don't silently write elsewhere.
    if root.has_root() {
        return Err("PI_CONFIG_DIR must be relative to the user home. Add a custom target for another layout.".into());
    }
    let root = home.join(root);
    let profile = profile
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "default");
    if let Some(name) = profile {
        let basename = name.split('.').next().unwrap_or_default();
        let reserved = ["con", "prn", "aux", "nul"].contains(&basename)
            || ["com", "lpt"].iter().any(|prefix| {
                basename
                    .strip_prefix(prefix)
                    .is_some_and(|tail| tail.len() == 1 && tail.as_bytes()[0].is_ascii_digit())
            });
        if name.len() > 64
            || name.ends_with('.')
            || !name
                .as_bytes()
                .first()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-.".contains(c))
            || reserved
        {
            return Err("Invalid OMP profile name".into());
        }
        return Ok(root.join("profiles").join(name).join("agent"));
    }
    match agent {
        Some(path) if path.is_absolute() => Ok(path),
        Some(_) => Err("OMP's PI_CODING_AGENT_DIR must be absolute. Add a custom target for a relative launch directory.".into()),
        None => Ok(root.join("agent")),
    }
}

pub(in crate::clients) fn omp_home() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not resolve the user home")?;
    let profile = std::env::var("OMP_PROFILE")
        .or_else(|_| std::env::var("PI_PROFILE"))
        .ok();
    omp_directory(
        &home,
        env_path("PI_CONFIG_DIR"),
        env_path("PI_CODING_AGENT_DIR"),
        profile.as_deref(),
    )
}

pub(super) fn instruction_file(home: &Path) -> PathBuf {
    // Pi loads the first existing file, even an empty override. Preserve a
    // legacy CLAUDE.md instead of shadowing the operator's existing guidance.
    [
        "AGENTS.override.md",
        "AGENTS.md",
        "AGENTS.MD",
        "CLAUDE.md",
        "CLAUDE.MD",
    ]
    .iter()
    .map(|name| home.join(name))
    .find(|path| path.is_file())
    .unwrap_or_else(|| home.join("AGENTS.md"))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;

    #[test]
    fn pi_preserves_existing_context_and_even_empty_overrides() {
        let temp = Scratch::new();
        assert_eq!(instruction_file(&temp.0), temp.0.join("AGENTS.md"));
        for name in ["CLAUDE.md", "AGENTS.md", "AGENTS.override.md"] {
            std::fs::write(temp.0.join(name), "").unwrap();
            assert_eq!(instruction_file(&temp.0), temp.0.join(name));
        }
    }

    #[test]
    fn omp_respects_default_custom_root_agent_and_profile_precedence() {
        let temp = Scratch::new();
        let home = &temp.0;
        assert_eq!(
            omp_directory(home, None, None, None).unwrap(),
            home.join(".omp/agent")
        );
        assert_eq!(
            omp_directory(home, Some(".omp-custom".into()), None, Some("work")).unwrap(),
            home.join(".omp-custom/profiles/work/agent")
        );
        let custom = home.join("custom");
        assert_eq!(
            omp_directory(home, None, Some(custom.clone()), Some("default")).unwrap(),
            custom
        );
        assert_eq!(
            omp_directory(home, None, Some(custom), Some("work")).unwrap(),
            home.join(".omp/profiles/work/agent")
        );
        assert!(omp_directory(home, None, None, Some("../other")).is_err());
        for profile in ["Work", "con", "com1", "work.", ".hidden"] {
            assert!(omp_directory(home, None, None, Some(profile)).is_err());
        }
        assert!(omp_directory(home, Some(home.join("absolute")), None, None).is_err());
        assert_eq!(
            absolute_override("~/custom".into(), home).unwrap(),
            home.join("custom")
        );
        assert!(absolute_override("relative".into(), home).is_err());
    }
}
