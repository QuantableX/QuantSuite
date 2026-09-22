//! `base/` — the part of a workspace vault that describes the codebase itself.
//!
//! Where QuantMCP's code index answers "where is this symbol", the base
//! answers "what is this directory for, how is it wired, what should I know
//! before touching it". Every base document is an ordinary memory (`type:
//! base`, frontmatter `base_path`) under `<vault>/base/`, so search, links,
//! graph, watcher and Obsidian all work on it unchanged.
//!
//! One document per **unit** — a directory: the workspace root plus every
//! directory discovery picks (shallow directories with enough source files,
//! any directory carrying a package manifest, anything an agent described).
//! What a sync generates lives between the `base:auto` markers and is
//! replaced every time; what an agent or the operator wrote — the
//! description under the H1, extra sections, the text after ` — ` on a file
//! line — is preserved.
//!
//! Tauri-free and SQLite-free: discovery walks the workspace with std::fs,
//! everything else is string work, all unit-tested. lib.rs does the vault
//! I/O and the index updates.

use crate::vault;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

pub const KIND: &str = "base";
pub const DIR: &str = "base";
pub const ROOT: &str = ".";
pub const AUTO_OPEN: &str = "<!-- base:auto -->";
pub const AUTO_CLOSE: &str = "<!-- /base:auto -->";
/// Placeholder in a fresh document — invisible in a preview, ignored by
/// `summary_of`, a hint to whoever opens the raw file.
pub const DESCRIBE_HINT: &str =
    "<!-- describe: what this directory is for, how it is wired, gotchas -->";

const FILES_HEADING: &str = "## Files";
const STRUCTURE_HEADING: &str = "## Structure";
const MAX_LISTED_FILES: usize = 150;
const MAX_LANGS: usize = 8;
const MAX_LINE_COUNT_BYTES: u64 = 1024 * 1024;
const MAX_WALK_DEPTH: usize = 24;

/// Same skip list as the code indexer (sidecars/python/codebase_index/utils.py)
/// plus the suite's own worktree folder. Hidden entries are skipped by rule.
const SKIP_DIRS: &[&str] = &[
    "node_modules", "__pycache__", "venv", "env", "dist", "build", "target", "bin", "obj",
    "coverage", "vendor", "Pods", ".qs-worktrees",
];
const SKIP_EXTENSIONS: &[&str] = &[
    "pyc", "pyo", "so", "dll", "dylib", "exe", "o", "obj", "a", "lib", "class", "jar", "war",
    "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "png", "jpg", "jpeg", "gif", "bmp", "ico",
    "svg", "webp", "mp3", "mp4", "avi", "mov", "wav", "flac", "ogg", "webm", "pdf", "doc",
    "docx", "xls", "xlsx", "ppt", "pptx", "lock", "map", "woff", "woff2", "ttf", "eot", "otf",
    "sqlite", "db", "db-shm", "db-wal", "suo", "user",
];
const SKIP_FILENAMES: &[&str] = &[
    "package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb",
    "cargo.lock", "poetry.lock", "pipfile.lock", "composer.lock", "deno.lock", "uv.lock",
    "thumbs.db", "desktop.ini",
];
/// A directory carrying one of these is a unit on its own, however deep.
const MANIFESTS: &[&str] = &[
    "Cargo.toml", "package.json", "module.json", "pyproject.toml", "setup.py", "go.mod",
    "pom.xml", "build.gradle", "build.gradle.kts", "CMakeLists.txt", "composer.json",
    "Gemfile", "mix.exs",
];

// ─── Discovery ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub files: usize,
    pub lines: usize,
    /// language → (files, lines)
    pub by_lang: BTreeMap<String, (usize, usize)>,
}

impl Totals {
    fn add_file(&mut self, lang: &str, lines: usize) {
        self.files += 1;
        self.lines += lines;
        let entry = self.by_lang.entry(lang.to_string()).or_default();
        entry.0 += 1;
        entry.1 += lines;
    }

    fn merge(&mut self, other: &Totals) {
        self.files += other.files;
        self.lines += other.lines;
        for (lang, (files, lines)) in &other.by_lang {
            let entry = self.by_lang.entry(lang.clone()).or_default();
            entry.0 += files;
            entry.1 += lines;
        }
    }
}

/// A file a unit owns: inside the unit's directory, not inside a nested unit.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnedFile {
    /// Relative to the unit's directory, `/`-separated.
    pub rel: String,
    pub lang: String,
    /// `None` for binary or oversized files.
    pub lines: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SubDir {
    pub name: String,
    /// Workspace-relative.
    pub rel_path: String,
    pub totals: Totals,
    pub is_unit: bool,
}

#[derive(Debug, Clone)]
pub struct Unit {
    /// Workspace-relative, `/`-separated; `"."` for the root.
    pub rel_path: String,
    pub manifests: Vec<String>,
    pub subdirs: Vec<SubDir>,
    pub files: Vec<OwnedFile>,
    /// The whole subtree, nested units included.
    pub totals: Totals,
}

#[derive(Debug, Clone)]
pub struct Discovery {
    /// Directories up to this depth are units when they hold `min_files`.
    pub depth: usize,
    pub min_files: usize,
    /// Directories with a manifest are units up to this depth.
    pub manifest_depth: usize,
    /// Directories that are units no matter what (existing docs, requests).
    pub forced: HashSet<String>,
}

