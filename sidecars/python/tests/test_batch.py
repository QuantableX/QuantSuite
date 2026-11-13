"""The batch columns against pandas: bit for bit on the CPU, within float rounding on the GPU."""
import unittest

import numpy as np
import pandas as pd

from smithery import batch as B
from smithery.contract import hysteresis_flip


def _series(seed: int, T: int = 700) -> np.ndarray:
    rng = np.random.default_rng(seed)
    x = 100 * np.exp(np.cumsum(rng.normal(0, 0.03, T)))
    x[:5] = np.nan                       # warm-up NaNs from a diff/shift upstream
    x[300:310] = x[299]                  # a flat stretch: pandas' same-value rules
    x[400] = np.nan                      # a hole
    x[500:520] = -x[500:520] / 50        # a negative stretch
    return x


def _data(S: int = 6, T: int = 700) -> np.ndarray:
    return np.stack([_series(s, T) for s in range(S)])


def _pandas_rows(a: np.ndarray, fn) -> np.ndarray:
    return np.stack([fn(pd.Series(row)).to_numpy(dtype=float) for row in a])


CASES = {
    "rolling_mean_20": lambda s: s.rolling(20).mean(),
    "rolling_sum_7": lambda s: s.rolling(7).sum(),
    "rolling_std_30": lambda s: s.rolling(30).std(),
    "rolling_var_5_mp3": lambda s: s.rolling(5, min_periods=3).var(),
    "rolling_max_14": lambda s: s.rolling(14).max(),
    "rolling_min_9_mp1": lambda s: s.rolling(9, min_periods=1).min(),
    "rolling_mean_1": lambda s: s.rolling(1).mean(),
    "rolling_median_11": lambda s: s.rolling(11).median(),
    "rolling_quantile_25": lambda s: s.rolling(25).quantile(0.8),
    "ewm_span_adjust_false": lambda s: s.ewm(span=12, adjust=False, min_periods=12).mean(),
    "ewm_halflife": lambda s: s.ewm(halflife=10).mean(),
    "ewm_alpha_ignore_na": lambda s: s.ewm(alpha=0.2, ignore_na=True).mean(),
    "ewm_std": lambda s: s.ewm(halflife=8).std(),
    "shift_3": lambda s: s.shift(3),
    "diff_1": lambda s: s.diff(),
    "pct_change_2": lambda s: s.pct_change(2),
    "cumsum": lambda s: s.cumsum(),
    "cummax": lambda s: s.cummax(),
    "ffill": lambda s: s.ffill(),
    "clip": lambda s: s.clip(lower=50, upper=150),
    "where": lambda s: s.where(s > 100, -1.0),
    "fillna": lambda s: s.fillna(0.0),
}


def _batch_case(name: str, col: B.Col) -> B.Col:
    fn = {
        "rolling_mean_20": lambda c: c.rolling(20).mean(),
        "rolling_sum_7": lambda c: c.rolling(7).sum(),
        "rolling_std_30": lambda c: c.rolling(30).std(),
        "rolling_var_5_mp3": lambda c: c.rolling(5, min_periods=3).var(),
        "rolling_max_14": lambda c: c.rolling(14).max(),
        "rolling_min_9_mp1": lambda c: c.rolling(9, min_periods=1).min(),
        "rolling_mean_1": lambda c: c.rolling(1).mean(),
        "rolling_median_11": lambda c: c.rolling(11).median(),
        "rolling_quantile_25": lambda c: c.rolling(25).quantile(0.8),
        "ewm_span_adjust_false": lambda c: c.ewm(span=12, adjust=False, min_periods=12).mean(),
        "ewm_halflife": lambda c: c.ewm(halflife=10).mean(),
        "ewm_alpha_ignore_na": lambda c: c.ewm(alpha=0.2, ignore_na=True).mean(),
        "ewm_std": lambda c: c.ewm(halflife=8).std(),
        "shift_3": lambda c: c.shift(3),
        "diff_1": lambda c: c.diff(),
        "pct_change_2": lambda c: c.pct_change(2),
        "cumsum": lambda c: c.cumsum(),
        "cummax": lambda c: c.cummax(),
        "ffill": lambda c: c.ffill(),
        "clip": lambda c: c.clip(lower=50, upper=150),
        "where": lambda c: c.where(c > 100, -1.0),
        "fillna": lambda c: c.fillna(0.0),
    }[name]
    return fn(col)


# Rolling var/std are computed from each window (exact) since 2026-11-12; pandas' running sums
# drift on series that cross orders of magnitude. SMITHERY_PANDAS_MOMENTS=1 restores pandas.
MOMENTS = ("rolling_std_30", "rolling_var_5_mp3")


