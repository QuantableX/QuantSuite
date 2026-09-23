"""Role versions (Standard, Optimized, Optimized 1H/4H/1D), per-indicator
signatures, REQUIRES and the parameter schema — on synthetic libraries only
(this repository is public; no real indicator enters a test)."""
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

import numpy as np

from smithery.contract import CONTRACT_VERSION, TrendIndicator, hysteresis_flip
from smithery.quantscript import lint, template

ENGINE = str(Path(__file__).resolve().parents[1])

BETA = textwrap.dedent('''
    """BetaTrend — AlphaTrend with its own name (a dependent script)."""
    from .alpha import AlphaTrend

    class BetaTrend(AlphaTrend):
        name = "BetaTrend"

    REGISTER = {"beta": BetaTrend}
    WARMUP = {"beta": 400}
    REQUIRES = ("alpha",)
''')


class Library:
    """A throwaway library + vault and a fresh interpreter per question —
    the registry is built at import time from QUANTSCRIPT_INDICATORS_DIR."""

    def __init__(self, case: unittest.TestCase):
        self.temp = tempfile.TemporaryDirectory(prefix="qs-versions-test-")
        case.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.folder = self.root / "indicators"
        self.folder.mkdir()
        (self.root / "vault" / "Output").mkdir(parents=True)
        self.case = case

    def script(self, name: str, source: str) -> Path:
        path = self.folder / name
        path.write_text(source, encoding="utf-8")
        return path

    def standard(self, key: str, cls: str):
        return self.script(f"{key}.py", template(key, cls, cls))

    def run(self, source: str):
        env = {**os.environ, "QUANTSCRIPT_INDICATORS_DIR": str(self.folder),
               "SMITHERY_VAULT": str(self.root / "vault"), "PYTHONPATH": ENGINE,
               "PYTHONDONTWRITEBYTECODE": "1", "PYTHONIOENCODING": "utf-8"}
        proc = subprocess.run([sys.executable, "-c", textwrap.dedent(source)], env=env,
                              capture_output=True, text=True, encoding="utf-8", timeout=120)
        self.case.assertEqual(proc.returncode, 0, proc.stderr)
        return json.loads(proc.stdout.strip().splitlines()[-1])

    def registry(self):
        return self.run("""
            import json
            from smithery.indicators import REGISTRY, DISCOVERY_ERRORS
            from smithery.variants import UNAVAILABLE, VERSIONS
            print(json.dumps(dict(keys=sorted(REGISTRY), errors=DISCOVERY_ERRORS,
                                  unavailable=[u['key'] for u in UNAVAILABLE], versions=VERSIONS)))
        """)

    def write(self, base: str, role: str, params: dict, evidence: dict | None = None):
        return self.run(f"""
            import json
            from smithery.variants import write_version
            print(json.dumps(write_version({base!r}, {role!r}, {params!r}, {evidence or {}!r})))
        """)


