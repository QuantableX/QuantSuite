"""JSON-RPC server for QuantSystems.

QuantSystems' Tauri (Rust) backend spawns this module as a long-lived
child process (``python -m rotation_lab.rpc``) and drives the proven
``rotation_lab`` engine over newline-delimited JSON on stdin/stdout —
the same model QuantAlgo uses for its Python strategy runtime.

Protocol (one JSON object per line):

* Request  (Rust → Python):   ``{"id": 1, "method": "backtest", "params": {...}}``
* Response (Python → Rust):   ``{"id": 1, "result": {...}}`` or ``{"id": 1, "error": "..."}``
* Notification (Python → Rust): ``{"method": "progress", "params": {...}}`` (no ``id``)

Notifications are emitted while a long call is running and are surfaced
by the Rust layer as Tauri events.

Methods
-------
``ping``           → ``{"ok": true, "version": "..."}``
``live``           → today's (or as-of) top-N universe + NxN score matrix + best asset
``backtest``       → equity curve, buy-and-hold, BTC EMA benchmarks, metrics, forced rotations
``universe``       → historical top-N for a single date
``fund``           → a fund over its members' equity: every split method, rebalances, correlation, robustness
``cache_stats``    → row counts + db size
``clear_cache``    → wipe rankings / ohlcv / all
``ranking_archive_status`` / ``_build`` / ``_cancel``
                   → the daily CMC ranking archive and its background backfill
                     (progress as ``ranking_archive`` notifications)
"""

from __future__ import annotations

import datetime as dt
import json
import math
import sys
import threading
from dataclasses import asdict, replace
from typing import Any

import numpy as np
import pandas as pd

from . import __version__, funds
from .backtest.engine import USD_SYMBOL, BacktestEngine, HoldingTrade, pair_scores, top_scored, with_path
from .backtest.metrics import METRIC_LABELS, PerformanceMetrics, compute_metrics
from .backtest.signals import pair_signal
from .backtest.universe import candle_mask, comparable, ranking_known_before_open
from .config import (
    Cadence,
    EmaCrossConfig,
    IndicatorConfig,
    RankingResolution,
    RankingSource,
    RunConfig,
    default_cache_path,
)
from .data.cache import get_default_cache
from .data.ohlcv import CoinRef, OhlcvFetcher
from .data.ranking.registry import RankingRegistry

# stdout is the wire; protect it with a lock so notifications emitted from
# a worker thread never interleave with the final response.
_WRITE_LOCK = threading.Lock()


# ── wire helpers ────────────────────────────────────────────────────────────


def _write(obj: dict[str, Any]) -> None:
    with _WRITE_LOCK:
        sys.stdout.write(
            json.dumps(_finite(obj), default=_json_default, allow_nan=False) + "\n"
        )
        sys.stdout.flush()


