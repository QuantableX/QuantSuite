"""The aggregate trend signal: the average of several indicators' ±1 votes,
bullish above 0, bearish below 0, held at 0."""
import datetime as dt
import unittest
from dataclasses import dataclass
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.backtest.signals import aggregate_signal, trend_signal, warmup_bars
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import EmaCrossConfig, IndicatorConfig, RunConfig
from rotation_lab.data.ranking.base import RankedCoin
from smithery.indicators import REGISTRY, WARMUP_BARS

SYMBOLS = ("AAA", "BBB", "CCC")


def _frame(seed: int, n: int = 1000) -> pd.DataFrame:
    rng = np.random.default_rng(seed)
    close = np.exp(np.cumsum(rng.normal(0.001, 0.03, n))) * 100
    index = pd.date_range("2022-01-01", periods=n, freq="D")
    return pd.DataFrame({
        "open": close * (1 + rng.normal(0, 0.005, n)),
        "high": close * 1.02, "low": close * 0.98, "close": close, "volume": 1.0,
    }, index=index)


FRAMES = {sym: _frame(i) for i, sym in enumerate(SYMBOLS)}


@dataclass
class _Series:
    frame: pd.DataFrame


class _Ohlcv:
    def get_series(self, ref, start, end, timeframe):
        f = FRAMES[ref.symbol]
        return _Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])


def _timeline(start):
    coins = [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=1e9 - i, price=1.0)
             for i, s in enumerate(SYMBOLS)]
    return UniverseTimeline(snapshots=[UniverseSnapshot(on_date=start, provider="test", coins=coins)])


def _run(indicator: IndicatorConfig, compare=()):
    config = RunConfig(top_n=3, start_date=dt.date(2024, 1, 1), end_date=dt.date(2024, 9, 30),
                       indicator=indicator, compare_trends=compare)
    engine = BacktestEngine(registry=object(), ohlcv=_Ohlcv())
    with patch("rotation_lab.backtest.engine.build_universe_timeline",
               return_value=_timeline(config.start_date)):
        return engine.run(config)


class AggregateSignalTests(unittest.TestCase):
    def test_average_of_votes_with_the_tie_held(self):
        df = FRAMES["AAA"]
        cfg = IndicatorConfig(trend="aggregate", aggregate=("ema_cross", "extremes", "rankbreak"))
        got = aggregate_signal(df, cfg).to_numpy()
        ema = np.where(trend_signal(df, IndicatorConfig(trend="ema_cross")).to_numpy() == 1, 1.0, -1.0)
        ext = REGISTRY["extremes"]().signal(df).to_numpy()
        rb = REGISTRY["rankbreak"]().signal(df).to_numpy()
        sigs = np.stack([ema, ext, rb])
        committed = (sigs != 0).sum(axis=0)
        score = sigs.sum(axis=0) / np.maximum(committed, 1)
        expected, cur = [], 0.0
        for s, c in zip(score, committed):
            if c > 0 and s > 0:
                cur = 1.0
            elif c > 0 and s < 0:
                cur = -1.0
            expected.append(cur)
        np.testing.assert_array_equal(got, np.array(expected))
        self.assertTrue(set(np.unique(got)) <= {-1.0, 0.0, 1.0})
        # Three voters: ties are impossible once all vote, splits 2:1 decide.
        self.assertGreater((got == 1).sum(), 0)
        self.assertGreater((got == -1).sum(), 0)

    def test_two_voters_hold_the_verdict_on_a_split(self):
        df = FRAMES["BBB"]
        cfg = IndicatorConfig(trend="aggregate", aggregate=("ema_cross", "extremes"))
        got = aggregate_signal(df, cfg).to_numpy()
        ema = np.where(trend_signal(df, IndicatorConfig(trend="ema_cross")).to_numpy() == 1, 1.0, -1.0)
        ext = REGISTRY["extremes"]().signal(df).to_numpy()
        split = (ext != 0) & (ema != ext)
        self.assertGreater(split.sum(), 0)
        # On every split bar the aggregate equals the bar before (held).
        idx = np.flatnonzero(split)
        idx = idx[idx > 0]
        np.testing.assert_array_equal(got[idx], got[idx - 1])
        # Where both agree after warm-up the aggregate is that vote.
        agree = (ext != 0) & (ema == ext)
        np.testing.assert_array_equal(got[agree], ext[agree])

    def test_a_single_member_is_that_member(self):
        df = FRAMES["CCC"]
        got = trend_signal(df, IndicatorConfig(trend="aggregate", aggregate=("extremes",)))
        pd.testing.assert_series_equal(got, trend_signal(df, IndicatorConfig(trend="extremes")), check_names=False)

    def test_the_ema_member_uses_the_configured_band(self):
        df = FRAMES["AAA"]
        wide = IndicatorConfig(trend="aggregate", aggregate=("ema_cross",), ema_cross=EmaCrossConfig(fast_length=5, slow_length=50))
        got = trend_signal(df, wide)
        pd.testing.assert_series_equal(
            got, trend_signal(df, IndicatorConfig(trend="ema_cross", ema_cross=EmaCrossConfig(fast_length=5, slow_length=50))),
            check_names=False)

    def test_warmup_is_the_largest_member(self):
        self.assertEqual(warmup_bars(IndicatorConfig(trend="aggregate", aggregate=("ema_cross", "extremes", "kalman"))),
                         max(WARMUP_BARS["extremes"], WARMUP_BARS["kalman"]))
        self.assertEqual(warmup_bars(IndicatorConfig(trend="aggregate", aggregate=("ema_cross",))), 0)
        self.assertEqual(warmup_bars(IndicatorConfig(trend="ema_cross")), 0)
        with self.assertRaisesRegex(ValueError, "at least one member"):
            warmup_bars(IndicatorConfig(trend="aggregate"))
        with self.assertRaisesRegex(ValueError, "unknown trend signal 'nope'"):
            warmup_bars(IndicatorConfig(trend="aggregate", aggregate=("nope",)))


class AggregateRunTests(unittest.TestCase):
    def test_the_rotation_runs_on_the_aggregate_and_says_so(self):
        result = _run(IndicatorConfig(trend="aggregate", aggregate=("ema_cross", "extremes")))
        self.assertEqual(result.strategies[0].label, "Aggregate (2)")
        self.assertGreater(len(result.equity_strategy), 100)

    def test_the_aggregate_can_be_compared_against_its_members(self):
        result = _run(IndicatorConfig(trend="aggregate", aggregate=("ema_cross", "extremes")), compare=("ema_cross", "extremes"))
        self.assertEqual([r.label for r in result.strategies], ["Aggregate (2)", "EMA 12/21", "ExtremeFlow"])
        # and a single indicator can compare against the aggregate
        other = _run(IndicatorConfig(trend="extremes", aggregate=("ema_cross", "extremes")), compare=("aggregate",))
        self.assertEqual([r.label for r in other.strategies], ["ExtremeFlow", "Aggregate (2)"])
        pd.testing.assert_series_equal(other.strategies[1].equity_strategy, result.strategies[0].equity_strategy)

    def test_no_members_fails_before_fetching(self):
        with self.assertRaisesRegex(ValueError, "at least one member"):
            _run(IndicatorConfig(trend="aggregate"))


if __name__ == "__main__":
    unittest.main()
