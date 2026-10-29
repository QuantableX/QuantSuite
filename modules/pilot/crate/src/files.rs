//! The inspector's Files tab (card 13): a lazy, gitignore-aware listing of
//! a thread's checkout, reading a file for its viewer (text, or base64 for
//! images and PDFs), saving an edit inside the checkout, a bounded project
//! text search, and the dev servers a thread started — is a port alive,
//! which process holds it, stop it.

use serde::Serialize;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SKIP_DIRS: [&str; 3] = [".git", ".qs-worktrees", "node_modules"];
/// A text file up to this size opens whole; a larger one is cut.
const TEXT_MAX: u64 = 4 * 1024 * 1024;
/// Images and PDFs up to this size are sent to the viewer.
const BINARY_MAX: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// Relative to the checkout, forward slashes.
    pub rel: String,
    pub dir: bool,
    pub size: u64,
}

/// One directory's children: folders first, then files, by name; ignored
/// files (`.gitignore`) and `.git` stay out.
pub fn list_dir(root: &Path, rel: &str) -> Result<Vec<DirEntry>, String> {
    let dir = inside(root, rel)?;
    if !dir.is_dir() {
        return Err(format!("{rel} is not a folder"));
    }
    let walker = ignore::WalkBuilder::new(&dir)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .build();
    let mut out = Vec::new();
    for entry in walker.flatten() {
        if entry.depth() == 0 {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        if is_dir && SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let Ok(r) = entry.path().strip_prefix(root) else {
            continue;
        };
        let size = if is_dir { 0 } else { entry.metadata().map(|m| m.len()).unwrap_or(0) };
        out.push(DirEntry { name, rel: r.to_string_lossy().replace('\\', "/"), dir: is_dir, size });
    }
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// `rel` under `root`, never outside it (no `..` escapes).
fn inside(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches(['/', '\\']);
    let path = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
    let canon_root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    let canon = path.canonicalize().map_err(|_| format!("{rel} is not there"))?;
    if canon.starts_with(&canon_root) {
        Ok(path)
    } else {
        Err(format!("{rel} is outside the thread's folder"))
    }
}

/// An absolute path, or one relative to `root`; whether it lies inside `root`.
pub fn resolve(root: &Path, path: &str) -> (PathBuf, bool) {
    let p = Path::new(path.trim());
    let abs = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    let within = match (abs.canonicalize(), root.canonicalize()) {
        (Ok(a), Ok(r)) => a.starts_with(r),
        _ => false,
    };
    (abs, within)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub path: String,
    /// Relative to the checkout when it lies inside.
    pub rel: Option<String>,
    /// `text`, `image`, `pdf` or `binary`.
    pub kind: String,
    pub text: Option<String>,
    pub base64: Option<String>,
    pub mime: String,
    pub size: u64,
    /// Only the first part of a large text file came.
    pub truncated: bool,
    /// Outside the checkout: no saving.
    pub read_only: bool,
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn image_mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

pub fn read(root: &Path, path: &str) -> Result<FileContent, String> {
    let (abs, within) = resolve(root, path);
    let meta = std::fs::metadata(&abs).map_err(|_| format!("{path} is not there"))?;
    if meta.is_dir() {
        return Err(format!("{path} is a folder"));
    }
    let size = meta.len();
    let rel = within.then(|| abs.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default());
    let e = ext(&abs);
    let mut out = FileContent { path: abs.to_string_lossy().into_owned(), rel, size, read_only: !within, ..Default::default() };
    if let Some(mime) = image_mime(&e).or((e == "pdf").then_some("application/pdf")) {
        if size > BINARY_MAX {
            return Err(format!("{} MB is too large to preview", size / (1024 * 1024)));
        }
        let bytes = std::fs::read(&abs).map_err(|e| e.to_string())?;
        out.kind = if e == "pdf" { "pdf" } else { "image" }.into();
        out.mime = mime.into();
        out.base64 = Some(crate::context::b64(&bytes));
        out.read_only = true;
        return Ok(out);
    }
    let mut f = std::fs::File::open(&abs).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(size.min(TEXT_MAX) as usize);
    f.by_ref().take(TEXT_MAX).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        out.kind = "binary".into();
        out.mime = "application/octet-stream".into();
        out.read_only = true;
        return Ok(out);
    }
    out.truncated = size > TEXT_MAX;
    out.read_only |= out.truncated;
    out.kind = "text".into();
    out.mime = if e == "svg" { "image/svg+xml" } else { "text/plain" }.into();
    out.text = Some(String::from_utf8_lossy(&bytes).into_owned());
    Ok(out)
}

/// Save an edit — only inside the checkout, only over an existing file.
pub fn write(root: &Path, path: &str, content: &str) -> Result<(), String> {
    let (abs, within) = resolve(root, path);
    if !within {
        return Err("files outside the thread's folder open read-only".into());
    }
    if !abs.is_file() {
        return Err(format!("{path} is not there any more"));
    }
    std::fs::write(&abs, content).map_err(|e| format!("{path}: {e}"))
}

/// Save as: the open buffer when given, else a copy of the file — to a
/// place the user picked in the save dialog.
pub fn save_as(root: &Path, path: &str, target: &str, content: Option<&str>) -> Result<(), String> {
    let target = Path::new(target.trim());
    if !target.is_absolute() {
        return Err("pick where to save the file".into());
    }
    match content {
        Some(text) => std::fs::write(target, text),
        None => std::fs::copy(resolve(root, path).0, target).map(|_| ()),
    }
    .map_err(|e| format!("{}: {e}", target.display()))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    pub line: usize,
    /// 1-based, in characters.
    pub column: usize,
    /// The line, cut around the match.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    pub rel: String,
    pub matches: Vec<LineMatch>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub files: Vec<FileMatches>,
    pub total: usize,
    /// A cap or the time budget stopped the search early.
    pub truncated: bool,
}

const SEARCH_MATCHES: usize = 1000;
const SEARCH_FILES: usize = 300;
const SEARCH_FILE_MAX: u64 = 1024 * 1024;
const SEARCH_BUDGET: Duration = Duration::from_secs(4);

/// A text search over the checkout's files — gitignore-aware, binary and
/// large files skipped, at most [`SEARCH_MATCHES`] matches.
pub fn search(root: &Path, query: &str, regex: bool, case: bool) -> Result<SearchResult, String> {
    if query.is_empty() {
        return Ok(SearchResult { files: Vec::new(), total: 0, truncated: false });
    }
    let pattern = if regex { query.to_string() } else { regex::escape(query) };
    let re = regex::RegexBuilder::new(&pattern).case_insensitive(!case).size_limit(1 << 20).build().map_err(|e| format!("not a valid pattern: {e}"))?;
    let started = Instant::now();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&name.as_ref()))
        })
        .build();
    let (mut files, mut total, mut truncated) = (Vec::new(), 0usize, false);
    for entry in walker.flatten() {
        if total >= SEARCH_MATCHES || files.len() >= SEARCH_FILES || started.elapsed() > SEARCH_BUDGET {
            truncated = true;
            break;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) || entry.metadata().map(|m| m.len() > SEARCH_FILE_MAX).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut matches = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = re.find(line) {
                let column = line[..m.start()].chars().count() + 1;
                let from = line[..m.start()].char_indices().rev().nth(40).map(|(i, _)| i).unwrap_or(0);
                let preview: String = line[from..].chars().take(240).collect();
                matches.push(LineMatch { line: i + 1, column, text: preview.trim_end().to_string() });
                total += 1;
                if total >= SEARCH_MATCHES {
                    break;
                }
            }
        }
        if !matches.is_empty() {
            let rel = entry.path().strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            files.push(FileMatches { rel, matches });
        }
    }
    Ok(SearchResult { files, total, truncated })
}

