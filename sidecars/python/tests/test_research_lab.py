"""Research orchestration: private staging, native contract, fair comparisons."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from smithery import lab


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
        gauntlet = render({"title": "Trend <research>", "status": "done"},
                          {"kind": "gauntlet", "rows": [{"key": "x", "base_key": "x", "role": "standard",
                                                         "name": "</script><b>x", "timeframes": {}}]})
        legacy = render({"title": "Trend <research>", "status": "done"},
                        {"kind": "profit", "rows": [{"name": "</script><b>x"}]})
        for page in (gauntlet, legacy):
            self.assertIn('<html lang="en">', page)
            self.assertIn("Trend &lt;research&gt;", page)
            self.assertNotIn("</script><b>x", page)
            # Offline: no remote resource of any kind (the SVG namespace is a name, not a fetch).
            self.assertNotRegex(page, r"(?i)(?:src|href|action)\s*=\s*[\"']?(?:https?:)?//|url\(\s*[\"']?(?:https?:)?//|@import|fetch\(|XMLHttpRequest")
            self.assertNotRegex(page, r"[äöüßÄÖÜ]")
        self.assertIn("Every indicator on every track", gauntlet)

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


if __name__ == "__main__":
    unittest.main()
