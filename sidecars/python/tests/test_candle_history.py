"""A coin's whole candle history is found: venues that answer nothing before the listing, renamed tickers, delisted markets.

Found 2026-11-08: TON was in the daily Top 10 on 368 days without a single saved candle. Coinbase answered nothing for early
days, the fetcher then took Coinbase's latest 300 candles as the listing, and the exchanges list TON only as GRAM since 2026."""

import datetime as dt
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import pandas as pd

from rotation_lab.data.cache import Cache, OhlcvRow, RankingRow
from rotation_lab.data.ohlcv import CoinRef, OhlcvFetcher

DAY = 86_400_000


def venue(first, last, close, cap=100):
    """A stand-in venue like Coinbase or OKX: pages of ``cap`` candles from ``since``, nothing for a window before the
    listing, and its latest page when asked without ``since``."""
    rows = [[int(pd.Timestamp(d, tz="UTC").timestamp() * 1000), close, close, close, close, 1]
            for d in pd.date_range(first, last)]

    def fetch_ohlcv(pair, timeframe="1d", since=None, limit=1000):
        n = min(limit, cap)
        if since is None:
            return rows[-n:]
        return [r for r in rows if since <= r[0] < since + n * DAY][:n]

    exchange = Mock()
    exchange.fetch_ohlcv.side_effect = fetch_ohlcv
    return exchange


def market(symbol, active=True):
    base, quote = symbol.split("/")
    return {"symbol": symbol, "base": base, "quote": quote, "spot": True, "swap": False, "active": active}


class CandleHistoryTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.cache = Cache(Path(directory.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())

    def fetcher(self, exchanges, markets=None, chain=None):
        fetcher = OhlcvFetcher(self.cache, min_request_interval=0, ccxt_exchanges=tuple(chain or exchanges))
        fetcher._ensure_exchange = Mock(side_effect=lambda name: exchanges.get(name))
        fetcher._markets = dict(markets or {})
        fetcher._coingecko_id = Mock(return_value=None)
        return fetcher

    def get(self, fetcher, symbol, start, end, now):
        self.cache.ensure_confirmed_ohlcv(f"sym:{symbol}", "1d")
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=now):
            return fetcher.get_series(CoinRef(None, symbol), start, end).frame

    def saved(self, symbol):
        return {r.day: (r.close, r.source) for r in self.cache.get_ohlcv_range(f"sym:{symbol}", dt.date(2000, 1, 1), dt.date(2100, 1, 1))}

    def test_a_venue_that_answers_nothing_before_the_listing_gives_its_real_first_candle(self):
        okx = venue("2021-03-10", "2022-01-04", 5.0)
        fetcher = self.fetcher({"okx": okx})
        fetcher._resolve_pairs = Mock(return_value=[("okx", "AAA/USDT")])
        self.get(fetcher, "AAA", dt.date(2019, 1, 1), dt.date(2021, 12, 31), dt.datetime(2022, 1, 5, 12, tzinfo=dt.timezone.utc))
        days = sorted(self.saved("AAA"))
        # The listing, not the first day of the venue's latest page (2021-09-27).
        self.assertEqual(days[0], dt.date(2021, 3, 10))
        self.assertEqual(days[-1], dt.date(2021, 12, 31))
        self.assertEqual(len(days), len(pd.date_range("2021-03-10", "2021-12-31")))
        self.assertEqual(self.cache.get_meta("ohlcv_first_v2:sym:AAA"), "2021-03-10")
        self.assertLess(okx.fetch_ohlcv.call_count, 30)  # a bisection, not a day-by-day walk

    def test_an_old_first_candle_note_no_longer_hides_history_and_saved_days_stay(self):
        okx = venue("2021-03-10", "2022-01-04", 5.0)
        self.cache.upsert_ohlcv(OhlcvRow("sym:AAA", d.date(), 7, 7, 7, 7.0, 1, "ccxt:okx:AAA/USDT", False)
                                for d in pd.date_range("2021-09-27", "2021-12-31"))
        self.cache.set_meta("ohlcv_first:sym:AAA", "2021-09-27")  # the old, wrong note
        fetcher = self.fetcher({"okx": okx})
        fetcher._resolve_pairs = Mock(return_value=[("okx", "AAA/USDT")])
        now = dt.datetime(2022, 1, 1, 12, tzinfo=dt.timezone.utc)
        self.get(fetcher, "AAA", dt.date(2021, 1, 1), dt.date(2021, 12, 31), now)
        saved = self.saved("AAA")
        self.assertEqual(min(saved), dt.date(2021, 3, 10))
        self.assertEqual(saved[dt.date(2021, 3, 10)], (5.0, "ccxt:okx:AAA/USDT"))
        self.assertEqual(saved[dt.date(2021, 10, 1)], (7.0, "ccxt:okx:AAA/USDT"))  # never rewritten
        # The new note holds the real listing, so the next run asks nothing for the days before it.
        calls = okx.fetch_ohlcv.call_count
        self.get(fetcher, "AAA", dt.date(2021, 1, 1), dt.date(2021, 12, 31), now)
        self.assertEqual(okx.fetch_ohlcv.call_count, calls)

    def rankings(self, symbol, first, last, price, cap, rank=10):
        self.cache.upsert_ranking([RankingRow("cmc", d.date(), rank, None, symbol, symbol, cap, price, dt.datetime(2024, 1, 1))
                                   for d in pd.date_range(first, last)])

    def test_renames_are_read_from_the_rankings_by_price_and_market_cap(self):
        self.rankings("TON", "2022-01-01", "2023-06-16", 2.00, 7.00e9, rank=9)
        self.rankings("GRAM", "2023-06-17", "2023-12-31", 2.01, 7.03e9, rank=9)
        self.rankings("EOS", "2022-01-01", "2022-05-27", 0.80, 1.20e9, rank=30)
        self.rankings("A", "2022-05-28", "2023-12-31", 0.79, 1.19e9, rank=31)
        # Same price on the next day, but three times the market cap: another coin, not a rename.
        self.rankings("ETP", "2022-01-01", "2022-03-01", 1.00, 1.0e8, rank=90)
        self.rankings("CRPT", "2022-03-02", "2023-12-31", 1.00, 3.0e8, rank=91)
        # Still listed at the end: nothing replaced it, even if another ticker starts at its price.
        self.rankings("BTC", "2022-01-01", "2023-12-31", 30000.0, 6.0e11, rank=1)
        renames = self.cache.ticker_renames()
        self.assertEqual(renames["TON"], {"GRAM"})
        self.assertEqual(renames["GRAM"], {"TON"})
        self.assertEqual(renames["EOS"], {"A"})
        for symbol in ("ETP", "CRPT", "BTC"):
            self.assertNotIn(symbol, renames)

    def test_a_renamed_coin_finds_its_history_under_the_new_ticker(self):
        self.rankings("TON", "2022-01-01", "2023-06-16", 2.00, 7.00e9, rank=9)
        self.rankings("GRAM", "2023-06-17", "2023-12-31", 2.01, 7.03e9, rank=9)
        exchanges = {"binance": venue("2021-08-01", "2023-06-15", 3.0, cap=1000),  # TON/USDT, inactive since the rename
                     "bybit": venue("2021-06-01", "2024-01-01", 2.0),  # lists only GRAM, with the whole history
                     "coinbase": venue("2023-01-01", "2024-01-01", 2.1, cap=300)}  # TON/USDC, listed late
        markets = {"binance": {"TON/USDT": market("TON/USDT", active=False)},
                   "bybit": {"GRAM/USDT": market("GRAM/USDT")},
                   "coinbase": {"TON/USDC": market("TON/USDC")}}
        fetcher = self.fetcher(exchanges, markets, chain=("binance", "bybit", "coinbase"))
        self.assertEqual(list(fetcher._resolve_pairs(CoinRef(None, "TON"))),
                         [("coinbase", "TON/USDC"), ("bybit", "GRAM/USDT"), ("binance", "TON/USDT")])
        self.get(fetcher, "TON", dt.date(2021, 6, 1), dt.date(2023, 12, 31), dt.datetime(2024, 1, 2, 12, tzinfo=dt.timezone.utc))
        saved = self.saved("TON")
        self.assertEqual(min(saved), dt.date(2021, 6, 1))
        self.assertEqual(len(saved), len(pd.date_range("2021-06-01", "2023-12-31")))
        self.assertEqual(saved[dt.date(2022, 6, 1)], (2.0, "ccxt:bybit:GRAM/USDT"))
        # The delisted market is history only: never asked while an active one spans the window.
        exchanges["binance"].fetch_ohlcv.assert_not_called()

    def test_days_before_the_chosen_venue_come_from_the_others(self):
        # Like XMR: still traded on KuCoin, but only since 2020-02-03 there; Binance delisted it with years of history.
        exchanges = {"binance": venue("2017-11-01", "2021-06-30", 3.0, cap=1000), "kucoin": venue("2020-02-03", "2022-01-04", 4.0)}
        markets = {"binance": {"XMR/USDT": market("XMR/USDT", active=False)}, "kucoin": {"XMR/USDT": market("XMR/USDT")}}
        fetcher = self.fetcher(exchanges, markets, chain=("binance", "kucoin"))
        self.get(fetcher, "XMR", dt.date(2019, 1, 1), dt.date(2021, 12, 31), dt.datetime(2022, 1, 5, 12, tzinfo=dt.timezone.utc))
        saved = self.saved("XMR")
        self.assertEqual(len(saved), len(pd.date_range("2019-01-01", "2021-12-31")))
        self.assertEqual(saved[dt.date(2020, 2, 2)], (3.0, "ccxt:binance:XMR/USDT"))
        # From its first day on, the venue that reaches today keeps every day.
        self.assertEqual(saved[dt.date(2020, 2, 3)], (4.0, "ccxt:kucoin:XMR/USDT"))
        self.assertEqual(saved[dt.date(2021, 6, 1)], (4.0, "ccxt:kucoin:XMR/USDT"))

    def test_a_perpetual_listed_before_the_spot_book_gives_no_spot_days(self):
        # Like FIL in 2020: futures traded from July, spot only from 2020-10-15.
        exchanges = {"binance": venue("2020-10-15", "2022-01-04", 30.0, cap=1000), "okx": venue("2020-07-22", "2022-01-04", 31.0)}
        markets = {"binance": {"FIL/USDT": market("FIL/USDT")},
                   "okx": {"FIL/USDT:USDT": {**market("FIL/USDT"), "symbol": "FIL/USDT:USDT", "spot": False, "swap": True}}}
        fetcher = self.fetcher(exchanges, markets, chain=("binance", "okx"))
        self.get(fetcher, "FIL", dt.date(2020, 1, 1), dt.date(2021, 12, 31), dt.datetime(2022, 1, 5, 12, tzinfo=dt.timezone.utc))
        saved = self.saved("FIL")
        self.assertEqual(min(saved), dt.date(2020, 10, 15))
        self.assertEqual({source for _, source in saved.values()}, {"ccxt:binance:FIL/USDT"})

    def test_a_coin_only_on_a_delisted_market_still_gets_its_history(self):
        exchanges = {"binance": venue("2019-01-01", "2021-06-30", 4.0, cap=1000)}
        fetcher = self.fetcher(exchanges, {"binance": {"MMM/USDT": market("MMM/USDT", active=False)}})
        self.get(fetcher, "MMM", dt.date(2020, 1, 1), dt.date(2021, 12, 31), dt.datetime(2022, 1, 5, 12, tzinfo=dt.timezone.utc))
        saved = self.saved("MMM")
        self.assertEqual((min(saved), max(saved)), (dt.date(2020, 1, 1), dt.date(2021, 6, 30)))
        self.assertEqual(saved[dt.date(2021, 6, 30)], (4.0, "ccxt:binance:MMM/USDT"))


if __name__ == "__main__":
    unittest.main()
