//! The shared candle archive (`qs_core::paths::candle_archive()`): every
//! closed exchange candle is downloaded once and kept.
//!
//! Rules, learned on rotation_lab's candle cache:
//! - only closed bars are stored (`open + step <= now` at fetch time); the
//!   forming bar is fetched every time and never persisted;
//! - a stored bar is never overwritten (`INSERT OR IGNORE`), so a later fetch
//!   window cannot change earlier results;
//! - `spans` records which open-time ranges were already asked of the venue;
//!   only ranges outside every span are fetched. What a span may cover is
//!   decided by [`covered`]: the range before a series' first bar
//!   (pre-listing) and holes shorter than [`HOLE_BARS`] (maintenance) are
//!   covered and never asked again; a longer hole, or the end of a range the
//!   venue did not fill, stays open and is asked again on the next run — a
//!   venue that answers one window empty by mistake (Bybit did, 2026-10-18)
//!   must not leave a permanent hole.
//!
//! Keys are `(venue, symbol, tf)` with the suite's pair spelling
//! (`BTC/USDT`), not the exchange's.

use std::path::Path;

use rusqlite::{params, Connection};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS candles (
    venue   TEXT    NOT NULL,
    symbol  TEXT    NOT NULL,
    tf      TEXT    NOT NULL,
    open_ms INTEGER NOT NULL,
    open    REAL    NOT NULL,
    high    REAL    NOT NULL,
    low     REAL    NOT NULL,
    close   REAL    NOT NULL,
    volume  REAL    NOT NULL,
    PRIMARY KEY (venue, symbol, tf, open_ms)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS spans (
    venue   TEXT    NOT NULL,
    symbol  TEXT    NOT NULL,
    tf      TEXT    NOT NULL,
    from_ms INTEGER NOT NULL,
    to_ms   INTEGER NOT NULL,
    PRIMARY KEY (venue, symbol, tf, from_ms)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version (version) SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM schema_version);
";

/// A run of missing bars at least this long is not trusted as a venue gap.
pub(crate) const HOLE_BARS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bar {
    pub(crate) open_ms: i64,
    pub(crate) open: f64,
    pub(crate) high: f64,
    pub(crate) low: f64,
    pub(crate) close: f64,
    pub(crate) volume: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Key<'a> {
    pub(crate) venue: &'a str,
    pub(crate) symbol: &'a str,
    pub(crate) tf: &'a str,
}

/// Fetches the bars opening in `[from, to]` from the venue, reporting the
/// share done (0..=1) as it pages.
pub(crate) type Fetch<'a> = dyn FnMut(i64, i64, &mut dyn FnMut(f64)) -> Result<Vec<Bar>, String> + 'a;

/// The bars a caller wants: opening in `[start_ms, end_ms]`, `step_ms` apart,
/// as seen at `now_ms`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Window {
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    pub(crate) step_ms: i64,
    pub(crate) now_ms: i64,
}

/// What one [`series`] call did: `from_archive` counts the bars that were
/// already stored before the call.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Loaded {
    pub(crate) bars: Vec<Bar>,
    pub(crate) from_archive: usize,
    pub(crate) fetched_ranges: usize,
}

pub(crate) struct CandleArchive {
    conn: Connection,
}

