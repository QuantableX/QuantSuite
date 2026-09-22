use ignore::overrides::{Override, OverrideBuilder};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Directories no workspace walk should ever descend into, `.gitignore` or
/// not. A Rust `target/` alone holds six-figure file counts after a few
/// builds, and `node_modules` is not far behind — walking them is what made
/// opening a big workspace (like QuantSuite itself) take seconds. Every
/// serious editor hardcodes this same list.
const HEAVY_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    ".nuxt",
    ".output",
    "dist",
    ".cache",
];

fn is_heavy_dir(entry: &ignore::DirEntry) -> bool {
    entry.file_type().is_some_and(|t| t.is_dir())
        && entry
            .file_name()
            .to_str()
            .is_some_and(|n| HEAVY_DIRS.contains(&n))
}

/// A node in the file tree sent to the frontend.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Option<Vec<FileEntry>>,
}

// ---------------------------------------------------------------------------
// read_dir_tree — returns a tree of FileEntry rooted at `path`
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn read_dir_tree(path: String, gitignore: bool) -> Result<Vec<FileEntry>, String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    // Collect every entry produced by the ignore-aware walker.
    // We skip the root directory entry itself.
    let walker = WalkBuilder::new(&root)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        // Without this, `.gitignore` files only apply INSIDE a git
        // repository — a plain folder (QuantSuite itself, pre-init) got no
        // filtering at all and the walk ingested node_modules and target.
        .require_git(false)
        .filter_entry(|e| !is_heavy_dir(e))
        .hidden(false) // show dotfiles; gitignore already filters
        .max_depth(None)
        .sort_by_file_path(|a, b| {
            // Directories first, then alphabetical (case-insensitive)
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a
                    .file_name()
                    .map(|n| n.to_ascii_lowercase())
                    .cmp(&b.file_name().map(|n| n.to_ascii_lowercase())),
            }
        })
        .build();

    // parent_path -> Vec<FileEntry>  (children collected per directory)
    let mut children_map: HashMap<PathBuf, Vec<FileEntry>> = HashMap::new();
    // Insertion-order list so we can resolve bottom-up later.
    let mut dir_order: Vec<PathBuf> = Vec::new();

    for entry_result in walker {
        let entry = entry_result.map_err(|e| e.to_string())?;
        let entry_path = entry.path().to_path_buf();

        // Skip the root itself
        if entry_path == root {
            children_map.insert(root.clone(), Vec::new());
            dir_order.push(root.clone());
            continue;
        }

        let is_dir = entry_path.is_dir();
        let name = entry_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let fe = FileEntry {
            name,
            path: entry_path.to_string_lossy().to_string(),
            is_directory: is_dir,
            children: if is_dir { Some(Vec::new()) } else { None },
        };

        // Register this entry under its parent
        let parent = entry_path.parent().unwrap_or(&root).to_path_buf();
        children_map.entry(parent).or_default().push(fe);

        if is_dir {
            children_map.entry(entry_path.clone()).or_default();
            dir_order.push(entry_path);
        }
    }

    // Resolve tree bottom-up: attach children to their directory entries.
    for dir in dir_order.iter().rev() {
        if *dir == root {
            continue;
        }
        let collected = children_map.remove(dir).unwrap_or_default();
        if collected.is_empty() {
            continue;
        }
        let parent = dir.parent().unwrap_or(&root).to_path_buf();
        if let Some(siblings) = children_map.get_mut(&parent) {
            for entry in siblings.iter_mut() {
                let entry_pb = PathBuf::from(&entry.path);
                if entry_pb == *dir {
                    entry.children = Some(collected);
                    break;
                }
            }
        }
    }

    // Sort children: directories first, then alphabetical
    fn sort_entries(entries: &mut [FileEntry]) {
        entries.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });
        for entry in entries.iter_mut() {
            if let Some(ref mut children) = entry.children {
                sort_entries(children);
            }
        }
    }

    let mut result = children_map.remove(&root).unwrap_or_default();

    sort_entries(&mut result);
    Ok(result)
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("File does not exist: {}", path));
    }
    if p.is_dir() {
        return Err(format!("Path is a directory, not a file: {}", path));
    }
    fs::read_to_string(p).map_err(|e| format!("Failed to read {}: {}", path, e))
}

// ---------------------------------------------------------------------------
// read_file_binary — reads any file and returns base64 + detected MIME type
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryFileResult {
    pub base64_data: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

fn detect_mime(path: &Path) -> String {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    match ext.as_str() {
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        // Video
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        // PDF
        "pdf" => "application/pdf",
        // Fallback
        _ => "application/octet-stream",
    }
    .to_string()
}

#[tauri::command]
pub async fn read_file_binary(path: String) -> Result<BinaryFileResult, String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("File does not exist: {}", path));
    }
    if p.is_dir() {
        return Err(format!("Path is a directory, not a file: {}", path));
    }

    let metadata = fs::metadata(p).map_err(|e| format!("Failed to read metadata for {}: {}", path, e))?;
    let size_bytes = metadata.len();

    // Cap at 50 MB to prevent memory issues
    if size_bytes > 50 * 1024 * 1024 {
        return Err(format!("File too large for preview ({} bytes): {}", size_bytes, path));
    }

    let bytes = fs::read(p).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    use base64::Engine;
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let mime_type = detect_mime(p);

    Ok(BinaryFileResult {
        base64_data,
        mime_type,
        size_bytes,
    })
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let p = Path::new(&path);
    // Ensure parent directory exists
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dirs for {}: {}", path, e))?;
        }
    }
    fs::write(p, content).map_err(|e| format!("Failed to write {}: {}", path, e))
}

