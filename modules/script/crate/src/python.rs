//! The interpreter QuantScript runs the forge's package with.
//!
//! The scripts are Python; listing what the registry knows about them and
//! checking a candidate both run `python -m smithery.quantscript`, which
//! needs numpy and pandas. No module may trust a bare `python` on PATH here
//! (the QuantSystems lesson of 2026-09-08: a venv without pandas answered),
//! so the interpreter is `QUANTSCRIPT_PYTHON` when set, else
//! `QUANTSYSTEMS_PYTHON` (same machine, same dependencies), else the first of
//! `py -3` / `python3` / `python` that imports both — probed once, lazily.

use serde::Serialize;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// What the settings section and the listing show about the interpreter.
#[derive(Debug, Clone, Serialize)]
pub struct PythonInfo {
    /// The command line, e.g. `py -3`.
    pub command: String,
    /// It imports numpy and pandas.
    pub ok: bool,
    pub error: Option<String>,
    /// `QUANTSCRIPT_PYTHON` | `QUANTSYSTEMS_PYTHON` | `probe`.
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct PythonCommand {
    program: String,
    args: Vec<String>,
}

impl PythonCommand {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A command for `python -m smithery.<module> …`, run from the sidecar
    /// tree so the package resolves, with the tree on `PYTHONPATH` for the
    /// child processes the check spawns.
    pub fn command(&self, sidecar_dir: &Path) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args).arg("-u");
        hide_console_window(&mut command);
        command.current_dir(sidecar_dir);
        let mut python_path = OsString::from(sidecar_dir);
        if let Some(existing) = std::env::var_os("PYTHONPATH") {
            python_path.push(if cfg!(windows) { ";" } else { ":" });
            python_path.push(existing);
        }
        command
            .env("PYTHONPATH", python_path)
            // The child prints JSON on a pipe; a cp1252 console encoding
            // would choke on the hypotheses' math.
            .env("PYTHONIOENCODING", "utf-8")
            .env("QUANTSUITE_HOME", qs_core::paths::root())
            .env("QUANTSCRIPT_INDICATORS_DIR", qs_core::paths::indicators_dir())
            .stdin(Stdio::null());
        command
    }

    fn has_deps(&self) -> Result<(), String> {
        let mut probe = Command::new(&self.program);
        probe.args(&self.args).args(["-c", "import numpy, pandas"]);
        hide_console_window(&mut probe);
        probe.stdin(Stdio::null());
        match probe.output() {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                Err(format!(
                    "{} cannot import numpy and pandas{}",
                    self.display(),
                    if last.is_empty() { String::new() } else { format!(": {last}") }
                ))
            }
            Err(e) => Err(format!("{} could not be started: {e}", self.display())),
        }
    }
}

/// CREATE_NO_WINDOW: the release binary is a GUI-subsystem process without a
/// console, so a spawned console child would otherwise open its own visible
/// console window for its whole lifetime.
pub fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn from_env(var: &str) -> Option<PythonCommand> {
    let value = std::env::var(var).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split_whitespace();
    let program = parts.next()?;
    let args: Vec<&str> = parts.collect();
    Some(PythonCommand::new(program, &args))
}

/// Find the interpreter. Never fails outright: without a working candidate
/// the last one is kept so every error names a real interpreter and the
/// operator installs the requirements there — the module keeps viewing,
/// editing and versioning scripts either way, only the checks stand down.
pub fn resolve() -> (PythonCommand, PythonInfo) {
    for var in ["QUANTSCRIPT_PYTHON", "QUANTSYSTEMS_PYTHON"] {
        if let Some(cmd) = from_env(var) {
            let error = cmd.has_deps().err();
            let info = PythonInfo {
                command: cmd.display(),
                ok: error.is_none(),
                error,
                source: var.to_string(),
            };
            return (cmd, info);
        }
    }
    let candidates: Vec<PythonCommand> = if cfg!(windows) {
        vec![
            PythonCommand::new("py", &["-3"]),
            PythonCommand::new("python3", &[]),
            PythonCommand::new("python", &[]),
        ]
    } else {
        vec![PythonCommand::new("python3", &[]), PythonCommand::new("python", &[])]
    };
    let mut errors = Vec::new();
    for cmd in &candidates {
        match cmd.has_deps() {
            Ok(()) => {
                log::info!(target: "script", "interpreter: {}", cmd.display());
                let info = PythonInfo {
                    command: cmd.display(),
                    ok: true,
                    error: None,
                    source: "probe".into(),
                };
                return (cmd.clone(), info);
            }
            Err(e) => errors.push(e),
        }
    }
    log::warn!(target: "script", "no interpreter with numpy and pandas found");
    let last = candidates.last().cloned().unwrap_or_else(|| PythonCommand::new("python", &[]));
    let info = PythonInfo {
        command: last.display(),
        ok: false,
        error: Some(format!(
            "No Python interpreter with numpy and pandas was found ({}). Install sidecars/python/requirements.txt or set QUANTSCRIPT_PYTHON.",
            errors.join("; ")
        )),
        source: "probe".into(),
    };
    (last, info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_line_displays_with_its_arguments() {
        assert_eq!(PythonCommand::new("py", &["-3"]).display(), "py -3");
        assert_eq!(PythonCommand::new("python3", &[]).display(), "python3");
    }

    #[test]
    fn the_sidecar_tree_leads_the_python_path() {
        let cmd = PythonCommand::new("python", &[]).command(Path::new("/tmp/sidecars"));
        let python_path = cmd
            .get_envs()
            .find(|(k, _)| *k == "PYTHONPATH")
            .and_then(|(_, v)| v.map(|v| v.to_string_lossy().to_string()))
            .unwrap();
        assert!(python_path.starts_with("/tmp/sidecars"));
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/tmp/sidecars")));
    }
}
