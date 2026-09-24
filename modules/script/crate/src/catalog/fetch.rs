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
        let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
        let remaining = header("x-ratelimit-remaining");
        let accepted = header("x-accepted-github-permissions");
        let expires = header("github-authentication-token-expiration");
        let body = response.text().unwrap_or_default();
        let repo = format!("{}/{}", self.owner, self.repo);
        let rate_limited = matches!(status, 403 | 429) && remaining.as_deref() == Some("0");
        match &self.token {
            Some(token) if matches!(status, 401 | 403 | 404) && !rate_limited => {
                let facts = self.token_facts(token, accepted, expires);
                Err(token_problem(&repo, &self.owner, status, &facts))
            }
            _ => Err(github_error(&repo, status, remaining.as_deref(), &body, self.token.is_some())),
        }
    }

    /// What GitHub says about the token behind a refused request — whose it
    /// is, whether it sees the repository, what the request needed. Two
    /// read-only calls; the token itself is never part of the answer.
    fn token_facts(&self, token: &str, accepted: Option<String>, expires: Option<String>) -> TokenFacts {
        let call = |url: String| {
            self.client
                .get(url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .bearer_auth(token)
                .send()
                .ok()
        };
        let mut facts = TokenFacts {
            kind: if token.starts_with("github_pat_") {
                "fine-grained"
            } else if token.starts_with("ghp_") {
                "classic"
            } else {
                "unrecognised"
            },
            accepted,
            expires,
            ..TokenFacts::default()
        };
        if let Some(user) = call(format!("{API}/user")) {
            facts.user_status = Some(user.status().as_u16());
            if user.status().is_success() {
                facts.login = user
                    .json::<serde_json::Value>()
                    .ok()
                    .and_then(|v| v.get("login").and_then(|l| l.as_str()).map(str::to_string));
            }
        }
        if let Some(repo) = call(format!("{API}/repos/{}/{}", self.owner, self.repo)) {
            facts.repo_status = Some(repo.status().as_u16());
        }
        facts
    }
}

/// GitHub's view of a token (see `GitHub::token_facts`).
#[derive(Debug, Default, Clone)]
pub struct TokenFacts {
    /// `fine-grained` | `classic` | `unrecognised` — from the prefix only.
    pub kind: &'static str,
    /// The account the token belongs to.
    pub login: Option<String>,
    /// GET /user: 200 = a working token, 401 = refused.
    pub user_status: Option<u16>,
    /// GET /repos/{owner}/{repo}: 200 = the repository is visible to it.
    pub repo_status: Option<u16>,
    /// X-Accepted-GitHub-Permissions of the refused request, e.g. `contents=read`.
    pub accepted: Option<String>,
    /// GitHub-Authentication-Token-Expiration.
    pub expires: Option<String>,
}

/// What is wrong with the token, and what to change, in plain words.
pub fn token_problem(repo: &str, owner: &str, status: u16, facts: &TokenFacts) -> String {
    let expires = facts.expires.as_deref().map(|e| format!(" It expires {e}.")).unwrap_or_default();
    if facts.user_status == Some(401) || status == 401 {
        return format!("GitHub refused the token ({} token): it is wrong, revoked or expired — set a new one.", facts.kind);
    }
    let who = match &facts.login {
        Some(login) => format!("The {} token belongs to {login}", facts.kind),
        None => format!("The {} token", facts.kind),
    };
    let owner_hint = match &facts.login {
        Some(login) if !login.eq_ignore_ascii_case(owner) => {
            format!(" It is not {owner}'s token — create it on the {owner} account (or pick {owner} as its resource owner).")
        }
        _ => String::new(),
    };
    let needed = facts.accepted.as_deref().filter(|a| !a.is_empty()).map(|a| format!(" GitHub wants: {a}.")).unwrap_or_default();
    if facts.repo_status == Some(200) {
        return format!(
            "{who} and sees {repo}, but may not read its files.{needed} Give it Repository permissions → Contents: Read-only.{expires}"
        );
    }
    format!(
        "{who} but GitHub does not show it {repo} (answer {status}).{owner_hint} In the token's settings: Repository access → Only select \
         repositories → {repo}, and Repository permissions → Contents: Read-only.{needed}{expires}"
    )
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
        // Without a token, a public catalog's files come from the raw host:
        // the REST API allows 60 anonymous requests an hour, an install
        // needs one per file. With a token, the contents API (private repos).
        let url = match self.token {
            Some(_) => format!("{API}/repos/{}/{}/contents/{full}?ref={commit}", self.owner, self.repo),
            None => raw_url(&self.owner, &self.repo, commit, &full),
        };
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

/// A file of a public repository at a commit, from GitHub's raw host.
pub fn raw_url(owner: &str, repo: &str, commit: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{owner}/{repo}/{commit}/{path}")
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
