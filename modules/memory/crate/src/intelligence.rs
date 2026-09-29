use super::*;
use serde::Deserialize;
use std::time::Duration;
use tauri::Emitter;

fn context_scopes(
    app: &AppHandle,
    scope: Option<&str>,
    include_general: bool,
    extra: &[String],
) -> Result<Vec<String>, String> {
    if extra.len() > 8 {
        return Err("At most 8 explicit additional scopes are allowed".into());
    }
    let mut scopes = vec![resolve_scope(app, Some(scope.unwrap_or("active")), true)?
        .ok_or("Choose a concrete workspace")?];
    if include_general {
        scopes.push(GENERAL_SCOPE.into());
    }
    for s in extra {
        scopes
            .push(resolve_scope(app, Some(s), true)?.ok_or("Additional scopes must be explicit")?);
    }
    scopes.sort();
    scopes.dedup();
    Ok(scopes)
}

#[tauri::command(async)]
pub fn get_embedding_config(app: AppHandle) -> Result<retrieval::EmbeddingConfig, String> {
    let db = app
        .try_state::<qs_core::db::Db>()
        .ok_or("Core settings unavailable")?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let value = qs_core::db::get_setting(&conn, "memory", "retrieval.embeddings")
        .map_err(|e| e.to_string())?;
    value
        .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
        .unwrap_or_else(|| Ok(retrieval::EmbeddingConfig::fresh()))
}

fn save_embedding_config(
    app: &AppHandle,
    config: &retrieval::EmbeddingConfig,
) -> Result<(), String> {
    config.validate()?;
    let previous = get_embedding_config(app.clone())?;
    let db = app
        .try_state::<qs_core::db::Db>()
        .ok_or("Core settings unavailable")?;
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    qs_core::db::set_setting(&conn, "memory", "retrieval.embeddings", &json!(config))
        .map_err(|e| e.to_string())?;
    drop(conn);
    if previous.key() != config.key() {
        app.state::<AppState>()
            .db
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM memory_embeddings", [])
            .map_err(|e| e.to_string())?;
    }
    if let Some(engine) = app.try_state::<engine::Engine>() {
        if !config.enabled || !config.is_builtin() {
            engine.stop();
        }
    }
    if let Some(indexer) = app.try_state::<Indexer>() {
        indexer.wake.notify_one();
    }
    Ok(())
}

/// Operator-only command: deliberately NOT exposed in the MCP capability catalogue.
#[tauri::command(async)]
pub fn set_embedding_config(
    config: retrieval::EmbeddingConfig,
    app: AppHandle,
) -> Result<(), String> {
    save_embedding_config(&app, &config)
}