def _finite(value: Any) -> Any:
    """Replace NaN/inf floats with ``None``, recursively.

    ``json.dumps`` serialises those itself, as the bare ``NaN``/``Infinity``
    tokens, without ever consulting ``default`` -- and serde_json on the
    Rust side rejects them, killing the whole response. So the payload has
    to be cleaned before it is dumped, not during.
    """

    if isinstance(value, float):  # np.float64 subclasses float
        return value if math.isfinite(value) else None
    if isinstance(value, dict):
        return {k: _finite(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_finite(v) for v in value]
    return value


def _json_default(value: Any) -> Any:
    if isinstance(value, (dt.date, dt.datetime)):
        return value.isoformat()
    raise TypeError(f"Not JSON serialisable: {type(value)!r}")


def _notify(method: str, params: dict[str, Any]) -> None:
    _write({"method": method, "params": params})


# ── config parsing ──────────────────────────────────────────────────────────


def _parse_date(value: Any, fallback: dt.date) -> dt.date:
    if not value:
        return fallback
    if isinstance(value, dt.date):
        return value
    return dt.date.fromisoformat(str(value)[:10])


def _indicator_params(raw: Any, inherited: dict[str, dict[str, Any]] | None = None) -> dict[str, dict[str, Any]]:
    """``indicator.params`` of the wire — ``{key: {param: value}}`` over
    ``inherited`` — typed and validated per key (the error names indicator
    and parameter). Keys the engine does not know are kept unchecked: a
    config may remember the parameters of an indicator it no longer uses."""

    from .backtest.signals import indicator_overrides
    from .backtest.smithery import REGISTRY

    if raw is not None and not isinstance(raw, dict):
        raise ValueError("indicator.params must be an object of indicator key → parameters")
    out = {key: dict(values) for key, values in (inherited or {}).items()}
    for key, values in (raw or {}).items():
        if not isinstance(values, dict):
            raise ValueError(f"indicator.params.{key} must be an object of parameter → value")
        out[str(key)] = {**out.get(str(key), {}), **values}
    return {key: (indicator_overrides(key, values) if key in REGISTRY else values)
            for key, values in out.items() if values}


def _build_indicator(ind_raw: dict[str, Any], inherited: dict[str, dict[str, Any]] | None = None) -> IndicatorConfig:
    ema_raw = ind_raw.get("emaCross") or ind_raw.get("ema_cross") or {}
    return IndicatorConfig(
        trend=ind_raw.get("trend", "ema_cross"),
        ema_cross=EmaCrossConfig(
            src=ema_raw.get("src", "close"),
            fast_length=int(ema_raw.get("fastLength", ema_raw.get("fast_length", 12))),
            slow_length=int(ema_raw.get("slowLength", ema_raw.get("slow_length", 21))),
        ),
        aggregate=tuple(str(kind) for kind in (ind_raw.get("aggregate") or []) if kind),
        params=_indicator_params(ind_raw.get("params"), inherited),
    )


def _build_config(raw: dict[str, Any]) -> RunConfig:
    raw = raw or {}
    from .config import SingleAssetConfig
    mode = str(raw.get("mode", "rotation"))
    # An unfinished market selection must not prevent switching back to rotation.
    single = (raw.get("singleAsset", raw.get("single_asset")) or {}) if mode == "single_asset" else {}
    today = dt.date.today()
    indicator = _build_indicator(raw.get("indicator") or {})
    market_raw = raw.get("marketIndicator", raw.get("market_indicator"))
    if market_raw is not None and market_raw.get('trend') == 'total_breakout':
        from .config import TotalBreakoutConfig
        breakout_defaults = TotalBreakoutConfig()
        market_indicator = TotalBreakoutConfig(
            trend_length=int(market_raw.get('trendLength', market_raw.get('trend_length', breakout_defaults.trend_length))),
            entry_length=int(market_raw.get('entryLength', market_raw.get('entry_length', breakout_defaults.entry_length))),
            exit_length=int(market_raw.get('exitLength', market_raw.get('exit_length', breakout_defaults.exit_length))),
        )
    else:
        # The TOTAL signal runs the config's parameter overrides too; its own win.
        market_indicator = _build_indicator(market_raw, indicator.params) if market_raw is not None else None
    compare_raw = raw.get("compareTrends", raw.get("compare_trends")) or []
    compare = tuple(str(kind) for kind in compare_raw if kind)
    return RunConfig(
        mode=mode,
        single_asset=SingleAssetConfig(
            exchange=str(single.get("exchange", "coinbase")),
            pair=str(single.get("pair", "BTC/USD")),
            timeframe=str(single.get("timeframe", "1d")),
            direction=str(single.get("direction", "long_cash")),
        ),
        top_n=int(raw.get("topN", raw.get("top_n", 5))),
        exclude_top_n=int(raw.get("excludeTopN", raw.get("exclude_top_n", 0))),
        exclude_symbols=tuple(sorted({str(s).strip().upper() for s in (raw.get("excludeSymbols") or raw.get("exclude_symbols") or []) if str(s).strip()})),
        cadence=Cadence(raw.get("cadence", "daily")),
        start_date=_parse_date(
            raw.get("startDate", raw.get("start_date")),
            today - dt.timedelta(days=365 * 3),
        ),
        end_date=_parse_date(raw.get("endDate", raw.get("end_date")), today),
        ranking_source=RankingSource(raw.get("rankingSource", raw.get("ranking_source", "auto"))),
        # Missing means weekly: configs saved before daily snapshots existed
        # (LCES among them) keep computing exactly as before.
        ranking_resolution=RankingResolution(
            raw.get("rankingResolution", raw.get("ranking_resolution", "weekly"))),
        exclude_stablecoins=bool(raw.get("excludeStablecoins", raw.get("exclude_stablecoins", True))),
        exclude_wrapped=bool(raw.get("excludeWrapped", raw.get("exclude_wrapped", True))),
        indicator=indicator,
        compare_trends=compare,
        market_filter=bool(raw.get("marketFilter", raw.get("market_filter", False))),
        market_indicator=market_indicator,
        include_usd=bool(raw.get("includeUsd", raw.get("include_usd", True))),
        fee_rate=float(raw.get("feeRate", raw.get("fee_rate", 0.001))),
        slippage_rate=float(raw.get("slippageRate", raw.get("slippage_rate", 0.0))),
        min_request_interval=float(raw.get("minRequestInterval", raw.get("min_request_interval", 1.2))),
        portfolio=_build_portfolio(raw.get("portfolio"), mode),
    )


_PORTFOLIO_CHOICES = {"withUsd": "with_usd", "withoutUsd": "without_usd", "onChange": "on_change"}


def _build_portfolio(raw: Any, mode: str):
    """Portfolio mode from a saved config: missing, ``enabled: false`` or a Single Asset system mean off."""
    from .config import PortfolioConfig
    if not isinstance(raw, dict) or not raw.get("enabled") or mode != "rotation":
        return None
    choice = lambda value: _PORTFOLIO_CHOICES.get(str(value), str(value))  # noqa: E731
    return PortfolioConfig(
        positions=int(raw.get("positions", 0) or 0),
        weighting=str(raw.get("weighting", "equal")),
        not_trending=choice(raw.get("notTrending", raw.get("not_trending", "withUsd"))),
        rebalance=choice(raw.get("rebalance", "onChange")),
    )


# ── serialisation ───────────────────────────────────────────────────────────


def _chart_time(ts: Any, intraday: bool) -> Any:
    """Lightweight-charts time: a ``YYYY-MM-DD`` string for daily bars, or a
    UNIX timestamp (seconds, UTC) for intraday bars so the axis shows hours."""

    t = pd.Timestamp(ts)
    if intraday:
        # ``.value`` is ns since epoch with the (UTC) wall clock; → seconds.
        return int(t.value // 1_000_000_000)
    return t.strftime("%Y-%m-%d")


def _series_to_points(series: pd.Series, intraday: bool = False) -> list[dict[str, Any]]:
    """Convert an equity Series → ``[{time, value}]`` (lightweight-charts shape)."""

    if series is None or series.empty:
        return []
    out: list[dict[str, Any]] = []
    for ts, value in series.items():
        if value is None or pd.isna(value):
            continue
        out.append({"time": _chart_time(ts, intraday), "value": float(value)})
    return out


def _held_to_points(series: pd.Series, intraday: bool = False) -> list[dict[str, Any]]:
    if series is None or series.empty:
        return []
    out: list[dict[str, Any]] = []
    for ts, value in series.items():
        out.append({
            "time": _chart_time(ts, intraday),
            "symbol": None if value is None or (isinstance(value, float) and pd.isna(value)) else str(value),
        })
    return out


def _weights_to_points(series: pd.Series | None, intraday: bool = False) -> list[dict[str, Any]]:
    """Portfolio mode's holdings per day: ``{"time", "weights": {symbol: share}}``, ``weights`` None before the first fill."""
    if series is None or series.empty:
        return []
    return [{"time": _chart_time(ts, intraday),
             "weights": None if not value else {str(k): float(v) for k, v in value.items()}}
            for ts, value in series.items()]


def _trades_to_dicts(trades: list[HoldingTrade], intraday: bool = False) -> list[dict[str, Any]]:
    """Holdings history rows with their PnL in percent (None for cash)."""

    def pct(value: float | None) -> float | None:
        return None if value is None or not math.isfinite(value) else value * 100

    return [
        {
            "symbol": trade.symbol,
            "from": _chart_time(trade.start, intraday),
            "to": _chart_time(trade.end, intraday),
            "tradePct": pct(trade.trade_return),
            "equityPct": pct(trade.equity_change),
        }
        for trade in trades
    ]


def _to_camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(part.title() for part in rest)


def _metrics_to_dict(metrics: PerformanceMetrics) -> dict[str, Any]:
    data = asdict(metrics)
    # The frontend `PerformanceMetrics` type uses camelCase keys; the dataclass
    # fields are snake_case, so convert (NaN → null along the way).
    return {
        _to_camel(k): (None if (isinstance(v, float) and v != v) else v)
        for k, v in data.items()
    }


def _coin_to_dict(coin: Any, *, excluded: bool = False) -> dict[str, Any]:
    return {
        "rank": coin.rank,
        "cgId": coin.cg_id,
        "symbol": coin.symbol,
        "name": coin.name,
        "marketCap": coin.market_cap,
        "price": coin.price,
        "excluded": excluded,
    }


# ── method: ping ─────────────────────────────────────────────────────────────


def _method_ping(_params: dict[str, Any]) -> dict[str, Any]:
    return {"ok": True, "version": __version__, "cachePath": str(default_cache_path())}


# ── method: universe (historical top-N on a single date) ─────────────────────


def _method_universe(params: dict[str, Any]) -> dict[str, Any]:
    cfg = _build_config(params.get("config") or {})
    on_date = _parse_date(params.get("onDate") or params.get("on_date"), cfg.end_date)
    registry = RankingRegistry(get_default_cache())
    resolved = registry.get_top_n(
        on_date,
        cfg.top_n,
        source=cfg.ranking_source,
        exclude_stablecoins=cfg.exclude_stablecoins,
        exclude_wrapped=cfg.exclude_wrapped,
        exclude_top_n=cfg.exclude_top_n,
        resolution=cfg.ranking_resolution,
        exclude_symbols=cfg.exclude_symbols,
    )
    return {
        "onDate": on_date.isoformat(),
        "provider": resolved.provider,
        "coins": [_coin_to_dict(c) for c in resolved.coins],
    }


# ── method: live (top-N + NxN score matrix + best asset) ─────────────────────


def _method_live(params: dict[str, Any]) -> dict[str, Any]:
    """Current signal plus a replay from the saved live start through today.

    Backtest dates and comparison indicators do not change the live window.
    Explicit asOf remains available for historical RPC callers.
    """
    from .data.ohlcv import utc_now
    cutoff = utc_now()
    today = cutoff.date()
    as_of = _parse_date(params.get("asOf") or params.get("as_of"), today)
    raw = dict(params.get("config") or {})
    value = raw.get("liveStartDate", raw.get("live_start_date", as_of.isoformat()))
    try:
        start = dt.date.fromisoformat(value)
    except (TypeError, ValueError):
        raise ValueError("Choose a valid live start day (YYYY-MM-DD)") from None
    if start.isoformat() != value or start > as_of or as_of > today:
        raise ValueError("Live start day must be on or before the evaluation day, which cannot be after today")
    raw.update(startDate=start.isoformat(), endDate=as_of.isoformat(), compareTrends=[])
    request = {**params, "asOf": as_of.isoformat(), "config": raw}
    snapshot = _method_live_snapshot(request, progress=lambda label, value: _notify(
        "progress", {"label": label, "value": min(1.0, max(0.0, value)) * 0.25}))
    try:
        tracking = _method_backtest(request, progress_callback=lambda label, value: _notify(
            "progress", {"label": f"Tracking · {label}", "value": 0.25 + min(1.0, max(0.0, value)) * 0.75}))
    except ValueError as exc:
        # A new daily tracker can start before its first candle has closed.
        # Keep the current signal available, without inventing a return.
        cfg = _build_config(raw)
        first_close = pd.Timestamp(start, tz="UTC") + pd.Timedelta(cfg.bar_cadence.ccxt_timeframe.replace('d', 'D'))
        waiting_for_close = cfg.mode == "single_asset" and first_close > pd.Timestamp(cutoff)
        if str(exc) != "No confirmed candles in the selected backtest window" and not (
            waiting_for_close and str(exc).startswith("No confirmed ")
        ):
            raise
        tracking = {**_market_info(_build_config(raw)), "strategies": [], "skippedStrategies": [],
                    "equityStrategy": [], "heldAsset": [], "buyAndHold": {}, "benchmarks": {},
                    "metricsStrategy": _metrics_to_dict(PerformanceMetrics()), "metricsBuyAndHold": {},
                    "metricsBenchmarks": {}, "metricLabels": list(METRIC_LABELS),
                    "forcedRotations": [], "notes": ["Waiting for the first confirmed candle in the live window."], "coins": []}
    _notify("progress", {"label": "Done", "value": 1.0})
    return {**snapshot, "tracking": {**tracking, "startDate": start.isoformat(), "endDate": as_of.isoformat()}}


def _method_live_snapshot(params: dict[str, Any], *, progress=None) -> dict[str, Any]:
    from .data.ohlcv import confirmed_frame, utc_now
    if progress is None:
        progress = lambda label, value: _notify("progress", {"label": label, "value": value})
    cfg = _build_config(params.get("config") or {})
    if cfg.mode == "single_asset":
        return _method_single_live(cfg, params, progress=progress)
    as_of = _parse_date(params.get("asOf") or params.get("as_of"), cfg.end_date)
    now = utc_now()

    def confirmed(frame):
        frame = confirmed_frame(frame, cfg.cadence.ccxt_timeframe, now)
        return frame.loc[frame.index < pd.Timestamp(as_of + dt.timedelta(days=1))] if not frame.empty else frame

    registry = RankingRegistry(
        get_default_cache(),
        progress=lambda label: progress(label, 0.05),
    )
    # The coins come from the ranking the next bar runs on: the one the replay's decision at the last closed
    # bar reads (universe.walk_lists). As of today that bar is yesterday's and the ranking today's; for a past
    # asOf, whose own bar has closed, it is the next day's.
    ranking_day = as_of
    if ranking_known_before_open(cfg.cadence, cfg.ranking_resolution):
        ranking_day = min(as_of + dt.timedelta(days=1), now.date())
    resolved = registry.get_top_n(
        ranking_day,
        cfg.top_n,
        source=cfg.ranking_source,
        exclude_stablecoins=cfg.exclude_stablecoins,
        exclude_wrapped=cfg.exclude_wrapped,
        exclude_top_n=cfg.exclude_top_n,
        resolution=cfg.ranking_resolution,
        exclude_symbols=cfg.exclude_symbols,
    )
    coins = resolved.coins
    if not coins:
        return {"asOf": as_of.isoformat(), "provider": resolved.provider,
                "universe": [], "symbols": [], "scoreMatrix": [], "scores": {}, "best": None}

    # Pull enough history to resolve the trend signal at `as_of`. Smithery
    # indicators self-tune on trailing windows and need far more bars than
    # the EMA cross. Intraday timeframes need fewer calendar days to cover
    # the same bar count.
    from .backtest.signals import warmup_bars
    timeframe = cfg.cadence.ccxt_timeframe
    def history_start(indicator):
        from .config import TotalBreakoutConfig
        bars_needed = (max(indicator.trend_length, indicator.entry_length, indicator.exit_length) * 8
                       if isinstance(indicator, TotalBreakoutConfig)
                       else warmup_bars(indicator) or indicator.ema_cross.slow_length * 8)
        if cfg.cadence.is_intraday:
            bars_per_day = {"1h": 24, "4h": 6, "12h": 2}[cfg.cadence.value]
            lookback_days = max(bars_needed // bars_per_day + 2, 3)
        else:
            lookback_days = max(bars_needed, 180)
        if isinstance(indicator, TotalBreakoutConfig) or not warmup_bars(indicator):
            return as_of - dt.timedelta(days=lookback_days)
        # A Smithery vote today uses the same history block as a backtest or live replay of today.
        from .backtest.signals import block_start
        return block_start(as_of).date() - dt.timedelta(days=lookback_days)

    start = history_start(cfg.indicator)
    fetcher = OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval)

    frames: dict[str, pd.DataFrame] = {}
    total = len(coins)
    # Daily bars carry their hourly path, as in the backtest (engine.run), for the coin-vs-coin candles.
    paths = None if cfg.cadence.is_intraday else getattr(fetcher, "get_path", None)
    for i, coin in enumerate(coins):
        progress(f"OHLCV {coin.symbol}", 0.05 + 0.7 * (i + 1) / max(total, 1))
        ref = CoinRef(cg_id=coin.cg_id, symbol=coin.symbol)
        try:
            series = fetcher.get_series(ref, start, as_of, timeframe)
        except Exception:  # noqa: BLE001
            continue
        frame = confirmed(series.frame)
        if not frame.empty:
            frames[coin.symbol] = with_path(frame, paths, ref, start, as_of) if callable(paths) else frame

    symbols = [c.symbol for c in coins if c.symbol in frames]
    include_usd = bool(cfg.include_usd)
    order = symbols + ([USD_SYMBOL] if include_usd else [])

    # The higher filter reads TOTAL as of today. SCES evaluates ranks below
    # an excluded head, but the market is the whole top-N, so the head's
    # candles are fetched for the index alone.
    market_filter: dict[str, Any] = {"enabled": bool(cfg.market_filter), "bullish": None}
    if cfg.market_filter:
        from .backtest.engine import _market_start
        from .backtest.market import market_gate, market_index
        from .backtest.universe import UniverseSnapshot, market_warmup_snapshots

        market_indicator = cfg.market_indicator or cfg.indicator
        # The same history and the same TOTAL as a backtest of today: from the
        # engine's warm-up start, each earlier day with its own cached ranking.
        market_start = (_market_start(replace(cfg, start_date=as_of)) if cfg.market_indicator is not None
                        else history_start(market_indicator))
        # TOTAL's newest snapshot belongs to asOf's own bar, like every bar of the backtest's TOTAL.
        market_coins = list(coins)
        if cfg.exclude_top_n > 0 or cfg.exclude_symbols or ranking_day != as_of:
            market_coins = registry.get_top_n(
                as_of, cfg.top_n, source=cfg.ranking_source,
                exclude_stablecoins=cfg.exclude_stablecoins,
                exclude_wrapped=cfg.exclude_wrapped, exclude_top_n=0,
                resolution=cfg.ranking_resolution,
            ).coins
        snapshots = market_warmup_snapshots(
            registry, warm_start=market_start, start=as_of, cadence=cfg.cadence, top_n=cfg.top_n,
            source=cfg.ranking_source, exclude_stablecoins=cfg.exclude_stablecoins,
            exclude_wrapped=cfg.exclude_wrapped, resolution=cfg.ranking_resolution,
        ) + [UniverseSnapshot(on_date=as_of, provider=resolved.provider, coins=market_coins)]
        wanted: dict[str, Any] = {}
        for snap in snapshots:
            for coin in snap.coins:
                wanted.setdefault(coin.symbol, coin)
        market_frames: dict[str, pd.DataFrame] = {}
        for coin in wanted.values():
            if coin.symbol in frames and market_start == start:
                market_frames[coin.symbol] = frames[coin.symbol]
                continue
            try:
                series = fetcher.get_series(CoinRef(cg_id=coin.cg_id, symbol=coin.symbol), market_start, as_of, timeframe)
            except Exception:  # noqa: BLE001
                continue
            frame = confirmed(series.frame)
            if not frame.empty:
                market_frames[coin.symbol] = frame
        total = market_index(snapshots, market_frames)
        gate = market_gate(total, market_indicator)
        market_filter["bullish"] = bool(int(gate.iloc[-1]) == 1) if not gate.empty else False

    # The backtest's own rule and scoring (universe.comparable, engine.pair_scores) on the bar that just
    # closed, so today's pick is the pick the replay makes on that bar: only coins with a candle on it are
    # compared, each pair is judged once with the higher rank as A, and a tie goes to the higher rank.
    master = pd.DatetimeIndex(sorted(set().union(*[set(f.index) for f in frames.values()]))) if frames else pd.DatetimeIndex([])
    last = len(master) - 1
    compared = comparable(symbols, candle_mask(frames, master), last) if len(master) else []
    judged: dict[tuple[str, str | None], np.ndarray] = {}

    def votes(a_sym: str, b_sym: str | None) -> np.ndarray:
        if (a_sym, b_sym) not in judged:
            sig = pair_signal(frames[a_sym], frames[b_sym] if b_sym else None, cfg.indicator)
            judged[a_sym, b_sym] = sig.reindex(master, fill_value=0).to_numpy(dtype=np.int8)
        return judged[a_sym, b_sym]

    # Compared coins whose closed bar has no hourly path yet although the bar before has one: the venue is
    # late with the last hours. The bot waits for them inside its retry window, so that the replay later
    # judges the same candle.
    path_missing: list[str] = []
    if callable(paths) and len(compared) > 1 and last >= 1:
        from smithery.ratio import path_columns
        for sym in compared:
            columns = path_columns(frames[sym])
            if not columns:
                continue
            whole = frames[sym][columns].notna().all(axis=1).reindex(master[last - 1:last + 1], fill_value=False)
            if bool(whole.iloc[0]) and not bool(whole.iloc[1]):
                path_missing.append(sym)

    scores: dict[str, int] = {s: 0 for s in order}
    best = USD_SYMBOL if include_usd else None  # nobody to compare: cash, like the walk
    if compared:
        scored, points = pair_scores(compared, votes, last, last + 1, include_usd)
        scores.update({s: int(p) for s, p in zip(scored, points[0])})
        best = str(top_scored(scored, points)[0])
    if market_filter["enabled"] and not market_filter["bullish"]:
        best = USD_SYMBOL

    # NxN matrix over `order`: 1 when the row beats the column, null on the diagonal and for a coin that was
    # not compared (no candle on the closed bar).
    rank = {s: i for i, s in enumerate(compared)}

    def cell(a: str, b: str) -> int | None:
        if a == b:
            return None
        if a == USD_SYMBOL:
            return int(1 - votes(b, None)[last]) if b in rank else None
        if b == USD_SYMBOL:
            return int(votes(a, None)[last]) if a in rank else None
        if a not in rank or b not in rank:
            return None
        return int(votes(a, b)[last]) if rank[a] < rank[b] else int(1 - votes(b, a)[last])

    matrix: list[list[int | None]] = [[cell(a, b) for b in order] for a in order]

    # Portfolio mode: tomorrow's weights by the backtest's own rule. `best` keeps its single-pick meaning.
    portfolio_target = None
    if cfg.portfolio is not None:
        from .backtest.engine import portfolio_target as target_of
        trend = {s: bool(votes(s, None)[last] == 1) for s in compared}
        portfolio_target = ({USD_SYMBOL: 1.0} if not compared or (market_filter["enabled"] and not market_filter["bullish"])
                            else target_of(compared, {s: scores.get(s, 0) for s in compared}, trend, cfg.portfolio))

    universe = []
    for c in coins:
        d = _coin_to_dict(c)
        d["score"] = scores.get(c.symbol) if c.symbol in rank else None
        d["hasData"] = c.symbol in frames
        universe.append(d)

    return {
        "asOf": as_of.isoformat(),
        "provider": resolved.provider,
        "universe": universe,
        "symbols": order,
        "scoreMatrix": matrix,
        "scores": scores,
        "best": best,
        "marketFilter": market_filter,
        "pathMissing": path_missing,
        **({"portfolioTarget": portfolio_target} if portfolio_target is not None else {}),
    }


# ── method: backtest ─────────────────────────────────────────────────────────


def _market_info(cfg: RunConfig) -> dict[str, Any]:
    return {"mode": cfg.mode, "singleAsset": asdict(cfg.single_asset) if cfg.mode == "single_asset" else None}


def _method_pair_markets(params: dict[str, Any]) -> dict[str, Any]:
    from .data.ohlcv import SINGLE_ASSET_EXCHANGES
    exchange = str(params.get("exchange", "coinbase"))
    fetcher = OhlcvFetcher(get_default_cache())
    return {"exchange": exchange, "exchanges": list(SINGLE_ASSET_EXCHANGES),
            "pairs": fetcher.pair_markets(exchange)}


def _method_single_live(cfg: RunConfig, params: dict[str, Any], *, progress) -> dict[str, Any]:
    from dataclasses import replace
    from .backtest.engine import _fetch_start
    from .backtest.signals import direction_signal, warmup_bars
    from .data.ohlcv import confirmed_frame, utc_now
    cfg = replace(cfg, cadence=cfg.bar_cadence)
    market = cfg.single_asset
    base, quote = market.pair.split("/")
    as_of = _parse_date(params.get("asOf") or params.get("as_of"), cfg.end_date)
    bars = warmup_bars(cfg.indicator) or max(180, cfg.indicator.ema_cross.slow_length * 8)
    days = math.ceil(bars * pd.Timedelta(market.timeframe.replace('d', 'D')).total_seconds() / 86400) + 2
    cfg = replace(cfg, start_date=min(cfg.start_date, as_of - dt.timedelta(days=days)))
    now = utc_now()
    fetcher = OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval)
    progress(f"Fetching {market.pair}", 0.1)
    data = fetcher.get_pair_series(market.exchange, market.pair, _fetch_start(cfg), as_of, market.timeframe)
    frame = confirmed_frame(data.frame, market.timeframe, now)
    frame = frame.loc[frame.index < pd.Timestamp(as_of + dt.timedelta(days=1))] if not frame.empty else frame
    if frame.empty:
        raise ValueError(f"No confirmed candles for {market.pair} in the selected window")
    if any(not math.isfinite(float(price)) or price <= 0 for price in frame["close"]):
        raise ValueError("Single Asset candles contain invalid close prices")
    verdict = int(direction_signal(frame, cfg.indicator).iloc[-1])
    position = "long" if verdict > 0 else "short" if verdict < 0 and market.direction == "long_short" else "cash"
    stamp = pd.Timestamp(frame.index[-1])
    delta = pd.Timedelta(market.timeframe.replace('d', 'D'))
    closed_at = (stamp + delta).isoformat() + "Z"
    progress("Current signal ready", 1.0)
    return {**_market_info(cfg), "asOf": as_of.isoformat(), "provider": market.exchange,
            "universe": [], "symbols": [base, quote], "scoreMatrix": [], "scores": {},
            "best": base if position == "long" else f"Short {base}" if position == "short" else quote,
            "marketFilter": {"enabled": False, "bullish": None},
            "singleAssetSignal": {"signal": "bullish" if verdict > 0 else "bearish" if verdict < 0 else "neutral",
                                  "position": position, "close": float(frame["close"].iloc[-1]),
                                  "closedAt": closed_at}}


def _method_backtest(params: dict[str, Any], *, progress_callback=None) -> dict[str, Any]:
    cfg = _build_config(params.get("config") or {})
    progress_value = 0.0

    def progress(label: str, value: float) -> None:
        nonlocal progress_value
        progress_value = float(value)
        if progress_callback is not None:
            progress_callback(label, progress_value)
        else:
            _notify("progress", {"label": label, "value": progress_value})

    engine = BacktestEngine(
        registry=RankingRegistry(
            get_default_cache(), progress=lambda label: progress(label, progress_value),
        ),
        ohlcv=OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval),
    )

    result = engine.run(cfg, progress=progress)
    intraday = cfg.bar_cadence.is_intraday

    # Successful strategies only. Scalars still represent the configured
    # primary, and stay empty if that primary could not be evaluated.
    strategies = [
        {
            "key": run.key,
            "label": run.label,
            "equityStrategy": _series_to_points(run.equity_strategy, intraday),
            "heldAsset": _held_to_points(run.held_asset, intraday),
            "trades": _trades_to_dicts(run.trades, intraday),
            "metricsStrategy": _metrics_to_dict(run.metrics_strategy),
            "forcedRotations": [d.isoformat() for d in run.forced_rotations],
            **({"heldWeights": _weights_to_points(run.weights, intraday)} if run.weights is not None else {}),
        }
        for run in result.strategies
    ]

    return {
        "strategies": strategies,
        **({"heldWeights": _weights_to_points(result.held_weights, intraday)} if result.held_weights is not None else {}),
        **_market_info(cfg),
        "skippedStrategies": result.skipped_strategies,
        "equityStrategy": _series_to_points(result.equity_strategy, intraday),
        "heldAsset": _held_to_points(result.held_asset, intraday),
        "buyAndHold": {sym: _series_to_points(s, intraday) for sym, s in result.buy_and_hold.items()},
        "benchmarks": {name: _series_to_points(s, intraday) for name, s in result.benchmarks.items()},
        "metricsStrategy": _metrics_to_dict(result.metrics_strategy),
        "metricsBuyAndHold": {sym: _metrics_to_dict(m) for sym, m in result.metrics_buy_and_hold.items()},
        "metricsBenchmarks": {name: _metrics_to_dict(m) for name, m in result.metrics_benchmarks.items()},
        "metricLabels": list(METRIC_LABELS),
        "forcedRotations": [d.isoformat() for d in result.forced_rotations],
        "notes": list(result.notes),
        "coins": [_coin_to_dict(c) for c in result.universe.unique_coins()],
    }


