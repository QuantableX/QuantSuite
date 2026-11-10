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

from ..config import IndicatorConfig, RankingResolution, RankingSource, RunConfig, TotalBreakoutConfig, TrendKind
from ..data.cache import get_default_cache
from ..data.ohlcv import CoinRef, OhlcvFetcher, confirmed_frame, utc_now
from ..data.ranking.registry import RankingRegistry
from ..data.ranking.base import RankedCoin
from .market import breakout_warmup, market_gate, market_index
from .metrics import PerformanceMetrics, compute_metrics
from .signals import (IndicatorDataUnavailable, aggregate_members, block_start, direction_signal, is_custom, pair_signal, trend_signal,
                      warmup_bars)


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
        per_day = {"1h": 24, "4h": 6, "12h": 2}[config.cadence.value]
        days = bars // per_day + 2
    else:
        days = bars
    # Votes come from history blocks (signals.bounded): the warm-up counts from
    # the start of the block that contains start_date.
    return block_start(config.start_date).date() - dt.timedelta(days=days)


def _market_start(config: RunConfig) -> dt.date:
    """Earliest date TOTAL must cover so the market filter is warmed up by
    ``start_date``. The fixed breakout reads ``breakout_warmup`` bars before
    each history block, an indicator its own warm-up (``_fetch_start``); a
    filter that is the strategy's own indicator reads what that one reads."""

    indicator = config.market_indicator
    if isinstance(indicator, TotalBreakoutConfig):
        bars = breakout_warmup(indicator)
        days = bars // {"1h": 24, "4h": 6, "12h": 2}[config.cadence.value] + 2 if config.cadence.is_intraday else bars
        return block_start(config.start_date).date() - dt.timedelta(days=days)
    if indicator is None:
        return _fetch_start(config)
    return _fetch_start(replace(config, indicator=indicator))


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
    """The run's name; "(custom)" when it runs parameters of its own."""

    if indicator.trend == "ema_cross":
        return f"EMA {indicator.ema_cross.fast_length}/{indicator.ema_cross.slow_length}"
    if indicator.trend == "aggregate":
        members = aggregate_members(indicator)
        custom = any(is_custom(indicator, kind) for kind in members if kind != "ema_cross")
        return f"Aggregate ({len(members)}){' (custom)' if custom else ''}"
    from .smithery import REGISTRY
    cls = REGISTRY.get(indicator.trend)
    name = cls.name if cls is not None else indicator.trend
    return f"{name} (custom)" if cls is not None and is_custom(indicator, indicator.trend) else name


def _require_daily_archive(registry: RankingRegistry, config: RunConfig) -> None:
    """Refuse a daily-ranking run whose archive gap is the backfill job's
    work: fetching it here would hold the run for up to an hour."""

    from ..data.ranking.archive import ON_DEMAND_LIMIT, download_minutes, missing_days

    missing = missing_days(registry.cache, daily_snapshot_days(config.start_date, config.end_date, config.cadence))
    if len(missing) > ON_DEMAND_LIMIT:
        raise ValueError(
            f"The daily ranking archive is missing {len(missing)} days between {missing[0]} and "
            f"{missing[-1]} (about {download_minutes(len(missing), config.min_request_interval)} min to download). "
            "Build it with Daily ranking archive in Settings, or set Ranking snapshots to Weekly.")


def _frames_from(frames: dict[str, pd.DataFrame], start: dt.date) -> dict[str, pd.DataFrame]:
    """Every frame cut to ``start`` — so an indicator sees exactly the history
    it would have fetched on its own and a compared run stays bit-identical
    to a solo run (an EMA's early values depend on where the series begins)."""

    ts = pd.Timestamp(start)
    return {sym: f.loc[f.index >= ts] for sym, f in frames.items()}
from .universe import (
    UniverseSnapshot,
    UniverseTimeline,
    bar_step,
    build_universe_timeline,
    daily_snapshot_days,
    market_warmup_snapshots,
    ranking_known_before_open,
    run_ends,
    walk_lists,
)

