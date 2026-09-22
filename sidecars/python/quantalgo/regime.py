"""Regime signals from the Indicator Smithery, for strategies.

The forge (``sidecars/python/smithery``, PLAN-QUANTALGO §6) forges causal
binary trend classifiers: +1 in an uptrend regime, -1 in a downtrend regime,
0 only while the indicator warms up. This module turns a strategy's candle
history into the OHLCV frame those indicators expect and hands back the
regime, so a strategy can write ``self.regime("hilbert")`` on every candle.

The indicator is recomputed over the available history. Prefix-causality
checks hold for the same starting history; the runner's capped history can
introduce differences from a full-history backtest. Generated strategies
wait for the registry warm-up before trading. The raw series here retains
the detector's own commitment point for compatibility with research.
"""

from __future__ import annotations

from typing import Any, Dict, Iterable, List, Optional


def candles_to_frame(candles: Iterable[Any]):
    """An UTC-indexed OHLCV ``DataFrame`` from Candle objects or candle dicts."""
    import pandas as pd

    times: List[Any] = []
    cols: Dict[str, List[float]] = {"open": [], "high": [], "low": [], "close": [], "volume": []}
    for c in candles:
        get = c.get if isinstance(c, dict) else lambda k, _c=c: getattr(_c, k)
        times.append(get("time"))
        for k in cols:
            cols[k].append(float(get(k)))
    frame = pd.DataFrame(cols, index=pd.to_datetime(times, utc=True))
    frame = frame[~frame.index.duplicated(keep="last")].sort_index()
    return frame


def available() -> Dict[str, Any]:
    """``{key: indicator class}`` — the forge's registry."""
    from smithery.indicators import REGISTRY

    return dict(REGISTRY)


def warmup_bars(indicator: str) -> int:
    """Bars of history the indicator wants before its signal is trustworthy."""
    from smithery.indicators import WARMUP_BARS

    return int(WARMUP_BARS.get(indicator, 400))


def _instance(indicator: str, params: Optional[dict]):
    registry = available()
    cls = registry.get(indicator)
    if cls is None:
        raise ValueError(
            f"unknown regime indicator '{indicator}' (the forge knows: {sorted(registry)})"
        )
    return cls(**(params or {}))


def regime_series(candles: Iterable[Any], indicator: str, params: Optional[dict] = None) -> List[int]:
    """The indicator's ±1 stream over the candles, 0 while warming up."""
    candles = list(candles)
    if len(candles) < 2:
        return [0] * len(candles)
    frame = candles_to_frame(candles)
    signal = _instance(indicator, params).signal(frame)
    return [int(v) for v in signal.to_numpy()]


def regime_signal(candles: Iterable[Any], indicator: str, params: Optional[dict] = None) -> int:
    """The current regime on the last candle: +1, -1, or 0 during warm-up."""
    series = regime_series(candles, indicator, params)
    return series[-1] if series else 0
