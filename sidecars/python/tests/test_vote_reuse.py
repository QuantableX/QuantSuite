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

    def test_a_text_column_is_keyed_by_its_values_not_by_where_they_live(self):
        import pickle
        df = frame().iloc[:300].copy()
        # A coin's own candles (coin against USD) carry the exchange's name and a flag.
        df["source"] = pd.array(["binance"] * 200 + ["okx"] * 100, dtype="str")
        df["approx"] = False
        base = vote_store.window_key("k", {}, df)
        # Another process holds the same values at other addresses: a pickled copy stands in for it.
        copy = pickle.loads(pickle.dumps(df))
        self.assertNotEqual(df["source"].to_numpy().tobytes(), copy["source"].to_numpy().tobytes())   # other addresses indeed
        self.assertEqual(base, vote_store.window_key("k", {}, copy))
        other = df.copy()
        other.loc[other.index[150], "source"] = "okx"
        self.assertNotEqual(base, vote_store.window_key("k", {}, other))
        flagged = df.copy()
        flagged.loc[flagged.index[10], "approx"] = True
        self.assertNotEqual(base, vote_store.window_key("k", {}, flagged))
        # And such a window is read back like any other.
        plain = self.votes(df)
        self.votes(df, "write")
        vote_store.close_all()
        self.calls.clear()
        read = self.votes(pickle.loads(pickle.dumps(df)), "read")
        self.assertEqual(self.calls, [])
        pd.testing.assert_series_equal(read, plain, check_exact=True)

    def test_a_store_that_fails_is_a_miss_and_the_votes_are_the_same(self):
        df = frame()
        plain = self.votes(df)
        self.votes(df, "write")
        with patch.dict(os.environ, {vote_store.ENV_FOLDER: str(self.folder), vote_store.ENV_MODE: "write"}):
            store = vote_store.active()
            store._con.close()   # every read and write now raises inside the store
            self.calls.clear()
            out = signals.member_signal(df, "k")
            self.assertGreater(store.errors, 0)
        self.assertGreater(len(self.calls), 3)   # computed again, nothing raised
        pd.testing.assert_series_equal(out, plain, check_exact=True)
        vote_store._OPEN.clear()

    def test_a_shared_store_gives_the_same_votes_and_never_reads_another_code_s(self):
        df = frame()
        plain = self.votes(df)
        with patch.dict(os.environ, {vote_store.ENV_SALT: "environment-a"}):
            written = self.votes(df, "write")
            self.calls.clear()
            read = self.votes(df, "read")
            self.assertEqual(self.calls, [])   # the same code and candles: all from the store
        for out in (written, read):
            pd.testing.assert_series_equal(out, plain, check_exact=True)
        # Another environment (engine, libraries, device): nothing of the first one is read.
        with patch.dict(os.environ, {vote_store.ENV_SALT: "environment-b"}):
            self.calls.clear()
            other = self.votes(df, "read")
            self.assertGreater(len(self.calls), 3)
        pd.testing.assert_series_equal(other, plain, check_exact=True)
        # A store of one run keeps its keys as they were.
        self.calls.clear()
        self.votes(df, "read")
        self.assertGreater(len(self.calls), 3)


class SharedKeyTests(unittest.TestCase):
    """A shared store's keys name the code: the script's signature, the indicator's own defaults, the environment."""

    def setUp(self):
        vote_store._SALTS.clear()
        self.addCleanup(vote_store._SALTS.clear)
        self.defaults = {"length": 20}
        self.signature = "signature-1"
        outer = self

        class Indicator:
            @classmethod
            def default_params(cls):
                return dict(outer.defaults)

        for p in (patch.dict("rotation_lab.backtest.smithery.REGISTRY", {"k": Indicator}),
                  patch("smithery.variants.source_signature", side_effect=lambda key: self.signature)):
            p.start()
            self.addCleanup(p.stop)

    def key(self, environment="environment-a", kind="k"):
        vote_store._SALTS.clear()
        env = {vote_store.ENV_SALT: environment} if environment else {}
        with patch.dict(os.environ, env):
            if not environment:
                os.environ.pop(vote_store.ENV_SALT, None)
            return vote_store.window_key(kind, {}, frame().iloc[:200])

    def test_one_run_s_store_has_no_salt(self):
        with patch.dict(os.environ):
            os.environ.pop(vote_store.ENV_SALT, None)
            self.assertEqual(vote_store.salt("k"), "")
        self.assertEqual(self.key(None), self.key(None))
        self.assertNotEqual(self.key(None), self.key())

    def test_the_same_code_gives_the_same_key_in_every_process(self):
        self.assertEqual(self.key(), self.key())

    def test_a_changed_script_version_or_environment_gives_another_key(self):
        base = self.key()
        self.signature = "signature-2"          # the script or one of its libraries changed
        self.assertNotEqual(base, self.key())
        self.signature = "signature-1"
        self.defaults = {"length": 21}          # another version's parameters behind the same key
        self.assertNotEqual(base, self.key())
        self.defaults = {"length": 20}
        self.assertNotEqual(base, self.key("environment-b"))   # engine, libraries or device
        self.assertEqual(base, self.key())

    def test_an_indicator_without_a_signature_is_never_shared(self):
        self.signature = None
        with patch.dict(os.environ, {vote_store.ENV_SALT: "environment-a"}):
            vote_store._SALTS.clear()
            mine = vote_store.salt("k")
            with patch("os.getpid", return_value=os.getpid() + 1):
                vote_store._SALTS.clear()
                self.assertNotEqual(mine, vote_store.salt("k"))   # another process: another key


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
                return [{"label": "base", "multiple": 2.0, "years": 4.0, "cagr": 0.19, "max_drawdown_pct": 30.0,
                         "equity": frame()["close"] / 100}]
            captured["variants"] = jobs
            raise Stop()

        with patch.object(sg, "run_native", side_effect=fake_run_native), \
                patch.object(sg, "parent_votes", side_effect=lambda jobs, store, progress=None: (jobs, "off")), \
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
                patch.object(sg, "parent_votes", side_effect=lambda jobs, store, progress=None: (jobs, "off")), \
                patch("smithery.registry.describe_all", return_value={"indicators": []}):
            with self.assertRaises(Stop):
                sg.run(config, data_dir=Path(tempfile.gettempdir()), fast=True)
        self.assertIsNone(captured["base"][0][4])
        self.assertFalse(any(job[4] for job in captured["variants"]))


