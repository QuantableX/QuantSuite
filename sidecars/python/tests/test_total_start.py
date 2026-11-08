"""TOTAL and the market filter are the same on every day whatever day a run starts, and today's live
signal equals a backtest of today. Before the fix, bars before a run's start took the start's own weights,
and the fixed breakout started fresh at the run's start."""
import datetime as dt
import unittest
from dataclasses import dataclass
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine, _market_start
from rotation_lab.backtest.market import breakout_warmup, market_gate, market_index
from rotation_lab.backtest.universe import UniverseSnapshot, market_warmup_snapshots
from rotation_lab.config import Cadence, IndicatorConfig, RankingResolution, RunConfig, TotalBreakoutConfig
from rotation_lab.data.ranking.base import RankedCoin
from rotation_lab.data.ranking.registry import ResolvedRanking

SYMBOLS = ("AAA", "BBB", "CCC")
FIRST = pd.Timestamp("2021-01-01")


def _frame(seed: int, n: int = 1400, drift: float = 0.0008) -> pd.DataFrame:
    rng = np.random.default_rng(seed)
    close = np.exp(np.cumsum(rng.normal(drift, 0.035, n))) * 100
    index = pd.date_range(FIRST, periods=n, freq="D")
    return pd.DataFrame({"open": close * (1 + rng.normal(0, 0.004, n)), "high": close * 1.02,
                         "low": close * 0.98, "close": close, "volume": 1.0}, index=index)


FRAMES = {sym: _frame(i + 7) for i, sym in enumerate(SYMBOLS)}


def _caps(day: dt.date) -> dict[str, float]:
    """Weights that move with the day, so a bar's own snapshot matters."""
    t = (day - FIRST.date()).days
    return {"AAA": 1e9 * np.exp(t / 250), "BBB": 1e9 * (1.5 + np.sin(t / 90)), "CCC": 8e9 * np.exp(-t / 250)}


class FakeRegistry:
    """Every day ranked, from the 'network' and from the 'cache' alike; ``cached_from`` limits the cache."""

    def __init__(self, cached_from: dt.date | None = None):
        self.cached_from = cached_from
        self.network_days: list[dt.date] = []

    def _coins(self, day):
        caps = _caps(day)
        order = sorted(SYMBOLS, key=lambda s: -caps[s])
        return [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=caps[s], price=1.0) for i, s in enumerate(order)]

    def get_top_n(self, on_date, n, **_kw):
        self.network_days.append(on_date)
        return ResolvedRanking(provider="test", coins=self._coins(on_date)[:n])

    def cached_top_n(self, on_date, n, **_kw):
        if self.cached_from is not None and on_date < self.cached_from:
            return None
        return ResolvedRanking(provider="test", coins=self._coins(on_date)[:n])


@dataclass
class _Series:
    frame: pd.DataFrame


class _Ohlcv:
    def get_series(self, ref, start, end, timeframe):
        f = FRAMES[ref.symbol]
        return _Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])


def _config(start, market, end=dt.date(2024, 9, 30)):
    return RunConfig(top_n=3, start_date=start, end_date=end, indicator=IndicatorConfig(trend="ema_cross"),
                     market_filter=True, market_indicator=market)


def _run(start, market, registry=None, end=dt.date(2024, 9, 30)):
    return BacktestEngine(registry=registry or FakeRegistry(), ohlcv=_Ohlcv()).run(_config(start, market, end))


FILTERS = {"fixed breakout": TotalBreakoutConfig(trend_length=60, entry_length=5, exit_length=15),
           "indicator": IndicatorConfig(trend="defensive_trend")}


def _seen_total(start, market):
    """The TOTAL the engine hands its market filter, as daily close returns."""
    import rotation_lab.backtest.engine as engine
    seen = []
    real = engine.market_gate

    def capture(total, cfg):
        seen.append(total["close"].pct_change())
        return real(total, cfg)

    with patch.object(engine, "market_gate", side_effect=capture):
        _run(start, market)
    return seen[0]


class WarmupSnapshotTests(unittest.TestCase):
    def test_each_day_before_the_start_gets_its_own_cached_snapshot_and_never_the_network(self):
        registry = FakeRegistry(cached_from=dt.date(2023, 3, 1))
        snaps = market_warmup_snapshots(registry, warm_start=dt.date(2023, 1, 1), start=dt.date(2023, 6, 1),
                                        cadence=Cadence("daily"), top_n=3, resolution=RankingResolution.DAILY)
        self.assertEqual(snaps[0].on_date, dt.date(2023, 3, 1))
        self.assertEqual(snaps[-1].on_date, dt.date(2023, 5, 31))
        self.assertEqual(len(snaps), (dt.date(2023, 6, 1) - dt.date(2023, 3, 1)).days)
        self.assertEqual(registry.network_days, [])

    def test_a_registry_without_a_cache_lookup_adds_nothing(self):
        self.assertEqual(market_warmup_snapshots(object(), warm_start=dt.date(2023, 1, 1), start=dt.date(2023, 2, 1),
                                                 cadence=Cadence("daily"), top_n=3), [])

    def test_total_returns_on_a_day_do_not_depend_on_the_start(self):
        registry = FakeRegistry()

        def total(start):
            snaps = market_warmup_snapshots(registry, warm_start=dt.date(2021, 6, 1), start=start, cadence=Cadence("daily"), top_n=3)
            snaps += [UniverseSnapshot(d.date(), "test", registry._coins(d.date())) for d in pd.date_range(start, "2024-09-30")]
            return market_index(snaps, FRAMES)["close"].pct_change()

        early, late = total(dt.date(2022, 1, 1)), total(dt.date(2024, 3, 1))
        days = early.index[(early.index >= pd.Timestamp("2021-06-02")) & (early.index <= pd.Timestamp("2024-09-30"))]
        np.testing.assert_allclose(late.reindex(days).to_numpy(), early.reindex(days).to_numpy(), rtol=0, atol=1e-15)


