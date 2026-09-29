use super::*;
use crate::index::FileRecord;
use serde_json::{json, Value};

fn add(conn: &Connection, id: &str, scope: &str, body: &str) {
    let text = format!("---\nid: {id}\n---\n{body}");
    index::upsert_from_file(
        conn,
        &FileRecord {
            scope,
            rel_path: &format!("{id}.md"),
            text: &text,
            mtime_ms: 1,
        },
    )
    .unwrap();
}

#[test]
fn retrieval_eval_recall_isolation_citations_injection_and_restart() {
    let path = std::env::temp_dir().join(format!("qm-eval-{}.db", uuid::Uuid::new_v4()));
    let suite: Value =
        serde_json::from_str(include_str!("../tests/fixtures/retrieval.json")).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        index::init_schema(&conn).unwrap();
        for note in suite["notes"].as_array().unwrap() {
            let id = note["id"].as_str().unwrap();
            let mut fm =
                json!({"id":id,"quality":note.get("quality").cloned().unwrap_or(json!({}))});
            if fm["quality"]["reviewed"] == true {
                // compose normalizes to a trailing newline, as a real review sees it on disk.
                let body = format!("{}\n", note["body"].as_str().unwrap().trim_end());
                fm["quality"]["reviewedContentHash"] = json!(crate::quality::body_hash(&body));
            }
            let text = vault::compose(fm.as_object().unwrap(), note["body"].as_str().unwrap());
            index::upsert_from_file(
                &conn,
                &FileRecord {
                    scope: note["scope"].as_str().unwrap(),
                    rel_path: &format!("{id}.md"),
                    text: &text,
                    mtime_ms: 1,
                },
            )
            .unwrap();
        }
    }
    // Cross-session recall comes from disk, not a cached UI or process variable.
    let conn = Connection::open(&path).unwrap();
    index::init_schema(&conn).unwrap();
    let started = std::time::Instant::now();
    for case in suite["cases"].as_array().unwrap() {
        let scopes: Vec<String> = serde_json::from_value(case["scopes"].clone()).unwrap();
        let result = assemble(
            &conn,
            case["query"].as_str().unwrap(),
            &scopes,
            8,
            8000,
            None,
            case["reviewedOnly"].as_bool().unwrap_or(false),
        )
        .unwrap();
        let ids: Vec<_> = result.sources.iter().map(|s| s.id.as_str()).collect();
        for expected in case["expected"].as_array().unwrap() {
            assert!(
                ids.contains(&expected.as_str().unwrap()),
                "{}: {:?}",
                case["name"],
                ids
            );
        }
        for forbidden in case["forbidden"].as_array().unwrap() {
            assert!(
                !ids.contains(&forbidden.as_str().unwrap()),
                "{}",
                case["name"]
            );
        }
        if case["expected"].as_array().unwrap().is_empty() {
            assert!(ids.is_empty(), "{}", case["name"]);
        }
        for source in &result.sources {
            assert!(scopes.contains(&source.scope));
            assert_eq!(source.citation, format!("[memory:{}]", source.id));
            assert!(!source.path.is_empty());
            assert!(!source.updated_at.is_empty());
        }
        assert!(result.policy.starts_with("UNTRUSTED_MEMORY_DATA"));
        assert!(result.cost.is_none());
    }
    eprintln!(
        "retrieval_eval: {}",
        json!({"cases":9,"recall":1.0,"scopeLeakage":0,"citationCoverage":1.0,"elapsedMs":started.elapsed().as_millis(),"cost":null,"modelBehaviorEvaluated":false})
    );
    assert!(
        started.elapsed().as_secs() < 10,
        "Small-fixture retrieval latency regression"
    );
    drop(conn);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn hybrid_recalls_paraphrases_and_never_stale_or_foreign_vectors() {
    let mut conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    add(&conn, "a", "alpha", "# Preferences\nAvoid peanuts.");
    add(&conn, "b", "beta", "# Allergies\nPrivate medical detail.");
    let config = EmbeddingConfig {
        enabled: true,
        builtin_model: "fixture".into(),
        ..Default::default()
    };
    let jobs = pending(&conn, &["alpha".into(), "beta".into()], &config, 10).unwrap();
    for job in &jobs {
        store_vectors(&mut conn, job, &config.key(), &[vec![1.0, 0.0]]).unwrap();
    }
    let result = assemble(
        &conn,
        "food intolerance",
        &["alpha".into()],
        5,
        256,
        Some((&config, &[1.0, 0.0])),
        false,
    )
    .unwrap();
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].id, "a");
    assert!(result.sources[0].reasons.contains(&"semantic match".into()));
    add(
        &conn,
        "a",
        "alpha",
        "# Preferences\nUpdated dietary preference.",
    );
    assert!(!store_vectors(&mut conn, &jobs[0], &config.key(), &[vec![1.0, 0.0]]).unwrap());
    assert!(assemble(
        &conn,
        "food intolerance",
        &["alpha".into()],
        5,
        256,
        Some((&config, &[1.0, 0.0])),
        false
    )
    .unwrap()
    .sources
    .is_empty());
    index::remove_by_id(&conn, "b").unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn bounds_unicode_and_model_change() {
    let mut conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    add(
        &conn,
        "a",
        "general",
        &format!("# Recall\n{}", "😀 recall ".repeat(2000)),
    );
    let result = assemble(&conn, "recall", &["general".into()], 100, 256, None, false).unwrap();
    assert!(result.excerpt_chars <= 256);
    assert!(result.sources[0].truncated);
    let config = EmbeddingConfig {
        enabled: true,
        builtin_model: "a".into(),
        ..Default::default()
    };
    let job = pending(&conn, &["general".into()], &config, 1)
        .unwrap()
        .remove(0);
    store_vectors(
        &mut conn,
        &job,
        &config.key(),
        &vec![vec![1.0, 0.0]; job.inputs.len()],
    )
    .unwrap();
    let changed = EmbeddingConfig {
        builtin_model: "b".into(),
        ..config.clone()
    };
    assert!(pending(&conn, &["general".into()], &config, 1)
        .unwrap()
        .is_empty());
    assert_eq!(
        pending(&conn, &["general".into()], &changed, 1)
            .unwrap()
            .len(),
        1
    );
    assert!(cosine(&[0.0], &[0.0]).is_none());
    assert!(cosine(&[1.0], &[1.0, 2.0]).is_none());
}