impl Default for Discovery {
    fn default() -> Self {
        Self { depth: 2, min_files: 3, manifest_depth: 4, forced: HashSet::new() }
    }
}

struct Walked {
    totals: Totals,
    is_unit: bool,
    /// Files the nearest unit ancestor owns (empty when this dir is a unit).
    owned: Vec<OwnedFile>,
}

/// Walk the workspace and return its units, root first, then by path.
pub fn discover(root: &Path, cfg: &Discovery) -> Vec<Unit> {
    let mut units = Vec::new();
    walk(root, "", 0, cfg, &mut units);
    units.sort_by(|a, b| {
        (a.rel_path != ROOT).cmp(&(b.rel_path != ROOT)).then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    units
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn skip_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    if SKIP_FILENAMES.contains(&lower.as_str()) {
        return true;
    }
    if lower.ends_with(".min.js") || lower.ends_with(".min.css") {
        return true;
    }
    match lower.rsplit_once('.') {
        Some((_, ext)) => SKIP_EXTENSIONS.contains(&ext),
        None => false,
    }
}

fn walk(dir: &Path, rel: &str, depth: usize, cfg: &Discovery, units: &mut Vec<Unit>) -> Walked {
    let mut totals = Totals::default();
    let mut direct: Vec<OwnedFile> = Vec::new();
    let mut from_children: Vec<OwnedFile> = Vec::new();
    let mut subdirs: Vec<SubDir> = Vec::new();
    let mut manifests: Vec<String> = Vec::new();

    let mut entries: Vec<(String, std::path::PathBuf, bool)> = Vec::new();
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Junctions and symlinks (a worktree's node_modules, a linked
            // .venv) are skipped like the indexer skips them: they are not
            // this codebase.
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            entries.push((name, entry.path(), ft.is_dir()));
        }
    }
    entries.sort_by_key(|entry| entry.0.to_lowercase());

    for (name, path, is_dir) in entries {
        if is_hidden(&name) {
            continue;
        }
        if is_dir {
            if SKIP_DIRS.contains(&name.as_str()) || depth >= MAX_WALK_DEPTH {
                continue;
            }
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let walked = walk(&path, &child_rel, depth + 1, cfg, units);
            totals.merge(&walked.totals);
            if !walked.is_unit {
                from_children.extend(walked.owned.into_iter().map(|f| OwnedFile {
                    rel: format!("{name}/{}", f.rel),
                    ..f
                }));
            }
            subdirs.push(SubDir {
                name,
                rel_path: child_rel,
                totals: walked.totals,
                is_unit: walked.is_unit,
            });
        } else {
            if skip_file(&name) {
                continue;
            }
            let lang = lang_of(&name).to_string();
            let lines = count_lines(&path);
            totals.add_file(&lang, lines.unwrap_or(0));
            if MANIFESTS.contains(&name.as_str()) {
                manifests.push(name.clone());
            }
            direct.push(OwnedFile { rel: name, lang, lines });
        }
    }

    let is_unit = depth == 0
        || cfg.forced.contains(rel)
        || (depth <= cfg.depth && totals.files >= cfg.min_files)
        || (depth <= cfg.manifest_depth && !manifests.is_empty());

    let mut owned = direct;
    owned.extend(from_children);
    owned.sort_by(|a, b| a.rel.cmp(&b.rel));

    if is_unit {
        units.push(Unit {
            rel_path: if rel.is_empty() { ROOT.to_string() } else { rel.to_string() },
            manifests,
            subdirs,
            files: owned,
            totals: totals.clone(),
        });
        Walked { totals, is_unit: true, owned: Vec::new() }
    } else {
        Walked { totals, is_unit: false, owned }
    }
}

/// Display language by extension — the set the suite's own repos use, with
/// the extension itself as a fallback so nothing reads as "unknown".
pub fn lang_of(name: &str) -> &str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "rs" => "Rust",
        "ts" | "mts" | "cts" | "tsx" => "TypeScript",
        "js" | "mjs" | "cjs" | "jsx" => "JavaScript",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "py" | "pyi" => "Python",
        "md" | "markdown" | "mdx" => "Markdown",
        "json" | "jsonc" | "json5" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "html" | "htm" => "HTML",
        "sh" | "bash" | "zsh" => "Shell",
        "ps1" | "psm1" => "PowerShell",
        "bat" | "cmd" => "Batch",
        "sql" => "SQL",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "txt" => "Text",
        "xml" | "plist" => "XML",
        _ => "other",
    }
}

/// Newline count of a text file; `None` when it is oversized or binary
/// (a NUL byte in the first KiB).
pub fn count_lines(path: &Path) -> Option<usize> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_LINE_COUNT_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.iter().take(1024).any(|b| *b == 0) {
        return None;
    }
    if bytes.is_empty() {
        return Some(0);
    }
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    Some(if bytes.ends_with(b"\n") { newlines } else { newlines + 1 })
}

// ─── Naming ───────────────────────────────────────────────────────────────

pub fn root_title(workspace_name: &str) -> String {
    format!("{} codebase", workspace_name.trim())
}

/// A unit's memory title: the root carries the workspace name, every other
/// unit is its own path — the name an agent already thinks in.
pub fn unit_title(rel_path: &str, root_title: &str) -> String {
    if rel_path == ROOT {
        root_title.to_string()
    } else {
        rel_path.to_string()
    }
}