impl CandleArchive {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Candle archive folder: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("Candle archive: {e}"))?;
        // Backtests and the forge may write at the same time.
        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(|e| format!("Candle archive: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("Candle archive: {e}"))?;
        Self::with_conn(conn)
    }

    pub(crate) fn open_default() -> Result<Self, String> {
        Self::open(&qs_core::paths::candle_archive())
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self::with_conn(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn with_conn(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(SCHEMA).map_err(|e| format!("Candle archive schema: {e}"))?;
        Ok(Self { conn })
    }

    fn spans(&self, key: &Key) -> Result<Vec<(i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT from_ms, to_ms FROM spans WHERE venue = ?1 AND symbol = ?2 AND tf = ?3 ORDER BY from_ms")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![key.venue, key.symbol, key.tf], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    fn first_open(&self, key: &Key) -> Result<Option<i64>, String> {
        self.conn
            .query_row(
                "SELECT MIN(open_ms) FROM candles WHERE venue = ?1 AND symbol = ?2 AND tf = ?3",
                params![key.venue, key.symbol, key.tf],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }

    /// Store closed bars and mark `spans` as asked, in one transaction: a
    /// span without its bars would hide them from every later run.
    fn record(&mut self, key: &Key, bars: &[Bar], spans: &[(i64, i64)]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT OR IGNORE INTO candles (venue, symbol, tf, open_ms, open, high, low, close, volume)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(|e| e.to_string())?;
            for b in bars {
                insert
                    .execute(params![key.venue, key.symbol, key.tf, b.open_ms, b.open, b.high, b.low, b.close, b.volume])
                    .map_err(|e| e.to_string())?;
            }
            for &(from, to) in spans {
            // Merge with every span it touches or adjoins.
            let mut stmt = tx
                .prepare(
                    "SELECT from_ms, to_ms FROM spans WHERE venue = ?1 AND symbol = ?2 AND tf = ?3
                     AND to_ms >= ?4 - 1 AND from_ms <= ?5 + 1",
                )
                .map_err(|e| e.to_string())?;
            let touching: Vec<(i64, i64)> = stmt
                .query_map(params![key.venue, key.symbol, key.tf, from, to], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let lo = touching.iter().map(|s| s.0).fold(from, i64::min);
            let hi = touching.iter().map(|s| s.1).fold(to, i64::max);
            for (f, _) in &touching {
                tx.execute(
                    "DELETE FROM spans WHERE venue = ?1 AND symbol = ?2 AND tf = ?3 AND from_ms = ?4",
                    params![key.venue, key.symbol, key.tf, f],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.execute(
                "INSERT INTO spans (venue, symbol, tf, from_ms, to_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![key.venue, key.symbol, key.tf, lo, hi],
            )
            .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    fn load(&self, key: &Key, from: i64, to: i64) -> Result<Vec<Bar>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT open_ms, open, high, low, close, volume FROM candles
                 WHERE venue = ?1 AND symbol = ?2 AND tf = ?3 AND open_ms BETWEEN ?4 AND ?5 ORDER BY open_ms",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![key.venue, key.symbol, key.tf, from, to], |r| {
                Ok(Bar {
                    open_ms: r.get(0)?,
                    open: r.get(1)?,
                    high: r.get(2)?,
                    low: r.get(3)?,
                    close: r.get(4)?,
                    volume: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }
}

/// The parts of `[from, to]` (open times, inclusive) no span covers.
pub(crate) fn missing(spans: &[(i64, i64)], from: i64, to: i64) -> Vec<(i64, i64)> {
    let mut gaps = Vec::new();
    let mut cursor = from;
    for &(f, t) in spans {
        if t < cursor {
            continue;
        }
        if f > to {
            break;
        }
        if f > cursor {
            gaps.push((cursor, f - 1));
        }
        cursor = cursor.max(t.saturating_add(1));
        if cursor > to {
            return gaps;
        }
    }
    if cursor <= to {
        gaps.push((cursor, to));
    }
    gaps
}

/// The parts of a fetched range `[from, to]` a span may cover, given the
/// open times the venue returned (sorted, inside the range). `pre_listing`
/// says no stored bar lies before the range, so an empty start is the time
/// before the pair existed.
pub(crate) fn covered(from: i64, to: i64, opens: &[i64], step: i64, pre_listing: bool) -> Vec<(i64, i64)> {
    let hole = |a: i64, b: i64| (b - a) / step > HOLE_BARS;
    let (Some(&first), Some(&last)) = (opens.first(), opens.last()) else {
        return if pre_listing { vec![(from, to)] } else { Vec::new() };
    };
    let mut spans = Vec::new();
    let mut start = if pre_listing || !hole(from - step, first) { from } else { first };
    for pair in opens.windows(2) {
        if hole(pair[0], pair[1]) {
            spans.push((start, pair[0]));
            start = pair[1];
        }
    }
    // Bars missing after the last one returned stay open: a delisting is
    // asked again for nothing, a dropped window is filled next time.
    spans.push((start, if to - last >= step { last } else { to }));
    spans
}

/// The window's bars: stored bars plus the ranges no earlier call asked for,
/// fetched with `fetch(from, to, progress)` and recorded, plus the forming
/// bar(s), fetched but never stored.
pub(crate) fn series(
    archive: &mut CandleArchive,
    key: &Key,
    window: Window,
    fetch: &mut Fetch,
    on_progress: &mut dyn FnMut(f64),
) -> Result<Loaded, String> {
    let Window { start_ms, end_ms, step_ms, now_ms } = window;
    // A bar is closed once its whole interval lies in the past.
    let closed_to = end_ms.min(now_ms - step_ms);
    let gaps = if closed_to >= start_ms {
        missing(&archive.spans(key)?, start_ms, closed_to)
    } else {
        Vec::new()
    };
    let forming = (end_ms > closed_to).then_some((start_ms.max(closed_to + 1), end_ms));

    let mut ranges: Vec<(i64, i64)> = gaps.clone();
    ranges.extend(forming);
    let total: f64 = ranges.iter().map(|(f, t)| (t - f + 1) as f64).sum::<f64>().max(1.0);
    let mut done = 0.0;
    let mut stored_now = 0;

    for &(from, to) in &gaps {
        let span = (to - from + 1) as f64;
        let bars = fetch(from, to, &mut |share| on_progress(((done + share * span) / total).clamp(0.0, 1.0)))?;
        let mut closed: Vec<Bar> = bars
            .into_iter()
            .filter(|b| b.open_ms >= from && b.open_ms <= to && b.open_ms + step_ms <= now_ms)
            .collect();
        closed.sort_by_key(|b| b.open_ms);
        closed.dedup_by_key(|b| b.open_ms);
        let pre_listing = archive.first_open(key)?.map_or(true, |first| first > to);
        let opens: Vec<i64> = closed.iter().map(|b| b.open_ms).collect();
        archive.record(key, &closed, &covered(from, to, &opens, step_ms, pre_listing))?;
        stored_now += closed.len();
        done += span;
    }

    let mut bars = if closed_to >= start_ms { archive.load(key, start_ms, closed_to)? } else { Vec::new() };
    let from_archive = bars.len().saturating_sub(stored_now);
    if let Some((from, to)) = forming {
        let span = (to - from + 1) as f64;
        let fresh = fetch(from, to, &mut |share| on_progress(((done + share * span) / total).clamp(0.0, 1.0)))?;
        bars.extend(fresh.into_iter().filter(|b| b.open_ms >= from && b.open_ms <= to));
    }
    on_progress(1.0);
    bars.sort_by_key(|b| b.open_ms);
    bars.dedup_by_key(|b| b.open_ms);
    Ok(Loaded { from_archive, fetched_ranges: ranges.len(), bars })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const KEY: Key = Key { venue: "binance", symbol: "BTC/USDT", tf: "1h" };

    fn bar(open_ms: i64) -> Bar {
        let p = open_ms as f64 / H as f64;
        Bar { open_ms, open: p, high: p + 1.0, low: p - 1.0, close: p + 0.5, volume: 10.0 }
    }

    /// A venue with hourly bars from `listed` on, missing `holes`, that
    /// records every request it answers.
    struct Venue {
        listed: i64,
        holes: Vec<i64>,
        calls: Vec<(i64, i64)>,
        price_bump: f64,
        now: i64,
    }

    impl Venue {
        fn new(listed: i64) -> Self {
            Venue { listed, holes: Vec::new(), calls: Vec::new(), price_bump: 0.0, now: i64::MAX }
        }

        fn fetch(&mut self, from: i64, to: i64) -> Vec<Bar> {
            self.calls.push((from, to));
            let first = ((from.max(self.listed) + H - 1) / H) * H;
            (0..)
                .map(|i| first + i * H)
                .take_while(|t| *t <= to && *t <= self.now)
                .filter(|t| !self.holes.contains(t))
                .map(|t| Bar { close: bar(t).close + self.price_bump, ..bar(t) })
                .collect()
        }
    }

    fn window(start_ms: i64, end_ms: i64, now_ms: i64) -> Window {
        Window { start_ms, end_ms, step_ms: H, now_ms }
    }

    fn run(archive: &mut CandleArchive, venue: &mut Venue, start: i64, end: i64, now: i64) -> Loaded {
        venue.now = now;
        series(archive, &KEY, window(start, end, now), &mut |f, t, _| Ok(venue.fetch(f, t)), &mut |_| {}).unwrap()
    }

    #[test]
    fn missing_ranges_skip_every_span() {
        let spans = [(10, 19), (30, 39)];
        assert_eq!(missing(&spans, 0, 50), vec![(0, 9), (20, 29), (40, 50)]);
        assert_eq!(missing(&spans, 12, 35), vec![(20, 29)]);
        assert_eq!(missing(&spans, 12, 18), vec![]);
        assert_eq!(missing(&[], 5, 7), vec![(5, 7)]);
    }

    #[test]
    fn a_second_run_reads_the_archive_only() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        let first = run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!((first.bars.len(), first.from_archive), (100, 0));
        assert_eq!(venue.calls.len(), 1);
        let second = run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!(second.bars, first.bars);
        assert_eq!(second.from_archive, 100);
        assert_eq!(venue.calls.len(), 1, "a closed range is never fetched twice");
    }

    #[test]
    fn the_forming_bar_is_fetched_every_time_and_never_stored() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        // now = 10:30 → bars 0..=9 are closed, the 10:00 bar is forming.
        let now = 10 * H + H / 2;
        let first = run(&mut archive, &mut venue, 0, 12 * H, now);
        assert_eq!(first.bars.len(), 11);
        assert_eq!(archive.load(&KEY, 0, 12 * H).unwrap().len(), 10);
        venue.price_bump = 5.0;
        let second = run(&mut archive, &mut venue, 0, 12 * H, now);
        assert_eq!(venue.calls.last(), Some(&(9 * H + H / 2 + 1, 12 * H)), "only the forming part is asked again");
        assert_eq!(second.bars[10].close, first.bars[10].close + 5.0);
        assert_eq!(second.bars[..10], first.bars[..10]);
    }

    #[test]
    fn stored_bars_are_never_overwritten() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        let first = run(&mut archive, &mut venue, 0, 9 * H, 1000 * H);
        venue.price_bump = 1.0;
        // A wider request fetches only the new tail; the stored head stays.
        let wider = run(&mut archive, &mut venue, 0, 19 * H, 1000 * H);
        assert_eq!(venue.calls.last(), Some(&(9 * H + 1, 19 * H)));
        assert_eq!(wider.bars[..10], first.bars[..]);
    }

    #[test]
    fn venue_gaps_and_pre_listing_days_are_not_asked_again() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(20 * H);
        venue.holes = vec![25 * H, 26 * H];
        let first = run(&mut archive, &mut venue, 0, 39 * H, 1000 * H);
        assert_eq!(first.bars.len(), 18);
        let again = run(&mut archive, &mut venue, 0, 39 * H, 1000 * H);
        assert_eq!(again.bars, first.bars);
        assert_eq!(venue.calls.len(), 1);
    }

    #[test]
    fn a_window_the_venue_dropped_once_is_asked_again_and_filled() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        venue.holes = (30..60).map(|h| h * H).collect();
        assert_eq!(run(&mut archive, &mut venue, 0, 99 * H, 1000 * H).bars.len(), 70);
        venue.holes.clear();
        let healed = run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!(venue.calls.last(), Some(&(29 * H + 1, 60 * H - 1)));
        assert_eq!(healed.bars.len(), 100);
        assert_eq!(archive.spans(&KEY).unwrap(), vec![(0, 99 * H)]);
        run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!(venue.calls.len(), 2);
    }

    #[test]
    fn an_unfilled_end_and_an_empty_answer_after_data_stay_open() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        venue.holes = (80..100).map(|h| h * H).collect();
        run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!(archive.spans(&KEY).unwrap(), vec![(0, 79 * H)]);
        // The tail comes back empty once more: still nothing is covered.
        run(&mut archive, &mut venue, 0, 99 * H, 1000 * H);
        assert_eq!(archive.spans(&KEY).unwrap(), vec![(0, 79 * H)]);
        venue.holes.clear();
        assert_eq!(run(&mut archive, &mut venue, 0, 99 * H, 1000 * H).bars.len(), 100);
        assert_eq!(archive.spans(&KEY).unwrap(), vec![(0, 99 * H)]);
    }

    #[test]
    fn covered_trusts_pre_listing_and_short_holes_only() {
        let opens = |hours: &[i64]| hours.iter().map(|h| h * H).collect::<Vec<_>>();
        assert_eq!(covered(0, 99 * H, &opens(&[50, 51, 52]), H, true), vec![(0, 52 * H)]);
        assert_eq!(covered(0, 99 * H, &opens(&[50, 51, 52]), H, false), vec![(50 * H, 52 * H)]);
        assert_eq!(covered(0, 9 * H, &opens(&[0, 3, 9]), H, false), vec![(0, 9 * H)]);
        assert_eq!(covered(0, 99 * H, &opens(&[0, 1, 90, 99]), H, false), vec![(0, H), (90 * H, 99 * H)]);
        assert_eq!(covered(0, 99 * H, &[], H, true), vec![(0, 99 * H)]);
        assert_eq!(covered(0, 99 * H, &[], H, false), vec![]);
    }

    #[test]
    fn requests_on_both_sides_of_a_span_fetch_only_those_sides() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        run(&mut archive, &mut venue, 50 * H, 59 * H, 1000 * H);
        let all = run(&mut archive, &mut venue, 40 * H, 69 * H, 1000 * H);
        assert_eq!(all.bars.len(), 30);
        assert_eq!(&venue.calls[1..], &[(40 * H, 50 * H - 1), (59 * H + 1, 69 * H)]);
        assert_eq!(archive.spans(&KEY).unwrap(), vec![(40 * H, 69 * H)]);
        run(&mut archive, &mut venue, 45 * H, 65 * H, 1000 * H);
        assert_eq!(venue.calls.len(), 3);
    }

    #[test]
    fn a_failed_fetch_records_nothing() {
        let mut archive = CandleArchive::in_memory();
        let err = series(&mut archive, &KEY, window(0, 9 * H, 1000 * H), &mut |_, _, _| Err("HTTP 503".into()), &mut |_| {});
        assert_eq!(err.unwrap_err(), "HTTP 503");
        assert!(archive.spans(&KEY).unwrap().is_empty());
        let mut venue = Venue::new(0);
        assert_eq!(run(&mut archive, &mut venue, 0, 9 * H, 1000 * H).bars.len(), 10);
    }

    #[test]
    fn keys_do_not_share_bars() {
        let mut archive = CandleArchive::in_memory();
        let mut venue = Venue::new(0);
        run(&mut archive, &mut venue, 0, 9 * H, 1000 * H);
        let other = Key { venue: "bybit", ..KEY };
        let loaded = series(&mut archive, &other, window(0, 9 * H, 1000 * H), &mut |f, t, _| Ok(venue.fetch(f, t)), &mut |_| {}).unwrap();
        assert_eq!((loaded.bars.len(), loaded.from_archive), (10, 0));
        assert_eq!(venue.calls.len(), 2);
    }
}