# ── method: fund ─────────────────────────────────────────────────────────────

METHOD_LABELS = {
    "equal": "Equal",
    "fixed": "Fixed %",
    "profit": "Profit",
    "sharpe": "Sharpe",
    "sortino": "Sortino",
    "return_drawdown": "Return / drawdown",
    "inverse_volatility": "Inverse volatility",
    "kelly": "Kelly",
    "combined": "Combined",
}
#: Member equity by its exact settings: choosing another method, lookback or cap
#: reruns nothing, and a member whose settings changed runs again.
_MEMBER_CURVES: dict[str, pd.Series] = {}


def _member_curve(raw: dict[str, Any], label: str, progress) -> pd.Series:
    key = json.dumps({"version": __version__, "config": raw}, sort_keys=True, default=str)
    cached = _MEMBER_CURVES.get(key)
    if cached is not None:
        return cached
    cfg = _build_config({**raw, "compareTrends": []})
    engine = BacktestEngine(
        registry=RankingRegistry(get_default_cache(), progress=lambda text: progress(f"{label}: {text}", None)),
        ohlcv=OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval),
    )
    result = engine.run(cfg, progress=lambda text, value: progress(f"{label}: {text}", value))
    curve = result.equity_strategy
    if curve is None or curve.empty:
        raise ValueError(f"{label} has no equity curve in its window: " + "; ".join(result.notes[-2:]))
    _MEMBER_CURVES[key] = curve
    return curve


