"""`quantscript stage-check`: a staged library is verified before a store
install writes anything. Synthetic scripts only."""
import json
import shutil
import tempfile
import textwrap
import unittest
from pathlib import Path

from smithery import quantscript
from smithery.contract import CONTRACT_VERSION
from smithery.variants import script_signature

HELPER = textwrap.dedent('''\
    """Smoothing for the synthetic stage-check indicator."""
    import pandas as pd


    def smooth(values, length):
        return pd.Series(values).ewm(span=length, adjust=False).mean().to_numpy()
    ''')

ALPHA = textwrap.dedent('''\
    """StageAlpha — synthetic.

    Hypothesis: a test fixture, no claim.
    """
    import numpy as np

    from ..contract import TrendIndicator
    from .stage_helper import smooth


    class StageAlpha(TrendIndicator):
        name = "StageAlpha"
        param_space = {"length": (5, 60)}

        @classmethod
        def default_params(cls):
            return {"length": 20}

        def _compute(self, df):
            close = df["close"].to_numpy(dtype=float)
            n = int(self.params["length"])
            out = np.where(close > smooth(close, n), 1, -1).astype(int)
            out[:n] = 0
            return out


    REGISTER = {"stage_alpha": StageAlpha}
    WARMUP = {"stage_alpha": 60}
    REQUIRES = ("stage_helper",)
    ''')


def version_doc(source: str, length: int) -> dict:
    return {"format": 2, "key": "stage_alpha_opt", "base_key": "stage_alpha", "role": "optimized",
            "label": "Optimized (general)", "created_at": "2026-09-24T00:00:00+00:00", "params": {"length": length},
            "status": "released", "source_sha256": source, "contract": CONTRACT_VERSION,
            "base_file": "stage_alpha.py", "evidence": {"decision": "current", "timeframes": {}}}


class StageCheckTests(unittest.TestCase):
    def setUp(self):
        self.library = Path(tempfile.mkdtemp(prefix="stage-lib-"))
        self.addCleanup(shutil.rmtree, self.library, True)
        (self.library / "stage_helper.py").write_bytes(HELPER.encode())
        (self.library / "stage_alpha.py").write_bytes(ALPHA.encode())
        (self.library / "versions" / "stage_alpha").mkdir(parents=True)

    def write_version(self, source: str):
        path = self.library / "versions" / "stage_alpha" / "stage_alpha_opt.json"
        path.write_text(json.dumps(version_doc(source, 30)), encoding="utf-8")

    def test_a_package_with_its_requirement_and_a_matching_version_passes(self):
        self.write_version(script_signature("stage_alpha", folder=self.library))
        doc = quantscript.stage_check(self.library, ["stage_alpha.py", "stage_helper.py"])
        self.assertFalse(doc["blocking"], doc)
        self.assertTrue(doc["ok"], doc)
        self.assertEqual(doc["unavailable"], [])
        self.assertEqual([i["key"] for i in doc["files"]["stage_alpha.py"]["indicators"]], ["stage_alpha"])

    def test_a_version_that_does_not_match_the_scripts_blocks(self):
        self.write_version("0" * 64)
        doc = quantscript.stage_check(self.library, ["stage_alpha.py"])
        self.assertTrue(doc["blocking"])
        self.assertEqual([u["key"] for u in doc["unavailable"]], ["stage_alpha_opt"])

    def test_a_script_that_does_not_import_blocks(self):
        (self.library / "stage_helper.py").unlink()
        doc = quantscript.stage_check(self.library, ["stage_alpha.py"])
        self.assertTrue(doc["blocking"])
        self.assertFalse(doc["ok"])


if __name__ == "__main__":
    unittest.main()
