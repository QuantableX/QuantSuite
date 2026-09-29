//! The built-in embedding engine: llama.cpp's `llama-server` hosting a
//! multilingual GGUF embedding model as a child process on 127.0.0.1.
//!
//! Like QuantVoice's Whisper, nothing ships in the installer. The operator's
//! click in Memory settings downloads a pinned llama.cpp build (the CUDA 13
//! build on machines with an NVIDIA driver — it carries sm_120 kernels for
//! Blackwell — otherwise the CPU build) and the chosen model from Hugging
//! Face into `<suite>/modules/memory/engine/`, each checked against a pinned
//! SHA-256. After that nothing touches the network: the server loads a local
//! file with `--offline`, listens on 127.0.0.1 only and wants a per-start
//! API key.
//!
//! The server starts with the first embedding request and exits after
//! `idleMinutes` without one, which gives its VRAM back to games and Whisper.
//! `auto` asks the runtime which devices it can drive and offloads every
//! layer to the first CUDA device; a GPU start that fails falls back to the
//! CPU. `--sleep-idle-seconds` is the safety net for a server orphaned by a
//! crashed suite: it drops its weights by itself, and the next start kills it.
use crate::retrieval::{cosine, EmbeddingConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The process register entry (ARCHITECTURE.md §6).
pub const PROCESS_ID: &str = "memory.embeddings";
/// The pinned llama.cpp release. A new build is a new runtime folder.
pub const LLAMA_BUILD: &str = "b11256";
pub const DEFAULT_MODEL: &str = "qwen3-embedding-0.6b";
const READY_TIMEOUT: Duration = Duration::from_secs(180);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Per-sequence context. A chunk is at most 1200 characters and a query at
/// most 2000 bytes; the whole input has to fit one physical batch.
const CONTEXT: &str = "4096";
const LOG_CAP: usize = 200;
const INSTALLED_MARKER: &str = ".installed";

pub struct Asset {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub struct Runtime {
    /// `cuda` (NVIDIA, also runs on the CPU) or `cpu`.
    pub variant: &'static str,
    pub assets: &'static [Asset],
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub const RUNTIMES: &[Runtime] = &[
    Runtime {
        variant: "cuda",
        assets: &[
            Asset {
                url: "https://github.com/ggml-org/llama.cpp/releases/download/b11256/llama-b11256-bin-win-cuda-13.4-x64.zip",
                sha256: "39b5c2bf0395fe26d35b1feb712e9c67ec1e1b3505f26f42e53a1c961eaade4b",
                size: 153_546_182,
            },
            Asset {
                url: "https://github.com/ggml-org/llama.cpp/releases/download/b11256/cudart-llama-bin-win-cuda-13.4-x64.zip",
                sha256: "738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668",
                size: 423_535_356,
            },
        ],
    },
    Runtime {
        variant: "cpu",
        assets: &[Asset {
            url: "https://github.com/ggml-org/llama.cpp/releases/download/b11256/llama-b11256-bin-win-cpu-x64.zip",
            sha256: "7c913da4c359ca3502278b7f69cc66f83781df087c330e69bdb8db9af316b802",
            size: 19_164_826,
        }],
    },
];
/// Other platforms keep Ollama until their llama.cpp archives are pinned here.
#[cfg(not(all(windows, target_arch = "x86_64")))]
pub const RUNTIMES: &[Runtime] = &[];

/// Qwen's recommended query format: an English task instruction, then the query.
const QWEN3_QUERY: &str =
    "Instruct: Given a question, retrieve the memory notes that answer it\nQuery:";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub repo: &'static str,
    /// Hugging Face commit the file is pinned to.
    pub revision: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub dims: usize,
    pub license: &'static str,
    /// Prepended to recall queries; memories are embedded as they are.
    pub query_prefix: &'static str,
    /// Worth it with a GPU; slow on a CPU.
    pub gpu_recommended: bool,
}

/// Multilingual (100+ languages), Apache-2.0. Q8_0 keeps the quality of the
/// fp16 weights at half the size.
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "qwen3-embedding-0.6b",
        label: "Qwen3-Embedding 0.6B",
        repo: "Qwen/Qwen3-Embedding-0.6B-GGUF",
        revision: "370f27d7550e0def9b39c1f16d3fbaa13aa67728",
        file: "Qwen3-Embedding-0.6B-Q8_0.gguf",
        sha256: "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439",
        size: 639_150_592,
        dims: 1024,
        license: "Apache-2.0",
        query_prefix: QWEN3_QUERY,
        gpu_recommended: false,
    },
    ModelSpec {
        id: "qwen3-embedding-4b",
        label: "Qwen3-Embedding 4B",
        repo: "Qwen/Qwen3-Embedding-4B-GGUF",
        revision: "f4602530db1d980e16da9d7d3a70294cf5c190be",
        file: "Qwen3-Embedding-4B-Q8_0.gguf",
        sha256: "b60ae5ce2dd6a0b77f82cadf21def1f310a3e10cde380ad0081b07a9d416949d",
        size: 4_279_660_224,
        dims: 2560,
        license: "Apache-2.0",
        query_prefix: QWEN3_QUERY,
        gpu_recommended: true,
    },
    ModelSpec {
        id: "qwen3-embedding-8b",
        label: "Qwen3-Embedding 8B",
        repo: "Qwen/Qwen3-Embedding-8B-GGUF",
        revision: "69d0e58a13e463cd99a9b83e3f5fee7c10265fab",
        file: "Qwen3-Embedding-8B-Q8_0.gguf",
        sha256: "d20ddc71e8a5c4344f2343481e242233a997dc5eaff442427a945836c97b4deb",
        size: 8_047_105_824,
        dims: 4096,
        license: "Apache-2.0",
        query_prefix: QWEN3_QUERY,
        gpu_recommended: true,
    },
];

