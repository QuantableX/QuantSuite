"""The robustness policy, the price variants, the bulk evaluator's delay and the System Gauntlet's analyses."""
import datetime as dt
import json
import math
import os
import random
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery import robust_policy as rp
from smithery.prices import PricesVariant, shuffle_window
from smithery import system_gauntlet as sg
from rotation_lab.backtest import vote_store


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

    def test_a_market_filter_that_is_an_indicator_is_varied_too(self):
        from rotation_lab.config import IndicatorConfig, RunConfig
        rows = {"filter_key": {"params": {"window": 100, "band": 0.5, "flag": True}, "param_space": {"window": [10, 400], "band": [0.1, 2.0]}}}
        config = RunConfig(top_n=5, market_filter=True, market_indicator=IndicatorConfig(trend="filter_key"))
        variants = dict(sg.settings_variants(config, rows))
        low, high = variants["market filter parameters ×0.8"], variants["market filter parameters ×1.2"]
        self.assertEqual(low.market_indicator.params["filter_key"]["window"], 80)
        self.assertEqual(high.market_indicator.params["filter_key"]["window"], 120)
        self.assertAlmostEqual(high.market_indicator.params["filter_key"]["band"], 0.6)
        self.assertNotIn("flag", low.market_indicator.params["filter_key"])
        jitter = sg.jittered(config, rows, random.Random(3)).market_indicator.params["filter_key"]
        self.assertNotEqual(jitter["window"], 100)
        self.assertTrue(85 <= jitter["window"] <= 115)
        ema = RunConfig(top_n=5, market_filter=True, market_indicator=IndicatorConfig(trend="ema_cross"))
        fast = ema.market_indicator.ema_cross.fast_length
        varied = dict(sg.settings_variants(ema, rows))["market filter parameters ×1.2"]
        self.assertEqual(varied.market_indicator.ema_cross.fast_length, max(2, round(fast * 1.2)))

    def test_total_breakout_keeps_its_own_check(self):
        from rotation_lab.config import IndicatorConfig, RunConfig, TotalBreakoutConfig
        config = RunConfig(top_n=5, market_filter=True, market_indicator=TotalBreakoutConfig(trend_length=100, entry_length=5, exit_length=10))
        labels = [label for label, _ in sg.settings_variants(config)]
        self.assertIn("market filter lengths ×0.8", labels)
        self.assertFalse(any("parameters" in label for label in labels))
        rows = {"k_a": {"params": {"length": 20}, "param_space": {"length": [5, 50]}}}
        member = RunConfig(indicator=IndicatorConfig(trend="k_a"), top_n=5, market_filter=True, market_indicator=config.market_indicator)
        out = sg.jittered(member, rows, random.Random(3))
        self.assertEqual(out.market_indicator, config.market_indicator)
        # The draws for the members did not move, so saved verdicts stay reproducible.
        draw = random.Random(3)
        self.assertEqual(out.indicator.params["k_a"]["length"], int(round(min(max(20 * (1 + draw.uniform(-sg.JITTER, sg.JITTER)), 5), 50))))



def _equity(collapse=False, days=1500, seed=3):
    index = pd.date_range("2020-01-01", periods=days, freq="D")
    steps = 0.002 + 0.01 * np.random.default_rng(seed).standard_normal(days)
    if collapse:
        steps[500:] -= 0.004   # two years and more of falling: the eras gate fails
    return pd.Series(np.exp(np.cumsum(steps)), index=index)


