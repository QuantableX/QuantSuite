"""Daily CMC ranking snapshots: date mapping, the archive and its backfill job.

No test touches CoinMarketCap: a fake session answers from the requested date
and counts every request.
"""

import datetime as dt
import hashlib
import json
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import Mock

import requests

from rotation_lab.backtest.engine import _require_daily_archive
from rotation_lab.backtest.universe import build_universe_timeline, daily_snapshot_days
from rotation_lab.config import Cadence, RankingResolution, RankingSource, RunConfig
from rotation_lab.data.cache import Cache
from rotation_lab.data.ranking import archive
from rotation_lab.data.ranking.cmc_snapshot import (
    CmcSnapshotProvider,
    latest_complete_day,
    snapshot_date,
)
from rotation_lab.data.ranking.registry import RankingRegistry
from rotation_lab.rpc import _build_config

SUN = dt.date(2021, 5, 9)
WED = dt.date(2021, 5, 12)
THU = dt.date(2021, 5, 13)


def day_tag(day: dt.date) -> str:
    """A coin whose symbol names the snapshot's day, ranked first."""
    return f"D{day:%y%m%d}"


def payload(day: dt.date) -> dict:
    coins = [(1, day_tag(day)), (2, "BTC"), (3, "USDT"), (4, "ETH")]
    return {"data": [
        {"cmcRank": rank, "symbol": sym, "name": sym, "slug": sym.lower(),
         "quotes": [{"name": "USD", "price": 1.0, "marketCap": 1e9 / rank}]}
        for rank, sym in coins
    ]}


def response(status=200, body=None):
    result = requests.Response()
    result.status_code = status
    result.url = "https://api.coinmarketcap.com/test"
    result._content = json.dumps(body if body is not None else {}).encode()
    return result


class FakeCmc:
    """Answers the data-api from the ``date`` param; HTML pages are 404."""

    def __init__(self, empty=(), failing=()):
        self.empty = set(empty)
        self.failing = set(failing)
        self.dates: list[dt.date] = []
        self.lock = threading.Lock()

    def get(self, url, params=None, headers=None, timeout=None):
        if "data-api" not in url:
            return response(404)
        day = dt.date.fromisoformat(params["date"])
        with self.lock:
            self.dates.append(day)
        if day in self.failing:
            return response(503)
        if day in self.empty:
            return response(body={"data": []})
        return response(body=payload(day))


class DailyRankingTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.cache = Cache(Path(self.directory.name) / "cache.sqlite")
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(lambda: self.cache._con.close() if self.cache._con else None)
        self.fake = FakeCmc()
        self.provider = self.make_provider(self.fake)

    def make_provider(self, fake):
        session = Mock(spec=requests.Session)
        session.get.side_effect = fake.get
        return CmcSnapshotProvider(self.cache, session=session, min_request_interval=0)

    def symbols(self, coins):
        return [c.symbol for c in coins]

    def rows_hash(self, day):
        rows = self.cache.get_ranking("cmc", day)
        return hashlib.sha256(repr([(r.rank, r.symbol, r.market_cap, r.fetched_at) for r in rows]).encode()).hexdigest()

    # ── date mapping ────────────────────────────────────────────────────

    def test_weekly_reads_the_sunday_and_daily_the_previous_day(self):
        self.assertEqual(snapshot_date(WED, RankingResolution.WEEKLY), SUN)
        self.assertEqual(snapshot_date(SUN, RankingResolution.WEEKLY), SUN)
        self.assertEqual(snapshot_date(THU, RankingResolution.DAILY), WED)
        self.assertEqual(snapshot_date(SUN, RankingResolution.DAILY), dt.date(2021, 5, 8))

    def test_config_without_the_field_stays_weekly(self):
        self.assertEqual(_build_config({}).ranking_resolution, RankingResolution.WEEKLY)
        self.assertEqual(RunConfig().ranking_resolution, RankingResolution.WEEKLY)
        self.assertEqual(_build_config({"rankingResolution": "daily"}).ranking_resolution, RankingResolution.DAILY)

    # ── provider ─────────────────────────────────────────────────────────

    def test_daily_fetches_the_previous_day_once(self):
        coins = self.provider.get_top_n(THU, 3, resolution=RankingResolution.DAILY)
        self.assertEqual(self.symbols(coins), [day_tag(WED), "BTC", "ETH"])  # USDT excluded
        self.assertEqual(self.fake.dates, [WED])
        self.provider.get_top_n(THU, 3, resolution=RankingResolution.DAILY)
        self.assertEqual(self.fake.dates, [WED], "a past day is never fetched twice")

    def test_weekly_reads_the_stored_sunday_next_to_daily_rows(self):
        for day in (SUN, WED):
            self.provider.fetch_day(day)
        before = self.rows_hash(SUN)
        self.fake.dates.clear()
        coins = self.provider.get_top_n(THU, 3)
        self.assertEqual(self.symbols(coins)[0], day_tag(SUN))
        self.assertEqual(self.fake.dates, [])
        self.assertEqual(self.rows_hash(SUN), before)

    def test_daily_never_asks_for_today(self):
        today = dt.datetime.now(dt.timezone.utc).date()
        self.provider.get_top_n(today + dt.timedelta(days=1), 3, resolution=RankingResolution.DAILY)
        self.assertEqual(self.fake.dates, [latest_complete_day()])

    def test_an_empty_answer_is_not_stored(self):
        provider = self.make_provider(FakeCmc(empty={WED}))
        with self.assertRaises(Exception):
            provider.get_top_n(THU, 3, resolution=RankingResolution.DAILY)
        self.assertEqual(self.cache.get_ranking("cmc", WED), [])

    def test_fetch_day_never_rewrites_a_stored_day(self):
        self.assertTrue(self.provider.fetch_day(WED))
        before = self.rows_hash(WED)
        self.assertEqual(self.provider.fetch_day(WED), 0)
        self.assertEqual(self.fake.dates, [WED])
        self.assertEqual(self.rows_hash(WED), before)

    # ── universe: no look-ahead ──────────────────────────────────────────

    def universe(self, cadence, resolution, start, end):
        registry = RankingRegistry(self.cache, cmc=self.provider, local=Mock(name="local"))
        return build_universe_timeline(
            registry, start=start, end=end, cadence=cadence, top_n=2,
            source=RankingSource.CMC, resolution=resolution)

    def first_symbols(self, timeline):
        return {s.on_date: s.coins[0].symbol for s in timeline.snapshots}

    def test_daily_universe_uses_the_previous_day_for_every_cadence(self):
        for cadence in (Cadence.DAILY, Cadence.HOUR_4, Cadence.HOUR_1):
            with self.subTest(cadence=cadence):
                timeline = self.universe(cadence, RankingResolution.DAILY, WED, THU)
                self.assertEqual(self.first_symbols(timeline),
                                 {WED: day_tag(dt.date(2021, 5, 11)), THU: day_tag(WED)})

    def test_weekly_intraday_sunday_takes_the_sunday_before(self):
        prev_sun = SUN - dt.timedelta(days=7)
        mon = SUN + dt.timedelta(days=1)
        intraday = self.first_symbols(self.universe(Cadence.HOUR_4, RankingResolution.WEEKLY, SUN, mon))
        self.assertEqual(intraday, {SUN: day_tag(prev_sun), mon: day_tag(SUN)})
        # Daily cadence keeps its weekly behaviour: Sunday's close sees Sunday.
        daily = self.first_symbols(self.universe(Cadence.DAILY, RankingResolution.WEEKLY, SUN, mon))
        self.assertEqual(daily, {SUN: day_tag(SUN), mon: day_tag(SUN)})

    # ── engine guard ─────────────────────────────────────────────────────

    def test_a_large_archive_gap_is_refused_before_any_fetch(self):
        registry = Mock(cache=self.cache)
        config = RunConfig(start_date=dt.date(2021, 1, 1), end_date=dt.date(2021, 6, 30),
                           ranking_resolution=RankingResolution.DAILY)
        with self.assertRaisesRegex(ValueError, r"missing 181 days .* Ranking snapshots to Weekly"):
            _require_daily_archive(registry, config)
        self.assertEqual(self.fake.dates, [])
        small = RunConfig(start_date=dt.date(2021, 6, 1), end_date=dt.date(2021, 6, 30),
                          ranking_resolution=RankingResolution.DAILY)
        _require_daily_archive(registry, small)  # 30 days: fetched on demand

    def test_snapshot_days_follow_the_cadence(self):
        weekly = daily_snapshot_days(dt.date(2021, 5, 3), dt.date(2021, 5, 31), Cadence.WEEKLY)
        self.assertEqual(weekly[0], dt.date(2021, 5, 2))
        self.assertLess(len(weekly), 10)
        self.assertEqual(len(daily_snapshot_days(WED, THU, Cadence.HOUR_1)), 2)

    # ── backfill job ─────────────────────────────────────────────────────

    def run_job(self, fake, start, end, cancel_after=None):
        events = []
        job = archive.DailyArchiveJob(lambda: self.make_provider(fake), events.append)
        if cancel_after is not None:
            original = fake.get

            def get(url, params=None, headers=None, timeout=None):
                if len(fake.dates) >= cancel_after:
                    job._cancel.set()
                return original(url, params=params, headers=headers, timeout=timeout)

            fake.get = get
        job.start(start, end)
        job._thread.join(10)
        return job, events

    def test_backfill_fetches_only_missing_days_and_resumes_after_cancel(self):
        start, end = dt.date(2021, 5, 1), dt.date(2021, 5, 20)
        self.provider.fetch_day(SUN)
        sunday = self.rows_hash(SUN)

        first = FakeCmc()
        job, events = self.run_job(first, start, end, cancel_after=5)
        self.assertTrue(events[-1]["cancelled"])
        self.assertLess(len(first.dates), 19)

        second = FakeCmc()
        job, events = self.run_job(second, start, end)
        self.assertFalse(events[-1]["running"])
        self.assertIsNone(events[-1]["error"])
        self.assertEqual(sorted(set(first.dates) | set(second.dates)),
                         [d for d in archive.day_range(start, end) if d != SUN])
        self.assertFalse(set(first.dates) & set(second.dates), "no day is fetched twice")
        self.assertEqual(set(self.cache.ranking_dates("cmc")), set(archive.day_range(start, end)))
        self.assertEqual(self.rows_hash(SUN), sunday)
        cov = archive.coverage(self.cache, start, end)
        self.assertEqual((cov["days"], cov["missingDays"]), (20, 0))

    def test_backfill_stops_when_cmc_stops_answering(self):
        start = dt.date(2021, 5, 1)
        days = archive.day_range(start, dt.date(2021, 5, 20))
        fake = FakeCmc(failing=set(days[2:]))
        job, events = self.run_job(fake, start, days[-1])
        last = events[-1]
        self.assertFalse(last["running"])
        self.assertRegex(last["error"], "5 days in a row")
        self.assertEqual(len(fake.dates), 7)
        self.assertEqual(set(self.cache.ranking_dates("cmc")), set(days[:2]))


if __name__ == "__main__":
    unittest.main()
