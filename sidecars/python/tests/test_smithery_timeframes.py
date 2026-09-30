"""The certification tracks are 1d / 4h / 1h — no minute track, no minute
cadence (user, 2026-09-23). Candle boundaries, shelf refresh per track,
evidence per track and bounded Monte Carlo batches on the hourly track."""
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
from smithery.robustness import GARCH_BARS, MC_MAX_BARS, TIMEFRAMES, Gauntlet
from smithery.indicators import REGISTRY
from rotation_lab.config import Cadence, IndicatorConfig, RunConfig
from rotation_lab.backtest.engine import BacktestEngine, _fetch_start
from rotation_lab.backtest.universe import UniverseTimeline, UniverseSnapshot
from rotation_lab.data.ohlcv import _TF_MS
from rotation_lab.data.ranking.base import RankedCoin

HOUR_MS = 3_600_000


class Exchange:
    timeframes = {'1h': '1h'}
    rateLimit = 0

    def __init__(self, pages, now=int(4.5 * HOUR_MS)):
        self.pages = iter(pages)
        self.now = now
        self.cursors = []

    def load_markets(self):
        pass

    def milliseconds(self):
        return self.now

    def parse_timeframe(self, tf):
        return 3600

    def fetch_ohlcv(self, symbol, timeframe, since, limit):
        self.cursors.append(since)
        return next(self.pages, [])


def candles(hours):
    return [[h * HOUR_MS, 100, 101, 99, 100, 1] for h in hours]


class TrackTests(unittest.TestCase):
    def test_the_tracks_are_daily_four_hour_and_hourly_only(self):
        self.assertEqual(TIMEFRAMES, ('1d', '4h', '1h'))
        self.assertEqual(set(MC_MAX_BARS), set(TIMEFRAMES))
        self.assertEqual(set(GARCH_BARS), set(TIMEFRAMES))
        self.assertEqual({tf for _, _, tf, _ in data.SHELF_CRYPTO}, set(TIMEFRAMES))

    def test_monte_carlo_windows_keep_the_same_calendar_horizon(self):
        self.assertEqual(MC_MAX_BARS['4h'], 6 * MC_MAX_BARS['1d'])
        self.assertEqual(MC_MAX_BARS['1h'], 4 * MC_MAX_BARS['4h'])
        self.assertEqual(GARCH_BARS['4h'], 6 * GARCH_BARS['1d'])
        self.assertEqual(GARCH_BARS['1h'], 4 * GARCH_BARS['4h'])

    def test_cli_accepts_every_track_for_all_four_jobs(self):
        for command, handler in [('refresh', 'run_refresh'), ('gauntlet', 'run_gauntlet'),
                                 ('walkforward', 'run_walkforward'), ('compare', 'run_comparison')]:
            for track in TIMEFRAMES:
                argv = [command, '--timeframe', track]
                if command != 'refresh':
                    argv += ['--indicator', 'robust', 'extremes']
                with patch.object(forge, handler, return_value=0) as run:
                    self.assertEqual(forge._main(argv), 0)
                    self.assertEqual(run.call_args.args[0].timeframe, track)

    def test_cli_rejects_the_minute_track(self):
        for command in ('refresh', 'gauntlet', 'walkforward', 'compare'):
            argv = [command, '--timeframe', '1m']
            if command != 'refresh':
                argv += ['--indicator', 'robust']
            with self.assertRaises(SystemExit):
                forge._main(argv)

    def test_minute_is_not_a_certification_track(self):
        with self.assertRaises(KeyError):
            registry.certification_table('1m')


