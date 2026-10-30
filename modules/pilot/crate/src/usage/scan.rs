//! The agents' own session logs, read for the Usage page (card 18). Token
//! counts only: prompts, replies and tool output are dropped as each line is
//! parsed.
//!
//! Claude Code writes `projects/**/*.jsonl` (subagent logs included). A
//! streamed reply is written several times and a resumed session copies the
//! history, so a request counts once by message id + request id across all
//! files. Codex writes rollouts under `sessions/` and `archived_sessions/`,
//! older ones zstd-compressed; its `token_count` events carry the session's
//! running total, and one whose total did not move repeats the last one.
//! Files are read again only when their size or time changed, an appended
//! log only from where the last read stopped.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Token counts of one request (Claude) or one hour of one model (Codex).
/// `input` is the uncached part for both providers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub output: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write_5m + self.cache_write_1h + self.output
    }
    pub fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.cache_read += o.cache_read;
        self.cache_write_5m += o.cache_write_5m;
        self.cache_write_1h += o.cache_write_1h;
        self.output += o.output;
    }
}

/// Prompt size for the long-context rates: 0 up to 200k, 1 up to 272k, 2 above.
pub fn size_class(prompt: u64) -> u8 {
    match prompt {
        0..=200_000 => 0,
        200_001..=272_000 => 1,
        _ => 2,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Claude's request identity for counting it once; Codex records have none.
    pub key: Option<u64>,
    /// Unix hour.
    pub hour: i64,
    pub model: String,
    pub class: u8,
    pub tokens: Tokens,
}

fn model_name(v: &Value) -> Option<String> {
    let name = v.as_str()?.trim();
    // `<synthetic>` replies never reached a model.
    (!name.is_empty() && !name.starts_with('<') && name.len() <= 128).then(|| name.to_ascii_lowercase())
}

fn hour_of(ts: &Value) -> Option<i64> {
    let t = chrono::DateTime::parse_from_rfc3339(ts.as_str()?).ok()?;
    Some(t.timestamp().div_euclid(3600))
}

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

/// One assistant line of a Claude Code log, if it reports usage.
pub fn claude_line(line: &[u8]) -> Option<Record> {
    let finder = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
    if !finder(b"\"usage\"") || !finder(b"\"assistant\"") {
        return None;
    }
    let entry: Value = serde_json::from_slice(line).ok()?;
    if entry["type"] != "assistant" {
        return None;
    }
    let message = &entry["message"];
    let usage = message.get("usage")?;
    let model = model_name(&message["model"])?;
    let hour = hour_of(&entry["timestamp"])?;
    let key = match (message["id"].as_str(), entry["requestId"].as_str()) {
        (Some(id), Some(request)) => {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (id, request).hash(&mut h);
            Some(h.finish())
        }
        _ => None,
    };
    let input = n(&usage["input_tokens"]);
    let cache_read = n(&usage["cache_read_input_tokens"]);
    let written = n(&usage["cache_creation_input_tokens"]);
    // Newer logs split cache writes by lifetime; an hour's write costs more.
    let hour_writes = n(&usage["cache_creation"]["ephemeral_1h_input_tokens"]).min(written);
    let tokens = Tokens { input, cache_read, cache_write_5m: written - hour_writes, cache_write_1h: hour_writes, output: n(&usage["output_tokens"]) };
    (tokens.total() > 0).then_some(Record { key, hour, model, class: size_class(input + cache_read + written), tokens })
}

/// What a Codex rollout carries from one line to the next.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexState {
    pub model: Option<String>,
    pub total: Option<u64>,
}

