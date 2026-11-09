"""Portfolio mode of rotation systems (card 77ef7dfd): several top-ranked coins at once, only those that trend."""
import unittest
from dataclasses import replace

import numpy as np
import pandas as pd

from rotation_lab.backtest import engine
from rotation_lab.backtest.engine import USD_SYMBOL, BacktestEngine, _rebalance, portfolio_target
from rotation_lab.backtest.signals import pair_signal
from rotation_lab.benchmark import fixture
from rotation_lab.config import PortfolioConfig, RunConfig
from rotation_lab.rpc import _build_config

# The user's example on the card: 1,000.00$, Top 5, BTC does not trend.
SYMBOLS = ["SOL", "ETH", "BTC", "XRP", "DOGE"]
SCORES = {"SOL": 5, "ETH": 3, "BTC": 3, "XRP": 2, "DOGE": 1}
TREND = {"SOL": True, "ETH": True, "BTC": False, "XRP": True, "DOGE": True}


def dollars(weights):
    return {s: round(w * 1000, 2) for s, w in weights.items()}


class TargetTests(unittest.TestCase):
    def check(self, weighting, not_trending, expected, positions=0):
        cfg = PortfolioConfig(positions=positions, weighting=weighting, not_trending=not_trending)
        got = dollars(portfolio_target(SYMBOLS, SCORES, TREND, cfg))
        self.assertEqual(got, expected)
        self.assertAlmostEqual(sum(got.values()), 1000.0, places=1)

    def test_the_cards_tables(self):
        self.check("equal", "with_usd", {"SOL": 200.0, "ETH": 200.0, "XRP": 200.0, "DOGE": 200.0, USD_SYMBOL: 200.0})
        self.check("equal", "without_usd", {"SOL": 250.0, "ETH": 250.0, "XRP": 250.0, "DOGE": 250.0})
        self.check("rank", "with_usd", {"SOL": 333.33, "ETH": 266.67, "XRP": 133.33, "DOGE": 66.67, USD_SYMBOL: 200.0})
        self.check("rank", "without_usd", {"SOL": 416.67, "ETH": 333.33, "XRP": 166.67, "DOGE": 83.33})
        self.check("score", "with_usd", {"SOL": 357.14, "ETH": 214.29, "XRP": 142.86, "DOGE": 71.43, USD_SYMBOL: 214.29})
        self.check("score", "without_usd", {"SOL": 454.55, "ETH": 272.73, "XRP": 181.82, "DOGE": 90.91})

    def test_three_positions_no_lower_rank_moves_up(self):
        self.check("equal", "with_usd", {"SOL": 333.33, "ETH": 333.33, USD_SYMBOL: 333.33}, positions=3)
        self.check("equal", "without_usd", {"SOL": 500.0, "ETH": 500.0}, positions=3)

    def test_nothing_trending_is_all_usd(self):
        cfg = PortfolioConfig(not_trending="without_usd")
        self.assertEqual(portfolio_target(SYMBOLS, SCORES, dict.fromkeys(SYMBOLS, False), cfg), {USD_SYMBOL: 1.0})

    def test_ties_keep_universe_order(self):
        cfg = PortfolioConfig(positions=2, not_trending="without_usd")
        self.assertEqual(list(portfolio_target(SYMBOLS, SCORES, TREND, cfg)), ["SOL", "ETH"])  # ETH before BTC at 3

    def test_settings_are_checked(self):
        with self.assertRaises(ValueError):
            PortfolioConfig(weighting="random")
        with self.assertRaises(ValueError):
            RunConfig(top_n=5, portfolio=PortfolioConfig(positions=6))
        with self.assertRaises(ValueError):
            RunConfig(mode="single_asset", portfolio=PortfolioConfig())


