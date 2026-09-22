"""Data shelf: ccxt (crypto, paginated), cached as CSV in `Price History/`.
Files: <EXCHANGE>_<SYMBOL>_<TF>.csv  — see the vault's folder note.

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
# BTC, ETH, XRP, BNB, SOL. Four certification tracks: the
# daily reference track, and the 4h / 1h / 1m intraday tracks (the top-5 on
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
    # Real minute candles, kept separate from hourly data. Multi-year
    # history is required by the existing chronological robustness checks.
    *[("binance", symbol, "1m", "2023-01-01") for symbol in
      ("BTC/USDT", "ETH/USDT", "XRP/USDT", "BNB/USDT", "SOL/USDT")],
    *[(venue, "BTC/USDT", "1m", "2023-01-01") for venue in ("bybit", "okx", "kucoin")],
]

# geo-block / outage fallbacks for the primary crypto venue
BINANCE_FALLBACKS = ["okx", "bybit", "kucoin"]


def _key(exchange: str, symbol: str, tf: str) -> str:
    return f"{exchange.upper()}_{symbol.replace('/', '').replace('=', '').replace('-', '')}_{tf}"


def csv_path(key: str) -> Path:
    return PRICE_DIR / f"{key}.csv"


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
    max_pages = (10_000 if tf == "1m" else 400) if max_pages is None else max_pages
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

    return results


def asset_class(key: str) -> str:
    return "crypto"
