"""Parameter overrides of Smithery indicators in a rotation config.

``indicator.params`` = {key: {param: value}} overrides a version's parameters
wherever that key runs in the config. The indicator and parameter under test
are picked from the registry at run time — the public repo names none.
"""
import unittest

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import _trend_label
from rotation_lab.backtest.signals import indicator_overrides, trend_signal
from rotation_lab.config import IndicatorConfig
from rotation_lab.rpc import _build_config
from smithery.indicators import REGISTRY, VARIANTS, WARMUP_BARS
from smithery.variants import version_key


def _pick():
    """A quick base indicator with a general version and a whole-number
    parameter that has room inside its bounds: (key, name, new value)."""
    for key in sorted(REGISTRY, key=lambda k: (WARMUP_BARS.get(k, 10**9), k)):
        if key in VARIANTS or version_key(key, "optimized") not in REGISTRY:
            continue
        for name, entry in REGISTRY[key].schema().items():
            if entry["type"] != "int":
                continue
            default = int(entry["default"])
            for candidate in (default + max(1, default // 2), default - max(1, default // 2)):
                if entry.get("min", -np.inf) <= candidate <= entry.get("max", np.inf) and candidate > 1:
                    return key, name, candidate
    raise unittest.SkipTest("no indicator with a whole-number parameter in this library")


KEY, PARAM, VALUE = _pick()
GENERAL = version_key(KEY, "optimized")


def _frame(n: int) -> pd.DataFrame:
    rng = np.random.default_rng(7)
    close = np.exp(np.cumsum(rng.normal(0.0005, 0.02, n))) * 100
    index = pd.date_range("2020-01-01", periods=n, freq="D")
    return pd.DataFrame({"open": close * (1 + rng.normal(0, 0.003, n)), "high": close * 1.01,
                         "low": close * 0.99, "close": close, "volume": 1.0}, index=index)


FRAME = _frame(WARMUP_BARS.get(KEY, 300) + 500)


class WireTests(unittest.TestCase):
    def test_params_are_read_typed_and_inherited_by_the_total_signal(self):
        cfg = _build_config({"indicator": {"trend": KEY, "params": {KEY: {PARAM: float(VALUE)}}},
                             "marketFilter": True, "marketIndicator": {"trend": KEY}})
        self.assertEqual(cfg.indicator.params, {KEY: {PARAM: VALUE}})
        self.assertIsInstance(cfg.indicator.params[KEY][PARAM], int)
        self.assertEqual(cfg.market_indicator.params, {KEY: {PARAM: VALUE}})

    def test_a_config_without_params_has_none(self):
        cfg = _build_config({"indicator": {"trend": "aggregate", "aggregate": [KEY, GENERAL]}})
        self.assertEqual(cfg.indicator.params, {})
        self.assertEqual(_trend_label(cfg.indicator), "Aggregate (2)")

    def test_errors_name_the_indicator_and_the_parameter(self):
        with self.assertRaisesRegex(ValueError, f"{KEY}: no_such_param: unknown parameter"):
            _build_config({"indicator": {"trend": KEY, "params": {KEY: {"no_such_param": 1}}}})
        with self.assertRaisesRegex(ValueError, f"{KEY}: {PARAM}: expected a finite number"):
            _build_config({"indicator": {"trend": KEY, "params": {KEY: {PARAM: "ten"}}}})
        with self.assertRaisesRegex(ValueError, "must be an object"):
            _build_config({"indicator": {"trend": KEY, "params": [1, 2]}})

    def test_parameters_of_an_indicator_the_engine_does_not_know_are_kept(self):
        cfg = _build_config({"indicator": {"trend": "ema_cross", "params": {"gone_indicator": {"x": 1}}}})
        self.assertEqual(cfg.indicator.params, {"gone_indicator": {"x": 1}})


class SignalTests(unittest.TestCase):
    def test_no_params_runs_the_version_as_before(self):
        expected = (REGISTRY[KEY]().signal(FRAME) > 0).astype(int)
        pd.testing.assert_series_equal(trend_signal(FRAME, IndicatorConfig(trend=KEY)), expected, check_names=False)

    def test_an_override_equal_to_the_version_params_is_no_override(self):
        params = dict(REGISTRY[GENERAL]().params)
        self.assertEqual(indicator_overrides(GENERAL, params), {})
        cfg = IndicatorConfig(trend=GENERAL, params={GENERAL: params})
        pd.testing.assert_series_equal(trend_signal(FRAME, cfg), trend_signal(FRAME, IndicatorConfig(trend=GENERAL)))
        self.assertNotIn("(custom)", _trend_label(cfg))

    def test_an_override_runs_everywhere_the_key_runs_and_is_labeled(self):
        cfg = IndicatorConfig(trend=KEY, params={KEY: {PARAM: VALUE}})
        expected = (REGISTRY[KEY](**{PARAM: VALUE}).signal(FRAME) > 0).astype(int)
        pd.testing.assert_series_equal(trend_signal(FRAME, cfg), expected, check_names=False)
        self.assertTrue(_trend_label(cfg).endswith("(custom)"))
        both = IndicatorConfig(trend="aggregate", aggregate=(KEY, KEY), params={KEY: {PARAM: VALUE}})
        pd.testing.assert_series_equal(trend_signal(FRAME, both), expected, check_names=False)
        self.assertEqual(_trend_label(both), "Aggregate (1) (custom)")


if __name__ == "__main__":
    unittest.main()