class RoleVersionTests(unittest.TestCase):
    def setUp(self):
        self.lib = Library(self)
        self.lib.standard("alpha", "AlphaTrend")
        self.lib.script("beta.py", BETA)
        self.lib.standard("gamma", "GammaTrend")

    def test_role_versions_register_stable_keys_with_their_params(self):
        doc = self.lib.write("alpha", "optimized_4h", {"fast": 12, "band": 0.4})
        self.assertEqual(doc["key"], "alpha_opt_4h")
        self.assertEqual(doc["format"], 2)
        self.assertEqual(doc["contract"], CONTRACT_VERSION)
        self.assertEqual(Path(doc["path"]).parent, self.lib.folder / "versions" / "alpha")
        self.lib.write("alpha", "optimized", {"slow": 150})
        got = self.lib.run("""
            import json
            from smithery.indicators import REGISTRY
            from smithery.registry import describe
            d = describe('alpha')
            child = describe('alpha_opt_4h')
            print(json.dumps(dict(params=REGISTRY['alpha_opt_4h']().params, base=REGISTRY['alpha']().params,
                                  name=REGISTRY['alpha_opt_4h'].name, versions=d['versions'], role=child['role'],
                                  base_key=child['base_key'], general=REGISTRY['alpha_opt']().params['slow'])))
        """)
        self.assertEqual(got["params"]["fast"], 12)
        self.assertEqual(got["params"]["band"], 0.4)
        self.assertEqual(got["base"]["fast"], 20)
        self.assertEqual(got["general"], 150)
        self.assertEqual(got["name"], "AlphaTrend · Optimized 4H")
        self.assertEqual(got["role"], "optimized_4h")
        self.assertEqual(got["base_key"], "alpha")
        self.assertEqual(got["versions"], {"standard": "alpha", "optimized": "alpha_opt", "optimized_1h": None,
                                           "optimized_4h": "alpha_opt_4h", "optimized_1d": None})

    def test_a_code_edit_invalidates_only_that_script_and_its_dependents(self):
        for base in ("alpha", "beta", "gamma"):
            self.lib.write(base, "optimized", {"band": 0.5})
        path = self.lib.folder / "alpha.py"
        path.write_text(path.read_text(encoding="utf-8").replace('"band": 0.25', '"band": 0.3'), encoding="utf-8")
        state = self.lib.registry()
        self.assertEqual(sorted(state["unavailable"]), ["alpha_opt", "beta_opt"])
        self.assertIn("gamma_opt", state["keys"])
        self.assertNotIn("alpha_opt", state["keys"])
        self.assertIn("re-forge", str(state["errors"]))

    def test_comments_and_docstrings_keep_versions_valid(self):
        self.lib.write("alpha", "optimized_1d", {"fast": 8})
        path = self.lib.folder / "alpha.py"
        text = path.read_text(encoding="utf-8").replace("a QuantScript indicator.", "a QuantScript indicator, reworded.")
        path.write_text(text + "\n# a comment at the end\n", encoding="utf-8")
        state = self.lib.registry()
        self.assertEqual(state["unavailable"], [])
        self.assertIn("alpha_opt_1d", state["keys"])

    def test_the_standard_file_carries_defaults_and_evidence_only(self):
        with self.assertRaises(AssertionError):   # the subprocess fails: params are not the defaults
            self.lib.write("alpha", "standard", {"fast": 5})
        verdict = {"timeframes": {"4h": {"score": 77, "grade": "A", "perm_p": 0.02, "certified": True,
                                         "date": "2026-09-23", "report": "r"}}}
        doc = self.lib.write("alpha", "standard", {}, verdict)
        self.assertEqual(doc["key"], "alpha")
        got = self.lib.run("""
            import json
            from smithery.indicators import REGISTRY
            from smithery.registry import verdict_for
            print(json.dumps(dict(n=len([k for k in REGISTRY if k.startswith('alpha')]), v=verdict_for('alpha', '4h'))))
        """)
        self.assertEqual(got["n"], 1)
        self.assertEqual(got["v"]["score"], 77)
        self.assertEqual(got["v"]["source"], "release")
        self.assertTrue(got["v"]["certified"])
        # The defaults changing makes the Standard's evidence unavailable, loudly.
        path = self.lib.folder / "alpha.py"
        path.write_text(path.read_text(encoding="utf-8").replace('"fast": 20,', '"fast": 21,'), encoding="utf-8")
        self.assertIn("alpha", self.lib.registry()["unavailable"])

    def test_update_evidence_merges_and_never_touches_params(self):
        self.lib.write("gamma", "optimized_1h", {"slow": 120},
                       {"timeframes": {"1h": {"score": 61, "grade": "B", "perm_p": 0.2}}})
        got = self.lib.run("""
            import json
            from smithery.variants import update_evidence
            from smithery.registry import verdict_for
            doc = update_evidence('gamma_opt_1h', {'timeframes': {'4h': {'score': 72, 'grade': 'A', 'perm_p': 0.05}}})
            print(json.dumps(dict(params=doc['params'], tfs=sorted(doc['evidence']['timeframes']),
                                  h1=verdict_for('gamma_opt_1h', '1h'), h4=verdict_for('gamma_opt_1h', '4h'))))
        """)
        self.assertEqual(got["params"]["slow"], 120)
        self.assertEqual(got["tfs"], ["1h", "4h"])
        self.assertEqual(got["h1"]["score"], 61)
        self.assertEqual(got["h4"]["score"], 72)
        # A base without a standard file gets one on its first evidence.
        created = self.lib.run("""
            import json
            from smithery.variants import update_evidence
            print(json.dumps(update_evidence('gamma', {'timeframes': {'1d': {'score': 50}}})['role']))
        """)
        self.assertEqual(created, "standard")

    def test_a_declared_requirement_that_is_missing_is_a_discovery_error(self):
        self.lib.script("delta.py", BETA.replace('"beta"', '"delta"').replace("BetaTrend", "DeltaTrend")
                        .replace('("alpha",)', '("alpha", "missing_one")'))
        state = self.lib.registry()
        self.assertNotIn("delta", state["keys"])
        self.assertIn("missing_one", state["errors"]["delta"])

    def test_bases_of_other_kinds_are_refused(self):
        self.lib.write("alpha", "optimized", {})
        with self.assertRaises(AssertionError):
            self.lib.write("alpha_opt", "optimized_4h", {})   # a version is not a base
        with self.assertRaises(AssertionError):
            self.lib.write("alpha", "optimized_2h", {})       # no such role

    def test_research_subversions_keep_working_under_the_new_signature(self):
        doc = self.lib.run("""
            import json
            from smithery.variants import create_variant
            print(json.dumps(create_variant('gamma', {'band': 0.6}, {'test': True})))
        """)
        self.assertEqual(doc["format"], 1)
        self.assertIn(doc["key"], self.lib.registry()["keys"])
        path = self.lib.folder / "alpha.py"   # an unrelated script
        path.write_text(path.read_text(encoding="utf-8").replace('"band": 0.25', '"band": 0.35'), encoding="utf-8")
        self.assertIn(doc["key"], self.lib.registry()["keys"])
        path = self.lib.folder / "gamma.py"   # its own base
        path.write_text(path.read_text(encoding="utf-8").replace('"slow": 100', '"slow": 110'), encoding="utf-8")
        self.assertNotIn(doc["key"], self.lib.registry()["keys"])

    def test_current_runs_follow_their_own_script_only(self):
        write_run = """
            import json
            from smithery import data
            from smithery.evidence import EVALUATION_VERSION, indicator_fingerprint, write_json
            from smithery.indicators import REGISTRY
            cls = REGISTRY['gamma']
            write_json(data.OUTPUT_DIR / '2026-09-23 run.json', dict(
                kind='gauntlet', version=EVALUATION_VERSION, name=cls.name, params=cls().params,
                timeframe='1d', fast=False, score=81, grade='A', perm_p=0.01, certified=True,
                indicator_sha256=indicator_fingerprint(cls)))
            print(json.dumps(True))
        """
        read = """
            import json
            from smithery.registry import verdict_for
            print(json.dumps(verdict_for('gamma', '1d')['source']))
        """
        self.lib.run(write_run)
        self.assertEqual(self.lib.run(read), "current_run")
        path = self.lib.folder / "alpha.py"
        path.write_text(path.read_text(encoding="utf-8").replace('"band": 0.25', '"band": 0.4'), encoding="utf-8")
        self.assertEqual(self.lib.run(read), "current_run")
        path = self.lib.folder / "gamma.py"
        path.write_text(path.read_text(encoding="utf-8").replace('"slow": 100', '"slow": 101'), encoding="utf-8")
        # The run's params no longer match the defaults either: no verdict at all.
        got = self.lib.run("""
            import json
            from smithery.registry import verdict_for
            print(json.dumps(verdict_for('gamma', '1d')))
        """)
        self.assertIsNone(got)


