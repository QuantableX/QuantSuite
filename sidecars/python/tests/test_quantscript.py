"""QuantScript's CLI: the listing, the sandboxed check, discovery, the template."""
import hashlib
import json
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

from smithery import quantscript
from smithery.contract import TrendIndicator
from smithery.indicators import REGISTRY
from smithery.indicators._discover import discover

INDICATORS = Path(quantscript.INDICATORS_DIR)


def candidate(text: str) -> Path:
    tmp = tempfile.NamedTemporaryFile("w", suffix=".py", delete=False, encoding="utf-8")
    tmp.write(text)
    tmp.close()
    return Path(tmp.name)


class ListingTests(unittest.TestCase):
    def test_every_script_is_listed_with_its_classes_and_verdicts(self):
        doc = quantscript.listing()
        self.assertIsNone(doc["registry_error"])
        self.assertEqual(doc["registry_keys"], len(REGISTRY))
        files = {s["file"]: s for s in doc["scripts"]}
        self.assertIn("extremes.py", files)
        self.assertIn("__init__.py", files)
        self.assertEqual(files["__init__.py"]["kind"], "registry")
        self.assertNotIn("_discover.py", files)
        extremes = files["extremes.py"]
        self.assertEqual(extremes["kind"], "script")
        self.assertTrue(extremes["editable"])
        self.assertTrue(extremes["summary"].startswith("ExtremeFlow"))
        row = next(c for c in extremes["classes"] if c["class_name"] == "ExtremeFlow")
        self.assertEqual(row["key"], "extremes")
        self.assertEqual(row["name"], "ExtremeFlow")
        self.assertGreater(row["line"], 1)
        self.assertIn("horizon", row["params"])
        self.assertIn("certification", row)
        self.assertEqual(extremes["registered"], 1)
        # A multi-class pack lists one row per registered key.
        pack = files["trend_pack.py"]
        self.assertGreater(pack["registered"], 5)
        self.assertIn("CheckedTrend", [c["class_name"] for c in pack["classes"] if c["key"] is None])
        # The contract is shown read-only next to the scripts.
        self.assertEqual([r["file"] for r in doc["reference"]], ["contract.py"])
        self.assertFalse(doc["reference"][0]["editable"])
        # Every entry hashes its bytes.
        self.assertEqual(extremes["sha256"], hashlib.sha256((INDICATORS / "extremes.py").read_bytes()).hexdigest())

    def test_the_listing_is_one_json_document_on_stdout(self):
        proc = subprocess.run([sys.executable, "-m", "smithery.quantscript", "list"],
                              cwd=str(INDICATORS.parent.parent), capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        doc = json.loads(proc.stdout.strip().splitlines()[-1])
        self.assertIn("scripts", doc)


class CheckTests(unittest.TestCase):
    def test_a_syntax_error_is_reported_with_its_line_and_never_leaves_the_process(self):
        path = candidate("import numpy as np\n\ndef broken(:\n    pass\n")
        doc = quantscript.check("extremes.py", path)
        self.assertFalse(doc["syntax"]["ok"])
        self.assertEqual(doc["syntax"]["line"], 3)
        self.assertTrue(doc["blocking"])
        self.assertFalse(doc["ok"])
        self.assertIsNone(doc["sandbox"])

    def test_a_candidate_is_verified_in_a_sandbox_and_the_package_stays_untouched(self):
        original = (INDICATORS / "extremes.py").read_bytes()
        text = original.decode("utf-8").replace('"horizon": 21', '"horizon": 13')
        doc = quantscript.check("extremes.py", candidate(text), depth="quick")
        self.assertTrue(doc["import"]["ok"], doc)
        self.assertFalse(doc["blocking"])
        self.assertTrue(doc["ok"], doc)
        self.assertNotEqual(Path(doc["sandbox"]).resolve(), quantscript.PACKAGE_DIR.resolve())
        self.assertEqual(doc["registry_keys"], len(REGISTRY))
        rows = {r["key"]: r for r in doc["indicators"]}
        self.assertEqual(list(rows), ["extremes"])
        self.assertEqual(rows["extremes"]["class_name"], "ExtremeFlow")
        self.assertEqual([c["law"] for c in rows["extremes"]["checks"]], ["contract"])
        self.assertIsNotNone(rows["extremes"]["committed_at"])
        # The real file was never written.
        self.assertEqual((INDICATORS / "extremes.py").read_bytes(), original)
        self.assertFalse((quantscript.PACKAGE_DIR / "indicators" / "__pycache__" / "verify.pyc").exists())

    def test_a_failing_import_blocks_the_save(self):
        doc = quantscript.check("extremes.py", candidate("import a_module_that_does_not_exist_qs\n"))
        self.assertTrue(doc["syntax"]["ok"])
        self.assertTrue(doc["import"]["ok"])  # Other scripts remain available.
        self.assertIn("a_module_that_does_not_exist_qs", doc["discovery_errors"]["extremes"])
        self.assertTrue(doc["blocking"])

    def test_a_broken_registry_init_blocks_too(self):
        doc = quantscript.check("__init__.py", candidate('raise RuntimeError("the registry is gone")\n'))
        self.assertFalse(doc["import"]["ok"])
        self.assertIn("the registry is gone", doc["import"]["message"])
        self.assertTrue(doc["blocking"])

    def test_a_contract_violation_is_reported_but_does_not_block(self):
        text = textwrap.dedent('''
            """A rule that returns to neutral — the contract forbids it."""
            import numpy as np
            from ..contract import TrendIndicator

            class NeutralCopOut(TrendIndicator):
                name = "NeutralCopOut"
                param_space = {}

                def _compute(self, df):
                    sig = np.ones(len(df))
                    sig[len(df) // 2] = 0
                    return sig

            REGISTER = {"qs_test_copout": NeutralCopOut}
        ''')
        doc = quantscript.check("qs_test_copout.py", candidate(text), depth="quick")
        self.assertTrue(doc["import"]["ok"], doc)
        self.assertFalse(doc["blocking"])
        self.assertFalse(doc["ok"])
        row = doc["indicators"][0]
        self.assertEqual(row["key"], "qs_test_copout")
        self.assertFalse(row["ok"])
        self.assertIn("returns to 0", row["error"])

    def test_a_script_that_cannot_register_is_blocking(self):
        text = textwrap.dedent('''
            from ..contract import TrendIndicator

            class Dup(TrendIndicator):
                name = "Dup"
                def _compute(self, df):
                    import numpy as np
                    return np.ones(len(df))

            REGISTER = {"extremes": Dup}
        ''')
        doc = quantscript.check("qs_test_dup.py", candidate(text))
        self.assertTrue(doc["import"]["ok"], doc)
        self.assertIn("qs_test_dup", doc["discovery_errors"])
        self.assertIn("already registered", doc["discovery_errors"]["qs_test_dup"])
        self.assertTrue(doc["blocking"])

    def test_the_template_registers_and_passes_every_law(self):
        source = quantscript.template("qs_demo", "DemoTrend", "DemoTrend")
        self.assertIn('REGISTER = {"qs_demo": DemoTrend}', source)
        doc = quantscript.check("qs_demo.py", candidate(source), depth="full")
        self.assertTrue(doc["import"]["ok"], doc)
        self.assertEqual(doc["discovery_errors"], {})
        self.assertFalse(doc["blocking"])
        self.assertTrue(doc["ok"], doc)
        self.assertEqual(doc["registry_keys"], len(REGISTRY) + 1)
        row = doc["indicators"][0]
        self.assertEqual(row["key"], "qs_demo")
        self.assertEqual([c["law"] for c in row["checks"]], ["contract", "causality", "scale"])
        self.assertTrue(all(c["ok"] for c in row["checks"]), row)
        self.assertLessEqual(row["committed_at"], row["warmup_bars"])

    def test_bad_names_are_refused(self):
        with self.assertRaises(ValueError):
            quantscript.check("../evil.py", candidate("x = 1\n"))
        with self.assertRaises(ValueError):
            quantscript.template("Bad Key", "Demo", "Demo")
        with self.assertRaises(ValueError):
            quantscript.template("ok_key", "lower", "Demo")


class DiscoverTests(unittest.TestCase):
    def test_discovery_takes_good_scripts_and_reports_the_rest(self):
        root = Path(tempfile.mkdtemp(prefix="qs-discover-"))
        pkg = root / "qs_discover_pkg"
        pkg.mkdir()
        (pkg / "__init__.py").write_text("", encoding="utf-8")
        (pkg / "good.py").write_text(textwrap.dedent('''
            import numpy as np
            from smithery.contract import TrendIndicator
            class Good(TrendIndicator):
                name = "Good"
                def _compute(self, df):
                    return np.ones(len(df))
            REGISTER = {"qs_good": Good}
            WARMUP = {"qs_good": 123}
        '''), encoding="utf-8")
        (pkg / "bad.py").write_text("raise ImportError('boom')\n", encoding="utf-8")
        (pkg / "dup.py").write_text(textwrap.dedent('''
            import numpy as np
            from smithery.contract import TrendIndicator
            class Dup(TrendIndicator):
                name = "Dup"
                def _compute(self, df):
                    return np.ones(len(df))
            REGISTER = {"extremes": Dup, "Bad-Key": Dup, "qs_notaclass": 42}
        '''), encoding="utf-8")
        (pkg / "helper.py").write_text("X = 1\n", encoding="utf-8")
        (pkg / "_private.py").write_text("raise RuntimeError('never imported')\n", encoding="utf-8")
        sys.path.insert(0, str(root))
        try:
            registry, warmup, errors, discovered = discover(pkg, "qs_discover_pkg", {"extremes"}, TrendIndicator)
        finally:
            sys.path.remove(str(root))
        self.assertEqual(list(registry), ["qs_good"])
        self.assertEqual(warmup, {"qs_good": 123})
        self.assertEqual(discovered, {"good": ["qs_good"]})
        self.assertEqual(set(errors), {"bad", "dup"})
        self.assertIn("boom", errors["bad"])
        self.assertIn("already registered", errors["dup"])
        self.assertIn("not a registry key", errors["dup"])
        self.assertIn("TrendIndicator subclass", errors["dup"])


class LintTests(unittest.TestCase):
    def test_a_syntax_error_carries_its_span_and_nothing_is_imported(self):
        doc = quantscript.lint(candidate("import a_module_that_does_not_exist_qs\n\ndef broken(:\n    pass\n"))
        self.assertFalse(doc["ok"])
        errors = [m for m in doc["markers"] if m["severity"] == "error"]
        self.assertEqual(len(errors), 1)
        self.assertEqual((errors[0]["line"], errors[0]["column"]), (3, 12))
        self.assertIn("syntax", errors[0]["message"])
        self.assertNotIn("a_module_that_does_not_exist_qs", sys.modules)

    def test_contract_shape_and_parser_warnings_are_hints_not_errors(self):
        text = textwrap.dedent('''
            import re
            PATTERN = "\\d+"
            from ..contract import TrendIndicator

            class Shapeless(TrendIndicator):
                pass

            REGISTER = {"Bad Key": Shapeless}
        ''')
        doc = quantscript.lint(candidate(text))
        self.assertTrue(doc["ok"])
        messages = [m["message"] for m in doc["markers"]]
        self.assertTrue(any("invalid escape sequence" in m for m in messages), messages)
        self.assertTrue(any("_compute" in m for m in messages), messages)
        self.assertTrue(any("name" in m and "abstract" in m for m in messages), messages)
        self.assertTrue(any("not a registry key" in m for m in messages), messages)
        self.assertTrue(all(m["severity"] == "warning" for m in doc["markers"]))

    def test_a_clean_script_has_no_markers(self):
        doc = quantscript.lint(candidate(quantscript.template("qs_lint", "LintTrend", "LintTrend")))
        self.assertEqual(doc, {"ok": True, "markers": []})

    def test_registration_tells_the_forges_modules_from_scripts(self):
        doc = quantscript.listing()
        by_file = {s["file"]: s["registration"] for s in doc["scripts"]}
        self.assertEqual(by_file["extremes.py"], "discovered")
        self.assertEqual(by_file["extremes_guard.py"], "none")
        self.assertEqual(by_file["__init__.py"], "none")


if __name__ == "__main__":
    unittest.main()
