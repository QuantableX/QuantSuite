"""Backtest = bot (card 4e80395e).

A decision at a bar's close compares the coins of the ranking the next bar runs on, and only those with a
candle on the bar. Today's live signal reads that same ranking and scores with the engine's own function, so
it picks what the replay picks, whether the clock stands just after the close or the day is long past.
"""
import datetime as dt
import pickle
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab import rpc
from rotation_lab.backtest.engine import USD_SYMBOL, BacktestEngine, pair_scores, top_scored
from rotation_lab.backtest.universe import (UniverseSnapshot, UniverseTimeline, bar_step, comparable,
                                            ranking_known_before_open, run_ends, walk_lists)
from rotation_lab.config import Cadence, RankingResolution
from rotation_lab.data.ranking.base import RankedCoin

FIRST = dt.date(2024, 1, 1)
DAYS = 420
COINS = ["BTC", "AAA", "BBB", "CCC", "DDD", "LATE"]
RAW = {"topN": 4, "cadence": "daily", "rankingResolution": "daily", "rankingSource": "cmc", "marketFilter": False,
       "includeUsd": True, "feeRate": 0.001, "slippageRate": 0.0,
       # A short EMA: its start is forgotten within weeks, so a live window and a replay agree to the bit.
       "indicator": {"trend": "ema_cross", "emaCross": {"fastLength": 3, "slowLength": 5, "src": "close"}}}


def coin(rank, symbol):
    return RankedCoin(rank=rank, cg_id=None, symbol=symbol, name=symbol, market_cap=1e9 / rank, price=1.0)


