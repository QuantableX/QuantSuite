//! Retrieval quality benchmark: recall@1, recall@5 and MRR per mode and query language.
//!
//! `cargo test -p tauri-plugin-memory --lib retrieval::bench -- --include-ignored --nocapture`
//! prints the metrics for the synthetic multilingual fixture (tests/fixtures/retrieval_bench.json,
//! which also fails the build below its floors) and, when `QS_MEMORY_BENCH` names a set file,
//! for a private set against vaults on disk. Vaults are only read: they are indexed into a
//! temporary database. The repo is public, so private sets live in the suite's private
//! workspace data, never here.
//!
//! A set file: `vaults` (scope → vault folder, `~` allowed) and/or synthetic `notes`,
//! `defaultScopes`, `cases` (`query`, `lang`, optional `cross` = query language differs from
//! the memory's, `scopes`, `expected` ids or titles), optional `embedding` (an
//! [`EmbeddingConfig`]; adds the hybrid mode) and optional `floors` per mode.
use super::*;
use crate::index::FileRecord;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

type Embedder<'a> = &'a dyn Fn(&[String]) -> Result<Vec<Vec<f32>>, String>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchSet {
    #[serde(default)]
    vaults: BTreeMap<String, String>,
    #[serde(default)]
    notes: Vec<Value>,
    #[serde(default)]
    default_scopes: Vec<String>,
    cases: Vec<Case>,
    #[serde(default)]
    embedding: Option<EmbeddingConfig>,
    #[serde(default)]
    floors: BTreeMap<String, Metrics>,
}

#[derive(Deserialize)]
struct Case {
    query: String,
    lang: String,
    #[serde(default)]
    cross: bool,
    #[serde(default)]
    scopes: Vec<String>,
    expected: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    recall1: f64,
    recall5: f64,
    mrr: f64,
}

struct Outcome {
    lang: String,
    cross: bool,
    query: String,
    expected: Vec<String>,
    got: Vec<String>,
}

impl Outcome {
    fn rank(&self) -> Option<usize> {
        self.got
            .iter()
            .position(|id| self.expected.contains(id))
            .map(|i| i + 1)
    }
    fn recall(&self, k: usize) -> f64 {
        let hits = self
            .expected
            .iter()
            .filter(|e| self.got.iter().take(k).any(|g| g == *e));
        hits.count() as f64 / self.expected.len() as f64
    }
}

fn metrics<'a>(outcomes: impl Iterator<Item = &'a Outcome>) -> (usize, Metrics) {
    let (mut n, mut m) = (0, Metrics::default());
    for o in outcomes {
        n += 1;
        m.recall1 += f64::from(u8::from(o.rank() == Some(1)));
        m.recall5 += o.recall(5);
        m.mrr += o.rank().map_or(0.0, |r| 1.0 / r as f64);
    }
    if n > 0 {
        for v in [&mut m.recall1, &mut m.recall5, &mut m.mrr] {
            *v /= n as f64;
        }
    }
    (n, m)
}

fn load(path: &std::path::Path) -> BenchSet {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(path),
    }
}

/// A fresh temporary index holding the set's synthetic notes and vault folders.
fn build(set: &BenchSet) -> (PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!("qm-bench-{}.db", uuid::Uuid::new_v4()));
    let conn = Connection::open(&path).unwrap();
    index::init_schema(&conn).unwrap();
    for note in &set.notes {
        let id = note["id"].as_str().unwrap();
        let fm = json!({"id": id, "quality": note.get("quality").cloned().unwrap_or(json!({}))});
        let text = vault::compose(fm.as_object().unwrap(), note["body"].as_str().unwrap());
        let file = FileRecord {
            scope: note["scope"].as_str().unwrap(),
            rel_path: &format!("{id}.md"),
            text: &text,
            mtime_ms: 1,
        };
        index::upsert_from_file(&conn, &file).unwrap();
    }
    for (scope, root) in &set.vaults {
        let root = expand(root);
        assert!(
            root.is_dir(),
            "vault folder for {scope} not found: {}",
            root.display()
        );
        index::scan_vault(&conn, &root, scope).unwrap();
    }
    index::resolve_links(&conn).unwrap();
    (path, conn)
}

