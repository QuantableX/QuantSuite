"""Pairwise A/B ratio trend signal using the 12/21 EMA band cross.

Pine's ``f_trend`` (QuantFolio) runs ``ta.ema(close, 12) >= ta.ema(close, 21)``
on the synthetic ``ticker_a / ticker_b`` series via ``request.security``.
We replicate that here: build a synthetic OHLC frame whose every column
is the A/B ratio, then take the EMA cross of the chosen source column.

For comparisons against USD/cash, "B" is a constant 1.0 series, so the
ratio collapses to A's own OHLC.
"""

from __future__ import annotations

import numpy as np
import pandas as pd

from ..config import IndicatorConfig, TrendKind
from . import indicators as ind


class IndicatorDataUnavailable(ValueError):
    """An indicator cannot evaluate this series with the available inputs."""


def aggregate_members(cfg: IndicatorConfig) -> list[TrendKind]:
    """The aggregate's members, each once, never the aggregate itself."""

    members: list[TrendKind] = []
    for kind in cfg.aggregate:
        if kind != "aggregate" and kind not in members:
            members.append(kind)
    return members


def warmup_bars(cfg: IndicatorConfig) -> int:
    """Bars of history a trend signal needs before its first verdict: 0 for
    the EMA cross (kept so results match the Pine version), the registry's
    figure for a Smithery indicator, the largest member's for an aggregate.
    Raises ``ValueError`` for an unknown kind — the engine process may
    predate a new indicator."""

    from .smithery import WARMUP_BARS

    kinds = aggregate_members(cfg) if cfg.trend == "aggregate" else [cfg.trend]
    if cfg.trend == "aggregate" and not kinds:
        raise ValueError("the aggregate trend signal needs at least one member indicator")
    bars = 0
    for kind in kinds:
        if kind == "ema_cross":
            continue
        try:
            bars = max(bars, WARMUP_BARS[kind])
        except KeyError:
            raise ValueError(
                f"unknown trend signal '{kind}' (engine knows: {sorted(WARMUP_BARS)}) — "
                "if this indicator was added recently, restart the engine process to load it"
            ) from None
    return bars


def _ema_cross(df: pd.DataFrame, cfg: IndicatorConfig) -> pd.Series:
    src = ind.resolve_source(df, cfg.ema_cross.src)
    fast = ind.ema(src, cfg.ema_cross.fast_length)
    slow = ind.ema(src, cfg.ema_cross.slow_length)
    return (fast >= slow).fillna(False).astype(int)


def _smithery(df: pd.DataFrame, kind: TrendKind) -> pd.Series:
    from .smithery import REGISTRY
    cls = REGISTRY.get(kind)
    if cls is None:
        raise ValueError(
            f"unknown trend signal '{kind}' (engine knows: "
            f"{sorted(REGISTRY)}) — if this indicator was added recently, "
            "restart the engine process to load it")
    try:
        return cls().signal(df)
    except KeyError as exc:
        if exc.args != ("volume",):
            raise
        raise IndicatorDataUnavailable(
            f"{cls.name}: volume data is unavailable for this series"
        ) from exc


def aggregate_signal(df: pd.DataFrame, cfg: IndicatorConfig) -> pd.Series:
    """The members' ±1 signals averaged — the EMA cross votes +1/-1 from its
    first bar, a Smithery member abstains (0) through its warm-up — then
    the Smithery hysteresis machine at a band of 0: +1 above, -1 below,
    the previous verdict at exactly 0 (an even split), 0 before any member
    has committed. Returned as ±1/0 like a Smithery indicator."""

    from smithery.contract import hysteresis_flip

    members = aggregate_members(cfg)
    if not members:
        raise ValueError("the aggregate trend signal needs at least one member indicator")
    votes = []
    for kind in members:
        if kind == "ema_cross":
            votes.append(np.where(_ema_cross(df, cfg).to_numpy() == 1, 1.0, -1.0))
        else:
            votes.append(_smithery(df, kind).to_numpy(dtype=float))
    sigs = np.stack(votes)
    committed = (sigs != 0).sum(axis=0)
    with np.errstate(invalid="ignore", divide="ignore"):
        score = np.where(committed > 0, sigs.sum(axis=0) / np.maximum(committed, 1), np.nan)
    return pd.Series(hysteresis_flip(score, 0.0), index=df.index, name="aggregate")


def ratio_frame(a: pd.DataFrame, b: pd.DataFrame | None) -> pd.DataFrame:
    """Compute the OHLC frame of ``a / b`` aligned on common days.

    ``b is None`` means compare against USD/cash -> returns ``a``.
    """

    if b is None:
        return a.copy()
    joined = a.join(b, how="inner", rsuffix="_b")
    if joined.empty:
        return joined.iloc[:0][["open", "high", "low", "close"]]
    out = pd.DataFrame(index=joined.index)
    out["open"] = joined["open"] / joined["open_b"]
    out["close"] = joined["close"] / joined["close_b"]
    # For high/low of the ratio: the conservative bounds (max possible
    # ratio reached intraday is high(a)/low(b)).
    out["high"] = joined["high"] / joined["low_b"]
    out["low"] = joined["low"] / joined["high_b"]
    return out


def trend_signal(df: pd.DataFrame, cfg: IndicatorConfig) -> pd.Series:
    """Return the 0/1 trend signal for an OHLC frame.

    ``cfg.trend == "ema_cross"`` is the direct port of QuantFolio's
    ``f_ema_cross``::

        ema_fast = ta.ema(close, fast_len)
        ema_slow = ta.ema(close, slow_len)
        sig = ema_fast >= ema_slow ? 1 : 0

    ``aggregate`` averages the members' signals (:func:`aggregate_signal`).
    Any other kind runs the corresponding Smithery indicator at its
    defaults on the same ratio frame. A +1/-1 output maps to
    1/0 ("A beats B" / not); the pre-commitment warm-up 0 maps to 0, like
    an EMA cross that has not yet crossed up.
    """

    if df.empty or "close" not in df.columns:
        return pd.Series(dtype=int)

    if cfg.trend == "aggregate":
        return (aggregate_signal(df, cfg) > 0).astype(int)
    if cfg.trend != "ema_cross":
        return (_smithery(df, cfg.trend) > 0).astype(int)
    return _ema_cross(df, cfg)


def pair_signal(
    a: pd.DataFrame,
    b: pd.DataFrame | None,
    cfg: IndicatorConfig,
) -> pd.Series:
    """Returns the (0/1) "A beats B" signal for the A/B ratio."""

    try:
        return trend_signal(ratio_frame(a, b), cfg)
    except IndicatorDataUnavailable as exc:
        if b is not None:
            raise IndicatorDataUnavailable(f"{exc}; A/B price ratios have no traded volume") from exc
        raise
