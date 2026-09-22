"""Offline, deterministic engine benchmark; no downloads or user cache writes.

Run from sidecars/python: python -m rotation_lab.benchmark --output result.json
An optional --baseline-engine path loads a saved engine.py for exact comparison.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import time
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import pandas as pd

from .backtest import engine
from .backtest.universe import UniverseSnapshot, UniverseTimeline
from .config import Cadence, IndicatorConfig, RunConfig
from .data.ranking.base import RankedCoin


def fixture(bars=12_000, trends=("ema_cross", "dc", "extremes", "hilbert")):
    index = pd.date_range("2020-01-01", periods=bars + 500, freq="h")
    frames = {}
    symbols = ["BTC", "AAA", "BBB", "CCC", "DDD", "EEE"]
    for seed, symbol in enumerate(symbols):
        rng = np.random.default_rng(seed)
        close = 100 * np.exp(np.cumsum(rng.normal(0.0001, 0.01, len(index))))
        frames[symbol] = pd.DataFrame({
            "open": close * (1 + rng.normal(0, 0.002, len(index))),
            "high": close * 1.02, "low": close * 0.98,
            "close": close, "volume": 1.0,
        }, index=index)
    config = RunConfig(
        top_n=5, cadence=Cadence("1h"), start_date=index[500].date(),
        end_date=index[-1].date(), indicator=IndicatorConfig(trend=trends[0]),
        compare_trends=tuple(trends[1:]), fee_rate=0.001, slippage_rate=0.0005,
    )
    snapshots = []
    for i, day in enumerate(pd.date_range(config.start_date, config.end_date, freq="7D")):
        order = symbols[i % 6:] + symbols[:i % 6]
        coins = [RankedCoin(rank=j + 1, cg_id=None, symbol=s, name=s,
                            market_cap=1e9 / (j + 1), price=1.0)
                 for j, s in enumerate(order[:5])]
        snapshots.append(UniverseSnapshot(day.date(), "benchmark", coins))
    return config, UniverseTimeline(snapshots), frames


def run(module, data):
    config, universe, frames = data

    def get_series(ref, start, end, timeframe):
        frame = frames[ref.symbol]
        return SimpleNamespace(frame=frame.loc[
            (frame.index >= pd.Timestamp(start)) &
            (frame.index < pd.Timestamp(end) + pd.Timedelta(days=1))])

    with patch.object(module, "build_universe_timeline", return_value=universe):
        return module.BacktestEngine(
            registry=object(), ohlcv=SimpleNamespace(get_series=get_series)).run(config)


def assert_equal(actual, expected):
    assert actual.notes == expected.notes
    assert actual.skipped_strategies == expected.skipped_strategies
    assert [r.key for r in actual.strategies] == [r.key for r in expected.strategies]
    for a, b in zip(actual.strategies, expected.strategies):
        pd.testing.assert_series_equal(a.equity_strategy, b.equity_strategy, check_exact=True)
        pd.testing.assert_series_equal(a.held_asset, b.held_asset, check_exact=True)
        assert a.forced_rotations == b.forced_rotations
        assert asdict(a.metrics_strategy) == asdict(b.metrics_strategy)
    for field in ("buy_and_hold", "benchmarks"):
        left, right = getattr(actual, field), getattr(expected, field)
        assert left.keys() == right.keys()
        for key in left:
            pd.testing.assert_series_equal(left[key], right[key], check_exact=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline-engine", type=Path)
    parser.add_argument("--bars", type=int, default=12_000)
    parser.add_argument("--repeat", type=int, default=1, help="Also measure subsequent requests in the same process")
    args = parser.parse_args()
    # Import registry before timing; app sidecars keep it loaded across requests.
    engine.warmup_bars(IndicatorConfig())
    baseline = None
    if args.baseline_engine:
        import sys
        name = "rotation_lab.backtest._benchmark_baseline"
        spec = importlib.util.spec_from_file_location(name, args.baseline_engine)
        baseline = importlib.util.module_from_spec(spec)
        sys.modules[name] = baseline
        spec.loader.exec_module(baseline)
    records = []
    for trends in [("ema_cross",), ("ema_cross", "dc", "extremes", "hilbert")]:
        data = fixture(args.bars, trends)
        record = {"bars": args.bars, "indicators": trends}
        if baseline:
            start = time.perf_counter()
            expected = run(baseline, data)
            record["before_seconds"] = time.perf_counter() - start
        start = time.perf_counter()
        actual = run(engine, data)
        record["after_seconds"] = time.perf_counter() - start
        record["scored_bars"] = len(actual.equity_strategy)
        repeat_seconds = []
        for _ in range(max(0, args.repeat - 1)):
            start = time.perf_counter()
            repeated = run(engine, data)
            repeat_seconds.append(time.perf_counter() - start)
            assert_equal(repeated, actual)
        if repeat_seconds:
            record["repeat_seconds"] = repeat_seconds
        if baseline:
            assert_equal(actual, expected)
            record["exact_match"] = True
            record["speedup"] = record["before_seconds"] / record["after_seconds"]
        records.append(record)
        print(json.dumps(record), flush=True)
    if args.output:
        args.output.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
