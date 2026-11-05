//! The gateway between two processes on this PC (card Server 4/7
//! acceptance): the server is `gateway-testserver` (src/bin), this test is the
//! client. Calls of every class and a denied one, a backtest streaming its
//! progress, a disconnect mid-stream resumed without lost or doubled events, a
//! timed-out write resolved by its idempotency key, files browsed and moved
//! with interrupted, resumed transfers, the heartbeat, the rate limit, cancel.
//!
//! The 1 GB transfer runs with `cargo test -p qs-gateway --test two_process
//! -- --ignored one_gigabyte`; `latency` prints the per-call cost.

use qs_gateway::client::{self, CallOptions, ClientEvent, ClientInfo, ConnectOptions, Session};
use qs_gateway::protocol::{EventKind, ResumePoint};
use qs_gateway::Identity;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    addr: String,
    key: [u8; 32],
    code: String,
    manifest: String,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn server(name: &str, env: &[(&str, &str)]) -> Server {
    let dir = std::env::temp_dir().join(format!("qs-gateway-2p-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gateway-testserver"));
    cmd.arg(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("the test server starts");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let v: Value = serde_json::from_str(&line).expect("its first line is JSON");
    let key = hex::decode(v["key"].as_str().unwrap()).unwrap().try_into().unwrap();
    Server {
        addr: v["addr"].as_str().unwrap().into(),
        key,
        code: v["code"].as_str().unwrap().into(),
        manifest: v["manifest"].as_str().unwrap().into(),
        child,
        dir,
    }
}

fn me(s: &Server) -> ClientInfo {
    ClientInfo { name: "Laptop".into(), app: "test".into(), manifest: s.manifest.clone() }
}

async fn paired(s: &Server) -> Identity {
    let device = Identity::generate().unwrap();
    let attempt = client::pair(&s.addr, &device, &s.code, &me(s)).await.unwrap();
    assert_eq!(attempt.server_key, s.key);
    attempt.finish().await.unwrap();
    device
}

async fn connect(s: &Server, device: &Identity, opts: ConnectOptions) -> Session {
    client::connect_with(&s.addr, device, s.key, &me(s), opts).await.unwrap()
}

/// Pseudo-random bytes, quick to make in any size.
fn random_file(path: &Path, size: u64, seed: u64) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut x = seed | 1;
    let mut buf = vec![0u8; 1 << 20];
    let mut left = size;
    while left > 0 {
        for c in buf.chunks_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            c.copy_from_slice(&x.to_le_bytes()[..c.len()]);
        }
        let n = left.min(buf.len() as u64) as usize;
        f.write_all(&buf[..n]).unwrap();
        left -= n as u64;
    }
}

fn sha(path: &Path) -> String {
    qs_gateway::files::sha256_file(path).unwrap()
}

#[tokio::test]
async fn calls_of_every_class_and_a_denied_one() {
    let s = server("classes", &[]);
    let device = paired(&s).await;
    let c = connect(&s, &device, ConnectOptions::default()).await;
    assert_eq!(c.call("plugin:notes|list_notes", json!({ "q": 1 })).await.unwrap(), json!({ "q": 1 }), "read");
    assert_eq!(c.call("plugin:systems|run_backtest", json!({ "n": 3 })).await.unwrap(), json!({ "done": 3 }), "compute");
    assert_eq!(c.call("plugin:notes|create_note", json!({ "note": "a" })).await.unwrap()["runs"], 1, "write");
    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap_err().code, "step_up_required", "external");
    c.step_up("482915").await.unwrap();
    assert_eq!(c.call("plugin:algo|start_bot", json!({})).await.unwrap(), json!("started"));
    assert_eq!(c.call("plugin:algo|wallet_export_key", json!({})).await.unwrap_err().code, "denied");
    assert_eq!(c.call("plugin:pilot|anything", json!({})).await.unwrap_err().code, "denied");
}

