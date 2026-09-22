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
        model: "fixture".into(),
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
        model: "a".into(),
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
        model: "b".into(),
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

fn local_embedding_stub(status: &str, body: &str) -> (u16, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let response=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: http://192.0.2.1/never-follow\r\nConnection: close\r\n\r\n{body}",body.len());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        let mut buffer = [0u8; 4096];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&request);
            if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                let length = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= length {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (port, handle)
}

#[tokio::test]
async fn embedding_transport_is_explicit_local_and_rejects_redirects_or_invalid_vectors() {
    assert!(
        embed(&EmbeddingConfig::default(), &["synthetic fixture".into()])
            .await
            .unwrap_err()
            .contains("disabled")
    );
    let (port, server) = local_embedding_stub("200 OK", r#"{"embeddings":[[1.0,0.0]]}"#);
    let mut config = EmbeddingConfig {
        enabled: true,
        port,
        model: "fixture-only".into(),
        ..Default::default()
    };
    assert_eq!(
        embed(&config, &["synthetic fixture".into()]).await.unwrap(),
        vec![vec![1.0, 0.0]]
    );
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /api/embed "));
    let payload: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(payload["input"], json!(["synthetic fixture"]));
    assert_eq!(payload["truncate"], false);
    let (port, server) = local_embedding_stub("302 Found", r#"{"embeddings":[[1.0,0.0]]}"#);
    config.port = port;
    assert!(embed(&config, &["fixture".into()])
        .await
        .unwrap_err()
        .contains("redirects"));
    server.join().unwrap();
    let (port, server) = local_embedding_stub("200 OK", r#"{"embeddings":[[0.0,0.0]]}"#);
    config.port = port;
    assert!(embed(&config, &["fixture".into()])
        .await
        .unwrap_err()
        .contains("invalid"));
    server.join().unwrap();
}