def _fund_rule(raw: dict[str, Any], fee_rate: float, method: str | None = None) -> funds.FundRule:
    return funds.FundRule(
        method=method or str(raw.get("method", "equal")),
        lookback_days=int(raw.get("lookbackDays", 180)),
        rebalance=str(raw.get("rebalance", "monthly")),
        cap_pct=float(raw.get("capPct", 50)),
        kelly_fraction=float(raw.get("kellyFraction", 0.25)),
        fixed_pct={str(k): float(v) for k, v in (raw.get("fixedPct") or {}).items()},
        fee_rate=fee_rate,
    )


def _fund_run_dict(run: funds.FundRun, method: str) -> dict[str, Any]:
    empty = pd.Series(dtype=float)
    return {
        "key": method,
        "label": METHOD_LABELS[method],
        "equityStrategy": _series_to_points(run.equity if len(run.equity) else empty),
        "heldAsset": [],
        "trades": [],
        "metricsStrategy": _metrics_to_dict(compute_metrics(run.equity)),
        "forcedRotations": [],
    }


def _method_fund(params: dict[str, Any], *, progress_callback=None) -> dict[str, Any]:
    """A fund over its members' own equity. ``live`` runs them through today and holds money from ``liveStartDate``."""
    members = params.get("members") or []
    if not members:
        raise ValueError("This fund has no systems left. Add a system in its settings.")
    rule_raw = params.get("rule") or {}
    amount = float(params.get("amount") or 0)
    if amount <= 0:
        raise ValueError("A fund needs an amount above 0.")
    live = bool(params.get("live"))
    today = dt.date.today()
    step = 0.9 / len(members)

    def progress(index: int):
        def report(label: str, value: float | None) -> None:
            base = 0.02 + index * step
            payload = {"label": label, "value": base + step * (0.5 if value is None else float(value))}
            if progress_callback is not None:
                progress_callback(payload["label"], payload["value"])
            else:
                _notify("progress", payload)
        return report

    curves: dict[str, pd.Series] = {}
    names: dict[str, str] = {}
    fees: list[float] = []
    for i, member in enumerate(members):
        member_id, name = str(member["id"]), str(member.get("name") or member["id"])
        raw = dict(member.get("config") or {})
        if live:
            raw["endDate"] = today.isoformat()
        names[member_id] = name
        curves[member_id] = _member_curve(raw, name, progress(i))
        cfg = _build_config(raw)
        fees.append(float(cfg.fee_rate) + float(cfg.slippage_rate))
    frame, limited_by = funds.aligned_curves(curves)
    if frame.empty or len(frame) < 2:
        raise ValueError("The members have no days in common.")
    fee_rate = sum(fees) / len(fees)
    chosen = str(rule_raw.get("method", "equal"))
    start = None
    if live:
        start = pd.Timestamp(str(params.get("liveStartDate") or today.isoformat()))
        if start > frame.index[-1]:
            start = frame.index[-1]

    # The chosen method first, Equal always beside it, the rest for comparison.
    methods = [chosen] + [m for m in funds.METHODS if m != chosen and (m != "fixed" or rule_raw.get("fixedPct"))]
    runs = {m: funds.run_fund(frame, _fund_rule(rule_raw, fee_rate, m), start) for m in methods}
    main = runs[chosen]
    rule = _fund_rule(rule_raw, fee_rate)
    value = amount * float(main.equity.iloc[-1]) if len(main.equity) else amount
    held = {k: value * main.final_weights.get(k, 0.0) for k in frame.columns}
    target = funds.split_dollars(main.target_weights, value)
    last_day = frame.index[-1].date()
    upcoming = funds.next_rebalance(last_day, rule.rebalance)

    notes = [
        "Members use today's parameters, so their own optimization is inside these curves, and weighting cannot remove it.",
    ]
    if limited_by:
        notes.append(f"{names[limited_by]} starts last and limits the common window.")
    removed = len(params.get("removed") or [])
    if removed:
        notes.append(f"{removed} deleted {'system is' if removed == 1 else 'systems are'} no longer part of this fund.")

    monte_carlo = None
    try:
        from smithery.system_gauntlet import bootstrap
        # Over the whole history: a Live page may hold money for a few days only.
        trial = bootstrap(funds.run_fund(frame, rule).equity, 300, 20261106)
        monte_carlo = {k: trial.get(k) for k in ("cagr_p5", "cagr_p50", "dd_p50", "dd_p95", "loss_1y", "loss_3y", "failed")}
    except Exception as exc:  # noqa: BLE001 — the result stands without it, and says so
        notes.append(f"Monte Carlo could not run: {exc}")

    corr = funds.correlation(frame)
    strategies = [_fund_run_dict(runs[m], m) for m in methods]
    return {
        **strategies[0],
        "strategies": strategies,
        "skippedStrategies": [],
        "buyAndHold": {},
        "benchmarks": {},
        "metricsBuyAndHold": {},
        "metricsBenchmarks": {},
        "metricLabels": list(METRIC_LABELS),
        "notes": notes,
        "coins": [],
        "fund": {
            "method": chosen,
            "windowStart": frame.index[0].date().isoformat(),
            "windowEnd": last_day.isoformat(),
            "limitedBy": names.get(limited_by) if limited_by else None,
            "live": live,
            "startDate": (start or frame.index[0]).date().isoformat(),
            "amount": amount,
            "value": value,
            "cash": value - sum(held.values()),
            "cost": amount * main.cost,
            "nextRebalance": upcoming.isoformat() if upcoming else None,
            "members": [
                {"id": k, "name": names[k], "heldDollars": held[k], "targetDollars": target.get(k, 0),
                 "targetShare": main.target_weights.get(k, 0.0)}
                for k in frame.columns
            ],
            "rebalances": [
                {"date": r.day.date().isoformat(), "cash": amount * r.cash, "cost": amount * r.cost,
                 "values": {k: amount * v for k, v in r.values.items()}}
                for r in reversed(main.rebalances)
            ],
            "correlation": {"ids": list(corr.columns), "names": [names[k] for k in corr.columns],
                            "matrix": [[_finite(float(v)) for v in row] for row in corr.to_numpy()]},
            "robustness": {**funds.robustness(frame, rule), "monteCarlo": monte_carlo},
        },
    }


