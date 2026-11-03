"""Card 2 (docs/ROBUSTNESS.md): universal optimization helpers, ratio frames and the strict certification lookup."""
import math
import unittest
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery import universal
from smithery.robustness import WEIGHTS, ratio_frame


def frame(seed, days=300):
    index = pd.date_range("2021-01-01", periods=days, freq="D", tz="UTC")
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(0.02 * rng.standard_normal(days)))
    open_ = close * (1 + 0.003 * rng.standard_normal(days))
    return pd.DataFrame({"open": open_, "high": np.maximum(open_, close) * 1.01, "low": np.minimum(open_, close) * 0.99,
                         "close": close, "volume": 1.0}, index=index)


class Universal(unittest.TestCase):
    def test_param_distance(self):
        space = {"length": (5, 500), "band": (0.0, 1.0)}
        self.assertEqual(universal.param_distance({"length": 20, "band": 0.5}, {"length": 20, "band": 0.5}, space), 0.0)
        far = universal.param_distance({"length": 5, "band": 0.0}, {"length": 500, "band": 1.0}, space)
        self.assertAlmostEqual(far, 1.0)
        # log scale: 10 -> 20 is as far as 100 -> 200
        a = universal.param_distance({"length": 10}, {"length": 20}, {"length": (5, 500)})
        b = universal.param_distance({"length": 100}, {"length": 200}, {"length": (5, 500)})
        self.assertAlmostEqual(a, b)

    def test_median_row_needs_a_majority(self):
        rows = [{"cagr": 0.1, "sharpe": 1.0, "sortino": 1.0, "omega": 1.2, "max_dd": -0.2, "turnover_per_year": 5},
                {"cagr": 0.3, "sharpe": 2.0, "sortino": 2.0, "omega": 1.5, "max_dd": -0.1, "turnover_per_year": 7},
                {"cagr": -0.2, "sharpe": -1.0, "sortino": -1.0, "omega": 0.8, "max_dd": -0.5, "turnover_per_year": 9, "bankrupt": True}]
        out = universal._median_row(rows)
        self.assertEqual(out["cagr"], 0.1)
        self.assertEqual(out["sharpe"], 1.0)
        self.assertFalse(out["bankrupt"])
        out = universal._median_row(rows[:1] + [{"cagr": float("nan")}] * 2)
        self.assertTrue(math.isnan(out["cagr"]))

    def test_slice_is_half_open_and_tz_safe(self):
        f = frame(1)
        part = universal._slice(f, "2021-02-01", "2021-03-01")
        self.assertEqual(str(part.index[0].date()), "2021-02-01")
        self.assertEqual(str(part.index[-1].date()), "2021-02-28")


class Gauntlet(unittest.TestCase):
    def test_six_axes_weigh_the_same(self):
        self.assertEqual(set(WEIGHTS), {"asset", "exchange", "parameter", "temporal", "windows", "monte_carlo"})
        self.assertAlmostEqual(sum(WEIGHTS.values()), 1.0)

    def test_ratio_frame(self):
        a, b = frame(1), frame(2)
        r = ratio_frame(a, b)
        self.assertEqual(list(r.columns), ["open", "high", "low", "close", "volume"])
        self.assertTrue(np.allclose(r["close"], a["close"] / b["close"]))
        self.assertTrue((r["high"] >= r[["open", "close"]].max(axis=1)).all())
        self.assertTrue((r["low"] <= r[["open", "close"]].min(axis=1)).all())


class StrictCertification(unittest.TestCase):
    def test_only_current_evaluations_count(self):
        from smithery import registry, variants
        from smithery.evidence import EVALUATION_VERSION
        good = {"score": 80, "perm_p": 0.02, "certified": True}
        docs = {"k_new": {"evidence": {"timeframes": {"1d": {**good, "evaluation_version": EVALUATION_VERSION}}}},
                "k_old": {"evidence": {"timeframes": {"1d": {**good, "evaluation_version": "smithery-2026-09-09.1"}}}},
                "k_fail": {"evidence": {"timeframes": {"1d": {**good, "certified": False, "evaluation_version": EVALUATION_VERSION}}}}}
        with patch.object(variants, "VERSION_DOCS", docs), patch.object(registry, "_current_runs", lambda historical=False: {}):
            self.assertTrue(registry.robust_on_track("k_new", "1d"))
            self.assertFalse(registry.robust_on_track("k_old", "1d"))
            self.assertFalse(registry.robust_on_track("k_fail", "1d"))
            self.assertFalse(registry.robust_on_track("k_missing", "1d"))


if __name__ == "__main__":
    unittest.main()
