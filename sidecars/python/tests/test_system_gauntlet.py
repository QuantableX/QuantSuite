"""The robustness policy, the price variants, the bulk evaluator's delay and the System Gauntlet's analyses."""
import datetime as dt
import json
import math
import random
import unittest
from types import SimpleNamespace

import numpy as np
import pandas as pd

from smithery import robust_policy as rp
from smithery.prices import PricesVariant, shuffle_window
from smithery import system_gauntlet as sg


def frame(days=400, seed=1, start="2019-06-01"):
    index = pd.date_range(start, periods=days, freq="D")
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(0.001 + 0.03 * rng.standard_normal(days)))
    open_ = close * (1 + 0.004 * rng.standard_normal(days))
    return pd.DataFrame({"open": open_, "high": np.maximum(open_, close) * 1.01, "low": np.minimum(open_, close) * 0.99,
                         "close": close, "volume": 1000.0}, index=index)


class Base:
    """A saved-data reader over fixed frames."""
    def __init__(self, frames):
        self.frames = frames

    def get_series(self, ref, start, end, timeframe="1d"):
        f = self.frames[ref.symbol]
        return SimpleNamespace(frame=f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])

    def get_pair_series(self, exchange, pair, start, end, timeframe="1d"):
        return self.get_series(SimpleNamespace(symbol=pair), start, end, timeframe)


def ref(symbol):
    from rotation_lab.data.ohlcv import CoinRef
    return CoinRef(cg_id=None, symbol=symbol)


class Policy(unittest.TestCase):
    def test_cagr_and_retention(self):
        self.assertAlmostEqual(rp.cagr(4.0, 2.0), 1.0)
        self.assertEqual(rp.cagr(0.0, 3.0), -1.0)
        self.assertAlmostEqual(rp.retention(0.5, 0.25), 0.5)
        self.assertTrue(math.isnan(rp.retention(-0.1, 0.2)))  # no edge to keep
        self.assertEqual(rp.score_of(0.4), 0.0)
        self.assertEqual(rp.score_of(0.9), 100.0)

    def test_gates(self):
        ok = rp.judge("price_noise", 1.0, [0.95, 0.9, 1.0])
        self.assertFalse(ok["failed"])
        low = rp.judge("price_noise", 1.0, [0.95, 0.45, 0.9])
        self.assertTrue(low["failed"])
        self.assertIn("worst retention", low["reasons"][0])
        flip = rp.judge("delay", 1.0, [-0.05])
        self.assertTrue(flip["failed"])
        self.assertTrue(any("lose money" in r for r in flip["reasons"]))

    def test_member_groups_below_six_members_are_shown_not_scored(self):
        small = rp.judge("member_drop", 1.0, [0.2, 0.3, 0.25], members=3)
        self.assertFalse(small["gated"])
        self.assertFalse(small["failed"])
        self.assertTrue(math.isnan(small["score"]))
        big = rp.judge("member_drop", 1.0, [0.2] * 8, members=8)
        self.assertTrue(big["failed"])

    def test_verdict(self):
        axes = {a: 80.0 for a in rp.AXES}
        self.assertEqual(rp.verdict(axes, [], 0.02, True, 0.5)["status"], "robust")
        self.assertEqual(rp.verdict(axes, ["delay: x"], 0.02, True, 0.5)["status"], "not robust")
        self.assertEqual(rp.verdict(axes, [], 0.02, False, 0.5)["status"], "incomplete")
        self.assertEqual(rp.verdict(axes, [], 0.02, True, -0.1)["status"], "not robust")
        self.assertEqual(rp.verdict({a: 90.0 for a in rp.AXES}, [], 0.3, True, 0.5)["grade"], "B")

    def test_jsonable(self):
        out = rp.jsonable({"a": float("nan"), "b": np.float64(1.5), "c": [np.int64(2), np.bool_(True)], "d": (1, float("inf"))})
        self.assertEqual(json.loads(json.dumps(out, allow_nan=False)), {"a": None, "b": 1.5, "c": [2, True], "d": [1, None]})


class Prices(unittest.TestCase):
    def setUp(self):
        self.frames = {"AAA": frame(seed=1), "BBB": frame(seed=2)}

    def test_noise_is_deterministic_per_day_whatever_the_window(self):
        v = PricesVariant(Base(self.frames), {"kind": "noise", "seed": 3, "sigma": 0.0003})
        whole = v.get_series(ref("AAA"), dt.date(2019, 6, 1), dt.date(2020, 6, 1)).frame
        part = v.get_series(ref("AAA"), dt.date(2019, 9, 1), dt.date(2019, 12, 31)).frame
        self.assertTrue(np.allclose(whole.loc[part.index, "close"], part["close"], rtol=0, atol=0))
        orig = self.frames["AAA"].loc[whole.index]
        rel = (whole["close"] / orig["close"] - 1).abs()
        self.assertGreater(rel.max(), 0)
        self.assertLess(rel.max(), 0.003)
        self.assertTrue((whole["high"] >= whole[["open", "close"]].max(axis=1)).all())
        self.assertTrue((whole["low"] <= whole[["open", "close"]].min(axis=1)).all())

    def test_other_seed_other_noise(self):
        a = PricesVariant(Base(self.frames), {"kind": "noise", "seed": 1}).get_series(ref("AAA"), dt.date(2019, 6, 1), dt.date(2019, 7, 1)).frame
        b = PricesVariant(Base(self.frames), {"kind": "noise", "seed": 2}).get_series(ref("AAA"), dt.date(2019, 6, 1), dt.date(2019, 7, 1)).frame
        self.assertFalse(np.allclose(a["close"], b["close"]))

    def test_shuffle_keeps_warmup_and_reorders_window_moves(self):
        f = self.frames["AAA"]
        lo, hi = dt.date(2020, 1, 1), dt.date(2020, 6, 30)
        out = shuffle_window(f, lo, hi, seed=5)
        before = f.index < pd.Timestamp(lo)
        self.assertTrue(np.array_equal(out.loc[before].to_numpy(), f.loc[before].to_numpy()))
        inside = (f.index >= pd.Timestamp(lo)) & (f.index <= pd.Timestamp(hi))
        real = (f["close"] / f["close"].shift(1))[inside].to_numpy()
        new = (out["close"] / out["close"].shift(1))[inside].to_numpy()
        self.assertFalse(np.allclose(real, new))
        self.assertTrue(np.allclose(np.sort(real), np.sort(new)))  # the same moves, another order
        again = shuffle_window(f, lo, hi, seed=5)
        self.assertTrue(np.array_equal(out.to_numpy(), again.to_numpy()))

    def test_unknown_variant_is_refused(self):
        with self.assertRaises(ValueError):
            PricesVariant(Base(self.frames), {"kind": "magic"})