class ParentVotesTests(unittest.TestCase):
    """The parent computes the variants' own votes (votes-only runs, write mode); the workers read."""

    def jobs(self):
        return [("parameter_jitter|jitter 1", "cfg-a", None, False, None),
                ("price_noise|noise seed 1", "cfg-b", {"kind": "noise", "seed": 1}, False, None),
                ("fees|2x fee", "cfg-c", None, False, ("base-store", "read"))]

    def test_own_votes_are_computed_once_in_the_parent_and_every_job_then_reads(self):
        from smithery import system_gauntlet as sg
        from rotation_lab.backtest import batch_votes
        seen = []

        def fake_native(job):
            seen.append((job, os.environ.get(vote_store.ENV_FOLDER), os.environ.get(vote_store.ENV_MODE),
                         os.environ.get(batch_votes.VOTES_ONLY)))
            return {"label": job[0], "error": "votes only"}

        with patch.object(sg, "native_job", side_effect=fake_native), \
                patch("smithery.batch.gpu_available", return_value=True), \
                patch.dict(os.environ, {"SMITHERY_PARENT_VOTES": "1", "SMITHERY_WORKER_GPU": "0"}):
            os.environ.pop(batch_votes.VOTES_ONLY, None)
            jobs, note = sg.parent_votes(self.jobs(), "the-store")
        self.assertEqual([s[0][0] for s in seen], ["parameter_jitter|jitter 1", "price_noise|noise seed 1"])
        for job, folder, mode, only in seen:
            self.assertEqual((len(job), folder, mode, only), (4, "the-store", "write", "1"))
        self.assertTrue(all(job[4] == ("the-store", "read") for job in jobs))
        self.assertEqual([job[:4] for job in jobs], [job[:4] for job in self.jobs()])
        self.assertNotIn(batch_votes.VOTES_ONLY, os.environ)
        self.assertIn("GPU", note)

    def test_off_without_a_gpu_or_when_switched_off(self):
        from smithery import system_gauntlet as sg
        with patch.object(sg, "native_job") as native, patch("smithery.batch.gpu_available", return_value=False):
            jobs, _ = sg.parent_votes(self.jobs(), "the-store")
        native.assert_not_called()
        self.assertEqual(jobs, self.jobs())
        with patch.object(sg, "native_job") as native, patch("smithery.batch.gpu_available", return_value=True), \
                patch.dict(os.environ, {"SMITHERY_PARENT_VOTES": "0"}):
            jobs, _ = sg.parent_votes(self.jobs(), "the-store")
        native.assert_not_called()
        self.assertEqual(jobs, self.jobs())

    def test_kept_votes_come_back_unchanged(self):
        folder = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(folder.cleanup)
        self.addCleanup(vote_store.close_all)
        votes = pd.Series([1, -1, 0], index=pd.date_range("2024-01-01", periods=3), name="k")
        with patch.dict(os.environ, {vote_store.ENV_FOLDER: folder.name, vote_store.ENV_MODE: "write"}):
            self.assertEqual(vote_store.active().put_many([("key-1", votes)]), 1)
        vote_store.close_all()
        with patch.dict(os.environ, {vote_store.ENV_FOLDER: folder.name, vote_store.ENV_MODE: "read"}):
            pd.testing.assert_series_equal(vote_store.active().get("key-1"), votes, check_exact=True)
            self.assertEqual(vote_store.active().put_many([("key-2", votes)]), 0)   # read mode keeps nothing
        vote_store.close_all()


if __name__ == "__main__":
    unittest.main()
