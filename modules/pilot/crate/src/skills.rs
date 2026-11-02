//! The skill library (parity 09): skills imported once into QuantPilot's own
//! store, activated per provider and scope by copying them where the CLI
//! itself looks, with every written file's hash recorded.
//!
//! | Provider    | User scope                      | Project scope             |
//! |-------------|---------------------------------|---------------------------|
//! | Claude Code | `<CLAUDE_CONFIG_DIR|~/.claude>/skills/<id>` | `<checkout>/.claude/skills/<id>` |
//! | Codex       | `~/.agents/skills/<id>`         | `<checkout>/.agents/skills/<id>` |
//! | others      | unsupported — never shown as installed                     |
//!
//! (Codex 0.166 `skills/list`: user skills from `~/.agents/skills`, repo
//! skills from `.agents/skills` and `.codex/skills`.)
//!
//! Ownership is per file: QuantPilot only overwrites or deletes a file whose
//! hash is still the one it wrote. An edited copy is "drifted" and kept on
//! deactivation; a folder it did not create is "unmanaged" and never touched.
//! Importing again changes only the library — active copies say "outdated"
//! until activated again. Rules stay in AgentOS (QuantMCP), not here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 64;
pub const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Activation {
    pub provider: String,
    /// `user` | `project`
    pub scope: String,
    /// The checkout of a project activation.
    pub root: String,
    /// The folder written.
    pub dir: String,
    /// The library version it holds.
    pub library_hash: String,
    /// Relative path → SHA-256 of what was written.
    pub files: BTreeMap<String, String>,
    /// The `.git/info/exclude` line it added (project scope in Git).
    pub exclude: Option<(String, String)>,
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Where it was imported from (a folder).
    pub source: String,
    pub hash: String,
    pub files: Vec<String>,
    pub imported_at: i64,
    pub activations: Vec<Activation>,
}