# ── method: cache stats / clear ──────────────────────────────────────────────


def _method_cache_stats(_params: dict[str, Any]) -> dict[str, Any]:
    cache = get_default_cache()
    path = cache.path
    size = path.stat().st_size if path.exists() else 0
    with cache._conn() as con:  # noqa: SLF001 - intentional internal access
        rankings = con.execute("SELECT COUNT(*) FROM rankings").fetchone()[0]
        ohlcv = con.execute("SELECT COUNT(*) FROM ohlcv").fetchone()[0]
        ohlcv += con.execute("SELECT COUNT(*) FROM ohlcv_intraday").fetchone()[0]
        coins = con.execute("SELECT COUNT(*) FROM coin_map").fetchone()[0]
    return {
        "path": str(path),
        "sizeBytes": int(size),
        "rankings": int(rankings),
        "ohlcv": int(ohlcv),
        "coins": int(coins),
    }


def _method_clear_cache(params: dict[str, Any]) -> dict[str, Any]:
    scope = (params.get("scope") or "all").lower()
    cache = get_default_cache()
    with cache._conn() as con:  # noqa: SLF001
        if scope in ("rankings", "all"):
            con.execute("DELETE FROM rankings")
        if scope in ("ohlcv", "all"):
            con.execute("DELETE FROM ohlcv")
            con.execute("DELETE FROM ohlcv_intraday")
            # The candles' bookkeeping goes with them: a history floor an
            # earlier exchange reported would otherwise bound the fresh download.
            con.execute("DELETE FROM meta WHERE key LIKE 'ohlcv_first%' OR key LIKE 'ohlcv_confirmed_v1:%'")
        if scope == "all":
            con.execute("DELETE FROM coin_map")
            con.execute("DELETE FROM meta")
    return {"ok": True, "scope": scope}


