"""Research orchestration: private staging, native contract, fair comparisons."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from smithery import lab
from smithery.lab_compare import combinations, FrozenService, validate_request


class ResearchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name)
        (self.path / "library").mkdir()
        self.env = patch.dict(os.environ, {"QUANTSUITE_HOME": str(self.path / "home"),
            "QUANTSCRIPT_INDICATORS_DIR": str(self.path / "library"), "ROTATION_LAB_CACHE": str(self.path / "cache.sqlite")})
        self.env.start()

    def tearDown(self):
        self.env.stop()
        self.tmp.cleanup()

    def test_report_import_is_byte_exact_idempotent_and_never_executes_script(self):
        source = self.path / "old.html"
        content = '<title>Historical research</title><script>const DATA={"subjects":[1,2]};throw Error("must not run")</script>'
        source.write_text(content, encoding="utf-8")
        run = lab.import_report(str(source))
        self.assertEqual(lab.import_report(str(source))["id"], run["id"])
        self.assertEqual(lab.details(run["id"], "html")["html"], content)
        legacy = lab.details(run["id"], "legacy")
        self.assertEqual((legacy["field"], legacy["total"], legacy["rows"]), ("subjects", 2, [1, 2]))
        self.assertEqual(source.read_text(encoding="utf-8"), content)
        self.assertEqual(lab.list_runs()[0]["status"], "imported")

    def test_large_imported_data_is_paged_and_compacted_for_agents(self):
        payload = {"dates": [f"d{i}" for i in range(300)],
                   "subjects": [{"id": f"s{i}", "equity": list(range(500))} for i in range(30)],
                   "reports": {f"r{i}": {"score": i} for i in range(150)}}
        source = self.path / "big.html"
        source.write_text("<title>Big</title><script>const DATA=" + json.dumps(payload) + ";</script>", encoding="utf-8")
        run = lab.import_report(str(source))
        page = lab.details(run["id"], "legacy", offset=10, limit=5)
        self.assertEqual((page["field"], page["total"]), ("subjects", 30))
        self.assertEqual([r["id"] for r in page["rows"]], ["s10", "s11", "s12", "s13", "s14"])
        self.assertEqual(page["rows"][0]["equity"], {"omitted_items": 500, "first": 0, "last": 499})
        self.assertEqual(page["fields"]["reports"], "object with 150 keys")
        full = lab.details(run["id"], "legacy", limit=1, field="subjects", compact=False)
        self.assertEqual(len(full["rows"][0]["equity"]), 500)
        self.assertEqual(lab.details(run["id"], "legacy", field="dates", limit=2)["rows"], ["d0", "d1"])
        with self.assertRaisesRegex(ValueError, "Unknown list field"):
            lab.details(run["id"], "legacy", field="reports")

    def test_generated_report_is_english_offline_and_escapes_data(self):
        from smithery.lab_report import render
        page = render({"title": "Trend <research>", "status": "done"},
                      {"kind": "gauntlet", "rows": [{"name": "</script><b>x", "timeframes": {}}]})
        self.assertIn('<html lang="en">', page)
        self.assertIn("Trend &lt;research&gt;", page)
        self.assertNotIn("</script><b>x", page)
        self.assertNotRegex(page, r"(?i)https?://|[äöüß]")

    def test_paths_and_running_lock(self):
        for bad in ("../library", "a" * 31, "F" * 32, "a" * 32 + "/x"):
            with self.assertRaises(ValueError):
                lab.folder(bad)
        doc = lab.create({"title": "Isolated"})
        with lab.locked(lab.folder(doc["id"])):
            with self.assertRaises(ValueError):
                with lab.locked(lab.folder(doc["id"])):
                    pass

    def test_candidate_uses_real_contract_and_preserves_active_library(self):
        from smithery.quantscript import template
        doc = lab.create({"title": "Synthetic contract integration"})
        source = template("fixture_trend", "FixtureTrend", "Fixture trend")
        answer = lab.candidate(doc["id"], "fixture_trend.py", source, ["fixture_trend"])
        self.assertTrue(answer["saved"], answer)
        self.assertEqual(answer["run"]["indicators"], ["fixture_trend"])
        self.assertFalse((self.path / "library/fixture_trend.py").exists())
        rejected = lab.candidate(doc["id"], "fixture_trend.py", "invalid python (", ["fixture_trend"])
        self.assertFalse(rejected["saved"])
        self.assertEqual((lab.folder(doc["id"]) / "library/fixture_trend.py").read_text(encoding="utf-8"), source)
        with self.assertRaises(ValueError):
            lab.candidate(doc["id"], "fixture_trend.py", source, ["wrong_key"])

    def test_candidate_cannot_replace_original_script(self):
        (self.path / "library/existing.py").write_text("# private original", encoding="utf-8")
        run = lab.create({})
        with self.assertRaisesRegex(ValueError, "frozen"):
            lab.candidate(run["id"], "existing.py", "# replacement", ["x"])
        self.assertEqual((self.path / "library/existing.py").read_text(), "# private original")

    def test_interrupted_worker_remains_visible_without_mutating_record(self):
        run = lab.create({})
        run.update(status="running", pid=123456789)
        lab.save(lab.folder(run["id"]) / "run.json", run)
        with patch.object(lab, "alive", return_value=False):
            self.assertEqual(lab.list_runs()[0]["status"], "interrupted")
        self.assertEqual(lab.read(lab.folder(run["id"]) / "run.json")["status"], "running")

    def test_aggregate_search_is_seeded_varies_size_and_excludes_cloned_votes(self):
        keys = ["a", "a_opt", "b", "c", "d"]
        bases = {k: k.split("_")[0] for k in keys}
        candidates, coverage = combinations(keys, bases, None, 100)
        self.assertEqual([c["size"] for c in coverage], [2, 3, 4])
        self.assertEqual(len(candidates), len(set(candidates)))
        self.assertTrue(all(len({bases[k] for k in c}) == len(c) for c in candidates))
        self.assertEqual(combinations(keys, bases, [2, 4], 3), combinations(keys, bases, [2, 4], 3))
        self.assertEqual(sum(len(c) == 2 for c in candidates), 9)
        with self.assertRaises(ValueError):
            combinations(keys, bases, [5], 1)

    def test_profit_requires_explicit_historical_range_and_costs(self):
        doc = {"config": {"startDate": "2020-01-01", "endDate": "2021-01-01",
            "feeRate": .001, "slippageRate": 0, "indicator": {"trend": "fixture"}},
            "indicators": ["fixture"], "samples_per_size": 2}
        validate_request(doc)
        for field, value in (("feeRate", float("nan")), ("endDate", "2999-01-01"), ("startDate", "2022-01-01")):
            bad = copy.deepcopy(doc); bad["config"][field] = value
            with self.assertRaises(ValueError):
                validate_request(bad)

    def test_native_system_engine_comparison_uses_same_frozen_input(self):
        # A fresh process owns the private registry; no test-order cache effects.
        from smithery.quantscript import template
        for key in ("fixture_a", "fixture_b"):
            (self.path / "library" / f"{key}.py").write_text(template(key, "Fixture" + key[-1].upper(), key), encoding="utf-8")
        output = self.path / "comparison"; output.mkdir()
        script = r'''
import datetime as dt
from pathlib import Path
from unittest.mock import patch
import numpy as np
import pandas as pd
from rotation_lab.data.ohlcv import OhlcvFetcher, OhlcvSeries, CoinRef
from smithery.lab_compare import compare
from smithery.lab import read
import sys
path=Path(sys.argv[1])
index=pd.date_range('2018-01-01','2021-12-31',freq='D')
close=100*np.exp(np.arange(len(index))*.001+np.sin(np.arange(len(index))/30)*.08)
frame=pd.DataFrame({'open':close,'high':close*1.01,'low':close*.99,'close':close,'volume':100},index=index)
calls=[]
def fetch(self,exchange,pair,start,end,timeframe='1d'):
    calls.append((start,end,timeframe))
    return OhlcvSeries(CoinRef(None,pair),frame.copy())
doc={'indicators':['fixture_a','fixture_b'],'sizes':[2],'samples_per_size':5,'config':{
 'mode':'single_asset','singleAsset':{'exchange':'fixture','pair':'BTC/USD','timeframe':'1d','direction':'long_cash'},
 'startDate':'2020-01-01','endDate':'2021-12-31','feeRate':.001,'slippageRate':.0005,
 'indicator':{'trend':'fixture_a'},'marketFilter':False}}
with patch.object(OhlcvFetcher,'get_pair_series',fetch): compare(path,doc)
result=read(path/'results.json')
assert len(calls)==1,calls
assert len(result['rows'])==4,result
assert not any(r.get('error') for r in result['rows']),result
assert len({tuple(tuple(p) for p in r['equity']) for r in result['rows']})==1
assert all(r['metrics']['net_return_multiplier']>0 for r in result['rows'])
assert result['coverage']==[{'size':2,'tested':1,'exhaustive':True}]
'''
        proc = subprocess.run([sys.executable, "-c", script, str(output)], capture_output=True, text=True, encoding="utf-8", timeout=120)
        self.assertEqual(proc.returncode, 0, proc.stdout[-2000:] + proc.stderr[-2000:])


if __name__ == "__main__":
    unittest.main()
