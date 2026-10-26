"""Research report tools: Gauntlet evidence compiled into the shared English template."""
import json
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

from smithery import lab
from smithery.lab_gauntlet import build, from_run, overview, vault_reports
from smithery.lab_report import payload, render

AXES = {"asset": 80.0, "exchange": 90.0, "parameter": 70.0, "temporal": 60.0, "monte_carlo": 75.0}


def entry(score, p, certified, report):
    return {"score": score, "grade": "A" if score >= 70 else "B", "perm_p": p, "certified": certified,
            "report": report, "date": "2026-10-01", "evaluation_version": "smithery-test"}


def registry():
    """Two bases: one with track versions, one with a missing 1h track and no versions."""
    rows = [{"key": "alpha", "base_key": "alpha", "role": "standard", "name": "Alpha Trend", "hypothesis": "h",
             "script": "alpha.py", "warmup_bars": 100, "params": {"length": 10},
             "versions": {"standard": "alpha", "optimized_1h": "alpha_opt_1h", "optimized_4h": "alpha_opt_4h",
                          "optimized_1d": "alpha_opt_1d"},
             "timeframes": {"1h": entry(65, .2, False, "2026-10-01 A 1h"), "4h": entry(72, .05, True, "2026-10-01 A 4h"),
                            "1d": entry(75, .04, True, "2026-10-01 A 1d")}},
            {"key": "beta", "base_key": "beta", "role": "standard", "name": "Beta", "script": "beta.py",
             "params": {}, "versions": {}, "timeframes": {"4h": entry(80, .01, True, "2026-10-02 B 4h"), "1d": None}}]
    for tf, score, decision in (("1h", 71, "promoted"), ("4h", 72, "standard_retained"), ("1d", 78, "promoted")):
        rows.append({"key": f"alpha_opt_{tf}", "base_key": "alpha", "role": f"optimized_{tf}", "params": {"length": 12},
                     "timeframes": {tf: entry(score, .03, True, f"2026-10-03 A opt {tf}")},
                     "variant": {"evidence": {"decision": decision, "walkforward":
                                 {"folds": 4, "wfe": .5, "oos_sharpe": .7, "is_sharpe_mean": 1.1, "series": "BTC"} if tf != "4h" else None}}})
    return {"generated_at": "2026-10-26T00:00:00+00:00", "version": "test", "contract_version": 2, "indicators": rows}


class GauntletReportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name)
        self.output = self.path / "vault" / "Output"
        self.output.mkdir(parents=True)
        for row in registry()["indicators"]:
            for verdict in (row.get("timeframes") or {}).values():
                if verdict:
                    axes = dict(AXES, temporal=None) if verdict["report"] == "2026-10-02 B 4h" else AXES
                    (self.output / (verdict["report"] + ".json")).write_text(json.dumps(
                        {"version": "smithery-test", "name": row.get("name"), "score": verdict["score"], "scores": axes,
                         "perm_p": verdict["perm_p"], "certified": verdict["certified"], "reasons": [],
                         "seed": 42, "counts": {"perm": 120, "boot": 200, "garch": 100}, "fast": False,
                         "huge_unused_field": list(range(1000))}), encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def test_worst_track_decides_and_versions_are_selected_per_track(self):
        result = overview(registry(), self.output)
        alpha, beta = result["rows"]
        self.assertEqual(alpha["overall"]["standard"], {"score": 65, "perm_p": .2, "certified": False, "complete": True, "worst": "1h"})
        self.assertEqual(alpha["overall"]["selected"]["score"], 71)
        self.assertTrue(alpha["overall"]["selected"]["certified"])
        self.assertEqual(alpha["decision"], {"1h": "promoted", "4h": "standard_retained", "1d": "promoted"})
        self.assertIsNone(alpha["wf"]["4h"])
        self.assertEqual(alpha["params"]["1d"], {"length": 12})
        # A track that never ran is missing, not zero, and blocks the overall verdict.
        self.assertIsNone(beta["standard"]["1h"])
        self.assertEqual(beta["overall"]["standard"]["score"], None)
        self.assertFalse(beta["overall"]["standard"]["certified"])
        # Without track versions the Standard verdict is what the track has.
        self.assertEqual(beta["selected"]["4h"], beta["standard"]["4h"])
        self.assertFalse(beta["standard"]["4h"]["complete"])
        summary = result["summary"]
        self.assertEqual(summary["counts"]["standard"], {"all": 0, "1h": 0, "4h": 2, "1d": 1})
        self.assertEqual(summary["counts"]["selected"], {"all": 1, "1h": 1, "4h": 2, "1d": 1})
        self.assertEqual(summary["incomplete_reports"], ["2026-10-02 B 4h"])
        self.assertEqual((summary["track_versions"], summary["missing_walkforward"]), (3, 1))
        self.assertEqual(summary["report_dates"], ["2026-10-01", "2026-10-03"])
        # Evidence is trimmed to the documented fields.
        self.assertNotIn("huge_unused_field", next(iter(result["reports"].values())))

    def test_key_filter_keeps_unknown_keys_visible_as_errors(self):
        result = overview(registry(), self.output, ["beta", "gamma"])
        self.assertEqual([r["key"] for r in result["rows"]], ["beta", "gamma"])
        self.assertEqual(result["rows"][1]["error"], "Not registered in this library")
        self.assertEqual(result["summary"]["errors"], 1)

    def test_research_run_results_use_their_embedded_evidence(self):
        rows = registry()["indicators"]
        lookup = vault_reports(self.output)
        for row in rows:
            row["evidence"] = {tf: json.loads((self.output / (v["report"] + ".json")).read_text())
                               for tf, v in (row.get("timeframes") or {}).items() if v}
        converted = from_run({"kind": "gauntlet", "rows": rows, "protocol": lab.PROTOCOL})
        self.assertEqual(converted["source"], "run")
        self.assertEqual(converted["rows"], build(rows, lookup)["rows"])

    def test_report_page_is_self_contained_english_and_escaped(self):
        rows = registry()["indicators"]
        rows[0]["name"] = "</script><img src=x onerror=alert(1)>"
        result = overview({**registry(), "indicators": rows}, self.output)
        page = render({"title": "Gauntlet <All>", "kind": "gauntlet_overview", "status": "done"}, result)
        self.assertIn("<title>Gauntlet &lt;All&gt;</title>", page)
        self.assertEqual(page.count("<script>"), 1)
        self.assertNotIn("<img src=x", page)
        self.assertNotRegex(page, r"(?i)(?:src|href)\s*=\s*[\"']?(?:https?:)?//|@import|fetch\(")
        data = re.search(r"const DATA=(.*?);\n", page, re.S)[1]
        self.assertEqual(json.loads(data)["summary"]["indicators"], 2)
        self.assertNotRegex(payload({"x": "</script>&"}), r"[<>&]")

    def test_overview_run_compiles_the_snapshot_without_retesting(self):
        from smithery.quantscript import template
        library = self.path / "library"
        library.mkdir()
        (library / "fixture_trend.py").write_text(template("fixture_trend", "FixtureTrend", "Fixture trend"), encoding="utf-8")
        env = {"QUANTSUITE_HOME": str(self.path / "home"), "QUANTSCRIPT_INDICATORS_DIR": str(library),
               "SMITHERY_VAULT": str(self.path / "vault")}
        with patch.dict(os.environ, env):
            run = lab.create({"kind": "gauntlet_overview", "title": "Overview"})
            self.assertEqual(run["indicators"], [])
            self.assertEqual(lab.run(run["id"]), 0)
            folder = lab.folder(run["id"])
            self.assertEqual(lab.read(folder / "run.json")["status"], "done")
            self.assertFalse((folder / "engine").exists())
            results = lab.read(folder / "results.json")
            self.assertEqual([r["key"] for r in results["rows"]], ["fixture_trend"])
            self.assertEqual(results["summary"]["reports"], 0)
            self.assertIn("Gauntlet", (folder / "report.html").read_text(encoding="utf-8"))
            self.assertIn("results.json", lab.read(folder / "evidence-hashes.json"))
            with self.assertRaisesRegex(ValueError, "fresh draft"):
                lab.run(run["id"])


if __name__ == "__main__":
    unittest.main()
