//! Card 18: the usage ledger against fixture logs.

use super::pricing::{self, PriceList, Rates};
use super::scan::{self, Ledger, Provider, Tokens};
use super::*;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;

fn total(c: &pricing::Cost) -> f64 {
    c.input + c.cache_read + c.cache_write + c.output
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qp-usage-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[allow(clippy::too_many_arguments)]
fn claude(id: &str, request: &str, ts: &str, model: &str, input: u64, read: u64, write_5m: u64, write_1h: u64, output: u64) -> String {
    json!({
        "type": "assistant", "timestamp": ts, "requestId": request, "sessionId": "s",
        "message": { "id": id, "model": model, "role": "assistant", "content": [{ "type": "text", "text": "a reply nobody keeps" }],
            "usage": { "input_tokens": input, "cache_read_input_tokens": read, "cache_creation_input_tokens": write_5m + write_1h,
                "cache_creation": { "ephemeral_5m_input_tokens": write_5m, "ephemeral_1h_input_tokens": write_1h }, "output_tokens": output } }
    })
    .to_string()
}

fn write(path: &PathBuf, lines: &[String]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
}

fn sum(ledger: &Ledger) -> (usize, Tokens) {
    let recs = ledger.records(0);
    let mut t = Tokens::default();
    recs.iter().for_each(|(_, r)| t.add(&r.tokens));
    (recs.len(), t)
}

#[test]
fn claude_requests_count_once_across_streams_and_resumed_sessions() {
    let dir = temp("claude");
    let a = claude("msg_a", "req_a", "2026-10-29T10:00:00.000Z", "claude-opus-5-5", 10, 1000, 200, 300, 50);
    let b = claude("msg_b", "req_b", "2026-10-29T10:05:00.000Z", "claude-opus-5-5", 5, 2000, 0, 0, 70);
    let c = claude("msg_c", "req_c", "2026-10-29T11:00:00.000Z", "claude-sonnet-5-5", 7, 0, 0, 0, 9);
    let user = json!({ "type": "user", "message": { "role": "user", "content": "a prompt with \"usage\" in it" } }).to_string();
    // The first session streams A twice; the resumed one copies A and B, then adds C; a subagent log adds D.
    write(&dir.join("proj/one.jsonl"), &[user.clone(), a.clone(), a.clone(), b.clone()]);
    write(&dir.join("proj/two.jsonl"), &[a, b, c]);
    let d = claude("msg_d", "req_d", "2026-10-29T11:30:00.000Z", "<synthetic>", 1, 0, 0, 0, 1);
    let e = claude("msg_e", "req_e", "2026-10-29T11:30:00.000Z", "claude-haiku-5-5", 3, 0, 0, 0, 4);
    write(&dir.join("proj/two/subagents/agent-1.jsonl"), &[d, e]);
    let files = scan::files(&[(Provider::Claude, dir.clone())]);
    assert_eq!(files.len(), 3);
    let mut ledger = Ledger::default();
    ledger.refresh(&files);
    let (n, t) = sum(&ledger);
    assert_eq!(n, 4, "A, B, C and the subagent's E; the synthetic reply never reached a model");
    assert_eq!(t, Tokens { input: 25, cache_read: 3000, cache_write_5m: 200, cache_write_1h: 300, output: 133 });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn appended_logs_are_read_from_where_the_last_scan_stopped() {
    let dir = temp("append");
    let path = dir.join("p/s.jsonl");
    write(&path, &[claude("m1", "r1", "2026-10-29T10:00:00Z", "claude-opus-5-5", 1, 0, 0, 0, 1)]);
    let files = scan::files(&[(Provider::Claude, dir.clone())]);
    let mut ledger = Ledger::default();
    assert!(ledger.refresh(&files).read > 0);
    assert_eq!(ledger.refresh(&files).read, 0, "nothing changed, nothing read");
    // A complete line and one still being written.
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    let second = claude("m2", "r2", "2026-10-29T10:01:00Z", "claude-opus-5-5", 2, 0, 0, 0, 2);
    let third = claude("m3", "r3", "2026-10-29T10:02:00Z", "claude-opus-5-5", 4, 0, 0, 0, 4);
    write!(f, "{second}\n{}", &third[..20]).unwrap();
    drop(f);
    let read = ledger.refresh(&files).read;
    assert!(read > 0 && (read as usize) <= second.len() + 1, "only the appended complete line: {read}");
    assert_eq!(sum(&ledger).1.input, 3);
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "{}", &third[20..]).unwrap();
    drop(f);
    ledger.refresh(&files);
    assert_eq!(sum(&ledger).1.input, 7, "the line finished later counts once");
    let _ = std::fs::remove_dir_all(dir);
}

fn rollout(lines: &[serde_json::Value]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn token_count(ts: &str, total: u64, input: u64, cached: u64, output: u64) -> serde_json::Value {
    json!({ "timestamp": ts, "type": "event_msg", "payload": { "type": "token_count", "info": {
        "total_token_usage": { "total_tokens": total },
        "last_token_usage": { "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output, "reasoning_output_tokens": 3 } } } })
}

#[test]
fn codex_totals_skip_repeats_and_compressed_rollouts_count_once() {
    let dir = temp("codex");
    let lines = [
        json!({ "timestamp": "2026-10-29T10:00:00Z", "type": "session_meta", "payload": { "id": "x" } }),
        json!({ "timestamp": "2026-10-29T10:00:01Z", "type": "turn_context", "payload": { "model": "gpt-5.5" } }),
        json!({ "timestamp": "2026-10-29T10:00:02Z", "type": "response_item", "payload": { "type": "message", "content": "token_count is only a word here" } }),
        token_count("2026-10-29T10:00:03Z", 1100, 1000, 800, 100),
        // The same total again: a repeat.
        token_count("2026-10-29T10:00:04Z", 1100, 1000, 800, 100),
        token_count("2026-10-29T10:10:00Z", 1700, 500, 400, 100),
        json!({ "timestamp": "2026-10-29T11:00:01Z", "type": "turn_context", "payload": { "model": "gpt-5.3-codex" } }),
        token_count("2026-10-29T11:00:02Z", 1800, 90, 0, 10),
    ];
    let text = rollout(&lines);
    let plain = dir.join("sessions/2026/10/29/rollout-2026-10-29T10-00-00-abc.jsonl");
    std::fs::create_dir_all(plain.parent().unwrap()).unwrap();
    std::fs::write(&plain, &text).unwrap();
    // The same rollout compressed in an archive, and another one only compressed.
    let other = rollout(&[json!({ "timestamp": "2026-10-28T09:00:00Z", "type": "turn_context", "payload": { "model": "o3" } }), token_count("2026-10-28T09:00:01Z", 30, 20, 0, 10)]);
    let zst = |s: &str| ruzstd::encoding::compress_to_vec(s.as_bytes(), ruzstd::encoding::CompressionLevel::Fastest);
    std::fs::create_dir_all(dir.join("archived_sessions")).unwrap();
    std::fs::write(dir.join("archived_sessions/rollout-2026-10-29T10-00-00-abc.jsonl.zst"), zst(&text)).unwrap();
    std::fs::write(dir.join("archived_sessions/rollout-2026-10-28T09-00-00-def.jsonl.zst"), zst(&other)).unwrap();
    let roots = [(Provider::Codex, dir.join("sessions")), (Provider::Codex, dir.join("archived_sessions"))];
    let files = scan::files(&roots);
    assert_eq!(files.len(), 2, "the shared rollout once, uncompressed: {files:?}");
    assert!(files.iter().any(|(_, p)| p == &plain));
    let mut ledger = Ledger::default();
    let stats = ledger.refresh(&files);
    assert_eq!(stats.unreadable, 0);
    let recs = ledger.records(0);
    let by = |m: &str| recs.iter().filter(|(_, r)| r.model == m).fold(Tokens::default(), |mut t, (_, r)| {
        t.add(&r.tokens);
        t
    });
    assert_eq!(by("gpt-5.5"), Tokens { input: 300, cache_read: 1200, cache_write_5m: 0, cache_write_1h: 0, output: 200 }, "cached tokens leave input; the repeat is skipped");
    assert_eq!(by("gpt-5.3-codex").input, 90);
    assert_eq!(by("o3").output, 10, "a compressed rollout counts");
    assert_eq!(recs.iter().filter(|(_, r)| r.model == "gpt-5.5").count(), 1, "one hourly record per model");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pricing_tiers_cache_writes_and_unpriced_models() {
    let prices = PriceList::bundled();
    let opus = prices.lookup("claude-opus-5-5").expect("in the snapshot");
    let t = Tokens { input: 1_000_000, cache_read: 1_000_000, cache_write_5m: 1_000_000, cache_write_1h: 1_000_000, output: 1_000_000 };
    let c = opus.cost(0, &t);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(close(c.input, opus.input_cost_per_token * 1e6));
    assert!(close(c.cache_write, (opus.cache_creation_input_token_cost.unwrap() + opus.cache_creation_input_token_cost_above_1hr.unwrap()) * 1e6), "one-hour writes at their own rate");
    assert!(close(c.savings, (opus.input_cost_per_token - opus.cache_read_input_token_cost.unwrap()) * 1e6));
    // GPT-5.5 costs more above 272k prompt tokens.
    let gpt = prices.lookup("gpt-5.5").unwrap();
    let small = gpt.cost(0, &Tokens { input: 1000, ..Default::default() });
    let large = gpt.cost(2, &Tokens { input: 1000, ..Default::default() });
    assert!(large.input > small.input);
    // A listing without cache rates charges cached tokens as input.
    let plain = Rates { input_cost_per_token: 1e-6, output_cost_per_token: 2e-6, ..Default::default() };
    let c = plain.cost(0, &Tokens { cache_read: 10, cache_write_5m: 10, ..Default::default() });
    assert!(close(c.cache_read, 1e-5) && close(c.cache_write, 1e-5) && close(c.savings, 0.0));
    // Dated and suffixed names find their listing; unknown models stay unpriced.
    assert!(prices.lookup("claude-haiku-4-5-20251001").is_some());
    assert!(prices.lookup("anthropic/claude-sonnet-5-5[1m]").is_some());
    assert!(prices.lookup("codex-auto-review").is_none());
}

#[test]
fn report_rows_price_each_size_class_and_mark_unpriced_ones() {
    let dir = temp("report");
    write(
        &dir.join("p/s.jsonl"),
        &[
            claude("m1", "r1", "2026-10-29T10:00:00Z", "claude-opus-5-5", 100, 0, 0, 0, 10),
            claude("m2", "r2", "2026-10-29T10:30:00Z", "claude-opus-5-5", 300_000, 0, 0, 0, 10),
            claude("m3", "r3", "2026-10-29T10:40:00Z", "claude-mystery-9", 5, 0, 0, 0, 5),
        ],
    );
    let mut ledger = Ledger::default();
    ledger.refresh(&scan::files(&[(Provider::Claude, dir.clone())]));
    let prices = PriceList::bundled();
    let rows = report_rows(&ledger, &prices, 0, true);
    assert_eq!(rows.len(), 2, "one row per hour and model");
    let opus = rows.iter().find(|r| r.model == "claude-opus-5-5").unwrap();
    assert_eq!((opus.input, opus.output), (300_100, 20));
    assert!(total(&opus.cost.unwrap()) > 0.0);
    let mystery = rows.iter().find(|r| r.model == "claude-mystery-9").unwrap();
    assert_eq!(mystery.cost, None, "no public price: tokens count, cost stays unknown");
    assert_eq!(opus.bucket, "2026-10-29T10:00:00Z".parse::<chrono::DateTime<chrono::Utc>>().unwrap().timestamp_millis());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn periods_start_on_whole_hours_or_local_days() {
    let now = chrono::Local.with_ymd_and_hms(2026, 10, 30, 14, 25, 0).unwrap();
    let day = |d: u32| chrono::Local.with_ymd_and_hms(2026, 10, d, 0, 0, 0).unwrap().timestamp_millis();
    assert_eq!(period_start(24 * 7, now), day(24), "7 days: today and the six before");
    assert_eq!(period_start(24 * 30, now), day(1));
    assert_eq!(period_start(24, now), (now.timestamp().div_euclid(3600) - 23) * 3_600_000, "24 hours: whole hours");
}

#[test]
fn litellm_file_filters_to_the_vendors_standard_rates() {
    let all = json!({
        "claude-x-1": { "litellm_provider": "anthropic", "mode": "chat", "input_cost_per_token": 1e-6, "output_cost_per_token": 2e-6, "input_cost_per_token_batches": 5e-7 },
        "bedrock/claude-x-1": { "litellm_provider": "bedrock", "mode": "chat", "input_cost_per_token": 9e-6, "output_cost_per_token": 9e-6 },
        "gpt-image-1": { "litellm_provider": "openai", "mode": "image_generation", "input_cost_per_token": 1e-6, "output_cost_per_token": 1e-6 },
        "sample_spec": { "input_cost_per_token": 0 },
    });
    let mut obj = all.as_object().unwrap().clone();
    for i in 0..12 {
        obj.insert(format!("gpt-test-{i}"), json!({ "litellm_provider": "openai", "mode": "chat", "input_cost_per_token": 1e-6, "output_cost_per_token": 1e-6 }));
    }
    let list = PriceList::from_litellm(&serde_json::Value::Object(obj)).unwrap();
    assert!(list.lookup("claude-x-1").is_some());
    assert!(list.lookup("gpt-image-1").is_none(), "not a chat model");
    let saved = PriceList::from_json(&list.to_json()).unwrap();
    assert_eq!(saved.lookup("claude-x-1"), list.lookup("claude-x-1"));
}

/// Live, opt-in: this machine's own logs, for a plausibility check against ccusage-style totals.
#[test]
#[ignore = "live: reads this machine's Claude Code and Codex logs"]
fn live_local_logs() {
    let started = std::time::Instant::now();
    let roots = scan::roots();
    let files = scan::files(&roots);
    let mut ledger = Ledger::default();
    let stats = ledger.refresh(&files);
    let first = started.elapsed();
    let again = std::time::Instant::now();
    ledger.refresh(&files);
    println!("roots {roots:?}\n{} files, {} MB read in {first:?}, rescan {:?}, unreadable {}", stats.files, stats.read / 1_000_000, again.elapsed(), stats.unreadable);
    let prices = PriceList::bundled();
    let rows = report_rows(&ledger, &prices, period_start(24 * 30, chrono::Local::now()), false);
    let mut days: std::collections::BTreeMap<(i64, String), (f64, u64)> = Default::default();
    for r in &rows {
        let d = days.entry((r.bucket, r.provider.clone())).or_default();
        d.0 += r.cost.map_or(0.0, |c| total(&c));
        d.1 += r.input + r.cache_read + r.cache_write + r.output;
    }
    for ((day, provider), (cost, tokens)) in days.iter().rev().take(10) {
        let date = chrono::Local.timestamp_millis_opt(*day).single().unwrap().format("%Y-%m-%d");
        println!("{date} {provider}: {cost:.2}$ {tokens} tokens");
    }
    let unpriced: std::collections::BTreeSet<_> = rows.iter().filter(|r| r.cost.is_none()).map(|r| r.model.clone()).collect();
    println!("unpriced: {unpriced:?}");
}
