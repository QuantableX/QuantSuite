"""Indicator primitives used by the rotation signal.

RotationLab only needs the 12/21 EMA band cross (from
``quantfolio.pine`` -> ``f_ema_cross``). Everything else has been
removed.
"""

from __future__ import annotations

import pandas as pd


def _series(s) -> pd.Series:
    if not isinstance(s, pd.Series):
        s = pd.Series(s)
    return s.astype("float64")


def ema(source: pd.Series, length: int) -> pd.Series:
    """Exponential moving average matching Pine's ``ta.ema``.

    Pine's ``ta.ema`` uses ``alpha = 2 / (length + 1)`` and is seeded
    by the first non-NA value. pandas' ``ewm(span=length, adjust=False)``
    is the standard equivalent.
    """

    source = _series(source)
    return source.ewm(span=int(length), adjust=False).mean()


def resolve_source(df: pd.DataFrame, src: str) -> pd.Series:
    """Return the requested price series from an OHLC frame.

    Mirrors Pine's ``input.string("close", ..., options=[...])`` choices.
    """

    src = src.lower()
    if src == "open": return df["open"].astype("float64")
    if src == "high": return df["high"].astype("float64")
    if src == "low": return df["low"].astype("float64")
    if src == "close": return df["close"].astype("float64")
    if src == "hl2": return ((df["high"] + df["low"]) / 2.0).astype("float64")
    if src == "hlc3":
        return ((df["high"] + df["low"] + df["close"]) / 3.0).astype("float64")
    if src == "ohlc4":
        return ((df["open"] + df["high"] + df["low"] + df["close"]) / 4.0).astype("float64")
    return df["close"].astype("float64")
