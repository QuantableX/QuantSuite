//! The terminal a session opens with, and the wrappers defined inside it.
//!
//! A session is a shell in the context folder. `claude`, `codex`, `pi`,
//! `omp`, `opencode`, `gemini` and any custom adapter exist in that shell as
//! functions (PowerShell, bash) or doskey macros (cmd) that call the real
//! executable with the session's arguments — the id, the mode, the hooks,
//! the resume flag — and first tell the relay which adapter was started.
//! The user decides what runs by typing it; the row stays the session.
//!
//! Everything long lives in files under the session's directory, so a
//! wrapper is one line of quoted arguments in any of the three syntaxes.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellKind {
    PowerShell,
    Cmd,
    Bash,
    Other,
}

/// One command the terminal defines.
#[derive(Clone, Debug)]
pub struct Wrapper {
    /// The command name — the adapter id.
    pub command: String,
    pub program: String,
    pub args: Vec<String>,
}

pub fn kind_of(shell_path: &str) -> ShellKind {
    let name = Path::new(shell_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = name.trim_end_matches(".exe");
    match stem {
        "pwsh" | "powershell" => ShellKind::PowerShell,
        "cmd" => ShellKind::Cmd,
        "bash" | "sh" | "zsh" => ShellKind::Bash,
        _ => ShellKind::Other,
    }
}

/// Settings override → pwsh → Windows PowerShell → cmd (the console's own order).
pub fn resolve(override_value: &str) -> String {
    let wanted = override_value.trim();
    if !wanted.is_empty() {
        if Path::new(wanted).is_file() {
            return wanted.to_string();
        }
        if let Some(p) = crate::providers::which(wanted.trim_end_matches(".exe")) {
            return p.to_string_lossy().into_owned();
        }
    }
    if cfg!(windows) {
        if let Some(p) = crate::providers::which("pwsh") {
            return p.to_string_lossy().into_owned();
        }
        let ps = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
        if Path::new(ps).is_file() {
            return ps.to_string();
        }
        "cmd.exe".to_string()
    } else {
        for p in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
            if Path::new(p).is_file() {
                return p.to_string();
            }
        }
        "sh".to_string()
    }
}

/// The init file's name for a shell kind, `None` when the shell gets no wrappers.
pub fn init_file_name(kind: ShellKind) -> Option<&'static str> {
    match kind {
        ShellKind::PowerShell => Some("init.ps1"),
        ShellKind::Cmd => Some("macros.txt"),
        ShellKind::Bash => Some("init.sh"),
        ShellKind::Other => None,
    }
}

/// Arguments that start the shell interactive with the init file loaded.
pub fn shell_args(kind: ShellKind, init: &Path, hint: &str) -> Vec<String> {
    let init_s = init.to_string_lossy().into_owned();
    match kind {
        ShellKind::PowerShell => vec![
            "-NoLogo".into(),
            "-NoExit".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            init_s,
        ],
        ShellKind::Cmd => vec![
            "/K".into(),
            format!("doskey /MACROFILE=\"{init_s}\" & echo {}", cmd_echo(hint)),
        ],
        ShellKind::Bash => vec!["--rcfile".into(), init_s.replace('\\', "/"), "-i".into()],
        ShellKind::Other => Vec::new(),
    }
}

fn cmd_echo(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' | '|' | '<' | '>' | '^' | '(' | ')' => format!("^{c}"),
            _ => c.to_string(),
        })
        .collect()
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn cmd_quote(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || matches!(c, '&' | '|' | '<' | '>' | '^' | '(' | ')')) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

fn launched_url(relay_port: u16, session_id: &str, adapter: &str) -> String {
    format!("http://127.0.0.1:{relay_port}/launched?session={session_id}&adapter={adapter}")
}