log = logging.getLogger(__name__)

USD_SYMBOL = "USD"
_PARALLEL_MIN_SECONDS = 1.0


@dataclass
class HoldingTrade:
    """One contiguous holding of a run: a row of the holdings history."""

    symbol: str | None
    start: pd.Timestamp
    end: pd.Timestamp
    # The holding's own return, net of its entry and exit fills. None for cash.
    trade_return: float | None = None
    # Equity gained or lost, in units of the starting capital (equity starts
    # at 1.0): summed over a run, it is the run's net return. None for cash.
    equity_change: float | None = None


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
    trades: list[HoldingTrade] = field(default_factory=list)
    # Portfolio mode only: {symbol: weight} per day after the open's trades and the day's moves.
    weights: pd.Series | None = None


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
    # Portfolio mode only: the primary run's {symbol: weight} per day.
    held_weights: pd.Series | None = None

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


def portfolio_target(symbols: list[str], scores: dict[str, int], trending: dict[str, bool], cfg) -> dict[str, float]:
    """Portfolio mode's target weights for one day, in rank order, cash last (``USD``).

    The coins are ranked by ``scores`` (ties keep the order of ``symbols``, today's universe) and the top
    ``cfg.positions`` take a slot (0 = all). Raw slot weights: 1 each (``equal``), K..1 (``rank``) or the
    score (``score``). A slot whose coin does not trend gives its weight to USD (``with_usd``) or drops out
    so the trending slots share everything (``without_usd``); no lower-ranked coin moves up. Nothing
    trending, or nothing to weigh, means all USD. When a by-score split would divide by zero (possible only
    without USD in the score), the trending slots share equally."""
    ranked = sorted(symbols, key=lambda s: (-scores.get(s, 0), symbols.index(s)))
    k = len(ranked) if not cfg.positions else min(cfg.positions, len(ranked))
    slots = ranked[:k]
    if cfg.weighting == "equal":
        raw = {s: 1.0 for s in slots}
    elif cfg.weighting == "rank":
        raw = {s: float(k - r) for r, s in enumerate(slots)}
    else:
        raw = {s: float(max(scores.get(s, 0), 0)) for s in slots}
    live = [s for s in slots if trending.get(s)]
    if not live:
        return {USD_SYMBOL: 1.0}
    if cfg.not_trending == "without_usd":
        total = sum(raw[s] for s in live)
        if total <= 0:
            return {s: 1.0 / len(live) for s in live}
        return {s: raw[s] / total for s in live if raw[s] > 0}
    total = sum(raw.values())
    if total <= 0:
        return {USD_SYMBOL: 1.0}
    out = {s: raw[s] / total for s in live if raw[s] > 0}
    cash = sum(raw[s] for s in slots if s not in out) / total
    if cash > 0:
        out[USD_SYMBOL] = cash
    return out or {USD_SYMBOL: 1.0}


def _rebalance(values: dict[str, float], target: dict[str, float], cost: float) -> tuple[dict[str, float], dict[str, float]]:
    """Trade ``values`` (money per holding, USD = cash) to the ``target`` weights; returns the new money per
    holding and each coin's fees.

    Sells first, then buys; each fill pays ``cost`` on the money that moves, cash pays nothing. The
    equity after the trades depends on the trades themselves, so it is found by fixed-point iteration
    (``cost`` is small, so a few rounds reach float precision). A full switch from coin A to coin B
    leaves ``E * (1 - cost)**2``, exactly the single pick's two legs."""
    equity = sum(values.values())
    coins = [s for s in dict.fromkeys([*values, *target]) if s != USD_SYMBOL]
    buy_cost = cost / (1.0 - cost) if cost < 1.0 else 0.0
    after = equity
    for _ in range(12):
        moves = {s: target.get(s, 0.0) * after - values.get(s, 0.0) for s in coins}
        nxt = (equity - cost * sum(-m for m in moves.values() if m < 0) - buy_cost * sum(m for m in moves.values() if m > 0)
               if cost < 1.0 else 0.0)
        if nxt == after:
            break
        after = nxt
    moves = {s: target.get(s, 0.0) * after - values.get(s, 0.0) for s in coins}
    fees = {s: (buy_cost * m if m > 0 else -cost * m) for s, m in moves.items() if m}
    return {s: w * after for s, w in target.items() if w > 0}, fees