class FixedBreakoutTests(unittest.TestCase):
    def test_reads_fixed_blocks_so_a_later_series_start_gives_the_same_verdicts(self):
        close = FRAMES["AAA"]["close"]
        cfg = TotalBreakoutConfig(trend_length=60, entry_length=5, exit_length=15)
        whole = market_gate(pd.DataFrame({"close": close}), cfg)
        later = pd.Timestamp("2023-06-01")
        from rotation_lab.backtest.signals import block_start
        cut = block_start(later) - pd.Timedelta(days=breakout_warmup(cfg))
        part = market_gate(pd.DataFrame({"close": close.loc[close.index >= cut]}), cfg)
        pd.testing.assert_series_equal(part.loc[part.index >= later], whole.loc[(whole.index >= later)], check_names=False)

    def test_market_start_covers_the_breakout_warmup_before_the_block(self):
        cfg = _config(dt.date(2024, 6, 15), TotalBreakoutConfig(trend_length=60, entry_length=5, exit_length=15))
        from rotation_lab.backtest.signals import block_start
        self.assertEqual(_market_start(cfg), block_start(cfg.start_date).date() - dt.timedelta(days=8 * 60))


class LaterStartTests(unittest.TestCase):
    def test_the_filter_reads_the_same_total_whatever_the_start(self):
        for name, market in FILTERS.items():
            with self.subTest(filter=name):
                early = _seen_total(dt.date(2023, 1, 1), market)
                for later in (dt.date(2023, 9, 14), dt.date(2024, 6, 15)):
                    late = _seen_total(later, market)
                    days = late.index[1:]
                    self.assertGreater(len(days[days < pd.Timestamp(later)]), 300)
                    np.testing.assert_allclose(late.reindex(days).to_numpy(), early.reindex(days).to_numpy(), rtol=0, atol=1e-14)

    def test_gate_from_a_later_start_equals_the_gate_of_the_earlier_run(self):
        for name, market in FILTERS.items():
            with self.subTest(filter=name):
                early = _run(dt.date(2023, 1, 1), market).strategies[0].market_gate
                for later in (dt.date(2023, 9, 14), dt.date(2024, 3, 2), dt.date(2024, 6, 15)):
                    gate = _run(later, market).strategies[0].market_gate
                    days = gate.index[gate.index >= pd.Timestamp(later)]
                    self.assertGreater(len(days), 60)
                    pd.testing.assert_series_equal(gate.reindex(days), early.reindex(days), check_names=False)

    def test_the_run_reads_the_cache_and_not_the_network_before_its_start(self):
        registry = FakeRegistry()
        _run(dt.date(2024, 3, 2), FILTERS["fixed breakout"], registry=registry)
        self.assertGreaterEqual(min(registry.network_days), dt.date(2024, 3, 2))


class LiveTests(unittest.TestCase):
    def test_todays_live_gate_equals_a_backtest_of_today(self):
        from rotation_lab import rpc
        as_of = dt.date(2024, 8, 20)
        for name, market in FILTERS.items():
            with self.subTest(filter=name):
                raw_market = ({"trend": "total_breakout", "trendLength": 60, "entryLength": 5, "exitLength": 15}
                              if isinstance(market, TotalBreakoutConfig) else {"trend": "defensive_trend"})
                backtest = _run(dt.date(2023, 1, 1), market, end=as_of).strategies[0].market_gate
                for day in (as_of, dt.date(2024, 7, 3), dt.date(2024, 5, 9)):
                    with patch.object(rpc, "RankingRegistry", return_value=FakeRegistry()), \
                         patch.object(rpc, "get_default_cache"), \
                         patch.object(rpc, "OhlcvFetcher", return_value=_Ohlcv()), \
                         patch.object(rpc, "_notify"):
                        live = rpc._method_live_snapshot({"asOf": day.isoformat(), "config": {
                            "topN": 3, "indicator": {"trend": "ema_cross"}, "marketFilter": True,
                            "marketIndicator": raw_market, "cadence": "daily", "rankingResolution": "daily"}})
                    self.assertEqual(live["marketFilter"]["bullish"], bool(backtest.loc[pd.Timestamp(day)] == 1), (name, day))


if __name__ == "__main__":
    unittest.main()
