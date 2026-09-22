"""Minute routing, candle boundaries, calendar horizons and bounded MC."""
import datetime as dt
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery import data, forge, parallel, registry
from smithery.backtest import periods_per_year
from smithery.robustness import Gauntlet, MC_MAX_BARS, GARCH_BARS
from smithery.indicators import REGISTRY
from rotation_lab.config import Cadence, IndicatorConfig, RunConfig
from rotation_lab.backtest.engine import BacktestEngine, _fetch_start
from rotation_lab.backtest.universe import UniverseTimeline, UniverseSnapshot
from rotation_lab.data.ohlcv import _TF_MS
from rotation_lab.data.ranking.base import RankedCoin


class Exchange:
    timeframes = {'1m': '1m'}
    rateLimit = 0

    def __init__(self, pages, now=270_000):
        self.pages = iter(pages)
        self.now = now
        self.cursors = []

    def load_markets(self):
        pass

    def milliseconds(self):
        return self.now

    def parse_timeframe(self, tf):
        return 60

    def fetch_ohlcv(self, symbol, timeframe, since, limit):
        self.cursors.append(since)
        return next(self.pages, [])


def candles(minutes):
    return [[m * 60_000, 100, 101, 99, 100, 1] for m in minutes]


class MinuteDataTests(unittest.TestCase):
    def test_pagination_preserves_closed_tail_and_excludes_forming_candle(self):
        exchange = Exchange([candles([0, 1]), candles([1, 2, 3, 4])])
        with patch('ccxt.binance', return_value=exchange):
            df = data.fetch_ccxt('binance', 'BTC/USDT', '1m', '1970-01-01')
        self.assertEqual(exchange.cursors, [0, 120_000])
        self.assertEqual(len(df), 4)
        self.assertEqual(df.index[-1], pd.Timestamp('1970-01-01T00:03Z'))
        self.assertAlmostEqual(periods_per_year(df.index), 525_960)

    def test_no_live_bar_does_not_drop_the_final_historical_bar(self):
        exchange = Exchange([candles([0, 1]), []], now=600_000)
        with patch('ccxt.binance', return_value=exchange):
            df = data.fetch_ccxt('binance', 'BTC/USDT', '1m', '1970-01-01')
        self.assertEqual(len(df), 2)

    def test_page_cap_is_an_explicit_failure_not_a_complete_shelf(self):
        exchange = Exchange([candles([0, 1])], now=600_000)
        with patch('ccxt.binance', return_value=exchange), self.assertRaisesRegex(RuntimeError, 'incomplete history'):
            data.fetch_ccxt('binance', 'BTC/USDT', '1m', '1970-01-01', max_pages=1)

    def test_refresh_one_track_preserves_other_tracks_and_resumes_at_a_minute(self):
        index = pd.date_range('2026-01-01T12:34Z', periods=2, freq='min')
        old = pd.DataFrame(dict(open=100, high=101, low=99, close=100, volume=1), index=index)
        new = old.copy()
        new.index += pd.Timedelta(minutes=1)
        with tempfile.TemporaryDirectory() as folder, patch.object(data, 'PRICE_DIR', Path(folder)):
            data._save('BINANCE_BTCUSDT_1m', old)
            with patch.object(data, 'fetch_ccxt', return_value=new) as fetch:
                result = data.refresh(verbose=False, timeframe='1m')
            self.assertTrue(all(key.endswith('_1m') for key in result))
            self.assertEqual(fetch.call_count, 8)
            self.assertEqual(fetch.call_args_list[0].args[3], '2026-01-01T12:35:00+00:00')
            self.assertEqual(len(data.load('BINANCE_BTCUSDT_1m')), 3)


