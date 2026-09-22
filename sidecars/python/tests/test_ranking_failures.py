"""Ranking outages must not turn into per-coin/per-date retry storms."""

import datetime as dt
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import requests

from rotation_lab.backtest.universe import build_universe_timeline
from rotation_lab.config import Cadence, RankingSource
from rotation_lab.data.cache import Cache, OhlcvRow, RankingRow
from rotation_lab.data.ranking.base import RankedCoin
from rotation_lab.data.ranking.local_reconstruct import LocalRankingUnavailable, LocalReconstructProvider
from rotation_lab.data.ranking.registry import RankingRegistry, ResolvedRanking


def response(status=200, payload=None, headers=None):
    result = requests.Response()
    result.status_code = status
    result.url = "https://api.coingecko.com/test"
    result.headers.update(headers or {})
    result._content = json.dumps(payload).encode()
    return result


class RankingFailureTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.cache = Cache(Path(self.directory.name) / "cache.sqlite")
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(lambda: self.cache._con.close() if self.cache._con else None)
        self.day = dt.datetime.now(dt.timezone.utc).date() - dt.timedelta(days=2)
        self.session = Mock(spec=requests.Session)
        self.provider = LocalReconstructProvider(self.cache, session=self.session, min_request_interval=0)
        self.cache.set_meta(self.provider.META_KEY_POOL, "bitcoin,ethereum")
        for cg_id, symbol in (("bitcoin", "BTC"), ("ethereum", "ETH")):
            self.cache.upsert_coin_map(cg_id=cg_id, symbol=symbol, name=cg_id)
        ts = dt.datetime.combine(self.day, dt.time(), dt.timezone.utc).timestamp() * 1000
        self.chart = {"market_caps": [[ts, 123.0]]}

    def test_chart_requests_only_the_public_year(self):
        self.session.get.return_value = response(payload=self.chart)
        self.assertEqual(self.provider._fetch_market_chart("bitcoin"), {self.day: 123.0})
        self.assertEqual(self.session.get.call_args.kwargs["params"], {"vs_currency": "usd", "days": 365})

    def test_access_and_rate_limits_abort_before_the_next_coin_without_sleep(self):
        for status in (401, 403, 429):
            with self.subTest(status=status), patch("rotation_lab.data.ranking.local_reconstruct.time.sleep") as sleep:
                self.session.get.reset_mock()
                self.session.get.return_value = response(status, headers={"Retry-After": "120"})
                with self.assertRaisesRegex(LocalRankingUnavailable, str(status)):
                    self.provider.get_top_n(self.day, 5)
                self.assertEqual(self.session.get.call_count, 1)
                sleep.assert_not_called()
                self.assertEqual(self.cache.get_ranking("local", self.day), [])
                self.assertEqual(self.provider._fetched_series, set())

    def test_rate_limit_with_http_date_is_not_parsed_as_seconds(self):
        self.session.get.return_value = response(429, headers={"Retry-After": "Mon, 14 Sep 2026 12:00:00 GMT"})
        with self.assertRaisesRegex(LocalRankingUnavailable, "HTTP 429"):
            self.provider.get_top_n(self.day, 5)
        self.assertEqual(self.session.get.call_count, 1)

    def test_network_retry_is_bounded_for_the_entire_build(self):
        self.session.get.side_effect = requests.Timeout("offline")
        with self.assertRaisesRegex(LocalRankingUnavailable, "offline"):
            self.provider.get_top_n(self.day, 5)
        self.assertEqual(self.session.get.call_count, 2)

    def test_server_outage_is_bounded(self):
        self.session.get.return_value = response(503)
        with self.assertRaisesRegex(LocalRankingUnavailable, "503"):
            self.provider.get_top_n(self.day, 5)
        self.assertEqual(self.session.get.call_count, 2)

    def test_transient_server_error_can_recover(self):
        self.session.get.side_effect = [response(503), response(payload=self.chart)]
        self.assertEqual(self.provider._fetch_market_chart("bitcoin"), {self.day: 123.0})
        self.assertEqual(self.session.get.call_count, 2)

    def test_invalid_payload_is_not_cached_as_missing_history(self):
        self.session.get.return_value = response(payload={"error": "upstream unavailable"})
        with self.assertRaisesRegex(LocalRankingUnavailable, "invalid market-cap data"):
            self.provider.get_top_n(self.day, 5)
        self.assertEqual(self.provider._fetched_series, set())
        self.assertEqual(self.cache.get_ranking("local", self.day), [])

    def test_failed_build_does_not_publish_partial_rankings_and_can_retry(self):
        self.session.get.side_effect = [response(payload=self.chart), response(401)]
        with self.assertRaises(LocalRankingUnavailable):
            self.provider.get_top_n(self.day, 5)
        self.assertEqual(self.cache.get_ranking("local", self.day), [])
        self.assertEqual(self.provider._fetched_series, {"bitcoin"})
        self.session.get.side_effect = [response(payload=self.chart)]
        coins = self.provider.get_top_n(self.day, 5)
        self.assertEqual({coin.symbol for coin in coins}, {"BTC", "ETH"})

    def test_unknown_coin_does_not_block_other_coins(self):
        self.session.get.side_effect = [response(404), response(payload=self.chart)]
        coins = self.provider.get_top_n(self.day, 5)
        self.assertEqual([coin.symbol for coin in coins], ["ETH"])

    def test_uncached_old_date_does_not_download_current_history(self):
        with self.assertRaisesRegex(LocalRankingUnavailable, "365 days"):
            self.provider.get_top_n(self.day - dt.timedelta(days=800), 5)
        self.session.get.assert_not_called()

    def test_cached_old_ranking_remains_usable_without_network(self):
        day = self.day - dt.timedelta(days=800)
        now = dt.datetime.now(dt.timezone.utc).replace(tzinfo=None)
        self.cache.upsert_ranking([RankingRow("local", day, 1, "bitcoin", "BTC", "Bitcoin", 123, 1, now)])
        self.assertEqual(self.provider.get_top_n(day, 5)[0].symbol, "BTC")
        self.session.get.assert_not_called()

    def test_cached_old_market_caps_can_be_rebuilt_without_network(self):
        day = self.day - dt.timedelta(days=800)
        self.cache.upsert_ohlcv([OhlcvRow("mcap:bitcoin", day, None, None, None, 123, None, "coingecko-mcap", False)])
        self.assertEqual(self.provider.get_top_n(day, 5)[0].symbol, "BTC")
        self.session.get.assert_not_called()

    def test_fallback_error_keeps_both_provider_causes(self):
        cmc = Mock(name="cmc")
        cmc.name = "cmc"
        cmc.get_top_n.side_effect = RuntimeError("snapshot not published")
        self.session.get.return_value = response(401)
        registry = RankingRegistry(self.cache, cmc=cmc, local=self.provider)
        with self.assertRaisesRegex(RuntimeError, "cmc: snapshot not published; local: .*401"):
            registry.get_top_n(self.day, 5)
        self.assertEqual(self.session.get.call_count, 1)

    def test_successful_cmc_does_not_contact_coingecko(self):
        cmc = Mock()
        cmc.name = "cmc"
        cmc.get_top_n.return_value = [RankedCoin(1, None, "BTC", "Bitcoin", 123, 1)]
        registry = RankingRegistry(self.cache, cmc=cmc, local=self.provider)
        self.assertEqual(registry.get_top_n(self.day, 5).provider, "cmc")
        self.session.get.assert_not_called()

    def test_explicit_cmc_failure_does_not_switch_provider(self):
        cmc = Mock()
        cmc.name = "cmc"
        cmc.get_top_n.side_effect = RuntimeError("snapshot not published")
        registry = RankingRegistry(self.cache, cmc=cmc, local=self.provider)
        with self.assertRaisesRegex(RuntimeError, "snapshot not published"):
            registry.get_top_n(self.day, 5, source=RankingSource.CMC)
        self.session.get.assert_not_called()

    def test_universe_outage_aborts_at_first_failure_and_reports_date_before_io(self):
        registry = Mock()
        events = []
        def fail(day, *_args, **_kwargs):
            self.assertEqual(events[-1], (0, 4, day))
            raise RuntimeError("HTTP 429")
        registry.get_top_n.side_effect = fail
        with self.assertRaisesRegex(RuntimeError, f"{self.day}.*429"):
            build_universe_timeline(registry, start=self.day, end=self.day + dt.timedelta(days=3),
                                    cadence=Cadence.DAILY, top_n=5, progress=lambda *args: events.append(args))
        self.assertEqual(registry.get_top_n.call_count, 1)

    def test_universe_does_not_treat_a_later_outage_as_a_cash_period(self):
        registry = Mock()
        coin = RankedCoin(1, None, "BTC", "Bitcoin", 123, 1)
        registry.get_top_n.side_effect = [ResolvedRanking("cmc", [coin]), RuntimeError("offline")]
        with self.assertRaisesRegex(RuntimeError, "offline"):
            build_universe_timeline(registry, start=self.day, end=self.day + dt.timedelta(days=3),
                                    cadence=Cadence.DAILY, top_n=5)
        self.assertEqual(registry.get_top_n.call_count, 2)

    def test_empty_universe_is_an_error(self):
        registry = Mock()
        registry.get_top_n.return_value = ResolvedRanking("local", [])
        with self.assertRaisesRegex(RuntimeError, "No coins resolved"):
            build_universe_timeline(registry, start=self.day, end=self.day, cadence=Cadence.DAILY, top_n=5)


if __name__ == "__main__":
    unittest.main()
