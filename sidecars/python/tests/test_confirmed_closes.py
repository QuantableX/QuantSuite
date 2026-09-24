"""Forming candles must never affect backtest returns or survive in the cache."""

import datetime as dt
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import Cadence, IndicatorConfig, RunConfig
from rotation_lab.data.cache import Cache, OhlcvBar, OhlcvRow
from rotation_lab.data.ohlcv import CoinRef, OhlcvFetcher, confirmed_frame
from rotation_lab.data.ranking.base import RankedCoin


NOW = dt.datetime(2026, 9, 24, 14, 30, tzinfo=dt.timezone.utc)
REF = CoinRef(None, "AAA")


def frame(index):
    close = 100 + np.arange(len(index), dtype=float)
    return pd.DataFrame({"open": close - 1, "high": close + 1, "low": close - 2,
                         "close": close, "volume": 1.0}, index=index)


class ConfirmedBacktestTests(unittest.TestCase):
    def run_backtest(self, data, cadence, now=NOW, end=None):
        config = RunConfig(
            top_n=1, start_date=data.index[0].date(), end_date=end or NOW.date(), cadence=cadence,
            indicator=IndicatorConfig(trend="aggregate", aggregate=("ema_cross",)),
            compare_trends=("ema_cross",), market_filter=True,
        )
        coin = RankedCoin(rank=1, cg_id=None, symbol="AAA", name="Synthetic", market_cap=1e9, price=100)
        timeline = UniverseTimeline([UniverseSnapshot(config.start_date, "test", [coin])])
        fetcher = SimpleNamespace(get_series=lambda *args: SimpleNamespace(frame=data.copy()))
        with patch("rotation_lab.backtest.engine.utc_now", return_value=now), \
             patch("rotation_lab.backtest.engine.build_universe_timeline", return_value=timeline):
            return BacktestEngine(registry=object(), ohlcv=fetcher).run(config)

    def assert_same_result(self, actual, expected):
        pd.testing.assert_series_equal(actual.equity_strategy, expected.equity_strategy)
        pd.testing.assert_series_equal(actual.held_asset, expected.held_asset)
        np.testing.assert_allclose(actual.metrics_strategy.as_row(), expected.metrics_strategy.as_row(), equal_nan=True)
        for left, right in zip(actual.strategies, expected.strategies):
            pd.testing.assert_series_equal(left.equity_strategy, right.equity_strategy)
            pd.testing.assert_series_equal(left.market_gate, right.market_gate)
            np.testing.assert_allclose(left.metrics_strategy.as_row(), right.metrics_strategy.as_row(), equal_nan=True)
        self.assertEqual(set(actual.benchmarks), {"BTC EMA L/S", "BTC EMA L/C"})
        for attr, metrics in (("buy_and_hold", "metrics_buy_and_hold"), ("benchmarks", "metrics_benchmarks")):
            for key, series in getattr(actual, attr).items():
                pd.testing.assert_series_equal(series, getattr(expected, attr)[key])
                np.testing.assert_allclose(getattr(actual, metrics)[key].as_row(),
                                           getattr(expected, metrics)[key].as_row(), equal_nan=True)

    def test_forming_close_cannot_change_any_curve_metric_signal_or_holding(self):
        for cadence in Cadence:
            with self.subTest(cadence=cadence):
                tf = cadence.ccxt_timeframe
                freq = "D" if tf == "1d" else tf
                last_open = pd.Timestamp(NOW).tz_localize(None).floor(freq)
                data = frame(pd.date_range(end=last_open, periods=90, freq=freq))
                expected = self.run_backtest(data.iloc[:-1], cadence)
                data.loc[last_open, ["high", "close"]] = 1e12
                result = self.run_backtest(data, cadence)
                self.assert_same_result(result, expected)
                self.assertEqual(len(result.strategies), 2)
                self.assertNotIn(last_open, result.equity_strategy.index)
                self.assertIn("Only confirmed candle closes", " ".join(result.notes))

    def test_daily_close_enters_at_midnight_utc_and_valuations_include_it(self):
        data = frame(pd.date_range("2026-09-20", "2026-09-24"))
        boundary = dt.datetime(2026, 9, 25, tzinfo=dt.timezone.utc)
        before = self.run_backtest(data, Cadence.DAILY, boundary - dt.timedelta(microseconds=1))
        after = self.run_backtest(data, Cadence.DAILY, boundary)
        self.assertEqual(before.equity_strategy.index[-1], pd.Timestamp("2026-09-23"))
        self.assertEqual(after.equity_strategy.index[-1], pd.Timestamp("2026-09-24"))
        self.assertGreater(after.metrics_strategy.net_return_multiplier, before.metrics_strategy.net_return_multiplier)
        # A historical run retains its last bar; it is not blindly dropped.
        historical = self.run_backtest(data, Cadence.DAILY, boundary + dt.timedelta(days=20))
        self.assert_same_result(after, historical)

    def test_future_end_and_future_rows_do_not_extend_confirmed_history(self):
        data = frame(pd.date_range("2026-09-20", "2026-09-28"))
        result = self.run_backtest(data, Cadence.DAILY, end=dt.date(2026, 9, 28))
        expected = self.run_backtest(data.iloc[:4], Cadence.DAILY)
        self.assert_same_result(result, expected)

    def test_window_with_only_an_open_daily_candle_is_empty(self):
        result = self.run_backtest(frame(pd.date_range("2026-09-24", periods=1)), Cadence.DAILY)
        self.assertTrue(result.equity_strategy.empty)
        self.assertEqual(result.strategies, [])

    def test_confirmation_uses_utc_for_timezone_aware_indexes(self):
        index = pd.date_range("2026-09-24 14:00", periods=3, freq="h", tz="Europe/Berlin")
        result = confirmed_frame(frame(index), "1h", NOW)
        self.assertEqual(list(result.index), list(index[:2]))


class ConfirmedFetcherTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.cache = Cache(Path(directory.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())
        self.fetcher = OhlcvFetcher(self.cache, min_request_interval=0, ccxt_exchanges=("test",))
        self.exchange = Mock()
        self.fetcher._ensure_exchange = Mock(return_value=self.exchange)
        self.fetcher._resolve_pairs = Mock(return_value=[("test", "AAA/USD")])
        self.fetcher._coingecko_id = Mock(return_value=None)
        self.raw = []
        self.exchange.fetch_ohlcv.side_effect = lambda pair, timeframe, since=0, limit=1000: [
            row for row in self.raw if row[0] >= since][:limit]

    def set_rows(self, indexes, close=110):
        self.raw = [[int(pd.Timestamp(ts, tz="UTC").timestamp() * 1000), 100, close, 99, close, 1]
                    for ts in indexes]

    def fetch(self, start, end, tf="1d", now=NOW):
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=now):
            return self.fetcher.get_series(REF, start, end, tf).frame

    def test_daily_open_is_not_cached_and_final_value_is_fetched_after_close(self):
        start, end = dt.date(2026, 9, 22), NOW.date()
        self.set_rows(pd.date_range(start, end))
        result = self.fetch(start, end)
        self.assertEqual(list(result.index), list(pd.date_range(start, "2026-09-23")))
        self.assertEqual([r.day for r in self.cache.get_ohlcv_range("sym:AAA", start, end)],
                         [start, dt.date(2026, 9, 23)])
        self.exchange.fetch_ohlcv.reset_mock()
        pd.testing.assert_frame_equal(self.fetch(start, end), result)
        self.exchange.fetch_ohlcv.assert_not_called()
        self.raw[-1][4] = 123
        after = self.fetch(start, end, now=dt.datetime(2026, 9, 25, tzinfo=dt.timezone.utc))
        self.assertEqual(after.iloc[-1]["close"], 123)
        self.assertEqual(after.index[-1].date(), end)

    def test_daily_non_midnight_exchange_close_is_checked_before_date_normalization(self):
        start, end = dt.date(2026, 9, 22), NOW.date()
        self.set_rows(pd.date_range("2026-09-22 16:00", periods=3, freq="D"))
        before = self.fetch(start, end)
        self.assertEqual(list(before.index), [pd.Timestamp("2026-09-22")])
        boundary = NOW.replace(hour=16, minute=0)
        after = self.fetch(start, end, now=boundary)
        self.assertEqual(list(after.index), list(pd.date_range("2026-09-22", "2026-09-23")))

    def test_intraday_boundary_refetches_final_candle_and_reuses_confirmed_cache(self):
        for tf in ("1h", "4h", "12h"):
            with self.subTest(tf=tf):
                start, end = dt.date(2026, 9, 23), NOW.date()
                last_open = pd.Timestamp(NOW).tz_localize(None).floor(tf)
                self.set_rows(pd.date_range(start, last_open, freq=tf))
                result = self.fetch(start, end, tf)
                self.assertNotIn(last_open, result.index)
                cached = self.cache.get_ohlcv_intraday_range("sym:AAA", tf, dt.datetime(2026, 9, 23),
                                                            dt.datetime(2026, 9, 25))
                self.assertEqual(len(cached), len(self.raw) - 1)
                self.exchange.fetch_ohlcv.reset_mock()
                pd.testing.assert_frame_equal(self.fetch(start, end, tf), result)
                self.exchange.fetch_ohlcv.assert_not_called()
                self.raw[-1][4] = 456
                boundary = (last_open + pd.Timedelta(tf)).tz_localize("UTC").to_pydatetime()
                after = self.fetch(start, end, tf, boundary)
                self.assertEqual(after.index[-1], last_open)
                self.assertEqual(after.iloc[-1]["close"], 456)

    def test_legacy_daily_partial_tail_is_refetched_even_after_a_long_gap(self):
        start, end = dt.date(2025, 1, 1), dt.date(2025, 1, 4)
        self.cache.upsert_ohlcv([OhlcvRow("sym:AAA", day.date(), 100, 100, 100, 999, 1, "test", False)
                                 for day in pd.date_range(start, end)])
        self.set_rows(pd.date_range(start, end), close=111)
        result = self.fetch(start, end)
        self.assertEqual(result.iloc[-1]["close"], 111)
        self.exchange.fetch_ohlcv.reset_mock()
        pd.testing.assert_frame_equal(self.fetch(start, end), result)
        self.exchange.fetch_ohlcv.assert_not_called()

    def test_legacy_intraday_tail_is_never_used_when_refetch_fails(self):
        start = dt.datetime(2025, 1, 1)
        self.cache.upsert_ohlcv_intraday([OhlcvBar("sym:AAA", "1h", start + dt.timedelta(hours=i),
                                                100, 100, 100, 999, 1, "test", False) for i in range(3)])
        result = self.fetch(start.date(), start.date(), "1h")
        self.assertEqual(list(result.index), list(pd.date_range(start, periods=2, freq="h")))

    def test_migration_preserves_older_history_and_other_coins_and_runs_once(self):
        for symbol in ("sym:AAA", "sym:BBB"):
            self.cache.upsert_ohlcv([OhlcvRow(symbol, day.date(), 1, 1, 1, 1, 1, "test", False)
                                     for day in pd.date_range("2025-01-01", periods=5)])
        self.cache.ensure_confirmed_ohlcv("sym:AAA", "1d")
        self.cache.ensure_confirmed_ohlcv("sym:AAA", "1d")
        for symbol, count in (("sym:AAA", 3), ("sym:BBB", 5)):
            self.assertEqual(len(self.cache.get_ohlcv_range(symbol, dt.date(2025, 1, 1), dt.date(2025, 1, 5))), count)

    def test_coingecko_fallback_excludes_todays_price_sample(self):
        self.fetcher._session = Mock()
        self.set_rows(pd.date_range("2026-09-22", periods=3))
        self.fetcher._session.get.return_value.status_code = 200
        self.fetcher._session.get.return_value.json.return_value = {
            "prices": [[row[0], row[4]] for row in self.raw], "total_volumes": []}
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW):
            rows = self.fetcher._fetch_coingecko(REF, dt.date(2026, 9, 22), NOW.date(), "synthetic")
        self.assertEqual([row.day for row in rows], [dt.date(2026, 9, 22), dt.date(2026, 9, 23)])


if __name__ == "__main__":
    unittest.main()