fn scopes_of<'a>(set: &'a BenchSet, case: &'a Case) -> &'a [String] {
    if case.scopes.is_empty() {
        &set.default_scopes
    } else {
        &case.scopes
    }
}

/// Expected entries are ids or titles; a title resolves inside the case's scopes.
fn resolve(conn: &Connection, scopes: &[String], wanted: &str) -> String {
    let found = scopes
        .iter()
        .find_map(|scope| index::get_by_identifier(conn, wanted, Some(scope)).unwrap())
        .filter(|meta| scopes.contains(&meta.scope));
    found
        .unwrap_or_else(|| panic!("expected memory not found in {scopes:?}: {wanted}"))
        .id
}

fn embed_all(
    conn: &mut Connection,
    scopes: &[String],
    config: &EmbeddingConfig,
    embed: Embedder,
) -> usize {
    let mut stored = 0;
    loop {
        let jobs = pending(conn, scopes, config, 32).unwrap();
        if jobs.is_empty() {
            return stored;
        }
        for job in jobs {
            let vectors = embed(&job.inputs).unwrap();
            assert!(store_vectors(conn, &job, &config.key(), &vectors).unwrap());
            stored += 1;
        }
    }
}

fn run(
    conn: &Connection,
    set: &BenchSet,
    semantic: Option<(&EmbeddingConfig, Embedder)>,
) -> Vec<Outcome> {
    set.cases
        .iter()
        .map(|case| {
            let scopes = scopes_of(set, case);
            let vector = semantic.map(|(config, embed)| {
                embed(&[config.query_input(&case.query)]).unwrap().remove(0)
            });
            let config = semantic.map(|(config, _)| config);
            let result = assemble(
                conn,
                &case.query,
                scopes,
                10,
                24000,
                config.zip(vector.as_deref()),
                false,
            )
            .unwrap();
            Outcome {
                lang: case.lang.clone(),
                cross: case.cross,
                query: case.query.clone(),
                expected: case
                    .expected
                    .iter()
                    .map(|e| resolve(conn, scopes, e))
                    .collect(),
                got: result.sources.into_iter().map(|s| s.id).collect(),
            }
        })
        .collect()
}

fn title_of(conn: &Connection, id: &str) -> String {
    index::get_by_identifier(conn, id, None)
        .unwrap()
        .map_or_else(|| id.to_string(), |m| m.title)
}

/// Prints the table and the misses; returns the overall metrics.
fn report(conn: &Connection, mode: &str, outcomes: &[Outcome]) -> Metrics {
    let row = |group: &str, (n, m): (usize, Metrics)| {
        if n > 0 {
            println!(
                "{mode:<8} {group:<11} {n:>3}  {:>5.2}  {:>5.2}  {:>5.2}",
                m.recall1, m.recall5, m.mrr
            );
        }
    };
    println!(
        "{:<8} {:<11} {:>3}  {:>5}  {:>5}  {:>5}",
        "mode", "group", "n", "R@1", "R@5", "MRR"
    );
    let all = metrics(outcomes.iter());
    row("all", all);
    row("same-lang", metrics(outcomes.iter().filter(|o| !o.cross)));
    row("cross-lang", metrics(outcomes.iter().filter(|o| o.cross)));
    let langs: std::collections::BTreeSet<_> = outcomes.iter().map(|o| o.lang.as_str()).collect();
    for lang in langs {
        row(
            &format!("lang={lang}"),
            metrics(outcomes.iter().filter(|o| o.lang == lang)),
        );
    }
    for o in outcomes.iter().filter(|o| o.rank() != Some(1)) {
        let top: Vec<_> = o.got.iter().take(3).map(|id| title_of(conn, id)).collect();
        println!(
            "  miss [{}] {:?} rank {} — want {:?}, top {:?}",
            o.lang,
            o.query,
            o.rank().map_or("-".into(), |r| r.to_string()),
            o.expected
                .iter()
                .map(|id| title_of(conn, id))
                .collect::<Vec<_>>(),
            top
        );
    }
    all.1
}