#[test]
fn duplicate_review_never_merges_notes() {
    let conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    add(&conn, "a", "alpha", "# Same\nA repeated observation.");
    add(&conn, "b", "alpha", "# Same\nA repeated observation.");
    add(&conn, "c", "beta", "# Same\nA repeated observation.");
    let queue = review_queue(&conn, Some("alpha")).unwrap();
    assert_eq!(queue.len(), 2);
    assert!(queue
        .iter()
        .all(|q| q.reasons.contains(&"duplicate content".into())
            && !q.related_ids.contains(&"c".into())));
    assert_eq!(index::list(&conn, None, None, None, 100).unwrap().len(), 3);
}

#[test]
fn review_queue_proposes_consolidation_for_logs_and_near_duplicates_without_writing() {
    let mut conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    let mut log = "# Synthetic experiment log\n\nWhat the experiment is for.\n".to_string();
    for day in 1..=crate::consolidation::LOG_MIN_DATED_ENTRIES {
        log.push_str(&format!(
            "\n## 2026-01-{day:02} — run {day}\n\n{}\n",
            "measured value ".repeat(100)
        ));
    }
    add(&conn, "log", "alpha", &log);
    add(
        &conn,
        "a",
        "alpha",
        "# Cache A\nThe cache is rebuilt on start.",
    );
    add(
        &conn,
        "d",
        "alpha",
        "# Cache D\nThe cache is rebuilt on start.",
    );
    add(
        &conn,
        "b",
        "alpha",
        "# Cache B\nOn start the cache gets rebuilt.",
    );
    add(&conn, "c", "alpha", "# Unrelated\nSomething else entirely.");
    add(
        &conn,
        "x",
        "beta",
        "# Cache X\nThe cache is rebuilt on start.",
    );
    let config = EmbeddingConfig {
        enabled: true,
        builtin_model: "fixture".into(),
        ..Default::default()
    };
    for job in pending(&conn, &["alpha".into(), "beta".into()], &config, 10).unwrap() {
        let vector = match job.id.as_str() {
            "a" | "d" | "x" => vec![1.0, 0.0, 0.0],
            "b" => vec![0.95, 0.2, 0.0],
            "c" => vec![0.0, 0.0, 1.0],
            _ => vec![0.0, 1.0, 0.0],
        };
        store_vectors(
            &mut conn,
            &job,
            &config.key(),
            &vec![vector; job.inputs.len()],
        )
        .unwrap();
    }
    let snapshot = |conn: &Connection| -> Vec<(String, String, String)> {
        let mut stmt = conn
            .prepare("SELECT m.id, m.content_hash, d.body FROM memories m JOIN docs d ON d.memory_id=m.id ORDER BY m.id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.map(Result::unwrap).collect()
    };
    let before = snapshot(&conn);
    let queue = review_queue(&conn, Some("alpha")).unwrap();
    assert_eq!(snapshot(&conn), before, "the review queue never writes");
    let issue = |id: &str| queue.iter().find(|q| q.id == id).unwrap();
    // Proposals come first.
    let with: Vec<_> = queue.iter().map(|q| !q.proposals.is_empty()).collect();
    assert!(with.windows(2).all(|w| w[0] >= w[1]), "{with:?}");
    assert_eq!(with.iter().filter(|p| **p).count(), 4);
    let words = vault::word_count(&log);
    assert!(issue("log").reasons.contains(&format!(
        "log-like: 5 dated entries (2026-01-01 to 2026-01-05), {words} words"
    )));
    assert!(issue("log").proposals[0].contains("## Current state"));
    assert!(issue("log").related_ids.is_empty());
    // a and d are exact duplicates: flagged once as such, not again as near.
    let a = issue("a");
    assert_eq!(a.related_ids, vec!["d".to_string(), "b".to_string()]);
    assert!(a.reasons.contains(&"duplicate content".into()));
    assert!(a
        .reasons
        .contains(&"near-duplicate (embedding similarity 0.98)".into()));
    assert!(a.proposals[0].contains("supersededBy"));
    let b = issue("b");
    assert_eq!(b.related_ids, vec!["a".to_string(), "d".to_string()]);
    assert!(!b.reasons.contains(&"duplicate content".into()));
    // Other scopes, unrelated vectors and memories without flags stay out.
    assert!(queue.iter().all(|q| !q.related_ids.contains(&"x".into())));
    assert!(issue("c").proposals.is_empty() && issue("c").related_ids.is_empty());
    // Changed content drops its vectors, so stale similarity never flags.
    add(
        &conn,
        "b",
        "alpha",
        "# Cache B\nNow about something different.",
    );
    let queue = review_queue(&conn, Some("alpha")).unwrap();
    assert!(queue
        .iter()
        .all(|q| !q.reasons.iter().any(|r| r.starts_with("near-duplicate"))));
}

#[test]
fn recency_is_a_small_decaying_bonus_from_the_later_of_update_and_verification() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-29T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let bonus = |updated: &str, verified: Option<&str>| recency(updated, verified, now).unwrap();
    assert_eq!(
        bonus("2026-09-29T12:00:00Z", None),
        (RECENCY_MAX_BONUS, "recently updated")
    );
    let (half, _) = bonus("2026-08-30T12:00:00+00:00", None);
    assert!((half - RECENCY_MAX_BONUS / 2.0).abs() < 1e-9);
    assert!(bonus("2025-09-29T12:00:00Z", None).0 < 1e-4);
    assert_eq!(
        bonus("2025-01-01T00:00:00Z", Some("2026-09-29T12:00:00Z")),
        (RECENCY_MAX_BONUS, "recently verified")
    );
    assert_eq!(bonus("2030-01-01T00:00:00Z", None).0, RECENCY_MAX_BONUS);
    assert!(recency("not a date", None, now).is_none());
}

