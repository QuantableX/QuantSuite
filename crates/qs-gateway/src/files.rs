//! Server files for other devices (PLAN-SERVER-CLIENT §6.5): a file browser
//! over chosen roots, downloads, and uploads into one transfers folder. A file
//! dialog on a client cannot see the server's disk; the pickers of
//! server-placed modules browse here instead.
//!
//! Paths are checked after resolving links: inside a root, never inside a
//! denied path (the vault, the signing key, `gateway.db`). Checksums are
//! SHA-256 over the whole file.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct FileRoots {
    /// `(label, folder)` — what the browser starts with.
    pub roots: Vec<(String, PathBuf)>,
    /// Never listed, never downloaded.
    pub deny: Vec<PathBuf>,
    /// Where uploads land.
    pub transfers: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    /// `None` for the list of roots.
    pub path: Option<String>,
    /// The folder above, while it is still inside a root.
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
}

/// Entries one listing returns at most.
const MAX_ENTRIES: usize = 5000;

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// A path as the user reads it (no `\\?\` prefix).
pub fn display(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(String::from).unwrap_or(s)
}

fn modified(m: &std::fs::Metadata) -> Option<String> {
    m.modified().ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

impl FileRoots {
    fn roots_canon(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|(_, p)| canon(p)).collect()
    }

    fn denied(&self, p: &Path) -> bool {
        self.deny.iter().any(|d| p.starts_with(canon(d)))
    }

    /// An existing path inside a root and outside every denied path.
    pub fn check(&self, path: &str) -> Result<PathBuf, String> {
        let p = std::fs::canonicalize(path).map_err(|_| format!("{path} does not exist on the server."))?;
        if !self.roots_canon().iter().any(|r| p.starts_with(r)) || self.denied(&p) {
            return Err(format!("{path} is outside the folders other devices may see."));
        }
        Ok(p)
    }

    pub fn list(&self, path: Option<&str>) -> Result<Listing, String> {
        let Some(path) = path.filter(|p| !p.trim().is_empty()) else {
            let entries = self
                .roots
                .iter()
                .filter(|(_, p)| p.is_dir())
                .map(|(label, p)| Entry { name: label.clone(), path: display(&canon(p)), dir: true, size: 0, modified: None })
                .collect();
            return Ok(Listing { path: None, parent: None, entries });
        };
        let dir = self.check(path)?;
        if !dir.is_dir() {
            return Err(format!("{path} is not a folder."));
        }
        let mut entries = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| format!("{path}: {e}"))?.flatten() {
            let p = e.path();
            if self.denied(&canon(&p)) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            entries.push(Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: display(&p),
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() },
                modified: modified(&m),
            });
            if entries.len() >= MAX_ENTRIES {
                break;
            }
        }
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let roots = self.roots_canon();
        let parent = dir
            .parent()
            .filter(|p| roots.iter().any(|r| p.starts_with(r)) && !roots.iter().any(|r| r == &dir))
            .map(display);
        Ok(Listing { path: Some(display(&dir)), parent, entries })
    }

    /// Where an upload called `name` lands: a plain file name in the
    /// transfers folder.
    pub fn upload_target(&self, name: &str) -> Result<PathBuf, String> {
        let base = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let clean: String = base.chars().filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')).collect();
        let clean = clean.trim().trim_matches('.').to_string();
        if clean.is_empty() || clean.ends_with(".part") || clean.ends_with(".part.meta") {
            return Err(format!("{name:?} is not a usable file name."));
        }
        std::fs::create_dir_all(&self.transfers).map_err(|e| format!("{}: {e}", self.transfers.display()))?;
        Ok(self.transfers.join(clean))
    }
}

/// SHA-256 of a whole file, hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(hex::encode(ctx.finish().as_ref()))
}

/// `name.part` and `name.part.meta` next to `target`.
pub fn part_paths(target: &Path) -> (PathBuf, PathBuf) {
    let s = target.as_os_str().to_owned();
    let mut part = s.clone();
    part.push(".part");
    let mut meta = s;
    meta.push(".part.meta");
    (PathBuf::from(part), PathBuf::from(meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qs-gateway-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_roots_and_never_denied_paths() {
        let d = tmp("roots");
        std::fs::create_dir_all(d.join("data/secrets")).unwrap();
        std::fs::create_dir_all(d.join("data/exports")).unwrap();
        std::fs::write(d.join("data/secrets/vault.db"), b"x").unwrap();
        std::fs::write(d.join("data/exports/a.csv"), b"1,2").unwrap();
        std::fs::write(d.join("outside.txt"), b"no").unwrap();
        let roots = FileRoots {
            roots: vec![("QuantSuite data".into(), d.join("data"))],
            deny: vec![d.join("data/secrets")],
            transfers: d.join("data/transfers"),
        };
        let top = roots.list(None).unwrap();
        assert_eq!(top.entries.len(), 1);
        let data = roots.list(Some(&top.entries[0].path)).unwrap();
        let names: Vec<_> = data.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["exports"], "secrets/ is not listed");
        assert!(data.parent.is_none(), "no way above a root");
        let exports = roots.list(Some(&data.entries[0].path)).unwrap();
        assert_eq!(exports.entries[0].size, 3);
        assert_eq!(exports.parent.as_deref(), data.path.as_deref());
        assert!(roots.check(&d.join("data/secrets/vault.db").display().to_string()).is_err());
        assert!(roots.check(&d.join("outside.txt").display().to_string()).is_err());
        assert!(roots.check(&d.join("data/exports/../secrets/vault.db").display().to_string()).is_err(), "no way around by ..");
        assert!(roots.check(&d.join("data/exports/a.csv").display().to_string()).is_ok());
    }

    #[test]
    fn upload_names_stay_in_the_transfers_folder() {
        let d = tmp("names");
        let roots = FileRoots { roots: vec![], deny: vec![], transfers: d.join("transfers") };
        assert_eq!(roots.upload_target("report.csv").unwrap(), d.join("transfers").join("report.csv"));
        assert_eq!(roots.upload_target(r"..\..\Windows\evil.dll").unwrap(), d.join("transfers").join("evil.dll"));
        assert!(roots.upload_target("..").is_err());
        assert!(roots.upload_target("x.part").is_err());
        let (p, m) = part_paths(&d.join("a.bin"));
        assert!(p.ends_with("a.bin.part") && m.ends_with("a.bin.part.meta"));
    }

    #[test]
    fn checksums_are_sha256() {
        let d = tmp("sha");
        std::fs::write(d.join("abc"), b"abc").unwrap();
        assert_eq!(sha256_file(&d.join("abc")).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
