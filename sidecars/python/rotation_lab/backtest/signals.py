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


# A Smithery indicator's vote can depend on all the history it was given: its
# hysteresis keeps a verdict until the score leaves the band again, and some
# scores never do, so a verdict committed years earlier (or by a quirk in old
# candles) could still decide today. Votes are therefore computed on calendar
# blocks of a year counted from a fixed origin, each block seeing only the
# indicator's warm-up before it. A vote on a given day is then the same in a
# backtest from 2020, a live replay from 2023 and today's live signal, and no
# history older than the warm-up plus one year decides it.
HISTORY_BLOCK = pd.Timedelta(days=365)
_BLOCK_ORIGIN = pd.Timestamp("1970-01-01")


def block_start(when) -> pd.Timestamp:
    """The start of the history block that contains ``when``."""
    when = pd.Timestamp(when)
    origin = _BLOCK_ORIGIN.tz_localize(when.tz) if when.tz is not None else _BLOCK_ORIGIN
    return origin + ((when - origin) // HISTORY_BLOCK) * HISTORY_BLOCK


def _bar(index: pd.Index) -> pd.Timedelta:
    head = index[:200]
    if isinstance(head, pd.DatetimeIndex) and len(head) > 1:
        steps = np.diff(head.asi8)
        if (steps == steps[0]).all():   # evenly spaced (the usual case): the median is that step
            return pd.Timedelta(int(steps[0]), unit=head.unit)
    steps = pd.Series(head).diff().dropna()
    return steps.median() if len(steps) else pd.Timedelta(days=1)


# Inside a batch run (batch_votes.active) a pair's ratio frame and a frame's block windows are
# made once and handed out again: the walk then meets the very windows the prefill computed
# (and vote_store remembers their keys), instead of cutting and hashing them a second time.
# Entries keep their inputs alive, so an id cannot be reused while the run lasts.
_FRAME_MEMO: dict | None = None
_WINDOW_MEMO: dict | None = None


def block_windows(df: pd.DataFrame, bars: int) -> list[tuple[pd.Timestamp | None, pd.DataFrame]]:
    """The windows ``bounded`` evaluates: (block start, block with ``bars`` bars of history before
    it), or one window (None, df) when there are no blocks. Shared with the batch prefill, so a
    prefilled vote is computed on exactly the window ``bounded`` asks for."""
    memo = _WINDOW_MEMO
    if memo is not None:
        hit = memo.get((id(df), bars))
        if hit is not None and hit[0] is df:
            return hit[1]
    windows = _block_windows(df, bars)
    if memo is not None:
        memo[id(df), bars] = (df, windows)
    return windows


def _block_windows(df: pd.DataFrame, bars: int) -> list[tuple[pd.Timestamp | None, pd.DataFrame]]:
    if df.empty or bars <= 0:
        return [(None, df)]
    index = df.index
    history = _bar(index) * bars
    start, last, out = block_start(index[0]), index[-1], []
    # On ordered bars the mask below is one contiguous run: found by binary search and taken as
    # a slice (the same rows; pandas' copy-on-write keeps the frame safe), not by a mask per block.
    ordered = index.is_monotonic_increasing
    while start <= last:
        end = start + HISTORY_BLOCK
        if ordered:
            first, stop = index.searchsorted(start - history), index.searchsorted(end)
            if index.searchsorted(start) < stop:
                out.append((start, df.iloc[first:stop]))
        else:
            window = df[(index >= start - history) & (index < end)]
            if (window.index >= start).any():
                out.append((start, window))
        start = end
    return out or [(None, df)]


def bounded(df: pd.DataFrame, compute, bars: int) -> pd.Series:
    """``compute(df)``, evaluated block by block with ``bars`` bars of history before each block."""
    windows = block_windows(df, bars)
    if windows[0][0] is None:
        return compute(windows[0][1])
    parts = []
    for start, window in windows:
        part = compute(window)
        index = part.index
        parts.append(part.iloc[index.searchsorted(start):] if index.is_monotonic_increasing
                     else part[index >= start])
    return pd.concat(parts)


# Votes computed ahead in batches (rotation_lab.backtest.batch_votes): window key -> votes.
# None when no batch prefill is active; member_signal then computes every window itself.
_BATCH_MEMO: dict | None = None


def member_signal(df: pd.DataFrame, kind: TrendKind, cfg: IndicatorConfig | None = None) -> pd.Series:
    """A Smithery indicator's +1/-1/0 votes from bounded, fixed history (see ``bounded``).

    A run that turns on the vote store (``vote_store``, the System Gauntlet's variants) takes
    blocks it has already computed from there; the result is the same by construction."""
    from . import vote_store
    from .smithery import WARMUP_BARS
    compute = lambda frame: _smithery(frame, kind, cfg)
    memo = _BATCH_MEMO
    if memo:
        overrides = indicator_overrides(kind, cfg.params.get(kind)) if cfg is not None else {}
        direct = compute

        def compute(frame, direct=direct):
            hit = memo.get(vote_store.window_key(kind, overrides, frame))
            return hit if hit is not None else direct(frame)
    store = vote_store.active()
    if store is not None:
        overrides = indicator_overrides(kind, cfg.params.get(kind)) if cfg is not None else {}
        compute = store.wrap(kind, overrides, compute)
    return bounded(df, compute, WARMUP_BARS.get(kind, 0))


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


def indicator_overrides(kind: str, values: dict | None) -> dict:
    """The parameter overrides ``values`` of indicator ``kind``, checked
    against its schema and typed like its defaults (a list becomes the
    default's tuple, a whole float an int). Entries equal to the version's
    own parameters drop out, so an override that repeats them is no
    override. Raises ``ValueError`` naming the indicator and parameter."""

    if not values:
        return {}
    from .smithery import REGISTRY
    cls = REGISTRY.get(kind)
    if cls is None:
        raise ValueError(f"parameters for unknown indicator '{kind}' — restart the engine if it is new")
    problems = cls.check_params(values)
    if problems:
        raise ValueError(f"{kind}: " + "; ".join(problems))
    defaults = cls.default_params()
    out = {}
    for name, value in values.items():
        default = defaults[name]
        if isinstance(default, tuple):
            value = tuple(type(d)(v) if isinstance(d, (int, float)) and not isinstance(d, bool) else v
                          for d, v in zip(default, value))
        elif isinstance(default, int) and not isinstance(default, bool) and isinstance(value, float):
            value = int(value)
        elif isinstance(default, float) and isinstance(value, int) and not isinstance(value, bool):
            value = float(value)
        if value != default:
            out[name] = value
    return out


def is_custom(cfg: IndicatorConfig, kind: str) -> bool:
    """Whether ``kind`` runs with parameters of its own in ``cfg``."""

    from .smithery import REGISTRY
    return kind in REGISTRY and bool(indicator_overrides(kind, cfg.params.get(kind)))


def _smithery(df: pd.DataFrame, kind: TrendKind, cfg: IndicatorConfig | None = None) -> pd.Series:
    from .smithery import REGISTRY
    cls = REGISTRY.get(kind)
    if cls is None:
        raise ValueError(
            f"unknown trend signal '{kind}' (engine knows: "
            f"{sorted(REGISTRY)}) — if this indicator was added recently, "
            "restart the engine process to load it")
    overrides = indicator_overrides(kind, cfg.params.get(kind)) if cfg is not None else {}
    try:
        return cls(**overrides).signal(df)
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
            votes.append(member_signal(df, kind, cfg).to_numpy(dtype=float))
    sigs = np.stack(votes)
    committed = (sigs != 0).sum(axis=0)
    with np.errstate(invalid="ignore", divide="ignore"):
        score = np.where(committed > 0, sigs.sum(axis=0) / np.maximum(committed, 1), np.nan)
    return pd.Series(hysteresis_flip(score, 0.0), index=df.index, name="aggregate")


def ratio_frame(a: pd.DataFrame, b: pd.DataFrame | None) -> pd.DataFrame:
    """The OHLC frame of ``a / b`` on their common bars, as an indicator gets it.

    ``b is None`` means compare against USD/cash: ``a``'s own candles. For two
    coins the candle is built by ``smithery.ratio.ratio_candles``, the one rule
    of the engine, the bulk evaluators, today's live signal and certification:
    open and close are the ratios of the opens and closes, high and low the
    highest and lowest ratio along the bar's path where both coins
    carry one (``path_*`` columns), a close stand-in where not.
    """

    from smithery.ratio import ratio_candles, without_path

    memo = _FRAME_MEMO
    if memo is not None:
        hit = memo.get((id(a), id(b)))
        if hit is not None and hit[0] is a and hit[1] is b:
            return hit[2]
    frame = without_path(a).copy() if b is None else ratio_candles(a, b)
    if memo is not None:
        memo[id(a), id(b)] = (a, b, frame)
    return frame


def trend_signal(df: pd.DataFrame, cfg: IndicatorConfig) -> pd.Series:
    """Return the 0/1 trend signal for an OHLC frame.

    ``cfg.trend == "ema_cross"`` is the direct port of QuantFolio's
    ``f_ema_cross``::

        ema_fast = ta.ema(close, fast_len)
        ema_slow = ta.ema(close, slow_len)
        sig = ema_fast >= ema_slow ? 1 : 0

    ``aggregate`` averages the members' signals (:func:`aggregate_signal`).
    Any other kind runs the corresponding Smithery indicator on the same
    ratio frame — at its version's parameters or ``cfg.params[kind]``. A +1/-1 output maps to
    1/0 ("A beats B" / not); the pre-commitment warm-up 0 maps to 0, like
    an EMA cross that has not yet crossed up.
    """

    if df.empty or "close" not in df.columns:
        return pd.Series(dtype=int)

    return (direction_signal(df, cfg) > 0).astype(int)


def direction_signal(df: pd.DataFrame, cfg: IndicatorConfig) -> pd.Series:
    """Signed verdict for a single market; preserve neutral warm-up as cash."""
    if df.empty:
        return pd.Series(dtype=int)
    if cfg.trend == "aggregate":
        return aggregate_signal(df, cfg)
    if cfg.trend != "ema_cross":
        return member_signal(df, cfg.trend, cfg)
    return _ema_cross(df, cfg) * 2 - 1


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