/// The file stem under `base/`. Links between base documents use it
/// (`[[slug|label]]`): QuantMemory resolves a slug, Obsidian a filename.
pub fn unit_slug(rel_path: &str) -> String {
    if rel_path == ROOT {
        "codebase".into()
    } else {
        vault::slugify(rel_path)
    }
}

/// Turn whatever a caller typed into a workspace-relative unit/file path:
/// backslashes, a leading `./`, a trailing slash, or the absolute workspace
/// path itself all collapse; empty means the root.
pub fn normalize_rel(input: &str, workspace_root: &Path) -> String {
    let mut s = input.trim().replace('\\', "/");
    let root = workspace_root.to_string_lossy().replace('\\', "/");
    let root_trimmed = root.trim_end_matches('/');
    if !root_trimmed.is_empty() && s.to_lowercase().starts_with(&root_trimmed.to_lowercase()) {
        let rest = &s[root_trimmed.len()..];
        if rest.is_empty() || rest.starts_with('/') {
            s = rest.to_string();
        }
    }
    let s = s.trim_start_matches("./").trim_matches('/');
    if s.is_empty() || s == "." {
        ROOT.to_string()
    } else {
        s.to_string()
    }
}

/// The unit responsible for a path: an exact unit, else the deepest unit
/// whose directory contains it. Returns `(unit, remainder)` — the remainder
/// is the path inside the unit (`None` for the unit itself).
pub fn owner_of<'a>(path: &str, unit_paths: &[&'a str]) -> Option<(&'a str, Option<String>)> {
    if let Some(exact) = unit_paths.iter().find(|u| **u == path) {
        return Some((*exact, None));
    }
    let mut best: Option<&'a str> = None;
    for unit in unit_paths {
        let contains = if *unit == ROOT {
            true
        } else {
            path.starts_with(&format!("{unit}/"))
        };
        if contains && best.map_or(true, |b| unit.len() > b.len()) {
            best = Some(*unit);
        }
    }
    let unit = best?;
    let rest = if unit == ROOT { path.to_string() } else { path[unit.len() + 1..].to_string() };
    Some((unit, Some(rest)))
}

// ─── Reading a document ───────────────────────────────────────────────────

/// A document split around its generated block.
#[derive(Debug, Clone, PartialEq)]
pub struct Parts {
    pub head: String,
    pub auto: Option<String>,
    pub tail: String,
}

pub fn split_auto(body: &str) -> Parts {
    let Some(open) = body.find(AUTO_OPEN) else {
        return Parts { head: body.to_string(), auto: None, tail: String::new() };
    };
    let after_open = open + AUTO_OPEN.len();
    match body[after_open..].find(AUTO_CLOSE) {
        Some(close_rel) => {
            let close = after_open + close_rel;
            Parts {
                head: body[..open].to_string(),
                auto: Some(body[after_open..close].to_string()),
                tail: body[close + AUTO_CLOSE.len()..].to_string(),
            }
        }
        // An opening marker with no close: everything after it is generated
        // and gets replaced — nothing hand-written can sit there by design.
        None => Parts { head: body[..open].to_string(), auto: Some(body[after_open..].to_string()), tail: String::new() },
    }
}