class IsolatedCheckTests(unittest.TestCase):
    def test_isolated_check_carries_the_declared_closure_only(self):
        lib = Library(self)
        lib.standard("alpha", "AlphaTrend")
        lib.script("beta.py", BETA)
        undeclared = BETA.replace('REQUIRES = ("alpha",)', "")
        cases = {"declared": BETA, "undeclared": undeclared}
        for label, source in cases.items():
            candidate = lib.root / f"{label}.py"
            candidate.write_text(source, encoding="utf-8")
            got = lib.run(f"""
                import json
                from pathlib import Path
                from smithery.quantscript import check
                print(json.dumps(check('beta.py', Path({str(candidate)!r}), isolated=True)))
            """)
            self.assertTrue(got["isolated"])
            if label == "declared":
                self.assertFalse(got["blocking"], got)
                self.assertEqual(got["registry_keys"], 2)
            else:
                self.assertTrue(got["blocking"], got)


class SchemaTests(unittest.TestCase):
    class Probe(TrendIndicator):
        name = "Probe"
        param_space = {"length": (10, 100), "band": (0.1, 1.0)}
        param_schema = {"length": {"min": 2, "max": 500, "label": "Length", "help": "Bars"}}

        @classmethod
        def default_params(cls):
            return {"length": 20, "band": 0.3, "long_only": False, "horizons": (5, 10), "mode": "fast"}

        def _compute(self, df):
            return hysteresis_flip(np.zeros(len(df)), 1.0)

    def test_the_schema_merges_declarations_over_derived_entries(self):
        schema = self.Probe.schema()
        self.assertEqual(schema["length"]["type"], "int")
        self.assertEqual(schema["length"]["min"], 2)
        self.assertEqual(schema["length"]["label"], "Length")
        self.assertEqual(schema["length"]["tested"], [10.0, 100.0])
        self.assertEqual(schema["length"]["default"], 20)
        self.assertEqual(schema["band"]["type"], "float")
        self.assertNotIn("min", schema["band"])
        self.assertEqual(schema["long_only"]["type"], "bool")
        self.assertEqual(schema["horizons"], {"type": "list", "label": "Horizons", "help": "", "default": [5, 10]})
        self.assertEqual(schema["mode"]["choices"], ["fast"])

    def test_check_params_names_every_problem(self):
        self.assertEqual(self.Probe.check_params({"length": 30, "band": 5.0, "long_only": True,
                                                  "horizons": [3, 9], "mode": "fast"}), [])
        problems = self.Probe.check_params({"length": 1, "band": "x", "long_only": 1, "horizons": [3],
                                            "mode": "slow", "nope": 1})
        self.assertEqual(len(problems), 6, problems)
        self.assertTrue(any(p.startswith("length: at least 2") for p in problems))
        self.assertTrue(any(p.startswith("nope: unknown") for p in problems))
        self.assertEqual(self.Probe.check_params({"length": 20.5}), ["length: expected a whole number"])
        self.assertEqual(self.Probe.check_params({"band": float("nan")}), ["band: expected a finite number"])


class LintTests(unittest.TestCase):
    def lint_source(self, source: str) -> list[str]:
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "probe.py"
            path.write_text(source, encoding="utf-8")
            return [m["message"] for m in lint(path)["markers"]]

    def test_the_template_is_lint_clean(self):
        self.assertEqual(self.lint_source(template("probe", "ProbeTrend", "ProbeTrend")), [])

    def test_undeclared_requirements_and_unlabelled_parameters_are_named(self):
        messages = self.lint_source(BETA.replace('REQUIRES = ("alpha",)', "")
                                    + '\nMEMBERS = ("hilbert",)\n')
        self.assertTrue(any("'alpha' is used but not declared" in m for m in messages), messages)
        self.assertTrue(any("'hilbert' is used but not declared" in m for m in messages), messages)
        unlabelled = template("probe", "ProbeTrend", "ProbeTrend").replace('"label": "Band",', "")
        self.assertTrue(any("no param_schema label for band" in m for m in self.lint_source(unlabelled)))


if __name__ == "__main__":
    unittest.main()
