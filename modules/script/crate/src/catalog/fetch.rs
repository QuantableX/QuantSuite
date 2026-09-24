//! Reading a catalog: resolve the source to one commit, then read every file
//! at that commit. GitHub goes through the REST API (a private repository
//! needs a token); a folder source reads a checkout directly. GitHub reads
//! are cached per source and commit under the module's data folder — a
//! commit never changes, and every byte is verified against its hash anyway.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::sources::Source;
use crate::store::sha256_hex;

pub trait Fetch {
    /// The commit every read of this session is pinned to.
    fn commit(&self) -> Result<String, String>;
    /// A file of the catalog at `commit`, by its repository-relative path.
    fn read(&self, commit: &str, path: &str) -> Result<Vec<u8>, String>;
}

/// Repository-relative, POSIX, no `..`, no absolute paths — what a manifest may name.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        && path.chars().all(|c| c.is_ascii_alphanumeric() || "_./-".contains(c))
}

// ─── GitHub ─────────────────────────────────────────────────────────────────

pub struct GitHub {
    owner: String,
    repo: String,
    branch: String,
    /// A folder inside the repository ('' = the root).
    prefix: String,
    token: Option<String>,
    cache: PathBuf,
    client: reqwest::blocking::Client,
}

const API: &str = "https://api.github.com";

impl GitHub {
    /// `cache` is this source's cache folder. Blocking: call it off the async runtime.
    pub fn new(source: &Source, token: Option<String>, cache: PathBuf) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("QuantSuite-QuantScript-Collection")
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("HTTP client: {e}"))?;
        Ok(Self {
            owner: source.owner.clone(),
            repo: source.repo.clone(),
            branch: source.branch.clone(),
            prefix: source.path.trim_matches('/').to_string(),
            token,
            cache,
            client,
        })
    }

    fn get(&self, url: &str, accept: &str) -> Result<reqwest::blocking::Response, String> {
        let mut request = self.client.get(url).header("Accept", accept).header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|e| format!("GitHub could not be reached ({}/{}): {}", self.owner, self.repo, e.without_url()))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = response.text().unwrap_or_default();
        Err(github_error(&format!("{}/{}", self.owner, self.repo), status, remaining.as_deref(), &body, self.token.is_some()))
    }
}

/// One sentence for a failed GitHub request.
pub fn github_error(repo: &str, status: u16, rate_remaining: Option<&str>, body: &str, has_token: bool) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_default();
    match status {
        404 if has_token => format!("{repo}: not found, or the token has no access to it."),
        404 => format!("{repo}: not found or no access — a private repository needs a token."),
        401 => format!("GitHub refused the token for {repo} — set a new one."),
        403 | 429 if rate_remaining == Some("0") => {
            if has_token {
                "GitHub rate limit reached — try again later.".to_string()
            } else {
                "GitHub rate limit reached — add a token.".to_string()
            }
        }
        403 => format!("GitHub denied access to {repo}{}", if message.is_empty() { ".".into() } else { format!(": {message}") }),
        _ => format!("GitHub answered {status} for {repo}{}", if message.is_empty() { ".".into() } else { format!(": {message}") }),
    }
}

impl Fetch for GitHub {
    fn commit(&self) -> Result<String, String> {
        let url = format!("{API}/repos/{}/{}/commits/{}", self.owner, self.repo, self.branch);
        let doc: serde_json::Value = self
            .get(&url, "application/vnd.github+json")?
            .json()
            .map_err(|e| format!("GitHub sent an unreadable commit: {e}"))?;
        doc.get("sha")
            .and_then(|s| s.as_str())
            .filter(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_string)
            .ok_or_else(|| format!("GitHub did not name a commit for {}/{}@{}.", self.owner, self.repo, self.branch))
    }

    fn read(&self, commit: &str, path: &str) -> Result<Vec<u8>, String> {
        if !safe_path(path) {
            return Err(format!("'{path}' is not a catalog path."));
        }
        let cached = self.cache.join(commit).join(path);
        if let Ok(bytes) = fs::read(&cached) {
            return Ok(bytes);
        }
        let full = if self.prefix.is_empty() { path.to_string() } else { format!("{}/{path}", self.prefix) };
        let url = format!("{API}/repos/{}/{}/contents/{full}?ref={commit}", self.owner, self.repo);
        let bytes = self
            .get(&url, "application/vnd.github.raw+json")?
            .bytes()
            .map_err(|e| format!("GitHub cut {path} short: {}", e.without_url()))?
            .to_vec();
        if let Some(parent) = cached.parent() {
            if fs::create_dir_all(parent).is_ok() {
                let _ = qs_core::paths::write_atomic(&cached, &bytes);
            }
        }
        Ok(bytes)
    }
}

// ─── A folder ───────────────────────────────────────────────────────────────

pub struct Folder {
    root: PathBuf,
}

impl Folder {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
}

impl Fetch for Folder {
    /// A checkout may hold uncommitted work: its "commit" is the catalog's hash.
    fn commit(&self) -> Result<String, String> {
        let catalog = self.root.join("catalog.json");
        let bytes = fs::read(&catalog).map_err(|e| format!("No catalog.json in {}: {e}", self.root.display()))?;
        Ok(format!("local-{}", &sha256_hex(&bytes)[..12]))
    }

    fn read(&self, _commit: &str, path: &str) -> Result<Vec<u8>, String> {
        if !safe_path(path) {
            return Err(format!("'{path}' is not a catalog path."));
        }
        fs::read(self.root.join(path)).map_err(|e| format!("Could not read {path} in {}: {e}", self.root.display()))
    }
}