class CpuIsPandas(unittest.TestCase):
    def test_every_case_bit_for_bit(self):
        a = _data()
        for name, fn in CASES.items():
            with self.subTest(name):
                expected = _pandas_rows(a, fn)
                got = _batch_case(name, B.Col(a.copy(), B.NUMPY)).to_numpy()
                if name in MOMENTS:
                    np.testing.assert_array_equal(np.isnan(got), np.isnan(expected))
                    np.testing.assert_allclose(got, expected, rtol=1e-9, atol=1e-10, equal_nan=True)
                else:
                    np.testing.assert_array_equal(got, expected)

    def test_pandas_moments_mode_is_pandas(self):
        import os
        a = _data()
        os.environ["SMITHERY_PANDAS_MOMENTS"] = "1"
        try:
            for name in MOMENTS:
                with self.subTest(name):
                    got = _batch_case(name, B.Col(a.copy(), B.NUMPY)).to_numpy()
                    np.testing.assert_array_equal(got, _pandas_rows(a, CASES[name]))
        finally:
            del os.environ["SMITHERY_PANDAS_MOMENTS"]

    def test_moments_are_exact_across_a_crash(self):
        import math
        a = _data()
        a[3, 450:] *= 1e-5                      # a coin that loses five orders of magnitude
        n = 10
        got = B.Col(a.copy(), B.NUMPY).rolling(n).std(ddof=0).to_numpy()
        for i in range(460, 700, 7):
            w = a[3, i - n + 1:i + 1]
            if np.isfinite(w).all() and w.max() != w.min():
                m = math.fsum(w) / n
                want = math.sqrt(math.fsum((w - m) ** 2) / n)
                self.assertAlmostEqual(got[3, i] / want, 1.0, places=12)

    def test_ufuncs_and_np_where(self):
        a = _data()
        c = B.Col(a, B.NUMPY)
        got = np.where(np.log(c) > 4.6, 1.0, np.where(np.log(c) < 4.5, -1.0, np.nan)).to_numpy()
        with np.errstate(all="ignore"):
            want = np.where(np.log(a) > 4.6, 1.0, np.where(np.log(a) < 4.5, -1.0, np.nan))
        np.testing.assert_array_equal(got, want)

    def test_hysteresis_is_the_contract_machine(self):
        rng = np.random.default_rng(3)
        s = rng.normal(0, 1, (8, 500))
        s[:, :40] = np.nan
        s[2, 100:120] = np.inf
        kit = B.Kit(B.NUMPY, s.shape)
        for band in (0.0, 0.3, 1.5):
            got = kit.hysteresis(B.Col(s, B.NUMPY), band).to_numpy()
            want = np.stack([hysteresis_flip(r, band) for r in s])
            np.testing.assert_array_equal(got, want)

    def test_max_of_skips_nan_like_concat_max(self):
        rng = np.random.default_rng(5)
        a, b, c = (rng.normal(0, 1, (3, 50)) for _ in range(3))
        b[:, 0] = np.nan
        a[1, 3] = b[1, 3] = c[1, 3] = np.nan
        kit = B.Kit(B.NUMPY, a.shape)
        got = kit.max_of(B.Col(a, B.NUMPY), B.Col(b, B.NUMPY), B.Col(c, B.NUMPY)).to_numpy()
        want = np.stack([pd.concat([pd.Series(a[i]), pd.Series(b[i]), pd.Series(c[i])], axis=1).max(axis=1)
                         for i in range(3)])
        np.testing.assert_array_equal(got, want)


@unittest.skipUnless(B.gpu_available(), "needs PyTorch with CUDA")
class GpuIsCpu(unittest.TestCase):
    def test_every_case_close(self):
        a = _data()
        gpu = B.ops_for("gpu")
        for name in CASES:
            with self.subTest(name):
                want = _batch_case(name, B.Col(a.copy(), B.NUMPY)).to_numpy()
                got = _batch_case(name, B.Col(gpu.asarray(a), gpu)).to_numpy()
                np.testing.assert_array_equal(np.isnan(got), np.isnan(want))
                # pandas' online variance keeps a residue (~1e-11) on a flat window; the GPU gives 0
                atol = 1e-10 if "var" in name else 1e-12
                np.testing.assert_allclose(got, want, rtol=1e-9, atol=atol, equal_nan=True)

    def test_hysteresis_exact(self):
        rng = np.random.default_rng(4)
        s = rng.normal(0, 1, (8, 500))
        s[:, :40] = np.nan
        gpu = B.ops_for("gpu")
        got = B.Kit(gpu, s.shape).hysteresis(B.Col(gpu.asarray(s), gpu), 0.4).to_numpy()
        want = np.stack([hysteresis_flip(r, 0.4) for r in s])
        np.testing.assert_array_equal(got, want)


