"""The vote store: a run that reuses another run's indicator votes gets exactly the votes it would compute.

The System Gauntlet's base run writes the store; the variants that keep its candles and members' parameters
(start shift, fees, system settings, universe) read it. Everything else never touches it."""

import datetime as dt
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest import signals, vote_store


def frame(seed=1, start="2018-01-01", end="2021-12-31"):
    index = pd.date_range(start, end, freq="D")
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(0.001 + 0.03 * rng.standard_normal(len(index))))
    return pd.DataFrame({"open": close, "high": close * 1.01, "low": close * 0.99, "close": close, "volume": 1.0}, index=index)


class VoteStoreTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(folder.cleanup)
        self.addCleanup(vote_store.close_all)
        self.folder = Path(folder.name)
        self.calls = []

        def indicator(df, kind, cfg=None):
            self.calls.append((df.index[0], df.index[-1]))
            fast, slow = df["close"].rolling(5).mean(), df["close"].rolling(20).mean()
            return pd.Series(np.where(fast > slow, 1, np.where(fast < slow, -1, 0)), index=df.index, name=kind)

        patches = [patch.object(signals, "_smithery", side_effect=indicator),
                   patch.dict("rotation_lab.backtest.smithery.WARMUP_BARS", {"k": 30})]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def votes(self, df, mode=None):
        env = {} if mode is None else {vote_store.ENV_FOLDER: str(self.folder), vote_store.ENV_MODE: mode}
        with patch.dict(os.environ, env):
            if mode is None:
                os.environ.pop(vote_store.ENV_FOLDER, None)
            return signals.member_signal(df, "k")

    def test_votes_from_the_store_are_the_votes_computed(self):
        df = frame()
        plain = self.votes(df)
        blocks = len(self.calls)
        self.assertGreater(blocks, 3)
        written = self.votes(df, "write")
        self.calls.clear()
        read = self.votes(df, "read")
        self.assertEqual(self.calls, [])  # every block came from the store
        for out in (written, read):
            pd.testing.assert_series_equal(out, plain, check_exact=True)

    def test_a_later_start_reads_the_blocks_it_shares(self):
        df = frame()
        plain = self.votes(df, "write")
        # A run from a later day fetches the warm-up before its block: that block onward is the base run's windows.
        block = signals.block_start(pd.Timestamp("2020-03-01"))
        later = df.loc[df.index >= block - pd.Timedelta(days=30)]
        self.calls.clear()
        out = self.votes(later, "read")
        self.assertEqual(len(self.calls), 1)  # only the cut-off block before it
        pd.testing.assert_series_equal(out, self.votes(later), check_exact=True)
        pd.testing.assert_series_equal(out[out.index >= block], plain[plain.index >= block], check_exact=True)

    def test_a_changed_candle_recomputes_only_its_block_and_read_mode_keeps_nothing(self):
        df = frame()
        self.votes(df, "write")
        changed = df.copy()
        changed.loc["2020-06-15", "close"] *= 1.5
        self.calls.clear()
        out = self.votes(changed, "read")
        self.assertEqual(len(self.calls), 1)  # only the block holding 2020-06-15
        pd.testing.assert_series_equal(out, self.votes(changed), check_exact=True)
        # Read mode kept nothing: the changed block is computed again.
        self.calls.clear()
        self.votes(changed, "read")
        self.assertEqual(len(self.calls), 1)

    def test_other_parameters_or_another_indicator_never_hit(self):
        df = frame().iloc[:400]
        base = vote_store.window_key("k", {}, df)
        self.assertNotEqual(base, vote_store.window_key("k", {"length": 21}, df))
        self.assertNotEqual(base, vote_store.window_key("other", {}, df))
        self.assertEqual(base, vote_store.window_key("k", {}, df.copy()))
        shifted = df.copy()
        shifted.index = shifted.index + pd.Timedelta(days=1)
        self.assertNotEqual(base, vote_store.window_key("k", {}, shifted))

    def test_without_the_variable_nothing_is_stored(self):
        self.votes(frame())
        self.assertFalse((self.folder / "votes.sqlite").exists())


class GauntletJobsTests(unittest.TestCase):
    def test_only_variants_on_the_base_run_s_candles_and_parameters_read_the_store(self):
        from smithery import system_gauntlet as sg
        from rotation_lab.rpc import _build_config
        config = _build_config({"mode": "rotation", "topN": 5, "startDate": "2020-01-01", "endDate": "2024-01-01",
                                "includeUsd": True, "feeRate": 0.001, "marketFilter": True,
                                "marketIndicator": {"trend": "total_breakout", "trendLength": 100, "entryLength": 5, "exitLength": 5},
                                "indicator": {"trend": "ema_cross"}})
        captured = {}

        class Stop(Exception):
            pass

        def fake_run_native(jobs, workers=None, progress=None, stage=""):
            if not captured:
                captured["base"] = jobs
                return [{"label": "base", "multiple": 2.0, "years": 4.0, "cagr": 0.19, "max_drawdown_pct": 30.0}]
            captured["variants"] = jobs
            raise Stop()

        with patch.object(sg, "run_native", side_effect=fake_run_native), \
                patch("smithery.registry.describe_all", return_value={"indicators": []}):
            with self.assertRaises(Stop):
                sg.run(config, data_dir=Path(tempfile.gettempdir()), fast=True)
        base = captured["base"][0]
        self.assertEqual(base[4][1], "write")
        reading = {job[0].split("|")[0] for job in captured["variants"] if job[4]}
        self.assertEqual(reading, {"start_shift", "fees", "system_settings", "universe"})
        for job in captured["variants"]:
            if job[4]:
                self.assertEqual((job[4][0], job[4][1]), (base[4][0], "read"))
                self.assertIsNone(job[2])  # never with changed prices
        self.assertTrue({"parameter_jitter", "price_noise", "permutation"} <= {job[0].split("|")[0] for job in captured["variants"]})
        # The off switch: no store at all.
        captured.clear()
        with patch.dict(os.environ, {"SMITHERY_VOTE_REUSE": "0"}), patch.object(sg, "run_native", side_effect=fake_run_native), \
                patch("smithery.registry.describe_all", return_value={"indicators": []}):
            with self.assertRaises(Stop):
                sg.run(config, data_dir=Path(tempfile.gettempdir()), fast=True)
        self.assertIsNone(captured["base"][0][4])
        self.assertFalse(any(job[4] for job in captured["variants"]))


if __name__ == "__main__":
    unittest.main()
