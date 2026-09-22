//! Connect (PLAN-QUANTMCP-CONNECT): the scan, the one button and the standard
//! snippet — for QuantMCP itself and for any registered MCP entry. The client
//! table behind it lives in `crate::clients`; nothing here knows a client by
//! name.

use crate::clients::{self, ClientScan, ConnectResult, Target};
use crate::mcp::McpEntry;
use crate::mcp_server;
use crate::{lock, AppState};
use serde::Serialize;
use std::collections::HashMap;

fn entry_by_id(state: &AppState, id: &str) -> Result<McpEntry, String> {
    let registry = lock(&state.registry);
    registry.get(id).cloned().ok_or_else(|| "MCP not found".to_string())
}

/// Which clients are installed here and which already list QuantMCP. No
/// client config is touched; a client found configured lands in the client
/// registry as `detected`, so the Connected-AIs list knows it.
#[tauri::command(async)]
pub(crate) fn scan_clients(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    custom_recipients: Option<Vec<clients::CustomRecipient>>,
) -> Result<Vec<ClientScan>, String> {
    // Preview custom settings before persisting, so invalid paths or aliases
    // cannot poison the saved list and prevent subsequent discovery.
    let custom = match custom_recipients {
        Some(custom) => {
            clients::validate_custom_recipients(&custom)?;
            custom
        }
        None => clients::load_custom_recipients(&app)?,
    };
    let mut scans = clients::scan(&Target::QuantMcp {
        port: state.mcp_server_port,
    });
    if let Err(e) = clients::record_scan(&app, &scans, qs_core::db::now_ms()) {
        eprintln!("[mcp] client registry: {e}");
    }
    scans.extend(custom.iter().map(clients::CustomRecipient::scan));
    Ok(scans)
}

/// Write QuantMCP only into explicitly selected clients found on this machine.
#[tauri::command]
pub(crate) async fn connect_clients(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    instructions_only: Option<bool>,
    client_ids: Option<Vec<String>>,
) -> Result<Vec<ConnectResult>, String> {
    if instructions_only.unwrap_or(false) {
        return clients::import_agent_instructions(client_ids.as_deref().unwrap_or_default(), &clients::load_custom_recipients(&app)?);
    }
    let port = state.mcp_server_port;
    let report = clients::connect_selected(&Target::QuantMcp { port }, client_ids.as_deref().unwrap_or_default()).await?;
    if let Err(e) = clients::record_connect(&app, &report, qs_core::db::now_ms()) {
        eprintln!("[mcp] client registry: {e}");
    }
    let written: Vec<&str> = report
        .iter()
        .filter(|r| matches!(r.outcome, clients::Outcome::Written | clients::Outcome::Ran))
        .map(|r| r.id.as_str())
        .collect();
    let _ = qs_core::bus::emit(&app, "mcp.client.installed", serde_json::json!({ "clients": written }));
    Ok(report)
}

/// The same button for one registered external MCP entry.
#[tauri::command]
pub(crate) async fn connect_mcp_entry(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<ConnectResult>, String> {
    let entry = entry_by_id(&state, &id)?;
    Ok(clients::connect(&Target::Entry(&entry)).await)
}

#[derive(Serialize)]
pub(crate) struct ConnectSnippet {
    name: String,
    /// The Streamable HTTP URL — absent for a stdio entry.
    url: Option<String>,
    /// Plain MCP: `mcpServers` → `{ "type": "http", "url" }`.
    snippet: String,
}

/// URL and standard snippet for QuantMCP (no `id`) or an MCP entry.
#[tauri::command(async)]
pub(crate) fn get_connect_snippet(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
) -> Result<ConnectSnippet, String> {
    let entry = match id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => Some(entry_by_id(&state, id)?),
        None => None,
    };
    let target = match &entry {
        Some(e) => Target::Entry(e),
        None => Target::QuantMcp {
            port: state.mcp_server_port,
        },
    };
    Ok(ConnectSnippet {
        name: target.name().to_string(),
        url: target.url(),
        snippet: clients::snippet(&target),
    })
}

/// One row of the Connected-AIs list (PLAN-QUANTMCP-CONNECT §3.2): a client
/// registry record, with the live sessions of that client on top.
#[derive(Serialize)]
pub(crate) struct ClientRow {
    id: String,
    name: String,
    spec_id: Option<String>,
    online: bool,
    sessions: usize,
    /// Earliest `connected_at` among the live sessions (ms since the epoch).
    connected_since: Option<i64>,
    last_seen: Option<i64>,
    last_version: Option<String>,
    installed: bool,
    installed_to: Vec<String>,
    install_source: Option<String>,
}

/// Registry ∪ live sessions, grouped by client: online first, then by
/// last activity. A client never installed and never connected is not here.
#[tauri::command]
pub(crate) async fn list_clients(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ClientRow>, String> {
    let (live, went_offline) = mcp_server::prune_and_list(&state.active_clients).await;
    for session in &went_offline {
        mcp_server::announce_disconnected(&app, session);
    }

    let mut rows: HashMap<String, ClientRow> = clients::load_records(&app)?
        .into_iter()
        .map(|(entity, record)| {
            let row = ClientRow {
                id: entity.id.clone(),
                name: entity.title,
                spec_id: record.spec_id.clone(),
                online: false,
                sessions: 0,
                connected_since: None,
                last_seen: record.last_seen,
                last_version: record.last_version.clone(),
                installed: record.installed(),
                installed_to: record.installed_to.clone(),
                install_source: record.install_source.clone(),
            };
            (entity.id, row)
        })
        .collect();

    for session in live {
        let key = clients::client_key(&session.client_name);
        let row = rows.entry(key.id.clone()).or_insert_with(|| ClientRow {
            id: key.id.clone(),
            name: key.title.clone(),
            spec_id: key.spec_id.clone(),
            online: false,
            sessions: 0,
            connected_since: None,
            last_seen: None,
            last_version: None,
            installed: false,
            installed_to: Vec::new(),
            install_source: None,
        });
        row.online = true;
        row.sessions += 1;
        row.connected_since = Some(
            row.connected_since
                .map_or(session.connected_at, |c| c.min(session.connected_at)),
        );
        if session.client_version.is_some() {
            row.last_version = session.client_version;
        }
        row.last_seen = Some(row.last_seen.map_or(qs_core::db::now_ms(), |l| l.max(session.connected_at)));
    }

    let mut rows: Vec<ClientRow> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then_with(|| b.last_seen.cmp(&a.last_seen))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(rows)
}

/// Drop a client from the registry — the row, not any config file.
#[tauri::command(async)]
pub(crate) fn forget_client(id: String) -> Result<(), String> {
    clients::forget_record(&id)
}

#[tauri::command(async)]
pub(crate) fn get_mcp_server_port(state: tauri::State<'_, AppState>) -> u16 {
    state.mcp_server_port
}
