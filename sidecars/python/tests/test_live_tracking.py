"""Live windows replay the same confirmed-candle engine as backtests."""
import copy
import datetime as dt
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab import rpc
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.data.ranking.base import RankedCoin

NOW = dt.datetime(2026, 9, 30, 12, 30, tzinfo=dt.timezone.utc)


class LiveTrackingTests(unittest.TestCase):
    def setUp(self):
        for name in ('rotation_lab.data.ohlcv.utc_now', 'rotation_lab.backtest.engine.utc_now'):
            self.enterContext(patch(name, return_value=NOW))
        self.enterContext(patch('rotation_lab.rpc.get_default_cache', return_value=object()))
        self.notify = self.enterContext(patch('rotation_lab.rpc._notify'))

    def test_live_window_ignores_backtest_dates_and_comparisons_without_mutating_config(self):
        raw = {'liveStartDate': '2026-09-01', 'startDate': '2020-01-01', 'endDate': '2021-01-01',
               'compareTrends': ['unavailable'], 'indicator': {'trend': 'ema_cross'}}
        before = copy.deepcopy(raw)
        with patch.object(rpc, '_method_live_snapshot', return_value={'asOf': '2026-09-30'}) as snapshot, \
             patch.object(rpc, '_method_backtest', return_value={'equityStrategy': []}) as replay:
            result = rpc._method_live({'config': raw})
        self.assertEqual(raw, before)
        for call in (snapshot.call_args, replay.call_args):
            cfg = call.args[0]['config']
            self.assertEqual((cfg['startDate'], cfg['endDate']), ('2026-09-01', '2026-09-30'))
            self.assertEqual(cfg['compareTrends'], [])
        self.assertEqual(result['tracking']['startDate'], '2026-09-01')
        self.assertEqual(result['tracking']['endDate'], result['asOf'])

    def test_bad_and_future_live_dates_fail_before_fetching(self):
        for day in ('', None, 123, '2026-02-30', '2026-9-01', '20261001', '2026-10-01'):
            with self.subTest(day=day), patch.object(rpc, '_method_live_snapshot') as snapshot:
                with self.assertRaisesRegex(ValueError, '[Ll]ive start day'):
                    rpc._method_live({'config': {'liveStartDate': day}})
                snapshot.assert_not_called()

    def test_missing_start_defaults_to_evaluation_day_and_historical_asof_still_works(self):
        with patch.object(rpc, '_method_live_snapshot', return_value={}), \
             patch.object(rpc, '_method_backtest', return_value={}) as replay:
            rpc._method_live({'asOf': '2026-09-15', 'config': {'endDate': '2020-01-01'}})
        cfg = replay.call_args.args[0]['config']
        self.assertEqual(cfg['startDate'], '2026-09-15')
        self.assertEqual(cfg['endDate'], '2026-09-15')

    def _fetcher(self, intraday=False):
        index = pd.date_range('2026-09-01', '2026-09-30 12:00', freq='4h' if intraday else 'D')
        close = 100 + np.sin(np.arange(len(index)) / 3) * 10 + np.arange(len(index))
        frame = pd.DataFrame({'open': close, 'high': close + 2, 'low': close - 2,
                              'close': close, 'volume': 50.}, index=index)
        frame.iloc[-1, frame.columns.get_loc('close')] = 1e9  # still open

        def fetch(*args):
            start, end = args[-3:-1]
            return SimpleNamespace(frame=frame.loc[(frame.index >= pd.Timestamp(start)) &
                (frame.index < pd.Timestamp(end + dt.timedelta(days=1)))])
        return Mock(frame=frame, get_pair_series=Mock(side_effect=fetch), get_series=Mock(side_effect=fetch))

    def test_rotation_standings_ignore_changes_to_the_open_candle(self):
        coins = [RankedCoin(1, None, 'BTC', 'Bitcoin', 1e9, 100)]
        registry = Mock()
        registry.get_top_n.return_value = SimpleNamespace(coins=coins, provider='test')
        fetcher = self._fetcher()
        with patch.object(rpc, 'RankingRegistry', return_value=registry), \
             patch.object(rpc, 'OhlcvFetcher', return_value=fetcher):
            request = {'asOf': '2026-09-30', 'config': {'topN': 1}}
            before = rpc._method_live_snapshot(request)
            fetcher.frame.iloc[-1, fetcher.frame.columns.get_loc('close')] = 0.01
            after = rpc._method_live_snapshot(request)
        for field in ('best', 'scores', 'scoreMatrix'):
            self.assertEqual(before[field], after[field])

    def test_single_asset_replay_matches_backtest_for_both_directions_and_timeframes(self):
        for direction in ('long_cash', 'long_short'):
            for timeframe in ('1d', '4h'):
                raw = {'mode': 'single_asset', 'liveStartDate': '2026-09-20',
                       'startDate': '2020-01-01', 'endDate': '2020-02-01',
                       'singleAsset': {'pair': 'BTC/USD', 'timeframe': timeframe, 'direction': direction}}
                with self.subTest(direction=direction, timeframe=timeframe), \
                     patch.object(rpc, 'RankingRegistry'), \
                     patch.object(rpc, 'OhlcvFetcher', return_value=self._fetcher(timeframe == '4h')):
                    live = rpc._method_live({'config': raw})
                    backtest = rpc._method_backtest({'config': {**raw, 'startDate': '2026-09-20', 'endDate': '2026-09-30'}})
                for field in ('equityStrategy', 'heldAsset', 'metricsStrategy', 'buyAndHold'):
                    self.assertEqual(rpc._finite(live['tracking'][field]), rpc._finite(backtest[field]))
                self.assertLess(live['singleAssetSignal']['close'], 1000)
                times = [pd.Timestamp(point['time'], unit='s') if isinstance(point['time'], int)
                         else pd.Timestamp(point['time']) for point in live['tracking']['heldAsset']]
                self.assertEqual(times[0], pd.Timestamp('2026-09-20'))
                self.assertLess(times[-1], pd.Timestamp('2026-09-30 12:00'))

    def test_rotation_replay_matches_backtest_and_keeps_standings(self):
        coins = [RankedCoin(1, None, 'BTC', 'Bitcoin', 1e9, 100)]
        timeline = UniverseTimeline([UniverseSnapshot(dt.date(2026, 9, 20), 'test', coins)])
        registry = Mock()
        registry.get_top_n.return_value = SimpleNamespace(coins=coins, provider='test')
        with patch.object(rpc, 'RankingRegistry', return_value=registry), \
             patch.object(rpc, 'OhlcvFetcher', return_value=self._fetcher()), \
             patch('rotation_lab.backtest.engine.build_universe_timeline', return_value=timeline):
            live = rpc._method_live({'config': {'liveStartDate': '2026-09-20', 'topN': 1}})
            backtest = rpc._method_backtest({'config': {'startDate': '2026-09-20', 'endDate': '2026-09-30', 'topN': 1}})
        self.assertEqual(live['tracking']['equityStrategy'], backtest['equityStrategy'])
        self.assertEqual(live['tracking']['heldAsset'], backtest['heldAsset'])
        self.assertEqual(live['symbols'], ['BTC', 'USD'])
        self.assertEqual(len(live['scoreMatrix']), 2)
        self.assertIn(live['best'], live['symbols'])
        self.assertEqual(live['tracking']['heldAsset'][-1]['time'], '2026-09-29')

    def test_starting_today_retains_signal_with_empty_history_and_null_metrics(self):
        with patch.object(rpc, 'RankingRegistry'), \
             patch.object(rpc, 'OhlcvFetcher', return_value=self._fetcher()):
            live = rpc._method_live({'config': {'mode': 'single_asset', 'liveStartDate': '2026-09-30'}})
        self.assertLess(live['singleAssetSignal']['close'], 1000)
        self.assertEqual(live['tracking']['equityStrategy'], [])
        self.assertTrue(all(v is None for v in rpc._finite(live['tracking']['metricsStrategy']).values()))


if __name__ == '__main__':
    unittest.main()