def _span_trade(symbol: str, span: dict, end: pd.Timestamp) -> HoldingTrade:
    """A portfolio position as a holdings-history row. Its PnL carries its own fills and moves, so the rows
    of a run sum to its net return; cash rows carry none, like the single pick's."""
    trade = HoldingTrade(symbol, span["start"], end)
    if symbol != USD_SYMBOL:
        trade.equity_change = span["pnl"]
        trade.trade_return = span["pnl"] / span["basis"] if span["basis"] > 0 else None
    return trade


def _buy_and_hold(frames: dict[str, pd.DataFrame], master_idx: pd.DatetimeIndex) -> dict[str, pd.Series]:
    """Buy-and-hold equity per coin in the union universe."""
    bah: dict[str, pd.Series] = {}
    for sym, f in frames.items():
        f_in = f[f.index.isin(master_idx)]
        if f_in.empty:
            continue
        ret = f_in["close"].pct_change().fillna(0.0)
        bah[sym] = (1.0 + ret).cumprod().reindex(master_idx).ffill().fillna(1.0)
    return bah


def _holding_trades(equity: pd.Series, held: pd.Series, cost_mult: float, cash: str) -> list[HoldingTrade]:
    """Split a run into contiguous holdings with their PnL.

    Every fill is charged to the position it opens or closes: a switch's sell
    leg belongs to the old holding, its buy leg to the new one. The newest
    holding is still open and has paid no exit fill yet. ``cash`` (or no
    holding at all) earns nothing and lists no PnL. Reads the simulated
    curve only, so the equity itself stays bit-identical.
    """

    values = equity.to_numpy(dtype=float)
    symbols = [None if pd.isna(value) else str(value) for value in held]
    spans: list[list] = []  # [symbol, first bar, last bar]
    for i, symbol in enumerate(symbols):
        if spans and spans[-1][0] == symbol:
            spans[-1][2] = i
        else:
            spans.append([symbol, i, i])

    def position(symbol: str | None) -> bool:
        return symbol is not None and symbol != cash

    trades: list[HoldingTrade] = []
    for k, (symbol, first, last) in enumerate(spans):
        trade = HoldingTrade(symbol, held.index[first], held.index[last])
        if position(symbol):
            before = values[first - 1] if first else 1.0
            if k and position(spans[k - 1][0]):
                before *= cost_mult  # the previous holding's sell fill
            after = values[last] * (cost_mult if k + 1 < len(spans) else 1.0)
            trade.equity_change = after - before
            trade.trade_return = after / before - 1.0 if before > 0 else None
        trades.append(trade)
    return trades


def pair_scores(
    symbols: list[str],
    votes: Callable[[str, str | None], np.ndarray],
    start: int,
    stop: int,
    include_usd: bool,
) -> tuple[list[str], np.ndarray]:
    """Every coin's points on bars ``[start, stop)``: the one scoring of the
    backtest walk and of today's live signal.

    ``symbols`` are the day's coins in rank order; ``votes(a, b)`` is the
    0/1 "A beats B" verdict of a pair on every bar (``b`` None = against
    USD). Each pair is judged once, the higher rank as A: a 1 is A's point,
    anything else B's (Pine's rule, so a pair without a verdict goes to B).
    Returns the order the points are in (the coins, then USD when it is in
    the universe) and the points, bars by order."""

    order = symbols + ([USD_SYMBOL] if include_usd else [])
    scores = np.zeros((stop - start, len(order)), dtype=np.int32)
    for a, a_sym in enumerate(symbols):
        if include_usd:
            won = votes(a_sym, None)[start:stop]
            scores[:, a] += won
            scores[:, -1] += 1 - won
        for b in range(a + 1, len(symbols)):
            won = votes(a_sym, symbols[b])[start:stop]
            scores[:, a] += won
            scores[:, b] += 1 - won
    return order, scores


