//! Opt-in acceptance test: real code -> Python CLI -> leased GPU engine.
use super::*;
use serde_json::{json, Value};

#[test]
#[ignore = "needs QS_MEMORY_ENGINE_DIR and QS_CODE_INDEX_{PYTHON,SOURCE,OUTPUT}; uses the installed test engine"]
fn semantic_code_index_uses_the_leased_engine() {
    let engine = Engine::new(PathBuf::from(std::env::var_os("QS_MEMORY_ENGINE_DIR").expect("test engine")));
    struct Stop<'a>(&'a Engine);
    impl Drop for Stop<'_> { fn drop(&mut self) { self.0.stop(); } }
    let _stop = Stop(&engine);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let config = EmbeddingConfig { idle_minutes: 1, ..Default::default() };
    let (port, key, release) = runtime.block_on(engine.lease(&config)).unwrap();
    let device = engine.running_on().unwrap().0;
    println!("Code index engine: {device}");
    assert_eq!(device, "cuda", "this opt-in acceptance test expects the NVIDIA test machine");
    let (_, _, cancelled) = runtime.block_on(engine.lease(&config)).unwrap();
    drop(cancelled);
    assert_eq!(lock(&engine.inner).in_flight, 1, "dropping a caller must release its hold");
    let cpu = EmbeddingConfig { device: "cpu".into(), ..config.clone() };
    assert!(runtime.block_on(engine.lease(&cpu)).err().unwrap().contains("busy"));
    engine.stop_if_unused();
    assert!(engine.status(&config).running, "disabling recall must preserve the code index lease");

    let python = std::env::var_os("QS_CODE_INDEX_PYTHON").expect("Python with index dependencies");
    let source = std::env::var("QS_CODE_INDEX_SOURCE").expect("read-only repo directory to index");
    let output = std::env::var_os("QS_CODE_INDEX_OUTPUT").expect("isolated index directory");
    let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../sidecars/python/codebase_index/cli.py");
    let spec = model(&config.builtin_model).unwrap();
    let endpoint = json!({"url": format!("http://127.0.0.1:{port}"), "model": spec.id,
        "dims": spec.dims, "queryPrefix": spec.code_query_prefix}).to_string();
    let run = |args: &[&str]| -> Value {
        let mut cmd = Command::new(&python);
        cmd.arg(&cli).args(args).env("QUANTMCP_INDEX_DIR", &output)
            .env("QUANTMCP_EMBEDDING_ENDPOINT", &endpoint).env("QUANTMCP_EMBEDDING_KEY", &key);
        no_window(&mut cmd);
        let result = cmd.output().unwrap();
        assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        serde_json::from_slice(&result.stdout).unwrap()
    };
    let began = Instant::now();
    let indexed = run(&["index", &source, "--name", "gpu-acceptance", "--mode", "both"]);
    assert_eq!(indexed["errors"], 0);
    println!("Indexed {} files / {} semantic chunks in {:?}", indexed["files_indexed"], indexed["semantic_entries"], began.elapsed());
    let found = run(&["search", "keep embedding engine alive until caller finishes", "--codebase", "gpu-acceptance", "--mode", "semantic", "--limit", "3"]);
    assert_eq!(found["mode"], "semantic");
    assert!(found["count"].as_u64().unwrap() > 0);
    println!("Semantic search returned {} results", found["count"]);
    let reindexed = run(&["reindex", "--codebase", "gpu-acceptance", "--mode", "semantic"]);
    assert_eq!(reindexed["errors"], 0);
    assert!(reindexed["semantic_entries"].as_u64().unwrap() > 0);
    // Idle time starts after the lease is released, even after a long index.
    std::thread::sleep(Duration::from_secs(80));
    assert!(engine.status(&config).running);
    release();
    let released = Instant::now();
    while engine.status(&config).running && released.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_secs(1));
    }
    assert!(!engine.status(&config).running);
    println!("Engine survived 80 seconds under lease, stopped {:?} after release", released.elapsed());
}