#[tokio::test]
async fn a_long_backtest_streams_its_progress_then_its_result() {
    let s = server("progress", &[]);
    let device = paired(&s).await;
    let c = connect(&s, &device, ConnectOptions::default()).await;
    let mut events = c.take_events().unwrap();
    let r = c.call_with("plugin:systems|run_backtest", json!({ "n": 300 }), CallOptions { timeout: Duration::from_secs(60), ..Default::default() }).await;
    assert_eq!(r.unwrap(), json!({ "done": 300 }));
    let mut seen = Vec::new();
    let mut last = 0;
    while seen.len() < 300 {
        match tokio::time::timeout(Duration::from_secs(10), events.recv()).await.unwrap().unwrap() {
            ClientEvent::Event { seq, kind, topic, payload } => {
                assert!(seq > last, "in order");
                last = seq;
                assert_eq!((kind, topic.as_str()), (EventKind::Tauri, "backtest:progress"));
                seen.push(payload["i"].as_u64().unwrap());
            }
            ClientEvent::Reload { .. } => panic!("no reload"),
        }
    }
    assert_eq!(seen, (0..300).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_disconnect_mid_stream_resumes_without_lost_or_doubled_events() {
    let s = server("resume", &[]);
    let device = paired(&s).await;
    let a = connect(&s, &device, ConnectOptions::default()).await;
    let epoch = a.welcome.epoch;
    let mut ev = a.take_events().unwrap();
    a.call("plugin:notes|stream", json!({ "n": 3000, "per_ms": 5 })).await.unwrap();
    let mut got: Vec<u64> = Vec::new();
    let mut last_seq = 0;
    while got.len() < 800 {
        if let ClientEvent::Event { seq, payload, .. } = tokio::time::timeout(Duration::from_secs(10), ev.recv()).await.unwrap().unwrap() {
            got.push(payload["i"].as_u64().unwrap());
            last_seq = seq;
        }
    }
    // The connection drops mid-stream; the stream goes on on the server.
    drop(ev);
    drop(a);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let resume = ResumePoint { epoch, last_seq };
    let b = connect(&s, &device, ConnectOptions { resume: Some(resume), ..Default::default() }).await;
    assert!(!b.welcome.reload, "the missed events are still kept");
    let mut ev = b.take_events().unwrap();
    let mut seqs = vec![last_seq];
    while got.len() < 3000 {
        if let ClientEvent::Event { seq, payload, .. } = tokio::time::timeout(Duration::from_secs(10), ev.recv()).await.unwrap().unwrap() {
            got.push(payload["i"].as_u64().unwrap());
            seqs.push(seq);
        }
    }
    let unique: BTreeSet<u64> = got.iter().copied().collect();
    assert_eq!(unique.len(), got.len(), "no event twice");
    assert_eq!(got, (0..3000).collect::<Vec<_>>(), "none lost, in order");
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "sequence numbers without a gap");

    // A resume point from another server run means: reload.
    let c = connect(&s, &device, ConnectOptions { resume: Some(ResumePoint { epoch: epoch ^ 1, last_seq: 5 }), ..Default::default() }).await;
    assert!(c.welcome.reload);
}

#[tokio::test]
async fn a_timed_out_write_is_resolved_by_its_idempotency_key() {
    let s = server("keyed", &[]);
    let device = paired(&s).await;
    let c = connect(&s, &device, ConnectOptions::default()).await;

    // The client gives up waiting, then asks what happened instead of sending again.
    let key = client::new_key();
    let opts = CallOptions { key: Some(key.clone()), timeout: Duration::from_millis(300) };
    let r = c.call_with("plugin:notes|create_note", json!({ "note": "n1", "delay_ms": 1500 }), opts.clone()).await;
    assert_eq!(r.unwrap_err().code, "timeout");
    assert_eq!(c.outcome(&key, Duration::from_secs(10)).await.unwrap()["runs"], 1, "waits for the running call");
    // Sent again with the same key: the remembered result, not a second run.
    let again = c.call_with("plugin:notes|create_note", json!({ "note": "n1" }), CallOptions { key: Some(key.clone()), ..Default::default() }).await;
    assert_eq!(again.unwrap()["runs"], 1);
    assert_eq!(c.call("plugin:notes|count_runs", json!({ "note": "n1" })).await.unwrap(), 1);

    // The connection drops while the write runs; a new one asks by key.
    let key2 = client::new_key();
    let opts = CallOptions { key: Some(key2.clone()), timeout: Duration::from_millis(50) };
    let _ = c.call_with("plugin:notes|create_note", json!({ "note": "n2", "delay_ms": 800 }), opts).await;
    drop(c);
    let d = connect(&s, &device, ConnectOptions::default()).await;
    assert_eq!(d.outcome(&key2, Duration::from_secs(10)).await.unwrap()["runs"], 1);
    assert_eq!(d.call("plugin:notes|count_runs", json!({ "note": "n2" })).await.unwrap(), 1, "ran exactly once");
    // A key the server never saw: safe to send.
    assert_eq!(d.outcome(&client::new_key(), Duration::from_secs(5)).await.unwrap_err().code, "unknown");
}

async fn transfer_round(s: &Server, device: &Identity, size: u64) {
    let big = s.dir.join("files").join("big.bin");
    random_file(&big, size, 0x9e3779b97f4a7c15);
    std::fs::create_dir_all(s.dir.join("files").join("secret")).unwrap();
    std::fs::write(s.dir.join("files").join("secret").join("key.txt"), b"no").unwrap();
    let want = sha(&big);
    let local_dir = s.dir.join("client");
    std::fs::create_dir_all(&local_dir).unwrap();
    let local = local_dir.join("big.bin");

    let c = connect(s, device, ConnectOptions::default()).await;
    let roots = c.list_files(None).await.unwrap();
    assert_eq!(roots.entries.len(), 1);
    let folder = c.list_files(Some(&roots.entries[0].path)).await.unwrap();
    let names: Vec<_> = folder.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["big.bin"], "the denied folder is not listed");
    let remote = folder.entries[0].path.clone();
    let secret = s.dir.join("files").join("secret").join("key.txt").display().to_string();
    assert!(c.download(&secret, &local_dir.join("key.txt")).await.is_err(), "nor downloadable");

    // Interrupted after a third: the connection drops.
    let t = Instant::now();
    assert_eq!(c.download_until(&remote, &local, Some(size / 3)).await.unwrap_err(), "stopped");
    drop(c);
    let c = connect(s, device, ConnectOptions::default()).await;
    let r = c.download(&remote, &local).await.unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert!(r.resumed_from >= size / 3, "resumed at {}", r.resumed_from);
    assert_eq!(r.sha256, want);
    assert_eq!(sha(&local), want, "the same file arrived");
    eprintln!("download of {} MB: {secs:.1} s ({:.0} MB/s), resumed at byte {}", size >> 20, (size >> 20) as f64 / secs, r.resumed_from);

    // Upload the other way, interrupted the same way.
    let up = local_dir.join("up.bin");
    random_file(&up, size / 4, 42);
    let t = Instant::now();
    assert_eq!(c.upload_until(&up, "up.bin", Some(size / 12)).await.unwrap_err(), "stopped");
    drop(c);
    let c = connect(s, device, ConnectOptions::default()).await;
    let r = c.upload(&up, "up.bin").await.unwrap();
    let secs = t.elapsed().as_secs_f64();
    // From what the server had written: chunks still queued when the
    // connection dropped never arrived.
    assert!(r.resumed_from > 0 && r.resumed_from <= size / 12 + (1 << 20), "resumed at {}", r.resumed_from);
    assert_eq!(sha(&s.dir.join("transfers").join("up.bin")), sha(&up));
    eprintln!("upload of {} MB: {secs:.1} s, resumed at byte {}", (size / 4) >> 20, r.resumed_from);
}

