"""SQLite cache for rankings and OHLCV.

Schemas:

* ``rankings(provider TEXT, snap_date DATE, rank INT, cg_id TEXT,
            symbol TEXT, name TEXT, market_cap REAL, price REAL,
            fetched_at TEXT, PRIMARY KEY(provider, snap_date, rank))``
* ``ohlcv(coin_key TEXT, day DATE, open REAL, high REAL, low REAL,
          close REAL, volume REAL, source TEXT, approx INT,
          PRIMARY KEY(coin_key, day))``
* ``meta(key TEXT PRIMARY KEY, value TEXT)`` for misc state and
  category sync timestamps.
* ``coin_map(cg_id TEXT PRIMARY KEY, symbol TEXT, name TEXT,
            cmc_slug TEXT, binance_symbol TEXT, last_seen DATE)``

Past (immutable) days cache forever. The ``ohlcv`` tables carry no
``fetched_at`` column, so the current (still forming) day is never
treated as settled - ``OhlcvFetcher`` re-fetches and replaces it on
every run. Only ``rankings`` has a real TTL, via its ``fetched_at``.
"""

from __future__ import annotations

import datetime as dt
import logging
import sqlite3
import threading
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator

log = logging.getLogger(__name__)


SCHEMA = [
    """
    CREATE TABLE IF NOT EXISTS rankings (
        provider    TEXT NOT NULL,
        snap_date   DATE NOT NULL,
        rank        INTEGER NOT NULL,
        cg_id       TEXT,
        symbol      TEXT,
        name        TEXT,
        market_cap  REAL,
        price       REAL,
        fetched_at  TEXT NOT NULL,
        PRIMARY KEY (provider, snap_date, rank)
    );
    """,
    "CREATE INDEX IF NOT EXISTS idx_rankings_date ON rankings(snap_date);",
    """
    CREATE TABLE IF NOT EXISTS ohlcv (
        coin_key  TEXT NOT NULL,
        day       DATE NOT NULL,
        open      REAL,
        high      REAL,
        low       REAL,
        close     REAL,
        volume    REAL,
        source    TEXT,
        approx    INTEGER DEFAULT 0,
        PRIMARY KEY (coin_key, day)
    );
    """,
    "CREATE INDEX IF NOT EXISTS idx_ohlcv_day ON ohlcv(day);",
    """
    CREATE TABLE IF NOT EXISTS ohlcv_intraday (
        coin_key  TEXT NOT NULL,
        tf        TEXT NOT NULL,
        ts        TEXT NOT NULL,
        open      REAL,
        high      REAL,
        low       REAL,
        close     REAL,
        volume    REAL,
        source    TEXT,
        approx    INTEGER DEFAULT 0,
        PRIMARY KEY (coin_key, tf, ts)
    );
    """,
    # The (coin_key, tf, ts) PRIMARY KEY already gives SQLite an implicit
    # unique index on exactly that triple, so the old idx_ohlcv_intraday
    # only doubled the write cost of the largest table. Drop it where an
    # earlier version created it.
    "DROP INDEX IF EXISTS idx_ohlcv_intraday;",
    """
    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT
    );
    """,
    """
    CREATE TABLE IF NOT EXISTS coin_map (
        cg_id          TEXT PRIMARY KEY,
        symbol         TEXT,
        name           TEXT,
        cmc_slug       TEXT,
        binance_symbol TEXT,
        last_seen      DATE
    );
    """,
]


@dataclass(frozen=True)
class RankingRow:
    provider: str
    snap_date: dt.date
    rank: int
    cg_id: str | None
    symbol: str | None
    name: str | None
    market_cap: float | None
    price: float | None
    fetched_at: dt.datetime


@dataclass(frozen=True)
class OhlcvRow:
    coin_key: str
    day: dt.date
    open: float | None
    high: float | None
    low: float | None
    close: float | None
    volume: float | None
    source: str | None
    approx: bool


@dataclass(frozen=True)
class OhlcvBar:
    """A single intraday candle (1h / 4h / 12h), keyed by a UTC timestamp."""

    coin_key: str
    tf: str
    ts: dt.datetime
    open: float | None
    high: float | None
    low: float | None
    close: float | None
    volume: float | None
    source: str | None
    approx: bool


def _adapt_date(value):  # registered as adapter
    return value.isoformat()


def _convert_date(value: bytes) -> dt.date:
    return dt.date.fromisoformat(value.decode())


sqlite3.register_adapter(dt.date, _adapt_date)
sqlite3.register_converter("DATE", _convert_date)


