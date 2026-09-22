"""Pure-Python technical indicators for QuantAlgo.

Every function uses only the standard library -- no numpy or pandas.
Edge-case convention: when the input series is shorter than the required
period the functions return a list padded with None values so the output
length always equals the input length.
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Union

# ---------------------------------------------------------------------------
# Simple Moving Average
# ---------------------------------------------------------------------------

def sma(series: List[float], period: int) -> List[Optional[float]]:
    """Return the simple moving average.

    Output length equals ``len(series)``.  The first ``period - 1``
    entries are ``None`` (not enough data yet).
    """
    if period <= 0:
        raise ValueError("period must be > 0")
    n = len(series)
    if n < period:
        return [None] * n

    result: List[Optional[float]] = [None] * (period - 1)
    window_sum = sum(series[:period])
    result.append(window_sum / period)

    for i in range(period, n):
        window_sum += series[i] - series[i - period]
        result.append(window_sum / period)

    return result


# ---------------------------------------------------------------------------
# Exponential Moving Average
# ---------------------------------------------------------------------------

def ema(series: List[float], period: int) -> List[Optional[float]]:
    """Return the exponential moving average.

    The first value is seeded with the SMA of the first *period* data
    points.  Preceding entries are ``None``.
    """
    if period <= 0:
        raise ValueError("period must be > 0")
    n = len(series)
    if n < period:
        return [None] * n

    k = 2.0 / (period + 1)
    result: List[Optional[float]] = [None] * (period - 1)

    # Seed with SMA
    seed = sum(series[:period]) / period
    result.append(seed)

    prev = seed
    for i in range(period, n):
        val = series[i] * k + prev * (1 - k)
        result.append(val)
        prev = val

    return result


# ---------------------------------------------------------------------------
# Relative Strength Index
# ---------------------------------------------------------------------------

def rsi(series: List[float], period: int = 14) -> List[Optional[float]]:
    """Return the RSI (0-100) using Wilder's smoothing method.

    The first ``period`` entries are ``None``.
    """
    if period <= 0:
        raise ValueError("period must be > 0")
    n = len(series)
    if n < period + 1:
        return [None] * n

    deltas = [series[i] - series[i - 1] for i in range(1, n)]

    gains = [max(d, 0.0) for d in deltas]
    losses = [max(-d, 0.0) for d in deltas]

    avg_gain = sum(gains[:period]) / period
    avg_loss = sum(losses[:period]) / period

    result: List[Optional[float]] = [None] * period

    if avg_loss == 0:
        result.append(100.0)
    else:
        rs = avg_gain / avg_loss
        result.append(100.0 - 100.0 / (1.0 + rs))

    for i in range(period, len(deltas)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0:
            result.append(100.0)
        else:
            rs = avg_gain / avg_loss
            result.append(100.0 - 100.0 / (1.0 + rs))

    return result


# ---------------------------------------------------------------------------
# MACD
# ---------------------------------------------------------------------------

def macd(
    series: List[float],
    fast: int = 12,
    slow: int = 26,
    signal: int = 9,
) -> Dict[str, List[Optional[float]]]:
    """Return MACD line, signal line and histogram.

    Returns a dict with keys ``'macd'``, ``'signal'``, ``'histogram'``,
    each a list the same length as *series*.
    """
    n = len(series)
    ema_fast = ema(series, fast)
    ema_slow = ema(series, slow)

    macd_line: List[Optional[float]] = []
    for f, s in zip(ema_fast, ema_slow):
        if f is None or s is None:
            macd_line.append(None)
        else:
            macd_line.append(f - s)

    # Build a dense sub-series of the non-None MACD values for the signal EMA
    macd_values = [v for v in macd_line if v is not None]
    signal_raw = ema(macd_values, signal) if len(macd_values) >= signal else [None] * len(macd_values)

    # Map signal_raw back into the full-length list
    signal_line: List[Optional[float]] = []
    idx = 0
    for v in macd_line:
        if v is None:
            signal_line.append(None)
        else:
            signal_line.append(signal_raw[idx])
            idx += 1

    histogram: List[Optional[float]] = []
    for m, s in zip(macd_line, signal_line):
        if m is None or s is None:
            histogram.append(None)
        else:
            histogram.append(m - s)

    return {"macd": macd_line, "signal": signal_line, "histogram": histogram}


# ---------------------------------------------------------------------------
# Bollinger Bands
# ---------------------------------------------------------------------------

def bollinger(
    series: List[float],
    period: int = 20,
    std: float = 2.0,
) -> Dict[str, List[Optional[float]]]:
    """Return Bollinger Bands (upper, middle, lower).

    ``middle`` is the SMA; ``upper`` / ``lower`` are ``middle +/- std * stdev``.
    """
    n = len(series)
    middle = sma(series, period)

    upper: List[Optional[float]] = [None] * n
    lower: List[Optional[float]] = [None] * n

    for i in range(period - 1, n):
        window = series[i - period + 1 : i + 1]
        mean = middle[i]
        if mean is None:
            continue
        variance = sum((x - mean) ** 2 for x in window) / period
        sd = math.sqrt(variance)
        upper[i] = mean + std * sd
        lower[i] = mean - std * sd

    return {"upper": upper, "middle": middle, "lower": lower}


# ---------------------------------------------------------------------------
# Average True Range
# ---------------------------------------------------------------------------

def atr(
    candles: List[Union[dict, object]],
    period: int = 14,
) -> List[Optional[float]]:
    """Return the Average True Range.

    *candles* is a list of dicts (or objects) with keys/attrs
    ``high``, ``low``, ``close``.
    """
    def _get(c, attr):
        if isinstance(c, dict):
            return float(c[attr])
        return float(getattr(c, attr))

    n = len(candles)
    if n == 0:
        return []

    true_ranges: List[float] = [_get(candles[0], "high") - _get(candles[0], "low")]
    for i in range(1, n):
        high = _get(candles[i], "high")
        low = _get(candles[i], "low")
        prev_close = _get(candles[i - 1], "close")
        tr = max(high - low, abs(high - prev_close), abs(low - prev_close))
        true_ranges.append(tr)

    if n < period:
        return [None] * n

    result: List[Optional[float]] = [None] * (period - 1)
    first_atr = sum(true_ranges[:period]) / period
    result.append(first_atr)

    prev_atr = first_atr
    for i in range(period, n):
        current_atr = (prev_atr * (period - 1) + true_ranges[i]) / period
        result.append(current_atr)
        prev_atr = current_atr

    return result