class Delay(unittest.TestCase):
    def system(self):
        from smithery.lab_fast import FastSystem
        fs = FastSystem.__new__(FastSystem)
        T = 8
        assets = ["A", "B", "USD"]
        fs.assets = assets
        fs.config = SimpleNamespace(fee_rate=0.001, slippage_rate=0.0, include_usd=True)
        fs.rotation = {"returns": np.array([[0.01] * T, [0.02] * T, [0.0] * T]), "in_today": np.ones((T, 3), dtype=bool),
                       "first_sym": np.zeros(T, dtype=np.int64), "has_syms": np.ones(T, dtype=bool)}
        return fs

    def test_delay_holds_each_decision_one_bar_longer(self):
        fs = self.system()
        best = np.array([[0, 0, 1, 1, 0, 2, 1, 1]])
        c0, h0 = fs._walk(best, [None], 0)
        c1, h1 = fs._walk(best, [None], 1)
        self.assertEqual(h0[0].tolist()[1:], best[0].tolist()[:-1])  # taken at the next bar
        self.assertEqual(h1[0].tolist()[2:], h0[0].tolist()[1:-1])   # one bar later again
        self.assertFalse(np.array_equal(c0, c1))
        again, _ = fs._walk(best, [None])
        self.assertTrue(np.array_equal(c0, again))


class Analyses(unittest.TestCase):
    def equity(self, drift=0.002, days=900, seed=4):
        rng = np.random.default_rng(seed)
        index = pd.date_range("2020-01-01", periods=days, freq="D")
        return pd.Series(np.cumprod(1 + drift + 0.02 * rng.standard_normal(days)), index=index)

    def test_eras(self):
        good = sg.eras(self.equity())
        self.assertEqual(set(good["years"]), {"2020", "2021", "2022"})
        self.assertFalse(good["failed"])
        bad = sg.eras(self.equity(drift=-0.003))
        self.assertTrue(bad["failed"])

    def test_bootstrap_is_seeded(self):
        eq = self.equity()
        a, b = sg.bootstrap(eq, 200, 7), sg.bootstrap(eq, 200, 7)
        self.assertEqual(a["cagr_p5"], b["cagr_p5"])
        self.assertLessEqual(a["dd_p50"], a["dd_p95"])
        self.assertLess(a["loss_3y"], 0.25)

    def test_permutation_p(self):
        r = sg.permutation(0.5, [0.1, 0.2, 0.6, 0.0])
        self.assertAlmostEqual(r["p"], 2 / 5)
        self.assertTrue(r["caps_grade"])

    def test_jitter_stays_in_the_parameter_space(self):
        from rotation_lab.config import IndicatorConfig, RunConfig
        config = RunConfig(indicator=IndicatorConfig(trend="aggregate", aggregate=("k_a", "k_b"), params={"k_b": {"length": 40}}))
        rows = {"k_a": {"params": {"length": 20, "band": 0.5, "src": "close"}, "param_space": {"length": [5, 50], "band": [0.1, 1.0]}},
                "k_b": {"params": {"length": 30}, "param_space": {"length": [5, 45]}}}
        for seed in range(20):
            out = sg.jittered(config, rows, random.Random(seed)).indicator.params
            self.assertTrue(5 <= out["k_a"]["length"] <= 50 and isinstance(out["k_a"]["length"], int))
            self.assertTrue(0.1 <= out["k_a"]["band"] <= 1.0)
            self.assertNotIn("src", out["k_a"])
            self.assertTrue(34 <= out["k_b"]["length"] <= 45)  # around the override, not the default

    def test_settings_and_universe_variants_are_valid(self):
        from rotation_lab.config import RunConfig, TotalBreakoutConfig
        config = RunConfig(top_n=5, market_filter=True, market_indicator=TotalBreakoutConfig(trend_length=100, entry_length=2, exit_length=2))
        labels = [label for label, _ in sg.settings_variants(config)]
        self.assertEqual(labels[:2], ["top 4", "top 6"])
        self.assertEqual(len(labels), 4)
        uni = dict(sg.universe_variants(config))
        self.assertEqual(uni["top 3"].top_n, 3)
        self.assertEqual(uni["without the largest coin"].exclude_top_n, 1)
        self.assertEqual(uni["without the largest coin"].top_n, 6)


if __name__ == "__main__":
    unittest.main()
