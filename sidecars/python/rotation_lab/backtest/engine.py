"""Rotation backtest engine.

Ports Pine's ``rotation_system_nr.pine`` rotation core, but the asset
universe is a **function of time** (the user's anti-survivorship-bias
fix). The high-level algorithm:

1. Build a :class:`UniverseTimeline` (top-N at each rotation date).
2. Resolve daily OHLCV for every coin that ever appears in the union of
   the snapshots (cached).
3. Walk daily bars; at each rotation bar, fix the active universe.
4. For every (a, b) in the active universe (including USD when enabled)
   compute the pairwise A/B trend signal. Each asset's score is the
   number of other assets it is outperforming.
5. The asset with the highest score becomes ``best_asset``. Ties are
   broken by the universe order (rank, descending market cap),
   matching the Pine "first-encountered wins" tiebreak.
6. Equity compounds open-to-close on the held asset. If the held asset
   leaves the universe at a rotation boundary, the engine forces an
   exit at the open of that bar and re-evaluates immediately (logged
   as a forced rotation).
7. Buy-and-hold equity curves are computed for every coin that ever
   appears.

The Pine indicator uses ``best_asset[2]`` for equity to model 1-bar
signal visibility before execution. We reproduce that here with the
``execution_lag`` parameter (default 1: the signal observed at the
close of bar t is acted upon at the open of bar t+1).
"""

from __future__ import annotations

import datetime as dt
import logging
import os
import time
from dataclasses import dataclass, field, replace
from typing import Callable

import numpy as np
import pandas as pd

from ..config import IndicatorConfig, RunConfig, TotalBreakoutConfig, TrendKind
from ..data.cache import get_default_cache
from ..data.ohlcv import CoinRef, OhlcvFetcher
from ..data.ranking.registry import RankingRegistry
from .market import market_gate, market_index
from .metrics import PerformanceMetrics, compute_metrics
from .signals import IndicatorDataUnavailable, aggregate_members, pair_signal, trend_signal, warmup_bars


def _fetch_start(config: RunConfig, trend: TrendKind | None = None) -> dt.date:
    """Earliest date OHLCV must cover so the trend signal is warmed up by
    ``start_date``. The classic EMA cross keeps the original behaviour
    (no pre-fetch) so results stay comparable with the Pine version;
    Smithery indicators self-tune on trailing windows and need history
    before the first rotation date.

    ``trend`` defaults to the configured indicator; a compared indicator
    passes its own kind."""

    bars = warmup_bars(_variant_config(config, trend or config.indicator.trend).indicator)
    if bars == 0:
        return config.start_date
    if config.cadence.is_intraday:
        per_day = {"1m": 1440, "1h": 24, "4h": 6, "12h": 2}[config.cadence.value]
        days = bars // per_day + 2
    else:
        days = bars
    return config.start_date - dt.timedelta(days=days)


def _trend_kinds(config: RunConfig) -> list[TrendKind]:
    """The configured indicator first, then every compared indicator once."""

    kinds: list[TrendKind] = [config.indicator.trend]
    for kind in config.compare_trends:
        if kind not in kinds:
            kinds.append(kind)
    return kinds


def _variant_config(config: RunConfig, trend: TrendKind) -> RunConfig:
    """``config`` with ``trend`` as its indicator; the EMA band keeps the
    configured lengths so a compared EMA cross is the user's EMA cross."""

    if trend == config.indicator.trend:
        return config
    return replace(config, indicator=replace(config.indicator, trend=trend))


def _trend_label(indicator: IndicatorConfig) -> str:
    if indicator.trend == "ema_cross":
        return f"EMA {indicator.ema_cross.fast_length}/{indicator.ema_cross.slow_length}"
    if indicator.trend == "aggregate":
        return f"Aggregate ({len(aggregate_members(indicator))})"
    from .smithery import REGISTRY
    cls = REGISTRY.get(indicator.trend)
    return cls.name if cls is not None else indicator.trend


def _frames_from(frames: dict[str, pd.DataFrame], start: dt.date) -> dict[str, pd.DataFrame]:
    """Every frame cut to ``start`` — so an indicator sees exactly the history
    it would have fetched on its own and a compared run stays bit-identical
    to a solo run (an EMA's early values depend on where the series begins)."""

    ts = pd.Timestamp(start)
    return {sym: f.loc[f.index >= ts] for sym, f in frames.items()}