def top_scored(order: list[str], scores: np.ndarray) -> np.ndarray:
    """The pick of every scored bar: the most points, the higher rank on a tie
    (argmax keeps the first maximum, and USD is last)."""

    return np.asarray(order, dtype=object)[scores.argmax(axis=1)]


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
        if config.mode == "single_asset":
            return self._run_single_asset(config, progress=progress)
        notes: list[str] = []
        # Freeze the valuation cutoff before fetching, and apply it before
        # computing signals, TOTAL, comparisons or buy-and-hold curves.
        as_of = utc_now()
        if config.end_date >= as_of.date():
            notes.append("Only confirmed candle closes are included in this backtest.")

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
            market_start = _market_start(config)
            fetch_start = min(fetch_start, market_start)

        _tick("Building the coin list", 0.05)
        if (config.ranking_resolution == RankingResolution.DAILY
                and config.ranking_source != RankingSource.LOCAL):
            _require_daily_archive(self.registry, config)
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
            resolution=config.ranking_resolution,
            progress=lambda i, n, d: _tick(f"Coin list {d}", 0.05 + 0.20 * i / max(n, 1)),
            exclude_symbols=config.exclude_symbols,
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
        if config.market_filter and (config.exclude_top_n > 0 or config.exclude_symbols):
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
                resolution=config.ranking_resolution,
            )
        if config.market_filter:
            # TOTAL before the start: each day's own cached ranking, so a day's
            # TOTAL is the same whenever a run starts (market_warmup_snapshots).
            early = market_warmup_snapshots(
                self.registry,
                warm_start=market_start or fetch_start,
                start=config.start_date,
                cadence=config.cadence,
                top_n=config.top_n,
                source=config.ranking_source,
                exclude_stablecoins=config.exclude_stablecoins,
                exclude_wrapped=config.exclude_wrapped,
                resolution=config.ranking_resolution,
            )
            if early:
                market_universe = UniverseTimeline(snapshots=early + market_universe.snapshots)
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
            frame = confirmed_frame(series.frame, timeframe, as_of)
            if frame.empty:
                notes.append(f"No OHLCV available for {coin.symbol}")
                continue
            all_frames[coin.symbol] = frame
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
            weights = held.attrs.get("weights")
            cost_mult = (1.0 - variant.fee_rate) * (1.0 - variant.slippage_rate)
            runs.append(StrategyRun(
                key=kind,
                label=label,
                equity_strategy=equity,
                held_asset=held,
                metrics_strategy=compute_metrics(equity, bars_per_year),
                forced_rotations=forced,
                market_gate=gate,
                trades=(_holding_trades(equity, held, cost_mult, USD_SYMBOL) if weights is None
                        else list(held.attrs.get("trades", []))),
                weights=weights,
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
            config, runs[0].equity_strategy, frames, notes, as_of=as_of
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
            held_weights=primary.weights,
        )

    # ------------------------------------------------------------------ #

    def _run_single_asset(self, config: RunConfig, *, progress=None) -> BacktestResult:
        config = replace(config, cadence=config.bar_cadence)
        market = config.single_asset
        base, quote = market.pair.split("/")
        as_of = utc_now()
        kinds = _trend_kinds(config)
        start = min(_fetch_start(config, kind) for kind in kinds)
        if progress:
            progress(f"Fetching {market.pair} on {market.exchange}", 0.10)
        data = self.ohlcv.get_pair_series(market.exchange, market.pair, start, config.end_date, market.timeframe)
        frame = confirmed_frame(data.frame, market.timeframe, as_of)
        if frame.empty:
            raise ValueError(f"No confirmed {market.timeframe} candles for {market.pair} on {market.exchange}")
        if ((frame[["open", "close"]] <= 0) | ~np.isfinite(frame[["open", "close"]])).any().any():
            raise ValueError("Single Asset candles contain invalid open/close prices")
        index = frame.index[(frame.index >= pd.Timestamp(config.start_date)) &
                            (frame.index < pd.Timestamp(config.end_date + dt.timedelta(days=1)))]
        if index.empty:
            raise ValueError("No confirmed candles in the selected backtest window")
        coin = RankedCoin(1, None, base, market.pair, None, float(frame["close"].iloc[-1]))
        universe = UniverseTimeline([UniverseSnapshot(config.start_date, market.exchange, [coin])])
        notes = [f"Single Asset: {market.pair} on {market.exchange}; returns in {quote}.",
                 "Only confirmed candle closes; signals execute at the next candle open."]
        if market.direction == "long_short":
            notes.append("Long/Short uses 1× bar exposure; funding and borrowing costs are not included.")
        runs = []
        skipped = []
        returns = _bar_returns(frame, index)
        for i, kind in enumerate(kinds):
            variant = _variant_config(config, kind)
            label = _trend_label(variant.indicator)
            if progress:
                progress(f"Evaluating {label}", 0.4 + 0.5 * i / len(kinds))
            history = frame.loc[frame.index >= pd.Timestamp(_fetch_start(config, kind))]
            try:
                signal = direction_signal(history, variant.indicator)
            except IndicatorDataUnavailable as exc:
                skipped.append({"key": kind, "label": label, "reason": str(exc)})
                continue
            positions = signal.shift(1).reindex(index).fillna(0).clip(-1, 1).astype(int)
            if market.direction == "long_cash":
                positions = positions.clip(lower=0)
            equity, previous = 1.0, 0
            values, holdings = [], []
            cost = (1 - config.fee_rate) * (1 - config.slippage_rate)
            for position, bar_return in zip(positions, returns):
                if equity <= 0:
                    position = 0
                equity *= cost ** abs(position - previous)
                equity *= max(0.0, 1 + position * bar_return)
                values.append(equity)
                holdings.append(base if position == 1 else f"Short {base}" if position == -1 else quote)
                previous = position
            curve = pd.Series(values, index=index, name="equity")
            held = pd.Series(holdings, index=index, name="held")
            runs.append(StrategyRun(kind, label, curve, held,
                                    compute_metrics(curve, config.cadence.bars_per_year),
                                    trades=_holding_trades(curve, held, cost, quote)))
        empty = StrategyRun(config.indicator.trend, _trend_label(config.indicator),
                            pd.Series(dtype=float), pd.Series(dtype=object))
        primary = next((run for run in runs if run.key == config.indicator.trend), empty)
        close = frame.loc[index, "close"]
        bah = close / close.iloc[0]
        if progress:
            progress("Done", 1.0)
        return BacktestResult(config=config, universe=universe,
            equity_strategy=primary.equity_strategy, held_asset=primary.held_asset,
            metrics_strategy=primary.metrics_strategy, strategies=runs, skipped_strategies=skipped,
            buy_and_hold={market.pair: bah},
            metrics_buy_and_hold={market.pair: compute_metrics(bah, config.cadence.bars_per_year)}, notes=notes)

    def _btc_ema_benchmarks(
        self,
        config: RunConfig,
        equity: pd.Series,
        frames: dict[str, pd.DataFrame],
        notes: list[str],
        *,
        as_of: dt.datetime | None = None,
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

        btc_frame = confirmed_frame(btc_frame, config.cadence.ccxt_timeframe, as_of or utc_now())

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
            return top_scored(*score_interval(symbols, start, stop))

        def score_interval(symbols: list[str], start: int, stop: int) -> tuple[list[str], np.ndarray]:
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
            return pair_scores(symbols, get_pair_sig, start, stop, bool(config.include_usd))

        returns = {sym: _bar_returns(f, master_idx) for sym, f in frames.items()}
        gate_values = (None if gate is None else
                       gate.reindex(master_idx, fill_value=0).to_numpy())

        # Who may be held on a bar, and whom a decision at its close compares:
        # the ranking the next bar runs on, as today's live signal reads it.
        today_lists, compare_lists = walk_lists(
            universe.snapshots, master_idx, frames,
            ahead=ranking_known_before_open(config.cadence, config.ranking_resolution),
            step=bar_step(config.cadence))
        # Bars that compare the same coins in the same order are scored together.
        compare_end = run_ends(compare_lists)

        if config.portfolio is not None:
            return self._walk_portfolio(config, frames, master_idx, returns, gate_values,
                                        get_pair_sig, score_interval,
                                        get_original_signal if custom_scorer else None,
                                        today_lists, compare_lists, compare_end)

        # Walk daily bars
        equity_values: list[float] = []
        held_values: list[str | None] = []
        forced_rotations: list[dt.date] = []

        equity = 1.0
        current_holding: str | None = None

        pending_holding: str | None = None  # decided at t-1, executes at t
        decisions: np.ndarray | None = None
        decisions_start = decisions_stop = 0

        # Per-side cost multiplier applied at each fill (at the open).
        cost_mult = (1.0 - config.fee_rate) * (1.0 - config.slippage_rate)

        for i, ts in enumerate(master_idx):
            day = ts.date()

            # The bar's own ranking: the most recent snapshot dated on or
            # before it.
            symbols_today = today_lists[i]
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
            # among the coins of the ranking tomorrow runs on.
            symbols = compare_lists[i]
            if not symbols:
                pending_holding = USD_SYMBOL if usd_in else current_holding
                continue
            # The higher filter: a bearish TOTAL at this close means cash
            # tomorrow, whatever the pairwise scores say — with or without a
            # USD leg in the universe, cash is what a bear market means.
            if gate_values is not None and int(gate_values[i]) != 1:
                pending_holding = USD_SYMBOL
                continue
            if custom_scorer:
                scores = self._score_universe(symbols, ts, get_original_signal, usd_in)
                order = symbols + ([USD_SYMBOL] if usd_in else [])
                pending_holding = (max(order, key=lambda s: (scores.get(s, -1), -order.index(s)))
                                   if scores else USD_SYMBOL if usd_in else current_holding)
                continue
            if decisions is None or i >= decisions_stop:
                # Lazy even with a market gate: an entirely bearish interval
                # must not evaluate otherwise-unused (e.g. volume-only) pairs.
                decisions_start, decisions_stop = i, compare_end[i]
                decisions = rank_interval(symbols, i, decisions_stop)
            pending_holding = decisions[i - decisions_start]

        equity_series = pd.Series(equity_values, index=master_idx, name="equity")
        held_series = pd.Series(held_values, index=master_idx, name="held").astype(object)
        return equity_series, held_series, forced_rotations, _buy_and_hold(frames, master_idx)

    # ------------------------------------------------------------------ #

    def _walk_portfolio(
        self,
        config: RunConfig,
        frames: dict[str, pd.DataFrame],
        master_idx: pd.DatetimeIndex,
        returns: dict[str, np.ndarray],
        gate_values: np.ndarray | None,
        get_pair_sig: Callable[[str, str | None], np.ndarray],
        score_interval: Callable[[list[str], int, int], tuple[list[str], np.ndarray]],
        custom_signal: Callable[[str, str | None], pd.Series] | None,
        today_lists: list[list[str]],
        compare_lists: list[list[str]],
        compare_end: list[int],
    ) -> tuple[pd.Series, pd.Series, list[dt.date], dict[str, pd.Series]]:
        """Portfolio mode (``config.portfolio``): the day's target weights from the ranking and the coins'
        own trend, executed at the next open like the single pick. ``held`` keeps one symbol per day for
        old callers (the largest weight, ``USD`` when all cash); the weights ride on ``held.attrs["weights"]``."""
        cfg = config.portfolio
        assert cfg is not None
        cost = 1.0 - (1.0 - config.fee_rate) * (1.0 - config.slippage_rate)
        usd_in = bool(config.include_usd)
        equity_values: list[float] = []
        held_values: list[str | None] = []
        weight_values: list[dict[str, float] | None] = []
        forced_rotations: list[dt.date] = []
        values: dict[str, float] = {}  # money per holding; empty = cash, before the first fill
        spans: dict[str, dict] = {}  # open positions (USD included): first day, PnL so far, money committed
        trades: list[HoldingTrade] = []
        pending: dict[str, float] | None = None
        block: tuple[int, int, list[str], np.ndarray] | None = None  # (first bar, end, order, scores) of a scored interval

        for i, ts in enumerate(master_idx):
            symbols_today = today_lists[i]

            if pending is not None:
                # Forced exit per position: a coin that left the universe goes to cash at the open.
                gone = [s for s in pending if s != USD_SYMBOL and s not in symbols_today]
                if gone:
                    forced_rotations.append(ts.date())
                    pending = dict(pending)
                    for s in gone:
                        pending[USD_SYMBOL] = pending.get(USD_SYMBOL, 0.0) + pending.pop(s)
                held_now = {s for s, v in values.items() if v > 0} or {USD_SYMBOL}
                if cfg.rebalance == "daily" or held_now != set(pending):
                    values, fees = _rebalance(values or {USD_SYMBOL: 1.0}, pending, cost)
                    for s, fee in fees.items():  # a sale or top-up belongs to the open position
                        if s in spans:
                            spans[s]["pnl"] -= fee
                    for s in [s for s in spans if values.get(s, 0.0) <= 0]:
                        span = spans.pop(s)
                        trades.append(_span_trade(s, span, master_idx[i - 1] if i else ts))
                    for s, v in values.items():
                        if v > 0 and s not in spans:
                            fee = fees.get(s, 0.0)  # a new position's buy fill
                            spans[s] = {"start": ts, "pnl": -fee, "basis": v + fee}

            for s in values:
                if s != USD_SYMBOL and s in returns:
                    move = values[s] * float(returns[s][i])
                    values[s] += move
                    spans[s]["pnl"] += move
            equity = sum(values.values()) if values else 1.0
            equity_values.append(equity)
            if values:
                weights = {s: v / equity for s, v in values.items() if v > 0}
                weight_values.append(weights)
                # The largest position, a coin before cash on a tie, the higher rank first among coins.
                held_values.append(max(weights, key=lambda s: (weights[s], s != USD_SYMBOL)) if weights else USD_SYMBOL)
            else:
                weight_values.append(None)
                held_values.append(None)

            # Tomorrow's target, from the signal at this close, among the
            # coins of the ranking tomorrow runs on.
            symbols = compare_lists[i]
            if not symbols:
                pending = {USD_SYMBOL: 1.0} if usd_in else None
                continue
            if gate_values is not None and int(gate_values[i]) != 1:
                pending = {USD_SYMBOL: 1.0}
                continue
            if custom_signal is not None:
                raw = self._score_universe(symbols, ts, custom_signal, usd_in)
                scores = {s: int(raw.get(s, 0)) for s in symbols}
            else:
                if block is None or i >= block[1]:
                    order, matrix = score_interval(symbols, i, compare_end[i])
                    block = (i, compare_end[i], order, matrix)
                first, _, order, matrix = block
                scores = {s: int(matrix[i - first, k]) for k, s in enumerate(order) if s != USD_SYMBOL}
            trending = {s: bool(get_pair_sig(s, None)[i] == 1) for s in symbols}
            pending = portfolio_target(symbols, scores, trending, cfg)

        if len(master_idx):
            for s, span in spans.items():  # still open: no exit fill paid yet
                trades.append(_span_trade(s, span, master_idx[-1]))
        trades.sort(key=lambda t: (t.start, t.symbol == USD_SYMBOL, str(t.symbol)))
        equity_series = pd.Series(equity_values, index=master_idx, name="equity")
        held_series = pd.Series(held_values, index=master_idx, name="held").astype(object)
        held_series.attrs["weights"] = pd.Series(weight_values, index=master_idx, name="weights", dtype=object)
        held_series.attrs["trades"] = trades
        return equity_series, held_series, forced_rotations, _buy_and_hold(frames, master_idx)

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
