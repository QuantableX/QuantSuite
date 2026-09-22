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
``cache_stats``    → row counts + db size
``clear_cache``    → wipe rankings / ohlcv / all
"""

from __future__ import annotations

import datetime as dt
import json
import math
import sys
import threading
from dataclasses import asdict
from typing import Any

import pandas as pd

from . import __version__
from .backtest.engine import USD_SYMBOL, BacktestEngine
from .backtest.metrics import METRIC_LABELS, PerformanceMetrics
from .backtest.signals import pair_signal
from .config import (
    Cadence,
    EmaCrossConfig,
    IndicatorConfig,
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


def _build_indicator(ind_raw: dict[str, Any]) -> IndicatorConfig:
    ema_raw = ind_raw.get("emaCross") or ind_raw.get("ema_cross") or {}
    return IndicatorConfig(
        trend=ind_raw.get("trend", "ema_cross"),
        ema_cross=EmaCrossConfig(
            src=ema_raw.get("src", "close"),
            fast_length=int(ema_raw.get("fastLength", ema_raw.get("fast_length", 12))),
            slow_length=int(ema_raw.get("slowLength", ema_raw.get("slow_length", 21))),
        ),
        aggregate=tuple(str(kind) for kind in (ind_raw.get("aggregate") or []) if kind),
    )


def _build_config(raw: dict[str, Any]) -> RunConfig:
    raw = raw or {}
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
        market_indicator = _build_indicator(market_raw) if market_raw is not None else None
    compare_raw = raw.get("compareTrends", raw.get("compare_trends")) or []
    compare = tuple(str(kind) for kind in compare_raw if kind)
    return RunConfig(
        top_n=int(raw.get("topN", raw.get("top_n", 5))),
        exclude_top_n=int(raw.get("excludeTopN", raw.get("exclude_top_n", 0))),
        cadence=Cadence(raw.get("cadence", "daily")),
        start_date=_parse_date(
            raw.get("startDate", raw.get("start_date")),
            today - dt.timedelta(days=365 * 3),
        ),
        end_date=_parse_date(raw.get("endDate", raw.get("end_date")), today),
        ranking_source=RankingSource(raw.get("rankingSource", raw.get("ranking_source", "auto"))),
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
    )
    return {
        "onDate": on_date.isoformat(),
        "provider": resolved.provider,
        "coins": [_coin_to_dict(c) for c in resolved.coins],
    }


# ── method: live (top-N + NxN score matrix + best asset) ─────────────────────


def _method_live(params: dict[str, Any]) -> dict[str, Any]:
    cfg = _build_config(params.get("config") or {})
    as_of = _parse_date(params.get("asOf") or params.get("as_of"), cfg.end_date)

    registry = RankingRegistry(
        get_default_cache(),
        progress=lambda label: _notify("progress", {"label": label, "value": 0.05}),
    )
    resolved = registry.get_top_n(
        as_of,
        cfg.top_n,
        source=cfg.ranking_source,
        exclude_stablecoins=cfg.exclude_stablecoins,
        exclude_wrapped=cfg.exclude_wrapped,
        exclude_top_n=cfg.exclude_top_n,
    )
    coins = resolved.coins
    if not coins:
        return {"asOf": as_of.isoformat(), "provider": resolved.provider,
                "universe": [], "symbols": [], "scoreMatrix": [], "best": None}

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
            bars_per_day = {"1m": 1440, "1h": 24, "4h": 6, "12h": 2}[cfg.cadence.value]
            lookback_days = max(bars_needed // bars_per_day + 2, 3)
        else:
            lookback_days = max(bars_needed, 180)
        return as_of - dt.timedelta(days=lookback_days)

    start = history_start(cfg.indicator)
    fetcher = OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval)

    frames: dict[str, pd.DataFrame] = {}
    total = len(coins)
    for i, coin in enumerate(coins):
        _notify("progress", {"label": f"OHLCV {coin.symbol}", "value": (i + 1) / max(total, 1)})
        try:
            series = fetcher.get_series(CoinRef(cg_id=coin.cg_id, symbol=coin.symbol), start, as_of, timeframe)
        except Exception:  # noqa: BLE001
            continue
        if not series.frame.empty:
            frames[coin.symbol] = series.frame

    symbols = [c.symbol for c in coins if c.symbol in frames]
    include_usd = bool(cfg.include_usd)
    order = symbols + ([USD_SYMBOL] if include_usd else [])

    # The higher filter reads TOTAL as of today. SCES evaluates ranks below
    # an excluded head, but the market is the whole top-N, so the head's
    # candles are fetched for the index alone.
    market_filter: dict[str, Any] = {"enabled": bool(cfg.market_filter), "bullish": None}
    if cfg.market_filter:
        from .backtest.market import market_gate, market_index
        from .backtest.universe import UniverseSnapshot

        market_indicator = cfg.market_indicator or cfg.indicator
        market_start = history_start(market_indicator)
        market_coins = list(coins)
        if cfg.exclude_top_n > 0:
            market_coins = registry.get_top_n(
                as_of, cfg.top_n, source=cfg.ranking_source,
                exclude_stablecoins=cfg.exclude_stablecoins,
                exclude_wrapped=cfg.exclude_wrapped, exclude_top_n=0,
            ).coins
        market_frames: dict[str, pd.DataFrame] = {}
        for coin in market_coins:
            if coin.symbol in frames and market_start == start:
                market_frames[coin.symbol] = frames[coin.symbol]
                continue
            try:
                series = fetcher.get_series(CoinRef(cg_id=coin.cg_id, symbol=coin.symbol), market_start, as_of, timeframe)
            except Exception:  # noqa: BLE001
                continue
            if not series.frame.empty:
                market_frames[coin.symbol] = series.frame
        total = market_index([UniverseSnapshot(on_date=as_of, provider=resolved.provider, coins=market_coins)], market_frames)
        gate = market_gate(total, market_indicator)
        market_filter["bullish"] = bool(int(gate.iloc[-1]) == 1) if not gate.empty else False

    def beats(a_sym: str, b_sym: str | None) -> int | None:
        a_df = frames.get(a_sym)
        if a_df is None:
            return None
        b_df = frames.get(b_sym) if b_sym else None
        sig = pair_signal(a_df, b_df, cfg.indicator)
        if sig.empty:
            return None
        return int(sig.iloc[-1])

    # Evaluate each unordered pair exactly once, in universe order, and
    # derive the mirrored cell as its complement -- the same rule as
    # BacktestEngine._score_universe. EMA is not reciprocal-invariant
    # (EMA(1/x) != 1/EMA(x)), so evaluating a-vs-b and b-vs-a separately
    # could award the pair's point to both coins or to neither, and make
    # live pick a different best asset than the backtest did on the same
    # data. Evaluating once also halves the pair_signal calls, which
    # matters for the self-tuning Smithery indicators.
    # USD sorts last in `order`, so `a` below is always a real symbol.
    cells: dict[tuple[str, str], int | None] = {}
    for i, a in enumerate(order):
        for b in order[i + 1:]:
            v = beats(a, None if b == USD_SYMBOL else b)
            cells[(a, b)] = v
            cells[(b, a)] = None if v is None else (0 if v == 1 else 1)

    # NxN matrix over `order`; diagonal is null.
    matrix: list[list[int | None]] = []
    scores: dict[str, int] = {s: 0 for s in order}
    for a in order:
        row: list[int | None] = []
        for b in order:
            if a == b:
                row.append(None)
                continue
            cell = cells[(a, b)]
            row.append(cell)
            if cell == 1:
                scores[a] += 1
        matrix.append(row)

    best = max(order, key=lambda s: (scores.get(s, -1), -order.index(s))) if order else None
    if market_filter["enabled"] and not market_filter["bullish"]:
        best = USD_SYMBOL

    universe = []
    for c in coins:
        d = _coin_to_dict(c)
        d["score"] = scores.get(c.symbol)
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
    }


# ── method: backtest ─────────────────────────────────────────────────────────


def _method_backtest(params: dict[str, Any]) -> dict[str, Any]:
    cfg = _build_config(params.get("config") or {})
    progress_value = 0.0

    def progress(label: str, value: float) -> None:
        nonlocal progress_value
        progress_value = float(value)
        _notify("progress", {"label": label, "value": progress_value})

    engine = BacktestEngine(
        registry=RankingRegistry(
            get_default_cache(), progress=lambda label: progress(label, progress_value),
        ),
        ohlcv=OhlcvFetcher(get_default_cache(), min_request_interval=cfg.min_request_interval),
    )

    result = engine.run(cfg, progress=progress)
    intraday = cfg.cadence.is_intraday

    # Successful strategies only. Scalars still represent the configured
    # primary, and stay empty if that primary could not be evaluated.
    strategies = [
        {
            "key": run.key,
            "label": run.label,
            "equityStrategy": _series_to_points(run.equity_strategy, intraday),
            "heldAsset": _held_to_points(run.held_asset, intraday),
            "metricsStrategy": _metrics_to_dict(run.metrics_strategy),
            "forcedRotations": [d.isoformat() for d in run.forced_rotations],
        }
        for run in result.strategies
    ]

    return {
        "strategies": strategies,
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
        if scope == "all":
            con.execute("DELETE FROM coin_map")
            con.execute("DELETE FROM meta")
    return {"ok": True, "scope": scope}


_METHODS = {
    "ping": _method_ping,
    "live": _method_live,
    "backtest": _method_backtest,
    "universe": _method_universe,
    "cache_stats": _method_cache_stats,
    "clear_cache": _method_clear_cache,
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