def world():
    """Daily candles for six coins and a ranking whose members and order keep changing.

    GHOST is ranked on some days and has no candles at all; LATE misses every seventh candle."""
    index = pd.date_range(FIRST, periods=DAYS, freq="D")
    frames = {}
    for seed, symbol in enumerate(COINS):
        rng = np.random.default_rng(100 + seed)
        close = 50 * np.exp(np.cumsum(rng.normal(0.0005 * (seed - 2), 0.03, DAYS)))
        frames[symbol] = pd.DataFrame({"open": close * (1 + rng.normal(0, 0.004, DAYS)), "high": close * 1.03,
                                       "low": close * 0.97, "close": close, "volume": 1.0}, index=index)
    frames["LATE"] = frames["LATE"].drop(index[6::7])

    def ranking(day):
        k = (day - FIRST).days
        names = COINS[(k // 3) % 6:] + COINS[:(k // 3) % 6]
        if k % 11 == 5:
            names = names[:2] + ["GHOST"] + names[2:]
        if k % 5 == 2:
            names[0], names[1] = names[1], names[0]
        return [coin(i + 1, s) for i, s in enumerate(names[:4])]

    return index, frames, ranking


class WalkListTests(unittest.TestCase):
    def setUp(self):
        self.index = pd.date_range("2024-01-01", periods=4, freq="D")
        frame = pd.DataFrame({"open": 1.0, "high": 1.0, "low": 1.0, "close": 1.0}, index=self.index)
        self.frames = {"A": frame, "B": frame, "C": frame.drop(self.index[1])}
        self.snapshots = [UniverseSnapshot(dt.date(2024, 1, 1), "t", [coin(1, "A"), coin(2, "B")]),
                          UniverseSnapshot(dt.date(2024, 1, 2), "t", [coin(1, "B"), coin(2, "C")]),
                          UniverseSnapshot(dt.date(2024, 1, 3), "t", [coin(1, "C"), coin(2, "GHOST"), coin(3, "A")])]

    def test_a_decision_reads_the_ranking_of_the_next_bar(self):
        today, compare = walk_lists(self.snapshots, self.index, self.frames, ahead=True, step=pd.Timedelta(days=1))
        self.assertEqual(today, [["A", "B"], ["B", "C"], ["C", "GHOST", "A"], ["C", "GHOST", "A"]])
        # Day 1 decides among day 2's coins, day 2 among day 3's; past the last snapshot it stays in force.
        # C has no candle on day 2 and GHOST has none at all: neither is compared on such a day.
        self.assertEqual(compare, [["B", "C"], ["A"], ["C", "A"], ["C", "A"]])

    def test_without_looking_ahead_it_is_the_bars_own_ranking(self):
        today, compare = walk_lists(self.snapshots, self.index, self.frames, ahead=False, step=pd.Timedelta(days=1))
        self.assertEqual(compare, [["A", "B"], ["B"], ["C", "A"], ["C", "A"]])
        self.assertEqual(today[1], ["B", "C"])

    def test_only_rankings_complete_before_the_open_are_read_ahead(self):
        self.assertTrue(ranking_known_before_open(Cadence.DAILY, RankingResolution.DAILY))
        self.assertTrue(ranking_known_before_open(Cadence("4h"), RankingResolution.WEEKLY))
        # Daily bars on weekly snapshots read a Sunday's own snapshot on that Sunday.
        self.assertFalse(ranking_known_before_open(Cadence.DAILY, RankingResolution.WEEKLY))
        self.assertEqual(bar_step(Cadence("4h")), pd.Timedelta(hours=4))
        self.assertEqual(bar_step(Cadence.WEEKLY), pd.Timedelta(days=1))

    def test_helpers(self):
        self.assertEqual(run_ends([["A"], ["A"], ["B"], ["B"], ["A"]]), [2, 2, 4, 4, 5])
        present = {"A": np.array([True, False]), "B": np.array([True, True])}
        self.assertEqual(comparable(["B", "X", "A"], present, 0), ["B", "A"])
        self.assertEqual(comparable(["B", "X", "A"], present, 1), ["B"])
        # Every pair says "A wins" on the first bar and gives no verdict on the second: then B (or USD) scores.
        order, points = pair_scores(["A", "B"], lambda a, b: np.array([1, 0], dtype=np.int8), 0, 2, True)
        self.assertEqual(order, ["A", "B", USD_SYMBOL])
        self.assertEqual(points.tolist(), [[2, 1, 0], [0, 1, 2]])
        self.assertEqual(list(top_scored(order, points)), ["A", USD_SYMBOL])
        # One point each: the tie goes to the higher rank, and USD is last.
        won = {("A", None): 1, ("A", "B"): 0, ("B", None): 0}
        order, points = pair_scores(["A", "B"], lambda a, b: np.array([won[a, b]], dtype=np.int8), 0, 1, True)
        self.assertEqual(points.tolist(), [[1, 1, 1]])
        self.assertEqual(list(top_scored(order, points)), ["A"])


class BotPicksWhatTheReplayPicksTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.index, cls.frames, ranking = world()
        cls.ranking = staticmethod(ranking)
        cls.config = rpc._build_config({**RAW, "startDate": FIRST.isoformat(), "endDate": cls.index[-1].date().isoformat()})
        cls.timeline = UniverseTimeline([UniverseSnapshot(ts.date(), "test", cls.ranking(ts.date())) for ts in cls.index])
        cls.equity, cls.held, cls.forced, _ = BacktestEngine(registry=object(), ohlcv=object())._simulate(
            cls.config, cls.timeline, cls.frames, [], None)

    def live(self, as_of, clock):
        """Today's signal for ``as_of`` with the wall clock at ``clock``."""
        frames, ranking = self.frames, self.ranking

        def top_n(day, n, **_):
            return SimpleNamespace(coins=ranking(day), provider="test")

        def series(ref, start, end, timeframe):
            if ref.symbol not in frames:
                raise ValueError("no candles")
            frame = frames[ref.symbol]
            return SimpleNamespace(frame=frame.loc[(frame.index >= pd.Timestamp(start)) &
                                                   (frame.index < pd.Timestamp(end) + pd.Timedelta(days=1))])

        registry, fetcher = Mock(get_top_n=Mock(side_effect=top_n)), Mock(get_series=Mock(side_effect=series))
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=clock), \
             patch.object(rpc, "get_default_cache", return_value=object()), \
             patch.object(rpc, "_notify"), \
             patch.object(rpc, "RankingRegistry", return_value=registry), \
             patch.object(rpc, "OhlcvFetcher", return_value=fetcher):
            return rpc._method_live_snapshot({"asOf": as_of.isoformat(), "config": RAW})

    def test_the_walk_never_holds_a_coin_outside_the_days_ranking(self):
        self.assertEqual(self.forced, [])
        for ts, held in self.held.items():
            if held not in (None, USD_SYMBOL) and not pd.isna(held):
                self.assertIn(held, [c.symbol for c in self.ranking(ts.date())], ts)
        # The fixture does rotate: the holding changes often and the ranking changes under it.
        self.assertGreater(self.held.nunique(), 4)

    def test_the_bot_at_the_close_picks_the_replays_next_holding(self):
        differing = 0
        for k in range(230, DAYS - 1):
            closed, new_day = self.index[k].date(), self.index[k + 1].date()
            # 00:15 UTC on the new day: the candle of `closed` is the newest finished one.
            clock = dt.datetime.combine(new_day, dt.time(0, 15), tzinfo=dt.timezone.utc)
            signal = self.live(new_day, clock)
            self.assertEqual(signal["best"], self.held.iloc[k + 1], f"close of {closed}")
            differing += [c.symbol for c in self.ranking(closed)] != [c.symbol for c in self.ranking(new_day)]
        self.assertGreater(differing, 50)  # many of those closes sit on a ranking change

    def test_a_past_day_gives_the_same_pick_long_after(self):
        clock = dt.datetime(2030, 1, 1, tzinfo=dt.timezone.utc)
        for k in range(230, DAYS - 1, 3):
            closed = self.index[k].date()
            self.assertEqual(self.live(closed, clock)["best"], self.held.iloc[k + 1], f"close of {closed}")

    def test_coins_without_a_candle_are_left_out_in_both(self):
        clock = dt.datetime(2030, 1, 1, tzinfo=dt.timezone.utc)
        today, compare = walk_lists(self.timeline.snapshots, self.index, self.frames, ahead=True, step=pd.Timedelta(days=1))
        seen = set()
        for k in range(230, DAYS - 1):
            ranked = [c.symbol for c in self.ranking(self.index[k + 1].date())]
            missing = [s for s in ranked if s not in compare[k]]
            if not missing or frozenset(missing) in seen:
                continue
            seen.add(frozenset(missing))
            signal = self.live(self.index[k].date(), clock)
            scored = {row["symbol"]: row["score"] for row in signal["universe"]}
            self.assertEqual([s for s in ranked if scored[s] is not None], compare[k])
            for s in missing:  # no score, no won or lost pair, never the pick
                self.assertIsNone(scored[s])
                self.assertNotEqual(signal["best"], s)
                if s in signal["symbols"]:
                    row = signal["symbols"].index(s)
                    self.assertTrue(all(cell is None for cell in signal["scoreMatrix"][row]))
        self.assertEqual(seen, {frozenset({"GHOST"}), frozenset({"LATE"}), frozenset({"GHOST", "LATE"})} & seen)
        self.assertGreaterEqual(len(seen), 2)


class SavedEvaluatorTests(unittest.TestCase):
    def test_a_cube_laid_out_for_the_earlier_walk_is_refused(self):
        from smithery.lab_fast import _STATE, FastSystem
        with tempfile.TemporaryDirectory() as folder:
            state = {name: None for name in _STATE if name != "walk"}
            state["single"] = False
            (Path(folder) / "state.pkl").write_bytes(pickle.dumps(state))
            with self.assertRaisesRegex(ValueError, "earlier walk"):
                FastSystem.load(folder)


if __name__ == "__main__":
    unittest.main()
