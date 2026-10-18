"""Data shelf: ccxt (crypto, paginated), cached as CSV in `Price History/`.
Files: <EXCHANGE>_<SYMBOL>_<TF>.csv  — see the vault's folder note.

The CSVs stay the forge's own shelf: certification reads exactly them, and
every series on it (axis_exchange takes all BTC venues), so nothing from
outside may appear there. Their candles are also mirrored into the suite's
shared candle archive (`candle_archive.py`), where QuantAlgo backtests on the
same venue, pair and timeframe read them instead of downloading them again.

Where the shelf, the reports and the docs live (PLAN-QUANTALGO §6):
`$SMITHERY_VAULT` when set, else the Obsidian vault "Indicator Smithery"
when it exists on this machine, else a private forge under the suite's
QuantAlgo data directory. The code no longer lives next to the shelf.
"""
from __future__ import annotations

import os
import time
from pathlib import Path

import numpy as np
import pandas as pd

DEFAULT_VAULT = Path("C:/Projects/Obsidian/Indicator Smithery")


def _resolve_root() -> Path:
    env = os.environ.get("SMITHERY_VAULT")
    if env:
        return Path(env)
    if DEFAULT_VAULT.is_dir():
        return DEFAULT_VAULT
    home = os.environ.get("QUANTSUITE_HOME")
    base = Path(home) if home else Path.home() / ".quantsuite"
    return base / "modules" / "algo" / "smithery"


ROOT = _resolve_root()                              # the vault (or the private forge)
PRICE_DIR = ROOT / "Price History"
OUTPUT_DIR = ROOT / "Output"
DOCS_DIR = ROOT / "Docs"

# (exchange, symbol, timeframe, since_iso) — one row per cell of the robustness grid.
# Asset shelf = top-5 crypto by market cap, stablecoins & staked versions excluded:
# BTC, ETH, XRP, BNB, SOL. Three certification tracks: the
# daily reference track, and the 4h / 1h intraday tracks (the top-5 on
# Binance for the asset axis, BTC on Bybit / OKX / KuCoin for the exchange
# axis — Bitstamp and Coinbase serve the daily exchange axis only).
SHELF_CRYPTO = [
    ("binance",  "BTC/USDT", "1d", "2017-08-01"),
    ("binance",  "ETH/USDT", "1d", "2017-08-01"),
    ("binance",  "XRP/USDT", "1d", "2018-05-01"),
    ("binance",  "BNB/USDT", "1d", "2017-11-01"),
    ("binance",  "SOL/USDT", "1d", "2020-08-01"),
    ("bitstamp", "BTC/USD",  "1d", "2014-01-01"),
    ("coinbase", "BTC/USD",  "1d", "2016-01-01"),
    # the 4h track
    ("binance",  "BTC/USDT", "4h", "2017-08-01"),
    ("binance",  "ETH/USDT", "4h", "2017-08-01"),
    ("binance",  "XRP/USDT", "4h", "2018-05-01"),
    ("binance",  "BNB/USDT", "4h", "2017-11-01"),
    ("binance",  "SOL/USDT", "4h", "2020-08-01"),
    ("bybit",    "BTC/USDT", "4h", "2018-11-01"),
    ("okx",      "BTC/USDT", "4h", "2018-01-01"),
    ("kucoin",   "BTC/USDT", "4h", "2018-01-01"),
    # the 1h track
    ("binance",  "BTC/USDT", "1h", "2017-08-01"),
    ("binance",  "ETH/USDT", "1h", "2017-08-01"),
    ("binance",  "XRP/USDT", "1h", "2018-05-01"),
    ("binance",  "BNB/USDT", "1h", "2017-11-01"),
    ("binance",  "SOL/USDT", "1h", "2020-08-01"),
    ("bybit",    "BTC/USDT", "1h", "2018-11-01"),
    ("okx",      "BTC/USDT", "1h", "2018-01-01"),
    ("kucoin",   "BTC/USDT", "1h", "2018-01-01"),
]

# geo-block / outage fallbacks for the primary crypto venue
BINANCE_FALLBACKS = ["okx", "bybit", "kucoin"]


def _key(exchange: str, symbol: str, tf: str) -> str:
    return f"{exchange.upper()}_{symbol.replace('/', '').replace('=', '').replace('-', '')}_{tf}"


def csv_path(key: str) -> Path:
    return PRICE_DIR / f"{key}.csv"


# ---------------------------------------------------------------- shared archive