class FeeTests(unittest.TestCase):
    def test_a_full_switch_costs_the_single_picks_two_legs(self):
        c = 0.0015
        values, fees = _rebalance({"AAA": 2.0}, {"BBB": 1.0}, c)
        self.assertAlmostEqual(values["BBB"], 2.0 * (1 - c) ** 2, places=15)
        self.assertAlmostEqual(sum(fees.values()), 2.0 - values["BBB"], places=15)

    def test_cash_to_a_coin_is_one_leg(self):
        values, _ = _rebalance({USD_SYMBOL: 1.0}, {"AAA": 1.0}, 0.001)
        self.assertAlmostEqual(values["AAA"], 0.999, places=15)

    def test_only_the_money_that_moves_pays(self):
        values, fees = _rebalance({"AAA": 0.5, "BBB": 0.5}, {"AAA": 0.5, USD_SYMBOL: 0.5}, 0.001)
        self.assertAlmostEqual(fees["BBB"], 0.5 * 0.001, places=12)  # BBB is sold
        self.assertLess(fees["AAA"], 1e-6)  # AAA only trims to half of the slightly smaller equity; cash pays nothing
        self.assertNotIn(USD_SYMBOL, fees)
        self.assertAlmostEqual(sum(values.values()), 1.0 - sum(fees.values()), places=15)


def _reference_portfolio(config, timeline, frames, gate):
    """Portfolio mode by the scalar scorer, independent of the array ranking path."""
    cfg = config.portfolio
    dates = sorted(set().union(*(set(f.index) for f in frames.values())))
    index = pd.DatetimeIndex([t for t in dates if config.start_date <= t.date() <= config.end_date])
    cache = {}

    def signal(a, b):
        if (a, b) not in cache:
            cache[a, b] = (pair_signal(frames[a], frames.get(b), config.indicator)
                           if a in frames and (b is None or b in frames) else pd.Series(dtype=int))
        return cache[a, b]

    cost = 1 - (1 - config.fee_rate) * (1 - config.slippage_rate)
    values, pending, equities = {}, None, []
    for ts in index:
        active = [s for s in timeline.snapshots if pd.Timestamp(s.on_date) <= ts]
        symbols = [c.symbol for c in active[-1].coins] if active else []
        if pending is not None:
            gone = [s for s in pending if s != USD_SYMBOL and s not in symbols]
            for s in gone:
                pending = dict(pending)
                pending[USD_SYMBOL] = pending.get(USD_SYMBOL, 0.0) + pending.pop(s)
            held = {s for s, v in values.items() if v > 0} or {USD_SYMBOL}
            if cfg.rebalance == "daily" or held != set(pending):
                values, _ = _rebalance(values or {USD_SYMBOL: 1.0}, pending, cost)
        for s in values:
            if s != USD_SYMBOL and s in frames and ts in frames[s].index:
                row = frames[s].loc[ts]
                o, c = row["open"], row["close"]
                if o and c and o > 0 and not pd.isna(o) and not pd.isna(c):
                    values[s] *= 1 + (c - o) / o
        equities.append(sum(values.values()) if values else 1.0)
        if not symbols:
            pending = {USD_SYMBOL: 1.0} if config.include_usd else None
        elif gate is not None and int(gate.get(ts, 0)) != 1:
            pending = {USD_SYMBOL: 1.0}
        else:
            raw = BacktestEngine._score_universe(symbols, ts, signal, config.include_usd)
            trend = {s: int(signal(s, None).get(ts, 0)) == 1 if not signal(s, None).empty else False for s in symbols}
            pending = portfolio_target(symbols, {s: raw.get(s, 0) for s in symbols}, trend, cfg)
    return pd.Series(equities, index=index, name="equity")


