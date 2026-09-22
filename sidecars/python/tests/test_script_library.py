"""User-library and version boundaries exercised in fresh consumer processes."""
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import numpy as np
import pandas as pd
from smithery import execution, quality
from smithery.contract import validate_causality, validate_scale_invariance
from smithery.indicators.recovery import RecoveryTrend
from smithery.quantscript import synthetic_frame


class LibraryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qs-library-test-")
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name) / "indicators"
        # Private fixtures are copied explicitly by tests, never by the runtime.
        from smithery.workspace import indicators_dir
        shutil.copytree(indicators_dir(), self.folder,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "versions"))
        self.env = {**os.environ, "QUANTSCRIPT_INDICATORS_DIR":str(self.folder),
                    "PYTHONDONTWRITEBYTECODE":"1"}

    def run_python(self, source):
        p = subprocess.run([sys.executable,"-c",source],env=self.env,
                           capture_output=True,text=True,encoding="utf-8",timeout=40)
        self.assertEqual(p.returncode,0,p.stderr)
        return json.loads(p.stdout.strip().splitlines()[-1])

    def test_sources_are_personal_editable_and_deletions_do_not_respawn(self):
        result = self.run_python("import inspect,json; from smithery.indicators import REGISTRY,DISCOVERY_ERRORS; print(json.dumps(dict(file=inspect.getfile(REGISTRY['extremes']),errors=DISCOVERY_ERRORS)))")
        self.assertEqual(Path(result["file"]).parent,self.folder)
        self.assertEqual(result["errors"],{})
        path = self.folder/"extremes.py"
        path.write_text(path.read_text(encoding="utf-8").replace('name = "ExtremeFlow"','name = "PersonalExtreme"'),encoding="utf-8")
        self.assertEqual(self.run_python("import json; from smithery.indicators import REGISTRY; print(json.dumps(REGISTRY['extremes'].name))"),"PersonalExtreme")
        path.unlink()
        keys = self.run_python("import json; from smithery.indicators import REGISTRY; print(json.dumps(list(REGISTRY)))")
        self.assertNotIn("extremes",keys)
        self.assertIn("hilbert",keys)
        self.assertFalse(path.exists())

    def test_subversion_preserves_base_and_loads_in_fresh_workers(self):
        doc = self.run_python("import json; from smithery.variants import create_variant; print(json.dumps(create_variant('ensemble_original',{'max_halflife':122},{'test':True})))")
        key = doc["key"]
        self.assertEqual(doc["base_key"],"ensemble")
        self.assertEqual(Path(doc["path"]).parent,self.folder/"versions"/"ensemble")
        source = f"import json,pickle; from smithery.indicators import REGISTRY; cls=REGISTRY[{key!r}]; print(json.dumps(dict(params=pickle.loads(pickle.dumps(cls())).params,base=REGISTRY['ensemble_original']().params)))"
        got = self.run_python(source)
        self.assertEqual(got["params"]["max_halflife"],122)
        self.assertEqual(got["base"]["max_halflife"],250)
        # Importing the serialized module directly simulates an idle spawn worker.
        self.assertTrue(self.run_python(f"import json; import smithery.variants as v; print(json.dumps(hasattr(v, 'Forge_{key}')))"))
        path=self.folder/"ensemble.py"
        path.write_text(path.read_text(encoding="utf-8")+"\n# changed base\n",encoding="utf-8")
        result=self.run_python("import json; from smithery.indicators import REGISTRY,DISCOVERY_ERRORS; print(json.dumps(dict(keys=list(REGISTRY),errors=DISCOVERY_ERRORS)))")
        self.assertNotIn(key,result["keys"])
        self.assertIn("Base scripts changed",str(result["errors"]))

    def test_forge_persists_a_reloadable_child_after_real_training(self):
        shelf=Path(self.temp.name)/"vault"/"Price History"
        shelf.mkdir(parents=True)
        synthetic_frame(1200).rename_axis("timestamp").to_csv(shelf/"BINANCE_BTCUSDT_1d.csv")
        env={**self.env,"SMITHERY_VAULT":str(shelf.parent),"SMITHERY_WORKERS":"2"}
        result=subprocess.run([sys.executable,"-m","smithery.forge","walkforward",
            "--indicator","recovery","--series","BINANCE_BTCUSDT_1d","--folds","2"],
            env=env,capture_output=True,text=True,encoding="utf-8",timeout=90)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        events=[json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        completed=next(event for event in events if event["event"]=="walkforward")
        child=completed["variant"]
        self.assertTrue(Path(child["path"]).is_file())
        self.assertEqual(child["base_key"],"recovery")
        self.assertEqual(completed["objective"],quality.OBJECTIVE_VERSION)
        self.assertIn("omega",completed["oos_metrics"])
        key=child["key"]
        self.assertEqual(self.run_python(f"import json; from smithery.indicators import REGISTRY; print(json.dumps(REGISTRY[{key!r}]().params))"),child["params"])

    def test_sandbox_uses_personal_dependencies_and_never_writes_them(self):
        self.run_python("import json; from smithery.indicators import REGISTRY; print(json.dumps(len(REGISTRY)))")
        path=self.folder/"extremes.py"
        before=path.read_bytes()
        candidate=Path(self.temp.name)/"candidate.py"
        candidate.write_bytes(before)
        result=self.run_python(f"import json; from pathlib import Path; from smithery.quantscript import check; print(json.dumps(check('extremes.py',Path({str(candidate)!r}))))")
        self.assertFalse(result["blocking"],result)
        self.assertEqual(path.read_bytes(),before)
        self.assertNotEqual(Path(result["sandbox"]),self.folder.parent)


class QualityTests(unittest.TestCase):
    def test_metrics_include_initial_loss_and_use_downside_deviation(self):
        r=pd.Series([-.10,.30,0.,-.10],index=pd.date_range("2020-01-01",periods=4))
        m=execution.metrics(r)
        self.assertAlmostEqual(m["max_dd"],-.1)
        self.assertAlmostEqual(m["omega"],1.5)
        self.assertAlmostEqual(m["sortino"],.025 / np.sqrt(.02/4) * np.sqrt(365.25))
        self.assertAlmostEqual(1+m["return"],.9*1.3*.9)

    def test_cash_invalid_and_dominated_candidates_cannot_win(self):
        good=dict(cagr=.5,sharpe=1.5,sortino=2.,omega=1.3,max_dd=-.2)
        worse={**good,"cagr":.3,"max_dd":-.3}
        cash=dict(cagr=0.,sharpe=0.,sortino=0.,omega=0.,max_dd=0.)
        self.assertEqual(quality.select([cash,worse,good],3),[2,1])
        self.assertEqual(quality.pareto_order([{**good,"omega":float("inf")}]),[])
        for field in ("cagr","sharpe","sortino","omega"):
            self.assertGreater(quality.score({**good,field:good[field]+.1}),quality.score(good))
        self.assertGreater(quality.score({**good,"max_dd":-.1}),quality.score(good))

    def test_drawdown_feasibility_does_not_claim_a_failed_target(self):
        r=dict(cagr=.5,sharpe=1.,sortino=2.,omega=1.2,max_dd=-.4)
        self.assertEqual(quality.select([r],1,max_drawdown=.3),[0])
        self.assertGreater(abs(r["max_dd"]),.3)

    def test_execution_fold_boundary_matches_one_continuous_run(self):
        df=synthetic_frame(150)
        sig=pd.Series(np.where(np.arange(len(df))%11<6,1.,-1.),index=df.index)
        whole=execution.evaluate(df,sig,start=1,mode="long_short",return_series=True)
        first=execution.evaluate(df,sig,start=1,stop=70,mode="long_short",return_series=True,liquidate=False)
        second=execution.evaluate(df,sig,start=70,mode="long_short",return_series=True,initial_position=first["final_position"])
        pd.testing.assert_series_equal(pd.concat([first["returns"],second["returns"]]),whole["returns"])

    def test_recovery_is_causal_and_scale_invariant(self):
        frame=synthetic_frame(500)
        self.assertTrue(validate_causality(RecoveryTrend(),frame)[0])
        self.assertTrue(validate_scale_invariance(RecoveryTrend(),frame)[0])


if __name__ == "__main__":
    unittest.main()