class Order(unittest.TestCase):
    """The order of the checks: the usual run is one pool, stop_early runs the cheapest first and nothing after the
    first failed gate. A check that runs gives the same numbers either way; a shared vote store changes none."""

    def config(self):
        from rotation_lab.rpc import _build_config
        return _build_config({"mode": "rotation", "topN": 5, "startDate": "2020-01-01", "endDate": "2024-01-01",
                              "includeUsd": True, "feeRate": 0.001, "marketFilter": True,
                              "marketIndicator": {"trend": "total_breakout", "trendLength": 100, "entryLength": 5, "exitLength": 5},
                              "indicator": {"trend": "ema_cross"}})

    def gauntlet(self, cagr_of=lambda label: 0.17, equity=None, gpu=False, env=None, **kwargs):
        """sg.run with the engine replaced: every variant's CAGR comes from ``cagr_of(label)``."""
        seen = {"calls": [], "jobs": [], "parent": [], "bulk_env": None, "salt": None, "base": None}
        base_equity = _equity() if equity is None else equity

        def fake_run_native(jobs, workers=None, progress=None, stage=""):
            if jobs[0][0] == "base":
                seen["base"] = jobs[0]
                return [{"label": "base", "multiple": 2.0, "years": 4.0, "cagr": 0.19, "max_drawdown_pct": 30.0, "equity": base_equity}]
            seen["calls"].append([job[0] for job in jobs])
            seen["jobs"].extend(jobs)
            seen["salt"] = os.environ.get(vote_store.ENV_SALT)
            return [{"label": job[0], "cagr": cagr_of(job[0])} for job in jobs]

        def fake_bulk(config, members, rows, data_dir, cube_dir, counts, rng, say):
            seen["calls"].append(["bulk"])
            seen["bulk_env"] = (os.environ.get(vote_store.ENV_FOLDER), os.environ.get(vote_store.ENV_MODE))
            seen["bulk_draw"] = rng.random()   # where the shared random stream stands when the bulk runs
            return {"base_cagr": 0.19, "drop": [], "add": [], "drop_details": [], "add_details": [], "delay": {1: 0.18, 2: 0.17}}

        def fake_parent_votes(jobs, store, progress=None, every=False):
            seen["parent"].append(every)
            return [(*job[:4], (store, "read")) for job in jobs], "parent"

        with patch.object(sg, "run_native", side_effect=fake_run_native), patch.object(sg, "_bulk", side_effect=fake_bulk), \
                patch.object(sg, "parent_votes", side_effect=fake_parent_votes), \
                patch("smithery.batch.gpu_available", return_value=gpu), patch("smithery.batch.device_note", return_value="GPU (test)"), \
                patch("smithery.registry.describe_all", return_value={"indicators": []}), patch.dict(os.environ, env or {}):
            for name in ("SMITHERY_VOTE_SHARE", "SMITHERY_GAUNTLET_STOP_EARLY", vote_store.ENV_SALT):
                if name not in (env or {}):
                    os.environ.pop(name, None)
            result = sg.run(self.config(), data_dir=Path(tempfile.gettempdir()), fast=True, venue_path=Path(__file__), **kwargs)
            seen["salt_after"] = os.environ.get(vote_store.ENV_SALT)
        return result, seen

    @staticmethod
    def numbers(result):
        return {k: v for k, v in result.items() if k not in ("seconds", "vote_store", "stopped_early")}

    def test_the_usual_run_is_one_pool_and_then_the_bulk(self):
        result, seen = self.gauntlet()
        self.assertEqual(len(seen["calls"]), 2)
        self.assertEqual(seen["calls"][1], ["bulk"])
        groups = [label.split("|")[0] for label in seen["calls"][0]]
        self.assertEqual(list(dict.fromkeys(groups)), ["parameter_jitter", "system_settings", "start_shift", "price_noise",
                                                       "venue", "universe", "fees", "permutation"])
        self.assertEqual(list(result["groups"]), [g for g in sg.RESULT_ORDER if g in result["groups"]])
        self.assertTrue(result["verdict"]["robust"])
        self.assertNotIn("stopped_early", result)
        self.assertNotIn("vote_store", result)

    def test_stop_early_runs_the_cheapest_first_and_changes_no_number(self):
        usual, seen_usual = self.gauntlet()
        staged, seen = self.gauntlet(stop_early=True)
        order = [sorted({label.split("|")[0] for label in call}) for call in seen["calls"]]
        self.assertEqual(order, [["fees", "start_shift", "system_settings", "universe"], ["bulk"], ["price_noise", "venue"],
                                 ["parameter_jitter"], ["permutation"]])
        self.assertNotIn("stopped_early", staged)
        self.assertEqual(json.dumps(self.numbers(staged), sort_keys=True), json.dumps(self.numbers(usual), sort_keys=True))
        # The same variants, drawn in the same order: a jittered config does not depend on when it runs.
        key = lambda job: job[0]  # noqa: E731
        self.assertEqual([(j[0], j[1], j[2]) for j in sorted(seen["jobs"], key=key)],
                         [(j[0], j[1], j[2]) for j in sorted(seen_usual["jobs"], key=key)])
        self.assertEqual(seen["bulk_draw"], seen_usual["bulk_draw"])
        # The switch in the environment is the same switch.
        by_env, _ = self.gauntlet(env={"SMITHERY_GAUNTLET_STOP_EARLY": "1"})
        self.assertEqual(json.dumps(self.numbers(by_env), sort_keys=True), json.dumps(self.numbers(usual), sort_keys=True))

    def test_a_failed_base_check_stops_before_any_variant(self):
        falling = _equity(collapse=True)
        usual, _ = self.gauntlet(equity=falling)
        staged, seen = self.gauntlet(equity=falling, stop_early=True)
        self.assertEqual(seen["calls"], [])
        self.assertEqual(staged["stopped_early"]["failed"], ["eras", "bootstrap"])   # both read the base run only
        self.assertIn("permutation", staged["stopped_early"]["not_run"])
        self.assertIn("bulk", staged["stopped_early"]["not_run"])
        self.assertEqual(staged["stopped_early"]["variant_runs_not_run"], len(sum(self.gauntlet(equity=falling)[1]["calls"], [])) - 1)
        for result in (usual, staged):
            self.assertFalse(result["verdict"]["robust"])
            self.assertEqual(result["verdict"]["status"], "not robust")
        self.assertEqual(staged["verdict"]["failures"],
                         [f for f in usual["verdict"]["failures"] if f.startswith(("eras", "bootstrap"))])
        self.assertEqual(staged["groups"]["eras"], usual["groups"]["eras"])
        self.assertEqual(staged["groups"]["bootstrap"], usual["groups"]["bootstrap"])

    def test_a_failed_cheap_stage_stops_the_expensive_ones(self):
        ruin = lambda label: 0.01 if label.startswith("fees|") else 0.17  # noqa: E731
        usual, _ = self.gauntlet(cagr_of=ruin)
        staged, seen = self.gauntlet(cagr_of=ruin, stop_early=True)
        self.assertEqual(len(seen["calls"]), 1)   # the stage on the base run's votes; no bulk, no new votes
        self.assertEqual(staged["stopped_early"]["failed"], ["fees"])
        self.assertEqual(staged["stopped_early"]["not_run"], ["bulk", "price_noise", "venue", "parameter_jitter", "permutation"])
        self.assertFalse(staged["verdict"]["robust"])
        self.assertFalse(usual["verdict"]["robust"])
        self.assertEqual(staged["verdict"]["status"], usual["verdict"]["status"])
        self.assertEqual(staged["verdict"]["failures"], usual["verdict"]["failures"])
        for group in ("fees", "start_shift", "system_settings", "universe", "eras", "bootstrap"):
            self.assertEqual(staged["groups"][group], usual["groups"][group])

    def test_a_shared_store_on_the_cpu_is_written_by_every_process(self):
        with tempfile.TemporaryDirectory() as share:
            usual, _ = self.gauntlet(workers=4)
            shared, seen = self.gauntlet(workers=4, votes_dir=share)
            folder = str(Path(share) / "cpu")
            self.assertEqual(seen["base"][4], (folder, "write"))
            self.assertTrue(all(job[4] == (folder, "write") for job in seen["jobs"]))
            self.assertEqual(seen["bulk_env"], (folder, "write"))
            self.assertEqual(seen["salt"], sg.vote_environment("cpu"))
            self.assertIsNone(seen["salt_after"])
            self.assertEqual(shared["vote_store"], {"shared": True, "device": "cpu"})
            self.assertEqual(json.dumps(self.numbers(shared), sort_keys=True), json.dumps(self.numbers(usual), sort_keys=True))
            by_env, seen_env = self.gauntlet(workers=4, env={"SMITHERY_VOTE_SHARE": share})
            self.assertEqual(seen_env["base"][4], (folder, "write"))

    def test_a_shared_store_with_gpu_parent_and_cpu_workers_is_written_by_the_parent_only(self):
        with tempfile.TemporaryDirectory() as share:
            _, seen = self.gauntlet(workers=4, votes_dir=share, gpu=True)
            folder = str(Path(share) / "gpu")
            self.assertEqual(seen["base"][4], (folder, "write"))
            self.assertEqual(seen["parent"], [True])   # the parent computes every variant's votes
            self.assertTrue(all(job[4] == (folder, "read") for job in seen["jobs"]))
            self.assertEqual(seen["salt"], sg.vote_environment("GPU (test)"))
            self.assertNotEqual(sg.vote_environment("GPU (test)"), sg.vote_environment("cpu"))
            # One process on the GPU writes; a GPU base run with CPU votes in the workers is not one device: no sharing.
            _, serial = self.gauntlet(workers=1, votes_dir=share, gpu=True)
            self.assertTrue(all(job[4] == (folder, "write") for job in serial["jobs"]))
            _, mixed = self.gauntlet(workers=4, votes_dir=share, gpu=True, env={"SMITHERY_PARENT_VOTES": "0"})
            self.assertFalse(mixed["base"][4][0].startswith(share))
            self.assertIsNone(mixed["salt"])

    def test_the_environment_fingerprint_follows_the_switches_that_change_arithmetic(self):
        sg._ENVIRONMENTS.clear()
        base = sg.vote_environment("cpu")
        self.assertEqual(base, sg.vote_environment("cpu"))
        with patch.dict(os.environ, {"SMITHERY_PANDAS_MOMENTS": "1"}):
            sg._ENVIRONMENTS.clear()
            self.assertNotEqual(base, sg.vote_environment("cpu"))
        sg._ENVIRONMENTS.clear()
        self.assertEqual(base, sg.vote_environment("cpu"))


if __name__ == "__main__":
    unittest.main()