class EngineTests(unittest.TestCase):
    def setUp(self):
        self.config, self.timeline, self.frames = fixture(600, ("ema_cross",))
        self.gate = pd.Series((np.arange(len(self.frames["BTC"])) % 23 < 15).astype(int), index=self.frames["BTC"].index)

    def simulate(self, config, gate=None):
        return BacktestEngine(registry=object(), ohlcv=object())._simulate(config, self.timeline, self.frames, [], gate)

    def test_matches_the_scalar_reference_for_every_setting(self):
        for weighting in ("equal", "rank", "score"):
            for not_trending in ("with_usd", "without_usd"):
                for rebalance in ("on_change", "daily"):
                    for positions in (0, 2):
                        cfg = PortfolioConfig(positions, weighting, not_trending, rebalance)
                        config = replace(self.config, portfolio=cfg)
                        equity = self.simulate(config, self.gate)[0]
                        expected = _reference_portfolio(config, self.timeline, self.frames, self.gate)
                        np.testing.assert_allclose(equity.to_numpy(), expected.to_numpy(), rtol=1e-12, err_msg=str(cfg))

    def test_weights_sum_to_one_and_history_rows_sum_to_the_net_return(self):
        config = replace(self.config, portfolio=PortfolioConfig(weighting="rank"))
        equity, held, _, _ = self.simulate(config, self.gate)
        weights = [w for w in held.attrs["weights"] if w]
        self.assertTrue(weights)
        for w in weights:
            self.assertAlmostEqual(sum(w.values()), 1.0, places=12)
        net = sum(t.equity_change for t in held.attrs["trades"] if t.equity_change is not None)
        self.assertAlmostEqual(net, equity.iloc[-1] - 1.0, places=10)

    def test_a_bearish_gate_means_all_usd_the_next_day(self):
        config = replace(self.config, portfolio=PortfolioConfig())
        _, held, _, _ = self.simulate(config, pd.Series(0, index=self.frames["BTC"].index))
        self.assertTrue(all(w in (None, {USD_SYMBOL: 1.0}) for w in held.attrs["weights"]))

    def test_off_is_the_original_engine(self):
        plain = self.simulate(self.config, self.gate)
        self.assertNotIn("weights", plain[1].attrs)
        self.assertIsNone(_build_config({"topN": 5}).portfolio)
        self.assertIsNone(_build_config({"topN": 5, "portfolio": {"enabled": False, "positions": 3}}).portfolio)
        self.assertIsNone(_build_config({"mode": "single_asset", "portfolio": {"enabled": True}}).portfolio)
        built = _build_config({"topN": 5, "portfolio": {"enabled": True, "positions": 3, "weighting": "score",
                                                        "notTrending": "withoutUsd", "rebalance": "daily"}}).portfolio
        self.assertEqual(built, PortfolioConfig(3, "score", "without_usd", "daily"))


class BulkTests(unittest.TestCase):
    def test_bulk_evaluators_refuse_what_they_cannot_score(self):
        # lab_fast walks portfolio mode itself (bit for bit: tests/test_research_fast.py), except without USD,
        # where the cube has no coin-against-USD votes for the trend. The GPU path scores the single pick only.
        from smithery import lab_gpu
        from smithery.lab_fast import unsupported_reason
        config, _, _ = fixture(50, ("ema_cross",))
        self.assertIsNone(unsupported_reason(config))
        self.assertIsNone(unsupported_reason(replace(config, portfolio=PortfolioConfig())))
        reason = unsupported_reason(replace(config, include_usd=False, portfolio=PortfolioConfig()))
        self.assertIn("Portfolio mode", reason)

        class Fake:
            single = False

        fake = Fake()
        fake.config = replace(config, portfolio=PortfolioConfig())
        self.assertFalse(lab_gpu.supports(fake))
        fake.config = config
        self.assertTrue(lab_gpu.supports(fake))

    def test_the_gauntlet_varies_positions_and_keeps_top_n_variants_valid(self):
        from smithery.system_gauntlet import settings_variants, universe_variants
        config, _, _ = fixture(50, ("ema_cross",))
        plain = [label for label, _ in settings_variants(config)]
        self.assertFalse(any("positions" in label for label in plain))
        five = replace(config, portfolio=PortfolioConfig(positions=5))
        variants = dict(settings_variants(five))
        self.assertEqual(variants["top 4"].portfolio.positions, 4)  # shrinks with Top N instead of failing
        self.assertEqual(variants["4 positions"].portfolio.positions, 4)
        self.assertNotIn("6 positions", variants)
        self.assertEqual(dict(universe_variants(five))["top 3"].portfolio.positions, 3)
        three = dict(settings_variants(replace(config, portfolio=PortfolioConfig(positions=3))))
        self.assertEqual({k for k in three if "positions" in k}, {"2 positions", "4 positions"})
        everything = dict(settings_variants(replace(config, portfolio=PortfolioConfig())))
        self.assertEqual({k for k in everything if "positions" in k}, {"4 positions"})

    def test_an_incomplete_gauntlet_is_never_robust(self):
        from smithery import robust_policy as rp
        axes = {name: 100.0 for name in ("parameters", "time", "exchange", "assets", "monte_carlo", "execution")}
        verdict = rp.verdict(axes, [], 0.01, False, 0.5)
        self.assertFalse(verdict["robust"])
        self.assertEqual(verdict["status"], "incomplete")


if __name__ == "__main__":
    unittest.main()
