//! Which workspace an agent session means: the folder the agent works in,
//! never the selection in the app (user rule 2026-11-06).
//!
//! The decision is pure and tested here. `mcp_server` keeps each session's
//! folder and puts the outcome into the tool call's arguments, so every tool
//! behind it receives an ordinary `workspace` or `scope` value.

use qs_core::workspaces::{containing_in, normalize_path, WorkspaceEntry};
use serde_json::{json, Value};

/// core.db scope and key of the folders the user answered "no" for.
const DECLINED_SCOPE: &str = "mcp";
const DECLINED_KEY: &str = "agentFolders.declined";

pub const FOLDER_UNKNOWN: &str = "QuantMCP does not know which folder this agent works in, and it does not guess. Call get_instructions with cwd=<your working folder>, or pass `workspace` (name, id, folder path, or \"general\") on this call.";
pub const NEEDS_FOLDER: &str = "This agent works in General, which has no folder, and this tool needs one. Pass `workspace` (name, id or folder path) on this call, or register the folder with answer_workspace_question.";

/// Memory tools whose missing scope means "every scope" rather than a place.
/// A tool not listed here is treated as bound to the session's place, the
/// narrower reading.
const MEMORY_ACROSS_SCOPES: [&str; 6] = ["search", "list", "orphans", "stats", "review_queue", "index_embeddings"];

/// Where a session works.
#[derive(Debug, Clone)]
pub enum AgentPlace {
    /// The agent has not told us, and its client offers no roots.
    Unknown,
    Workspace(WorkspaceEntry),
    /// A folder outside every workspace. `ask` is the folder to offer as a new
    /// workspace, or `None` when the user must not be asked about it.
    General { folder: String, ask: Option<String> },
}

impl AgentPlace {
    /// The value tools take for this place.
    fn target(&self) -> Option<&str> {
        match self {
            AgentPlace::Unknown => None,
            AgentPlace::Workspace(ws) => Some(&ws.id),
            AgentPlace::General { .. } => Some("general"),
        }
    }
}

/// What `place_of` needs to know about the machine.
#[derive(Debug, Default)]
pub struct FolderFacts {
    pub exists: bool,
    /// Folders the user answered "no" for.
    pub declined: Vec<String>,
    /// Home, Desktop, Documents, Downloads: General without a question.
    pub general_places: Vec<String>,
    /// The temp folder: General without a question, subfolders included.
    pub temp: Option<String>,
    /// The git repository around the folder, offered instead of a subfolder.
    pub repo_root: Option<String>,
}

fn is_drive_root(norm: &str) -> bool {
    norm.is_empty() || (norm.len() == 2 && norm.ends_with(':'))
}

fn is_inside(norm: &str, root: &str) -> bool {
    let root = normalize_path(root);
    !root.is_empty() && (norm == root || norm.strip_prefix(root.as_str()).is_some_and(|rest| rest.starts_with('/')))
}

/// The decision: a registered folder is that workspace, anything else is
/// General, and only an ordinary folder gets the question.
pub fn place_of(folder: Option<&str>, workspaces: Vec<WorkspaceEntry>, facts: &FolderFacts) -> AgentPlace {
    let Some(folder) = folder.map(str::trim).filter(|f| !f.is_empty()) else {
        return AgentPlace::Unknown;
    };
    if let Some(ws) = containing_in(workspaces, folder) {
        return AgentPlace::Workspace(ws);
    }
    let norm = normalize_path(folder);
    let general_place = |path: &str| {
        let path = normalize_path(path);
        is_drive_root(&path) || facts.general_places.iter().any(|p| normalize_path(p) == path)
    };
    let in_temp = facts.temp.as_deref().is_some_and(|t| is_inside(&norm, t));
    // A folder that does not exist here belongs to another machine (a client
    // connected to a server): there is nothing to register.
    let quiet = !facts.exists || in_temp || general_place(&norm);
    let offer = facts
        .repo_root
        .as_deref()
        .filter(|root| !general_place(root))
        .unwrap_or(folder)
        .to_string();
    let declined = facts.declined.iter().any(|d| {
        let d = normalize_path(d);
        d == norm || d == normalize_path(&offer)
    });
    AgentPlace::General { folder: folder.to_string(), ask: (!quiet && !declined).then_some(offer) }
}

