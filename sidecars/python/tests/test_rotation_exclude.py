"""A restricted universe (`excludeSymbols`) is the same in backtest and live.

Manual execution's "restrict the universe to tradable coins"
(PLAN-AUTO-EXECUTION §2.3, card Automation 3b): a coin the venue cannot
trade is dropped from every day's ranking before scoring — in the backtest,
the live signal and its tracking replay alike — so the backtest shows what
live can actually hold.
"""
import datetime as dt
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab import rpc
from rotation_lab.data.ranking.base import RankedCoin
from rotation_lab.data.ranking.registry import RankingRegistry

NOW = dt.datetime(2026, 9, 30, 12, 30, tzinfo=dt.timezone.utc)
COINS = [
    RankedCoin(1, 'bitcoin', 'BTC', 'Bitcoin', 3e12, 100),
    RankedCoin(2, 'ethereum', 'ETH', 'Ethereum', 1e12, 100),
    RankedCoin(3, 'solana', 'SOL', 'Solana', 1e11, 100),
]


class FakeProvider:
    """A ranking that is the same every day."""

    def __init__(self, name):
        self.name = name
        self.calls = 0

    def get_top_n(self, on_date, n, **_):
        self.calls += 1
        return list(COINS[:n])


def registry():
    return RankingRegistry(cache=object(), cmc=FakeProvider('cmc'), local=FakeProvider('local'))


def fetcher():
    """ETH climbs hardest, SOL a little, BTC drifts: an unrestricted system holds ETH."""
    index = pd.date_range('2026-07-01', '2026-09-30 12:00', freq='D')
    t = np.arange(len(index), dtype=float)
    slopes = {'BTC': 0.0, 'ETH': 3.0, 'SOL': 1.0}

    def frame_for(symbol):
        close = 100 + slopes[symbol] * t + np.sin(t / 3) * 2
        frame = pd.DataFrame({'open': close, 'high': close + 1, 'low': close - 1, 'close': close, 'volume': 50.},
                             index=index)
        frame.iloc[-1, frame.columns.get_loc('close')] = 1e9  # the open candle
        return frame

    def series(coin, start, end, _tf):
        frame = frame_for(coin.symbol)
        return SimpleNamespace(frame=frame.loc[(frame.index >= pd.Timestamp(start))
                                               & (frame.index < pd.Timestamp(end + dt.timedelta(days=1)))])

    return Mock(get_series=Mock(side_effect=series), get_pair_series=Mock(side_effect=series))


class RestrictedUniverseTests(unittest.TestCase):
    def setUp(self):
        for name in ('rotation_lab.data.ohlcv.utc_now', 'rotation_lab.backtest.engine.utc_now'):
            self.enterContext(patch(name, return_value=NOW))
        self.enterContext(patch('rotation_lab.rpc.get_default_cache', return_value=object()))
        self.enterContext(patch('rotation_lab.rpc._notify'))

    def test_the_ranking_drops_excluded_symbols_without_refilling(self):
        reg = registry()
        kept = reg.get_top_n(dt.date(2026, 9, 1), 3, exclude_symbols=['eth'])
        self.assertEqual([c.symbol for c in kept.coins], ['BTC', 'SOL'])
        # An excluded head and an excluded symbol combine.
        kept = reg.get_top_n(dt.date(2026, 9, 1), 3, exclude_top_n=1, exclude_symbols=('SOL',))
        self.assertEqual([c.symbol for c in kept.coins], ['ETH'])
        self.assertEqual([c.symbol for c in reg.get_top_n(dt.date(2026, 9, 1), 3).coins], ['BTC', 'ETH', 'SOL'])

    def test_the_config_reads_the_list(self):
        cfg = rpc._build_config({'excludeSymbols': ['eth', ' SOL ', '', 'eth']})
        self.assertEqual(cfg.exclude_symbols, ('ETH', 'SOL'))
        self.assertEqual(rpc._build_config({}).exclude_symbols, ())

    def run_both(self, exclude):
        config = {'liveStartDate': '2026-09-10', 'topN': 3, 'includeUsd': True}
        if exclude:
            config['excludeSymbols'] = exclude
        with patch.object(rpc, 'RankingRegistry', return_value=registry()), \
             patch.object(rpc, 'OhlcvFetcher', return_value=fetcher()):
            live = rpc._method_live({'config': config})
            backtest = rpc._method_backtest({'config': {**config, 'startDate': '2026-09-10', 'endDate': '2026-09-30'}})
        return live, backtest

    def test_backtest_and_live_hold_the_same_coins_under_the_restriction(self):
        free_live, _ = self.run_both(None)
        self.assertEqual(free_live['best'], 'ETH', 'the fixture makes ETH the unrestricted pick')

        live, backtest = self.run_both(['ETH'])
        self.assertNotIn('ETH', live['symbols'])
        self.assertNotEqual(live['best'], 'ETH')
        held = [p['value'] if isinstance(p, dict) and 'value' in p else p for p in live['tracking']['heldAsset']]
        self.assertTrue(held, 'the replay held something')
        self.assertFalse(any('ETH' in str(h) for h in held), held)
        # The live replay is the backtest of the same window, restriction included.
        self.assertEqual(live['tracking']['heldAsset'], backtest['heldAsset'])
        self.assertEqual(live['tracking']['equityStrategy'], backtest['equityStrategy'])
        # Today's pick is what the backtest would hold next.
        self.assertIn(live['best'], ('BTC', 'SOL', 'USD'))


if __name__ == '__main__':
    unittest.main()
