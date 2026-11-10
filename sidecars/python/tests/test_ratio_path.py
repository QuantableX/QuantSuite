"""Real coin-vs-coin candles (card 33f7d61c).

The candle of an A/B ratio takes its high and low from the ratio's hourly path wherever both coins carry one
(smithery.ratio), in the engine, in today's live signal and in certification. The hours of a day come from the
venue and pair that day's own candle came from, are fetched once and only extended at both ends.
"""
import datetime as dt
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab import rpc
from rotation_lab.backtest.engine import USD_SYMBOL, BacktestEngine, path_note, with_path
from rotation_lab.backtest.signals import ratio_frame
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.data import ohlcv
from rotation_lab.data.cache import Cache, OhlcvBar, OhlcvRow
from rotation_lab.data.ohlcv import CoinRef, OhlcvFetcher, market_key, market_of, paths_from_bars, saved_bars
from rotation_lab.data.ranking.base import RankedCoin
from smithery import ratio
from smithery.ratio import PATH_OF, path_columns, path_names, paths_from_fine, ratio_candles, real_bars, without_path

HOURS = path_names(24)


def candle(rows, index):
    return pd.DataFrame(rows, columns=["open", "high", "low", "close"], index=index, dtype=float)


class RuleTests(unittest.TestCase):
    def setUp(self):
        self.days = pd.date_range("2024-01-01", periods=2, freq="D")
        # Day 1: A spikes to 110 in hour 5, B to 1.1 in hour 20; both open and close where they started.
        a_path = np.full((2, 24), 100.0)
        b_path = np.full((2, 24), 1.0)
        a_path[0, 4], b_path[0, 19] = 110.0, 1.1
        self.a = candle([[100, 110, 100, 100], [100, 100, 100, 100]], self.days).join(pd.DataFrame(a_path, self.days, HOURS))
        self.b = candle([[1, 1.1, 1, 1], [1, 1, 1, 1]], self.days).join(pd.DataFrame(b_path, self.days, HOURS))

    def test_high_and_low_follow_the_ratio_hour_by_hour(self):
        r = ratio_candles(self.a, self.b)
        self.assertEqual(list(r.columns), ["open", "close", "high", "low"])
        # The ratio was 110 when A peaked and 100/1.1 when B peaked; from the daily candles alone it looks flat.
        np.testing.assert_allclose(r.loc[self.days[0], ["open", "close", "high", "low"]], [100, 100, 110, 100 / 1.1])
        np.testing.assert_allclose(r.loc[self.days[1], ["open", "close", "high", "low"]], [100, 100, 100, 100])
        flat = ratio_candles(without_path(self.a), without_path(self.b))
        np.testing.assert_allclose(flat.loc[self.days[0], ["high", "low"]], [100, 100])
        self.assertEqual(real_bars(self.a, self.b).tolist(), [True, True])

    def test_a_bar_without_a_whole_path_takes_the_stand_in(self):
        a = self.a.copy()
        a.loc[self.days[0], "path_07"] = np.nan
        r = ratio_candles(a, self.b)
        np.testing.assert_allclose(r.loc[self.days[0], ["high", "low"]], [100, 100])  # high(A)/high(B), holding open and close
        self.assertEqual(real_bars(a, self.b).tolist(), [False, True])
        # One coin without any path: the stand-in on every bar.
        np.testing.assert_allclose(ratio_candles(without_path(self.a), self.b)["high"], [100, 100])

    def test_the_candle_always_holds_its_open_and_close(self):
        a = self.a.copy()
        a.loc[self.days[1], ["open", "close"]] = [103.0, 97.0]  # the day's own candle says more than the hours do
        r = ratio_candles(a, self.b)
        np.testing.assert_allclose(r.loc[self.days[1], ["high", "low"]], [103, 97])

    def test_an_indicator_never_sees_the_path(self):
        self.assertEqual(path_columns(self.a), HOURS)
        self.assertEqual(list(ratio_frame(self.a, None).columns), ["open", "high", "low", "close"])
        self.assertEqual(list(ratio_frame(self.a, self.b).columns), ["open", "close", "high", "low"])
        plain = self.b[["open", "high", "low", "close"]]
        self.assertIs(without_path(plain), plain)  # nothing to take away: the same frame

    def test_paths_from_finer_bars(self):
        bars = pd.date_range("2024-01-01", periods=3, freq="4h")
        fine = pd.Series(np.arange(1.0, 13.0), index=pd.date_range("2024-01-01", periods=12, freq="h")).drop(pd.Timestamp("2024-01-01 06:00"))
        paths = paths_from_fine(bars, fine, 4)
        self.assertEqual(list(paths.columns), path_names(4))
        np.testing.assert_allclose(paths.iloc[0], [1, 2, 3, 4])
        self.assertTrue(paths.iloc[1].isna().all())  # one hour missing: no path for the whole bar
        np.testing.assert_allclose(paths.iloc[2], [9, 10, 11, 12])