// ---------------------------------------------------------------------------
// create_file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_file(path: String, is_directory: bool) -> Result<(), String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let p = Path::new(&path);
    if p.exists() {
        return Err(format!("Path already exists: {}", path));
    }
    if is_directory {
        fs::create_dir_all(p)
            .map_err(|e| format!("Failed to create directory {}: {}", path, e))
    } else {
        // Ensure parent exists
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dirs for {}: {}", path, e))?;
            }
        }
        fs::write(p, "").map_err(|e| format!("Failed to create file {}: {}", path, e))
    }
}

// ---------------------------------------------------------------------------
// delete_file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    let path = super::workspace::resolve_legacy_path(&path)?;
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if p.is_dir() {
        fs::remove_dir_all(p)
            .map_err(|e| format!("Failed to delete directory {}: {}", path, e))
    } else {
        fs::remove_file(p).map_err(|e| format!("Failed to delete file {}: {}", path, e))
    }
}

// ---------------------------------------------------------------------------
// rename_file
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    let old_path = super::workspace::resolve_legacy_path(&old_path)?;
    let new_path = super::workspace::resolve_legacy_path(&new_path)?;
    let src = Path::new(&old_path);
    if !src.exists() {
        return Err(format!("Source path does not exist: {}", old_path));
    }
    let dest = Path::new(&new_path);
    // Ensure destination parent exists
    if let Some(parent) = dest.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dirs for {}: {}", new_path, e))?;
        }
    }
    fs::rename(src, dest)
        .map_err(|e| format!("Failed to rename {} -> {}: {}", old_path, new_path, e))
}

// ---------------------------------------------------------------------------
// search_files — case-insensitive text search across files in a directory
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    pub line_number: usize,
    pub line: String,
    /// Byte offsets of the hit inside `line`, so the UI can highlight it.
    pub match_start: usize,
    pub match_end: usize,
}

/// The search box's filter panel, mirroring VS Code's: two globs and three
/// switches. All optional — an absent block searches everything, literally,
/// case-insensitively, which is what the old signature did.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// Comma-separated globs to search, e.g. `src/**,*.ts`.
    pub include: String,
    /// Comma-separated globs to skip. Added on top of .gitignore.
    pub exclude: String,
    /// Cap on returned matches. The default keeps a bad query from flooding.
    pub limit: Option<usize>,
}

/// Build the matcher once, for every flavour of the four switches.
///
/// Everything goes through `regex`: a literal search is the escaped pattern,
/// whole-word wraps it in boundaries, case-insensitivity is a flag. One code
/// path means the four options cannot disagree with each other.
fn build_matcher(pattern: &str, opts: &SearchOptions) -> Result<Regex, String> {
    let body = if opts.regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    let body = if opts.whole_word {
        format!(r"\b(?:{body})\b")
    } else {
        body
    };
    RegexBuilder::new(&body)
        .case_insensitive(!opts.case_sensitive)
        .build()
        .map_err(|e| format!("Invalid search pattern: {e}"))
}

/// Turn `a,b` into an override matcher. `include` allow-lists, `exclude`
/// denies; `ignore`'s override syntax marks denials with a leading `!`.
fn build_overrides(root: &Path, opts: &SearchOptions) -> Result<Option<Override>, String> {
    let include: Vec<&str> = opts.include.split(',').map(str::trim).filter(|g| !g.is_empty()).collect();
    let exclude: Vec<&str> = opts.exclude.split(',').map(str::trim).filter(|g| !g.is_empty()).collect();
    if include.is_empty() && exclude.is_empty() {
        return Ok(None);
    }

    let mut builder = OverrideBuilder::new(root);
    for glob in include {
        // A bare `*.ts` should match at any depth, the way VS Code reads it.
        let pattern = if glob.contains('/') { glob.to_string() } else { format!("**/{glob}") };
        builder.add(&pattern).map_err(|e| format!("Invalid include glob \"{glob}\": {e}"))?;
    }
    for glob in exclude {
        let pattern = if glob.contains('/') { glob.to_string() } else { format!("**/{glob}") };
        builder
            .add(&format!("!{pattern}"))
            .map_err(|e| format!("Invalid exclude glob \"{glob}\": {e}"))?;
    }
    builder.build().map(Some).map_err(|e| format!("Invalid glob set: {e}"))
}