# ── method: daily ranking archive ────────────────────────────────────────────


_ARCHIVE_JOB = None


def _archive_job():
    global _ARCHIVE_JOB
    if _ARCHIVE_JOB is None:
        from .data.ranking.archive import DailyArchiveJob
        from .data.ranking.cmc_snapshot import CmcSnapshotProvider

        _ARCHIVE_JOB = DailyArchiveJob(
            lambda: CmcSnapshotProvider(get_default_cache()),
            lambda status: _notify("ranking_archive", status),
        )
    return _ARCHIVE_JOB


def _method_ranking_archive_status(_params: dict[str, Any]) -> dict[str, Any]:
    return _archive_job().status(get_default_cache())


def _method_ranking_archive_build(params: dict[str, Any]) -> dict[str, Any]:
    start = params.get("start")
    end = params.get("end")
    return _archive_job().start(
        dt.date.fromisoformat(start) if start else None,
        dt.date.fromisoformat(end) if end else None,
    )


def _method_ranking_archive_cancel(_params: dict[str, Any]) -> dict[str, Any]:
    return _archive_job().cancel(get_default_cache())


_METHODS = {
    "pair_markets": _method_pair_markets,
    "ping": _method_ping,
    "live": _method_live,
    "backtest": _method_backtest,
    "fund": _method_fund,
    "universe": _method_universe,
    "cache_stats": _method_cache_stats,
    "clear_cache": _method_clear_cache,
    "ranking_archive_status": _method_ranking_archive_status,
    "ranking_archive_build": _method_ranking_archive_build,
    "ranking_archive_cancel": _method_ranking_archive_cancel,
}


# ── dispatch loop ────────────────────────────────────────────────────────────


def _handle(request: dict[str, Any]) -> None:
    req_id = request.get("id")
    method = request.get("method")
    params = request.get("params") or {}
    handler = _METHODS.get(method)
    if handler is None:
        _write({"id": req_id, "error": f"Unknown method: {method}"})
        return
    try:
        result = handler(params)
        _write({"id": req_id, "result": result})
    except Exception as exc:  # noqa: BLE001 - report everything back over the wire
        import traceback

        _write({"id": req_id, "error": str(exc), "trace": traceback.format_exc()})


def main() -> None:
    # The pair workers this engine starts (smithery.parallel) take one OpenBLAS thread each.
    from smithery import threads
    threads.one_for_children()
    # Signal readiness so the parent can verify the sidecar booted.
    _notify("ready", {"version": __version__})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            _write({"id": None, "error": f"Invalid JSON: {exc}"})
            continue
        _handle(request)


if __name__ == "__main__":
    main()