#[test]
fn recency_breaks_ties_but_never_beats_relevance_or_supersession() {
    let conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    let today = chrono::Utc::now().to_rfc3339();
    let put = |id: &str, updated: &str, quality: Value, body: &str| {
        let fm = json!({"id": id, "updated": updated, "quality": quality});
        let text = vault::compose(fm.as_object().unwrap(), body);
        index::upsert_from_file(
            &conn,
            &FileRecord {
                scope: "alpha",
                rel_path: &format!("{id}.md"),
                text: &text,
                mtime_ms: 1,
            },
        )
        .unwrap();
    };
    let tie = "# Cache note\nThe cache rebuilds on start.";
    put("a-old", "2020-01-01T00:00:00Z", json!({}), tie);
    put("b-new", &today, json!({}), tie);
    put("c-replaced", &today, json!({"supersededBy": "b-new"}), tie);
    put(
        "z-strong",
        "2020-01-01T00:00:00Z",
        json!({}),
        "# Cache cache\nCache: the cache, the cache and the cache.",
    );
    let result = assemble(&conn, "cache", &["alpha".into()], 8, 8000, None, false).unwrap();
    let ids: Vec<_> = result.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["z-strong", "b-new", "a-old"]);
    assert!(result.sources[1]
        .reasons
        .contains(&"recently updated".into()));
    assert!(!result.sources[2]
        .reasons
        .iter()
        .any(|r| r.starts_with("recently")));
}