/// A folder as an agent or client reports it, in the registry's shape. Git
/// Bash reports `/c/Projects/x`, which names `C:/Projects/x` on Windows.
/// `None` for a relative path: it says nothing about where the agent is.
pub fn clean_folder(raw: &str, windows: bool) -> Option<String> {
    let raw = raw.trim();
    let bytes = raw.as_bytes();
    if windows && bytes.len() >= 2 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && (bytes.len() == 2 || bytes[2] == b'/') {
        return Some(format!("{}:{}", (bytes[1] as char).to_ascii_uppercase(), &raw[2..]));
    }
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    (drive || raw.starts_with('/') || raw.starts_with('\\')).then(|| raw.to_string())
}

/// The folder in an MCP root's `file:` URI. VS Code writes `file:///c%3A/x`.
pub fn folder_from_root_uri(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.trim().strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()))
            .flatten();
        match hex {
            Some(byte) => {
                decoded.push(byte);
                i += 3;
            }
            None => {
                decoded.push(bytes[i]);
                i += 1;
            }
        }
    }
    let path = String::from_utf8(decoded).ok()?;
    // `/C:/x` is a drive path on any machine.
    let unslashed = path.strip_prefix('/').filter(|p| p.as_bytes().get(1) == Some(&b':')).unwrap_or(&path);
    clean_folder(unslashed, windows)
}

/// The value given for `param`, unless it is the old word "active", which
/// means the session's place now.
fn explicit<'a>(arguments: &'a Value, param: &str) -> Option<&'a str> {
    arguments
        .get(param)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("active"))
}

fn says_active(arguments: &Value, param: &str) -> bool {
    arguments.get(param).and_then(Value::as_str).is_some_and(|v| v.trim().eq_ignore_ascii_case("active"))
}

fn set_param(arguments: &mut Value, param: &str, target: Option<&str>) {
    if !arguments.is_object() {
        *arguments = json!({});
    }
    let args = arguments.as_object_mut().expect("an object");
    match target {
        Some(value) => {
            args.insert(param.into(), Value::String(value.into()));
        }
        None => {
            args.remove(param);
        }
    }
}

/// The inbuilt tools (code index, AgentOS, worktrees, kanban): the session's
/// place goes into `workspace` unless the call names one. With no place the
/// argument stays out, and the resolver refuses the tools that need it.
pub fn with_place(mut arguments: Value, place: &AgentPlace) -> Value {
    let named = ["workspace", "project_name", "codebase"].iter().any(|p| explicit(&arguments, p).is_some());
    if !named {
        set_param(&mut arguments, "workspace", place.target());
    }
    arguments
}

/// The `quantsuite.*` capabilities. The app serves them too, so a missing
/// argument still means the app's selection there: a call from an agent must
/// never arrive without one. `schema` is the capability's input schema.
pub fn with_place_suite(tool: &str, mut arguments: Value, schema: Option<&Value>, place: &AgentPlace) -> Result<Value, String> {
    let memory = tool.strip_prefix("quantsuite.memory.");
    if memory == Some("context") {
        let mut request = arguments.get("request").cloned().unwrap_or_else(|| json!({}));
        if explicit(&request, "scope").is_none() {
            set_param(&mut request, "scope", Some(place.target().ok_or(FOLDER_UNKNOWN)?));
            set_param_value(&mut arguments, "request", request);
        }
        return Ok(arguments);
    }
    let param = if memory.is_some() { "scope" } else { "workspace" };
    let has_param = schema.and_then(|s| s.get("properties")).and_then(|p| p.get(param)).is_some();
    if !has_param || explicit(&arguments, param).is_some() {
        return Ok(arguments);
    }
    if memory.is_some_and(|name| MEMORY_ACROSS_SCOPES.contains(&name)) && !says_active(&arguments, param) {
        return Ok(arguments);
    }
    match place {
        AgentPlace::Unknown => Err(FOLDER_UNKNOWN.into()),
        AgentPlace::General { .. } if param == "workspace" => Err(NEEDS_FOLDER.into()),
        _ => {
            set_param(&mut arguments, param, place.target());
            Ok(arguments)
        }
    }
}

fn set_param_value(arguments: &mut Value, param: &str, value: Value) {
    if !arguments.is_object() {
        *arguments = json!({});
    }
    arguments.as_object_mut().expect("an object").insert(param.into(), value);
}

