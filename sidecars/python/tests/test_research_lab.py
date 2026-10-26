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

    def test_system_runs_freeze_the_systems_cache_and_a_gauntlet_does_not(self):
        import sqlite3
        from contextlib import closing
        shelf = self.path / "shelf"
        shelf.mkdir()
        (shelf / "BINANCE_BTCUSDT_1d.csv").write_text("time,open\n", encoding="utf-8")
        with closing(sqlite3.connect(self.path / "cache.sqlite")) as db, db:
            db.execute("create table candles (pair text)")
            db.execute("insert into candles values ('BTCUSDT')")
        for kind, frozen in (("profit", True), ("aggregates", True), ("gauntlet", False)):
            path = self.path / "runs" / kind
            (path / "library").mkdir(parents=True)
            with patch("smithery.data.PRICE_DIR", shelf):
                lab.freeze(path, kind)
            self.assertEqual((path / "cache.sqlite").is_file(), frozen, kind)
            self.assertEqual("cache.sqlite" in lab.read(path / "inputs.json"), frozen, kind)
            if frozen:
                with closing(sqlite3.connect(path / "cache.sqlite")) as db:
                    self.assertEqual(db.execute("select pair from candles").fetchall(), [("BTCUSDT",)])

    def test_library_overview_pages_carry_only_their_rows_reports(self):
        run = lab.create({})
        path = lab.folder(run["id"])
        track = lambda i, tf: {"report": f"r{i} {tf}", "score": i}
        rows = [{"key": f"k{i}", "standard": {tf: track(i, tf) for tf in ("1h", "1d")},
                 "selected": {"4h": track(i, "4h")}, "curve": list(range(80))} for i in range(120)]
        reports = {f"r{i} {tf}": {"axes": {"asset": i}, "returns": list(range(300))}
                   for i in range(120) for tf in ("1h", "4h", "1d")}
        lab.save(path / "results.json", {"kind": "gauntlet_overview", "rows": rows, "reports": reports,
                                         "summary": {"indicators": 120}, "wide": {f"w{i}": i for i in range(150)}})
        page = lab.details(run["id"], offset=5, limit=2)
        self.assertEqual(page["total"], 120)
        self.assertEqual([r["key"] for r in page["rows"]], ["k5", "k6"])
        self.assertEqual(sorted(page["reports"]), ["r5 1d", "r5 1h", "r5 4h", "r6 1d", "r6 1h", "r6 4h"])
        self.assertEqual(page["reports"]["r5 1h"]["returns"], {"omitted_items": 300, "first": 0, "last": 299})
        self.assertEqual(page["wide"]["omitted_keys"], 150)
        self.assertEqual(page["summary"], {"indicators": 120})
        self.assertLess(len(json.dumps(page)), 5000)
        full = lab.details(run["id"], limit=1, compact=False)
        self.assertEqual(sorted(full["reports"]), ["r0 1d", "r0 1h", "r0 4h"])
        self.assertEqual(len(full["reports"]["r0 1h"]["returns"]), 300)
        self.assertEqual(len(full["rows"][0]["curve"]), 80)


if __name__ == "__main__":
    unittest.main()
