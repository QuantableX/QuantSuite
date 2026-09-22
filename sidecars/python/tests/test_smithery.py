import tempfile
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
import pandas as pd

from smithery.contract import TrendIndicator, validate_causality, validate_scale_invariance
from smithery.execution import evaluate, metrics
from smithery.evidence import certification_gates, write_json
from smithery.indicators import REGISTRY
from smithery.optimize import param_grid, walk_forward


def frame(n=600):
    rng = np.random.default_rng(2718)
    close = np.exp(np.cumsum(rng.normal(.0005, .02, n))) * 100
    return pd.DataFrame({"open": close, "high": close * 1.01, "low": close * .99,
                         "close": close, "volume": 100.0},
                        index=pd.date_range("2020-01-01", periods=n, freq="D", tz="UTC"))


class FixedSignal(TrendIndicator):
    name = "fixed"
    param_space = {"direction": (-1, 1)}

    @classmethod
    def default_params(cls):
        return {"direction": 1}

    def _compute(self, df):
        return np.full(len(df), self.params["direction"])


class ExecutionTests(unittest.TestCase):
    def test_close_signal_cannot_earn_the_gap_before_its_entry(self):
        df = frame(3)
        df["open"] = [100, 200, 200]
        df["close"] = [100, 200, 220]
        sig = pd.Series([1, 1, 1], index=df.index)
        result = evaluate(df, sig, cost_bps=0, return_series=True)
        self.assertAlmostEqual(result["returns"].iloc[0], 0)
        self.assertAlmostEqual(result["return"], .1)

    def test_existing_position_gets_gap_even_when_new_signal_exits(self):
        df = frame(4)
        df["open"] = [100, 100, 200, 200]
        df["close"] = [100, 100, 300, 200]
        sig = pd.Series([1, -1, -1, -1], index=df.index)
        self.assertAlmostEqual(evaluate(df, sig, cost_bps=0)["return"], 1.0)

    def test_short_is_not_inverse_price_log_return(self):
        df = frame(3)
        df["open"] = [100, 100, 50]
        df["close"] = [100, 50, 50]
        sig = pd.Series(-1, index=df.index)
        result = evaluate(df, sig, mode="long_short", cost_bps=0)
        self.assertAlmostEqual(result["return"], .5)  # not +100% from -log(.5)

    def test_entry_and_final_exit_pay_fees(self):
        df = frame(3)
        df[["open", "close"]] = 100
        sig = pd.Series(1, index=df.index)
        self.assertAlmostEqual(evaluate(df, sig, cost_bps=100)["return"], .995 ** 2 - 1)

    def test_short_ruin_never_recovers(self):
        df = frame(4)
        df["open"] = [100, 100, 300, 100]
        df["close"] = [100, 300, 100, 100]
        result = evaluate(df, pd.Series(-1, index=df.index), mode="long_short", cost_bps=0)
        self.assertTrue(result["bankrupt"])
        self.assertEqual(result["return"], -1)

    def test_drawdown_includes_initial_equity(self):
        result = metrics(pd.Series([-.1, 0.0], index=frame(2).index))
        self.assertAlmostEqual(result["max_dd"], -.1)

    def test_signal_alignment_errors_are_not_filled_silently(self):
        df = frame(5)
        with self.assertRaises(ValueError):
            evaluate(df, pd.Series(1, index=df.index[1:]))