/// Byte range of the description: after the H1 line, up to the first `## `
/// heading (or the end). `None` when the head has no H1.
fn description_span(head: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut start: Option<usize> = None;
    for line in head.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        match start {
            None => {
                if content.starts_with("# ") {
                    start = Some(offset + line.len());
                }
            }
            Some(s) => {
                if content.starts_with("## ") {
                    return Some((s, offset));
                }
            }
        }
        offset += line.len();
    }
    start.map(|s| (s, head.len()))
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("<!--") {
        out.push_str(&rest[..open]);
        match rest[open..].find("-->") {
            Some(close) => rest = &rest[open + close + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The hand-written description, comments removed, trimmed.
pub fn description_of(body: &str) -> String {
    let parts = split_auto(body);
    match description_span(&parts.head) {
        Some((s, e)) => strip_comments(&parts.head[s..e]).trim().to_string(),
        None => String::new(),
    }
}

/// One line that stands for the document in a listing: the first line of
/// the description, list markers and emphasis stripped, cut at 140 chars.
pub fn summary_of(body: &str) -> Option<String> {
    let description = description_of(body);
    let line = description.lines().map(str::trim).find(|l| !l.is_empty())?;
    let cleaned = line
        .trim_start_matches(['-', '*', '>'])
        .trim()
        .replace("**", "")
        // Only *emphasis* underscores at the edges go; `qs_mod_memory` stays.
        .trim_matches('_')
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.chars().count() <= 140 {
        return Some(cleaned);
    }
    let cut: String = cleaned.chars().take(139).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// Replace the description (keeping H1, extra sections and the generated
/// block); a body without an H1 gets `# title` first.
pub fn set_description(body: &str, title: &str, text: &str) -> String {
    let parts = split_auto(body);
    let head = if description_span(&parts.head).is_some() {
        parts.head.clone()
    } else {
        format!("# {title}\n\n{}", parts.head.trim_start())
    };
    let (s, e) = description_span(&head).expect("H1 ensured above");
    let mut out = String::with_capacity(head.len() + text.len());
    out.push_str(head[..s].trim_end_matches(['\r', '\n']));
    out.push_str("\n\n");
    let text = text.trim();
    if !text.is_empty() {
        out.push_str(text);
        out.push_str("\n\n");
    }
    out.push_str(head[e..].trim_start_matches(['\r', '\n']));
    reassemble(&out, parts.auto.as_deref(), &parts.tail)
}

// ─── File lines ───────────────────────────────────────────────────────────

/// `- \`path\` (meta) — description` — the one grammar for a file entry,
/// written by the sync and edited by agents (or by hand in the vault).
#[derive(Debug, Clone, PartialEq)]
pub struct FileLine {
    pub path: String,
    pub meta: Option<String>,
    pub desc: Option<String>,
}

pub fn parse_file_line(line: &str) -> Option<FileLine> {
    let rest = line.trim_start().strip_prefix("- `")?;
    let end = rest.find('`')?;
    let path = rest[..end].to_string();
    if path.is_empty() {
        return None;
    }
    let mut rest = rest[end + 1..].trim_start();
    let meta = if let Some(inner) = rest.strip_prefix('(') {
        let close = inner.find(')')?;
        let m = inner[..close].trim().to_string();
        rest = inner[close + 1..].trim_start();
        Some(m).filter(|m| !m.is_empty())
    } else {
        None
    };
    let desc = rest
        .strip_prefix('—')
        .or_else(|| rest.strip_prefix("--"))
        .or_else(|| rest.strip_prefix('-'))
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    Some(FileLine { path, meta, desc })
}

pub fn render_file_line(path: &str, meta: Option<&str>, desc: Option<&str>) -> String {
    let mut out = format!("- `{path}`");
    if let Some(meta) = meta.filter(|m| !m.trim().is_empty()) {
        out.push_str(&format!(" ({meta})"));
    }
    if let Some(desc) = desc.map(str::trim).filter(|d| !d.is_empty()) {
        out.push_str(" — ");
        out.push_str(&desc.replace('\n', " "));
    }
    out
}

/// Whether a line sits inside a `## Files` section — the only place a file
/// line lives; a directory line in `## Structure` uses the same grammar and
/// must not be mistaken for one.
fn in_files_section(line: &str, in_files: &mut bool) -> bool {
    let trimmed = line.trim();
    if trimmed == FILES_HEADING {
        *in_files = true;
        return false;
    }
    if trimmed.starts_with("## ") || trimmed.starts_with("# ") {
        *in_files = false;
        return false;
    }
    *in_files
}

/// Every file line of the document, from its `## Files` sections.
pub fn file_lines(body: &str) -> Vec<FileLine> {
    let mut in_files = false;
    body.lines()
        .filter(|line| in_files_section(line, &mut in_files))
        .filter_map(parse_file_line)
        .collect()
}

/// Every described file in the document.
pub fn file_descriptions(body: &str) -> BTreeMap<String, String> {
    file_lines(body)
        .into_iter()
        .filter_map(|l| l.desc.map(|d| (l.path, d)))
        .collect()
}

/// Set (or clear, with empty text) one file's description. A file without a
/// line yet gets one at the end of the `## Files` list — inside the generated
/// block when there is one, so the next sync keeps it in place.
pub fn set_file_description(body: &str, file_rel: &str, meta: Option<&str>, text: &str) -> String {
    let text = text.trim();
    let desc = if text.is_empty() { None } else { Some(text) };

    // Existing line anywhere → rewrite it in place.
    let mut out = String::with_capacity(body.len() + text.len());
    let mut done = false;
    let mut in_files = false;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let candidate = in_files_section(content, &mut in_files);
        if !done && candidate && parse_file_line(content).is_some_and(|l| l.path == file_rel) {
            let existing = parse_file_line(content).and_then(|l| l.meta);
            out.push_str(&render_file_line(file_rel, meta.or(existing.as_deref()), desc));
            out.push_str(&line[content.len()..]);
            done = true;
        } else {
            out.push_str(line);
        }
    }
    if done {
        return out;
    }
    let Some(desc) = desc else { return body.to_string() };
    let new_line = render_file_line(file_rel, meta, Some(desc));

    let parts = split_auto(body);
    match parts.auto {
        Some(auto) => {
            let auto = append_to_files_list(&auto, &new_line);
            reassemble(&parts.head, Some(&auto), &parts.tail)
        }
        None => {
            let head = parts.head.trim_end().to_string();
            if head.contains(&format!("\n{FILES_HEADING}")) || head.starts_with(FILES_HEADING) {
                format!("{}\n", append_to_files_list(&head, &new_line).trim_end())
            } else {
                format!("{head}\n\n{FILES_HEADING}\n{new_line}\n")
            }
        }
    }
}

/// Append a line after the last entry of the `## Files` list (creating the
/// section at the end when absent).
fn append_to_files_list(block: &str, new_line: &str) -> String {
    let mut lines: Vec<&str> = block.lines().collect();
    let heading_at = lines.iter().position(|l| l.trim() == FILES_HEADING);
    match heading_at {
        Some(h) => {
            let mut insert_at = h + 1;
            for (i, line) in lines.iter().enumerate().skip(h + 1) {
                if line.starts_with("## ") {
                    break;
                }
                if parse_file_line(line).is_some() {
                    insert_at = i + 1;
                }
            }
            lines.insert(insert_at, new_line);
            let mut out = lines.join("\n");
            if block.ends_with('\n') {
                out.push('\n');
            }
            out
        }
        None => format!("{}\n\n{FILES_HEADING}\n{new_line}\n", block.trim_end()),
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────

pub struct RenderCtx<'a> {
    pub root_title: &'a str,
    pub workspace_path: &'a str,
    /// unit path → one-line summary from its document.
    pub summaries: &'a BTreeMap<String, String>,
}

fn link_to(rel_path: &str, label: &str) -> String {
    format!("[[{}|{}]]", unit_slug(rel_path), label)
}

fn languages_line(totals: &Totals) -> String {
    let mut langs: Vec<(&String, &(usize, usize))> = totals.by_lang.iter().collect();
    langs.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then_with(|| b.1 .0.cmp(&a.1 .0)));
    let shown: Vec<String> = langs
        .iter()
        .take(MAX_LANGS)
        .map(|(lang, (files, lines))| format!("{lang} {files} files / {lines} lines"))
        .collect();
    let more = langs.len().saturating_sub(MAX_LANGS);
    let mut line = format!("Languages: {}", shown.join(" · "));
    if more > 0 {
        line.push_str(&format!(" · {more} more"));
    }
    line
}

fn dir_line(sub: &SubDir, summaries: &BTreeMap<String, String>) -> String {
    let label = format!("{}/", sub.name);
    let name = if sub.is_unit { link_to(&sub.rel_path, &label) } else { format!("`{label}`") };
    let mut line = format!("- {name} — {} files · {} lines", sub.totals.files, sub.totals.lines);
    if sub.is_unit {
        if let Some(summary) = summaries.get(&sub.rel_path) {
            line.push_str(" — ");
            line.push_str(summary);
        }
    }
    line
}

fn file_meta(file: &OwnedFile) -> String {
    match file.lines {
        Some(lines) => format!("{}, {lines} lines", file.lang),
        None => file.lang.clone(),
    }
}

/// The generated block for one unit: structure, then the file list with the
/// descriptions `previous` carried over (missing files keep theirs).
pub fn render_auto(unit: &Unit, all: &[Unit], ctx: &RenderCtx<'_>, previous: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    out.push_str(STRUCTURE_HEADING);
    out.push('\n');
    if unit.rel_path == ROOT {
        out.push_str(&format!(
            "Workspace root `{}` · {} files · {} lines\n",
            ctx.workspace_path, unit.totals.files, unit.totals.lines
        ));
    } else {
        out.push_str(&format!(
            "Part of {} · {} files · {} lines\n",
            link_to(ROOT, ctx.root_title),
            unit.totals.files,
            unit.totals.lines
        ));
    }
    if !unit.totals.by_lang.is_empty() {
        out.push_str(&languages_line(&unit.totals));
        out.push('\n');
    }
    if !unit.manifests.is_empty() {
        let list: Vec<String> = unit.manifests.iter().map(|m| format!("`{m}`")).collect();
        out.push_str(&format!("Manifests: {}\n", list.join(", ")));
    }

    if unit.rel_path == ROOT {
        let nested: Vec<&Unit> = all.iter().filter(|u| u.rel_path != ROOT).collect();
        if !nested.is_empty() {
            out.push_str("\nUnits:\n");
            for u in &nested {
                let ancestors = nested
                    .iter()
                    .filter(|a| u.rel_path.starts_with(&format!("{}/", a.rel_path)))
                    .count();
                let mut line = format!(
                    "{}- {} — {} files · {} lines",
                    "  ".repeat(ancestors),
                    link_to(&u.rel_path, &format!("{}/", u.rel_path)),
                    u.totals.files,
                    u.totals.lines
                );
                if let Some(summary) = ctx.summaries.get(&u.rel_path) {
                    line.push_str(" — ");
                    line.push_str(summary);
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
        let plain: Vec<&SubDir> = unit.subdirs.iter().filter(|s| !s.is_unit).collect();
        if !plain.is_empty() {
            out.push_str("\nOther directories:\n");
            for sub in plain {
                out.push_str(&dir_line(sub, ctx.summaries));
                out.push('\n');
            }
        }
    } else if !unit.subdirs.is_empty() {
        out.push_str("\nDirectories:\n");
        for sub in &unit.subdirs {
            out.push_str(&dir_line(sub, ctx.summaries));
            out.push('\n');
        }
    }

    out.push('\n');
    out.push_str(FILES_HEADING);
    out.push('\n');
    let mut listed: HashSet<&str> = HashSet::new();
    let mut skipped = 0usize;
    for file in &unit.files {
        let desc = previous.get(&file.rel);
        if listed.len() >= MAX_LISTED_FILES && desc.is_none() {
            skipped += 1;
            continue;
        }
        listed.insert(file.rel.as_str());
        out.push_str(&render_file_line(&file.rel, Some(&file_meta(file)), desc.map(String::as_str)));
        out.push('\n');
    }
    for (path, desc) in previous {
        if listed.contains(path.as_str()) {
            continue;
        }
        out.push_str(&render_file_line(path, Some("missing"), Some(desc)));
        out.push('\n');
    }
    if skipped > 0 {
        out.push_str(&format!("- … {skipped} more files not listed (describe one to list it)\n"));
    }
    out
}

/// The generated block for a unit whose directory is gone.
pub fn render_missing(when: &str) -> String {
    format!("{STRUCTURE_HEADING}\n_Directory not found in the workspace at the last sync ({when})._\n")
}

fn reassemble(head: &str, auto: Option<&str>, tail: &str) -> String {
    let mut out = head.trim_end().to_string();
    if let Some(auto) = auto {
        out.push_str("\n\n");
        out.push_str(AUTO_OPEN);
        out.push('\n');
        out.push_str(auto.trim());
        out.push('\n');
        out.push_str(AUTO_CLOSE);
        out.push('\n');
    } else {
        out.push('\n');
    }
    let tail = tail.trim_start_matches(['\r', '\n']).trim_end();
    if !tail.is_empty() {
        out.push('\n');
        out.push_str(tail);
        out.push('\n');
    }
    out
}

/// `old_body` with its generated block replaced (or appended).
pub fn compose_body(old_body: &str, generated: &str) -> String {
    let parts = split_auto(old_body);
    reassemble(&parts.head, Some(generated), &parts.tail)
}

/// A brand-new document: H1, the describe hint, the generated block.
pub fn fresh_body(title: &str, generated: &str) -> String {
    reassemble(&format!("# {title}\n\n{DESCRIBE_HINT}\n"), Some(generated), "")
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qm-base-{name}-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn discovery_finds_root_shallow_dirs_and_manifest_dirs() {
        let root = temp_root("discover");
        touch(&root, "package.json", "{}\n");
        touch(&root, "README.md", "# x\n");
        touch(&root, "modules/memory/module.json", "{}\n");
        touch(&root, "modules/memory/app/a.vue", "<template/>\n");
        touch(&root, "modules/memory/app/b.ts", "export {}\n");
        touch(&root, "modules/memory/crate/Cargo.toml", "[package]\n");
        touch(&root, "modules/memory/crate/src/lib.rs", "fn a() {}\nfn b() {}\n");
        touch(&root, "modules/memory/crate/src/deep/x/y/z.rs", "// deep\n");
        touch(&root, "node_modules/pkg/index.js", "junk\n");
        touch(&root, "small/one.txt", "1\n");
        touch(&root, ".hidden/secret.md", "no\n");

        let units = discover(&root, &Discovery::default());
        let paths: Vec<&str> = units.iter().map(|u| u.rel_path.as_str()).collect();
        assert_eq!(paths, vec![".", "modules", "modules/memory", "modules/memory/crate"]);

        let root_unit = &units[0];
        assert_eq!(root_unit.files.iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), vec!["README.md", "package.json", "small/one.txt"]);
        assert_eq!(root_unit.totals.files, 9);
        assert!(root_unit.subdirs.iter().any(|s| s.name == "small" && !s.is_unit));
        assert!(!root_unit.subdirs.iter().any(|s| s.name == "node_modules"));

        let memory = units.iter().find(|u| u.rel_path == "modules/memory").unwrap();
        // The crate is a nested unit, so its files are not owned here; app/ is not.
        assert_eq!(memory.files.iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), vec!["app/a.vue", "app/b.ts", "module.json"]);
        assert_eq!(memory.manifests, vec!["module.json"]);
        assert!(memory.subdirs.iter().any(|s| s.name == "crate" && s.is_unit));
        assert_eq!(memory.totals.files, 6);

        let krate = units.iter().find(|u| u.rel_path == "modules/memory/crate").unwrap();
        assert_eq!(krate.files.iter().find(|f| f.rel == "src/lib.rs").unwrap().lines, Some(2));
        assert_eq!(krate.totals.by_lang["Rust"], (2, 3));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn forced_paths_become_units() {
        let root = temp_root("forced");
        touch(&root, "a/b/c/one.rs", "1\n");
        let mut cfg = Discovery::default();
        cfg.forced.insert("a/b/c".into());
        let units = discover(&root, &cfg);
        let paths: Vec<&str> = units.iter().map(|u| u.rel_path.as_str()).collect();
        assert_eq!(paths, vec![".", "a/b/c"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn line_counting_handles_binaries_and_missing_newline() {
        let root = temp_root("lines");
        touch(&root, "a.txt", "one\ntwo");
        touch(&root, "b.txt", "one\ntwo\n");
        fs::write(root.join("c.bin"), [1u8, 0, 2]).unwrap();
        assert_eq!(count_lines(&root.join("a.txt")), Some(2));
        assert_eq!(count_lines(&root.join("b.txt")), Some(2));
        assert_eq!(count_lines(&root.join("c.bin")), None);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn names_and_paths() {
        assert_eq!(root_title("QuantSuite"), "QuantSuite codebase");
        assert_eq!(unit_title(".", "QuantSuite codebase"), "QuantSuite codebase");
        assert_eq!(unit_title("modules/memory", "x"), "modules/memory");
        assert_eq!(unit_slug("."), "codebase");
        assert_eq!(unit_slug("modules/memory"), "modules-memory");

        let root = Path::new("C:\\Projects\\QuantSuite");
        assert_eq!(normalize_rel("", root), ".");
        assert_eq!(normalize_rel("./", root), ".");
        assert_eq!(normalize_rel("modules\\memory\\", root), "modules/memory");
        assert_eq!(normalize_rel("C:/projects/quantsuite/modules/memory", root), "modules/memory");
        assert_eq!(normalize_rel("C:\\Projects\\QuantSuite", root), ".");
        assert_eq!(normalize_rel("C:\\Projects\\QuantSuiteX\\a", root), "C:/Projects/QuantSuiteX/a");
    }

    #[test]
    fn owner_is_exact_or_deepest_prefix() {
        let units = [".", "modules", "modules/memory", "modules/memory/crate"];
        assert_eq!(owner_of("modules/memory", &units), Some(("modules/memory", None)));
        assert_eq!(
            owner_of("modules/memory/crate/src/lib.rs", &units),
            Some(("modules/memory/crate", Some("src/lib.rs".into())))
        );
        assert_eq!(owner_of("modules/memoryx/a.rs", &units), Some(("modules", Some("memoryx/a.rs".into()))));
        assert_eq!(owner_of("README.md", &units), Some((".", Some("README.md".into()))));
        assert_eq!(owner_of("x", &[]), None);
    }

    #[test]
    fn split_and_reassemble_round_trip() {
        let body = "# T\n\ndesc\n\n<!-- base:auto -->\n## Structure\nx\n<!-- /base:auto -->\n\n## Notes\nkept\n";
        let parts = split_auto(body);
        assert_eq!(parts.head, "# T\n\ndesc\n\n");
        assert_eq!(parts.auto.as_deref(), Some("\n## Structure\nx\n"));
        assert_eq!(parts.tail, "\n\n## Notes\nkept\n");
        assert_eq!(compose_body(body, "## Structure\nx"), body);
        let replaced = compose_body(body, "## Structure\ny");
        assert!(replaced.contains("\ny\n<!-- /base:auto -->"));
        assert!(replaced.ends_with("## Notes\nkept\n"));
    }

    #[test]
    fn a_body_without_markers_gets_the_block_appended() {
        let out = compose_body("# T\n\nhand written\n", "## Structure\nz");
        assert_eq!(out, "# T\n\nhand written\n\n<!-- base:auto -->\n## Structure\nz\n<!-- /base:auto -->\n");
    }

    #[test]
    fn description_is_between_h1_and_first_section_without_comments() {
        let body = fresh_body("modules/memory", "## Structure\n");
        assert_eq!(description_of(&body), "");
        assert_eq!(summary_of(&body), None);

        let described = set_description(&body, "modules/memory", "**The memory module.**\n\nSecond paragraph.");
        assert_eq!(description_of(&described), "**The memory module.**\n\nSecond paragraph.");
        assert_eq!(summary_of(&described).as_deref(), Some("The memory module."));
        assert!(described.contains(AUTO_OPEN));
        assert!(!described.contains(DESCRIBE_HINT));

        let with_section = "# T\n\nfirst\n\n## Gotchas\n- one\n";
        assert_eq!(description_of(with_section), "first");
        let replaced = set_description(with_section, "T", "new");
        assert_eq!(replaced, "# T\n\nnew\n\n## Gotchas\n- one\n");
        let cleared = set_description(with_section, "T", "");
        assert_eq!(cleared, "# T\n\n## Gotchas\n- one\n");

        // Without an H1 the whole text was the description — it is replaced.
        let no_h1 = set_description("just text\n", "Title", "d");
        assert_eq!(no_h1, "# Title\n\nd\n");
    }

    #[test]
    fn summary_is_cut_at_140_chars() {
        let long = "x".repeat(200);
        let body = format!("# T\n\n{long}\n");
        let summary = summary_of(&body).unwrap();
        assert_eq!(summary.chars().count(), 140);
        assert!(summary.ends_with('…'));

        // Emphasis is stripped, identifiers are not.
        let body = "# T\n\n_The `qs_mod_memory` crate._\n";
        assert_eq!(summary_of(body).as_deref(), Some("The `qs_mod_memory` crate."));
    }

    #[test]
    fn file_lines_parse_and_render() {
        let line = parse_file_line("- `src/lib.rs` (Rust, 12 lines) — the plugin").unwrap();
        assert_eq!(line, FileLine { path: "src/lib.rs".into(), meta: Some("Rust, 12 lines".into()), desc: Some("the plugin".into()) });
        let bare = parse_file_line("- `a.md`").unwrap();
        assert_eq!(bare, FileLine { path: "a.md".into(), meta: None, desc: None });
        let dash = parse_file_line("  - `a.md` - hand written").unwrap();
        assert_eq!(dash.desc.as_deref(), Some("hand written"));
        assert!(parse_file_line("- not a file line").is_none());
        assert_eq!(render_file_line("a.md", Some("Markdown, 3 lines"), Some("x\ny")), "- `a.md` (Markdown, 3 lines) — x y");
        assert_eq!(render_file_line("a.md", None, None), "- `a.md`");
    }

    #[test]
    fn file_description_is_rewritten_in_place_or_appended_into_the_block() {
        let body = "# T\n\n<!-- base:auto -->\n## Structure\nx\n\n## Files\n- `a.rs` (Rust, 1 lines)\n- `b.rs` (Rust, 2 lines) — old\n<!-- /base:auto -->\n";
        let set = set_file_description(body, "b.rs", None, "new");
        assert!(set.contains("- `b.rs` (Rust, 2 lines) — new\n"));
        assert!(!set.contains("old"));

        let added = set_file_description(body, "c.rs", Some("Rust, 3 lines"), "third");
        let idx_b = added.find("- `b.rs`").unwrap();
        let idx_c = added.find("- `c.rs` (Rust, 3 lines) — third").unwrap();
        let idx_close = added.find(AUTO_CLOSE).unwrap();
        assert!(idx_b < idx_c && idx_c < idx_close);

        let cleared = set_file_description(body, "b.rs", None, "");
        assert!(cleared.contains("- `b.rs` (Rust, 2 lines)\n"));

        let descs = file_descriptions(&added);
        assert_eq!(descs.get("b.rs").map(String::as_str), Some("old"));
        assert_eq!(descs.get("c.rs").map(String::as_str), Some("third"));

        // A directory line in Structure shares the grammar but is no file.
        let with_dir = "# T\n\n## Structure\n- `app/` — 3 files\n\n## Files\n- `a.rs` — real\n";
        assert_eq!(file_lines(with_dir).len(), 1);
        assert!(file_descriptions(with_dir).contains_key("a.rs"));
        let untouched = set_file_description(with_dir, "app/", None, "x");
        assert!(untouched.contains("- `app/` — 3 files\n"));
        assert!(untouched.contains("- `app/` — x\n"), "appended as a file line, the dir line kept");

        let no_block = set_file_description("# T\n\ndesc\n", "z.rs", None, "zed");
        assert_eq!(no_block, "# T\n\ndesc\n\n## Files\n- `z.rs` — zed\n");
    }

    #[test]
    fn sync_keeps_descriptions_and_lists_missing_files() {
        let unit = Unit {
            rel_path: "modules/memory".into(),
            manifests: vec!["module.json".into()],
            subdirs: vec![
                SubDir { name: "crate".into(), rel_path: "modules/memory/crate".into(), totals: Totals { files: 2, lines: 30, by_lang: BTreeMap::new() }, is_unit: true },
                SubDir { name: "app".into(), rel_path: "modules/memory/app".into(), totals: Totals { files: 1, lines: 5, by_lang: BTreeMap::new() }, is_unit: false },
            ],
            files: vec![
                OwnedFile { rel: "app/a.vue".into(), lang: "Vue".into(), lines: Some(5) },
                OwnedFile { rel: "module.json".into(), lang: "JSON".into(), lines: Some(10) },
            ],
            totals: Totals { files: 4, lines: 45, by_lang: BTreeMap::from([("Vue".to_string(), (1, 5)), ("JSON".to_string(), (1, 10)), ("Rust".to_string(), (2, 30))]) },
        };
        let mut summaries = BTreeMap::new();
        summaries.insert("modules/memory/crate".to_string(), "The Rust side.".to_string());
        let ctx = RenderCtx { root_title: "QS codebase", workspace_path: "C:/x", summaries: &summaries };
        let previous = BTreeMap::from([
            ("module.json".to_string(), "manifest".to_string()),
            ("gone.rs".to_string(), "was here".to_string()),
        ]);
        let auto = render_auto(&unit, std::slice::from_ref(&unit), &ctx, &previous);
        assert!(auto.starts_with("## Structure\nPart of [[codebase|QS codebase]] · 4 files · 45 lines\n"));
        assert!(auto.contains("Languages: Rust 2 files / 30 lines · JSON 1 files / 10 lines · Vue 1 files / 5 lines\n"));
        assert!(auto.contains("Manifests: `module.json`\n"));
        assert!(auto.contains("- [[modules-memory-crate|crate/]] — 2 files · 30 lines — The Rust side.\n"));
        assert!(auto.contains("- `app/` — 1 files · 5 lines\n"));
        assert!(auto.contains("- `app/a.vue` (Vue, 5 lines)\n"));
        assert!(auto.contains("- `module.json` (JSON, 10 lines) — manifest\n"));
        assert!(auto.contains("- `gone.rs` (missing) — was here\n"));

        // Second sync over the composed body changes nothing.
        let body = fresh_body("modules/memory", &auto);
        let again = compose_body(&body, &render_auto(&unit, std::slice::from_ref(&unit), &ctx, &file_descriptions(&body)));
        assert_eq!(again, body);
    }

    #[test]
    fn root_lists_units_as_a_tree() {
        let mk = |p: &str, files: usize| Unit {
            rel_path: p.into(),
            manifests: vec![],
            subdirs: vec![],
            files: vec![],
            totals: Totals { files, lines: 0, by_lang: BTreeMap::new() },
        };
        let root = Unit { rel_path: ROOT.into(), manifests: vec![], subdirs: vec![SubDir { name: "misc".into(), rel_path: "misc".into(), totals: Totals::default(), is_unit: false }], files: vec![OwnedFile { rel: "README.md".into(), lang: "Markdown".into(), lines: Some(1) }], totals: Totals::default() };
        let all = vec![root.clone(), mk("apps", 3), mk("apps/shell", 2), mk("modules", 9)];
        let summaries = BTreeMap::from([("apps/shell".to_string(), "The Nuxt shell.".to_string())]);
        let ctx = RenderCtx { root_title: "QS codebase", workspace_path: "C:/x", summaries: &summaries };
        let auto = render_auto(&root, &all, &ctx, &BTreeMap::new());
        assert!(auto.contains("Workspace root `C:/x`"));
        assert!(auto.contains("\nUnits:\n- [[apps|apps/]] — 3 files · 0 lines\n  - [[apps-shell|apps/shell/]] — 2 files · 0 lines — The Nuxt shell.\n- [[modules|modules/]] — 9 files · 0 lines\n"));
        assert!(auto.contains("Other directories:\n- `misc/` — 0 files · 0 lines\n"));
        assert!(auto.contains("## Files\n- `README.md` (Markdown, 1 lines)\n"));
    }
}