# Venues and timeframes whose ccxt candles were checked bar for bar against
# QuantAlgo's own pagers (2026-10-18, 2023-2024 on 14 series: open, high,
# low, close and volume identical). Not mirrored: OKX "1d" (QuantAlgo's daily
# bars open at 16:00 UTC, OKX's Hong Kong day; the shelf's at 00:00 UTC),
# KuCoin (ccxt rounds a few long volume strings one ulp differently) and
# Bitstamp (no QuantAlgo pager).
ARCHIVE_VERIFIED = {
    "binance": {"1d", "4h", "1h"},
    "bybit": {"1d", "4h", "1h"},
    "okx": {"4h", "1h"},
    "coinbase": {"1d"},
}
_QUOTES = ("USDT", "USDC", "USD", "EUR", "BTC")
_STEP_MS = {"1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000}


def archive_series(key: str) -> tuple[str, str, str] | None:
    """(venue, BASE/QUOTE, tf) of a shelf key, or None when the archive must
    not hold it."""
    parts = key.split("_")
    if len(parts) != 3:
        return None
    venue, compact, tf = parts[0].lower(), parts[1], parts[2]
    if tf not in ARCHIVE_VERIFIED.get(venue, ()):
        return None
    for quote in _QUOTES:
        if compact.endswith(quote) and len(compact) > len(quote):
            return venue, f"{compact[:-len(quote)]}/{quote}", tf
    return None


def _epoch_ms(stamps) -> pd.Series:
    """UTC timestamps to epoch milliseconds, whatever resolution pandas chose."""
    index = stamps.index if isinstance(stamps, pd.Series) else None
    ts = pd.Series(pd.to_datetime(stamps, utc=True), index=index)
    return (ts - pd.Timestamp(0, tz="UTC")) // pd.Timedelta(milliseconds=1)


def _csv_ends(path: Path) -> tuple[int, int] | None:
    """First and last bar of a shelf CSV in epoch ms, from its first and last
    lines only."""
    with path.open("rb") as fh:
        fh.readline()
        first = fh.readline()
        if not first.strip():
            return None
        fh.seek(0, os.SEEK_END)
        fh.seek(max(fh.tell() - 4096, 0))
        last = [ln for ln in fh.read().split(b"\n") if ln.strip()][-1]
    ends = _epoch_ms([line.split(b",", 1)[0].decode() for line in (first, last)])
    return int(ends.iloc[0]), int(ends.iloc[-1])


def mirror_to_archive(key: str, archive=None) -> int:
    """Make the shared archive cover this shelf series. Only the parts of its
    span that no earlier mirror recorded are read and stored, with an exact
    float round trip from the CSV. Returns the bars stored; 0 when already
    covered, not verified for the archive, or missing."""
    series, path = archive_series(key), csv_path(key)
    if series is None or not path.exists():
        return 0
    from .candle_archive import CandleArchive, covered, missing

    own = archive is None
    archive = archive if archive is not None else CandleArchive()
    try:
        ends = _csv_ends(path)
        gaps = missing(archive.spans(*series), *ends) if ends else []
        if not gaps:
            return 0
        df = pd.read_csv(path, float_precision="round_trip")
        ms = _epoch_ms(df["timestamp"])
        stored = 0
        for start, end in gaps:
            part = df[(ms >= start) & (ms <= end)]
            opens = ms[part.index].tolist()
            rows = list(zip(opens, *(part[c].astype(float).tolist()
                                     for c in ("open", "high", "low", "close", "volume"))))
            first = archive.first_open(*series)
            spans = covered(start, end, opens, _STEP_MS[series[2]], first is None or first > end)
            stored += archive.record(*series, rows, spans)
        return stored
    finally:
        if own:
            archive.close()


def mirror_shelf(log=None) -> int:
    """Mirror every shelf CSV into the shared archive; failures only log —
    the archive is an optimisation, never a reason for a refresh to fail."""
    try:
        from .candle_archive import CandleArchive
        archive = CandleArchive()
    except Exception as e:  # noqa: BLE001
        if log:
            log(f"  candle archive unavailable ({type(e).__name__}: {e})")
        return 0
    stored = 0
    try:
        for key in available():
            try:
                stored += mirror_to_archive(key, archive)
            except Exception as e:  # noqa: BLE001
                if log:
                    log(f"  {key}: not mirrored to the candle archive ({type(e).__name__}: {str(e)[:120]})")
    finally:
        archive.close()
    if log and stored:
        log(f"  candle archive: {stored:,} bars mirrored to {archive.path}")
    return stored


def load(key: str) -> pd.DataFrame:
    """Load a cached series as a UTC-indexed OHLCV frame."""
    df = pd.read_csv(csv_path(key), parse_dates=["timestamp"])
    df["timestamp"] = pd.to_datetime(df["timestamp"], utc=True)
    df = df.set_index("timestamp").sort_index()
    return df[~df.index.duplicated(keep="last")]


def available() -> list[str]:
    if not PRICE_DIR.is_dir():
        return []
    return sorted(p.stem for p in PRICE_DIR.glob("*.csv"))


def _save(key: str, df: pd.DataFrame) -> None:
    PRICE_DIR.mkdir(parents=True, exist_ok=True)
    out = df.copy()
    out.index.name = "timestamp"
    out.reset_index().to_csv(csv_path(key), index=False)


# ---------------------------------------------------------------- ccxt

def fetch_ccxt(exchange_id: str, symbol: str, tf: str, since_iso: str,
               max_pages: int | None = None, progress=None) -> pd.DataFrame:
    import ccxt
    max_pages = 400 if max_pages is None else max_pages
    if max_pages < 1:
        raise ValueError("max_pages must be positive")
    ex = getattr(ccxt, exchange_id)({"enableRateLimit": True, "timeout": 20000})
    ex.load_markets()
    if tf not in ex.timeframes:
        raise ValueError(f"{exchange_id} does not support {tf} candles")
    start = pd.Timestamp(since_iso)
    start = start.tz_localize("UTC") if start.tzinfo is None else start.tz_convert("UTC")
    since = int(start.timestamp() * 1000)
    bar_ms = int(ex.parse_timeframe(tf) * 1000)
    # Freeze the last closed-bar boundary at the start of the fetch.
    closed_before = ex.milliseconds() // bar_ms * bar_ms
    rows, last = [], -1
    for page in range(max_pages):
        batch = ex.fetch_ohlcv(symbol, tf, since=since, limit=1000)
        if not batch:
            break
        batch = sorted((r for r in batch if r[0] >= since and r[0] > last), key=lambda r: r[0])
        if not batch:
            break
        rows.extend(r for r in batch if r[0] < closed_before)
        last = batch[-1][0]
        since = last + bar_ms
        if progress and (page == 0 or (page + 1) % 25 == 0):
            progress(page + 1, len(rows), pd.Timestamp(last, unit="ms", tz="UTC"))
        if since >= closed_before:
            break
        time.sleep(getattr(ex, "rateLimit", 200) / 1000)
    else:
        raise RuntimeError(f"{exchange_id} {symbol} {tf}: reached {max_pages} pages before the live edge; incomplete history was not saved")
    if not rows:
        raise RuntimeError(f"{exchange_id} returned no data for {symbol} {tf}")
    df = pd.DataFrame(rows, columns=["ts", "open", "high", "low", "close", "volume"])
    df["timestamp"] = pd.to_datetime(df["ts"], unit="ms", utc=True)
    df = df.drop(columns="ts").set_index("timestamp").sort_index()
    df = df[~df.index.duplicated(keep="last")]
    return df  # only bars closed before the frozen boundary were collected


# ---------------------------------------------------------------- refresh

def refresh(verbose: bool = True, log=None, timeframe: str = "all") -> dict[str, str]:
    """Fetch/refresh the whole shelf. Incremental where cached. Returns
    {key: status}; individual failures never abort the run. `log` receives
    every progress line (the suite's Smithery page); `verbose` prints them."""
    results: dict[str, str] = {}
    supported = {row[2] for row in SHELF_CRYPTO}
    if timeframe != "all" and timeframe not in supported:
        raise ValueError(f"Unknown shelf timeframe: {timeframe}")
    listener = log

    def log(msg):
        if verbose:
            print(msg, flush=True)
        if listener is not None:
            listener(msg)

    for ex_id, sym, tf, since in SHELF_CRYPTO:
        if timeframe != "all" and tf != timeframe:
            continue
        candidates = [ex_id] + (BINANCE_FALLBACKS if ex_id == "binance" else [])
        done = False
        for cand in candidates:
            key = _key(cand, sym, tf)
            try:
                eff_since = since
                if csv_path(key).exists():
                    cached = load(key)
                    if not cached.empty:
                        eff_since = cached.index[-1].isoformat()
                log(f"  {key}: fetching from {eff_since}")
                df = fetch_ccxt(cand, sym, tf, eff_since,
                                progress=lambda page, bars, stamp: log(f"  {key}: page {page}, {bars:,} bars through {stamp}"))
                if csv_path(key).exists():
                    old = load(key)
                    df = pd.concat([old, df])
                    df = df[~df.index.duplicated(keep="last")].sort_index()
                _save(key, df)
                results[key] = f"ok ({len(df)} bars, {df.index[0].date()} -> {df.index[-1].date()})"
                log(f"  {key}: {results[key]}")
                done = True
                break
            except Exception as e:  # noqa: BLE001 — venue failures must not abort the shelf
                log(f"  {key}: FAILED ({type(e).__name__}: {str(e)[:120]}) - trying fallback")
        if not done:
            results[_key(ex_id, sym, tf)] = "failed on all venues"

    mirror_shelf(log)
    return results


def asset_class(key: str) -> str:
    return "crypto"