/// The first lines of `get_instructions`: where this session works and, for a
/// new folder, the question for the user.
pub fn place_note(place: &AgentPlace) -> String {
    match place {
        AgentPlace::Unknown => format!("WORKING FOLDER UNKNOWN. {FOLDER_UNKNOWN} Tools that need a workspace are refused until then."),
        AgentPlace::Workspace(ws) => format!(
            "Working folder: this session works in workspace '{}' ({}). Tools without `workspace` go there. The selection in the app never affects you.",
            ws.name, ws.path
        ),
        AgentPlace::General { folder, ask: Some(offer) } => format!(
            "Working folder: {folder} is not a workspace, so this session works in General.\nASK THE USER ONCE, in their language: \"Should {offer} be set up as a workspace?\" Then call answer_workspace_question with folder=\"{offer}\" and create=true or false. The answer is remembered, so do not ask again."
        ),
        AgentPlace::General { folder, ask: None } => format!(
            "Working folder: {folder} is not a workspace, so this session works in General: the General board and the general memory vault. Do not offer to register this folder."
        ),
    }
}

// ── The machine and core.db ───────────────────────────────────────────────

fn path_text(path: Option<std::path::PathBuf>) -> Option<String> {
    path.map(|p| p.to_string_lossy().into_owned())
}

