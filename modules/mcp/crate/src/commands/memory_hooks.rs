//! Memory in prompts (docs/MEMORY-HOOKS.md): which agents carry the
//! QuantMemory prompt hook, and the install / remove buttons on the Connect
//! page. The endpoint the hooks call lives in `crate::memory_hook`.

use crate::clients;
use crate::memory_hook::install::{self, Change};
use crate::memory_hook::HookClient;
use crate::AppState;
use serde::Serialize;

fn note(client: HookClient) -> &'static str {
    match client {
        HookClient::ClaudeCode => "New Claude Code sessions pick it up.",
        HookClient::Codex => "Codex runs a new hook only after you trust it once in /hooks, and shows the added context in its transcript.",
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
    installed: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryHookStatus {
    id: &'static str,
    name: &'static str,
    detected: bool,
    /// Every target file carries the hook.
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
                    .map(|path| match install::installed_in(&path) {
                        Ok(installed) => HookTarget { path: path.display().to_string(), installed, error: None },
                        Err(e) => HookTarget { path: path.display().to_string(), installed: false, error: Some(e) },
                    })
                    .collect(),
                Err(e) => vec![HookTarget { path: String::new(), installed: false, error: Some(e) }],
            };
            MemoryHookStatus {
                id: client.slug(),
                name: name(client),
                detected: detected(client),
                installed: !targets.is_empty() && targets.iter().all(|t| t.installed),
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