class MinuteEvidenceTests(unittest.TestCase):
    def test_old_artifact_stays_visible_as_historical_on_its_own_track(self):
        from smithery import evidence
        with tempfile.TemporaryDirectory() as folder, patch.object(data, 'OUTPUT_DIR', Path(folder)):
            evidence.write_json(Path(folder) / '2026-09-08 old.json', {
                'kind': 'gauntlet', 'name': REGISTRY['robust'].name, 'timeframe': '1h',
                'version': 'previous-evaluator', 'code_sha256': 'previous-code',
                'params': REGISTRY['robust']().params, 'fast': False,
                'score': 79, 'grade': 'A', 'perm_p': .02, 'certified': True,
            })
            registry._current_runs.cache_clear()
            try:
                verdict = registry.verdict_for('robust', '1h')
                self.assertEqual(verdict['score'], 79)
                self.assertEqual(verdict['source'], 'historical')
                self.assertIsNone(registry.verdict_for('robust', '1m'))
            finally:
                registry._current_runs.cache_clear()

    def test_single_minute_comparison_report_keeps_its_track(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / '2026-09-09 Smithery Comparison (test).md'
            path.write_text('Comparison', encoding='utf-8')
            path.with_suffix('.manifest.json').write_text(json.dumps({'timeframes': ['1m']}), encoding='utf-8')
            self.assertEqual(forge._report_meta(path)['timeframe'], '1m')

    def test_minute_never_inherits_hourly_certification(self):
        with patch.object(registry, '_current_runs', return_value={}):
            self.assertIsNone(registry.verdict_for('rankbreak', '1m'))
            verdict = registry.overall_verdict('rankbreak')
            self.assertEqual(verdict['tracks'], 4)
            self.assertEqual(verdict['tracks_run'], 3)
            self.assertFalse(verdict['certified'])

    def test_full_mc_retains_calendar_horizons(self):
        self.assertEqual(MC_MAX_BARS['1m'], 60 * MC_MAX_BARS['1h'])
        self.assertEqual(GARCH_BARS['1m'], 60 * GARCH_BARS['1h'])

    def test_minutes_of_history_cannot_pass_the_temporal_axis(self):
        index = pd.date_range('2026-01-01', periods=100, freq='min', tz='UTC')
        close = np.exp(np.arange(100) / 10000)
        df = pd.DataFrame(dict(open=close, high=close * 1.01, low=close * .99, close=close), index=index)
        gauntlet = Gauntlet(REGISTRY['extremes'](), timeframe='1m')
        with patch('smithery.robustness.load', return_value=df):
            self.assertTrue(np.isnan(gauntlet.axis_temporal('test')))
        self.assertIn('calendar year', gauntlet.sections[-1])

    def test_mc_generator_is_consumed_in_bounded_batches_in_original_order(self):
        generated, evaluated = [], []
        def frames():
            for i in range(9):
                self.assertLess(len(generated) - len(evaluated), 2)
                generated.append(i)
                yield pd.DataFrame({'close': [i]})
        def pmap(fn, tasks):
            values = [int(task[1]['close'].iloc[0]) for task in tasks]
            evaluated.extend(values)
            return [(value, value + 1) for value in values]
        with patch.object(parallel, 'pmap', side_effect=pmap), patch.object(parallel, 'WORKERS', 8):
            result = parallel.simulated_results(None, frames(), 10, 250_000)
        self.assertEqual(result, [(i, i + 1) for i in range(9)])

    def test_cli_accepts_minutes_for_all_four_jobs(self):
        for command, handler in [('refresh', 'run_refresh'), ('gauntlet', 'run_gauntlet'),
                                 ('walkforward', 'run_walkforward'), ('compare', 'run_comparison')]:
            argv = [command, '--timeframe', '1m']
            if command != 'refresh':
                argv += ['--indicator', 'robust', 'extremes']
            with patch.object(forge, handler, return_value=0) as run:
                self.assertEqual(forge._main(argv), 0)
                self.assertEqual(run.call_args.args[0].timeframe, '1m')


class MinuteSystemTests(unittest.TestCase):
    def test_live_evaluation_requests_minute_candles_with_minute_warmup(self):
        from rotation_lab import rpc
        coin = RankedCoin(rank=1, cg_id=None, symbol='BTC', name='BTC', market_cap=1e9, price=100)
        resolved = SimpleNamespace(coins=[coin], provider='test')
        frame = pd.DataFrame({'close': np.linspace(100, 110, 200)},
                             index=pd.date_range('2026-01-01', periods=200, freq='min'))
        with patch.object(rpc, 'get_default_cache', return_value=object()), \
                patch.object(rpc, 'RankingRegistry') as ranking, \
                patch.object(rpc, 'OhlcvFetcher') as fetcher, \
                patch.object(rpc, '_notify'):
            ranking.return_value.get_top_n.return_value = resolved
            fetcher.return_value.get_series.return_value = SimpleNamespace(frame=frame)
            result = rpc._method_live({'config': {'cadence': '1m', 'topN': 1}, 'asOf': '2026-01-03'})
            args = fetcher.return_value.get_series.call_args.args
            self.assertEqual(args[1:], (dt.date(2025, 12, 31), dt.date(2026, 1, 3), '1m'))
            self.assertIn('BTC', result['symbols'])
            self.assertIsNotNone(result['best'])

    def test_resolution_warmup_and_annualisation_are_minutes(self):
        cadence = Cadence('1m')
        self.assertEqual(cadence.pandas_freq, '1min')
        self.assertEqual(cadence.ccxt_timeframe, '1m')
        self.assertEqual(_TF_MS[cadence.value], 60_000)
        self.assertEqual(cadence.bars_per_year, 525_600)
        cfg = RunConfig(cadence=cadence, start_date=dt.date(2026, 1, 3), indicator=IndicatorConfig(trend='scale'))
        self.assertEqual(_fetch_start(cfg), dt.date(2026, 1, 1))

    def test_system_preserves_every_minute_and_requests_minute_candles(self):
        index = pd.date_range('2026-01-01', periods=120, freq='min')
        close = np.exp(np.arange(120) / 1000) * 100
        frame = pd.DataFrame(dict(open=close, high=close * 1.01, low=close * .99, close=close), index=index)
        coin = RankedCoin(rank=1, cg_id=None, symbol='BTC', name='BTC', market_cap=1e9, price=100)
        timeline = UniverseTimeline(snapshots=[UniverseSnapshot(on_date=dt.date(2026, 1, 1), provider='test', coins=[coin])])
        requests = []
        def series(ref, start, end, timeframe):
            requests.append(timeframe)
            return SimpleNamespace(frame=frame)
        engine = BacktestEngine(registry=object(), ohlcv=SimpleNamespace(get_series=series))
        cfg = RunConfig(top_n=1, cadence=Cadence.MINUTE_1, start_date=dt.date(2026, 1, 1), end_date=dt.date(2026, 1, 1))
        with patch('rotation_lab.backtest.engine.build_universe_timeline', return_value=timeline):
            result = engine.run(cfg)
        self.assertTrue(requests)
        self.assertEqual(set(requests), {'1m'})
        self.assertEqual(len(result.equity_strategy), 120)
        pd.testing.assert_index_equal(result.equity_strategy.index, index)


if __name__ == '__main__':
    unittest.main()