/// A skill folder read for import: name, description and files.
#[derive(Debug)]
pub struct Scanned {
    pub name: String,
    pub description: String,
    pub files: Vec<(String, Vec<u8>)>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The library version: the files' paths and contents together.
pub fn hash_files(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (rel, bytes) in sorted {
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(sha(bytes).as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// `name` / `description` from SKILL.md frontmatter (plain, quoted or `>`/`|` blocks).
pub fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let body: Vec<&str> = lines.take_while(|l| l.trim() != "---").collect();
    let get = |key: &str| -> Option<String> {
        let i = body.iter().position(|l| l.trim_start() == *l && l.starts_with(&format!("{key}:")))?;
        let rest = body[i][key.len() + 1..].trim();
        let value = if rest == ">" || rest == "|" || rest == ">-" || rest == "|-" {
            let block: Vec<&str> = body[i + 1..].iter().take_while(|l| l.starts_with(' ') || l.is_empty()).map(|l| l.trim()).collect();
            block.join(if rest.starts_with('>') { " " } else { "\n" }).trim().to_string()
        } else {
            rest.trim_matches('"').trim_matches('\'').to_string()
        };
        (!value.is_empty()).then_some(value)
    };
    (get("name"), get("description"))
}

/// A folder with a SKILL.md, within the limits; links are refused.
pub fn scan_folder(dir: &Path) -> Result<Scanned, String> {
    let skill_md = dir.join("SKILL.md");
    let text = std::fs::read_to_string(&skill_md).map_err(|_| format!("{} has no SKILL.md.", dir.display()))?;
    let (name, description) = frontmatter(&text);
    let name = name.ok_or("SKILL.md has no `name` in its frontmatter.")?;
    let description = description.ok_or("SKILL.md has no `description` in its frontmatter.")?;
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten() {
            let ft = e.file_type().map_err(|e| e.to_string())?;
            let path = e.path();
            let fname = e.file_name().to_string_lossy().into_owned();
            if fname == ".git" {
                continue;
            }
            if ft.is_symlink() {
                return Err(format!("{} is a link; skills are imported without links.", path.display()));
            }
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            let len = e.metadata().map_err(|e| e.to_string())?.len();
            total += len;
            if files.len() >= MAX_FILES {
                return Err(format!("The skill has more than {MAX_FILES} files."));
            }
            if total > MAX_BYTES {
                return Err(format!("The skill is larger than {} KiB.", MAX_BYTES / 1024));
            }
            let rel = path.strip_prefix(dir).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
            files.push((rel, std::fs::read(&path).map_err(|e| e.to_string())?));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Scanned { name, description, files })
}

/// A folder-safe id from a skill name.
pub fn id_of(name: &str) -> String {
    let id: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(64)
        .collect();
    if id.is_empty() { "skill".into() } else { id }
}

pub struct Library {
    pub root: PathBuf,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(format!("qp-tmp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

impl Library {
    pub fn new(root: PathBuf) -> Self {
        Library { root }
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    pub fn content_dir(&self, id: &str) -> PathBuf {
        self.root.join("lib").join(id)
    }

    pub fn items(&self) -> Vec<Item> {
        let mut out: Vec<Item> = std::fs::read_dir(&self.root)
            .map(|r| {
                r.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                    .filter_map(|t| serde_json::from_str::<Item>(&t).ok())
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    pub fn get(&self, id: &str) -> Option<Item> {
        std::fs::read_to_string(self.meta_path(id)).ok().and_then(|t| serde_json::from_str(&t).ok())
    }

    pub fn save(&self, item: &Item) -> Result<(), String> {
        write_atomic(&self.meta_path(&item.id), serde_json::to_string_pretty(item).map_err(|e| e.to_string())?.as_bytes())
    }

    /// Import (or update) from a folder. Another source with the same name
    /// is a collision: refused unless `replace`.
    pub fn import(&self, dir: &Path, replace: bool) -> Result<Item, String> {
        let scanned = scan_folder(dir)?;
        let id = id_of(&scanned.name);
        let source = dir.to_string_lossy().into_owned();
        let existing = self.get(&id);
        if let Some(e) = &existing {
            if !e.source.eq_ignore_ascii_case(&source) && !replace {
                return Err(format!("The library already has \"{}\" from {}. Import with replace to swap its source.", e.name, e.source));
            }
        }
        // The new content goes to a fresh folder, then takes the old one's place.
        let target = self.content_dir(&id);
        let staging = self.root.join("lib").join(format!(".{id}-{}", uuid::Uuid::new_v4().simple()));
        for (rel, bytes) in &scanned.files {
            write_atomic(&staging.join(rel), bytes)?;
        }
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&staging, &target).map_err(|e| e.to_string())?;
        let item = Item {
            id: id.clone(),
            name: scanned.name.clone(),
            description: scanned.description.clone(),
            source,
            hash: hash_files(&scanned.files),
            files: scanned.files.iter().map(|f| f.0.clone()).collect(),
            imported_at: now(),
            activations: existing.map(|e| e.activations).unwrap_or_default(),
        };
        self.save(&item)?;
        Ok(item)
    }

    fn content(&self, item: &Item) -> Result<Vec<(String, Vec<u8>)>, String> {
        item.files
            .iter()
            .map(|rel| std::fs::read(self.content_dir(&item.id).join(rel)).map(|b| (rel.clone(), b)).map_err(|e| format!("the library copy is damaged ({rel}: {e})")))
            .collect()
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let item = self.get(id).ok_or("Unknown skill.")?;
        if !item.activations.is_empty() {
            return Err("Deactivate it everywhere first.".into());
        }
        let _ = std::fs::remove_dir_all(self.content_dir(id));
        std::fs::remove_file(self.meta_path(id)).map_err(|e| e.to_string())
    }
}

/// Where a provider looks for skills in a scope; `None` = no integration.
pub fn surface(provider: &str, scope: &str, root: Option<&Path>, homes: &Homes) -> Option<PathBuf> {
    match (provider, scope) {
        ("claude", "user") => Some(homes.claude.join("skills")),
        ("claude", "project") => root.map(|r| r.join(".claude").join("skills")),
        ("codex", "user") => Some(homes.agents.join("skills")),
        ("codex", "project") => root.map(|r| r.join(".agents").join("skills")),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct Homes {
    pub claude: PathBuf,
    /// `~/.agents` (Codex's user skills).
    pub agents: PathBuf,
}

impl Homes {
    pub fn detect() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Homes {
            claude: std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".claude")),
            agents: home.join(".agents"),
        }
    }
}

/// An activation's state: `active`, `outdated`, `drifted` or `missing`.
pub fn state_of(item: &Item, a: &Activation) -> (String, Vec<String>) {
    let dir = Path::new(&a.dir);
    if !dir.is_dir() {
        return ("missing".into(), Vec::new());
    }
    let drifted: Vec<String> = a
        .files
        .iter()
        .filter(|(rel, want)| std::fs::read(dir.join(rel)).map(|b| sha(&b) != **want).unwrap_or(true))
        .map(|(rel, _)| rel.clone())
        .collect();
    if !drifted.is_empty() {
        return ("drifted".into(), drifted);
    }
    if a.library_hash != item.hash {
        return ("outdated".into(), Vec::new());
    }
    ("active".into(), Vec::new())
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub written: Vec<String>,
    /// Files left as they are: edited by someone else.
    pub kept: Vec<String>,
    pub dir: String,
}

fn git_exclude(root: &Path) -> Option<PathBuf> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["rev-parse", "--git-path", "info/exclude"]).current_dir(root).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    Some(if p.is_absolute() { p } else { root.join(p) })
}

/// Copy the library version into the provider's surface. A folder there that
/// QuantPilot did not write is never touched; files edited since the last
/// activation are kept unless `keep_edited` is false, which then refuses.
pub fn activate(lib: &Library, id: &str, provider: &str, scope: &str, root: Option<&Path>, homes: &Homes, keep_edited: bool) -> Result<Change, String> {
    let mut item = lib.get(id).ok_or("Unknown skill.")?;
    let base = surface(provider, scope, root, homes).ok_or_else(|| format!("{provider} has no {scope} skills QuantPilot can manage."))?;
    let dir = base.join(&item.id);
    let dir_s = dir.to_string_lossy().into_owned();
    let pos = item.activations.iter().position(|a| a.dir.eq_ignore_ascii_case(&dir_s));
    let previous = pos.map(|i| item.activations[i].clone());
    if previous.is_none() && dir.exists() {
        return Err(format!("{} already exists and was not written by QuantPilot — it is left alone.", dir.display()));
    }
    let content = lib.content(&item)?;
    let mut change = Change { dir: dir_s.clone(), ..Default::default() };
    let mut files = BTreeMap::new();
    for (rel, bytes) in &content {
        let path = dir.join(rel);
        let ours = previous.as_ref().and_then(|p| p.files.get(rel)).cloned();
        let current = std::fs::read(&path).ok().map(|b| sha(&b));
        let edited = match (&current, &ours) {
            (Some(c), Some(o)) => c != o,
            (Some(_), None) => true,
            _ => false,
        };
        if edited {
            if !keep_edited {
                return Err(format!("{rel} was edited in {} — activate with keep edited to leave it as it is.", dir.display()));
            }
            change.kept.push(rel.clone());
            if let Some(o) = ours {
                files.insert(rel.clone(), o);
            }
            continue;
        }
        write_atomic(&path, bytes)?;
        files.insert(rel.clone(), sha(bytes));
        change.written.push(rel.clone());
    }
    // Files of the old version the new one no longer has: removed if untouched.
    if let Some(p) = &previous {
        for (rel, want) in &p.files {
            if !files.contains_key(rel) && !content.iter().any(|c| &c.0 == rel) {
                let path = dir.join(rel);
                if std::fs::read(&path).map(|b| sha(&b) == *want).unwrap_or(false) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
    let mut exclude = previous.as_ref().and_then(|p| p.exclude.clone());
    if scope == "project" && exclude.is_none() {
        if let (Some(r), Some(file)) = (root, root.and_then(git_exclude)) {
            let rel = dir.strip_prefix(r).map(|p| format!("/{}/", p.to_string_lossy().replace('\\', "/"))).unwrap_or_default();
            if !rel.is_empty() {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                if !text.lines().any(|l| l.trim() == rel) {
                    let mut t = text.clone();
                    if !t.is_empty() && !t.ends_with('\n') {
                        t.push('\n');
                    }
                    t.push_str(&rel);
                    t.push('\n');
                    if write_atomic(&file, t.as_bytes()).is_ok() {
                        exclude = Some((file.to_string_lossy().into_owned(), rel));
                    }
                }
            }
        }
    }
    let a = Activation {
        provider: provider.into(),
        scope: scope.into(),
        root: root.map(|r| r.to_string_lossy().into_owned()).unwrap_or_default(),
        dir: dir_s,
        library_hash: item.hash.clone(),
        files,
        exclude,
        at: now(),
    };
    match pos {
        Some(i) => item.activations[i] = a,
        None => item.activations.push(a),
    }
    lib.save(&item)?;
    Ok(change)
}

/// Remove what QuantPilot wrote; edited files stay (and are named).
pub fn deactivate(lib: &Library, id: &str, provider: &str, scope: &str, root: Option<&Path>, homes: &Homes) -> Result<Change, String> {
    let mut item = lib.get(id).ok_or("Unknown skill.")?;
    let base = surface(provider, scope, root, homes).ok_or("No such activation.")?;
    let dir = base.join(&item.id);
    let dir_s = dir.to_string_lossy().into_owned();
    let i = item.activations.iter().position(|a| a.dir.eq_ignore_ascii_case(&dir_s)).ok_or("It is not active there.")?;
    let a = item.activations.remove(i);
    let mut change = Change { dir: dir_s, ..Default::default() };
    for (rel, want) in &a.files {
        let path = dir.join(rel);
        match std::fs::read(&path) {
            Ok(b) if sha(&b) == *want => {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                change.written.push(rel.clone());
            }
            Ok(_) => change.kept.push(rel.clone()),
            Err(_) => {}
        }
    }
    // Empty folders go, bottom up; anything else in them stays.
    let mut dirs: Vec<PathBuf> = a.files.keys().filter_map(|r| dir.join(r).parent().map(Path::to_path_buf)).collect();
    dirs.push(dir.clone());
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    dirs.dedup();
    for d in dirs {
        if d.starts_with(&dir) {
            let _ = std::fs::remove_dir(&d);
        }
    }
    if let Some((file, line)) = &a.exclude {
        if let Ok(text) = std::fs::read_to_string(file) {
            let kept: Vec<&str> = text.lines().filter(|l| l.trim() != line).collect();
            let _ = write_atomic(Path::new(file), (kept.join("\n") + "\n").as_bytes());
        }
    }
    lib.save(&item)?;
    Ok(change)
}

/// Skill folders in a surface QuantPilot does not manage.
pub fn unmanaged(base: &Path, managed: &[String]) -> Vec<(String, String)> {
    std::fs::read_dir(base)
        .map(|r| {
            r.flatten()
                .filter(|e| e.path().join("SKILL.md").is_file())
                .filter(|e| !managed.iter().any(|m| Path::new(m) == e.path()))
                .map(|e| {
                    let (name, _) = frontmatter(&std::fs::read_to_string(e.path().join("SKILL.md")).unwrap_or_default());
                    (name.unwrap_or_else(|| e.file_name().to_string_lossy().into_owned()), e.path().to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Commands ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub supported: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub provider: String,
    pub scope: String,
    /// `active` | `outdated` | `drifted` | `missing` | `off`
    pub state: String,
    pub dir: Option<String>,
    pub drifted: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    #[serde(flatten)]
    pub item: Item,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub providers: Vec<ProviderView>,
    pub items: Vec<ItemView>,
    /// Skills in the surfaces QuantPilot does not manage: provider, scope, name, folder.
    pub unmanaged: Vec<(String, String, String, String)>,
    /// The thread whose checkout "project" means.
    pub project: Option<(String, String)>,
}

fn library(state: &crate::PilotState) -> Library {
    Library::new(state.data_dir.join("skills"))
}

fn project_root(state: &crate::PilotState, thread: &Option<String>) -> Result<Option<(String, PathBuf)>, String> {
    let Some(id) = thread.as_deref().filter(|t| !t.is_empty()) else { return Ok(None) };
    let meta = state.db.get_session(id).map_err(|e| e.to_string())?.ok_or("unknown thread")?;
    let root = crate::checkpoints::repo_root(Path::new(&meta.cwd)).unwrap_or_else(|| PathBuf::from(&meta.cwd));
    Ok(Some((meta.title, root)))
}

const PROVIDERS: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex")];

#[tauri::command(async)]
pub fn pilot_skill_library(state: tauri::State<'_, crate::PilotState>, thread_id: Option<String>) -> Result<LibraryView, String> {
    let lib = library(&state);
    let homes = Homes::detect();
    let project = project_root(&state, &thread_id)?;
    let root = project.as_ref().map(|p| p.1.clone());
    let mut providers: Vec<ProviderView> = PROVIDERS
        .iter()
        .map(|(id, name)| ProviderView {
            id: id.to_string(),
            name: name.to_string(),
            installed: crate::providers::which(id).is_some() || (*id == "claude" && homes.claude.is_dir()),
            supported: true,
        })
        .collect();
    // The other chat providers have no skill folders QuantPilot can manage.
    let settings = crate::lock(&state.settings).clone();
    for a in crate::adapters::all(&settings) {
        if !PROVIDERS.iter().any(|(id, _)| *id == a.id) && settings.is_chat_provider(&a.id) {
            providers.push(ProviderView { id: a.id.clone(), name: a.label.clone(), installed: true, supported: false });
        }
    }
    let items: Vec<Item> = lib.items();
    let mut managed: Vec<String> = Vec::new();
    let views = items
        .into_iter()
        .map(|item| {
            let mut cells = Vec::new();
            for (pid, _) in PROVIDERS {
                for scope in ["user", "project"] {
                    let Some(base) = surface(pid, scope, root.as_deref(), &homes) else { continue };
                    let dir = base.join(&item.id);
                    let dir_s = dir.to_string_lossy().into_owned();
                    match item.activations.iter().find(|a| a.dir.eq_ignore_ascii_case(&dir_s)) {
                        Some(a) => {
                            managed.push(a.dir.clone());
                            let (state, drifted) = state_of(&item, a);
                            cells.push(Cell { provider: pid.into(), scope: scope.into(), state, dir: Some(dir_s), drifted });
                        }
                        None => cells.push(Cell { provider: pid.into(), scope: scope.into(), state: "off".into(), dir: Some(dir_s), drifted: Vec::new() }),
                    }
                }
            }
            ItemView { item, cells }
        })
        .collect();
    let mut unmanaged_list = Vec::new();
    for (pid, _) in PROVIDERS {
        for scope in ["user", "project"] {
            if let Some(base) = surface(pid, scope, root.as_deref(), &homes) {
                for (name, path) in unmanaged(&base, &managed) {
                    unmanaged_list.push((pid.to_string(), scope.to_string(), name, path));
                }
            }
        }
    }
    Ok(LibraryView { providers, items: views, unmanaged: unmanaged_list, project: project.map(|(t, r)| (t, r.to_string_lossy().into_owned())) })
}

#[tauri::command(async)]
pub fn pilot_skill_import(state: tauri::State<'_, crate::PilotState>, path: String, replace: Option<bool>) -> Result<Item, String> {
    let dir = PathBuf::from(path.trim());
    if !dir.is_dir() {
        return Err(format!("{} is not a folder.", dir.display()));
    }
    library(&state).import(&dir, replace.unwrap_or(false))
}

#[tauri::command(async)]
pub fn pilot_skill_activate(
    state: tauri::State<'_, crate::PilotState>,
    id: String,
    provider: String,
    scope: String,
    thread_id: Option<String>,
    keep_edited: Option<bool>,
) -> Result<Change, String> {
    let root = project_root(&state, &thread_id)?.map(|p| p.1);
    if scope == "project" && root.is_none() {
        return Err("Pick a thread: project scope means its checkout.".into());
    }
    let change = activate(&library(&state), &id, &provider, &scope, root.as_deref(), &Homes::detect(), keep_edited.unwrap_or(false))?;
    state.runtime.forget_catalog(&provider);
    Ok(change)
}

#[tauri::command(async)]
pub fn pilot_skill_deactivate(state: tauri::State<'_, crate::PilotState>, id: String, provider: String, scope: String, thread_id: Option<String>) -> Result<Change, String> {
    let root = project_root(&state, &thread_id)?.map(|p| p.1);
    let change = deactivate(&library(&state), &id, &provider, &scope, root.as_deref(), &Homes::detect())?;
    state.runtime.forget_catalog(&provider);
    Ok(change)
}

#[tauri::command(async)]
pub fn pilot_skill_remove(state: tauri::State<'_, crate::PilotState>, id: String) -> Result<(), String> {
    library(&state).remove(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qp-skills-{name}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn skill(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\ndescription: >\n  Does a\n  thing\n---\n{body}\n")).unwrap();
        std::fs::write(dir.join("scripts/run.ps1"), "echo hi").unwrap();
    }

    #[test]
    fn frontmatter_reads_plain_quoted_and_folded_values() {
        assert_eq!(frontmatter("---\nname: \"x y\"\ndescription: plain\n---\n"), (Some("x y".into()), Some("plain".into())));
        assert_eq!(frontmatter("---\nname: z\ndescription: |\n  a\n  b\n---\n").1.as_deref(), Some("a\nb"));
        assert_eq!(frontmatter("no frontmatter"), (None, None));
    }

    #[test]
    fn import_activate_drift_update_and_deactivate_keep_edits() {
        let base = tmp("flow");
        let src = base.join("source with space");
        skill(&src, "QA Probe", "v1");
        let lib = Library::new(base.join("lib store"));
        let homes = Homes { claude: base.join("claude-home"), agents: base.join("agents-home") };
        let item = lib.import(&src, false).unwrap();
        assert_eq!((item.id.as_str(), item.files.len()), ("qa-probe", 2));
        assert_eq!(item.description, "Does a thing");

        // Activate for Claude (user) and Codex (project).
        let project = base.join("checkout");
        std::fs::create_dir_all(&project).unwrap();
        activate(&lib, "qa-probe", "claude", "user", None, &homes, false).unwrap();
        activate(&lib, "qa-probe", "codex", "project", Some(&project), &homes, false).unwrap();
        assert!(homes.claude.join("skills/qa-probe/SKILL.md").is_file());
        assert!(project.join(".agents/skills/qa-probe/scripts/run.ps1").is_file());
        let item = lib.get("qa-probe").unwrap();
        assert!(item.activations.iter().all(|a| state_of(&item, a).0 == "active"));
        assert!(activate(&lib, "qa-probe", "gemini", "user", None, &homes, false).is_err(), "no integration, no claim");

        // A user edit is drift; an update makes the other copy outdated.
        let edited = homes.claude.join("skills/qa-probe/SKILL.md");
        std::fs::write(&edited, "my own edit").unwrap();
        skill(&src, "QA Probe", "v2");
        lib.import(&src, false).unwrap();
        let item = lib.get("qa-probe").unwrap();
        let states: Vec<String> = item.activations.iter().map(|a| state_of(&item, a).0).collect();
        assert_eq!(states, ["drifted", "outdated"]);

        // Re-activating refuses to overwrite the edit unless told to keep it.
        assert!(activate(&lib, "qa-probe", "claude", "user", None, &homes, false).is_err());
        let c = activate(&lib, "qa-probe", "claude", "user", None, &homes, true).unwrap();
        assert_eq!(c.kept, ["SKILL.md"]);
        assert_eq!(std::fs::read_to_string(&edited).unwrap(), "my own edit");

        // Deactivate: our files go, the edit stays.
        let d = deactivate(&lib, "qa-probe", "claude", "user", None, &homes).unwrap();
        assert_eq!(d.kept, ["SKILL.md"]);
        assert!(edited.is_file() && !homes.claude.join("skills/qa-probe/scripts/run.ps1").exists());
        deactivate(&lib, "qa-probe", "codex", "project", Some(&project), &homes).unwrap();
        assert!(!project.join(".agents/skills/qa-probe").exists(), "an untouched copy goes completely");
        lib.remove("qa-probe").unwrap();
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn collisions_unmanaged_folders_and_limits() {
        let base = tmp("edge");
        let a = base.join("a");
        let b = base.join("b");
        skill(&a, "same", "a");
        skill(&b, "same", "b");
        let lib = Library::new(base.join("lib"));
        let homes = Homes { claude: base.join("ch"), agents: base.join("ah") };
        lib.import(&a, false).unwrap();
        assert!(lib.import(&b, false).unwrap_err().contains("already has"));
        assert_eq!(lib.import(&b, true).unwrap().source, b.to_string_lossy());

        // A folder of the same name the user installed by hand stays theirs.
        skill(&homes.claude.join("skills/same"), "same", "hand-made");
        assert!(activate(&lib, "same", "claude", "user", None, &homes, true).unwrap_err().contains("not written by QuantPilot"));
        assert_eq!(unmanaged(&homes.claude.join("skills"), &[]).len(), 1);

        let big = base.join("big");
        std::fs::create_dir_all(&big).unwrap();
        std::fs::write(big.join("SKILL.md"), "---\nname: big\ndescription: d\n---\n").unwrap();
        std::fs::write(big.join("blob.bin"), vec![0u8; (MAX_BYTES + 1) as usize]).unwrap();
        assert!(scan_folder(&big).unwrap_err().contains("larger"));
        let _ = std::fs::remove_dir_all(base);
    }
}