/// One line of a Codex rollout: the model from `turn_context`, tokens from `token_count`.
pub fn codex_line(line: &[u8], state: &mut CodexState) -> Option<Record> {
    // The type comes first; a short look rejects the large message lines unparsed.
    let head = &line[..line.len().min(240)];
    let context = memchr::memmem::find(head, b"\"turn_context\"").is_some();
    if !context && memchr::memmem::find(head, b"\"token_count\"").is_none() {
        return None;
    }
    let entry: Value = serde_json::from_slice(line).ok()?;
    let payload = &entry["payload"];
    if context {
        if entry["type"] == "turn_context" {
            if let Some(m) = model_name(&payload["model"]) {
                state.model = Some(m);
            }
        }
        return None;
    }
    if payload["type"] != "token_count" {
        return None;
    }
    let info = payload.get("info").filter(|i| i.is_object())?;
    let total = info["total_token_usage"]["total_tokens"].as_u64();
    if total.is_some() && total == state.total {
        return None;
    }
    state.total = total;
    let last = &info["last_token_usage"];
    let hour = hour_of(&entry["timestamp"])?;
    // OpenAI counts cached and cache-write tokens inside input_tokens, reasoning inside output_tokens.
    let prompt = n(&last["input_tokens"]);
    let cache_read = n(&last["cached_input_tokens"]).min(prompt);
    let cache_write = n(&last["cache_write_input_tokens"]).min(prompt - cache_read);
    let tokens = Tokens { input: prompt - cache_read - cache_write, cache_read, cache_write_5m: cache_write, cache_write_1h: 0, output: n(&last["output_tokens"]) };
    (tokens.total() > 0).then(|| Record { key: None, hour, model: state.model.clone().unwrap_or_else(|| "unknown".into()), class: size_class(prompt), tokens })
}

/// Codex needs hourly totals per model and size only.
fn merge_hourly(records: &mut Vec<Record>, r: Record) {
    match records.iter_mut().rev().take(8).find(|x| x.hour == r.hour && x.class == r.class && x.model == r.model) {
        Some(x) => x.tokens.add(&r.tokens),
        None => records.push(r),
    }
}

// ── Where the logs are ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
        }
    }
}

fn existing(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| p.is_dir())
        .filter(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
        .collect()
}

fn env_dirs(var: &str) -> Vec<PathBuf> {
    std::env::var(var).map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).collect()).unwrap_or_default()
}

/// The folders to read: the CLIs' own homes and the ones their variables name.
pub fn roots() -> Vec<(Provider, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut claude: Vec<PathBuf> = env_dirs("CLAUDE_CONFIG_DIR").into_iter().map(|d| d.join("projects")).collect();
    claude.push(home.join(".claude").join("projects"));
    claude.push(home.join(".config").join("claude").join("projects"));
    let mut codex = Vec::new();
    for base in env_dirs("CODEX_HOME").into_iter().chain([home.join(".codex")]) {
        codex.push(base.join("sessions"));
        codex.push(base.join("archived_sessions"));
    }
    existing(claude).into_iter().map(|d| (Provider::Claude, d)).chain(existing(codex).into_iter().map(|d| (Provider::Codex, d))).collect()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, keep: &dyn Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() && depth > 0 {
            walk(&path, depth - 1, out, keep);
        } else if kind.is_file() && path.file_name().and_then(|n| n.to_str()).is_some_and(keep) {
            out.push(path);
        }
    }
}

/// Every log file. A Codex rollout counts once even when two homes share it
/// (hard links, archived copies), the uncompressed copy first.
pub fn files(roots: &[(Provider, PathBuf)]) -> Vec<(Provider, PathBuf)> {
    let mut out = Vec::new();
    let mut rollouts: HashMap<String, PathBuf> = HashMap::new();
    for (provider, dir) in roots {
        let mut found = Vec::new();
        match provider {
            Provider::Claude => walk(dir, 6, &mut found, &|n| n.ends_with(".jsonl")),
            Provider::Codex => walk(dir, 6, &mut found, &|n| n.starts_with("rollout-") && (n.ends_with(".jsonl") || n.ends_with(".jsonl.zst"))),
        }
        for f in found {
            if *provider == Provider::Claude {
                out.push((Provider::Claude, f));
                continue;
            }
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let id = name.trim_end_matches(".zst").to_string();
            let plain = !name.ends_with(".zst");
            match rollouts.get(&id) {
                Some(prev) if !plain || !prev.to_string_lossy().ends_with(".zst") => {}
                _ => {
                    rollouts.insert(id, f);
                }
            }
        }
    }
    let mut codex: Vec<_> = rollouts.into_values().map(|p| (Provider::Codex, p)).collect();
    codex.sort_by(|a, b| a.1.cmp(&b.1));
    out.extend(codex);
    out
}