#[test]
fn v2_migration_preserves_notes_and_forces_provenance_backfill() {
    let conn = Connection::open_in_memory().unwrap();
    index::init_schema(&conn).unwrap();
    add(
        &conn,
        "a",
        "general",
        "# Existing note\nDo not delete this.",
    );
    conn.execute_batch("ALTER TABLE memories DROP COLUMN quality_json; PRAGMA user_version=2;")
        .unwrap();
    index::init_schema(&conn).unwrap();
    index::init_schema(&conn).unwrap();
    assert!(index::body_of(&conn, "a")
        .unwrap()
        .unwrap()
        .contains("Do not delete"));
    let (version,hash,mtime):(i64,String,i64)=conn.query_row("SELECT (SELECT user_version FROM pragma_user_version),content_hash,mtime_ms FROM memories WHERE id='a'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(version, 3);
    assert_eq!(hash, "");
    assert_eq!(mtime, -1);
}

#[test]
fn stored_settings_from_before_the_ollama_removal_migrate_to_the_builtin_engine() {
    // Saved before the built-in engine existed: no provider, so it was Ollama.
    let pre_engine =
        json!({"enabled": true, "port": 11434, "model": "nomic-embed-text", "revision": "1"});
    let stored = StoredEmbeddingConfig::parse(pre_engine).unwrap();
    assert!(stored.legacy && stored.legacy_ollama);
    assert_eq!(stored.clone().migrate(false), EmbeddingConfig::default());
    assert!(stored.migrate(true).enabled);

    // Ollama chosen explicitly: the engine choices survive, recall stays off
    // until the engine is downloaded.
    let ollama = json!({"enabled": true, "provider": "ollama", "port": 11434, "model": "bge-m3",
        "revision": "2", "builtinModel": "qwen3-embedding-4b", "device": "cpu", "idleMinutes": 9});
    assert_eq!(
        StoredEmbeddingConfig::parse(ollama).unwrap().migrate(false),
        EmbeddingConfig {
            enabled: false,
            builtin_model: "qwen3-embedding-4b".into(),
            device: "cpu".into(),
            idle_minutes: 9,
        }
    );

    // The built-in engine saved with the old fields keeps its switch.
    let builtin = json!({"enabled": true, "provider": "builtin", "port": 11434, "model": "",
        "revision": "1", "builtinModel": "qwen3-embedding-0.6b", "device": "auto", "idleMinutes": 5});
    let stored = StoredEmbeddingConfig::parse(builtin).unwrap();
    assert!(stored.legacy && !stored.legacy_ollama);
    assert!(stored.migrate(false).enabled);

    // Today's shape round-trips and is not legacy; unknown keys are still refused.
    let current = serde_json::to_value(EmbeddingConfig {
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    let mut keys: Vec<&String> = current.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["builtinModel", "device", "enabled", "idleMinutes"]);
    let stored = StoredEmbeddingConfig::parse(current).unwrap();
    assert!(!stored.legacy && stored.config.enabled);
    assert!(StoredEmbeddingConfig::parse(json!({"enabled": true, "endpoint": "x"})).is_err());

    // Only the engine's catalog validates; the vector key names no provider port.
    assert!(EmbeddingConfig::default().validate().is_ok());
    assert!(EmbeddingConfig {
        builtin_model: "nomic-embed-text".into(),
        ..Default::default()
    }
    .validate()
    .is_err());
    assert!(EmbeddingConfig::default()
        .key()
        .starts_with("builtin:qwen3-embedding-0.6b:"));
    // An unknown stored model falls back to the default instead of failing.
    let unknown = json!({"enabled": false, "builtinModel": "gone-model"});
    assert_eq!(
        StoredEmbeddingConfig::parse(unknown).unwrap().migrate(true).builtin_model,
        engine::DEFAULT_MODEL
    );
}

#[test]
fn sections_follow_headings_and_keep_fenced_code_whole() {
    let long = "word ".repeat(80);
    let body = format!(
        "# Title\n{long}\n\n## Setup\n{long}\n\n```\n# not a heading\n```\n\n## Usage\n### Keys\n{long}\n"
    );
    let pieces = sections(&body);
    let paths: Vec<Vec<&str>> =
        pieces.iter().map(|c| c.path.iter().map(String::as_str).collect()).collect();
    assert_eq!(paths, [vec!["Title"], vec!["Title", "Setup"], vec!["Title", "Usage", "Keys"]]);
    assert!(pieces[1].text.starts_with("## Setup") && pieces[1].text.contains("# not a heading"));
    // "## Usage" alone is shorter than a section and joins the one before it.
    assert!(pieces[1].text.ends_with("## Usage"));
    assert!(pieces[2].text.starts_with("### Keys"));
}

#[test]
fn long_sections_split_at_paragraphs_and_windows() {
    let para = "a".repeat(500);
    let body = format!("## Log\n{para}\n\n{para}\n\n{para}");
    let pieces = sections(&body);
    assert_eq!(pieces.len(), 2);
    assert!(pieces.iter().all(|c| c.path == ["Log"] && c.text.chars().count() <= SECTION_CHARS));
    let huge = "b".repeat(3000);
    assert!(sections(&huge).iter().all(|c| c.text.chars().count() <= SECTION_CHARS));
}

#[test]
fn excerpts_start_at_the_section_that_matched() {
    let filler = "Background detail that does not matter here. ".repeat(12);
    let tail = "Keep the old build around for a day. ".repeat(12);
    let body = format!(
        "# Deploy notes\n{filler}\n\n## Rollback\nRestore the previous release with the rollback script, then {tail}"
    );
    assert!(best_excerpt(&body, "how do I rollback a release").starts_with("## Rollback"));
    // A semantic hit's embedded window snaps to the section around its middle.
    let window = chunks(&format!("Deploy notes\n{body}"))[1].clone();
    let snapped = section_of(&body, &window).unwrap();
    assert!(snapped.starts_with("## Rollback"), "{snapped}");
}