pub fn model(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

fn runtime(variant: &str) -> Option<&'static Runtime> {
    RUNTIMES.iter().find(|r| r.variant == variant)
}

fn no_window(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(0x0800_0000);
    }
}

/// An NVIDIA driver is installed: `nvcuda.dll` (what CUDA programs load) or
/// `nvidia-smi` on the current PATH.
pub fn nvidia_present() -> bool {
    if cfg!(windows) {
        let system = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        if system.join("System32").join("nvcuda.dll").is_file() {
            return true;
        }
    }
    let exe = if cfg!(windows) { "nvidia-smi.exe" } else { "nvidia-smi" };
    std::env::split_paths(&qs_core::system_path::current()).any(|dir| dir.join(exe).is_file())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    pub id: String,
    pub name: String,
    pub total_mib: u64,
    pub free_mib: u64,
}

/// `  CUDA0: NVIDIA GeForce RTX 5090 Laptop GPU (24435 MiB, 23151 MiB free)`
fn parse_devices(output: &str) -> Vec<GpuDevice> {
    output
        .lines()
        .filter_map(|line| {
            let (id, rest) = line.trim().split_once(": ")?;
            if id.is_empty() || id.contains(' ') {
                return None;
            }
            let open = rest.rfind(" (")?;
            let (name, memory) = (&rest[..open], rest[open + 2..].trim_end_matches(')'));
            let mut numbers = memory
                .split(',')
                .filter_map(|part| part.split_whitespace().next()?.parse::<u64>().ok());
            Some(GpuDevice {
                id: id.to_string(),
                name: name.to_string(),
                total_mib: numbers.next()?,
                free_mib: numbers.next().unwrap_or(0),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub busy: bool,
    /// `runtime` | `extract` | `model` | `done` | `error` | empty
    pub phase: String,
    pub file: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

struct Server {
    child: Child,
    pid: u32,
    port: u16,
    api_key: String,
    /// Model, device setting and runtime: a change restarts the server.
    signature: String,
    model: &'static str,
    /// `cuda` or `cpu` — what the server actually runs on.
    device: String,
    gpu: Option<String>,
    ready: bool,
    started: Instant,
    load_ms: Option<u128>,
}

#[derive(Default)]
struct Inner {
    server: Option<Server>,
    generation: u64,
    last_used: Option<Instant>,
    in_flight: usize,
    idle: Duration,
    logs: VecDeque<String>,
    last_error: Option<String>,
    last_warning: Option<String>,
    devices: HashMap<&'static str, Vec<GpuDevice>>,
}

type Notifier = Arc<dyn Fn(Option<u32>) + Send + Sync>;

pub struct Engine {
    root: PathBuf,
    inner: Arc<Mutex<Inner>>,
    setup: Mutex<SetupState>,
    start: tokio::sync::Mutex<()>,
    notifier: Mutex<Option<Notifier>>,
    local: reqwest::Client,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub variant: &'static str,
    pub installed: bool,
    pub download_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    #[serde(flatten)]
    pub spec: &'static ModelSpec,
    pub installed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// Whether this platform has pinned runtime archives.
    pub supported: bool,
    pub build: &'static str,
    pub nvidia: bool,
    pub runtimes: Vec<RuntimeStatus>,
    pub models: Vec<ModelStatus>,
    /// What the saved settings still need to download (0 = ready to run).
    pub missing_bytes: u64,
    pub running: bool,
    pub ready: bool,
    pub pid: Option<u32>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub gpu: Option<String>,
    pub load_ms: Option<u128>,
    pub idle_seconds: Option<u64>,
    pub last_error: Option<String>,
    pub last_warning: Option<String>,
    pub setup: SetupState,
    pub dir: String,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Inner {
    fn log(&mut self, line: String) {
        if self.logs.len() >= LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }
}

fn push_log(inner: &Mutex<Inner>, line: String) {
    lock(inner).log(line);
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Engine {
    /// The engine rooted at `root` (normally `<suite>/modules/memory/engine`).
    pub fn new(root: PathBuf) -> Self {
        let local = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            root,
            inner: Arc::new(Mutex::new(Inner::default())),
            setup: Mutex::new(SetupState::default()),
            start: tokio::sync::Mutex::new(()),
            notifier: Mutex::new(None),
            local,
        }
    }

    /// Called with the pid when the server starts and `None` when it stops.
    pub fn set_notifier(&self, notifier: Notifier) {
        *lock(&self.notifier) = Some(notifier);
    }

    fn notify(&self, pid: Option<u32>) {
        let notifier = lock(&self.notifier).clone();
        if let Some(n) = notifier {
            n(pid);
        }
    }

    fn runtime_dir(&self, variant: &str) -> PathBuf {
        self.root.join("runtime").join(format!("{LLAMA_BUILD}-{variant}"))
    }

    fn server_exe(&self, variant: &str) -> PathBuf {
        let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
        self.runtime_dir(variant).join(exe)
    }

    fn model_path(&self, spec: &ModelSpec) -> PathBuf {
        self.root.join("models").join(spec.file)
    }

    fn marker_for(runtime: &Runtime) -> String {
        runtime.assets.iter().map(|a| a.sha256).collect::<Vec<_>>().join("\n")
    }

    pub fn runtime_installed(&self, variant: &str) -> bool {
        runtime(variant).is_some_and(|rt| {
            self.server_exe(variant).is_file()
                && std::fs::read_to_string(self.runtime_dir(variant).join(INSTALLED_MARKER))
                    .is_ok_and(|m| m == Self::marker_for(rt))
        })
    }

    pub fn model_installed(&self, spec: &ModelSpec) -> bool {
        let path = self.model_path(spec);
        path.metadata().is_ok_and(|m| m.len() == spec.size)
            && std::fs::read_to_string(path.with_extension("sha256")).is_ok_and(|s| s.trim() == spec.sha256)
    }

    /// The runtime a setting wants downloaded: CUDA on an NVIDIA machine unless
    /// the operator chose the CPU, else the small CPU build.
    fn wanted_runtime(config: &EmbeddingConfig) -> &'static str {
        if config.device != "cpu" && nvidia_present() {
            "cuda"
        } else {
            "cpu"
        }
    }

    /// The installed runtime to start for a setting (the CUDA build runs on the CPU too).
    fn installed_runtime(&self, config: &EmbeddingConfig) -> Option<&'static Runtime> {
        let order: &[&str] = if config.device == "cpu" { &["cpu", "cuda"] } else { &["cuda", "cpu"] };
        order.iter().copied().find(|v| self.runtime_installed(v)).and_then(runtime)
    }

    /// What a setting still has to download: runtime variant (if any) and whether the model is missing.
    fn missing(&self, config: &EmbeddingConfig) -> Result<(Option<&'static Runtime>, Option<&'static ModelSpec>), String> {
        if RUNTIMES.is_empty() {
            return Err("The built-in engine ships for Windows x64 so far; use Ollama on this system".into());
        }
        let spec = model(&config.builtin_model).ok_or("Unknown built-in model")?;
        let wanted = Self::wanted_runtime(config);
        let runtime_ok = self.runtime_installed(wanted) || (wanted == "cpu" && self.runtime_installed("cuda"));
        Ok((
            (!runtime_ok).then(|| runtime(wanted)).flatten(),
            (!self.model_installed(spec)).then_some(spec),
        ))
    }

    pub fn status(&self, config: &EmbeddingConfig) -> EngineStatus {
        let missing_bytes = self.missing(config).map_or(0, |(rt, spec)| {
            rt.map_or(0, |r| r.assets.iter().map(|a| a.size).sum::<u64>()) + spec.map_or(0, |s| s.size)
        });
        let mut inner = lock(&self.inner);
        let alive = inner.server.as_mut().is_some_and(|s| matches!(s.child.try_wait(), Ok(None)));
        let server = inner.server.as_ref().filter(|_| alive);
        EngineStatus {
            supported: !RUNTIMES.is_empty(),
            build: LLAMA_BUILD,
            nvidia: nvidia_present(),
            runtimes: RUNTIMES
                .iter()
                .map(|r| RuntimeStatus {
                    variant: r.variant,
                    installed: self.runtime_installed(r.variant),
                    download_bytes: r.assets.iter().map(|a| a.size).sum(),
                })
                .collect(),
            models: MODELS.iter().map(|spec| ModelStatus { spec, installed: self.model_installed(spec) }).collect(),
            missing_bytes,
            running: alive,
            ready: server.is_some_and(|s| s.ready),
            pid: server.map(|s| s.pid),
            model: server.map(|s| s.model.to_string()),
            device: server.map(|s| s.device.clone()),
            gpu: server.and_then(|s| s.gpu.clone()),
            load_ms: server.and_then(|s| s.load_ms),
            idle_seconds: server.and(inner.last_used).map(|t| t.elapsed().as_secs()),
            last_error: inner.last_error.clone(),
            last_warning: inner.last_warning.clone(),
            setup: lock(&self.setup).clone(),
            dir: self.root.to_string_lossy().into_owned(),
        }
    }

    pub fn logs(&self, limit: usize) -> Vec<String> {
        let inner = lock(&self.inner);
        let n = limit.min(inner.logs.len());
        inner.logs.iter().skip(inner.logs.len() - n).cloned().collect()
    }

    /// The device the running server uses (`cuda`/`cpu`), if one runs.
    pub fn device(&self) -> Option<String> {
        lock(&self.inner).server.as_ref().filter(|s| s.ready).map(|s| s.device.clone())
    }

    /// Stop the server; its RAM and VRAM go back to the system.
    pub fn stop(&self) {
        let server = {
            let mut inner = lock(&self.inner);
            inner.generation += 1;
            inner.server.take()
        };
        if let Some(mut s) = server {
            let _ = s.child.kill();
            let _ = s.child.wait();
            let _ = std::fs::remove_file(self.root.join("server.pid"));
            push_log(&self.inner, format!("[engine] stopped pid {}", s.pid));
            self.notify(None);
        }
    }

    // ── Installing ──

    /// Download and unpack what `config` needs. One install at a time; progress
    /// is in [`Engine::status`]. The caller enables the setting afterwards.
    pub async fn install(&self, config: &EmbeddingConfig) -> Result<(), String> {
        {
            let mut setup = lock(&self.setup);
            if setup.busy {
                return Err("A download is already running".into());
            }
            *setup = SetupState { busy: true, ..Default::default() };
        }
        let outcome = self.install_inner(config).await;
        let mut setup = lock(&self.setup);
        setup.busy = false;
        match &outcome {
            Ok(()) => setup.phase = "done".into(),
            Err(e) => {
                setup.phase = "error".into();
                setup.error = Some(e.clone());
            }
        }
        outcome
    }

    async fn install_inner(&self, config: &EmbeddingConfig) -> Result<(), String> {
        let (rt, spec) = self.missing(config)?;
        let web = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(120))
            .user_agent(concat!("QuantSuite/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        if let Some(rt) = rt {
            let downloads = self.root.join("downloads");
            let mut archives = Vec::new();
            for asset in rt.assets {
                let name = asset.url.rsplit('/').next().unwrap_or("runtime.zip");
                let dest = downloads.join(name);
                self.download(&web, "runtime", asset.url, &dest, asset.sha256, asset.size).await?;
                archives.push(dest);
            }
            self.set_phase("extract", "llama.cpp runtime", 0, 0);
            let final_dir = self.runtime_dir(rt.variant);
            let staging = final_dir.with_extension("staging");
            let marker = Self::marker_for(rt);
            tokio::task::spawn_blocking(move || extract(&archives, &staging, &final_dir, &marker))
                .await
                .map_err(|e| e.to_string())??;
            let _ = std::fs::remove_dir_all(&downloads);
        }
        if let Some(spec) = spec {
            let url = format!("https://huggingface.co/{}/resolve/{}/{}", spec.repo, spec.revision, spec.file);
            let dest = self.model_path(spec);
            self.download(&web, "model", &url, &dest, spec.sha256, spec.size).await?;
            std::fs::write(dest.with_extension("sha256"), spec.sha256).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn set_phase(&self, phase: &str, file: &str, done: u64, total: u64) {
        let mut setup = lock(&self.setup);
        setup.phase = phase.into();
        setup.file = file.into();
        setup.done = done;
        setup.total = total;
    }

    /// Stream `url` into `dest`, hashing on the way; a wrong size or digest is deleted, never used.
    async fn download(&self, web: &reqwest::Client, phase: &str, url: &str, dest: &Path, sha256: &str, size: u64) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let name = dest.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.set_phase(phase, &name, 0, size);
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
        }
        let part = dest.with_extension("part");
        let mut response = web
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Download of {name} failed: {e}"))?;
        let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let result: Result<(), String> = async {
            while let Some(chunk) = response.chunk().await.map_err(|e| format!("Download of {name} failed: {e}"))? {
                done += chunk.len() as u64;
                if done > size {
                    return Err(format!("{name} is larger than the pinned file"));
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                lock(&self.setup).done = done;
            }
            file.flush().await.map_err(|e| e.to_string())?;
            Ok(())
        }
        .await;
        drop(file);
        let digest = hex(&hasher.finalize());
        if let Err(e) = result.and_then(|()| {
            if done == size && digest == sha256 {
                Ok(())
            } else {
                Err(format!("{name} failed its integrity check (size {done}/{size}, sha256 {digest})"))
            }
        }) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }
        tokio::fs::rename(&part, dest).await.map_err(|e| e.to_string())
    }

    // ── Running ──

    /// Embed `inputs` (1-64 texts), starting the server when needed.
    pub async fn embed(&self, config: &EmbeddingConfig, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if inputs.is_empty() || inputs.len() > 64 {
            return Err("Embedding batch must contain 1-64 chunks".into());
        }
        // `ensure_running` counts the request in, so the idle watch cannot stop the server under it.
        let (port, key) = self.ensure_running(config).await?;
        let result = self.request(port, &key, inputs).await;
        let mut inner = lock(&self.inner);
        inner.in_flight -= 1;
        inner.last_used = Some(Instant::now());
        if let Err(e) = &result {
            inner.last_error = Some(e.clone());
        }
        result
    }

    async fn request(&self, port: u16, key: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
        #[derive(Deserialize)]
        struct Item {
            index: usize,
            embedding: Vec<f32>,
        }
        #[derive(Deserialize)]
        struct Response {
            data: Vec<Item>,
        }
        let response = self
            .local
            .post(format!("http://127.0.0.1:{port}/v1/embeddings"))
            .bearer_auth(key)
            .json(&serde_json::json!({ "input": inputs, "encoding_format": "float" }))
            .send()
            .await
            .map_err(|e| format!("Built-in embedding engine unavailable: {e}"))?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(format!("Built-in embedding engine returned {status}: {}", text.chars().take(300).collect::<String>()));
        }
        let mut parsed: Response = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid embedding response: {e}"))?;
        parsed.data.sort_by_key(|item| item.index);
        let dims = parsed.data.first().map_or(0, |item| item.embedding.len());
        if parsed.data.len() != inputs.len()
            || dims == 0
            || dims > 16384
            || parsed.data.iter().enumerate().any(|(i, item)| item.index != i || item.embedding.len() != dims || cosine(&item.embedding, &item.embedding).is_none())
        {
            return Err("Embedding response has invalid vectors or dimensions".into());
        }
        Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
    }

    /// The running server's port and key; starts (or restarts) it for `config`.
    async fn ensure_running(&self, config: &EmbeddingConfig) -> Result<(u16, String), String> {
        let spec = model(&config.builtin_model).ok_or("Unknown built-in model")?;
        let rt = self
            .installed_runtime(config)
            .ok_or("The built-in engine is not downloaded yet — activate it in Memory settings")?;
        if !self.model_installed(spec) {
            return Err(format!("{} is not downloaded yet — activate it in Memory settings", spec.label));
        }
        let signature = format!("{}|{}|{}", spec.id, config.device, rt.variant);
        let _guard = self.start.lock().await;
        let resume = {
            let mut inner = lock(&self.inner);
            inner.idle = Duration::from_secs(u64::from(config.idle_minutes.max(1)) * 60);
            let alive = inner.server.as_mut().is_some_and(|s| matches!(s.child.try_wait(), Ok(None)));
            let generation = inner.generation;
            match inner.server.as_ref().filter(|s| alive && s.signature == signature) {
                Some(s) if s.ready => {
                    let found = (s.port, s.api_key.clone());
                    inner.in_flight += 1;
                    inner.last_used = Some(Instant::now());
                    return Ok(found);
                }
                // A start whose caller gave up (a recall timeout): keep waiting for it.
                Some(s) => Some((s.port, s.api_key.clone(), s.started, generation)),
                None => None,
            }
        };
        if let Some((port, key, started, generation)) = resume {
            if self.wait_ready(port, started, generation).await.is_ok() {
                self.watch_idle(generation);
                self.count_in();
                return Ok((port, key));
            }
        }
        // Another setting or a dead child: start clean.
        self.stop();
        self.kill_orphan();
        let devices = self.devices(rt).await;
        let gpu = devices.iter().find(|d| d.id.starts_with("CUDA")).cloned();
        let mut attempts: Vec<Option<GpuDevice>> = Vec::new();
        match (config.device.as_str(), gpu) {
            ("cpu", _) => attempts.push(None),
            ("gpu", None) => {
                return Err("No usable NVIDIA GPU: the CUDA runtime sees no device (driver too old for CUDA 13, or no NVIDIA GPU). Choose Auto or CPU.".into())
            }
            ("gpu", Some(g)) => attempts.push(Some(g)),
            (_, Some(g)) => {
                attempts.push(Some(g));
                attempts.push(None);
            }
            (_, None) => attempts.push(None),
        }
        let mut errors = Vec::new();
        for gpu in attempts {
            let label = gpu.as_ref().map_or("CPU".to_string(), |g| g.name.clone());
            match self.start_server(rt, spec, gpu, &signature, config).await {
                Ok(ok) => {
                    lock(&self.inner).last_warning = (!errors.is_empty()).then(|| format!("GPU start failed, running on the CPU: {}", errors.join("; ")));
                    self.count_in();
                    return Ok(ok);
                }
                Err(e) => {
                    self.stop();
                    errors.push(format!("{label}: {e}"));
                }
            }
        }
        let message = errors.join("; ");
        lock(&self.inner).last_error = Some(message.clone());
        Err(message)
    }

    /// Devices the runtime can offload to (`--list-devices`), cached per runtime.
    async fn devices(&self, rt: &'static Runtime) -> Vec<GpuDevice> {
        if let Some(cached) = lock(&self.inner).devices.get(rt.variant) {
            return cached.clone();
        }
        let exe = self.server_exe(rt.variant);
        let listed = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&exe);
            cmd.arg("--list-devices").stdin(Stdio::null());
            no_window(&mut cmd);
            cmd.output().map(|o| parse_devices(&format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))))
        })
        .await;
        let devices = match listed {
            Ok(Ok(devices)) => devices,
            other => {
                push_log(&self.inner, format!("[engine] --list-devices failed: {other:?}"));
                Vec::new()
            }
        };
        push_log(&self.inner, format!("[engine] {} devices: {devices:?}", rt.variant));
        // Only a found device is cached: a driver installed later is seen on the next start.
        if !devices.is_empty() {
            lock(&self.inner).devices.insert(rt.variant, devices.clone());
        }
        devices
    }

    /// A server a crashed suite left behind would hold its model until it sleeps; end it.
    fn kill_orphan(&self) {
        let pid_file = self.root.join("server.pid");
        let Ok(pid) = std::fs::read_to_string(&pid_file) else { return };
        let _ = std::fs::remove_file(&pid_file);
        let Ok(pid) = pid.trim().parse::<u32>() else { return };
        if cfg!(windows) {
            let mut list = Command::new("tasklist");
            list.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
            no_window(&mut list);
            let is_ours = list.output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("llama-server"));
            if is_ours {
                let mut kill = Command::new("taskkill");
                kill.args(["/PID", &pid.to_string(), "/F"]);
                no_window(&mut kill);
                let _ = kill.output();
                push_log(&self.inner, format!("[engine] ended orphaned server pid {pid}"));
            }
        }
    }

    async fn start_server(
        &self,
        rt: &'static Runtime,
        spec: &'static ModelSpec,
        gpu: Option<GpuDevice>,
        signature: &str,
        config: &EmbeddingConfig,
    ) -> Result<(u16, String), String> {
        let port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|l| l.local_addr())
            .map_err(|e| format!("no free local port: {e}"))?
            .port();
        let api_key = uuid::Uuid::new_v4().simple().to_string();
        let sleep_after = (u64::from(config.idle_minutes.max(1)) * 60 + 60).to_string();
        let mut cmd = Command::new(self.server_exe(rt.variant));
        cmd.arg("-m")
            .arg(self.model_path(spec))
            .args(["--embedding", "--pooling", "last", "--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--no-webui", "--offline", "-np", "1", "-c", CONTEXT, "-b", CONTEXT, "-ub", CONTEXT])
            .args(["--cache-ram", "0", "-fa", "on", "-lv", "2", "--sleep-idle-seconds", &sleep_after]);
        match &gpu {
            Some(g) => cmd.args(["-dev", &g.id, "-ngl", "all"]),
            None => cmd.args(["-dev", "none", "-ngl", "0"]),
        };
        cmd.current_dir(self.runtime_dir(rt.variant))
            .env("LLAMA_API_KEY", &api_key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        no_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("could not start llama-server: {e}"))?;
        let pid = child.id();
        let _ = std::fs::write(self.root.join("server.pid"), pid.to_string());
        if let Some(stderr) = child.stderr.take() {
            let inner = Arc::clone(&self.inner);
            let _ = std::thread::Builder::new().name("qm-embed-log".into()).spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    push_log(&inner, line);
                }
            });
        }
        let device = if gpu.is_some() { "cuda" } else { "cpu" };
        push_log(&self.inner, format!("[engine] started pid {pid}: {} on {device} ({})", spec.id, rt.variant));
        let started = Instant::now();
        let generation = {
            let mut inner = lock(&self.inner);
            inner.generation += 1;
            inner.server = Some(Server {
                child,
                pid,
                port,
                api_key: api_key.clone(),
                signature: signature.to_string(),
                model: spec.id,
                device: device.into(),
                gpu: gpu.map(|g| g.name),
                ready: false,
                started,
                load_ms: None,
            });
            inner.last_used = Some(Instant::now());
            inner.generation
        };
        self.notify(Some(pid));
        self.wait_ready(port, started, generation).await?;
        self.watch_idle(generation);
        Ok((port, api_key))
    }

    fn count_in(&self) {
        let mut inner = lock(&self.inner);
        inner.in_flight += 1;
        inner.last_used = Some(Instant::now());
    }

    /// Poll `/health` until the model is loaded, the child exits or the start times out.
    async fn wait_ready(&self, port: u16, started: Instant, generation: u64) -> Result<(), String> {
        loop {
            if started.elapsed() > READY_TIMEOUT {
                return Err(format!("not ready after {}s", READY_TIMEOUT.as_secs()));
            }
            {
                let mut guard = lock(&self.inner);
                let inner = &mut *guard;
                if inner.generation != generation {
                    return Err("stopped while starting".into());
                }
                if let Some(s) = inner.server.as_mut() {
                    if let Ok(Some(status)) = s.child.try_wait() {
                        let tail: Vec<String> = inner.logs.iter().rev().take(4).cloned().collect();
                        return Err(format!("llama-server exited ({status}): {}", tail.join(" | ")));
                    }
                }
            }
            let health = self.local.get(format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_secs(5)).send().await;
            if health.is_ok_and(|r| r.status().is_success()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        if inner.generation != generation {
            return Err("stopped while starting".into());
        }
        let ready = inner.server.as_mut().map(|s| {
            s.ready = true;
            s.load_ms = Some(started.elapsed().as_millis());
            format!("[engine] ready after {} ms on {}", started.elapsed().as_millis(), s.device)
        });
        if let Some(line) = ready {
            inner.log(line);
        }
        inner.last_error = None;
        Ok(())
    }

    /// Stops the server once it has been idle for `idleMinutes`.
    fn watch_idle(&self, generation: u64) {
        let inner = Arc::clone(&self.inner);
        let notifier = lock(&self.notifier).clone();
        let pid_file = self.root.join("server.pid");
        let _ = std::thread::Builder::new().name("qm-embed-idle".into()).spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let mut guard = lock(&inner);
            if guard.generation != generation || guard.server.is_none() {
                return;
            }
            let idle = guard.last_used.is_some_and(|t| t.elapsed() >= guard.idle);
            if !idle || guard.in_flight > 0 {
                continue;
            }
            guard.generation += 1;
            if let Some(mut s) = guard.server.take() {
                let _ = s.child.kill();
                let _ = s.child.wait();
                let _ = std::fs::remove_file(&pid_file);
                guard.log(format!("[engine] idle: stopped pid {} to free its memory", s.pid));
            }
            drop(guard);
            if let Some(n) = &notifier {
                n(None);
            }
            return;
        });
    }
}

