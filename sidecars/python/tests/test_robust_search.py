"""Card 3 (docs/ROBUSTNESS.md): the search's pool gate, worst-world scoring, windows and walk-forward."""
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery import robust_search as rs
from smithery.lab_fast import _eval_local


def dates(years, start="2020-01-01"):
    return pd.date_range(start, periods=int(365.25 * years), freq="D", tz="UTC")


class Windows(unittest.TestCase):
    def test_blind_window_is_the_last_two_years(self):
        d = dates(6.8)
        cut = rs.default_cut(d)
        self.assertEqual((d[-1] - d[cut]).days, 730)
        self.assertEqual(rs.default_cut(d, blind_days=365), int(d.searchsorted(d[-1] - pd.Timedelta(days=365))))

    def test_a_short_history_keeps_a_third_blind(self):
        d = dates(3)
        cut = rs.default_cut(d)
        self.assertAlmostEqual((d[-1] - d[cut]).days / (d[-1] - d[0]).days, 1 / 3, delta=0.01)
        with self.assertRaises(ValueError):
            rs.default_cut(dates(0.2))


class PoolGate(unittest.TestCase):
    def test_only_current_certifications_enter(self):
        with patch("smithery.registry.robust_on_track", side_effect=lambda k, tf: k in {"a_opt", "c_opt_1d"}):
            kept, excluded = rs.pool_gate(["a_opt", "b_opt", "c_opt_1d"], "1d")
        self.assertEqual(kept, ["a_opt", "c_opt_1d"])
        self.assertEqual(excluded, ["b_opt"])

    def test_a_recorded_pool_is_used_as_is(self):
        # A frozen run cannot see the user's evidence; the list recorded at creation decides.
        with patch("smithery.registry.robust_on_track", side_effect=AssertionError("no lookup in a frozen run")):
            kept, excluded = rs.pool_gate(["a_opt", "b_opt", "c_opt_1d"], "1d", certified=["b_opt", "z_opt"])
        self.assertEqual(kept, ["b_opt"])
        self.assertEqual(excluded, ["a_opt", "c_opt_1d"])


class FakeSystem:
    def __init__(self, factor):
        self.factor, self.failed = factor, {}

    def evaluate(self, trials, delay=0, end=None):
        return [(self.factor * (1 + len(t)) / (1 + delay), 10.0 * self.factor + delay) for t in trials]


class WorstWorld(unittest.TestCase):
    def worlds(self):
        w = rs.Worlds.__new__(rs.Worlds)
        w.systems = {"saved": FakeSystem(3.0), "noise 1": FakeSystem(2.0), "noise 2": FakeSystem(2.5)}
        w.gpu, w.dates = {}, dates(5)
        return w

    def test_a_candidate_scores_its_worst_world(self):
        out = self.worlds().evaluate([("a", "b"), ("a",)], end=100)
        # capital: min of 3*3, 2*3, 2.5*3 and the delayed saved world 3*3/2 -> 4.5; drawdown: max of 30, 20, 25, 31
        self.assertEqual(out[0], (4.5, 31.0))
        self.assertEqual(out[1], (3.0, 31.0))

    def test_unusable_members_score_nothing(self):
        w = self.worlds()
        w.systems["noise 2"].failed = {"bad": "error"}
        self.assertEqual(w.evaluate([("bad", "a")], end=100), [None])


class WalkForward(unittest.TestCase):
    def test_leaders_must_beat_random_aggregates_on_the_blind_window(self):
        d = dates(5)
        cut = len(d) - 400

        class Curves:
            dates = d

            def curves(self, trials):
                # growth after the cut by the trial's first member: 'good' doubles, others creep
                out = []
                for t in trials:
                    g = 2.0 if "good" in t else 1.05
                    curve = np.ones(len(d))
                    curve[cut:] = np.linspace(1, g, len(d) - cut)
                    out.append(curve)
                arr = np.array(out)
                return {"saved": arr, "noise 1": arr * 1.0, "one day later": arr}

        pool = ["good", "x1", "x2", "x3", "x4", "x5"]
        bases = {k: k for k in pool}
        res = rs.walk_forward(Curves(), [(("good", "x1"), (5.0, 20.0)), (("x2", "x3"), (6.0, 20.0))], pool, bases, cut, seed=1)
        good, plain = res["leaders"]
        self.assertTrue(good["passed"])
        self.assertGreater(good["percentile"], 50)
        self.assertFalse(plain["passed"])
        self.assertEqual(res["passed"], 1)
        self.assertEqual(res["cut"], str(d[cut].date()))


class EvaluateWindow(unittest.TestCase):
    def test_metrics_stop_at_the_window_end(self):
        curve = np.array([[1.0, 2.0, 4.0, 1.0]])
        system = SimpleNamespace(simulate=lambda trials, delay=0: (curve, None))
        self.assertEqual(_eval_local(system, [("a",)], end=3), [(4.0, 0.0)])
        self.assertEqual(_eval_local(system, [("a",)])[0][0], 1.0)


if __name__ == "__main__":
    unittest.main()