#[tokio::test]
async fn files_are_browsed_and_moved_with_interrupted_resumed_transfers() {
    let s = server("files", &[]);
    let device = paired(&s).await;
    transfer_round(&s, &device, 64 << 20).await;
}

#[tokio::test]
#[ignore = "1 GB on disk twice; run explicitly"]
async fn one_gigabyte_download_interrupted_and_resumed() {
    let s = server("gigabyte", &[]);
    let device = paired(&s).await;
    transfer_round(&s, &device, 1 << 30).await;
}

#[tokio::test]
async fn the_heartbeat_keeps_a_connection_and_silence_closes_it() {
    let s = server("heartbeat", &[("QS_IDLE_MS", "800")]);
    let device = paired(&s).await;
    let alive = connect(&s, &device, ConnectOptions { ping_every: Duration::from_millis(200), ..Default::default() }).await;
    let silent = connect(&s, &device, ConnectOptions { ping_every: Duration::from_secs(60), ..Default::default() }).await;
    tokio::time::timeout(Duration::from_secs(4), silent.closed()).await.expect("a silent client is closed");
    assert!(!alive.is_closed());
    assert!(alive.call("plugin:notes|list_notes", json!({})).await.is_ok());
}

#[tokio::test]
async fn the_rate_limit_slows_a_flood() {
    let s = server("rate", &[("QS_RATE", "5,10")]);
    let device = paired(&s).await;
    let c = connect(&s, &device, ConnectOptions::default()).await;
    let mut limited = 0;
    for _ in 0..30 {
        if let Err(e) = c.call("plugin:notes|list_notes", json!({})).await {
            assert_eq!(e.code, "rate_limited");
            limited += 1;
        }
    }
    assert!((15..=20).contains(&limited), "{limited} of 30 limited");
}

#[tokio::test]
async fn a_running_call_can_be_cancelled() {
    let s = server("cancel", &[]);
    let device = paired(&s).await;
    let c = std::sync::Arc::new(connect(&s, &device, ConnectOptions::default()).await);
    let c2 = c.clone();
    // The first call of a connection has id 1.
    let slow = tokio::spawn(async move { c2.call("plugin:systems|slow_read", json!({ "secs": 30 })).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    c.cancel(1).await;
    let r = tokio::time::timeout(Duration::from_secs(3), slow).await.expect("answered at once").unwrap();
    assert_eq!(r.unwrap_err().code, "cancelled");
}

#[tokio::test]
#[ignore = "prints numbers; run explicitly"]
async fn latency() {
    let s = server("latency", &[("QS_RATE", "100000,100000")]);
    let device = paired(&s).await;
    let c = std::sync::Arc::new(connect(&s, &device, ConnectOptions::default()).await);
    for _ in 0..50 {
        c.call("plugin:notes|list_notes", json!({})).await.unwrap();
    }
    let n = 2000;
    let t = Instant::now();
    for _ in 0..n {
        c.call("plugin:notes|list_notes", json!({ "q": "x" })).await.unwrap();
    }
    let seq = t.elapsed() / n;
    let t = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let c = c.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..100 {
                c.call("plugin:notes|list_notes", json!({})).await.unwrap();
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let par = t.elapsed() / 2000;
    eprintln!("LATENCY one call after another: {:.3} ms; 20 at a time: {:.3} ms per call ({:.0} calls/s)", seq.as_secs_f64() * 1e3, par.as_secs_f64() * 1e3, 1.0 / par.as_secs_f64());
}
