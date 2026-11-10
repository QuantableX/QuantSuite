"""The candle of an A/B ratio: one rule for the engine, the bulk evaluators, today's live signal and
certification.

A coin-vs-coin series has no candles of its own; they are built from the two coins' candles. Open and
close are the ratios of the opens and of the closes. High and low are the highest and lowest value the
ratio had along the bar, read from the two coins' closes at every hour inside it (its *path*) together
with its open and close. A coin's candles carry that path as columns ``path_01`` .. ``path_NN`` (the
close of each hour of the bar, in order).

Where a bar has no complete path for both coins, a stand-in is used: high(A)/high(B) and low(A)/low(B),
widened to hold open and close. Coins mostly peak and bottom together, so this is close: measured against
the real daily high and low of eight pairs (1h candles, 2022-12 to 2026-08) it is off by 0.25% of the close
with 0.82 of the real range. The widest the ratio could have been, high(A)/low(B) and low(A)/high(B), which
the engine used until 2026-11-10, was off by 2.9% with 3.3 times the range and the close always
mid-candle; the candle's body alone, which certification used, by 0.4% with 0.55 of the range and no wicks.

The path is sampled at the hour, so a move inside one hour is not in it.
"""
from __future__ import annotations

import numpy as np
import pandas as pd

PATH_PREFIX = "path_"
HOUR = pd.Timedelta(hours=1)


def path_names(steps: int) -> list[str]:
    return [f"{PATH_PREFIX}{k:02d}" for k in range(1, steps + 1)]


def path_columns(frame: pd.DataFrame) -> list[str]:
    """The path columns a candle frame carries, in order."""
    return sorted(c for c in frame.columns if isinstance(c, str) and c.startswith(PATH_PREFIX))


def without_path(frame: pd.DataFrame) -> pd.DataFrame:
    """The frame as an indicator gets it: candles only."""
    columns = path_columns(frame)
    return frame.drop(columns=columns) if columns else frame


def paths_from_fine(index: pd.DatetimeIndex, fine_close: pd.Series, steps: int, step: pd.Timedelta = HOUR) -> pd.DataFrame:
    """For bars opening at ``index``: the closes of the ``steps`` finer bars inside each, as path columns.

    ``fine_close`` is indexed by the finer bars' opening times. A bar with a missing or non-positive
    finer close gets no path at all (NaN in every column), so it is either whole or absent."""
    closes = pd.Series(fine_close, dtype=float)
    closes = closes[~closes.index.duplicated(keep="last")]
    values = np.column_stack([closes.reindex(index + k * step).to_numpy(dtype=float) for k in range(steps)]) if len(index) else \
        np.empty((0, steps))
    whole = np.isfinite(values).all(axis=1) & (values > 0).all(axis=1)
    values[~whole] = np.nan
    return pd.DataFrame(values, index=index, columns=path_names(steps))


def real_bars(a: pd.DataFrame, b: pd.DataFrame) -> pd.Series:
    """On the common bars of two coins: whether both carry a whole path, so the ratio's candle is real."""
    joined = a.join(b, how="inner", rsuffix="_b")
    columns = [c for c in path_columns(a) if c in b.columns]
    if joined.empty or not columns:
        return pd.Series(False, index=joined.index)
    both = joined[columns + [c + "_b" for c in columns]].to_numpy(dtype=float)
    return pd.Series(np.isfinite(both).all(axis=1) & (both > 0).all(axis=1), index=joined.index)


def ratio_candles(a: pd.DataFrame, b: pd.DataFrame) -> pd.DataFrame:
    """The open, close, high and low of ``a / b`` on their common bars (see the module's description)."""
    joined = a.join(b, how="inner", rsuffix="_b")
    if joined.empty:
        return joined.iloc[:0][["open", "high", "low", "close"]]
    out = pd.DataFrame(index=joined.index)
    out["open"] = joined["open"] / joined["open_b"]
    out["close"] = joined["close"] / joined["close_b"]
    o, c = out["open"].to_numpy(dtype=float), out["close"].to_numpy(dtype=float)
    high = np.maximum(np.maximum((joined["high"] / joined["high_b"]).to_numpy(dtype=float), o), c)
    low = np.minimum(np.minimum((joined["low"] / joined["low_b"]).to_numpy(dtype=float), o), c)
    columns = [name for name in path_columns(a) if name in b.columns]
    if columns:
        pa = joined[columns].to_numpy(dtype=float)
        pb = joined[[name + "_b" for name in columns]].to_numpy(dtype=float)
        whole = np.isfinite(pa).all(axis=1) & np.isfinite(pb).all(axis=1) & (pa > 0).all(axis=1) & (pb > 0).all(axis=1)
        if whole.any():
            with np.errstate(divide="ignore", invalid="ignore"):
                path = pa / pb
            # The ratio at every hour's close, with the bar's own open and close.
            top = np.where(whole[:, None], path, -np.inf).max(axis=1)
            bottom = np.where(whole[:, None], path, np.inf).min(axis=1)
            high = np.where(whole, np.maximum(np.maximum(top, o), c), high)
            low = np.where(whole, np.minimum(np.minimum(bottom, o), c), low)
    out["high"] = high
    out["low"] = low
    return out