if __name__ == "__main__":
    unittest.main()


class GpuFirstLintTests(unittest.TestCase):
    """quantscript lint warns about scripts that are not written for the batch (GPU-first rule)."""

    def _messages(self, source):
        import tempfile
        from pathlib import Path
        from smithery.quantscript import lint
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "probe.py"
            path.write_text(source, encoding="utf-8")
            return [m["message"] for m in lint(path)["markers"]]

    def test_one_series_compute_is_flagged(self):
        src = ("from ..contract import TrendIndicator\n\n\nclass Probe(TrendIndicator):\n    name = 'Probe'\n\n"
               "    def _compute(self, df):\n        return df.close * 0\n\n\nREGISTER = {'probe': Probe}\n")
        self.assertTrue(any("_compute_batch" in m for m in self._messages(src)))

    def test_bar_loop_in_batch_method_is_flagged(self):
        src = ("from ..contract import TrendIndicator\n\n\nclass Probe(TrendIndicator):\n    name = 'Probe'\n\n"
               "    def _compute_batch(self, b):\n        x = b.close.to_numpy()\n"
               "        for i in range(x.shape[1]):\n            x[:, i] = 0\n        return x\n\n\nREGISTER = {'probe': Probe}\n")
        self.assertTrue(any("per-bar Python loop" in m for m in self._messages(src)))

    def test_batched_research_script_is_clean(self):
        src = ("from . import research_common as rc\n\n\nclass Probe(rc.ResearchTrend):\n    name = 'Probe'\n"
               "    batched = True\n\n    def score(self, b):\n        return rc.sma(b.close, 5) - b.close\n\n\n"
               "REGISTER = {'probe': Probe}\n")
        self.assertFalse(any("GPU-first" in m for m in self._messages(src)))


class DeviceLineTests(unittest.TestCase):
    """Jobs say where indicator series ran (GPU-first rule): never a silent CPU fallback."""

    def test_cpu_series_name_their_reason(self):
        before = B.tally()
        B._count("cpu", 3, "compiled per-row kernels (gpu = False)")
        self.assertEqual(B.device_line(before), "CPU: compiled per-row kernels (gpu = False) 3")

    def test_nothing_computed(self):
        self.assertEqual(B.device_line(B.tally()), "no indicator series computed")

    def test_unbatched_script_is_labelled(self):
        class Old:
            gpu = True

            @classmethod
            def has_batch(cls):
                return False
        self.assertEqual(B.device_label(Old()), "CPU: no batch form (one series at a time)")


class EwmRowsTests(unittest.TestCase):
    """The compiled ewm (the GPU's fallback for gappy series) is pandas' recursion bit for bit."""

    def test_kernel_is_pandas(self):
        a = _data()
        a[1, 200:204] = np.nan
        for spec in (dict(span=12, adjust=False), dict(alpha=0.2, adjust=False, min_periods=5),
                     dict(alpha=0.3, ignore_na=True), dict(halflife=7, adjust=False), dict(com=2.5)):
            with self.subTest(spec=spec):
                want = _pandas_rows(a, lambda s: s.ewm(**spec).mean())
                out = np.empty_like(a)
                B._ewm_rows(np.ascontiguousarray(a), B._ewm_alpha(spec), bool(spec.get("adjust", True)),
                            bool(spec.get("ignore_na", False)), max(int(spec.get("min_periods", 0) or 0), 1), out)
                np.testing.assert_array_equal(out, want)


@unittest.skipUnless(B.gpu_available(), "needs PyTorch with CUDA")
class GpuExactTests(unittest.TestCase):
    def test_gappy_ewm_is_exact(self):
        a = _data()          # a hole at bar 400: the scans cannot take it, the kernel does
        gpu = B.ops_for("gpu")
        want = _pandas_rows(a, lambda s: s.ewm(span=12, adjust=False).mean())
        got = B.Col(gpu.asarray(a), gpu).ewm(span=12, adjust=False).mean().to_numpy()
        np.testing.assert_array_equal(got, want)

    def test_mean_of_booleans_divides_like_numpy(self):
        rng = np.random.default_rng(9)
        w = rng.normal(0, 1, (50, 40))
        gpu = B.ops_for("gpu")
        xp = B._TorchNP(gpu)
        want = np.mean(w[:, :20, None] <= np.sort(w, axis=1)[:, None, :], axis=1)
        t = gpu.asarray(w)
        got = xp.mean(t[:, :20, None] <= xp.sort(t, axis=1)[:, None, :], axis=1)
        np.testing.assert_array_equal(xp.asnumpy(got), want)