/// Runs lexical, plus hybrid when an embedder is given; checks the set's floors.
fn bench(
    set: &BenchSet,
    embedder: Option<(&EmbeddingConfig, Embedder)>,
) -> BTreeMap<String, Metrics> {
    let (path, mut conn) = build(set);
    let mut results = BTreeMap::new();
    results.insert(
        "lexical".to_string(),
        report(&conn, "lexical", &run(&conn, set, None)),
    );
    if let Some((config, embed)) = embedder {
        let mut scopes: Vec<String> = set
            .cases
            .iter()
            .flat_map(|c| scopes_of(set, c).to_vec())
            .collect();
        scopes.sort();
        scopes.dedup();
        let started = std::time::Instant::now();
        let stored = embed_all(&mut conn, &scopes, config, embed);
        println!("embedded {stored} memories in {:?}", started.elapsed());
        results.insert(
            "hybrid".to_string(),
            report(&conn, "hybrid", &run(&conn, set, Some((config, embed)))),
        );
    }
    drop(conn);
    let _ = std::fs::remove_file(path);
    for (mode, floor) in &set.floors {
        if let Some(got) = results.get(mode) {
            assert!(
                got.recall1 >= floor.recall1
                    && got.recall5 >= floor.recall5
                    && got.mrr >= floor.mrr,
                "{mode} fell below its floor: {got:?} < {floor:?}"
            );
        }
    }
    results
}

/// Deterministic stand-in for a model: hashed character trigrams. Exercises the hybrid
/// plumbing without a local inference service; it measures nothing about real models.
fn trigram_embed(inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
    Ok(inputs
        .iter()
        .map(|text| {
            let chars: Vec<char> = text.to_lowercase().chars().collect();
            let mut v = vec![0f32; 256];
            for w in chars.windows(3) {
                let h = vault::fnv1a64(&w.iter().collect::<String>());
                v[(h % 256) as usize] += 1.0;
            }
            v[0] += 1e-3;
            v
        })
        .collect())
}

#[test]
fn fixture_meets_floors() {
    let set: BenchSet =
        serde_json::from_str(include_str!("../tests/fixtures/retrieval_bench.json")).unwrap();
    let fake = EmbeddingConfig {
        enabled: true,
        model: "bench-trigram".into(),
        ..Default::default()
    };
    let results = bench(&set, Some((&fake, &trigram_embed)));
    assert!(results.contains_key("hybrid"));
}

#[test]
#[ignore = "needs QS_MEMORY_BENCH=<set.json>; reads local vaults"]
fn private_set() {
    let Some(path) = std::env::var_os("QS_MEMORY_BENCH") else {
        println!("QS_MEMORY_BENCH is not set; no private set to run");
        return;
    };
    let set = load(std::path::Path::new(&path));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    // QS_MEMORY_ENGINE_DIR runs the hybrid mode through the built-in engine installed
    // there (QS_MEMORY_BENCH_MODEL / QS_MEMORY_BENCH_DEVICE pick model and device).
    let engine_dir = std::env::var_os("QS_MEMORY_ENGINE_DIR");
    let real = match &engine_dir {
        Some(_) => {
            let env = |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.into());
            Some(EmbeddingConfig {
                enabled: true,
                builtin_model: env("QS_MEMORY_BENCH_MODEL", engine::DEFAULT_MODEL),
                device: env("QS_MEMORY_BENCH_DEVICE", "auto"),
                ..EmbeddingConfig::fresh()
            })
        }
        None => set.embedding.clone().filter(|c| c.enabled),
    };
    let engine = engine::Engine::new(engine_dir.map_or_else(
        || qs_core::paths::module_dir("memory").join("engine"),
        PathBuf::from,
    ));
    let real_embed = |inputs: &[String]| {
        let config = real.as_ref().unwrap();
        if config.is_builtin() {
            runtime.block_on(engine.embed(config, inputs))
        } else {
            runtime.block_on(embed(config, inputs))
        }
    };
    bench(&set, real.as_ref().map(|c| (c, &real_embed as Embedder)));
    if let Some(config) = &real {
        let status = engine.status(config);
        println!("engine: model {:?} on {:?} ({:?}), load {:?} ms", status.model, status.device, status.gpu, status.load_ms);
    }
}
