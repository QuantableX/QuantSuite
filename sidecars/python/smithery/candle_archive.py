"""The suite's shared candle archive from Python.

Same file and schema as ``modules/algo/crate/src/candle_archive.rs``
(``qs_core::paths::candle_archive()``): candles keyed by venue, the suite's
pair spelling (``BTC/USDT``) and timeframe; closed bars only; a stored bar is
never rewritten; ``spans`` mark the open-time ranges already asked of the
venue, so a QuantAlgo backtest inside a span reads the archive and makes no
request. :func:`covered` is the Rust rule: pre-listing time and holes shorter
than ``HOLE_BARS`` are covered, longer holes and an unfilled end stay open. The spawners pass ``QUANTSUITE_HOME``, so a dev build and the
installed suite each use their own archive.
"""
from __future__ import annotations

import os
import sqlite3
from pathlib import Path
from typing import Iterable

SCHEMA = """
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
"""

Row = tuple[int, float, float, float, float, float]  # open_ms, open, high, low, close, volume

HOLE_BARS = 24  # a run of missing bars at least this long is not a venue gap


def default_path() -> Path:
    home = os.environ.get("QUANTSUITE_HOME")
    root = Path(home) if home else Path.home() / ".quantsuite"
    return root / "market-data" / "candles.sqlite"


def missing(spans: list[tuple[int, int]], start: int, end: int) -> list[tuple[int, int]]:
    """The parts of ``[start, end]`` (open times, inclusive) no span covers."""
    gaps, cursor = [], start
    for f, t in spans:
        if t < cursor:
            continue
        if f > end:
            break
        if f > cursor:
            gaps.append((cursor, f - 1))
        cursor = max(cursor, t + 1)
        if cursor > end:
            return gaps
    if cursor <= end:
        gaps.append((cursor, end))
    return gaps


def covered(start: int, end: int, opens: list[int], step: int, pre_listing: bool) -> list[tuple[int, int]]:
    """The parts of a range ``[start, end]`` a span may cover, given the sorted
    open times in it; ``pre_listing``: no stored bar lies before the range."""
    def hole(a: int, b: int) -> bool:
        return (b - a) // step > HOLE_BARS

    if not opens:
        return [(start, end)] if pre_listing else []
    spans = []
    seg = start if pre_listing or not hole(start - step, opens[0]) else opens[0]
    for a, b in zip(opens, opens[1:]):
        if hole(a, b):
            spans.append((seg, a))
            seg = b
    spans.append((seg, opens[-1] if end - opens[-1] >= step else end))
    return spans


class CandleArchive:
    def __init__(self, path: Path | None = None) -> None:
        self.path = Path(path) if path else default_path()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        # QuantAlgo backtests may write at the same time.
        self.con = sqlite3.connect(self.path, timeout=30.0)
        self.con.execute("PRAGMA journal_mode=WAL")
        self.con.executescript(SCHEMA)

    def close(self) -> None:
        self.con.close()

    def spans(self, venue: str, symbol: str, tf: str) -> list[tuple[int, int]]:
        return [tuple(r) for r in self.con.execute(
            "SELECT from_ms, to_ms FROM spans WHERE venue = ? AND symbol = ? AND tf = ? ORDER BY from_ms",
            (venue, symbol, tf))]

    def first_open(self, venue: str, symbol: str, tf: str) -> int | None:
        return self.con.execute(
            "SELECT MIN(open_ms) FROM candles WHERE venue = ? AND symbol = ? AND tf = ?",
            (venue, symbol, tf)).fetchone()[0]

    def record(self, venue: str, symbol: str, tf: str, rows: Iterable[Row], spans: list[tuple[int, int]]) -> int:
        """Store the bars and mark ``spans`` as asked, in one transaction (a
        span without its bars would hide them). Returns the bars offered."""
        rows = list(rows)
        with self.con:
            self.con.executemany(
                "INSERT OR IGNORE INTO candles (venue, symbol, tf, open_ms, open, high, low, close, volume)"
                " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [(venue, symbol, tf, *row) for row in rows])
            for start, end in spans:
                self._merge_span(venue, symbol, tf, start, end)
        return len(rows)

    def _merge_span(self, venue: str, symbol: str, tf: str, start: int, end: int) -> None:
        # Merge with every span it touches or adjoins.
        touching = self.con.execute(
            "SELECT from_ms, to_ms FROM spans WHERE venue = ? AND symbol = ? AND tf = ?"
            " AND to_ms >= ? - 1 AND from_ms <= ? + 1",
            (venue, symbol, tf, start, end)).fetchall()
        lo = min([start, *(f for f, _ in touching)])
        hi = max([end, *(t for _, t in touching)])
        self.con.executemany(
            "DELETE FROM spans WHERE venue = ? AND symbol = ? AND tf = ? AND from_ms = ?",
            [(venue, symbol, tf, f) for f, _ in touching])
        self.con.execute(
            "INSERT INTO spans (venue, symbol, tf, from_ms, to_ms) VALUES (?, ?, ?, ?, ?)",
            (venue, symbol, tf, lo, hi))
