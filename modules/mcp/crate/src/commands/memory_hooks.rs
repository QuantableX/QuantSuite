//! Memory in prompts (docs/MEMORY-HOOKS.md): which agents carry the
//! QuantMemory hooks (session-start index, per-prompt excerpts), the install
//! / remove buttons on the Connect page, and the Claude Code auto-memory
//! switch. The endpoint the hooks call lives in `crate::memory_hook`.

use crate::clients;
use crate::memory_hook::install::{self, Change};
use crate::memory_hook::{auto_memory, HookClient};
use crate::AppState;
use serde::Serialize;

fn note(client: HookClient) -> &'static str {
    match client {
        HookClient::ClaudeCode => "New Claude Code sessions start with the index and get excerpts with each prompt.",
        HookClient::Codex => "Codex runs a new or changed hook only after you trust it once in /hooks, and shows the added context in its transcript.",
        HookClient::Cursor => "Cursor adds the index when an agent chat starts and excerpts with each prompt; reload Cursor after installing.",
    }
}

fn detected(client: HookClient) -> bool {
    clients::spec(client.slug()).is_some_and(|spec| spec.installed())
}

fn name(client: HookClient) -> &'static str {
    clients::spec(client.slug()).map_or(client.slug(), |spec| spec.name)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookTarget {
    path: String,
    /// The file holds at least one hook of ours.
    installed: bool,
    /// ... and one for every event the client hooks into.
    complete: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryHookStatus {
    id: &'static str,
    name: &'static str,
    detected: bool,
    /// Every target file carries all of the client's hooks.
    installed: bool,
    targets: Vec<HookTarget>,
    note: &'static str,
}

/// One row per agent whose prompt hooks can add context.
#[tauri::command(async)]
pub(crate) fn memory_hook_status() -> Vec<MemoryHookStatus> {
    HookClient::ALL
        .into_iter()
        .map(|client| {
            let targets: Vec<HookTarget> = match install::targets(client) {
                Ok(paths) => paths
                    .into_iter()
                    .map(|path| {
                        let state = install::installed_in(&path)
                            .and_then(|installed| Ok((installed, install::complete_in(&path, client)?)));
                        match state {
                            Ok((installed, complete)) => {
                                HookTarget { path: path.display().to_string(), installed, complete, error: None }
                            }
                            Err(e) => HookTarget {
                                path: path.display().to_string(),
                                installed: false,
                                complete: false,
                                error: Some(e),
                            },
                        }
                    })
                    .collect(),
                Err(e) => vec![HookTarget { path: String::new(), installed: false, complete: false, error: Some(e) }],
            };
            MemoryHookStatus {
                id: client.slug(),
                name: name(client),
                detected: detected(client),
                installed: !targets.is_empty() && targets.iter().all(|t| t.complete),
                targets,
                note: note(client),
            }
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryHookResult {
    id: &'static str,
    name: &'static str,
    path: String,
    change: Option<Change>,
    error: Option<String>,
}

/// Install (`enabled`) or remove the hook for the selected agents. The whole
/// selection is validated before any file is touched; installing skips an
/// agent that is not on this machine, removing never does.
#[tauri::command(async)]
pub(crate) fn set_memory_hooks(
    state: tauri::State<'_, AppState>,
    client_ids: Vec<String>,
    enabled: bool,
) -> Result<Vec<MemoryHookResult>, String> {
    if client_ids.is_empty() {
        return Err("Select at least one agent.".into());
    }
    let selected = client_ids
        .iter()
        .map(|id| HookClient::from_slug(id).ok_or_else(|| format!("{id} has no prompt hook QuantMemory can use.")))
        .collect::<Result<Vec<_>, _>>()?;
    let state_dir = install::state_dir();
    let mut report = Vec::new();
    for client in selected {
        if enabled && !detected(client) {
            report.push(MemoryHookResult {
                id: client.slug(),
                name: name(client),
                path: String::new(),
                change: None,
                error: Some("not installed on this machine".into()),
            });
            continue;
        }
        let paths = match install::targets(client) {
            Ok(paths) => paths,
            Err(e) => {
                report.push(MemoryHookResult { id: client.slug(), name: name(client), path: String::new(), change: None, error: Some(e) });
                continue;
            }
        };
        for path in paths {
            let outcome = if enabled {
                install::install_file(&path, &state_dir, state.mcp_server_port, client)
            } else {
                install::uninstall_file(&path, &state_dir)
            };
            let (change, error) = match outcome {
                Ok(change) => (Some(change), None),
                Err(e) => (None, Some(e)),
            };
            report.push(MemoryHookResult { id: client.slug(), name: name(client), path: path.display().to_string(), change, error });
        }
    }
    Ok(report)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutoMemoryStatus {
    path: String,
    /// Claude Code is on this machine.
    detected: bool,
    /// Claude Code's own auto memory is on (its default).
    enabled: bool,
    error: Option<String>,
}

/// Whether Claude Code's auto memory is on, per its user settings.
#[tauri::command(async)]
pub(crate) fn claude_auto_memory_status() -> AutoMemoryStatus {
    let detected = detected(HookClient::ClaudeCode);
    let path = match auto_memory::settings_path() {
        Ok(path) => path,
        Err(e) => return AutoMemoryStatus { path: String::new(), detected, enabled: true, error: Some(e) },
    };
    let read = install_read(&path).and_then(|text| auto_memory::enabled_in(&text));
    AutoMemoryStatus {
        path: path.display().to_string(),
        detected,
        enabled: *read.as_ref().unwrap_or(&true),
        error: read.err(),
    }
}

fn install_read(path: &std::path::Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Cannot read {}: {e}", path.display())),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutoMemoryResult {
    path: String,
    change: auto_memory::Change,
}

/// Turn Claude Code's auto memory off (`enabled: false`) or back on. Only
/// the one key in its user settings changes; memory files stay as they are.
#[tauri::command(async)]
pub(crate) fn set_claude_auto_memory(enabled: bool) -> Result<AutoMemoryResult, String> {
    let path = auto_memory::settings_path()?;
    let state_dir = install::state_dir();
    let change = if enabled {
        auto_memory::turn_on(&path, &state_dir)?
    } else {
        auto_memory::turn_off(&path, &state_dir)?
    };
    Ok(AutoMemoryResult { path: path.display().to_string(), change })
}