#[tauri::command]
pub async fn search_files(
    path: String,
    pattern: String,
    options: Option<SearchOptions>,
) -> Result<Vec<SearchMatch>, String> {
    if pattern.is_empty() {
        return Ok(Vec::new());
    }

    let opts = options.unwrap_or_default();
    let matcher = build_matcher(&pattern, &opts)?;
    let limit = opts.limit.unwrap_or(500);
    let root = Path::new(&path);
    let overrides = build_overrides(root, &opts)?;

    let mut matches = Vec::new();

    // Same ignore-aware walk as the file tree — this used plain WalkDir with
    // NO filtering, so every search crawled node_modules and target too.
    let mut builder = WalkBuilder::new(&path);
    builder
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|e| !is_heavy_dir(e))
        .hidden(false);
    if let Some(ov) = overrides {
        builder.overrides(ov);
    }

    for entry in builder.build().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }

        // Non-UTF-8 files (binaries) are skipped rather than scanned.
        let Ok(content) = std::fs::read_to_string(entry_path) else {
            continue;
        };

        for (i, line) in content.lines().enumerate() {
            // One hit per line: the list is for finding the place, and a line
            // with six hits should not push five other files off the results.
            if let Some(m) = matcher.find(line) {
                matches.push(SearchMatch {
                    path: entry_path.to_string_lossy().to_string(),
                    line_number: i + 1,
                    // Very long lines (minified files) would bloat the payload.
                    line: line.chars().take(400).collect(),
                    match_start: m.start(),
                    match_end: m.end(),
                });
                if matches.len() >= limit {
                    return Ok(matches);
                }
            }
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod search_tests {
    use super::*;

    fn opts() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn literal_is_case_insensitive_by_default() {
        let m = build_matcher("todo", &opts()).unwrap();
        assert!(m.is_match("// TODO: fix"));
        assert!(m.is_match("todo"));
    }

    #[test]
    fn case_sensitive_switch_narrows_it() {
        let o = SearchOptions { case_sensitive: true, ..opts() };
        let m = build_matcher("TODO", &o).unwrap();
        assert!(m.is_match("// TODO"));
        assert!(!m.is_match("// todo"));
    }

    #[test]
    fn literal_pattern_is_escaped_not_treated_as_regex() {
        // Without escaping this would match any three characters.
        let m = build_matcher("a.c", &opts()).unwrap();
        assert!(m.is_match("a.c"));
        assert!(!m.is_match("abc"));
    }

    #[test]
    fn regex_switch_lets_the_pattern_through() {
        let o = SearchOptions { regex: true, ..opts() };
        let m = build_matcher(r"fn\s+\w+", &o).unwrap();
        assert!(m.is_match("pub fn search_files("));
        assert!(!m.is_match("function foo"));
    }

    #[test]
    fn whole_word_does_not_match_inside_a_word() {
        let o = SearchOptions { whole_word: true, ..opts() };
        let m = build_matcher("get", &o).unwrap();
        assert!(m.is_match("let x = get(1)"));
        assert!(!m.is_match("widget"));
    }

    #[test]
    fn whole_word_and_regex_combine() {
        let o = SearchOptions { whole_word: true, regex: true, ..opts() };
        let m = build_matcher("ge|se", &o).unwrap();
        // The alternation is grouped before the boundaries are applied, so
        // `\bge|se\b` (wrong) versus `\b(?:ge|se)\b` (right) is the test.
        assert!(m.is_match("a ge b"));
        assert!(!m.is_match("agent"));
    }

    #[test]
    fn a_broken_regex_is_an_error_not_a_panic() {
        let o = SearchOptions { regex: true, ..opts() };
        assert!(build_matcher("(unclosed", &o).is_err());
    }

    #[test]
    fn match_offsets_point_at_the_hit() {
        let m = build_matcher("world", &opts()).unwrap();
        let found = m.find("hello world!").unwrap();
        assert_eq!((found.start(), found.end()), (6, 11));
    }

    #[test]
    fn no_globs_means_no_overrides() {
        let root = Path::new(".");
        assert!(build_overrides(root, &opts()).unwrap().is_none());
    }

    #[test]
    fn a_bare_glob_matches_at_any_depth() {
        let root = Path::new("/w");
        let o = SearchOptions { include: "*.ts".into(), ..opts() };
        let ov = build_overrides(root, &o).unwrap().unwrap();
        assert!(ov.matched("/w/deep/nested/a.ts", false).is_whitelist());
        assert!(!ov.matched("/w/deep/a.js", false).is_whitelist());
    }

    #[test]
    fn exclude_globs_deny() {
        let root = Path::new("/w");
        let o = SearchOptions { exclude: "*.spec.ts".into(), ..opts() };
        let ov = build_overrides(root, &o).unwrap().unwrap();
        assert!(ov.matched("/w/a.spec.ts", false).is_ignore());
    }

    #[test]
    fn an_invalid_glob_is_reported() {
        let root = Path::new("/w");
        let o = SearchOptions { include: "[".into(), ..opts() };
        assert!(build_overrides(root, &o).is_err());
    }
}