/// The init file's content.
pub fn init_script(kind: ShellKind, session_id: &str, relay_port: u16, wrappers: &[Wrapper], hint: &str) -> String {
    match kind {
        ShellKind::PowerShell => {
            // A BOM, or Windows PowerShell reads the file as ANSI and the
            // hint's `·` prints as `Â·`.
            let mut out = String::from("\u{FEFF}# QuantPilot session — generated at every launch; edits are overwritten.\n");
            out.push_str("function global:__qp_launched([string]$adapter) {\n");
            out.push_str(&format!(
                "  try {{ & curl.exe -s \"http://127.0.0.1:{relay_port}/launched?session={session_id}&adapter=$adapter\" | Out-Null }} catch {{}}\n"
            ));
            out.push_str("}\n");
            for w in wrappers {
                let args: Vec<String> = w.args.iter().map(|a| ps_quote(a)).collect();
                out.push_str(&format!(
                    "function global:{} {{ __qp_launched {}; & {} {} @args }}\n",
                    w.command,
                    ps_quote(&w.command),
                    ps_quote(&w.program),
                    args.join(" ")
                ));
            }
            out.push_str(&format!("Write-Host {} -ForegroundColor DarkGray\n", ps_quote(hint)));
            out
        }
        ShellKind::Cmd => {
            let mut out = String::new();
            for w in wrappers {
                let args: Vec<String> = w.args.iter().map(|a| cmd_quote(a)).collect();
                out.push_str(&format!(
                    "{}=curl.exe -s \"{}\" >NUL $T {} {} $*\n",
                    w.command,
                    launched_url(relay_port, session_id, &w.command),
                    cmd_quote(&w.program),
                    args.join(" ")
                ));
            }
            out
        }
        ShellKind::Bash => {
            let mut out = String::from("# QuantPilot session — generated at every launch; edits are overwritten.\n");
            out.push_str("[ -f ~/.bashrc ] && . ~/.bashrc\n");
            out.push_str(&format!(
                "__qp_launched() {{ curl -s \"http://127.0.0.1:{relay_port}/launched?session={session_id}&adapter=$1\" >/dev/null 2>&1; }}\n"
            ));
            for w in wrappers {
                let args: Vec<String> = w.args.iter().map(|a| sh_quote(a)).collect();
                out.push_str(&format!(
                    "{}() {{ __qp_launched {}; {} {} \"$@\"; }}\n",
                    w.command,
                    sh_quote(&w.command),
                    sh_quote(&w.program.replace('\\', "/")),
                    args.join(" ")
                ));
            }
            out.push_str(&format!("echo {}\n", sh_quote(hint)));
            out
        }
        ShellKind::Other => String::new(),
    }
}

