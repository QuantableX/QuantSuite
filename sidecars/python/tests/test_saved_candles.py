"""Saved candles stay as saved: a coin keeps its exchange, and no saved day is rewritten."""

import datetime as dt
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import pandas as pd

from rotation_lab.data.cache import Cache, OhlcvRow
from rotation_lab.data.ohlcv import CoinRef, OhlcvFetcher


NOW = dt.datetime(2020, 1, 20, 12, tzinfo=dt.timezone.utc)
REF = CoinRef(None, "AAA")


def candles(first, last, close):
    return [[int(pd.Timestamp(day, tz="UTC").timestamp() * 1000), close, close, close, close, 1]
            for day in pd.date_range(first, last)]


class SavedCandleTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.cache = Cache(Path(directory.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())
        # A saved cache has long passed the one-time cleanup of legacy partial tails.
        self.cache.ensure_confirmed_ohlcv("sym:AAA", "1d")
        self.fetcher = OhlcvFetcher(self.cache, min_request_interval=0, ccxt_exchanges=("gate", "binance"))
        # Gate reaches back further (it would span a long warm-up); Binance lists the coin later.
        self.raw = {"gate": candles("2015-01-01", "2020-01-19", 200.0),
                    "binance": candles("2017-08-17", "2020-01-19", 100.0)}
        self.exchanges = {}
        for name in self.raw:
            exchange = Mock()
            exchange.fetch_ohlcv.side_effect = lambda pair, timeframe, since=0, limit=1000, name=name: [
                row for row in self.raw[name] if row[0] >= since][:limit]
            self.exchanges[name] = exchange
        self.fetcher._ensure_exchange = Mock(side_effect=lambda name: self.exchanges[name])
        self.fetcher._resolve_pairs = Mock(return_value=[("gate", "AAA/USDT"), ("binance", "AAA/USDT")])
        self.fetcher._coingecko_id = Mock(return_value=None)

    def saved(self, start, end):
        return {r.day: (r.close, r.source) for r in self.cache.get_ohlcv_range("sym:AAA", start, end)}

    def fetch(self, start, end):
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=NOW):
            return self.fetcher.get_series(REF, start, end).frame

    def test_a_long_warm_up_keeps_saved_days_and_their_exchange(self):
        self.cache.upsert_ohlcv(OhlcvRow("sym:AAA", d.date(), 99, 99, 99, 99.0, 1, "ccxt:binance:AAA/USDT", False)
                                for d in pd.date_range("2018-01-01", "2019-12-31"))
        before = self.saved(dt.date(2018, 1, 1), dt.date(2019, 12, 31))
        frame = self.fetch(dt.date(2015, 1, 1), dt.date(2020, 1, 10))
        after = self.saved(dt.date(2015, 1, 1), dt.date(2020, 1, 10))
        # Saved days are untouched.
        self.assertEqual({d: after[d] for d in before}, before)
        # New days after the saved ones come from the coin's own exchange, not the one spanning the window.
        self.assertEqual(after[dt.date(2020, 1, 10)], (100.0, "ccxt:binance:AAA/USDT"))
        self.assertEqual(after[dt.date(2017, 8, 17)], (100.0, "ccxt:binance:AAA/USDT"))
        # Only days its exchange never had are filled from the chain.
        self.assertEqual(after[dt.date(2015, 1, 1)], (200.0, "ccxt:gate:AAA/USDT"))
        self.assertEqual(after[dt.date(2017, 8, 16)], (200.0, "ccxt:gate:AAA/USDT"))
        self.assertEqual(len(frame), len(pd.date_range("2015-01-01", "2020-01-10")))
        self.assertEqual(frame.loc["2019-06-01", "close"], 99.0)

    def test_new_days_need_no_other_exchange_when_the_coin_s_own_covers_them(self):
        self.cache.upsert_ohlcv(OhlcvRow("sym:AAA", d.date(), 99, 99, 99, 99.0, 1, "ccxt:binance:AAA/USDT", False)
                                for d in pd.date_range("2018-01-01", "2020-01-05"))
        self.fetch(dt.date(2019, 1, 1), dt.date(2020, 1, 10))
        self.exchanges["gate"].fetch_ohlcv.assert_not_called()
        self.assertEqual(self.saved(dt.date(2020, 1, 6), dt.date(2020, 1, 10))[dt.date(2020, 1, 8)],
                         (100.0, "ccxt:binance:AAA/USDT"))

    def test_the_exchange_chain_is_binance_first_and_never_gate_or_mexc(self):
        from rotation_lab.data.ohlcv import _EXCHANGE_CHAIN
        self.assertEqual(_EXCHANGE_CHAIN, ("binance", "bybit", "okx", "coinbase", "kucoin"))
        names = OhlcvFetcher(self.cache, min_request_interval=0)._exchange_names
        self.assertEqual(names[0], "binance")
        self.assertFalse({"gate", "mexc"} & set(names))

    def test_a_coin_without_saved_candles_still_takes_the_best_exchange(self):
        self.fetch(dt.date(2016, 1, 1), dt.date(2020, 1, 10))
        self.assertEqual(self.saved(dt.date(2016, 1, 1), dt.date(2016, 1, 1))[dt.date(2016, 1, 1)],
                         (200.0, "ccxt:gate:AAA/USDT"))


if __name__ == "__main__":
    unittest.main()
