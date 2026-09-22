use super::*;
use serde::Deserialize;

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
        .unwrap_or_else(|| Ok(Default::default()))
}

/// Operator-only command: deliberately NOT exposed in the MCP capability catalogue.
#[tauri::command(async)]
pub fn set_embedding_config(
    config: retrieval::EmbeddingConfig,
    state: State<'_, AppState>,
    app: AppHandle,
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
        state
            .db
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM memory_embeddings", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
        match retrieval::embed(&config, std::slice::from_ref(&request.query)).await {
            Ok(mut values) => values.pop(),
            Err(e) => {
                warning = Some(format!("{e}; using lexical retrieval."));
                None
            }
        }
    } else {
        None
    };
    // If the operator disabled or changed inference while awaiting the response, discard it.
    let current = get_embedding_config(app)?;
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
    result.elapsed_ms = started.elapsed().as_millis();
    Ok(result)
}

#[tauri::command]
pub async fn index_embeddings(
    scope: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Value, String> {
    let scopes = context_scopes(&app, Some(&scope), false, &[])?;
    let config = get_embedding_config(app.clone())?;
    if !config.enabled {
        return Err("Enable a local embedding model in Memory settings first".into());
    }
    let jobs = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        retrieval::pending(&conn, &scopes, &config, 10)?
    };
    let started = std::time::Instant::now();
    let mut indexed = 0;
    for job in jobs {
        if started.elapsed().as_secs() >= 60 {
            break;
        }
        let vectors = retrieval::embed(&config, &job.inputs).await?;
        if get_embedding_config(app.clone())? != config {
            return Err("Embedding settings changed; indexing stopped".into());
        }
        let mut conn = state.db.lock().map_err(|e| e.to_string())?;
        if retrieval::store_vectors(&mut conn, &job, &config.key(), &vectors)? {
            indexed += 1;
        }
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let remaining = !retrieval::pending(&conn, &scopes, &config, 1)?.is_empty();
    Ok(
        json!({ "indexed":indexed,"hasMore":remaining,"elapsedMs":started.elapsed().as_millis(),
        "chunkLimit":64,"charsPerChunk":1200,"cost":null }),
    )
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
