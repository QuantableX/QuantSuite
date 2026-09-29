//! The suite's embedding service — one seam between the module that runs an
//! embedding model and the modules that need vectors, without either of them
//! depending on the other (modules never import each other; ARCHITECTURE.md §1).
//!
//! QuantMemory registers its built-in llama.cpp engine here during setup.
//! QuantMCP's code index asks it for an endpoint whenever a semantic index,
//! reindex or search runs. The endpoint is the engine's OpenAI-compatible
//! `/v1/embeddings` on 127.0.0.1 with a per-start API key; both change with
//! every start of the engine, so a caller asks again for every run and never
//! stores them. The [`Lease`] that carries the endpoint keeps the engine from
//! idling out until it is dropped — an index run can take minutes, and the
//! subprocess talking to the engine is invisible to the engine's idle watch.
//!
//! Nothing here starts a download or enables anything: an engine the operator
//! has not downloaded reports `installed: false` with a hint for the user.

use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

/// What a caller shows before it asks for an endpoint (the code index card).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingInfo {
    /// Runtime and model are on disk: [`acquire`] can start the engine.
    pub installed: bool,
    /// The model id vectors are tagged with, e.g. `qwen3-embedding-0.6b`.
    pub model: String,
    pub model_label: String,
    pub dims: usize,
    /// The device setting: `auto`, `gpu` or `cpu`.
    pub device: String,
    /// What a running engine uses right now (`cuda` or `cpu`), if one runs.
    pub running_device: Option<String>,
    /// The GPU a running engine offloads to.
    pub gpu: Option<String>,
    /// Why the engine cannot run, addressed to the user. `None` when installed.
    pub hint: Option<String>,
}

/// Where to send embedding requests for the lifetime of one [`Lease`].
#[derive(Clone, PartialEq)]
pub struct Endpoint {
    /// `http://127.0.0.1:<port>` — requests go to `<url>/v1/embeddings`.
    pub url: String,
    /// Bearer token of this start of the engine. Hand it to a child process
    /// through its environment only — never argv, logs or disk.
    pub api_key: String,
    pub model: String,
    pub dims: usize,
    /// Prepended to search queries (instruction-aware models); indexed
    /// documents are embedded as they are.
    pub query_prefix: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.url)
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("dims", &self.dims)
            .field("query_prefix", &self.query_prefix)
            .finish()
    }
}

type Release = Box<dyn FnOnce() + Send + Sync>;

/// A running engine held for one caller. Dropping it lets the engine idle out
/// again (after its configured idle time, not at once).
pub struct Lease {
    endpoint: Endpoint,
    release: Option<Release>,
}

impl Lease {
    /// Built by the service; `release` runs exactly once, on drop.
    pub fn new(endpoint: Endpoint, release: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self {
            endpoint,
            release: Some(Box::new(release)),
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease").field("endpoint", &self.endpoint).finish()
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

pub type AcquireFuture = Pin<Box<dyn Future<Output = Result<Lease, String>> + Send>>;

/// Implemented by the module that owns the engine.
pub trait EmbeddingService: Send + Sync {
    /// Cheap: reads settings and files, never starts the engine.
    fn info(&self) -> EmbeddingInfo;
    /// Start the engine if needed and hold it until the lease drops.
    fn acquire(&self) -> AcquireFuture;
}

/// The message for a suite without a registered service.
pub const NOT_LOADED: &str =
    "Semantic search needs QuantMemory's built-in embedding engine, and QuantMemory is not loaded in this build.";

static SERVICE: RwLock<Option<Arc<dyn EmbeddingService>>> = RwLock::new(None);

/// Called once by the owning module's setup. A second registration replaces the first.
pub fn register(service: Arc<dyn EmbeddingService>) {
    *SERVICE.write().unwrap_or_else(|p| p.into_inner()) = Some(service);
}

fn service() -> Option<Arc<dyn EmbeddingService>> {
    SERVICE.read().unwrap_or_else(|p| p.into_inner()).clone()
}

/// The engine's state for display; `installed: false` with a hint when absent.
pub fn info() -> EmbeddingInfo {
    service().map_or_else(
        || EmbeddingInfo {
            hint: Some(NOT_LOADED.into()),
            ..Default::default()
        },
        |s| s.info(),
    )
}

/// A ready endpoint plus the lease that keeps it up. Errors are user-facing.
pub async fn acquire() -> Result<Lease, String> {
    match service() {
        Some(s) => s.acquire().await,
        None => Err(NOT_LOADED.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        held: Arc<AtomicUsize>,
    }

    impl EmbeddingService for Fake {
        fn info(&self) -> EmbeddingInfo {
            EmbeddingInfo {
                installed: true,
                model: "fixture".into(),
                dims: 2,
                ..Default::default()
            }
        }
        fn acquire(&self) -> AcquireFuture {
            let held = Arc::clone(&self.held);
            Box::pin(async move {
                held.fetch_add(1, Ordering::SeqCst);
                let endpoint = Endpoint {
                    url: "http://127.0.0.1:9".into(),
                    api_key: "secret-key".into(),
                    model: "fixture".into(),
                    dims: 2,
                    query_prefix: "Query:".into(),
                };
                Ok(Lease::new(endpoint, move || {
                    held.fetch_sub(1, Ordering::SeqCst);
                }))
            })
        }
    }

    // One test: the registry is process-global, so a second test would race it.
    #[test]
    fn a_lease_holds_until_dropped_and_never_prints_its_key() {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        *SERVICE.write().unwrap() = None;
        assert!(!info().installed);
        assert_eq!(info().hint.as_deref(), Some(NOT_LOADED));
        assert_eq!(runtime.block_on(acquire()).unwrap_err(), NOT_LOADED);

        let held = Arc::new(AtomicUsize::new(0));
        register(Arc::new(Fake { held: Arc::clone(&held) }));
        assert!(info().installed);
        let lease = runtime.block_on(acquire()).unwrap();
        let second = runtime.block_on(acquire()).unwrap();
        assert_eq!(held.load(Ordering::SeqCst), 2);
        assert_eq!(lease.endpoint().dims, 2);
        let printed = format!("{lease:?}");
        assert!(printed.contains("<redacted>") && !printed.contains("secret-key"));
        drop(lease);
        assert_eq!(held.load(Ordering::SeqCst), 1);
        drop(second);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        *SERVICE.write().unwrap() = None;
    }
}