class HourlyDataTests(unittest.TestCase):
    def test_pagination_preserves_closed_tail_and_excludes_forming_candle(self):
        exchange = Exchange([candles([0, 1]), candles([1, 2, 3, 4])])
        with patch('ccxt.binance', return_value=exchange):
            df = data.fetch_ccxt('binance', 'BTC/USDT', '1h', '1970-01-01')
        self.assertEqual(exchange.cursors, [0, 2 * HOUR_MS])
        self.assertEqual(len(df), 4)
        self.assertEqual(df.index[-1], pd.Timestamp('1970-01-01T03:00Z'))
        self.assertAlmostEqual(periods_per_year(df.index), 8766)

    def test_no_live_bar_does_not_drop_the_final_historical_bar(self):
        exchange = Exchange([candles([0, 1]), []], now=10 * HOUR_MS)
        with patch('ccxt.binance', return_value=exchange):
            df = data.fetch_ccxt('binance', 'BTC/USDT', '1h', '1970-01-01')
        self.assertEqual(len(df), 2)

    def test_page_cap_is_an_explicit_failure_not_a_complete_shelf(self):
        exchange = Exchange([candles([0, 1])], now=10 * HOUR_MS)
        with patch('ccxt.binance', return_value=exchange), self.assertRaisesRegex(RuntimeError, 'incomplete history'):
            data.fetch_ccxt('binance', 'BTC/USDT', '1h', '1970-01-01', max_pages=1)

    def test_refresh_one_track_preserves_other_tracks_and_resumes_at_the_last_bar(self):
        index = pd.date_range('2026-01-01T12:00Z', periods=2, freq='h')
        old = pd.DataFrame(dict(open=100, high=101, low=99, close=100, volume=1), index=index)
        new = old.copy()
        new.index += pd.Timedelta(hours=1)
        with tempfile.TemporaryDirectory() as folder, patch.object(data, 'PRICE_DIR', Path(folder)):
            data._save('BINANCE_BTCUSDT_1h', old)
            data._save('BINANCE_BTCUSDT_4h', old)
            with patch.object(data, 'fetch_ccxt', return_value=new) as fetch:
                result = data.refresh(verbose=False, timeframe='1h')
            self.assertTrue(all(key.endswith('_1h') for key in result))
            self.assertEqual(fetch.call_count, 8)
            self.assertEqual(fetch.call_args_list[0].args[3], '2026-01-01T13:00:00+00:00')
            self.assertEqual(len(data.load('BINANCE_BTCUSDT_1h')), 3)
            self.assertEqual(len(data.load('BINANCE_BTCUSDT_4h')), 2)


