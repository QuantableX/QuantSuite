//! Events of server-placed modules for every connected device
//! (PLAN-SERVER-CLIENT §5 "Connection", §6.3.2).
//!
//! Each event gets the next sequence number of this server run (`epoch`) and
//! stays in a replay buffer for a while. A reconnecting client names the last
//! event it received; it gets everything after it, or — when that is no longer
//! kept, or the server restarted — the order to reload its pages' state.
//! Attaching snapshots the buffer and subscribes under one lock, so replay and
//! live events neither overlap nor leave a gap.

use crate::protocol::{EventKind, ResumePoint};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Events kept for a reconnecting client.
pub const KEEP_EVENTS: usize = 10_000;
pub const KEEP_FOR: Duration = Duration::from_secs(10 * 60);
/// Events a slow connection may fall behind before it must reload.
const LIVE_BACKLOG: usize = 4096;

#[derive(Debug)]
pub struct EventItem {
    pub seq: u64,
    pub kind: EventKind,
    pub topic: String,
    pub payload: Value,
    at: Instant,
}

struct Ring {
    next: u64,
    buf: VecDeque<Arc<EventItem>>,
}

pub struct EventLog {
    epoch: u64,
    keep: usize,
    keep_for: Duration,
    ring: Mutex<Ring>,
    tx: broadcast::Sender<Arc<EventItem>>,
}

pub struct Attach {
    pub rx: broadcast::Receiver<Arc<EventItem>>,
    /// The newest event at attach time; live events follow it.
    pub seq: u64,
    /// Events after the resume point, oldest first.
    pub replay: Vec<Arc<EventItem>>,
    /// The resume point could not be honoured.
    pub reload: bool,
}

fn random_epoch() -> u64 {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 8];
    let _ = ring::rand::SystemRandom::new().fill(&mut b);
    // Never 0: a client without a resume point sends none at all.
    u64::from_be_bytes(b).max(1)
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new(KEEP_EVENTS, KEEP_FOR)
    }
}

impl EventLog {
    pub fn new(keep: usize, keep_for: Duration) -> Self {
        let (tx, _) = broadcast::channel(LIVE_BACKLOG);
        Self { epoch: random_epoch(), keep, keep_for, ring: Mutex::new(Ring { next: 1, buf: VecDeque::new() }), tx }
    }

    fn ring(&self) -> std::sync::MutexGuard<'_, Ring> {
        self.ring.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The newest sequence number (0 before the first event).
    pub fn current(&self) -> u64 {
        self.ring().next - 1
    }

    pub fn publish(&self, kind: EventKind, topic: &str, payload: Value) -> u64 {
        let mut r = self.ring();
        let seq = r.next;
        r.next += 1;
        let item = Arc::new(EventItem { seq, kind, topic: topic.to_string(), payload, at: Instant::now() });
        r.buf.push_back(item.clone());
        while r.buf.len() > self.keep || r.buf.front().is_some_and(|i| i.at.elapsed() > self.keep_for) {
            r.buf.pop_front();
        }
        // Sent under the lock: subscribers see events in sequence order.
        let _ = self.tx.send(item);
        seq
    }

    pub fn attach(&self, resume: Option<ResumePoint>) -> Attach {
        let r = self.ring();
        let rx = self.tx.subscribe();
        let seq = r.next - 1;
        let Some(point) = resume else {
            return Attach { rx, seq, replay: Vec::new(), reload: false };
        };
        let oldest = r.buf.front().map(|i| i.seq).unwrap_or(r.next);
        if point.epoch != self.epoch || point.last_seq > seq || point.last_seq + 1 < oldest {
            return Attach { rx, seq, replay: Vec::new(), reload: true };
        }
        let replay = r.buf.iter().filter(|i| i.seq > point.last_seq).cloned().collect();
        Attach { rx, seq, replay, reload: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish(log: &EventLog, n: u64) {
        for i in 0..n {
            log.publish(EventKind::Tauri, "t", Value::from(i));
        }
    }

    #[test]
    fn resume_replays_exactly_what_was_missed() {
        let log = EventLog::new(100, KEEP_FOR);
        publish(&log, 10);
        let a = log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 4 }));
        assert!(!a.reload);
        assert_eq!(a.replay.iter().map(|i| i.seq).collect::<Vec<_>>(), (5..=10).collect::<Vec<_>>());
        assert_eq!(a.seq, 10);
        // Up to date: nothing to replay.
        let a = log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 10 }));
        assert!(!a.reload && a.replay.is_empty());
    }

    #[test]
    fn live_events_follow_the_snapshot_without_overlap() {
        let log = EventLog::new(100, KEEP_FOR);
        publish(&log, 3);
        let mut a = log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 1 }));
        publish(&log, 2);
        let mut seen: Vec<u64> = a.replay.iter().map(|i| i.seq).collect();
        while let Ok(i) = a.rx.try_recv() {
            seen.push(i.seq);
        }
        assert_eq!(seen, vec![2, 3, 4, 5]);
    }

    #[test]
    fn a_lost_or_foreign_resume_point_means_reload() {
        let log = EventLog::new(5, KEEP_FOR);
        publish(&log, 20);
        assert!(log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 3 })).reload, "events no longer kept");
        assert!(!log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 15 })).reload, "16..20 are kept");
        assert!(log.attach(Some(ResumePoint { epoch: log.epoch() ^ 1, last_seq: 19 })).reload, "another server run");
        assert!(log.attach(Some(ResumePoint { epoch: log.epoch(), last_seq: 99 })).reload, "from the future");
        assert!(!log.attach(None).reload);
    }
}
