"""A backtest that compares several trend signals on the same candles.

Each compared indicator must reproduce its solo run bit for bit; the
configured indicator stays the result's primary strategy.
"""
import datetime as dt
import unittest
from dataclasses import dataclass
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import IndicatorConfig, RunConfig
from rotation_lab.data.ranking.base import RankedCoin

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
    """The cache's contract: candles for exactly the requested window."""

    def get_series(self, ref, start, end, timeframe):
        f = FRAMES[ref.symbol]
        return _Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])


def _timeline(start: dt.date) -> UniverseTimeline:
    coins = [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=1e9 - i, price=1.0)
             for i, s in enumerate(SYMBOLS)]
    return UniverseTimeline(snapshots=[UniverseSnapshot(on_date=start, provider="test", coins=coins)])


def _run(trend: str, compare: tuple[str, ...] = ()):
    config = RunConfig(
        top_n=3, start_date=dt.date(2024, 1, 1), end_date=dt.date(2024, 9, 30),
        indicator=IndicatorConfig(trend=trend), compare_trends=compare,
    )
    engine = BacktestEngine(registry=object(), ohlcv=_Ohlcv())
    with patch("rotation_lab.backtest.engine.build_universe_timeline",
               return_value=_timeline(config.start_date)):
        return engine.run(config)