from .universe import (
    UniverseSnapshot,
    UniverseTimeline,
    build_universe_timeline,
)

log = logging.getLogger(__name__)

USD_SYMBOL = "USD"
_PARALLEL_MIN_SECONDS = 1.0


@dataclass
class StrategyRun:
    """The rotation simulated with one trend signal."""

    key: TrendKind
    label: str
    equity_strategy: pd.Series
    held_asset: pd.Series  # symbol per day (NaN before first bar)
    metrics_strategy: PerformanceMetrics = field(default_factory=PerformanceMetrics)
    forced_rotations: list[dt.date] = field(default_factory=list)
    # The higher filter's 0/1 series when the run used it (1 = TOTAL bullish).
    market_gate: pd.Series | None = None


@dataclass
class BacktestResult:
    config: RunConfig
    universe: UniverseTimeline
    equity_strategy: pd.Series
    held_asset: pd.Series  # symbol per day (NaN before first bar)
    buy_and_hold: dict[str, pd.Series] = field(default_factory=dict)
    metrics_strategy: PerformanceMetrics = field(default_factory=PerformanceMetrics)
    metrics_buy_and_hold: dict[str, PerformanceMetrics] = field(default_factory=dict)
    # Successful runs only, in requested order. Scalar fields mirror the
    # configured indicator; they stay empty if that indicator was skipped.
    strategies: list[StrategyRun] = field(default_factory=list)
    skipped_strategies: list[dict[str, str]] = field(default_factory=list)
    # BTC traded with the canonical 12/21 EMA cross signal (always,
    # regardless of the strategy's selected indicator): "BTC EMA L/S"
    # (long when bullish, short when bearish) and "BTC EMA L/C" (long
    # when bullish, cash when bearish). Same execution lag and bar
    # convention as the rotation strategy.
    benchmarks: dict[str, pd.Series] = field(default_factory=dict)
    metrics_benchmarks: dict[str, PerformanceMetrics] = field(default_factory=dict)
    forced_rotations: list[dt.date] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    @property
    def daily_universe(self) -> pd.DataFrame:
        return self.universe.as_membership_frame()


@dataclass
class _AssetState:
    symbol: str
    cg_id: str | None
    frame: pd.DataFrame  # OHLC indexed by date


def _coin_key(symbol: str, cg_id: str | None) -> str:
    return f"{symbol}|{cg_id or ''}"


def _switch_legs(prev: str | None, new: str | None) -> int:
    """Number of fills needed to move from ``prev`` to ``new`` holding.

    USD and ``None`` are both "cash". Cash -> asset is one buy, asset ->
    cash is one sell, and asset A -> asset B is a sell to USD followed
    by a buy (two legs).
    """

    prev_cash = prev is None or prev == USD_SYMBOL
    new_cash = new is None or new == USD_SYMBOL
    if prev_cash and new_cash:
        return 0
    if prev_cash or new_cash:
        return 1
    return 0 if prev == new else 2


def _bar_returns(frame: pd.DataFrame, index: pd.DatetimeIndex) -> np.ndarray:
    """Open-to-close returns aligned once, preserving the missing-bar cash rule."""
    prices = frame.reindex(index)
    opens = prices["open"].to_numpy(dtype=float)
    closes = prices["close"].to_numpy(dtype=float)
    valid = (opens > 0) & (closes != 0) & ~np.isnan(opens) & ~np.isnan(closes)
    result = np.zeros(len(index), dtype=float)
    np.divide(closes - opens, opens, out=result, where=valid)
    return result


def _pair_worker(task) -> pd.Series:
    """Top-level entry point for Windows' spawn-based process pool."""
    return pair_signal(*task)


