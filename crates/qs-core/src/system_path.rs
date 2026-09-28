//! The PATH to find and run command-line tools with.
//!
//! The suite inherits its environment from Explorer, and Explorer keeps the
//! PATH it had at sign-in until an installer broadcasts a change — not every
//! installer does. On 2026-09-28 Claude Code's installer put `claude.exe` in
//! a folder on the user's PATH, yet QuantMCP's Connect reported "command not
//! found" because Explorer had been running for two weeks. So on Windows the
//! PATH is read fresh from the registry, the way a newly opened terminal gets
//! it, with the inherited PATH appended for anything set only for this
//! process. Elsewhere it is the inherited PATH.

use std::ffi::OsString;

/// The current system PATH, then the user PATH, then the inherited one, with
/// duplicates dropped.
#[cfg(windows)]
pub fn current() -> OsString {
    let var = |name: &str| std::env::var(name).ok();
    let mut parts: Vec<String> = [registry::machine_path(), registry::user_path()]
        .into_iter()
        .flatten()
        .map(|value| expand(&value, var))
        .collect();
    parts.push(std::env::var_os("PATH").unwrap_or_default().to_string_lossy().into_owned());
    OsString::from(merge(&parts))
}

#[cfg(not(windows))]
pub fn current() -> OsString {
    std::env::var_os("PATH").unwrap_or_default()
}

#[cfg(windows)]
mod registry {
    const MACHINE: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    pub(super) fn machine_path() -> Option<String> {
        windows_registry::LOCAL_MACHINE.open(MACHINE).and_then(|key| key.get_string("Path")).ok()
    }

    pub(super) fn user_path() -> Option<String> {
        windows_registry::CURRENT_USER.open("Environment").and_then(|key| key.get_string("Path")).ok()
    }
}

/// Expand `%NAME%` the way Windows does for REG_EXPAND_SZ values; an unknown
/// name stays as written.
#[cfg_attr(not(windows), allow(dead_code))]
fn expand(value: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match var(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Join `;`-separated lists in order, without empty entries or duplicates
/// (compared case-insensitively, ignoring a trailing separator).
#[cfg_attr(not(windows), allow(dead_code))]
fn merge(lists: &[String]) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<&str> = Vec::new();
    for entry in lists.iter().flat_map(|list| list.split(';')).map(str::trim) {
        let key = entry.trim_end_matches(['\\', '/']).to_lowercase();
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(entry);
    }
    out.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_variables_and_keeps_unknown_ones() {
        let var = |name: &str| (name.eq_ignore_ascii_case("USERPROFILE")).then(|| r"C:\Users\bitzer".to_string());
        assert_eq!(expand(r"%USERPROFILE%\.local\bin", var), r"C:\Users\bitzer\.local\bin");
        assert_eq!(expand(r"%UserProfile%\a;%NOPE%\b", var), r"C:\Users\bitzer\a;%NOPE%\b");
        assert_eq!(expand("100% sure", var), "100% sure");
        assert_eq!(expand("%%", var), "%%");
        assert_eq!(expand("plain", var), "plain");
    }

    #[test]
    fn merges_in_order_without_duplicates_or_blanks() {
        let lists = [
            r"C:\Windows\system32;C:\Windows;".to_string(),
            r"C:\Users\bitzer\.local\bin;C:\Users\bitzer\AppData\Roaming\npm".to_string(),
            r"C:\WINDOWS\System32\;;C:\Users\bitzer\AppData\Roaming\npm;C:\only\inherited".to_string(),
        ];
        assert_eq!(
            merge(&lists),
            r"C:\Windows\system32;C:\Windows;C:\Users\bitzer\.local\bin;C:\Users\bitzer\AppData\Roaming\npm;C:\only\inherited"
        );
    }

    /// The registry PATH is what a new terminal gets, so every folder on it
    /// must be in `current()`.
    #[cfg(windows)]
    #[test]
    fn current_contains_the_registry_user_path() {
        let current = current().to_string_lossy().to_lowercase();
        let var = |name: &str| std::env::var(name).ok();
        for entry in registry::user_path().map(|p| expand(&p, var)).unwrap_or_default().split(';').map(str::trim).filter(|e| !e.is_empty()) {
            assert!(current.contains(&entry.trim_end_matches('\\').to_lowercase()), "{entry} missing from {current}");
        }
    }
}
