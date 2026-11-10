"""Price variants for robustness runs: the saved candles, changed the way another market would.

Each variant wraps the saved-data reader (``lab_compare.SavedPrices``), so the
unchanged engine runs on it. Every change is deterministic per coin and day —
the same candle whatever window a run asks for — so a run's warm-up and its
window always see one consistent market.

* ``noise``: every open, high, low and close × (1 + N(0, sigma)), seeded per coin;
  high and low stay outside open and close.
* ``venue``: another exchange's candle on every day that exchange has one
  (``prices.prepare_venue`` saves them); every other day stays the saved one, so
  the coins and days stay the same and only the prices differ.
* ``shuffle``: the calendar days of the backtest window are permuted for every
  coin at once (the same permutation for all, so the cross-section stays intact);
  a coin without a candle on the drawn day takes one of its own window days.
  Each coin's candle shapes stay; their order in time is gone — the luck test.
"""
from __future__ import annotations

import datetime as dt
import hashlib
from pathlib import Path

import numpy as np
import pandas as pd

ORIGIN = dt.date(1970, 1, 1)
SPAN_DAYS = 80 * 366  # 1970..2050
_OHLC = ["open", "high", "low", "close"]


def _seed(*parts) -> int:
    return int.from_bytes(hashlib.sha256("|".join(map(str, parts)).encode()).digest()[:8], "little")


def _ordinals(index) -> np.ndarray:
    days = pd.DatetimeIndex(index)
    if days.tz is not None:
        days = days.tz_convert("UTC").tz_localize(None)
    return (days.normalize() - pd.Timestamp(ORIGIN)).days.to_numpy()


class PricesVariant:
    """``SavedPrices`` with one deterministic change of the daily candles."""

    def __init__(self, base, spec: dict | None):
        self.base = base
        self.spec = dict(spec or {})
        self.kind = self.spec.get("kind")
        self._noise: dict[str, np.ndarray] = {}
        self._shuffled: dict[str, pd.DataFrame] = {}
        self._venue = None
        if self.kind == "venue":
            from rotation_lab.data.cache import Cache
            self._venue = Cache(Path(self.spec["path"]))
        if self.kind not in (None, "noise", "venue", "shuffle"):
            raise ValueError(f"Unknown price variant: {self.kind}")

    # ------------------------------------------------------------- reader API
    def get_series(self, ref, start, end, timeframe="1d"):
        from rotation_lab.data.ohlcv import OhlcvSeries, coin_key_for
        if self.kind == "shuffle" and timeframe == "1d":
            key = coin_key_for(ref)
            full = self._shuffled_frame(ref, key)
            return OhlcvSeries(coin_ref=ref, frame=_window(full, start, end))
        series = self.base.get_series(ref, start, end, timeframe)
        return OhlcvSeries(coin_ref=ref, frame=self._change(coin_key_for(ref), series.frame, start, end, timeframe))

    def get_pair_series(self, exchange, pair, start, end, timeframe="1d"):
        from rotation_lab.data.ohlcv import CoinRef, OhlcvSeries
        series = self.base.get_pair_series(exchange, pair, start, end, timeframe)
        key = f"market:{exchange}:{pair}"
        frame = series.frame
        if self.kind == "venue" and frame.empty:
            frame = self._venue_frame(key, start, end, timeframe)
        return OhlcvSeries(CoinRef(None, pair, cache_key=key), self._change(key, frame, start, end, timeframe))

    def get_path(self, ref, start, end, timeframe="1d"):
        """The saved path of a coin's bars. A shuffled world has none: its days are no longer the days the
        finer candles belong to, so its coin-vs-coin candles are the stand-in from the shuffled candles."""
        if self.kind == "shuffle":
            return None
        return self.base.get_path(ref, start, end, timeframe)

    def __getattr__(self, name):
        return getattr(self.base, name)

    # ---------------------------------------------------------------- changes
    def _change(self, key: str, frame: pd.DataFrame, start, end, timeframe: str) -> pd.DataFrame:
        if frame is None or frame.empty or self.kind is None:
            return frame
        if self.kind == "noise":
            return self._noisy(key, frame)
        if self.kind == "venue":
            other = self._venue_frame(key, start, end, timeframe)
            if other.empty:
                return frame
            frame = frame.copy()
            common = frame.index.intersection(other.index)
            cols = [c for c in (*_OHLC, "volume") if c in frame.columns and c in other.columns]
            frame.loc[common, cols] = other.loc[common, cols].to_numpy()
            return frame
        return frame

    def _noisy(self, key: str, frame: pd.DataFrame) -> pd.DataFrame:
        sigma = float(self.spec.get("sigma", 0.0003))
        if key not in self._noise:
            rng = np.random.default_rng(_seed("noise", self.spec.get("seed", 1), key))
            self._noise[key] = rng.standard_normal((SPAN_DAYS, 4))
        z = self._noise[key][np.clip(_ordinals(frame.index), 0, SPAN_DAYS - 1)]
        out = frame.copy()
        o, h, l, c = (out[col].to_numpy(dtype=float) * (1 + sigma * z[:, i]) for i, col in enumerate(_OHLC))
        out["open"], out["close"] = o, c
        out["high"] = np.maximum.reduce([h, o, c])
        out["low"] = np.minimum.reduce([l, o, c])
        return out

    def _venue_frame(self, key: str, start, end, timeframe: str) -> pd.DataFrame:
        from rotation_lab.data.ohlcv import _bars_to_frame, _rows_to_frame
        if timeframe == "1d":
            return _rows_to_frame(self._venue.get_ohlcv_range(key, start, end))
        from .lab_compare import bar_window
        return _bars_to_frame(self._venue.get_ohlcv_intraday_range(key, timeframe, *bar_window(start, end, timeframe)))

    def _shuffled_frame(self, ref, key: str) -> pd.DataFrame:
        if key in self._shuffled:
            return self._shuffled[key]
        lo = dt.date.fromisoformat(self.spec["start"])
        hi = dt.date.fromisoformat(self.spec["end"])
        full = self.base.get_series(ref, dt.date(2000, 1, 1), hi, "1d").frame
        self._shuffled[key] = shuffle_window(full, lo, hi, self.spec.get("seed", 1))
        return self._shuffled[key]