/// Where a session's generated files live.
pub fn session_dir(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir.join("sessions").join(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrappers() -> Vec<Wrapper> {
        vec![
            Wrapper {
                command: "claude".into(),
                program: "C:\\Users\\me\\.local\\bin\\claude.exe".into(),
                args: vec!["--session-id".into(), "abc".into(), "--settings".into(), "C:\\d\\it's.json".into()],
            },
            Wrapper {
                command: "pi".into(),
                program: "C:\\Program Files\\nodejs\\node.exe".into(),
                args: vec!["C:\\npm\\node_modules\\pi\\cli.js".into(), "--session-id".into(), "abc".into()],
            },
        ]
    }

    #[test]
    fn detects_shell_kinds() {
        assert_eq!(kind_of("C:\\Program Files\\PowerShell\\7\\pwsh.exe"), ShellKind::PowerShell);
        assert_eq!(kind_of("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"), ShellKind::PowerShell);
        assert_eq!(kind_of("cmd.exe"), ShellKind::Cmd);
        assert_eq!(kind_of("C:\\Program Files\\Git\\bin\\bash.exe"), ShellKind::Bash);
        assert_eq!(kind_of("nu.exe"), ShellKind::Other);
    }

    #[test]
    fn powershell_init_defines_global_functions() {
        let s = init_script(ShellKind::PowerShell, "abc", 4500, &wrappers(), "hi 'there'");
        assert!(s.contains("function global:claude { __qp_launched 'claude'; & 'C:\\Users\\me\\.local\\bin\\claude.exe' '--session-id' 'abc' '--settings' 'C:\\d\\it''s.json' @args }"));
        assert!(s.contains("function global:pi { __qp_launched 'pi'; & 'C:\\Program Files\\nodejs\\node.exe' 'C:\\npm\\node_modules\\pi\\cli.js'"));
        assert!(s.contains("launched?session=abc&adapter=$adapter"));
        assert!(s.contains("Write-Host 'hi ''there''' -ForegroundColor DarkGray"));
        let args = shell_args(ShellKind::PowerShell, Path::new("C:\\d\\init.ps1"), "hi");
        assert_eq!(args, vec!["-NoLogo", "-NoExit", "-ExecutionPolicy", "Bypass", "-File", "C:\\d\\init.ps1"]);
    }

    #[test]
    fn cmd_macros_chain_the_report_and_the_program() {
        let s = init_script(ShellKind::Cmd, "abc", 4500, &wrappers(), "hi");
        assert!(
            s.starts_with("claude=curl.exe -s \"http://127.0.0.1:4500/launched?session=abc&adapter=claude\" >NUL $T C:\\Users\\me\\.local\\bin\\claude.exe --session-id abc --settings C:\\d\\it's.json $*\n"),
            "{s}"
        );
        assert!(s.contains("pi=curl.exe") && s.contains("$T \"C:\\Program Files\\nodejs\\node.exe\""));
        let args = shell_args(ShellKind::Cmd, Path::new("C:\\d\\macros.txt"), "a & b");
        assert_eq!(args[0], "/K");
        assert_eq!(args[1], "doskey /MACROFILE=\"C:\\d\\macros.txt\" & echo a ^& b");
    }

    /// The PowerShell init file, run by a real PowerShell: the wrapper
    /// must call its program with the baked arguments plus the typed ones,
    /// quotes and all, and must not die when the relay is not there.
    #[test]
    fn powershell_wrapper_runs_for_real() {
        use std::process::Command;
        let ps = "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
        if !Path::new(ps).is_file() {
            eprintln!("skipped: no Windows PowerShell here");
            return;
        }
        let dir = std::env::temp_dir().join(format!("qs-pilot-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let init = dir.join("init.ps1");
        let w = Wrapper {
            command: "qptest".into(),
            program: "C:\\Windows\\System32\\cmd.exe".into(),
            args: vec!["/c".into(), "echo".into(), "it's-baked".into()],
        };
        // Port 1: nothing listens; the report must fail quietly.
        std::fs::write(&init, init_script(ShellKind::PowerShell, "s-1", 1, &[w], "hint")).unwrap();
        // `-File` with the init, then the typed command — the way a session
        // starts, minus -NoExit.
        let runner = dir.join("run.ps1");
        std::fs::write(&runner, format!(". '{}'\nqptest typed two-words\n", init.display())).unwrap();
        let out = Command::new(ps)
            .args(["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&runner)
            .output()
            .expect("powershell runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "powershell failed: {stderr}\n{stdout}");
        assert!(stdout.contains("hint"), "the hint line prints: {stdout}");
        assert!(stdout.contains("it's-baked typed two-words"), "wrapper output: {stdout}\n{stderr}");
    }

    #[test]
    fn bash_init_sources_the_rc_and_forwards_args() {
        let s = init_script(ShellKind::Bash, "abc", 4500, &wrappers(), "hi");
        assert!(s.contains("[ -f ~/.bashrc ] && . ~/.bashrc"));
        assert!(s.contains("claude() { __qp_launched 'claude'; 'C:/Users/me/.local/bin/claude.exe' '--session-id' 'abc' '--settings' 'C:\\d\\it'\\''s.json' \"$@\"; }"));
        assert_eq!(shell_args(ShellKind::Bash, Path::new("C:\\d\\init.sh"), "hi"), vec!["--rcfile", "C:/d/init.sh", "-i"]);
    }
}