class EvidenceTests(unittest.TestCase):
    def test_old_artifact_stays_visible_as_historical_on_its_own_track(self):
        from smithery import evidence, variants
        # Only the artifact: no version-file evidence (it outranks history —
        # every forged Standard carries some since the timeframe round).
        with tempfile.TemporaryDirectory() as folder, patch.object(data, 'OUTPUT_DIR', Path(folder)),                 patch.object(variants, 'version_evidence', lambda timeframe: {}):
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
            finally:
                registry._current_runs.cache_clear()

    def test_single_track_comparison_report_keeps_its_track(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / '2026-09-09 Smithery Comparison (test).md'
            path.write_text('Comparison', encoding='utf-8')
            path.with_suffix('.manifest.json').write_text(json.dumps({'timeframes': ['1h']}), encoding='utf-8')
            self.assertEqual(forge._report_meta(path)['timeframe'], '1h')

    def test_three_certified_tracks_certify_the_indicator(self):
        passed = {'score': 80, 'grade': 'A', 'perm_p': .02, 'date': '2026-09-23', 'report': 'r'}
        with patch.object(registry, 'certification_table', return_value={'x': passed}):
            verdict = registry.overall_verdict('x')
        self.assertEqual(verdict['tracks'], 3)
        self.assertEqual(verdict['tracks_run'], 3)
        self.assertTrue(verdict['certified'])

    def test_a_missing_track_is_not_certified(self):
        passed = {'score': 80, 'grade': 'A', 'perm_p': .02, 'date': '2026-09-23', 'report': 'r'}
        tables = {'1d': {'x': passed}, '4h': {'x': passed}, '1h': {}}
        with patch.object(registry, 'certification_table', side_effect=lambda tf: tables[tf]):
            verdict = registry.overall_verdict('x')
        self.assertEqual(verdict['tracks_run'], 2)
        self.assertFalse(verdict['certified'])

    def test_hours_of_history_cannot_pass_the_temporal_axis(self):
        index = pd.date_range('2026-01-01', periods=100, freq='h', tz='UTC')
        close = np.exp(np.arange(100) / 10000)
        df = pd.DataFrame(dict(open=close, high=close * 1.01, low=close * .99, close=close), index=index)
        gauntlet = Gauntlet(REGISTRY['extremes'](), timeframe='1h')
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


class SystemCadenceTests(unittest.TestCase):
    def test_minute_cadence_is_rejected_with_a_clear_message(self):
        with self.assertRaisesRegex(ValueError, "unsupported cadence '1m'"):
            Cadence('1m')
        self.assertEqual([c.value for c in Cadence], ['1h', '4h', '12h', 'daily', 'weekly', 'monthly'])

    def test_live_evaluation_requests_hourly_candles_with_hourly_warmup(self):
        from rotation_lab import rpc
        coin = RankedCoin(rank=1, cg_id=None, symbol='BTC', name='BTC', market_cap=1e9, price=100)
        resolved = SimpleNamespace(coins=[coin], provider='test')
        frame = pd.DataFrame({'close': np.linspace(100, 110, 400)},
                             index=pd.date_range('2025-12-15', periods=400, freq='h'))
        with patch.object(rpc, 'get_default_cache', return_value=object()), \
                patch.object(rpc, 'RankingRegistry') as ranking, \
                patch.object(rpc, 'OhlcvFetcher') as fetcher, \
                patch.object(rpc, '_notify'):
            ranking.return_value.get_top_n.return_value = resolved
            fetcher.return_value.get_series.return_value = SimpleNamespace(frame=frame)
            result = rpc._method_live_snapshot({'config': {'cadence': '1h', 'topN': 1}, 'asOf': '2026-01-03'})
            args = fetcher.return_value.get_series.call_args.args
            # EMA 12/21 cross: 21 × 8 bars of history = 168 hours → 7 + 2 days
            self.assertEqual(args[1:], (dt.date(2025, 12, 25), dt.date(2026, 1, 3), '1h'))
            self.assertIn('BTC', result['symbols'])
            self.assertIsNotNone(result['best'])

    def test_resolution_warmup_and_annualisation_are_hours(self):
        cadence = Cadence('1h')
        self.assertEqual(cadence.pandas_freq, '1h')
        self.assertEqual(cadence.ccxt_timeframe, '1h')
        self.assertEqual(_TF_MS[cadence.value], HOUR_MS)
        self.assertEqual(cadence.bars_per_year, 8760)
        cfg = RunConfig(cadence=cadence, start_date=dt.date(2026, 1, 30), indicator=IndicatorConfig(trend='scale'))
        with patch('rotation_lab.backtest.engine.warmup_bars', return_value=480):
            self.assertEqual(_fetch_start(cfg), dt.date(2026, 1, 8))   # 480 // 24 + 2 days

    def test_system_preserves_every_bar_and_requests_hourly_candles(self):
        index = pd.date_range('2026-01-01', periods=120, freq='h')
        close = np.exp(np.arange(120) / 1000) * 100
        frame = pd.DataFrame(dict(open=close, high=close * 1.01, low=close * .99, close=close), index=index)
        coin = RankedCoin(rank=1, cg_id=None, symbol='BTC', name='BTC', market_cap=1e9, price=100)
        timeline = UniverseTimeline(snapshots=[UniverseSnapshot(on_date=dt.date(2026, 1, 1), provider='test', coins=[coin])])
        requests = []
        def series(ref, start, end, timeframe):
            requests.append(timeframe)
            return SimpleNamespace(frame=frame)
        engine = BacktestEngine(registry=object(), ohlcv=SimpleNamespace(get_series=series))
        cfg = RunConfig(top_n=1, cadence=Cadence.HOUR_1, start_date=dt.date(2026, 1, 1), end_date=dt.date(2026, 1, 5))
        with patch('rotation_lab.backtest.engine.build_universe_timeline', return_value=timeline):
            result = engine.run(cfg)
        self.assertTrue(requests)
        self.assertEqual(set(requests), {'1h'})
        self.assertEqual(len(result.equity_strategy), 120)
        pd.testing.assert_index_equal(result.equity_strategy.index, index)


if __name__ == '__main__':
    unittest.main()