class BacktestEngine:
    """Top-level orchestrator. Stateless aside from injected services."""

    def __init__(
        self,
        registry: RankingRegistry | None = None,
        ohlcv: OhlcvFetcher | None = None,
    ) -> None:
        self.registry = registry or RankingRegistry(get_default_cache())
        self.ohlcv = ohlcv or OhlcvFetcher(get_default_cache())

    # ------------------------------------------------------------------ #

    def run(
        self,
        config: RunConfig,
        *,
        progress: Callable[[str, float], None] | None = None,
    ) -> BacktestResult:
        notes: list[str] = []

        def _tick(label: str, value: float) -> None:
            if progress:
                try:
                    progress(label, value)
                except Exception:  # noqa: BLE001
                    pass

        # Every trend signal is resolved up front, so an unknown compared
        # indicator fails here and not after minutes of fetching.
        kinds = _trend_kinds(config)
        fetch_start = min(_fetch_start(config, kind) for kind in kinds)
        market_start = None
        if config.market_filter and config.market_indicator is not None:
            market_start = (config.start_date if isinstance(config.market_indicator, TotalBreakoutConfig)
                            else _fetch_start(replace(config, indicator=config.market_indicator)))
            fetch_start = min(fetch_start, market_start)

        _tick("Building the coin list", 0.05)
        universe = build_universe_timeline(
            self.registry,
            start=config.start_date,
            end=config.end_date,
            cadence=config.cadence,
            top_n=config.top_n,
            source=config.ranking_source,
            exclude_stablecoins=config.exclude_stablecoins,
            exclude_wrapped=config.exclude_wrapped,
            exclude_top_n=config.exclude_top_n,
            progress=lambda i, n, d: _tick(f"Coin list {d}", 0.05 + 0.20 * i / max(n, 1)),
        )

        coins = universe.unique_coins()
        if not coins:
            notes.append("No coins resolved from any ranking provider; aborting.")
            return BacktestResult(
                config=config, universe=universe,
                equity_strategy=pd.Series(dtype=float),
                held_asset=pd.Series(dtype=object),
                notes=notes,
            )

        # TOTAL is the whole ranked top-N. SCES evaluates the ranks below an
        # excluded head, so its market list is resolved separately and the
        # head's candles are fetched for the index alone (never traded).
        market_universe = universe
        if config.market_filter and config.exclude_top_n > 0:
            _tick("Building the market list", 0.25)
            market_universe = build_universe_timeline(
                self.registry,
                start=config.start_date,
                end=config.end_date,
                cadence=config.cadence,
                top_n=config.top_n,
                source=config.ranking_source,
                exclude_stablecoins=config.exclude_stablecoins,
                exclude_wrapped=config.exclude_wrapped,
                exclude_top_n=0,
            )
        strategy_symbols = {c.symbol for c in coins}
        fetch_coins = list(coins)
        if config.market_filter:
            seen = set(strategy_symbols)
            for coin in market_universe.unique_coins():
                if coin.symbol not in seen:
                    seen.add(coin.symbol)
                    fetch_coins.append(coin)

        _tick("Fetching OHLCV", 0.30)
        timeframe = config.cadence.ccxt_timeframe
        all_frames: dict[str, pd.DataFrame] = {}
        for i, coin in enumerate(fetch_coins):
            ref = CoinRef(cg_id=coin.cg_id, symbol=coin.symbol)
            try:
                series = self.ohlcv.get_series(ref, fetch_start, config.end_date, timeframe)
            except Exception as exc:  # noqa: BLE001
                log.warning("OHLCV failed for %s: %s", coin.symbol, exc)
                continue
            if series.frame.empty:
                notes.append(f"No OHLCV available for {coin.symbol}")
                continue
            all_frames[coin.symbol] = series.frame
            _tick(
                f"OHLCV {coin.symbol}",
                0.30 + 0.40 * (i + 1) / max(len(fetch_coins), 1),
            )
        frames = {sym: f for sym, f in all_frames.items() if sym in strategy_symbols}

        total: pd.DataFrame | None = None
        if config.market_filter and all_frames:
            _tick("Building TOTAL", 0.72)
            market_symbols = {c.symbol for c in market_universe.unique_coins()}
            total = market_index(
                market_universe.snapshots,
                {sym: f for sym, f in all_frames.items() if sym in market_symbols},
            )

        if not frames:
            notes.append("Could not fetch OHLCV for any coin in the list.")
            return BacktestResult(
                config=config, universe=universe,
                equity_strategy=pd.Series(dtype=float),
                held_asset=pd.Series(dtype=object),
                notes=notes,
            )

        # One rotation per trend signal on the shared candles. Each signal
        # sees the history it would have fetched alone (warm-up differs per
        # indicator), so a compared run equals a solo run bit for bit. The
        # buy-and-hold curves do not depend on the signal: first run's.
        bars_per_year = config.cadence.bars_per_year
        runs: list[StrategyRun] = []
        skipped: list[dict[str, str]] = []
        bah: dict[str, pd.Series] = {}
        for i, kind in enumerate(kinds):
            variant = _variant_config(config, kind)
            label = _trend_label(variant.indicator)
            _tick(
                f"Computing rotation signals ({label})" if len(kinds) > 1 else "Computing rotation signals",
                0.75 + 0.15 * i / len(kinds),
            )
            variant_start = _fetch_start(config, kind)
            gate: pd.Series | None = None
            scope = "TOTAL filter"
            try:
                if total is not None and not total.empty:
                    # Independent history and parameters for TOTAL. Pair signals
                    # still receive exactly their original strategy history.
                    gate = market_gate(
                        total.loc[total.index >= pd.Timestamp(market_start or variant_start)],
                        config.market_indicator or variant.indicator,
                    )
                scope = "coin ranking"
                equity, held, forced, run_bah = self._simulate(
                    variant, universe, _frames_from(frames, variant_start), notes, gate=gate,
                    parallel_pairs=True,
                )
            except IndicatorDataUnavailable as exc:
                reason = f"{scope}: {exc}"
                skipped.append({"key": kind, "label": label, "reason": reason})
                notes.append(f"Skipped {label} — {reason}.")
                continue
            if not runs:
                bah = run_bah
            runs.append(StrategyRun(
                key=kind,
                label=label,
                equity_strategy=equity,
                held_asset=held,
                metrics_strategy=compute_metrics(equity, bars_per_year),
                forced_rotations=forced,
                market_gate=gate,
            ))
        if not runs:
            _tick("Done — no compatible strategies", 1.0)
            return BacktestResult(
                config=config, universe=universe,
                equity_strategy=pd.Series(dtype=float), held_asset=pd.Series(dtype=object),
                notes=notes, skipped_strategies=skipped,
            )
        # Never pass a comparison off as the configured primary if it was skipped.
        primary = next((run for run in runs if run.key == config.indicator.trend), None)
        if primary is None:
            primary = StrategyRun(
                key=config.indicator.trend, label=_trend_label(config.indicator),
                equity_strategy=pd.Series(dtype=float), held_asset=pd.Series(dtype=object),
            )

        _tick("Computing BTC EMA benchmarks", 0.90)
        benchmarks, metrics_bench = self._btc_ema_benchmarks(
            config, runs[0].equity_strategy, frames, notes
        )

        _tick("Computing metrics", 0.95)
        metrics_bah = {sym: compute_metrics(series, bars_per_year) for sym, series in bah.items()}

        _tick("Done", 1.0)
        return BacktestResult(
            config=config,
            universe=universe,
            equity_strategy=primary.equity_strategy,
            held_asset=primary.held_asset,
            buy_and_hold=bah,
            metrics_strategy=primary.metrics_strategy,
            metrics_buy_and_hold=metrics_bah,
            benchmarks=benchmarks,
            metrics_benchmarks=metrics_bench,
            forced_rotations=primary.forced_rotations,
            notes=notes,
            strategies=runs,
            skipped_strategies=skipped,
        )

    # ------------------------------------------------------------------ #

    def _btc_ema_benchmarks(
        self,
        config: RunConfig,
        equity: pd.Series,
        frames: dict[str, pd.DataFrame],
        notes: list[str],
    ) -> tuple[dict[str, pd.Series], dict[str, PerformanceMetrics]]:
        """Trade BTC with the canonical 12/21 close EMA cross signal.

        Always the 12/21 EMA cross, independent of the indicator the
        rotation strategy runs with — these are fixed reference curves.

        Returns two equity curves aligned to ``equity.index``:

        * ``BTC EMA L/S`` - long when EMA(12) >= EMA(21), short otherwise.
        * ``BTC EMA L/C`` - long when bullish, flat (cash) otherwise.

        The trend signal is computed on BTC's own OHLC (no ratio), with
        the same execution lag of 1 bar that the rotation engine uses
        (signal observed at close of ``t`` -> applied at open of ``t+1``).
        """

        if equity.empty:
            return {}, {}

        master_idx = equity.index

        btc_frame = frames.get("BTC")
        if btc_frame is None:
            try:
                ref = CoinRef(cg_id="bitcoin", symbol="BTC")
                series = self.ohlcv.get_series(
                    ref, _fetch_start(config), config.end_date, config.cadence.ccxt_timeframe
                )
                if not series.frame.empty:
                    btc_frame = series.frame
            except Exception as exc:  # noqa: BLE001
                log.warning("BTC benchmark unavailable: %s", exc)
                notes.append(f"BTC benchmark unavailable: {exc}")

        if btc_frame is None or btc_frame.empty:
            return {}, {}

        # The BTC benchmarks are pinned to the canonical 12/21 close EMA
        # cross regardless of the indicator selected for the rotation
        # strategy — they are a fixed reference, not a variant of the run.
        # Slice to the backtest window first: Smithery indicators pre-fetch
        # warm-up history before start_date (_fetch_start), and an EMA's
        # early values depend on where the series begins — without the
        # slice the benchmark would shift with the selected indicator.
        btc_frame = btc_frame.loc[btc_frame.index >= pd.Timestamp(config.start_date)]
        if btc_frame.empty:
            return {}, {}
        sig = trend_signal(btc_frame, IndicatorConfig())
        if sig.empty:
            return {}, {}

        sig_lagged = sig.shift(1).fillna(0).astype(int)
        sig_aligned = sig_lagged.reindex(master_idx).fillna(0).astype(int)

        returns = _bar_returns(btc_frame, master_idx)

        # Same per-side trading costs as the rotation strategy, charged
        # at the open when the position changes. L/S is always in a
        # position: the first bar is one entry leg, every signal flip
        # closes one position and opens the opposite (two legs). L/C
        # pays one leg per entry (buy) and one per exit (sell).
        cost_mult = (1.0 - config.fee_rate) * (1.0 - config.slippage_rate)

        eq_ls_values: list[float] = []
        eq_lc_values: list[float] = []
        eq_ls = 1.0
        eq_lc = 1.0
        prev_s: int | None = None
        for r_today, s in zip(returns.tolist(), sig_aligned.to_list()):
            if prev_s is None:
                eq_ls *= cost_mult  # initial entry (long or short)
                if s == 1:
                    eq_lc *= cost_mult
            elif s != prev_s:
                eq_ls *= cost_mult ** 2  # close + open opposite
                eq_lc *= cost_mult  # one buy (0->1) or one sell (1->0)
            prev_s = s
            eq_ls *= 1.0 + (r_today if s == 1 else -r_today)
            eq_lc *= 1.0 + (r_today if s == 1 else 0.0)
            eq_ls_values.append(eq_ls)
            eq_lc_values.append(eq_lc)

        ls_series = pd.Series(eq_ls_values, index=master_idx, name="btc_ema_ls")
        lc_series = pd.Series(eq_lc_values, index=master_idx, name="btc_ema_lc")

        benchmarks = {
            "BTC EMA L/S": ls_series,
            "BTC EMA L/C": lc_series,
        }
        metrics = {k: compute_metrics(v, config.cadence.bars_per_year) for k, v in benchmarks.items()}
        return benchmarks, metrics

    # ------------------------------------------------------------------ #

    def _simulate(
        self,
        config: RunConfig,
        universe: UniverseTimeline,
        frames: dict[str, pd.DataFrame],
        notes: list[str],
        gate: pd.Series | None = None,
        *,
        parallel_pairs: bool = False,
    ) -> tuple[pd.Series, pd.Series, list[dt.date], dict[str, pd.Series]]:
        # Master daily index: union of all coin frames, restricted to range
        if not frames:
            return pd.Series(dtype=float), pd.Series(dtype=object), [], {}
        indexes = iter(f.index for f in frames.values())
        master_idx = next(indexes)
        for index in indexes:
            master_idx = master_idx.union(index)
        start = pd.Timestamp(config.start_date, tz=master_idx.tz)
        end = pd.Timestamp(config.end_date + dt.timedelta(days=1), tz=master_idx.tz)
        master_idx = master_idx[(master_idx >= start) & (master_idx < end)]
        master_idx = pd.DatetimeIndex(master_idx, freq=None)
        if master_idx.empty:
            return pd.Series(dtype=float), pd.Series(dtype=object), [], {}

        # Align a pair once instead of looking up timestamps for every bar.
        # Keep direction in the key: B/A is not necessarily the inverse of A/B.
        signal_cache: dict[tuple[str, str], np.ndarray] = {}
        custom_scorer = self._score_universe is not _DEFAULT_SCORER
        original_signals: dict[tuple[str, str | None], pd.Series] = {}

        def get_original_signal(a: str, b: str | None) -> pd.Series:
            # Research callers can override the scalar scorer (e.g. a veto of
            # the selected coin). Preserve that hook and its full-history input.
            if (a, b) not in original_signals:
                original_signals[a, b] = (pair_signal(frames[a], frames.get(b), config.indicator)
                    if a in frames and (b is None or b in frames) else pd.Series(dtype=int))
            return original_signals[a, b]

        def get_pair_sig(a_sym: str, b_sym: str | None) -> np.ndarray:
            key = (a_sym, b_sym or USD_SYMBOL)
            if key in signal_cache:
                return signal_cache[key]
            a_df = frames.get(a_sym)
            b_df = frames.get(b_sym) if b_sym else None
            if a_df is None or (b_sym is not None and b_df is None):
                sig = np.zeros(len(master_idx), dtype=np.int8)
            else:
                sig = pair_signal(a_df, b_df, config.indicator).reindex(
                    master_idx, fill_value=0).to_numpy(dtype=np.int8)
            signal_cache[key] = sig
            return sig

        def rank_interval(symbols: list[str], start: int, stop: int) -> np.ndarray:
            order = symbols + ([USD_SYMBOL] if config.include_usd else [])
            # Time one necessary pair first: cheap indicators should not pay
            # Windows process startup just because their histories are long.
            pairs = [(a, b) for i, a in enumerate(symbols)
                     for b in ([None] if config.include_usd else []) + symbols[i + 1:]
                     if (a, b or USD_SYMBOL) not in signal_cache
                     and a in frames and (b is None or b in frames)]
            rows = max((len(frames[a]) for a, _ in pairs), default=0)
            if (parallel_pairs and config.indicator.trend != "ema_cross" and len(pairs) >= 5
                    and rows >= 512):
                try:
                    workers = max(1, min(8, int(os.environ.get("ROTATION_LAB_WORKERS", "8"))))
                except ValueError:
                    workers = 8
                if workers > 1:
                    from smithery import parallel
                    first, *remaining = pairs
                    started = time.perf_counter()
                    get_pair_sig(*first)
                    estimated = (time.perf_counter() - started) * len(remaining)
                    threshold = _PARALLEL_MIN_SECONDS * (0.15 if parallel.pool_ready() else 1.0)
                    if estimated >= threshold:
                        signals = parallel.pmap(_pair_worker,
                            [(frames[a], frames.get(b), config.indicator) for a, b in remaining],
                            max_workers=workers)
                        for (a, b), sig in zip(remaining, signals):
                            signal_cache[a, b or USD_SYMBOL] = sig.reindex(
                                master_idx, fill_value=0).to_numpy(dtype=np.int8)
            scores = np.zeros((stop - start, len(order)), dtype=np.int32)
            for a, a_sym in enumerate(symbols):
                if config.include_usd:
                    votes = get_pair_sig(a_sym, None)[start:stop]
                    scores[:, a] += votes
                    scores[:, -1] += 1 - votes
                for b in range(a + 1, len(symbols)):
                    votes = get_pair_sig(a_sym, symbols[b])[start:stop]
                    scores[:, a] += votes
                    scores[:, b] += 1 - votes
            # argmax's first maximum implements the original rank tiebreak.
            return np.asarray(order, dtype=object)[scores.argmax(axis=1)]

        returns = {sym: _bar_returns(f, master_idx) for sym, f in frames.items()}
        gate_values = (None if gate is None else
                       gate.reindex(master_idx, fill_value=0).to_numpy())

        # Walk daily bars
        equity_values: list[float] = []
        held_values: list[str | None] = []
        forced_rotations: list[dt.date] = []

        equity = 1.0
        current_holding: str | None = None
        active_snapshot: UniverseSnapshot | None = None

        # Iterate through master_idx; on each "rotation boundary" pick a
        # new active snapshot. With coupled cadence the rotation
        # boundaries are exactly the snapshot dates.
        snap_iter = iter(universe.snapshots)
        next_snap = next(snap_iter, None)

        pending_holding: str | None = None  # decided at t-1, executes at t
        decisions: np.ndarray | None = None
        decisions_start = 0

        # Per-side cost multiplier applied at each fill (at the open).
        cost_mult = (1.0 - config.fee_rate) * (1.0 - config.slippage_rate)

        for i, ts in enumerate(master_idx):
            day = ts.date()

            # Advance the active universe snapshot to the most recent
            # snapshot date <= day.
            while next_snap and pd.Timestamp(next_snap.on_date) <= ts:
                active_snapshot = next_snap
                next_snap = next(snap_iter, None)
                decisions = None

            symbols_today = (
                [c.symbol for c in active_snapshot.coins]
                if active_snapshot
                else []
            )
            usd_in = bool(config.include_usd)

            # Forced rotation check: if our previously-pending holding no
            # longer fits the active universe, exit to cash.
            if pending_holding and pending_holding != USD_SYMBOL and pending_holding not in symbols_today:
                forced_rotations.append(day)
                pending_holding = USD_SYMBOL if usd_in else (symbols_today[0] if symbols_today else None)

            # Execute the pending decision from the prior bar. Trading
            # costs are charged per fill at the open: switching assets
            # sells to USD then buys, so it pays two legs.
            if pending_holding is not None:
                legs = _switch_legs(current_holding, pending_holding)
                if legs:
                    equity *= cost_mult ** legs
                current_holding = pending_holding

            # Realise the day's return based on the held asset
            r_today = 0.0
            if current_holding and current_holding != USD_SYMBOL:
                asset_returns = returns.get(current_holding)
                if asset_returns is not None:
                    r_today = float(asset_returns[i])
            equity *= 1.0 + r_today
            equity_values.append(equity)
            held_values.append(current_holding)

            # Decide tomorrow's holding (signal observed at close of `day`)
            if not symbols_today:
                pending_holding = USD_SYMBOL if usd_in else current_holding
                continue
            # The higher filter: a bearish TOTAL at this close means cash
            # tomorrow, whatever the pairwise scores say — with or without a
            # USD leg in the universe, cash is what a bear market means.
            if gate_values is not None and int(gate_values[i]) != 1:
                pending_holding = USD_SYMBOL
                continue
            if custom_scorer:
                scores = self._score_universe(symbols_today, ts, get_original_signal, usd_in)
                order = symbols_today + ([USD_SYMBOL] if usd_in else [])
                pending_holding = (max(order, key=lambda s: (scores.get(s, -1), -order.index(s)))
                                   if scores else USD_SYMBOL if usd_in else current_holding)
                continue
            if decisions is None:
                # Lazy even with a market gate: an entirely bearish interval
                # must not evaluate otherwise-unused (e.g. volume-only) pairs.
                stop = (len(master_idx) if next_snap is None else
                        int(master_idx.searchsorted(pd.Timestamp(next_snap.on_date))))
                decisions_start = i
                decisions = rank_interval(symbols_today, i, stop)
            pending_holding = decisions[i - decisions_start]

        equity_series = pd.Series(equity_values, index=master_idx, name="equity")
        held_series = pd.Series(held_values, index=master_idx, name="held").astype(object)

        # Buy-and-hold equity per coin in the union universe
        bah: dict[str, pd.Series] = {}
        for sym, f in frames.items():
            f_in = f[f.index.isin(master_idx)]
            if f_in.empty:
                continue
            ret = f_in["close"].pct_change().fillna(0.0)
            bah[sym] = (1.0 + ret).cumprod().reindex(master_idx).ffill().fillna(1.0)

        return equity_series, held_series, forced_rotations, bah

    # ------------------------------------------------------------------ #

    @staticmethod
    def _score_universe(
        symbols: list[str],
        ts: pd.Timestamp,
        get_pair_sig: Callable[[str, str | None], pd.Series],
        include_usd: bool,
    ) -> dict[str, int]:
        """Compute the pairwise score vector at ``ts``.

        For every pair (a, b) we look up ``a vs b`` signal at ``ts``.
        If it is 1, a scores +1; else b scores +1 (Pine's behaviour).
        USD is treated as a synthetic constant series via
        ``pair_signal(a, None)``.
        """

        scores: dict[str, int] = {s: 0 for s in symbols}
        if include_usd:
            scores[USD_SYMBOL] = 0
        for i, a in enumerate(symbols):
            # vs USD
            if include_usd:
                sig = get_pair_sig(a, None)
                val = int(sig.get(ts, 0)) if not sig.empty else 0
                if val:
                    scores[a] += 1
                else:
                    scores[USD_SYMBOL] += 1
            # vs other coins
            for b in symbols[i + 1:]:
                sig = get_pair_sig(a, b)
                val = int(sig.get(ts, 0)) if not sig.empty else 0
                if val:
                    scores[a] += 1
                else:
                    scores[b] += 1
        return scores


# Capture the original function so instance/subclass/research overrides retain
# their behavior while the standard engine uses interval-wise array scoring.
_DEFAULT_SCORER = BacktestEngine._score_universe