/// Embed with whatever the setting names: the built-in engine or Ollama.
async fn embed_texts(
    app: &AppHandle,
    config: &retrieval::EmbeddingConfig,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    if config.is_builtin() {
        let engine = app
            .try_state::<engine::Engine>()
            .ok_or("The built-in embedding engine is unavailable")?;
        engine.embed(config, inputs).await
    } else {
        retrieval::embed(config, inputs).await
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextRequest {
    query: String,
    scope: Option<String>,
    #[serde(default)]
    include_general: bool,
    #[serde(default)]
    extra_scopes: Vec<String>,
    limit: Option<usize>,
    max_chars: Option<usize>,
    #[serde(default)]
    reviewed_only: bool,
}

#[tauri::command]
pub async fn memory_context(
    request: ContextRequest,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<retrieval::ContextResult, String> {
    if request.query.trim().is_empty() || request.query.len() > 2000 {
        return Err("Query must contain 1-2000 bytes".into());
    }
    let started = std::time::Instant::now();
    let scopes = context_scopes(
        &app,
        request.scope.as_deref(),
        request.include_general,
        &request.extra_scopes,
    )?;
    let config = get_embedding_config(app.clone())?;
    let mut warning = None;
    let vector = if config.enabled {
        let query = config.query_input(&request.query);
        // A cold engine loads its model first; recall does not wait for a slow start.
        match tokio::time::timeout(
            QUERY_EMBED_TIMEOUT,
            embed_texts(&app, &config, std::slice::from_ref(&query)),
        )
        .await
        {
            Ok(Ok(mut values)) => values.pop(),
            Ok(Err(e)) => {
                warning = Some(format!("{e}; using lexical retrieval."));
                None
            }
            Err(_) => {
                warning = Some(
                    "The embedding engine is still loading its model; using lexical retrieval."
                        .into(),
                );
                None
            }
        }
    } else {
        None
    };
    // If the operator disabled or changed inference while awaiting the response, discard it.
    let current = get_embedding_config(app.clone())?;
    let semantic = vector
        .as_deref()
        .filter(|_| current == config)
        .map(|v| (&config, v));
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut result = retrieval::assemble(
        &conn,
        &request.query,
        &scopes,
        request.limit.unwrap_or(8),
        request.max_chars.unwrap_or(8000),
        semantic,
        request.reviewed_only,
    )?;
    if let Some(warning) = warning {
        result.warnings.push(warning);
    }
    if result.mode == "hybrid" {
        let (device, model) = if config.is_builtin() {
            let device = app.try_state::<engine::Engine>().and_then(|e| e.device());
            (
                device.unwrap_or_else(|| "unknown".into()),
                config.builtin_model.clone(),
            )
        } else {
            ("ollama".into(), config.model.clone())
        };
        result.device = Some(device);
        result.embedding_model = Some(model);
    }
    result.elapsed_ms = started.elapsed().as_millis();
    Ok(result)
}

#[tauri::command]
pub async fn index_embeddings(scope: String, app: AppHandle) -> Result<Value, String> {
    let scopes = context_scopes(&app, Some(&scope), false, &[])?;
    let config = get_embedding_config(app.clone())?;
    if !config.enabled {
        return Err("Enable a local embedding model in Memory settings first".into());
    }
    let started = std::time::Instant::now();
    let (indexed, remaining) =
        embed_pending(&app, &config, &scopes, 10, Duration::from_secs(60)).await?;
    Ok(
        json!({ "indexed":indexed,"hasMore":remaining,"elapsedMs":started.elapsed().as_millis(),
        "chunkLimit":64,"charsPerChunk":1200,"cost":null }),
    )
}

/// Embed up to `max_jobs` changed memories of `scopes` within `budget`;
/// returns how many were stored and whether more are waiting.
async fn embed_pending(
    app: &AppHandle,
    config: &retrieval::EmbeddingConfig,
    scopes: &[String],
    max_jobs: usize,
    budget: Duration,
) -> Result<(usize, bool), String> {
    let state = app.state::<AppState>();
    let started = std::time::Instant::now();
    let mut indexed = 0;
    'batches: while indexed < max_jobs && started.elapsed() < budget {
        let jobs = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            retrieval::pending(&conn, scopes, config, (max_jobs - indexed).min(8))?
        };
        if jobs.is_empty() {
            break;
        }
        let mut stored_any = false;
        for job in jobs {
            if started.elapsed() >= budget {
                break 'batches;
            }
            let vectors = embed_texts(app, config, &job.inputs).await?;
            if get_embedding_config(app.clone())? != *config {
                return Err("Embedding settings changed; indexing stopped".into());
            }
            let mut conn = state.db.lock().map_err(|e| e.to_string())?;
            if retrieval::store_vectors(&mut conn, &job, &config.key(), &vectors)? {
                indexed += 1;
                stored_any = true;
            }
        }
        // Every job went stale under us (the files keep changing): leave it to the next pass.
        if !stored_any {
            break;
        }
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let remaining = !retrieval::pending(&conn, scopes, config, 1)?.is_empty();
    Ok((indexed, remaining))
}

const QUERY_EMBED_TIMEOUT: Duration = Duration::from_secs(30);
const INDEXER_PERIOD: Duration = Duration::from_secs(30);

/// The background indexer: new and changed memories get their vectors without
/// anyone calling index_embeddings. It never downloads anything.
#[derive(Default)]
pub struct Indexer {
    wake: tokio::sync::Notify,
    last_error: Mutex<Option<String>>,
    last_run: Mutex<Option<Value>>,
}

/// Manage the engine and the indexer, register the process, start the loop.
pub fn init_embeddings(app: &AppHandle) {
    let engine = engine::Engine::new(qs_core::paths::module_dir("memory").join("engine"));
    let handle = app.clone();
    engine.set_notifier(Arc::new(move |pid| match pid {
        Some(pid) => qs_core::processes::mark_running(&handle, engine::PROCESS_ID, Some(pid)),
        None => qs_core::processes::mark_stopped(&handle, engine::PROCESS_ID),
    }));
    app.manage(engine);
    app.manage(Indexer::default());
    qs_core::processes::announce(
        app,
        qs_core::processes::ProcessInfo::new(
            engine::PROCESS_ID,
            "memory",
            "QuantMemory — embedding engine",
        )
        .stop_with("plugin:memory|embedding_engine_stop")
        .logs_with("plugin:memory|embedding_engine_logs"),
    );
    let handle = app.clone();
    qs_core::shutdown::on_shutdown(
        app,
        "memory:embeddings",
        Box::new(move || {
            if let Some(engine) = handle.try_state::<engine::Engine>() {
                engine.stop();
            }
        }),
    );
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { background_indexer(handle).await });
}

fn all_scopes(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let Ok(conn) = state.db.lock() else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare("SELECT DISTINCT scope FROM memories ORDER BY scope") else {
        return Vec::new();
    };
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

async fn background_indexer(app: AppHandle) {
    loop {
        {
            let indexer = app.state::<Indexer>();
            let _ = tokio::time::timeout(INDEXER_PERIOD, indexer.wake.notified()).await;
        }
        let Ok(config) = get_embedding_config(app.clone()) else {
            continue;
        };
        if !config.enabled {
            continue;
        }
        // The built-in engine runs only once the operator downloaded it.
        if config.is_builtin() && app.state::<engine::Engine>().status(&config).missing_bytes > 0 {
            continue;
        }
        let scopes = all_scopes(&app);
        let started = std::time::Instant::now();
        let outcome = embed_pending(&app, &config, &scopes, 400, Duration::from_secs(600)).await;
        let indexer = app.state::<Indexer>();
        match outcome {
            Ok((indexed, more)) => {
                *lock_or_recover(&indexer.last_error) = None;
                if indexed > 0 {
                    *lock_or_recover(&indexer.last_run) = Some(json!({
                        "indexed": indexed,
                        "elapsedMs": started.elapsed().as_millis(),
                        "at": now_iso(),
                    }));
                }
                if more && indexed > 0 {
                    indexer.wake.notify_one();
                }
            }
            Err(e) => {
                log::warn!("memory embeddings: {e}");
                *lock_or_recover(&indexer.last_error) = Some(e);
                // A failing engine or service is not retried in a tight loop.
                tokio::time::sleep(Duration::from_secs(120)).await;
            }
        }
    }
}

fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Engine, download and indexer state for the Memory settings (operator-only).
/// `draft` is the form's unsaved selection: what it would still download.
#[tauri::command(async)]
pub fn embedding_engine_status(
    app: AppHandle,
    draft: Option<retrieval::EmbeddingConfig>,
) -> Result<Value, String> {
    let config = get_embedding_config(app.clone())?;
    let engine = app.state::<engine::Engine>();
    let indexer = app.state::<Indexer>();
    let scopes = all_scopes(&app);
    let pending = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        retrieval::pending_count(&conn, &scopes, &config)?
    };
    let last_error = lock_or_recover(&indexer.last_error).clone();
    let last_run = lock_or_recover(&indexer.last_run).clone();
    Ok(json!({
        "engine": engine.status(draft.as_ref().unwrap_or(&config)),
        "config": config,
        "pending": pending,
        "indexerError": last_error,
        "lastRun": last_run,
    }))
}

/// The operator's activation click: download the runtime and model the setting
/// needs, then enable it. Operator-only — never in the MCP capability catalogue.
#[tauri::command(async)]
pub fn embedding_engine_setup(
    model: String,
    device: String,
    idle_minutes: Option<u32>,
    app: AppHandle,
) -> Result<(), String> {
    let mut config = get_embedding_config(app.clone())?;
    config.provider = "builtin".into();
    config.builtin_model = model;
    config.device = device;
    if let Some(minutes) = idle_minutes {
        config.idle_minutes = minutes;
    }
    config.validate()?;
    if app.state::<engine::Engine>().status(&config).setup.busy {
        return Err("A download is already running".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = handle.state::<engine::Engine>();
        let outcome = match engine.install(&config).await {
            Ok(()) => {
                config.enabled = true;
                save_embedding_config(&handle, &config)
            }
            Err(e) => Err(e),
        };
        let _ = handle.emit(
            "memory-embedding-setup",
            json!({ "ok": outcome.is_ok(), "error": outcome.err() }),
        );
    });
    Ok(())
}

/// Stop the built-in engine now (the process page's Stop); it frees its RAM and VRAM.
#[tauri::command(async)]
pub fn embedding_engine_stop(app: AppHandle) -> Result<(), String> {
    app.state::<engine::Engine>().stop();
    Ok(())
}

#[tauri::command(async)]
pub fn embedding_engine_logs(app: AppHandle, limit: Option<usize>) -> Vec<String> {
    app.state::<engine::Engine>().logs(limit.unwrap_or(200))
}

fn write_quality(
    identifier: &str,
    mut quality: quality::Quality,
    revision: &str,
    reviewed: bool,
    state: &AppState,
    app: &AppHandle,
) -> Result<MemoryMeta, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let meta = require_meta(&conn, identifier, None)?;
    quality.reviewed = reviewed;
    quality.reviewed_content_hash = None;
    quality.validate(&meta.id)?;
    for related in quality
        .conflicts_with
        .iter()
        .chain(quality.superseded_by.iter())
    {
        let target = require_meta(&conn, related, Some(&meta.scope))?;
        if target.id != *related || (target.scope != meta.scope && target.scope != GENERAL_SCOPE) {
            return Err("Quality links must use exact IDs from this workspace or General".into());
        }
        if quality.superseded_by.as_ref() == Some(related) && target.quality.superseded_by.is_some()
        {
            return Err("Choose a current, non-superseded replacement".into());
        }
    }
    let vault = scope_vault_dir(state, &meta.scope)?;
    let abs = vault.join(&meta.rel_path);
    let text = read_vault_file(&abs)?;
    if format!("{:016x}", vault::fnv1a64(&text)) != revision {
        return Err("Memory changed. Reload before reviewing it.".into());
    }
    let (yaml, body) = vault::split_frontmatter(&text);
    let mut fm = frontmatter_for_write(
        yaml.and_then(vault::parse_frontmatter),
        &meta.id,
        &meta.created_at,
    );
    if reviewed {
        quality.last_verified = Some(now_iso());
        quality.reviewed_content_hash = Some(quality::body_hash(body));
    }
    fm.insert("quality".into(), json!(quality));
    write_vault_file(state, &abs, &vault::compose(&fm, body))?;
    index_file(&conn, &vault, &abs, &meta.scope)?;
    let saved = require_meta(&conn, &meta.id, None)?;
    drop(conn);
    mirror(&saved);
    publish(
        app,
        "memory.note.updated",
        json!({"id":saved.id,"scope":saved.scope}),
    );
    Ok(saved)
}

#[tauri::command(async)]
pub fn set_memory_quality(
    identifier: String,
    quality: quality::Quality,
    revision: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    write_quality(&identifier, quality, &revision, false, &state, &app)
}

/// Human review is a separate UI operation, never an agent capability.
#[tauri::command(async)]
pub fn review_memory_quality(
    identifier: String,
    quality: quality::Quality,
    revision: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MemoryMeta, String> {
    write_quality(&identifier, quality, &revision, true, &state, &app)
}

#[tauri::command(async)]
pub fn get_memory_review_queue(
    scope: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<retrieval::ReviewIssue>, String> {
    let scope = resolve_scope(&app, scope.as_deref(), false)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    retrieval::review_queue(&conn, scope.as_deref())
}