def _window(frame: pd.DataFrame, start, end) -> pd.DataFrame:
    if frame.empty:
        return frame
    days = pd.DatetimeIndex(frame.index)
    lo, hi = pd.Timestamp(start), pd.Timestamp(end)
    if days.tz is not None:
        lo, hi = lo.tz_localize(days.tz), hi.tz_localize(days.tz)
    return frame.loc[(days >= lo) & (days <= hi + pd.Timedelta(hours=23, minutes=59))]


def day_permutation(lo: dt.date, hi: dt.date, seed) -> dict[int, int]:
    """target day ordinal -> source day ordinal, one permutation of the window for every coin."""
    days = np.arange((lo - ORIGIN).days, (hi - ORIGIN).days + 1)
    rng = np.random.default_rng(_seed("shuffle", seed))
    return dict(zip(days.tolist(), rng.permutation(days).tolist()))


def shuffle_window(frame: pd.DataFrame, lo: dt.date, hi: dt.date, seed) -> pd.DataFrame:
    """A coin's candles with the window's days permuted (bars before the window unchanged).

    Each window candle keeps the coin's own open/high/low/close relative to its
    previous close on the source day; closes are chained from the last close
    before the window, so the coin's path is a reordering of its own moves."""
    if frame.empty:
        return frame
    out = frame.copy()
    ords = _ordinals(out.index)
    close = out["close"].to_numpy(dtype=float)
    prev = np.r_[np.nan, close[:-1]]
    ratios = np.column_stack([out[c].to_numpy(dtype=float) / prev for c in _OHLC])
    in_window = (ords >= (lo - ORIGIN).days) & (ords <= (hi - ORIGIN).days)
    rows = np.flatnonzero(in_window)
    if len(rows) < 2:
        return out
    position = {int(o): i for i, o in enumerate(ords)}
    usable = [i for i in rows if np.isfinite(ratios[i]).all()]
    if not usable:
        return out
    mapping = day_permutation(lo, hi, seed)
    fallback = np.random.default_rng(_seed("fallback", seed, len(rows))).integers(0, len(usable), size=len(rows))
    first = rows[0]
    level = close[first - 1] if first > 0 else close[first]
    new = out[_OHLC].to_numpy(dtype=float).copy()
    for n, i in enumerate(rows):
        if i == 0 or not np.isfinite(prev[i]):
            level = close[i]
            continue
        src = position.get(mapping.get(int(ords[i]), -1))
        if src is None or src not in usable:
            src = usable[fallback[n]]
        new[i] = level * ratios[src]
        level = new[i, 3]
    out[_OHLC] = new
    out["high"] = np.maximum.reduce([out["high"], out["open"], out["close"]])
    out["low"] = np.minimum.reduce([out["low"], out["open"], out["close"]])
    return out


def venue_cache_path(chain: tuple[str, ...]) -> Path:
    """Where another venue chain's candles are kept, next to the saved systems cache."""
    from .lab import systems_cache
    return systems_cache().parent / "venues" / ("-".join(chain) + ".sqlite")


DEFAULT_VENUES = ("okx", "bybit", "kucoin", "coinbase")
NEVER_VENUES = {"binance", "gate", "gateio", "mexc"}


def prepare_venue(config, chain: tuple[str, ...] = DEFAULT_VENUES, progress=None) -> Path:
    """Download the candles a system reads from another exchange chain into its own cache.

    Runs the system once with the normal downloader pointed at that chain (the saved
    rankings, the venue cache for candles); later runs add only missing days. The
    saved systems cache is never written. Binance, Gate.io and MEXC are never used."""
    from rotation_lab.backtest.engine import BacktestEngine
    from rotation_lab.data.cache import Cache
    from rotation_lab.data.ohlcv import OhlcvFetcher
    from .lab import systems_cache
    from .lab_compare import saved_rankings
    chain = tuple(chain)
    if not chain or NEVER_VENUES & set(chain):
        raise ValueError("The venue chain must name other exchanges than Binance, Gate.io and MEXC")
    path = venue_cache_path(chain)
    path.parent.mkdir(parents=True, exist_ok=True)
    saved, venue = Cache(systems_cache()), Cache(path)
    try:
        if progress:
            progress(f"Loading candles from {', '.join(chain)} for the venue check")
        BacktestEngine(registry=saved_rankings(saved),
                       ohlcv=OhlcvFetcher(venue, ccxt_exchanges=chain,
                                          min_request_interval=config.min_request_interval)).run(config)
    finally:
        for cache in (saved, venue):
            con = getattr(cache, "_con", None)
            if con is not None:
                con.close()
    return path