class EvidenceTests(unittest.TestCase):
    def test_incomplete_or_low_power_runs_cannot_certify(self):
        axes = ("asset", "exchange", "parameter", "temporal", "monte_carlo")
        scores = {axis: 100 for axis in axes}
        counts = {"perm": 120, "boot": 200, "garch": 100}
        self.assertEqual(certification_gates(scores, .01, counts, fast=False, coverage=True, required_axes=axes), [])
        for missing in axes:
            with self.subTest(missing=missing):
                broken = {**scores, missing: np.nan}
                self.assertTrue(certification_gates(broken, .01, counts, fast=False, coverage=True, required_axes=axes))
        for kwargs in ({"fast": True, "coverage": True}, {"fast": False, "coverage": False}):
            self.assertTrue(certification_gates(scores, .01, counts, required_axes=axes, **kwargs))
        self.assertTrue(certification_gates(scores, .01, {"perm": 1, "boot": 1, "garch": 1},
                                           fast=False, coverage=True, required_axes=axes))
        self.assertTrue(certification_gates(scores, None, counts, fast=False, coverage=True, required_axes=axes))

    def test_artifacts_never_overwrite_and_emit_valid_json(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "run.json"
            write_json(path, {"missing": np.nan})
            self.assertIn('"missing": null', path.read_text())
            with self.assertRaises(FileExistsError):
                write_json(path, {"replacement": True})

    def test_gauntlet_does_not_reweight_around_missing_axes(self):
        from smithery.robustness import Gauntlet
        df = frame()
        with tempfile.TemporaryDirectory() as folder:
            with patch("smithery.robustness.OUTPUT_DIR", Path(folder)), \
                 patch("smithery.robustness.DOCS_DIR", Path(folder)), \
                 patch("smithery.robustness.find_primary", return_value="test"), \
                 patch("smithery.robustness.load", return_value=df):
                gauntlet = Gauntlet(FixedSignal())
                gauntlet.axis_validators = lambda _: "ok"
                gauntlet.axis_asset = lambda _: 100
                gauntlet.axis_exchange = lambda: np.nan
                gauntlet.axis_parameter = lambda _: 100
                gauntlet.axis_temporal = lambda _: 100
                def mc(_):
                    gauntlet.perm_p = .01
                    return 100
                gauntlet.axis_monte_carlo = mc
                result = gauntlet.run()
                self.assertEqual(result["score"], 80)
                self.assertFalse(result["certified"])
                self.assertNotIn("Battle-Ready", Path(result["report"]).read_text(encoding="utf-8"))


class ContractAndRecoveryTests(unittest.TestCase):
    def test_recovered_parameters_and_members(self):
        self.assertEqual(REGISTRY["ensemble_original"]().params["max_halflife"], 250)
        self.assertEqual(REGISTRY["ensemble_original"]().params["band"], .5)
        self.assertEqual(REGISTRY["council2_original"]().params["halflife"], 63)
        self.assertEqual(len(REGISTRY["council2_original"]()._members()), 4)
        self.assertEqual(REGISTRY["scale_original"]().params, {"band": .4, "detector_band": .3684})
        self.assertEqual(REGISTRY["ensemble"]().params["max_halflife"], 60)

    def test_short_prefixes_cannot_repaint(self):
        df = frame()
        for key in ("hilbert", "rankbreak", "bretrace", "scale", "robust",
                    "ensemble_original", "council2_original", "scale_original", "wavelet", "imm", "kalman", "rkalman"):
            with self.subTest(indicator=key):
                ind = REGISTRY[key]()
                self.assertTrue(validate_causality(ind, df)[0])
                self.assertTrue(validate_scale_invariance(ind, df)[0])
                self.assertEqual(len(ind.signal(df.iloc[:0])), 0)

    def test_hmm_first_em_fit_does_not_repaint(self):
        self.assertTrue(validate_causality(REGISTRY["hmm"](fit_window=100), frame(300))[0])

    def test_early_only_repainting_is_detected(self):
        class EarlyRepaint(FixedSignal):
            def _compute(self, df):
                return np.ones(len(df)) if len(df) > 64 else np.zeros(len(df))
        self.assertFalse(validate_causality(EarlyRepaint(), frame())[0])


class WalkForwardTests(unittest.TestCase):
    def test_defaults_are_included_in_the_search(self):
        class BetweenGrid(FixedSignal):
            param_space = {"direction": (.1, .9)}
            @classmethod
            def default_params(cls):
                return {"direction": .31415}
        self.assertIn({"direction": .31415}, param_grid(BetweenGrid()))

    def test_switching_cost_uses_actual_previous_fold_position_and_selected_is(self):
        df = frame(800)
        df[["open", "high", "low", "close"]] = 100
        def fake_backtest(task):
            ind, _, _, _ = task
            return {"sharpe": 4 if ind.params["direction"] == 1 else 2,
                    "max_dd": -.1, "cagr": .5, "sortino": 3, "omega": 1.5, "turnover_per_year": 0}
        with patch("smithery.optimize.param_grid", return_value=[{"direction": 1}, {"direction": -1}]), \
             patch("smithery.optimize._medoid", side_effect=[{"direction": 1}, {"direction": -1}]), \
             patch("smithery.optimize.par.pmap", side_effect=lambda fn, tasks: [fake_backtest(t) for t in tasks]):
            result = walk_forward(FixedSignal(), df, 100, n_folds=2, purge=60, mode="long_short")
        self.assertAlmostEqual(result["oos_total_log_ret"], 2*np.log1p(-.005) + np.log1p(-.01))
        self.assertEqual(result["is_sharpe_mean"], 3)
        self.assertEqual(result["folds"][1]["is_selected_sharpe"], 2)
        self.assertLess(result["folds"][0]["train_end"], result["folds"][0]["test_start"])

    def test_insufficient_history_and_purge_are_rejected(self):
        with self.assertRaises(ValueError):
            walk_forward(FixedSignal(), frame(100), 20)
        with self.assertRaises(ValueError):
            walk_forward(FixedSignal(), frame(), 20, purge=0)


class IntegrationTests(unittest.TestCase):
    def test_generated_strategy_preserves_json_params_and_waits_for_warmup(self):
        source = (Path(__file__).resolve().parents[3] / 'modules/algo/crate/src/strategies.rs').read_text(encoding='utf-8')
        template = source.split('const REGIME_TREND_TEMPLATE: &str = r#"', 1)[1].split('"#;', 1)[0]
        params = {"band": .2, "nullable": None, "boolean": True, "text": 'quote " slash \\'}
        code = template.replace('__CLASS_NAME__', 'Generated').replace('__INDICATOR_NAME__', 'RobustConsensus') \
            .replace('__INDICATOR_KEY__', 'robust').replace('__WARMUP_BARS__', '600') \
            .replace('__INDICATOR_PARAMS__', f"__import__('json').loads({json.dumps(json.dumps(params))})")
        namespace = {}
        exec(compile(code, '<generated-strategy>', 'exec'), namespace)
        strategy = namespace['Generated']()
        self.assertEqual(strategy.params['indicator_params'], params)
        with patch.object(strategy, 'regime', return_value=0) as regime:
            strategy._candle_history = [None] * 599
            strategy.on_candle(None)
            regime.assert_not_called()
            strategy._candle_history.append(None)
            strategy.on_candle(None)
            regime.assert_called_once_with('robust', params)

    def test_bot_and_lces_use_the_same_recovered_signals(self):
        from quantalgo.regime import regime_series
        from rotation_lab.backtest.signals import trend_signal
        from rotation_lab.config import IndicatorConfig
        df = frame(1100)
        candles = [{"time": str(index), **row.to_dict()} for index, row in df.iterrows()]
        for key in ("ensemble_original", "council2_original", "scale_original", "robust"):
            with self.subTest(key=key):
                expected = REGISTRY[key]().signal(df).to_numpy()
                np.testing.assert_array_equal(regime_series(candles, key), expected)
                np.testing.assert_array_equal(trend_signal(df, IndicatorConfig(trend=key)), expected > 0)

    def test_latest_matching_failed_run_replaces_pass_but_foreign_code_does_not(self):
        from smithery import registry, evidence, data
        with tempfile.TemporaryDirectory() as directory, patch.object(data, "OUTPUT_DIR", Path(directory)), \
                patch.object(evidence, "code_fingerprint", return_value="test-code"):
            base = {"kind": "gauntlet", "version": evidence.EVALUATION_VERSION,
                    "code_sha256": "test-code", "params": REGISTRY["robust"]().params,
                    "name": REGISTRY["robust"].name, "timeframe": "1d", "fast": False,
                    "score": 88, "grade": "S", "perm_p": .01, "certified": True}
            write_json(Path(directory) / "2026-09-08 first.json", base)
            registry._current_runs.cache_clear()
            self.assertTrue(registry.verdict_for("robust")["certified"])
            write_json(Path(directory) / "2026-09-08 second.json", {**base, "certified": False, "reasons": ["coverage"]})
            registry._current_runs.cache_clear()
            self.assertFalse(registry.verdict_for("robust")["certified"])
            write_json(Path(directory) / "2026-09-08 third.json", {**base, "code_sha256": "old-code"})
            registry._current_runs.cache_clear()
            self.assertFalse(registry.verdict_for("robust")["certified"])
        registry._current_runs.cache_clear()


class ComparisonTests(unittest.TestCase):
    def test_positive_sharpe_is_insufficient_when_compounded_return_is_negative(self):
        from smithery.research import _profitable
        self.assertFalse(_profitable({"sharpe": .3, "return": -.05, "bankrupt": False}))
        self.assertTrue(_profitable({"sharpe": .3, "return": .05, "bankrupt": False}))

    def test_selection_is_locked_before_holdout_and_cannot_use_its_returns(self):
        from smithery import research

        class Negative(FixedSignal):
            @classmethod
            def default_params(cls):
                return {"direction": -1}

        df = frame(1600)
        for col in ("open", "high", "low", "close"):
            df[col] = 100 * np.exp(np.arange(len(df)) * .001)
        cutoff = df.index[0] + (df.index[-1] - df.index[0]) * .75
        winners = []
        real_cell = research._cell
        for holdout_direction in (1, -1):
            changed = df.copy()
            mask = changed.index >= cutoff
            offset = np.arange(mask.sum())
            for col in ("open", "high", "low", "close"):
                changed.loc[mask, col] = df.loc[mask, col].iloc[0] * np.exp(offset * .003 * holdout_direction)
            with tempfile.TemporaryDirectory() as directory:
                def checked_cell(*args):
                    if args[5] == "holdout":
                        paths = list(Path(directory).glob("*.selection.json"))
                        self.assertEqual(len(paths), 1)
                        selection = json.loads(paths[0].read_text())
                        self.assertTrue(selection["holdout_not_evaluated"])
                        self.assertEqual(args[2], changed.index.searchsorted(cutoff))
                    return real_cell(*args)
                with patch.object(research, "OUTPUT_DIR", Path(directory)), \
                        patch.object(research, "REGISTRY", {"positive": FixedSignal, "negative": Negative}), \
                        patch.object(research, "WARMUP_BARS", {"positive": 800, "negative": 800}), \
                        patch.object(research, "_frames", return_value={"BINANCE_BTCUSDT_1d": changed}), \
                        patch.object(research, "find_primary", return_value="BINANCE_BTCUSDT_1d"), \
                        patch.object(research, "_cell", side_effect=checked_cell):
                    result = research.compare(["positive", "negative"], timeframes=["1d"])
                winners.append(result["winner"])
                self.assertFalse(result["certified"])
                self.assertGreaterEqual(pd.Timestamp(result["validation_start"]), changed.index[800])
                self.assertTrue(Path(result["report"]).exists())
                self.assertTrue(all(cell["era"] != "holdout" for row in result["rows"] for cell in row["development"]))
        self.assertEqual(winners, ["positive", "positive"])


if __name__ == "__main__":
    unittest.main()
