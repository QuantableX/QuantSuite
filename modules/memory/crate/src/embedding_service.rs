//! The built-in engine offered to the rest of the suite through
//! `qs_core::embeddings` — QuantMCP's semantic code index embeds with it.
//!
//! The code index uses the model, device and idle time of Memory's setting.
//! It does not need Memory's own recall switched on: an engine the operator
//! downloaded is available to both — unless the suite-wide switch
//! (`engine_off`) turns it off for both. Nothing here downloads or enables anything.
use crate::{engine, intelligence, retrieval::EmbeddingConfig};
use qs_core::embeddings::{AcquireFuture, EmbeddingInfo, EmbeddingService, Endpoint, Lease};
use tauri::{AppHandle, Manager};

/// Where the user downloads the engine.
pub const SETUP_HINT: &str = "The built-in embedding engine is not downloaded yet. Download it in Memory settings (QuantMemory → Settings → Local semantic recall).";
/// Where the user switches the engine back on.
pub const OFF_HINT: &str = "Local embeddings are switched off. Switch them on in Memory settings (QuantMemory → Settings → Local semantic recall).";

pub struct EngineService {
    app: AppHandle,
}

impl EngineService {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// The setting to run with, or why the engine cannot run (addressed to the user).
fn usable(app: &AppHandle) -> Result<EmbeddingConfig, String> {
    qs_core::apps::require_module(app, "memory").map_err(|_| {
        "QuantMemory is deactivated in Settings > Apps; semantic search uses its built-in embedding engine.".to_string()
    })?;
    if engine::RUNTIMES.is_empty() {
        return Err("The built-in embedding engine ships for Windows x64 so far; semantic search is not available on this system.".into());
    }
    let config = intelligence::get_embedding_config(app.clone())?;
    if config.engine_off {
        return Err(OFF_HINT.into());
    }
    let engine = app
        .try_state::<engine::Engine>()
        .ok_or("The built-in embedding engine is unavailable")?;
    if !engine.ready_to_run(&config) {
        return Err(SETUP_HINT.into());
    }
    Ok(config)
}

impl EmbeddingService for EngineService {
    fn info(&self) -> EmbeddingInfo {
        let config = intelligence::get_embedding_config(self.app.clone()).unwrap_or_default();
        let spec = engine::model(&config.builtin_model);
        let running = self
            .app
            .try_state::<engine::Engine>()
            .and_then(|e| e.running_on());
        let hint = usable(&self.app).err();
        EmbeddingInfo {
            installed: hint.is_none(),
            model: config.builtin_model.clone(),
            model_label: spec.map_or_else(|| config.builtin_model.clone(), |s| s.label.into()),
            dims: spec.map_or(0, |s| s.dims),
            device: config.device.clone(),
            running_device: running.as_ref().map(|(device, _)| device.clone()),
            gpu: running.and_then(|(_, gpu)| gpu),
            hint,
        }
    }

    fn acquire(&self) -> AcquireFuture {
        let app = self.app.clone();
        Box::pin(async move {
            let config = usable(&app)?;
            let spec = engine::model(&config.builtin_model).ok_or("Unknown built-in model")?;
            let engine = app
                .try_state::<engine::Engine>()
                .ok_or("The built-in embedding engine is unavailable")?;
            let (port, api_key, release) = engine.lease(&config).await?;
            let endpoint = Endpoint {
                url: format!("http://127.0.0.1:{port}"),
                api_key,
                model: spec.id.into(),
                dims: spec.dims,
                query_prefix: spec.code_query_prefix.into(),
            };
            Ok(Lease::new(endpoint, release))
        })
    }
}