// ── The ledger: what each file said, kept between scans ──

#[derive(Debug, Default)]
struct FileEntry {
    provider: Option<Provider>,
    modified: Option<SystemTime>,
    len: u64,
    /// Where the next read of an appended log starts.
    offset: u64,
    records: Vec<Record>,
    codex: CodexState,
}

#[derive(Debug, Default)]
pub struct Ledger {
    files: HashMap<PathBuf, FileEntry>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanStats {
    pub files: usize,
    /// Bytes read in this scan (appended or changed files only).
    pub read: u64,
    pub unreadable: usize,
}

/// The longest line kept; a log line past it (an inlined image) carries no usage worth the memory.
const MAX_LINE: usize = 16 * 1024 * 1024;

fn parse_lines(bytes: &[u8], provider: Provider, entry: &mut FileEntry) {
    for line in bytes.split(|b| *b == b'\n') {
        if line.len() > MAX_LINE || line.is_empty() {
            continue;
        }
        match provider {
            Provider::Claude => {
                if let Some(r) = claude_line(line) {
                    entry.records.push(r);
                }
            }
            Provider::Codex => {
                if let Some(r) = codex_line(line, &mut entry.codex) {
                    merge_hourly(&mut entry.records, r);
                }
            }
        }
    }
}

fn read_zst(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(std::io::BufReader::new(file)).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

impl Ledger {
    /// Reads what changed since the last scan; files that are gone drop out.
    pub fn refresh(&mut self, files: &[(Provider, PathBuf)]) -> ScanStats {
        let mut stats = ScanStats { files: files.len(), ..Default::default() };
        let keep: HashSet<&PathBuf> = files.iter().map(|(_, p)| p).collect();
        self.files.retain(|p, _| keep.contains(p));
        for (provider, path) in files {
            let Ok(meta) = std::fs::metadata(path) else {
                stats.unreadable += 1;
                continue;
            };
            let modified = meta.modified().ok();
            let len = meta.len();
            let entry = self.files.entry(path.clone()).or_default();
            if entry.provider == Some(*provider) && entry.modified == modified && entry.len == len {
                continue;
            }
            let compressed = path.extension().is_some_and(|e| e == "zst");
            // An appended log is read from where the last read stopped; anything else from the start.
            if compressed || entry.provider != Some(*provider) || len < entry.offset {
                *entry = FileEntry { provider: Some(*provider), ..Default::default() };
            }
            let read = if compressed {
                read_zst(path).map(|b| (b.len(), b))
            } else {
                (|| {
                    let mut f = std::fs::File::open(path)?;
                    f.seek(SeekFrom::Start(entry.offset))?;
                    let mut buf = Vec::new();
                    f.read_to_end(&mut buf)?;
                    // Only complete lines: a line still being written waits for the next scan.
                    let end = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
                    buf.truncate(end);
                    Ok::<_, std::io::Error>((end, buf))
                })()
            };
            match read {
                Ok((consumed, bytes)) => {
                    stats.read += bytes.len() as u64;
                    parse_lines(&bytes, *provider, entry);
                    entry.offset += consumed as u64;
                    entry.modified = modified;
                    entry.len = len;
                }
                Err(_) => {
                    stats.unreadable += 1;
                    self.files.remove(path);
                }
            }
        }
        stats
    }

    /// Everything since `since_hour`, each Claude request once.
    pub fn records(&self, since_hour: i64) -> Vec<(Provider, &Record)> {
        let mut seen = HashSet::new();
        let mut paths: Vec<&PathBuf> = self.files.keys().collect();
        // A stable order, so the copy that counts does not change between scans.
        paths.sort();
        let mut out = Vec::new();
        for p in paths {
            let e = &self.files[p];
            let Some(provider) = e.provider else { continue };
            for r in &e.records {
                if r.hour < since_hour {
                    continue;
                }
                if let Some(k) = r.key {
                    if !seen.insert(k) {
                        continue;
                    }
                }
                out.push((provider, r));
            }
        }
        out
    }
}
