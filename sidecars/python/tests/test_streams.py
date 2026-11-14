"""GPU jobs side by side on CUDA streams: the same results as one after another, in order."""
import os
import unittest
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery import batch as B
from smithery import parallel as par
from smithery import streams
from smithery.contract import TrendIndicator
from smithery.jit import launch, njit, prange


@njit
def _state_rows(c, band, sig):
    # a bar-by-bar state machine (the kind that leaves the GPU idle with one series per call)
    for s in prange(c.shape[0]):
        cur = 0.0
        anchor = c[s, 0]
        for t in range(c.shape[1]):
            x = c[s, t]
            if x > anchor * (1.0 + band):
                cur = 1.0
                anchor = x
            elif x < anchor * (1.0 - band):
                cur = -1.0
                anchor = x
            sig[s, t] = cur


class _Machine(TrendIndicator):
    name = "StreamTestMachine"
    param_space = {"band": (0.005, 0.05)}

    @classmethod
    def default_params(cls):
        return {"band": 0.02}

    def _compute_batch(self, b):
        xp = B.namespace(b.close)
        c = xp.ascontiguousarray(b.close.a)
        sig = xp.zeros(c.shape)
        launch(_state_rows, c, float(self.params["band"]), sig)
        # warm-up: 0 until the first move, then never 0 again (the contract's law)
        return b.k.col(sig)


def _frame(seed, start, bars=600):
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(rng.normal(0, 0.02, bars)))
    index = pd.date_range(start, periods=bars, freq="D")
    return pd.DataFrame({"open": close, "high": close * 1.01, "low": close * 0.99, "close": close,
                         "volume": np.ones(bars)}, index=index)


class PlainLoopTests(unittest.TestCase):
    def test_one_stream_or_no_gpu_is_the_plain_loop(self):
        calls = []
        with patch.dict(os.environ, {"SMITHERY_GPU_STREAMS": "1"}):
            self.assertEqual(streams.gpu_map(lambda x: calls.append(x) or x * 2, [1, 2, 3]), [2, 4, 6])
        self.assertEqual(calls, [1, 2, 3])
        with patch.object(streams, "_cuda", return_value=False):
            with self.assertRaises(KeyError):
                streams.gpu_map(lambda x: {}[x], ["a", "b"])

    def test_odd_settings_fall_back_to_the_default_width(self):
        with patch.dict(os.environ, {"SMITHERY_GPU_STREAMS": "many"}):
            self.assertEqual(streams.width(), streams.DEFAULT_STREAMS)
        with patch.dict(os.environ, {"SMITHERY_GPU_STREAMS": "0"}):
            self.assertEqual(streams.width(), 1)


@unittest.skipUnless(B.gpu_available(), "needs PyTorch with CUDA")
class SideBySideTests(unittest.TestCase):
    def test_results_in_order_and_the_first_error_wins(self):
        import torch

        def job(x):
            if x in (5, 9):
                raise ValueError(f"job {x}")
            a = torch.full((64,), float(x), dtype=torch.float64, device="cuda")
            return float((a * a).sum().item())

        self.assertEqual(streams.gpu_map(job, range(5)), [64.0 * x * x for x in range(5)])
        with self.assertRaisesRegex(ValueError, "job 5"):
            streams.gpu_map(job, range(12))

    def test_a_job_inside_a_job_runs_its_own_jobs_in_order(self):
        seen = []

        def inner(x):
            seen.append(streams.inside())
            return x

        out = streams.gpu_map(lambda x: streams.gpu_map(inner, [x, x + 1]), [0, 10, 20])
        self.assertEqual(out, [[0, 1], [10, 11], [20, 21]])
        self.assertTrue(all(seen))

    def test_variants_and_time_grids_give_the_same_votes_as_one_after_another(self):
        frames = [_frame(1, "2020-01-01"), _frame(2, "2020-01-01"), _frame(3, "2020-03-01"),
                  _frame(4, "2021-01-01", 400), _frame(5, "2019-06-01", 800)]
        variants = [_Machine(band=b) for b in (0.006, 0.01, 0.015, 0.02, 0.03, 0.045)]
        with patch.dict(os.environ, {"SMITHERY_GPU_STREAMS": "1"}):
            alone = par.signals_table(variants, frames)
        together = par.signals_table(variants, frames)
        cpu = [v.signal(df).to_numpy() for v in variants for df in frames]
        self.assertEqual(len(together), len(cpu))
        for a, b, c in zip(alone, together, cpu):
            np.testing.assert_array_equal(a, b)
            np.testing.assert_array_equal(b, c)
        # one variant on frames of several grids: the grids side by side inside batch.signals
        stats = {}
        grids = B.signals(variants[0], frames, device="gpu", stats=stats)
        for got, df in zip(grids, frames):
            np.testing.assert_array_equal(got, variants[0].signal(df).to_numpy())
        self.assertEqual((stats["gpu_series"], stats["retried"]), (len(frames), 0))


if __name__ == "__main__":
    unittest.main()