class CompareTests(unittest.TestCase):
    def test_missing_ratio_volume_skips_only_that_comparison(self):
        result = _run('ema_cross', ('bvc', 'extremes'))
        self.assertEqual([r.key for r in result.strategies], ['ema_cross', 'extremes'])
        self.assertEqual([r['key'] for r in result.skipped_strategies], ['bvc'])
        self.assertIn('A/B price ratios have no traded volume', result.skipped_strategies[0]['reason'])
        solo = _run('ema_cross')
        pd.testing.assert_series_equal(result.equity_strategy, solo.equity_strategy)
        pd.testing.assert_series_equal(result.strategies[1].equity_strategy, _run('extremes').equity_strategy)

    def test_missing_volume_in_primary_keeps_comparisons_without_relabeling(self):
        result = _run('bvc', ('ema_cross',))
        self.assertEqual(result.config.indicator.trend, 'bvc')
        self.assertEqual([r.key for r in result.strategies], ['ema_cross'])
        self.assertTrue(result.equity_strategy.empty)
        self.assertTrue(result.held_asset.empty)
        pd.testing.assert_series_equal(result.strategies[0].equity_strategy, _run('ema_cross').equity_strategy)

    def test_all_skipped_returns_empty_result_with_reason(self):
        result = _run('bvc')
        self.assertEqual(result.strategies, [])
        self.assertTrue(result.equity_strategy.empty)
        self.assertEqual(result.skipped_strategies[0]['label'], 'BulkVolumeFlow')
        self.assertTrue(result.notes)

    def test_volume_dependent_aggregate_is_not_silently_reduced(self):
        config = RunConfig(top_n=3, start_date=dt.date(2024,1,1), end_date=dt.date(2024,9,30),
            indicator=IndicatorConfig(trend='aggregate',aggregate=('ema_cross','bvc')),
            compare_trends=('ema_cross',))
        with patch('rotation_lab.backtest.engine.build_universe_timeline',return_value=_timeline(config.start_date)):
            result = BacktestEngine(registry=object(),ohlcv=_Ohlcv()).run(config)
        self.assertEqual([r.key for r in result.strategies], ['ema_cross'])
        self.assertEqual(result.skipped_strategies[0]['key'], 'aggregate')
        self.assertEqual(result.config.indicator.aggregate, ('ema_cross','bvc'))

    def test_unrelated_programming_errors_are_not_swallowed(self):
        with patch('rotation_lab.backtest.signals._smithery',side_effect=KeyError('unexpected_column')):
            with self.assertRaisesRegex(KeyError,'unexpected_column'):
                _run('hilbert')

    def test_missing_volume_in_total_filter_does_not_disable_the_filter(self):
        from rotation_lab.backtest.signals import IndicatorDataUnavailable
        config = RunConfig(top_n=3, start_date=dt.date(2024,1,1), end_date=dt.date(2024,9,30),
            indicator=IndicatorConfig(trend='ema_cross'),compare_trends=('extremes',),
            market_filter=True,market_indicator=IndicatorConfig(trend='bvc'))
        with patch('rotation_lab.backtest.engine.build_universe_timeline',return_value=_timeline(config.start_date)), \
             patch('rotation_lab.backtest.engine.market_gate',side_effect=IndicatorDataUnavailable('volume unavailable')):
            result = BacktestEngine(registry=object(),ohlcv=_Ohlcv()).run(config)
        self.assertEqual(result.strategies, [])
        self.assertEqual(len(result.skipped_strategies),2)
        self.assertTrue(all('TOTAL filter' in r['reason'] for r in result.skipped_strategies))

    def test_rpc_reports_skips_even_when_no_strategy_survives(self):
        from rotation_lab.rpc import _method_backtest
        result = _run('bvc')
        with patch('rotation_lab.rpc.BacktestEngine') as engine, \
             patch('rotation_lab.rpc.RankingRegistry'), patch('rotation_lab.rpc.OhlcvFetcher'), \
             patch('rotation_lab.rpc.get_default_cache'):
            engine.return_value.run.return_value = result
            raw = _method_backtest({'config':{'indicator':{'trend':'bvc'}}})
        self.assertEqual(raw['strategies'], [])
        self.assertEqual(raw['skippedStrategies'][0]['key'], 'bvc')
        self.assertEqual(raw['equityStrategy'], [])

    def test_primary_first_and_mirrored_in_the_scalar_fields(self):
        result = _run("ema_cross", ("extremes", "ema_cross"))
        self.assertEqual([r.key for r in result.strategies], ["ema_cross", "extremes"])
        self.assertEqual(result.strategies[0].label, "EMA 12/21")
        self.assertEqual(result.strategies[1].label, "ExtremeFlow")
        pd.testing.assert_series_equal(result.equity_strategy, result.strategies[0].equity_strategy)
        pd.testing.assert_series_equal(result.held_asset, result.strategies[0].held_asset)
        self.assertEqual(result.metrics_strategy, result.strategies[0].metrics_strategy)

    def test_a_compared_indicator_equals_its_solo_run(self):
        # The Smithery indicator pulls warm-up history before start_date; the
        # EMA cross must not see that history, or its early values shift.
        together = _run("extremes", ("ema_cross",))
        solo_ema = _run("ema_cross")
        solo_extremes = _run("extremes")
        pd.testing.assert_series_equal(together.strategies[1].equity_strategy, solo_ema.equity_strategy)
        pd.testing.assert_series_equal(together.strategies[1].held_asset, solo_ema.held_asset)
        pd.testing.assert_series_equal(together.strategies[0].equity_strategy, solo_extremes.equity_strategy)
        self.assertEqual(together.strategies[0].metrics_strategy, solo_extremes.metrics_strategy)
        self.assertEqual(together.strategies[1].metrics_strategy, solo_ema.metrics_strategy)
        # Two signals really ran: the rotations differ somewhere.
        self.assertFalse(together.strategies[0].held_asset.equals(together.strategies[1].held_asset))

    def test_buy_and_hold_and_benchmarks_do_not_depend_on_the_signal(self):
        together = _run("ema_cross", ("extremes",))
        solo = _run("ema_cross")
        for sym in SYMBOLS:
            pd.testing.assert_series_equal(together.buy_and_hold[sym], solo.buy_and_hold[sym])
        self.assertEqual(set(together.benchmarks), set(solo.benchmarks))

    def test_an_unknown_compared_indicator_fails_before_fetching(self):
        with self.assertRaisesRegex(ValueError, "unknown trend signal 'nope'"):
            _run("ema_cross", ("nope",))


if __name__ == "__main__":
    unittest.main()