/// Unpack the runtime archives with the system's bsdtar (Windows 10+), keeping
/// only the server and its libraries, then swap the folder into place.
fn extract(archives: &[PathBuf], staging: &Path, final_dir: &Path, marker: &str) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let tar = if cfg!(windows) {
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    };
    for archive in archives {
        let mut cmd = Command::new(&tar);
        cmd.arg("-xf").arg(archive).arg("-C").arg(staging);
        for pattern in ["llama-server*", "*.dll", "LICENSE*"] {
            cmd.args(["--include", pattern]);
        }
        cmd.stdin(Stdio::null());
        no_window(&mut cmd);
        let out = cmd.output().map_err(|e| format!("could not run {}: {e}", tar.display()))?;
        if !out.status.success() {
            return Err(format!("unpacking {} failed: {}", archive.display(), String::from_utf8_lossy(&out.stderr).trim()));
        }
    }
    let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    if !staging.join(exe).is_file() {
        return Err(format!("{exe} is missing from the runtime archive"));
    }
    std::fs::write(staging.join(INSTALLED_MARKER), marker).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(final_dir);
    std::fs::rename(staging, final_dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_list() {
        let out = "ggml_cuda_init: found 1 CUDA devices\nAvailable devices:\n  CUDA0: NVIDIA GeForce RTX 5090 Laptop GPU (24435 MiB, 23151 MiB free)\n";
        let devices = parse_devices(out);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "CUDA0");
        assert_eq!(devices[0].name, "NVIDIA GeForce RTX 5090 Laptop GPU");
        assert_eq!((devices[0].total_mib, devices[0].free_mib), (24435, 23151));
        assert!(parse_devices("Available devices:\n  (none)\n").is_empty());
    }

    #[test]
    fn catalog_is_pinned_and_permissive() {
        for spec in MODELS {
            assert_eq!(spec.sha256.len(), 64);
            assert_eq!(spec.revision.len(), 40);
            assert_eq!(spec.license, "Apache-2.0");
            assert!(spec.file.ends_with(".gguf"));
        }
        assert!(model(DEFAULT_MODEL).is_some());
        for rt in RUNTIMES {
            for asset in rt.assets {
                assert_eq!(asset.sha256.len(), 64);
                assert!(asset.url.contains(LLAMA_BUILD));
            }
        }
    }

    #[test]
    fn nothing_installed_means_nothing_runs() {
        let root = std::env::temp_dir().join(format!("qm-engine-{}", uuid::Uuid::new_v4().simple()));
        let engine = Engine::new(root.clone());
        let config = EmbeddingConfig { enabled: true, ..EmbeddingConfig::fresh() };
        let status = engine.status(&config);
        assert!(!status.running);
        assert!(status.models.iter().all(|m| !m.installed));
        if status.supported {
            assert!(status.missing_bytes >= MODELS[0].size);
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let err = runtime.block_on(engine.embed(&config, &["x".into()])).unwrap_err();
        assert!(err.contains("not downloaded") || err.contains("Windows x64"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// Real inference in the engine folder `QS_MEMORY_ENGINE_DIR`. With
    /// `QS_MEMORY_ENGINE_INSTALL=1` it first downloads what the default setting
    /// needs (the activation path); otherwise nothing is downloaded.
    #[test]
    #[ignore = "needs QS_MEMORY_ENGINE_DIR with an installed runtime and model"]
    fn embeds_on_every_device() {
        let Some(dir) = std::env::var_os("QS_MEMORY_ENGINE_DIR") else { return };
        let engine = Engine::new(PathBuf::from(dir));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        if std::env::var("QS_MEMORY_ENGINE_INSTALL").is_ok_and(|v| v == "1") {
            let config = EmbeddingConfig { enabled: true, ..EmbeddingConfig::fresh() };
            let started = Instant::now();
            runtime.block_on(engine.install(&config)).unwrap();
            let status = engine.status(&config);
            println!("installed in {:?}: missing {} bytes, runtimes {:?}", started.elapsed(), status.missing_bytes, status.runtimes);
            assert_eq!(status.missing_bytes, 0);
        }
        let texts: Vec<String> = ["Der Hund schläft im Garten.", "The dog is sleeping in the garden.", "株価が急落した。"].map(String::from).to_vec();
        let mut by_device = Vec::new();
        for device in ["auto", "cpu"] {
            let config = EmbeddingConfig { enabled: true, device: device.into(), ..EmbeddingConfig::fresh() };
            let started = Instant::now();
            let v = runtime.block_on(engine.embed(&config, &texts)).unwrap();
            let status = engine.status(&config);
            println!("{device}: ran on {:?} ({:?}), first call {:?}, load {:?} ms", status.device, status.gpu, started.elapsed(), status.load_ms);
            assert!(cosine(&v[0], &v[1]).unwrap() > cosine(&v[0], &v[2]).unwrap());
            by_device.push(v);
        }
        let agreement = cosine(&by_device[0][0], &by_device[1][0]).unwrap();
        println!("GPU/CPU vector agreement: {agreement:.5}");
        assert!(agreement > 0.99);
        engine.stop();
        assert!(!engine.status(&EmbeddingConfig::fresh()).running);
    }
}