pub fn declined_folders(conn: &rusqlite::Connection) -> Vec<String> {
    qs_core::db::get_setting(conn, DECLINED_SCOPE, DECLINED_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Remember the user's "no" for a folder.
pub fn decline_folder(conn: &rusqlite::Connection, folder: &str) -> Result<(), String> {
    let mut declined = declined_folders(conn);
    let norm = normalize_path(folder);
    if !declined.iter().any(|d| normalize_path(d) == norm) {
        declined.push(folder.to_string());
    }
    qs_core::db::set_setting(conn, DECLINED_SCOPE, DECLINED_KEY, &json!(declined)).map_err(|e| e.to_string())
}

/// `place_of` against this machine and the registry. Only `get_instructions`
/// needs the question: every other call runs on a default set of facts, so a
/// tool call in General does not start a git process.
pub fn place_on_this_machine(conn: &rusqlite::Connection, folder: Option<&str>, with_question: bool) -> AgentPlace {
    let facts = folder
        .filter(|_| with_question)
        .map(|f| FolderFacts {
            exists: std::path::Path::new(f).is_dir(),
            declined: declined_folders(conn),
            general_places: [dirs::home_dir(), dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir()]
                .into_iter()
                .filter_map(path_text)
                .collect(),
            temp: path_text(Some(std::env::temp_dir())),
            repo_root: crate::worktree::repo_root(std::path::Path::new(f)),
        })
        .unwrap_or_default();
    place_of(folder, qs_core::workspaces::list(conn), &facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(name: &str, path: &str) -> WorkspaceEntry {
        WorkspaceEntry { id: format!("core:workspace:{name}"), name: name.into(), path: path.into() }
    }

    fn registry() -> Vec<WorkspaceEntry> {
        vec![ws("suite", r"C:\Projects\QuantSuite"), ws("games", r"C:\Projects\QuantGames")]
    }

    fn facts() -> FolderFacts {
        FolderFacts {
            exists: true,
            general_places: vec![r"C:\Users\root".into(), r"C:\Users\root\Desktop".into()],
            temp: Some(r"C:\Users\root\AppData\Local\Temp".into()),
            ..FolderFacts::default()
        }
    }

    fn name_of(place: &AgentPlace) -> String {
        match place {
            AgentPlace::Unknown => "unknown".into(),
            AgentPlace::Workspace(ws) => ws.name.clone(),
            AgentPlace::General { ask: Some(offer), .. } => format!("general, ask {offer}"),
            AgentPlace::General { ask: None, .. } => "general".into(),
        }
    }

    fn place(folder: &str, facts: &FolderFacts) -> String {
        name_of(&place_of(Some(folder), registry(), facts))
    }

    #[test]
    fn a_registered_folder_is_its_workspace() {
        let f = facts();
        assert_eq!(place(r"C:\Projects\QuantSuite", &f), "suite");
        assert_eq!(place(r"c:/projects/quantsuite/modules/mcp", &f), "suite");
        assert_eq!(place(r"C:\Projects\QuantSuite\.qs-worktrees\2d8a94b6", &f), "suite");
        assert_eq!(place(r"C:\Projects\QuantGames", &f), "games");
    }

    #[test]
    fn a_name_that_merely_starts_alike_is_not_inside() {
        assert_eq!(place(r"C:\Projects\QuantSuiteOld", &facts()), r"general, ask C:\Projects\QuantSuiteOld");
    }

    #[test]
    fn a_new_folder_gets_the_question_and_a_no_is_remembered() {
        let mut f = facts();
        assert_eq!(place(r"C:\Projects\NewThing", &f), r"general, ask C:\Projects\NewThing");
        f.declined.push("c:/projects/newthing/".into());
        assert_eq!(place(r"C:\Projects\NewThing", &f), "general");
    }

    #[test]
    fn inside_a_repository_the_question_offers_its_root() {
        let mut f = facts();
        f.repo_root = Some("C:/Projects/NewThing".into());
        assert_eq!(place(r"C:\Projects\NewThing\src", &f), "general, ask C:/Projects/NewThing");
        // A no for the repository covers its subfolders.
        f.declined.push("C:/Projects/NewThing".into());
        assert_eq!(place(r"C:\Projects\NewThing\src", &f), "general");
    }

    #[test]
    fn desktop_and_the_like_go_to_general_without_a_question() {
        let f = facts();
        for folder in [r"C:\Users\root\Desktop", r"C:\Users\root", r"C:\", "D:", "/", r"C:\Users\root\AppData\Local\Temp", r"C:\Users\root\AppData\Local\Temp\x\y"] {
            assert_eq!(place(folder, &f), "general", "{folder}");
        }
        // Only those folders themselves: a project on the Desktop is asked about.
        assert_eq!(place(r"C:\Users\root\Desktop\Project", &f), r"general, ask C:\Users\root\Desktop\Project");
    }

    #[test]
    fn a_home_folder_repository_is_not_offered() {
        let mut f = facts();
        f.repo_root = Some("C:/Users/root".into());
        assert_eq!(place(r"C:\Users\root\Notes", &f), r"general, ask C:\Users\root\Notes");
    }

    #[test]
    fn a_folder_from_another_machine_is_general_without_a_question() {
        let mut f = facts();
        f.exists = false;
        assert_eq!(place("/home/me/project", &f), "general");
        // A registered path still matches by its text.
        assert_eq!(place(r"C:\Projects\QuantSuite\src", &f), "suite");
    }

    #[test]
    fn no_folder_means_unknown() {
        let f = facts();
        assert_eq!(name_of(&place_of(None, registry(), &f)), "unknown");
        assert_eq!(name_of(&place_of(Some("  "), registry(), &f)), "unknown");
    }

    #[test]
    fn folders_are_cleaned_up() {
        assert_eq!(clean_folder("/c/Projects/x", true).as_deref(), Some("C:/Projects/x"));
        assert_eq!(clean_folder("/c", true).as_deref(), Some("C:"));
        assert_eq!(clean_folder("/code/x", true).as_deref(), Some("/code/x"));
        assert_eq!(clean_folder("/c/Projects/x", false).as_deref(), Some("/c/Projects/x"));
        assert_eq!(clean_folder(r"C:\Projects\x", true).as_deref(), Some(r"C:\Projects\x"));
        assert_eq!(clean_folder("Projects/x", true), None);
        assert_eq!(clean_folder(".", true), None);
    }

    #[test]
    fn root_uris_become_folders() {
        assert_eq!(folder_from_root_uri("file:///C:/Projects/QuantSuite", true).as_deref(), Some("C:/Projects/QuantSuite"));
        assert_eq!(folder_from_root_uri("file:///c%3A/Projects/My%20App", true).as_deref(), Some("c:/Projects/My App"));
        assert_eq!(folder_from_root_uri("file://localhost/C:/x", true).as_deref(), Some("C:/x"));
        assert_eq!(folder_from_root_uri("file:///home/me/w%C3%B6rk", false).as_deref(), Some("/home/me/wörk"));
        assert_eq!(folder_from_root_uri("https://example.com/x", true), None);
    }

    fn suite_place() -> AgentPlace {
        AgentPlace::Workspace(ws("suite", r"C:\Projects\QuantSuite"))
    }

    fn general_place() -> AgentPlace {
        AgentPlace::General { folder: "x".into(), ask: None }
    }

    #[test]
    fn inbuilt_tools_get_the_place_unless_the_call_names_one() {
        let filled = with_place(json!({"title": "t"}), &suite_place());
        assert_eq!(filled["workspace"], "core:workspace:suite");
        assert_eq!(filled["title"], "t");
        assert_eq!(with_place(json!({}), &general_place())["workspace"], "general");
        // Explicit wins, the aliases count, and "active" is no choice anymore.
        assert_eq!(with_place(json!({"workspace": "QuantGames"}), &suite_place())["workspace"], "QuantGames");
        assert!(with_place(json!({"project_name": "QuantGames"}), &suite_place()).get("workspace").is_none());
        assert_eq!(with_place(json!({"workspace": "active"}), &suite_place())["workspace"], "core:workspace:suite");
        // No place: nothing goes in, so the resolver refuses instead of guessing.
        assert!(with_place(json!({"workspace": "active"}), &AgentPlace::Unknown).get("workspace").is_none());
        assert!(with_place(json!({}), &AgentPlace::Unknown).get("workspace").is_none());
    }

    #[test]
    fn memory_writes_and_lookups_follow_the_place() {
        let schema = json!({"properties": {"scope": {}, "title": {}}});
        let create = "quantsuite.memory.create";
        let filled = with_place_suite(create, json!({"title": "t"}), Some(&schema), &suite_place()).unwrap();
        assert_eq!(filled["scope"], "core:workspace:suite");
        assert_eq!(with_place_suite(create, json!({}), Some(&schema), &general_place()).unwrap()["scope"], "general");
        assert_eq!(with_place_suite(create, json!({"scope": "general"}), Some(&schema), &suite_place()).unwrap()["scope"], "general");
        assert_eq!(with_place_suite(create, json!({"scope": "active"}), Some(&schema), &suite_place()).unwrap()["scope"], "core:workspace:suite");
        assert_eq!(with_place_suite(create, json!({}), Some(&schema), &AgentPlace::Unknown).unwrap_err(), FOLDER_UNKNOWN);
    }

    #[test]
    fn memory_searches_keep_reading_every_scope() {
        let schema = json!({"properties": {"scope": {}}});
        let search = "quantsuite.memory.search";
        for place in [suite_place(), general_place(), AgentPlace::Unknown] {
            assert!(with_place_suite(search, json!({"query": "x"}), Some(&schema), &place).unwrap().get("scope").is_none());
        }
        assert_eq!(with_place_suite(search, json!({"scope": "active"}), Some(&schema), &suite_place()).unwrap()["scope"], "core:workspace:suite");
        assert!(with_place_suite(search, json!({"scope": "active"}), Some(&schema), &AgentPlace::Unknown).is_err());
    }

    #[test]
    fn memory_context_takes_its_scope_inside_the_request() {
        let tool = "quantsuite.memory.context";
        let filled = with_place_suite(tool, json!({"request": {"query": "x"}}), None, &suite_place()).unwrap();
        assert_eq!(filled["request"]["scope"], "core:workspace:suite");
        assert_eq!(filled["request"]["query"], "x");
        let named = with_place_suite(tool, json!({"request": {"query": "x", "scope": "general"}}), None, &suite_place()).unwrap();
        assert_eq!(named["request"]["scope"], "general");
        assert!(with_place_suite(tool, json!({"request": {"query": "x"}}), None, &AgentPlace::Unknown).is_err());
    }

    #[test]
    fn other_suite_tools_get_a_workspace_only_where_they_take_one() {
        let takes = json!({"properties": {"workspace": {}}});
        let tool = "quantsuite.canvas.design_list";
        assert_eq!(with_place_suite(tool, json!({}), Some(&takes), &suite_place()).unwrap()["workspace"], "core:workspace:suite");
        assert_eq!(with_place_suite(tool, json!({}), Some(&takes), &general_place()).unwrap_err(), NEEDS_FOLDER);
        assert_eq!(with_place_suite(tool, json!({}), Some(&takes), &AgentPlace::Unknown).unwrap_err(), FOLDER_UNKNOWN);
        assert_eq!(with_place_suite(tool, json!({"workspace": "QuantGames"}), Some(&takes), &general_place()).unwrap()["workspace"], "QuantGames");
        let other = json!({"properties": {"amount": {}}});
        let untouched = with_place_suite("quantsuite.finance.create_item", json!({"amount": 1}), Some(&other), &AgentPlace::Unknown).unwrap();
        assert!(untouched.get("workspace").is_none());
    }

    #[test]
    fn the_question_appears_only_for_a_new_folder() {
        let asks = place_note(&AgentPlace::General { folder: "C:/x".into(), ask: Some("C:/x".into()) });
        assert!(asks.contains("ASK THE USER") && asks.contains("answer_workspace_question"));
        assert!(!place_note(&general_place()).contains("ASK THE USER"));
        assert!(!place_note(&suite_place()).contains("ASK THE USER"));
        assert!(place_note(&AgentPlace::Unknown).contains("cwd="));
    }
}