// ── Dev servers ──

/// Which of these local ports something listens on.
pub fn alive_ports(ports: &[u16]) -> Vec<u16> {
    ports
        .iter()
        .copied()
        .filter(|p| {
            [SocketAddr::from(([127, 0, 0, 1], *p)), SocketAddr::from(([0u16, 0, 0, 0, 0, 0, 0, 1], *p))]
                .iter()
                .any(|a| TcpStream::connect_timeout(a, Duration::from_millis(150)).is_ok())
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
}

/// The pid listening on `port` in `netstat -ano` output.
pub fn pid_in_netstat(out: &str, port: u16) -> Option<u32> {
    out.lines().find_map(|l| {
        let cols: Vec<&str> = l.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") || !cols[3].eq_ignore_ascii_case("LISTENING") {
            return None;
        }
        let local_port = cols[1].rsplit(':').next()?.parse::<u16>().ok()?;
        (local_port == port).then(|| cols[4].parse().ok()).flatten()
    })
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let (ok, out, err) = crate::providers::run_capture_split(cmd, Duration::from_secs(10))?;
    if ok {
        Ok(out)
    } else {
        Err(err.trim().to_string())
    }
}

/// The process that listens on a local port (Windows: `netstat`, `tasklist`).
pub fn port_owner(port: u16) -> Result<PortOwner, String> {
    let out = run("netstat", &["-ano", "-p", "TCP"])?;
    let pid = pid_in_netstat(&out, port).ok_or_else(|| format!("nothing listens on port {port} any more"))?;
    let filter = format!("PID eq {pid}");
    let name = run("tasklist", &["/FI", &filter, "/FO", "CSV", "/NH"])
        .ok()
        .and_then(|t| t.lines().next().and_then(|l| l.split(',').next()).map(|n| n.trim_matches('"').to_string()))
        .filter(|n| !n.is_empty() && !n.starts_with("INFO"))
        .unwrap_or_else(|| "unknown".into());
    Ok(PortOwner { pid, name })
}

/// End a server's process tree — only the one that still holds `port`.
pub fn stop_port(port: u16, pid: u32) -> Result<(), String> {
    let now = port_owner(port)?;
    if now.pid != pid {
        return Err(format!("port {port} belongs to another process now ({})", now.name));
    }
    run("taskkill", &["/PID", &pid.to_string(), "/T", "/F"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qp-files-{}", uuid::Uuid::new_v4()));
        for d in ["src/deep", "node_modules/x", "dist", ".git"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join(".gitignore"), "dist/\n*.log\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Hello\nfind me here\n").unwrap();
        std::fs::write(dir.join("src/a.ts"), "export const a = 1\n// FIND ME too\n").unwrap();
        std::fs::write(dir.join("src/deep/b.ts"), "const b = 'find me'\n").unwrap();
        std::fs::write(dir.join("dist/out.js"), "find me (ignored)\n").unwrap();
        std::fs::write(dir.join("x.log"), "find me (ignored)\n").unwrap();
        std::fs::write(dir.join("blob.bin"), [0u8, 1, 2, b'f']).unwrap();
        std::fs::write(dir.join("pic.png"), [137u8, 80, 78, 71]).unwrap();
        dir
    }

    #[test]
    fn listing_is_lazy_sorted_and_gitignore_aware() {
        let dir = tree();
        let top: Vec<(String, bool)> = list_dir(&dir, "").unwrap().into_iter().map(|e| (e.rel, e.dir)).collect();
        assert_eq!(
            top,
            vec![
                ("src".to_string(), true),
                (".gitignore".to_string(), false),
                ("blob.bin".to_string(), false),
                ("pic.png".to_string(), false),
                ("README.md".to_string(), false),
            ]
        );
        let src: Vec<String> = list_dir(&dir, "src").unwrap().into_iter().map(|e| e.rel).collect();
        assert_eq!(src, vec!["src/deep", "src/a.ts"]);
        assert!(list_dir(&dir, "../").is_err(), "no escaping the checkout");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reading_knows_text_images_and_binaries() {
        let dir = tree();
        let md = read(&dir, "README.md").unwrap();
        assert_eq!((md.kind.as_str(), md.rel.as_deref(), md.read_only), ("text", Some("README.md"), false));
        assert!(md.text.unwrap().starts_with("# Hello"));
        let png = read(&dir, "pic.png").unwrap();
        assert_eq!((png.kind.as_str(), png.mime.as_str()), ("image", "image/png"));
        assert!(png.base64.is_some());
        assert_eq!(read(&dir, "blob.bin").unwrap().kind, "binary");
        let abs = dir.join("src/a.ts").to_string_lossy().into_owned();
        assert_eq!(read(&dir, &abs).unwrap().rel.as_deref(), Some("src/a.ts"));
        let outside = std::env::temp_dir().join(format!("qp-outside-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&outside, "x").unwrap();
        let o = read(&dir, &outside.to_string_lossy()).unwrap();
        assert!(o.read_only && o.rel.is_none());
        assert!(write(&dir, &outside.to_string_lossy(), "y").is_err(), "outside files are read-only");
        write(&dir, "src/a.ts", "changed\n").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("src/a.ts")).unwrap(), "changed\n");
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn search_skips_ignored_and_binary_files() {
        let dir = tree();
        let r = search(&dir, "find me", false, false).unwrap();
        let mut rels: Vec<&str> = r.files.iter().map(|f| f.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, vec!["README.md", "src/a.ts", "src/deep/b.ts"]);
        assert_eq!(r.total, 3);
        let a = r.files.iter().find(|f| f.rel == "src/a.ts").unwrap();
        assert_eq!((a.matches[0].line, a.matches[0].column), (2, 4));
        let cased = search(&dir, "find me", false, true).unwrap();
        assert_eq!(cased.total, 2, "case-sensitive leaves FIND ME out");
        let re = search(&dir, r"const \w = ", true, false).unwrap();
        assert_eq!(re.total, 2);
        assert!(search(&dir, "(", true, false).is_err());
        assert_eq!(search(&dir, "(", false, false).unwrap().total, 0, "literal mode escapes");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn netstat_lines_name_the_listener() {
        let out = "\nActive Connections\n\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1196\n  TCP    127.0.0.1:3000         0.0.0.0:0              LISTENING       4242\n  TCP    127.0.0.1:52000        127.0.0.1:3000         ESTABLISHED     7777\n  TCP    [::1]:5173             [::]:0                 LISTENING       5151\n";
        assert_eq!(pid_in_netstat(out, 3000), Some(4242));
        assert_eq!(pid_in_netstat(out, 5173), Some(5151));
        assert_eq!(pid_in_netstat(out, 52000), None);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(alive_ports(&[port]), vec![port]);
        drop(listener);
        assert!(alive_ports(&[port]).is_empty());
    }
}
