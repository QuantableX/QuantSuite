//! The agent-callable one-shot (docs/PLAN-CONSOLE.md §7, phase P8).
//!
//! This is the sharpest edge in the module: arbitrary shell execution, offered to
//! the agent layer as an MCP tool. It therefore has **two locks**, and both have
//! to be open (§11 decision 4):
//!
//!   1. the setting `console / agent.exec.enabled`, which ships **off**;
//!   2. QuantMCP's approval mode, which gates the call itself — this command is
//!      deliberately outside the plugin's default permission set, so it is
//!      granted where it is needed rather than handed to every webview.
//!
//! Three decisions about *how* it runs, each one a refusal to be convenient:
//!
//!   - **No PTY, and stdin is closed.** An interactive session would let a command
//!     that asks a question hang forever with nobody to answer it; a closed stdin
//!     makes it fail fast instead. An agent cannot type.
//!   - **A timeout, always.** Without one, `npm install` on a bad network holds a
//!     tool call open until the agent gives up, and the child keeps running.
//!   - **A one-shot shell, not the session's.** Running inside a live session
//!     would interleave an agent's output with the user's blocks and could inherit
//!     half-finished state (a `cd`, an activated venv). This gets its own process
//!     in an explicit directory.

use crate::settings;
use serde::Serialize;
use std::process::Stdio;
use tauri::AppHandle;

/// Output kept per call. Beyond this the tail is dropped and `truncated` says so —
/// an agent that asked for a build log does not need ten megabytes of it, and the
/// MCP transport would carry every byte.
const MAX_OUTPUT: usize = 256 * 1024;

/// Default and ceiling for the timeout. The ceiling is the point: a caller
/// passing `timeout_ms: 3_600_000` would reinvent the hang this guards against.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 300_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    /// stdout and stderr, in the order the process wrote them to each pipe.
    pub text: String,
    /// `None` when the process was killed by a signal or the platform reported
    /// no code — never silently 0, which an agent would read as success.
    pub exit_code: Option<i32>,
    pub truncated: bool,
    pub cwd: String,
    pub command: String,
}

/// How to hand one command line to a shell, per platform.
fn shell_invocation(command: &str) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        // `-NoProfile` because a profile is the user's interactive setup and can
        // print banners, prompt, or take seconds; `-NonInteractive` so a
        // confirmation prompt errors instead of waiting.
        let shell = if std::path::Path::new(
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        )
        .exists()
        {
            "powershell.exe".to_string()
        } else {
            "cmd.exe".to_string()
        };
        if shell.starts_with("powershell") {
            return (
                shell,
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    command.to_string(),
                ],
            );
        }
        (shell, vec!["/C".into(), command.to_string()])
    }
    #[cfg(not(windows))]
    {
        let shell = qs_pty::default_shell().to_string();
        (shell, vec!["-c".into(), command.to_string()])
    }
}

fn clamp(text: Vec<u8>) -> (String, bool) {
    let truncated = text.len() > MAX_OUTPUT;
    let slice = if truncated { &text[..MAX_OUTPUT] } else { &text[..] };
    (String::from_utf8_lossy(slice).into_owned(), truncated)
}

/// Run one command and return what it printed.
///
/// Refuses unless the setting is on; the refusal names the setting, because an
/// agent told only "denied" will retry forever.
#[tauri::command]
pub async fn run_command(
    app: AppHandle,
    command: String,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<CommandResult, String> {
    if command.trim().is_empty() {
        return Err("nothing to run".into());
    }
    if !settings::agent_exec_enabled(&app) {
        return Err(
            "console: agent execution is off. Turn on Settings → QuantConsole → Agent access \
             (console / agent.exec.enabled) to allow it."
                .into(),
        );
    }

    let directory = cwd.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".into())
    });
    if !std::path::Path::new(&directory).is_dir() {
        return Err(format!("not a directory: {directory}"));
    }

    let budget = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS);
    let (shell, args) = shell_invocation(&command);

    let mut cmd = tokio::process::Command::new(&shell);
    // CREATE_NO_WINDOW — the release binary has no console; without the flag
    // every agent command flashes a console window in installed builds.
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let child = cmd
        .args(&args)
        .current_dir(&directory)
        // Closed, not inherited: a command that asks a question must fail rather
        // than wait for an answer that cannot come.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The child dies with the future when the timeout drops it. Without this,
        // a timed-out `npm install` keeps running with nobody watching.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot run {shell}: {e}"))?;

    // Both pipes are read by `wait_with_output`, which is why it is used rather
    // than reading them here: reading one at a time deadlocks as soon as the
    // other pipe's buffer fills, and a `cargo build` fills stderr.
    let output = match tokio::time::timeout(
        std::time::Duration::from_millis(budget),
        child.wait_with_output(),
    )
    .await
    {
        Ok(result) => result.map_err(|e| format!("{command} failed: {e}"))?,
        Err(_) => {
            // The future owned the child, so it is already being killed; there is
            // no partial output to hand back. Saying so beats an empty success.
            return Err(format!(
                "timed out after {budget} ms and was killed; no output captured"
            ));
        }
    };

    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    let (text, truncated) = clamp(bytes);

    Ok(CommandResult {
        text,
        exit_code: output.status.code(),
        truncated,
        cwd: directory,
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_over_the_cap_is_marked_not_silently_cut() {
        let (text, truncated) = clamp(vec![b'x'; MAX_OUTPUT + 10]);
        assert!(truncated);
        assert_eq!(text.len(), MAX_OUTPUT);

        let (text, truncated) = clamp(b"short".to_vec());
        assert!(!truncated);
        assert_eq!(text, "short");
    }

    /// The invocation must be non-interactive on every platform: an agent has no
    /// way to answer a prompt, so a shell that waits for one is a hung tool call.
    #[test]
    fn the_invocation_is_non_interactive() {
        let (shell, args) = shell_invocation("echo hi");
        assert!(!shell.is_empty());
        assert!(args.iter().any(|a| a == "echo hi"), "the command is passed as one argument");
        #[cfg(windows)]
        assert!(
            args.iter().any(|a| a == "-NonInteractive" || a == "/C"),
            "got {args:?}"
        );
        #[cfg(not(windows))]
        assert!(args.iter().any(|a| a == "-c"), "got {args:?}");
    }

    /// A caller asking for an hour gets the ceiling, not the hour: without it the
    /// timeout is advisory and the hang it guards against comes back.
    #[test]
    fn the_timeout_has_a_ceiling() {
        // The same expression the command runs, over the inputs that matter.
        let budget = |asked: Option<u64>| asked.unwrap_or(DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS);
        assert_eq!(budget(Some(3_600_000)), MAX_TIMEOUT_MS, "an hour is clamped");
        assert_eq!(budget(None), DEFAULT_TIMEOUT_MS, "no answer means the default");
        assert_eq!(budget(Some(500)), 500, "a short deadline is honoured");
    }
}
