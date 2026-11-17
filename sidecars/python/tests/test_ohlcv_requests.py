"""The downloader does not ask the exchanges again for days none of them had (card 6ca6608d).

Measured 2026-11-17: a research run's preparation on complete data still made 300 + 662 requests, every run,
for about a dozen coins: holes no venue has (BCH around its 2018 fork, LUNA's 2022 collapse), a coin no venue
lists, a delisted coin's tail. Each such day is now noted when it was asked for and not delivered; a noted day
is not asked again for 30 days. Days younger than a week and loads with a failed request are never noted."""

import datetime as dt
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import pandas as pd

from rotation_lab.data.cache import Cache
from rotation_lab.data.ohlcv import CoinRef, OhlcvFetcher

DAY = 86_400_000
NOW = dt.datetime(2022, 3, 1, 12, tzinfo=dt.timezone.utc)


def venue(days, close=5.0, fail=False):
    """A stand-in exchange with candles on ``days`` only; ``fail``: every request raises."""
    rows = [[int(pd.Timestamp(d, tz="UTC").timestamp() * 1000), close, close, close, close, 1] for d in days]

    def fetch_ohlcv(pair, timeframe="1d", since=None, limit=1000):
        if fail:
            raise TimeoutError("venue did not answer")
        if since is None:
            return rows[-limit:]
        return [r for r in rows if r[0] >= since][:limit]

    exchange = Mock()
    exchange.fetch_ohlcv.side_effect = fetch_ohlcv
    return exchange


class AbsentDayTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.cache = Cache(Path(directory.name) / "cache.sqlite")
        self.addCleanup(lambda: self.cache._con.close())

    def fetcher(self, exchange):
        fetcher = OhlcvFetcher(self.cache, min_request_interval=0, ccxt_exchanges=("okx",))
        fetcher._ensure_exchange = Mock(return_value=exchange)
        fetcher._resolve_pairs = Mock(return_value=[("okx", "AAA/USDT")] if exchange is not None else [])
        fetcher._coingecko_id = Mock(return_value=None)
        fetcher._populate = Mock(wraps=fetcher._populate)
        return fetcher

    def get(self, fetcher, start, end, now=NOW):
        self.cache.ensure_confirmed_ohlcv("sym:AAA", "1d")
        with patch("rotation_lab.data.ohlcv.utc_now", return_value=now):
            frame = fetcher.get_series(CoinRef(None, "AAA"), start, end).frame
        return list(frame.index)

    def test_a_hole_no_exchange_has_is_asked_for_once(self):
        days = [d for d in pd.date_range("2021-01-01", "2021-12-31") if not dt.date(2021, 6, 10) <= d.date() <= dt.date(2021, 6, 14)]
        exchange = venue(days)
        fetcher = self.fetcher(exchange)
        first = self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31))
        asked = exchange.fetch_ohlcv.call_count
        self.assertGreater(asked, 0)
        self.assertEqual(len(first), 365 - 5)
        # The next run: the five days are still missing, nobody is asked, the same candles come back.
        again = self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31))
        self.assertEqual(again, first)
        self.assertEqual(exchange.fetch_ohlcv.call_count, asked)
        self.assertEqual(fetcher._populate.call_count, 1)
        # A new day after the noted span is asked for as before.
        self.get(fetcher, dt.date(2021, 1, 1), dt.date(2022, 1, 31))
        self.assertEqual(fetcher._populate.call_count, 2)

    def test_a_coin_no_exchange_lists_is_asked_again_after_30_days(self):
        fetcher = self.fetcher(None)
        self.assertEqual(self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31)), [])
        self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31))
        self.assertEqual(fetcher._populate.call_count, 1)
        self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31), NOW + dt.timedelta(days=31))
        self.assertEqual(fetcher._populate.call_count, 2)

    def test_days_younger_than_a_week_are_always_asked_again(self):
        # The venue has nothing for the last three days before yesterday yet.
        exchange = venue(pd.date_range("2022-01-01", "2022-02-25"))
        fetcher = self.fetcher(exchange)
        self.get(fetcher, dt.date(2022, 1, 1), dt.date(2022, 2, 28))
        self.get(fetcher, dt.date(2022, 1, 1), dt.date(2022, 2, 28))
        self.assertEqual(fetcher._populate.call_count, 2)

    def test_nothing_is_noted_when_a_request_failed(self):
        exchange = venue(pd.date_range("2021-01-01", "2021-12-31"), fail=True)
        fetcher = self.fetcher(exchange)
        self.assertEqual(self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31)), [])
        self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31))
        self.assertEqual(fetcher._populate.call_count, 2)
        self.assertIsNone(self.cache.get_meta("ohlcv_absent_v1:sym:AAA"))

    def test_a_cache_that_cannot_be_written_keeps_no_note_and_still_answers(self):
        exchange = venue([d for d in pd.date_range("2021-01-01", "2021-12-31") if d.day != 15])
        fetcher = self.fetcher(exchange)
        with patch.object(self.cache, "set_meta", side_effect=OSError("read-only")):
            got = self.get(fetcher, dt.date(2021, 1, 1), dt.date(2021, 12, 31))
        self.assertEqual(len(got), 365 - 12)


if __name__ == "__main__":
    unittest.main()