def hour_bars(market, day, closes):
    return [OhlcvBar(coin_key=market, tf="1h", ts=dt.datetime.combine(day, dt.time(h)), open=c, high=c, low=c, close=c,
                     volume=1.0, source="test", approx=False) for h, c in closes.items()]


def fine_bars(market, tf, start, closes):
    """Candles of timeframe ``tf`` from ``start`` on, one per value (None = the venue has no candle there)."""
    step = dt.timedelta(minutes=5) if tf == "5m" else dt.timedelta(hours=1)
    return [OhlcvBar(coin_key=market, tf=tf, ts=start + k * step, open=c, high=c, low=c, close=c, volume=1.0, source="test", approx=False)
            for k, c in enumerate(closes) if c is not None]


def day_row(day, source, close=100.0):
    return OhlcvRow(coin_key="sym:AAA", day=day, open=close, high=close, low=close, close=close, volume=1.0, source=source, approx=False)


class SavedPathTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.cache = Cache(Path(self.folder.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())  # before the folder goes: Windows keeps an open file
        self.d = [dt.date(2024, 1, k) for k in range(1, 5)]

    def test_a_days_hours_come_from_the_venue_of_its_own_candle(self):
        binance, kucoin = market_key("binance", "AAA/USDT"), market_key("kucoin", "AAA/USDT")
        self.cache.upsert_ohlcv([day_row(self.d[0], "ccxt:binance:AAA/USDT"), day_row(self.d[1], "ccxt:binance:AAA/USDT"),
                                 day_row(self.d[2], "ccxt:kucoin:AAA/USDT"), day_row(self.d[3], "coingecko")])
        self.cache.upsert_ohlcv_intraday(
            hour_bars(binance, self.d[0], {h: 100.0 + h for h in range(24)})
            + hour_bars(binance, self.d[1], {h: 200.0 + h for h in range(23)})   # the last hour is missing
            + hour_bars(binance, self.d[2], {h: 999.0 for h in range(24)})       # not this day's venue
            + hour_bars(kucoin, self.d[2], {h: 300.0 + h for h in range(24)})
            + hour_bars(binance, self.d[3], {h: 400.0 for h in range(24)}))      # the day's candle is not an exchange's
        paths = paths_from_bars(self.cache, saved_bars(self.cache, "sym:AAA", self.d[0], self.d[3]))
        self.assertEqual([t.date() for t in paths.index], [self.d[0], self.d[2]])
        np.testing.assert_allclose(paths.iloc[0], [100.0 + h for h in range(24)])
        np.testing.assert_allclose(paths.iloc[1], [300.0 + h for h in range(24)])
        self.assertEqual(market_of("ccxt:bybit:HYPE/USDT:USDT"), ("bybit", "HYPE/USDT:USDT"))
        self.assertIsNone(market_of("coingecko"))
        self.assertTrue(paths_from_bars(self.cache, []).empty)

    def test_intraday_bars_take_their_path_from_five_minute_candles(self):
        # Three 4h bars of one venue; each holds 48 five-minute candles. The second lacks one.
        market, day = market_key("okx", "AAA/USDT"), dt.datetime(2024, 1, 1)
        bars = [OhlcvBar("sym:AAA", "4h", day + dt.timedelta(hours=4 * k), 1, 1, 1, 1, 1.0, "ccxt:okx:AAA/USDT", False) for k in range(3)]
        closes = [float(k) for k in range(1, 145)]
        closes[60] = None
        self.cache.upsert_ohlcv_intraday(bars + fine_bars(market, "5m", day, closes))
        self.assertEqual(PATH_OF, {"1d": ("1h", 24), "12h": ("1h", 12), "4h": ("5m", 48), "1h": ("5m", 12)})
        paths = paths_from_bars(self.cache, saved_bars(self.cache, "sym:AAA", day.date(), day.date(), "4h"), "4h")
        self.assertEqual(list(paths.columns), path_names(48))
        self.assertEqual(list(paths.index), [pd.Timestamp(day), pd.Timestamp(day + dt.timedelta(hours=8))])
        np.testing.assert_allclose(paths.iloc[0], np.arange(1.0, 49.0))
        np.testing.assert_allclose(paths.iloc[1], np.arange(97.0, 145.0))
        # The same candles as 1h bars: twelve five-minute closes each.
        self.cache.upsert_ohlcv_intraday([OhlcvBar("sym:AAA", "1h", day + dt.timedelta(hours=k), 1, 1, 1, 1, 1.0, "ccxt:okx:AAA/USDT", False) for k in range(12)])
        hourly = paths_from_bars(self.cache, saved_bars(self.cache, "sym:AAA", day.date(), day.date(), "1h"), "1h")
        self.assertEqual(len(hourly), 11)  # the hour with the missing five minutes has no path
        np.testing.assert_allclose(hourly.iloc[0], np.arange(1.0, 13.0))


class FetchTests(unittest.TestCase):
    """The hours are asked for once: later calls fetch only days before and after, and a venue's gap stays a gap."""

    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.cache = Cache(Path(self.folder.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())  # before the folder goes: Windows keeps an open file
        self.cache.upsert_ohlcv([day_row(dt.date(2024, 1, k), "ccxt:binance:AAA/USDT") for k in range(1, 11)])
        self.fetcher = OhlcvFetcher(self.cache)
        self.asked: list[tuple[dt.date, dt.date]] = []
        self.missing = {(dt.date(2024, 1, 5), 5)}          # the venue never had this hour
        self.late = {(dt.date(2024, 1, 10), h) for h in range(20, 24)}  # not published yet
        self.fail = False
        self.listed = None                                    # asked from before this day the venue answers nothing
        self.ref = CoinRef(None, "AAA")
        self.enterContext(patch.object(ohlcv, "utc_now", return_value=dt.datetime(2024, 1, 11, 0, 15, tzinfo=dt.timezone.utc)))
        self.enterContext(patch.object(self.fetcher, "_fetch_ccxt_intraday_one", side_effect=self.venue))

    def venue(self, name, pair, ref, start, end, tf, strict=False):
        self.asked.append((start, end))
        if self.fail:
            raise RuntimeError("venue down")
        if self.listed and start < self.listed:
            return []
        bars, day = [], start
        while day <= end:
            if tf == "5m":
                bars += fine_bars(ref.cache_key, "5m", dt.datetime.combine(day, dt.time.min), [100.0 + k for k in range(288)])
            else:
                bars += hour_bars(ref.cache_key, day, {h: 100.0 + h for h in range(24)
                                                        if (day, h) not in self.missing and (day, h) not in self.late})
            day += dt.timedelta(days=1)
        return bars

    def days(self, start, end):
        return [t.date().day for t in self.fetcher.get_path(self.ref, dt.date(2024, 1, start), dt.date(2024, 1, end)).index]

    def test_only_new_days_are_fetched_and_gaps_are_not_asked_again(self):
        self.assertEqual(self.days(3, 8), [3, 4, 6, 7, 8])      # day 5 lacks an hour: no path
        self.assertEqual(self.asked, [(dt.date(2024, 1, 3), dt.date(2024, 1, 8))])
        self.assertEqual(self.days(1, 10), [1, 2, 3, 4, 6, 7, 8, 9])
        self.assertEqual(self.asked[1:], [(dt.date(2024, 1, 1), dt.date(2024, 1, 2)), (dt.date(2024, 1, 9), dt.date(2024, 1, 10))])
        # Day 10 is new and not whole: it is asked for again until its hours are there. Day 5 never is.
        self.late.clear()
        self.assertEqual(self.days(1, 10), [1, 2, 3, 4, 6, 7, 8, 9, 10])
        self.assertEqual(self.asked[3:], [(dt.date(2024, 1, 10), dt.date(2024, 1, 10))])
        self.assertEqual(self.days(1, 10), [1, 2, 3, 4, 6, 7, 8, 9, 10])
        self.assertEqual(len(self.asked), 4)                    # nothing left to ask

    def test_a_coins_first_day_does_not_hide_its_hours(self):
        # The coin's first candle is from 2024-01-01, but its hours begin during that day: asked from
        # midnight the venue answers nothing. The following day is asked before anything is concluded.
        self.listed = dt.date(2024, 1, 2)
        self.assertEqual(self.days(1, 9), [2, 3, 4, 6, 7, 8, 9])
        self.assertEqual(self.asked, [(dt.date(2024, 1, 1), dt.date(2024, 1, 9)), (dt.date(2024, 1, 2), dt.date(2024, 1, 9))])
        # A venue with no hours at all leaves no note behind, so it is asked again next time.
        other = OhlcvFetcher(self.cache)
        calls = []
        with patch.object(other, "_fetch_ccxt_intraday_one", side_effect=lambda *a, **k: calls.append(a[3]) or []):
            self.cache.upsert_ohlcv([OhlcvRow("sym:BBB", dt.date(2024, 1, k), 1, 1, 1, 1, 1.0, "ccxt:okx:BBB/USDT", False) for k in range(1, 6)])
            for _ in range(2):
                self.assertTrue(other.get_path(CoinRef(None, "BBB"), dt.date(2024, 1, 1), dt.date(2024, 1, 5)).empty)
        self.assertEqual(len(calls), 8)  # four tries (the first day and the three after it), both times

    def test_each_finer_timeframe_keeps_its_own_account(self):
        day = dt.datetime(2024, 1, 2)
        self.cache.upsert_ohlcv_intraday([OhlcvBar("sym:AAA", "1h", day + dt.timedelta(hours=k), 1, 1, 1, 1, 1.0, "ccxt:binance:AAA/USDT", False)
                                         for k in range(48)])
        first = self.fetcher.get_path(self.ref, dt.date(2024, 1, 2), dt.date(2024, 1, 3), "1h")
        self.assertEqual((len(first), list(first.columns)), (48, path_names(12)))
        np.testing.assert_allclose(first.iloc[1], [112.0 + k for k in range(12)])
        self.assertEqual(self.asked, [(dt.date(2024, 1, 2), dt.date(2024, 1, 3))])
        # The hourly candles for the daily bars are another account: asked for on their own, once each.
        self.assertEqual(self.days(2, 3), [2, 3])
        self.fetcher.get_path(self.ref, dt.date(2024, 1, 2), dt.date(2024, 1, 3), "1h")
        self.assertEqual(len(self.asked), 2)
        with self.assertRaisesRegex(ValueError, "no path is defined"):
            self.fetcher.get_path(self.ref, dt.date(2024, 1, 2), dt.date(2024, 1, 3), "weekly")

    def test_a_venue_that_cannot_be_reached_is_asked_again(self):
        self.fail = True
        self.assertEqual(self.days(1, 9), [])
        self.fail = False
        self.assertEqual(self.days(1, 9), [1, 2, 3, 4, 6, 7, 8, 9])
        self.assertEqual(len(self.asked), 2)


def hourly_world(days=760, seed=7):
    """Four coins whose daily candles are made of their own hours, with the hours as paths; a changing ranking."""
    first = dt.date(2024, 1, 1)
    index = pd.date_range(first, periods=days, freq="D")
    frames, paths = {}, {}
    for k, symbol in enumerate(["BTC", "AAA", "BBB", "CCC"]):
        rng = np.random.default_rng(seed + k)
        steps = np.exp(np.cumsum(rng.normal(0.00002 * (k - 1), 0.007, days * 24))) * 50
        hours = steps.reshape(days, 24)
        opens = np.concatenate([[50.0], hours[:-1, -1]])
        frames[symbol] = pd.DataFrame({"open": opens, "high": np.maximum(hours.max(axis=1), opens), "low": np.minimum(hours.min(axis=1), opens),
                                       "close": hours[:, -1], "volume": 1.0}, index=index)
        paths[symbol] = pd.DataFrame(hours, index=index, columns=HOURS)

    def ranking(day):
        k = (day - first).days
        names = ["BTC", "AAA", "BBB", "CCC"]
        names = names[(k // 4) % 4:] + names[:(k // 4) % 4]
        return [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=1e9 / (i + 1), price=1.0) for i, s in enumerate(names[:3])]

    return index, frames, paths, ranking


RAW = {"topN": 3, "cadence": "daily", "rankingResolution": "daily", "rankingSource": "cmc", "marketFilter": False,
       "includeUsd": True, "feeRate": 0.001, "slippageRate": 0.0, "indicator": {"trend": "dmi"}}


class EngineAndLiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.index, cls.frames, cls.paths, ranking = hourly_world()
        cls.ranking = staticmethod(ranking)
        cls.config = rpc._build_config({**RAW, "startDate": cls.index[0].date().isoformat(), "endDate": cls.index[-1].date().isoformat()})
        cls.timeline = UniverseTimeline([UniverseSnapshot(ts.date(), "test", ranking(ts.date())) for ts in cls.index])
        cls.with_paths = {s: f.join(cls.paths[s]) for s, f in cls.frames.items()}
        simulate = BacktestEngine(registry=object(), ohlcv=object())._simulate
        cls.held = simulate(cls.config, cls.timeline, cls.with_paths, [], None)[1]
        cls.held_stand_in = simulate(cls.config, cls.timeline, cls.frames, [], None)[1]

    def test_the_real_candle_is_the_hours_own_range(self):
        a, b = self.with_paths["AAA"], self.with_paths["BBB"]
        r = ratio_frame(a, b)
        hourly = self.paths["AAA"].to_numpy() / self.paths["BBB"].to_numpy()
        opens = (self.frames["AAA"]["open"] / self.frames["BBB"]["open"]).to_numpy()
        np.testing.assert_allclose(r["high"], np.maximum(hourly.max(axis=1), opens))
        np.testing.assert_allclose(r["low"], np.minimum(hourly.min(axis=1), opens))
        stand_in = ratio_frame(self.frames["AAA"], self.frames["BBB"])
        self.assertGreater(float(((r["high"] - r["low"]) / (stand_in["high"] - stand_in["low"])).median()), 1.05)

    def test_a_member_that_reads_high_and_low_decides_differently_on_real_candles(self):
        differing = int((self.held.iloc[560:] != self.held_stand_in.iloc[560:]).sum())
        self.assertGreater(differing, 0)
        self.assertGreater(self.held.iloc[560:].nunique(), 2)

    def live(self, as_of, clock, paths=None):
        frames, ranking, paths = self.frames, self.ranking, self.paths if paths is None else paths

        def cut(frame, start, end):
            return frame.loc[(frame.index >= pd.Timestamp(start)) & (frame.index < pd.Timestamp(end) + pd.Timedelta(days=1))]

        fetcher = SimpleNamespace(get_series=lambda ref, start, end, timeframe: SimpleNamespace(frame=cut(frames[ref.symbol], start, end)),
                                  get_path=lambda ref, start, end, timeframe="1d": cut(paths[ref.symbol], start, end))
        registry = Mock(get_top_n=Mock(side_effect=lambda day, n, **_: SimpleNamespace(coins=ranking(day), provider="test")))
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=clock), patch.object(rpc, "get_default_cache", return_value=object()), \
             patch.object(rpc, "_notify"), patch.object(rpc, "RankingRegistry", return_value=registry), \
             patch.object(rpc, "OhlcvFetcher", return_value=fetcher):
            return rpc._method_live_snapshot({"asOf": as_of.isoformat(), "config": RAW})

    def test_the_bot_judges_the_same_candles_as_the_replay(self):
        for k in range(600, 756, 13):
            new_day = self.index[k + 1].date()
            signal = self.live(new_day, dt.datetime.combine(new_day, dt.time(0, 15), tzinfo=dt.timezone.utc))
            self.assertEqual(signal["best"], self.held.iloc[k + 1], f"close of {self.index[k].date()}")
            self.assertEqual(signal["pathMissing"], [])

    def test_hours_that_are_late_are_named_and_a_venue_without_hours_is_not(self):
        k = 700
        closed, new_day = self.index[k], self.index[k + 1].date()
        paths = {s: p.copy() for s, p in self.paths.items()}
        coins = [c.symbol for c in self.ranking(new_day)]
        late, never = coins[0], coins[1]
        paths[late] = paths[late].drop(closed)                                # yesterday's hours are not there yet
        paths[never] = paths[never].drop([closed, self.index[k - 1]])         # this venue had none the day before either
        signal = self.live(new_day, dt.datetime.combine(new_day, dt.time(0, 15), tzinfo=dt.timezone.utc), paths)
        self.assertEqual(signal["pathMissing"], [late])


class EngineRunTests(unittest.TestCase):
    def test_a_run_attaches_the_paths_and_says_how_many_days_carry_one(self):
        index, frames, paths, ranking = hourly_world(days=120)
        paths["CCC"] = paths["CCC"].iloc[:60]
        raw = {**RAW, "indicator": {"trend": "ema_cross"}, "startDate": index[10].date().isoformat(), "endDate": index[-1].date().isoformat()}
        seen = {}

        class Probe(BacktestEngine):
            def _simulate(self, config, universe, frames, notes, gate=None, *, parallel_pairs=False):
                seen.update(frames)
                return super()._simulate(config, universe, frames, notes, gate=gate, parallel_pairs=False)

        def cut(frame, start, end):
            return frame.loc[(frame.index >= pd.Timestamp(start)) & (frame.index < pd.Timestamp(end) + pd.Timedelta(days=1))]

        source = SimpleNamespace(get_series=lambda ref, start, end, timeframe: SimpleNamespace(frame=cut(frames[ref.symbol], start, end)),
                                 get_path=lambda ref, start, end, timeframe="1d": cut(paths[ref.symbol], start, end))
        registry = Mock(get_top_n=Mock(side_effect=lambda day, n, **_: SimpleNamespace(coins=ranking(day), provider="test")))
        with patch("rotation_lab.backtest.engine._require_daily_archive"), \
             patch("rotation_lab.backtest.engine.utc_now", return_value=dt.datetime(2030, 1, 1, tzinfo=dt.timezone.utc)):
            result = Probe(registry=registry, ohlcv=source).run(rpc._build_config(raw))
        self.assertEqual(path_columns(seen["BTC"]), HOURS)
        note = next(n for n in result.notes if n.startswith("Coin-vs-coin candles"))
        self.assertIn("CCC 60 days", note)
        self.assertRegex(note, r"1h path on 8\d\.\d% of coin-days")
        # A source without hours (older stand-ins, a shuffled world) leaves the candles as they are.
        frame = frames["BTC"]
        self.assertIs(with_path(frame, lambda *a: None, CoinRef(None, "BTC"), index[0].date(), index[-1].date()), frame)
        self.assertIs(with_path(frame, Mock(side_effect=RuntimeError("down")), CoinRef(None, "BTC"), index[0].date(), index[-1].date()), frame)
        self.assertIn("no 1h candles are saved", path_note({"BTC": frame, "AAA": frames["AAA"]}, index[0].date()))

    def test_an_intraday_run_attaches_its_bars_paths_too(self):
        # A 4h cadence: 48 five-minute closes per bar, asked for with the bars' own timeframe.
        index = pd.date_range("2024-01-01", periods=240, freq="4h")
        rng = np.random.default_rng(3)
        frames, paths, asked = {}, {}, []
        for k, symbol in enumerate(["BTC", "AAA", "BBB"]):
            fine = np.exp(np.cumsum(rng.normal(0, 0.002, (len(index), 48)), axis=1) + rng.normal(0, 0.02, (len(index), 1)).cumsum(axis=0)) * 50
            opens = np.concatenate([[50.0], fine[:-1, -1]])
            frames[symbol] = pd.DataFrame({"open": opens, "high": np.maximum(fine.max(axis=1), opens), "low": np.minimum(fine.min(axis=1), opens),
                                           "close": fine[:, -1], "volume": 1.0}, index=index)
            paths[symbol] = pd.DataFrame(fine, index=index, columns=path_names(48))
        coins = [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=1e9 / (i + 1), price=1.0) for i, s in enumerate(frames)]
        raw = {**RAW, "cadence": "4h", "indicator": {"trend": "ema_cross"}, "startDate": "2024-01-05", "endDate": "2024-02-09"}
        seen = {}

        class Probe(BacktestEngine):
            def _simulate(self, config, universe, frames, notes, gate=None, *, parallel_pairs=False):
                seen.update(frames)
                return super()._simulate(config, universe, frames, notes, gate=gate, parallel_pairs=False)

        def cut(frame, start, end):
            return frame.loc[(frame.index >= pd.Timestamp(start)) & (frame.index < pd.Timestamp(end) + pd.Timedelta(days=1))]

        def get_path(ref, start, end, timeframe="1d"):
            asked.append(timeframe)
            return cut(paths[ref.symbol], start, end)

        source = SimpleNamespace(get_series=lambda ref, start, end, timeframe: SimpleNamespace(frame=cut(frames[ref.symbol], start, end)), get_path=get_path)
        registry = Mock(get_top_n=Mock(side_effect=lambda day, n, **_: SimpleNamespace(coins=coins, provider="test")))
        with patch("rotation_lab.backtest.engine._require_daily_archive"), \
             patch("rotation_lab.backtest.engine.utc_now", return_value=dt.datetime(2030, 1, 1, tzinfo=dt.timezone.utc)):
            result = Probe(registry=registry, ohlcv=source).run(rpc._build_config(raw))
        self.assertEqual(set(asked), {"4h"})
        self.assertEqual(path_columns(seen["AAA"]), path_names(48))
        self.assertIn("5m path on 100.0% of coin-bars", next(n for n in result.notes if n.startswith("Coin-vs-coin candles")))
        r = ratio_frame(seen["AAA"], seen["BBB"])
        along = (paths["AAA"].to_numpy() / paths["BBB"].to_numpy())[index >= seen["AAA"].index[0]]
        np.testing.assert_allclose(r["high"], np.maximum(along.max(axis=1), r["open"]))


class CertificationTests(unittest.TestCase):
    def test_the_gauntlets_pair_candle_is_the_engines(self):
        from smithery.robustness import RATIO_PATH, ratio_frame as certified_frame
        days = pd.date_range("2024-01-01", periods=3, freq="D", tz="UTC")
        hours = pd.date_range("2024-01-01", periods=72, freq="h", tz="UTC")
        a_hours = pd.DataFrame({"close": 100.0}, index=hours)
        b_hours = pd.DataFrame({"close": 1.0}, index=hours)
        a_hours.loc[hours[4], "close"], b_hours.loc[hours[19], "close"] = 110.0, 1.1
        a = pd.DataFrame({"open": 100.0, "high": [110, 100, 100], "low": 100.0, "close": 100.0, "volume": 5.0}, index=days)
        b = pd.DataFrame({"open": 1.0, "high": [1.1, 1, 1], "low": 1.0, "close": 1.0, "volume": 7.0}, index=days)
        real = certified_frame(a, b, a_hours, b_hours, 24)
        self.assertEqual(list(real.columns), ["open", "high", "low", "close", "volume"])
        np.testing.assert_allclose(real.iloc[0][["high", "low"]], [110, 100 / 1.1])
        np.testing.assert_allclose(certified_frame(a, b).iloc[0][["high", "low"]], [100, 100])  # no finer series: the stand-in
        self.assertEqual(RATIO_PATH, {"1d": ("1h", 24), "4h": ("5m", 48), "1h": ("5m", 12)})

    def test_the_intraday_tracks_read_five_minute_series_kept_beside_the_shelf(self):
        from smithery import data, robustness
        hours = pd.date_range("2024-01-01", periods=6, freq="h", tz="UTC")
        minutes = pd.date_range("2024-01-01", periods=72, freq="5min", tz="UTC")
        a5 = pd.DataFrame({"close": 100.0}, index=minutes)
        b5 = pd.DataFrame({"close": 1.0}, index=minutes)
        a5.loc[minutes[3], "close"], b5.loc[minutes[9], "close"] = 110.0, 1.1
        a = pd.DataFrame({"open": 100.0, "high": [110, 100, 100, 100, 100, 100], "low": 100.0, "close": 100.0, "volume": 5.0}, index=hours)
        b = pd.DataFrame({"open": 1.0, "high": [1.1, 1, 1, 1, 1, 1], "low": 1.0, "close": 1.0, "volume": 7.0}, index=hours)
        real = robustness.ratio_frame(a, b, a5, b5, 12, pd.Timedelta(minutes=5))
        np.testing.assert_allclose(real.iloc[0][["high", "low"]], [110, 100 / 1.1])
        with tempfile.TemporaryDirectory() as folder, patch.object(data, "PRICE_DIR", Path(folder)):
            self.assertIsNone(data.path_series("ETHUSDT", "5m"))
            out = a5.copy()
            out.index.name = "timestamp"
            data.path_dir().mkdir()
            self.assertEqual(data.path_dir(), Path(folder) / "Paths")  # it follows the shelf, wherever that is
            out.reset_index().to_csv(data.path_csv("BINANCE_ETHUSDT_5m"), index=False)
            loaded = robustness.fine_series("ETHUSDT", "5m")
            self.assertEqual((len(loaded), float(loaded["close"].iloc[3])), (72, 110.0))
            self.assertEqual(data.available(), data.available())  # the shelf's own list does not see the path folder
            self.assertNotIn("BINANCE_ETHUSDT_5m", data.available())


class ResearchSourceTests(unittest.TestCase):
    def test_price_variants_and_frozen_answers_hand_the_paths_on(self):
        from smithery.lab_compare import FrozenService
        from smithery.prices import PricesVariant
        path = pd.DataFrame(np.ones((2, 24)), index=pd.date_range("2024-01-01", periods=2, freq="D"), columns=HOURS)
        base = SimpleNamespace(get_path=Mock(return_value=path))
        self.assertIs(PricesVariant(base, {"kind": "noise", "seed": 1}).get_path("ref", 1, 2), path)
        self.assertIsNone(PricesVariant(base, {"kind": "shuffle", "start": "2024-01-01", "end": "2024-01-02"}).get_path("ref", 1, 2))
        with tempfile.TemporaryDirectory() as folder:
            frozen = FrozenService(base, {"get_path"}, Path(folder), dt.date(2023, 1, 1))
            first, second = frozen.get_path("ref", dt.date(2024, 1, 1), dt.date(2024, 1, 2)), frozen.get_path("ref", dt.date(2024, 1, 1), dt.date(2024, 1, 2))
            pd.testing.assert_frame_equal(first, path)
            pd.testing.assert_frame_equal(second, path)
            self.assertEqual(base.get_path.call_args.args[1], dt.date(2023, 1, 1))  # every request starts at the earliest warm-up
            self.assertEqual(len(list(Path(folder).glob("paths-*.csv"))), 1)

    def test_the_other_exchange_world_follows_that_exchanges_own_hours(self):
        from smithery.lab_compare import SavedPrices
        from smithery.prices import PricesVariant
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        saved, venue = Cache(Path(folder.name) / "cache.sqlite"), Cache(Path(folder.name) / "venue.sqlite")
        d = [dt.date(2024, 1, k) for k in range(1, 4)]
        saved.upsert_ohlcv([day_row(day, "ccxt:binance:AAA/USDT") for day in d])
        saved.upsert_ohlcv_intraday([bar for day in d for bar in hour_bars(market_key("binance", "AAA/USDT"), day, {h: 100.0 for h in range(24)})])
        # The other exchange has the first two days' candles, and the hours of the first only.
        venue.upsert_ohlcv([day_row(day, "ccxt:okx:AAA/USDT", close=101.0) for day in d[:2]])
        venue.upsert_ohlcv_intraday(hour_bars(market_key("okx", "AAA/USDT"), d[0], {h: 200.0 + h for h in range(24)}))
        for cache in (saved, venue):
            self.addCleanup(cache._con.close)
        world = PricesVariant(SavedPrices(saved), {"kind": "venue", "path": str(Path(folder.name) / "venue.sqlite")})
        self.addCleanup(world._venue._con.close)
        ref = CoinRef(None, "AAA")
        self.assertEqual(float(world.get_series(ref, d[0], d[2]).frame["close"].iloc[0]), 101.0)
        paths = world.get_path(ref, d[0], d[2])
        self.assertEqual(list(paths.columns), HOURS)
        self.assertEqual([t.date() for t in paths.index], d)
        np.testing.assert_allclose(paths.iloc[0], [200.0 + h for h in range(24)])  # its own hours
        np.testing.assert_allclose(paths.iloc[1], 100.0)  # its candle, but no hours there: the saved path stays
        np.testing.assert_allclose(paths.iloc[2], 100.0)  # no candle there: the saved candle and its path
        # Without any hours of the other exchange the saved paths are handed on unchanged.
        venue._con.execute("delete from ohlcv_intraday")
        venue._con.commit()
        pd.testing.assert_frame_equal(world.get_path(ref, d[0], d[2]), SavedPrices(saved).get_path(ref, d[0], d[2]))


if __name__ == "__main__":
    unittest.main()