class Cache:
    """Thread-safe wrapper around a small SQLite database."""

    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        self._con: sqlite3.Connection | None = None
        self._init_schema()

    @contextmanager
    def _conn(self) -> Iterator[sqlite3.Connection]:
        """One connection per cache, reused for every statement.

        A run issues thousands of single-statement queries; opening a
        fresh connection (plus its two PRAGMAs) for each one costs more
        than the queries themselves. The RLock serialises access, which
        is what ``check_same_thread=False`` requires.
        """

        with self._lock:
            con = self._con
            if con is None:
                con = sqlite3.connect(
                    self.path,
                    detect_types=sqlite3.PARSE_DECLTYPES,
                    timeout=30.0,
                    check_same_thread=False,
                )
                con.execute("PRAGMA journal_mode=WAL;")
                con.execute("PRAGMA synchronous=NORMAL;")
                self._con = con
            try:
                yield con
                con.commit()
            except Exception:
                # The connection outlives the failure now, so the aborted
                # statements have to be rolled back explicitly instead of
                # disappearing with a close().
                con.rollback()
                raise

    def _init_schema(self) -> None:
        with self._conn() as con:
            for stmt in SCHEMA:
                con.execute(stmt)

    # ----------------------------- rankings ----------------------------- #

    def upsert_ranking(self, rows: Iterable[RankingRow]) -> int:
        rows = list(rows)
        if not rows:
            return 0
        sql = (
            "INSERT OR REPLACE INTO rankings(provider, snap_date, rank, cg_id, "
            "symbol, name, market_cap, price, fetched_at) VALUES (?,?,?,?,?,?,?,?,?)"
        )
        with self._conn() as con:
            con.executemany(
                sql,
                [
                    (
                        r.provider, r.snap_date, r.rank, r.cg_id, r.symbol,
                        r.name, r.market_cap, r.price, r.fetched_at.isoformat(),
                    )
                    for r in rows
                ],
            )
        return len(rows)

    def get_ranking(
        self,
        provider: str,
        snap_date: dt.date,
        limit: int | None = None,
    ) -> list[RankingRow]:
        sql = (
            "SELECT provider, snap_date, rank, cg_id, symbol, name, market_cap, "
            "price, fetched_at FROM rankings WHERE provider=? AND snap_date=? "
            "ORDER BY rank ASC"
        )
        params: tuple = (provider, snap_date)
        if limit is not None:
            sql += " LIMIT ?"
            params = (*params, int(limit))
        with self._conn() as con:
            rows = con.execute(sql, params).fetchall()
        out: list[RankingRow] = []
        for row in rows:
            out.append(
                RankingRow(
                    provider=row[0],
                    snap_date=row[1] if isinstance(row[1], dt.date) else dt.date.fromisoformat(row[1]),
                    rank=row[2],
                    cg_id=row[3],
                    symbol=row[4],
                    name=row[5],
                    market_cap=row[6],
                    price=row[7],
                    fetched_at=dt.datetime.fromisoformat(row[8]),
                )
            )
        return out

    def latest_ranking_date(self, provider: str) -> dt.date | None:
        with self._conn() as con:
            row = con.execute(
                "SELECT MAX(snap_date) FROM rankings WHERE provider=?", (provider,)
            ).fetchone()
        if row and row[0]:
            return row[0] if isinstance(row[0], dt.date) else dt.date.fromisoformat(row[0])
        return None

    def ranking_dates(self, provider: str) -> list[dt.date]:
        with self._conn() as con:
            rows = con.execute(
                "SELECT DISTINCT snap_date FROM rankings WHERE provider=? ORDER BY snap_date",
                (provider,),
            ).fetchall()
        return [r[0] if isinstance(r[0], dt.date) else dt.date.fromisoformat(r[0]) for r in rows]

    def is_ranking_fresh(
        self,
        provider: str,
        snap_date: dt.date,
        ttl_seconds: float,
    ) -> bool:
        with self._conn() as con:
            row = con.execute(
                "SELECT fetched_at FROM rankings WHERE provider=? AND snap_date=? LIMIT 1",
                (provider, snap_date),
            ).fetchone()
        if not row:
            return False
        fetched = dt.datetime.fromisoformat(row[0])
        return (dt.datetime.utcnow() - fetched).total_seconds() < ttl_seconds

    # ------------------------------ ohlcv ------------------------------- #

    def upsert_ohlcv(self, rows: Iterable[OhlcvRow]) -> int:
        rows = list(rows)
        if not rows:
            return 0
        sql = (
            "INSERT OR REPLACE INTO ohlcv(coin_key, day, open, high, low, close, "
            "volume, source, approx) VALUES (?,?,?,?,?,?,?,?,?)"
        )
        with self._conn() as con:
            con.executemany(
                sql,
                [
                    (
                        r.coin_key, r.day, r.open, r.high, r.low, r.close,
                        r.volume, r.source, 1 if r.approx else 0,
                    )
                    for r in rows
                ],
            )
        return len(rows)

    def get_ohlcv_range(
        self,
        coin_key: str,
        start: dt.date,
        end: dt.date,
    ) -> list[OhlcvRow]:
        sql = (
            "SELECT coin_key, day, open, high, low, close, volume, source, approx "
            "FROM ohlcv WHERE coin_key=? AND day BETWEEN ? AND ? ORDER BY day"
        )
        with self._conn() as con:
            rows = con.execute(sql, (coin_key, start, end)).fetchall()
        out: list[OhlcvRow] = []
        for r in rows:
            out.append(
                OhlcvRow(
                    coin_key=r[0],
                    day=r[1] if isinstance(r[1], dt.date) else dt.date.fromisoformat(r[1]),
                    open=r[2], high=r[3], low=r[4], close=r[5], volume=r[6],
                    source=r[7], approx=bool(r[8]),
                )
            )
        return out

    def ohlcv_coverage(self, coin_key: str) -> tuple[dt.date | None, dt.date | None]:
        with self._conn() as con:
            row = con.execute(
                "SELECT MIN(day), MAX(day) FROM ohlcv WHERE coin_key=?", (coin_key,)
            ).fetchone()
        if not row or not row[0]:
            return None, None
        lo = row[0] if isinstance(row[0], dt.date) else dt.date.fromisoformat(row[0])
        hi = row[1] if isinstance(row[1], dt.date) else dt.date.fromisoformat(row[1])
        return lo, hi

    # ------------------------- ohlcv (intraday) ------------------------- #

    def upsert_ohlcv_intraday(self, rows: Iterable[OhlcvBar]) -> int:
        rows = list(rows)
        if not rows:
            return 0
        sql = (
            "INSERT OR REPLACE INTO ohlcv_intraday(coin_key, tf, ts, open, high, "
            "low, close, volume, source, approx) VALUES (?,?,?,?,?,?,?,?,?,?)"
        )
        with self._conn() as con:
            con.executemany(
                sql,
                [
                    (
                        r.coin_key, r.tf, r.ts.isoformat(), r.open, r.high, r.low,
                        r.close, r.volume, r.source, 1 if r.approx else 0,
                    )
                    for r in rows
                ],
            )
        return len(rows)

    def get_ohlcv_intraday_range(
        self,
        coin_key: str,
        tf: str,
        start: dt.datetime,
        end: dt.datetime,
    ) -> list[OhlcvBar]:
        sql = (
            "SELECT coin_key, tf, ts, open, high, low, close, volume, source, approx "
            "FROM ohlcv_intraday WHERE coin_key=? AND tf=? AND ts BETWEEN ? AND ? "
            "ORDER BY ts"
        )
        with self._conn() as con:
            rows = con.execute(
                sql, (coin_key, tf, start.isoformat(), end.isoformat())
            ).fetchall()
        out: list[OhlcvBar] = []
        for r in rows:
            out.append(
                OhlcvBar(
                    coin_key=r[0], tf=r[1], ts=dt.datetime.fromisoformat(r[2]),
                    open=r[3], high=r[4], low=r[5], close=r[6], volume=r[7],
                    source=r[8], approx=bool(r[9]),
                )
            )
        return out

    def intraday_coverage(
        self, coin_key: str, tf: str
    ) -> tuple[dt.datetime | None, dt.datetime | None]:
        with self._conn() as con:
            row = con.execute(
                "SELECT MIN(ts), MAX(ts) FROM ohlcv_intraday WHERE coin_key=? AND tf=?",
                (coin_key, tf),
            ).fetchone()
        if not row or not row[0]:
            return None, None
        return dt.datetime.fromisoformat(row[0]), dt.datetime.fromisoformat(row[1])

    def intraday_bar_count(
        self, coin_key: str, tf: str, start: dt.datetime, end: dt.datetime
    ) -> int:
        """How many bars are cached inside a window (MIN/MAX hides holes)."""

        with self._conn() as con:
            row = con.execute(
                "SELECT COUNT(*) FROM ohlcv_intraday WHERE coin_key=? AND tf=? "
                "AND ts BETWEEN ? AND ?",
                (coin_key, tf, start.isoformat(), end.isoformat()),
            ).fetchone()
        return int(row[0]) if row else 0

    # ------------------------------ meta -------------------------------- #

    def get_meta(self, key: str) -> str | None:
        with self._conn() as con:
            row = con.execute("SELECT value FROM meta WHERE key=?", (key,)).fetchone()
        return row[0] if row else None

    def set_meta(self, key: str, value: str) -> None:
        with self._conn() as con:
            con.execute(
                "INSERT OR REPLACE INTO meta(key, value) VALUES (?, ?)", (key, value)
            )

    # ---------------------------- coin map ------------------------------ #

    def upsert_coin_map(
        self,
        cg_id: str,
        *,
        symbol: str | None = None,
        name: str | None = None,
        cmc_slug: str | None = None,
        binance_symbol: str | None = None,
        last_seen: dt.date | None = None,
    ) -> None:
        with self._conn() as con:
            self._upsert_coin_map_on(
                con, cg_id, symbol=symbol, name=name, cmc_slug=cmc_slug,
                binance_symbol=binance_symbol, last_seen=last_seen,
            )

    def upsert_coin_map_many(self, entries: Iterable[dict]) -> int:
        """Apply :meth:`upsert_coin_map` for many coins in one transaction.

        Each entry is a kwargs dict (``cg_id`` plus any optional column).
        Loading a 250-coin listing page one commit at a time is the bulk
        of the id-resolution cost.
        """

        entries = list(entries)
        if not entries:
            return 0
        with self._conn() as con:
            for entry in entries:
                self._upsert_coin_map_on(con, **entry)
        return len(entries)

    @staticmethod
    def _upsert_coin_map_on(
        con: sqlite3.Connection,
        cg_id: str,
        *,
        symbol: str | None = None,
        name: str | None = None,
        cmc_slug: str | None = None,
        binance_symbol: str | None = None,
        last_seen: dt.date | None = None,
    ) -> None:
        cur = con.execute(
            "SELECT symbol, name, cmc_slug, binance_symbol, last_seen "
            "FROM coin_map WHERE cg_id=?",
            (cg_id,),
        ).fetchone()
        if cur:
            cur_symbol, cur_name, cur_cmc, cur_binance, cur_seen = cur
            con.execute(
                "UPDATE coin_map SET symbol=?, name=?, cmc_slug=?, "
                "binance_symbol=?, last_seen=? WHERE cg_id=?",
                (
                    symbol or cur_symbol,
                    name or cur_name,
                    cmc_slug or cur_cmc,
                    binance_symbol or cur_binance,
                    last_seen or cur_seen,
                    cg_id,
                ),
            )
        else:
            con.execute(
                "INSERT INTO coin_map(cg_id, symbol, name, cmc_slug, "
                "binance_symbol, last_seen) VALUES (?,?,?,?,?,?)",
                (cg_id, symbol, name, cmc_slug, binance_symbol, last_seen),
            )

    def lookup_coin_map(self, cg_id: str) -> dict | None:
        with self._conn() as con:
            row = con.execute(
                "SELECT cg_id, symbol, name, cmc_slug, binance_symbol, last_seen "
                "FROM coin_map WHERE cg_id=?",
                (cg_id,),
            ).fetchone()
        if not row:
            return None
        return {
            "cg_id": row[0], "symbol": row[1], "name": row[2],
            "cmc_slug": row[3], "binance_symbol": row[4],
            "last_seen": row[5],
        }

    def lookup_cg_id_by_symbol(self, symbol: str) -> str | None:
        """Reverse lookup: ticker -> real CoinGecko id.

        Skips the ``cmc:``-prefixed pseudo-ids the CMC scraper writes, since
        those are CoinMarketCap slugs and are not valid CoinGecko ids
        (CMC's ``unus-sed-leo`` is CoinGecko's ``leo-token``).
        """

        if not symbol:
            return None
        with self._conn() as con:
            row = con.execute(
                "SELECT cg_id FROM coin_map "
                "WHERE UPPER(symbol)=? AND cg_id NOT LIKE 'cmc:%' "
                "ORDER BY last_seen IS NULL, last_seen DESC LIMIT 1",
                (symbol.upper(),),
            ).fetchone()
        return row[0] if row else None


_singleton: Cache | None = None
_singleton_lock = threading.Lock()


def get_default_cache() -> Cache:
    """Module-level singleton bound to ``config.default_cache_path()``."""

    global _singleton
    if _singleton is None:
        with _singleton_lock:
            if _singleton is None:
                from ..config import default_cache_path

                _singleton = Cache(default_cache_path())
    return _singleton
