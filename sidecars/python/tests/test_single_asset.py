"""Single-market manual systems: exact data, signed signals and mode isolation."""
import datetime as dt
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.config import Cadence, IndicatorConfig, RunConfig, SingleAssetConfig
from rotation_lab.data.cache import Cache
from rotation_lab.data.ohlcv import OhlcvFetcher
from rotation_lab.rpc import _build_config, _method_backtest, _method_live_snapshot, _trades_to_dicts

NOW = dt.datetime(2026, 9, 24, 14, 30, tzinfo=dt.timezone.utc)


def candles(freq="D"):
    close = [100, 110, 90, 100, 120, 1e9]
    return pd.DataFrame({"open": 100., "high": np.maximum(close, 100), "low": np.minimum(close, 100),
                         "close": close, "volume": 50.}, index=pd.date_range("2026-09-19", periods=6, freq=freq))


def config(direction="long_cash", pair="BTC/USD"):
    return RunConfig(mode="single_asset", single_asset=SingleAssetConfig(pair=pair, direction=direction),
                     start_date=dt.date(2026, 9, 19), end_date=dt.date(2026, 9, 24), fee_rate=0.01)


class SingleAssetTests(unittest.TestCase):
    def run_system(self, cfg, data=None, signals=None):
        data = candles() if data is None else data
        fetcher = Mock()
        fetcher.get_pair_series.return_value = SimpleNamespace(frame=data)
        with patch("rotation_lab.backtest.engine.utc_now", return_value=NOW), \
             patch("rotation_lab.backtest.engine.build_universe_timeline", side_effect=AssertionError("Ranking used")), \
             patch("rotation_lab.backtest.engine.market_index", side_effect=AssertionError("TOTAL used")):
            engine = BacktestEngine(registry=object(), ohlcv=fetcher)
            if signals is None:
                result = engine.run(cfg)
            else:
                with patch("rotation_lab.backtest.engine.direction_signal", side_effect=lambda f, _: pd.Series(signals[:len(f)], index=f.index)):
                    result = engine.run(cfg)
        fetcher.get_series.assert_not_called()
        self.assertEqual(fetcher.get_pair_series.call_args.args[:2], (cfg.single_asset.exchange, cfg.single_asset.pair))
        return result

    def test_long_cash_has_next_bar_execution_cash_and_one_fee_per_leg(self):
        result = self.run_system(config(), signals=[1, -1, 0, 1, -1])
        expected = [1, .99 * 1.1, .99**2 * 1.1, .99**2 * 1.1, .99**3 * 1.1 * 1.2]
        np.testing.assert_allclose(result.equity_strategy, expected)
        self.assertEqual(result.held_asset.tolist(), ["USD", "BTC", "USD", "USD", "BTC"])
        self.assertAlmostEqual(result.metrics_strategy.net_return_multiplier, expected[-1])
        self.assertEqual(result.benchmarks, {})
        self.assertEqual(set(result.buy_and_hold), {"BTC/USD"})

    def test_long_short_reversal_pays_two_legs_and_neutral_stays_cash(self):
        result = self.run_system(config("long_short", "ETH/BTC"), signals=[1, -1, 0, 1, -1])
        expected = [1, .99 * 1.1, .99**3 * 1.1 * 1.1, .99**4 * 1.1**2, .99**5 * 1.1**2 * 1.2]
        np.testing.assert_allclose(result.equity_strategy, expected)
        self.assertEqual(result.held_asset.tolist(), ["BTC", "ETH", "Short ETH", "BTC", "ETH"])

    def test_holdings_pnl_charges_each_trade_its_own_fills(self):
        result = self.run_system(config(), signals=[1, -1, 0, 1, -1])
        trades = result.strategies[0].trades
        self.assertEqual([t.symbol for t in trades], ["USD", "BTC", "USD", "BTC"])
        self.assertEqual([(t.trade_return, t.equity_change) for t in trades[::2]], [(None, None)] * 2)
        # Closed: buy and sell fill. Open (newest): buy fill only.
        self.assertAlmostEqual(trades[1].trade_return, .99**2 * 1.1 - 1)
        self.assertAlmostEqual(trades[3].trade_return, .99 * 1.2 - 1)
        # Equity PnL is in units of the starting capital, so it compounds.
        self.assertAlmostEqual(trades[3].equity_change, .99**2 * 1.1 * (.99 * 1.2 - 1))
        self.assertAlmostEqual(sum(t.equity_change or 0 for t in trades), result.equity_strategy.iloc[-1] - 1)
        rows = _trades_to_dicts(trades)
        self.assertEqual(rows[1]["from"], "2026-09-20")
        self.assertAlmostEqual(rows[1]["tradePct"], (.99**2 * 1.1 - 1) * 100)
        self.assertIsNone(rows[2]["equityPct"])

    def test_holdings_pnl_of_a_reversal_splits_its_two_fills(self):
        result = self.run_system(config("long_short", "ETH/BTC"), signals=[1, -1, 0, 1, -1])
        trades = result.strategies[0].trades
        self.assertEqual([t.symbol for t in trades], ["BTC", "ETH", "Short ETH", "BTC", "ETH"])
        self.assertIsNone(trades[0].trade_return)  # the quote is the cash leg
        self.assertAlmostEqual(trades[1].trade_return, .99**2 * 1.1 - 1)
        self.assertAlmostEqual(trades[2].trade_return, .99**2 * 1.1 - 1)  # short through a 10% drop
        self.assertAlmostEqual(trades[2].equity_change, .99**2 * 1.1 * (.99**2 * 1.1 - 1))
        self.assertAlmostEqual(sum(t.equity_change or 0 for t in trades), result.equity_strategy.iloc[-1] - 1)

    def test_open_candle_is_excluded_and_historical_last_bar_is_retained(self):
        before = self.run_system(config(), signals=[1] * 6)
        self.assertEqual(before.equity_strategy.index[-1], pd.Timestamp("2026-09-23"))
        data = candles()
        data.iloc[-1, data.columns.get_loc("close")] = 0.1
        changed = self.run_system(config(), data, [1] * 6)
        pd.testing.assert_series_equal(before.equity_strategy, changed.equity_strategy)
        historical = self.run_system(replace(config(), end_date=dt.date(2026, 9, 22)), signals=[1] * 6)
        self.assertEqual(historical.equity_strategy.index[-1], pd.Timestamp("2026-09-22"))

    def test_rotation_only_settings_never_affect_single_asset_results(self):
        cfg = config()
        baseline = self.run_system(cfg)
        altered = self.run_system(replace(cfg, top_n=999, exclude_top_n=900, include_usd=False,
                                         market_filter=True, cadence=Cadence.MONTHLY))
        pd.testing.assert_series_equal(baseline.equity_strategy, altered.equity_strategy)

    def test_aggregate_comparison_equals_solo_and_keeps_parameters(self):
        cfg = replace(config(), indicator=IndicatorConfig(trend="aggregate", aggregate=("ema_cross",)), compare_trends=("ema_cross",))
        result = self.run_system(cfg)
        solo = self.run_system(replace(cfg, indicator=IndicatorConfig(), compare_trends=()))
        self.assertEqual([run.key for run in result.strategies], ["aggregate", "ema_cross"])
        pd.testing.assert_series_equal(result.strategies[1].equity_strategy, solo.equity_strategy)

    def test_bankrupt_short_cannot_recover_into_negative_equity(self):
        data = candles().iloc[:5].copy()
        data.iloc[1, data.columns.get_loc("close")] = 300
        result = self.run_system(config("long_short"), data, [-1] * 5)
        self.assertEqual(result.equity_strategy.tolist(), [1, 0, 0, 0, 0])

    def test_missing_or_invalid_candles_fail_clearly(self):
        with self.assertRaisesRegex(ValueError, "No confirmed"):
            self.run_system(config(), candles().iloc[0:0])
        data = candles()
        data.iloc[1, data.columns.get_loc("open")] = 0
        with self.assertRaisesRegex(ValueError, "invalid open/close"):
            self.run_system(config(), data)

    def test_rpc_parses_modes_and_preserves_legacy_rotation_default(self):
        self.assertEqual(_build_config({}).mode, "rotation")
        self.assertEqual(_build_config({"mode": "rotation", "singleAsset": {"pair": ""}}).mode, "rotation")
        for direction in ("long_cash", "long_short"):
            cfg = _build_config({"mode": "single_asset", "singleAsset": {"pair": "ETH/BTC", "direction": direction, "timeframe": "4h"}})
            self.assertEqual(cfg.single_asset.direction, direction)
            self.assertEqual(cfg.bar_cadence, Cadence.HOUR_4)
        for raw in ({"mode": "unknown"}, {"mode": "single_asset", "singleAsset": {"pair": "BTC"}},
                    {"mode": "single_asset", "singleAsset": {"direction": "invalid"}}):
            with self.assertRaises(ValueError):
                _build_config(raw)

    def test_live_uses_confirmed_signed_signal_without_ranking(self):
        for direction, position in (("long_cash", "cash"), ("long_short", "short")):
            with self.subTest(direction=direction), \
                 patch("rotation_lab.rpc.RankingRegistry", side_effect=AssertionError("Ranking used")), \
                 patch("rotation_lab.rpc.OhlcvFetcher") as fetcher, \
                 patch("rotation_lab.rpc.get_default_cache"), \
                 patch("rotation_lab.rpc._notify"), \
                 patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW), \
                 patch("rotation_lab.backtest.signals.direction_signal", side_effect=lambda f, _: pd.Series(-1, index=f.index)):
                fetcher.return_value.get_pair_series.return_value = SimpleNamespace(frame=candles())
                result = _method_live_snapshot({"config": {"mode": "single_asset", "startDate": "2026-09-19", "endDate": "2026-09-24",
                                                   "singleAsset": {"direction": direction}}})
                self.assertEqual(result["singleAssetSignal"]["position"], position)
                self.assertEqual(result["singleAssetSignal"]["close"], 120)
                self.assertEqual(result["singleAssetSignal"]["closedAt"], "2026-09-24T00:00:00Z")
                self.assertEqual(result["mode"], "single_asset")

    def test_rpc_intraday_chart_times_follow_single_asset_timeframe(self):
        cfg = replace(config(), single_asset=replace(config().single_asset, timeframe="4h"))
        result = self.run_system(cfg, candles("4h"))
        with patch("rotation_lab.rpc.BacktestEngine") as engine, patch("rotation_lab.rpc.RankingRegistry"), \
             patch("rotation_lab.rpc.OhlcvFetcher"), patch("rotation_lab.rpc.get_default_cache"):
            engine.return_value.run.return_value = result
            payload = _method_backtest({"config": {"mode": "single_asset", "singleAsset": {"timeframe": "4h"}}})
        self.assertIsInstance(payload["equityStrategy"][0]["time"], int)
        self.assertIsInstance(payload["strategies"][0]["trades"][0]["from"], int)
        self.assertEqual(payload["singleAsset"]["timeframe"], "4h")


class ExactMarketTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.cache = Cache(Path(folder.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())
        self.fetcher = OhlcvFetcher(self.cache)
        self.exchange = Mock(timeframes={"1h": "1h", "4h": "4h", "1d": "1d"})
        self.fetcher._ensure_exchange = Mock(return_value=self.exchange)
        markets = {pair: {"symbol": pair, "spot": True, "active": True} for pair in ("BTC/USD", "BTC/USDT")}
        self.fetcher._markets = {name: markets for name in ("coinbase", "kraken")}
        self.rows = [[int(ts.timestamp() * 1000), 100, 120, 90, 110, 10]
                     for ts in pd.date_range("2026-09-22", periods=3, tz="UTC")]
        self.exchange.fetch_ohlcv.side_effect = lambda pair, timeframe, since, limit: [row for row in self.rows if row[0] >= since]

    def test_exact_venue_quote_and_confirmed_cache_are_isolated(self):
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW):
            for exchange, pair in (("coinbase", "BTC/USD"), ("coinbase", "BTC/USDT"), ("kraken", "BTC/USD")):
                result = self.fetcher.get_pair_series(exchange, pair, dt.date(2026, 9, 22), NOW.date())
                self.assertEqual(len(result.frame), 2)
                self.assertEqual(result.coin_key, f"market:{exchange}:{pair}")
            self.assertEqual(self.cache._con.execute("SELECT COUNT(DISTINCT coin_key) FROM ohlcv_intraday").fetchone()[0], 3)
            self.exchange.fetch_ohlcv.reset_mock()
            self.fetcher.get_pair_series("coinbase", "BTC/USD", dt.date(2026, 9, 22), NOW.date())
            self.exchange.fetch_ohlcv.assert_not_called()

    def test_missing_pair_and_timeframe_fail_without_fallback(self):
        with self.assertRaisesRegex(ValueError, "not an available spot pair"):
            self.fetcher.get_pair_series("coinbase", "ETH/BTC", dt.date(2026, 1, 1), NOW.date())
        self.exchange.timeframes = {"1d": "1d"}
        with self.assertRaisesRegex(ValueError, "does not provide 4h"):
            self.fetcher.get_pair_series("coinbase", "BTC/USD", dt.date(2026, 1, 1), NOW.date(), "4h")
        self.exchange.fetch_ohlcv.assert_not_called()

    def test_pagination_stops_before_requesting_a_future_candle(self):
        self.exchange.fetch_ohlcv.side_effect = [self.rows, RuntimeError("start must not be in the future")]
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW):
            result = self.fetcher.get_pair_series("coinbase", "BTC/USD", dt.date(2026, 9, 22), NOW.date())
        self.assertEqual(len(result.frame), 2)
        self.assertEqual(self.exchange.fetch_ohlcv.call_count, 1)

    def test_pair_picker_excludes_derivatives_and_inactive_markets(self):
        self.fetcher._markets["coinbase"] = {"BTC/USD": {"spot": True}, "BTC/USD:USD": {"spot": False},
                                             "OLD/USD": {"spot": True, "active": False}}
        self.assertEqual(self.fetcher.pair_markets("coinbase"), ["BTC/USD"])

    def test_failed_history_page_is_reported_instead_of_a_partial_backtest(self):
        self.exchange.fetch_ohlcv.side_effect = [self.rows[:2], RuntimeError("Connection lost")]
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW):
            with self.assertRaisesRegex(RuntimeError, "complete 1d history.*Connection lost"):
                self.fetcher.get_pair_series("coinbase", "BTC/USD", dt.date(2026, 9, 22), NOW.date())
        self.assertEqual(self.cache._con.execute("SELECT COUNT(*) FROM ohlcv_intraday").fetchone()[0], 0)


if __name__ == "__main__":
    unittest.main()
