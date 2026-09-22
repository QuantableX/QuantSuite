"""Exercise the exact shipped runtime in isolation, without any private alpha."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class PrivateDistributionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qs-clean-install-")
        self.addCleanup(self.temp.cleanup)
        self.install = Path(self.temp.name) / "QuantSuite with spaces"
        root = Path(__file__).resolve().parents[3]
        config_dir = root / "apps/src-tauri"
        resources = json.loads((config_dir / "tauri.conf.json").read_text(encoding="utf-8"))["bundle"]["resources"]
        for source, destination in resources.items():
            target = self.install / destination
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(config_dir / source, target)
        self.runtime = self.install / "sidecars/python"
        self.library = self.install / "QuantScript/indicators"
        self.env = {key: value for key, value in os.environ.items()
                    if key not in ("QUANTSCRIPT_INDICATORS_DIR", "PYTHONPATH", "QUANTSUITE_HOME", "SMITHERY_VAULT")}
        self.env.update(PYTHONPATH=str(self.runtime), PYTHONDONTWRITEBYTECODE="1",
                        QUANTSUITE_HOME=str(self.install / "home"),
                        SMITHERY_VAULT=str(self.install / "vault"))

    def run_python(self, code):
        proc = subprocess.run([sys.executable, "-c", code], cwd=self.runtime, env=self.env,
                              capture_output=True, text=True, encoding="utf-8", timeout=60)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        return json.loads(proc.stdout.strip().splitlines()[-1])

    def test_clean_install_is_empty_and_all_consumers_import(self):
        result = self.run_python('''
import json
from smithery.indicators import REGISTRY, DISCOVERY_ERRORS
from smithery.registry import CERTIFICATION, CERTIFICATION_TF
from smithery.quantscript import listing
from smithery import forge
from quantalgo import strategy, backtest_engine
from quantalgo.regime import available as algo_registry
from rotation_lab.backtest import engine
from rotation_lab.backtest.smithery import REGISTRY as SYSTEMS
doc = listing()
print(json.dumps(dict(keys=list(REGISTRY), systems=list(SYSTEMS), algo=list(algo_registry()),
    forge=forge.info()['indicators'], errors=DISCOVERY_ERRORS,
    certifications=[CERTIFICATION, CERTIFICATION_TF], folder=doc['indicators_dir'],
    editable=[s['file'] for s in doc['scripts'] if s['editable']], error=doc['registry_error'])))
''')
        self.assertEqual(result["keys"], [])
        self.assertEqual(result["systems"], [])
        self.assertEqual(result["algo"], [])
        self.assertEqual(result["forge"], [])
        self.assertEqual(result["errors"], {})
        self.assertEqual(result["certifications"], [{}, {}])
        self.assertEqual(result["editable"], [])
        self.assertIsNone(result["error"])
        self.assertEqual(Path(result["folder"]), self.library)
        self.assertEqual([p.name for p in self.library.iterdir()], ["README.txt"])

    def test_drop_in_check_edit_and_delete_never_respawn(self):
        source = self.run_python("import json; from smithery.quantscript import template; print(json.dumps(template('local_demo', 'LocalDemo', 'Local Demo')))")
        # The same neutral starter used by the editor; no private test fixture.
        path = self.library / "local_demo.py"
        path.write_text(source, encoding="utf-8")
        check = self.run_python("import json; from pathlib import Path; from smithery.quantscript import check; print(json.dumps(check('local_demo.py', Path('../../QuantScript/indicators/local_demo.py'))))")
        self.assertFalse(check["blocking"], check)
        query = "import json; from smithery.indicators import REGISTRY; from rotation_lab.backtest.smithery import REGISTRY as S; from quantalgo.regime import available; print(json.dumps([REGISTRY['local_demo'].name, S['local_demo'].name, available()['local_demo'].name]))"
        self.assertEqual(self.run_python(query), ["Local Demo"] * 3)
        path.write_text(source.replace('Local Demo', 'Edited Demo'), encoding="utf-8")
        self.assertEqual(self.run_python(query), ["Edited Demo"] * 3)
        path.unlink()
        self.assertEqual(self.run_python("import json; from smithery.indicators import REGISTRY; print(json.dumps(list(REGISTRY)))"), [])
        self.assertFalse(path.exists())

    def test_stale_runtime_and_legacy_home_scripts_are_never_loaded(self):
        for folder in (self.runtime / "smithery/indicators",
                       self.install / "home/modules/script/indicators"):
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "stale_alpha.py").write_text("raise RuntimeError('private stale script imported')", encoding="utf-8")
        result = self.run_python("import json; from smithery.indicators import REGISTRY, DISCOVERY_ERRORS; print(json.dumps([list(REGISTRY), DISCOVERY_ERRORS]))")
        self.assertEqual(result, [[], {}])
        self.assertFalse((self.library / "stale_alpha.py").exists())

    def test_override_is_shared_and_broken_script_is_isolated(self):
        explicit = self.install / "custom scripts"
        explicit.mkdir()
        (explicit / "broken.py").write_text("raise RuntimeError('broken user script')", encoding="utf-8")
        self.env["QUANTSCRIPT_INDICATORS_DIR"] = str(explicit)
        result = self.run_python("import json; from smithery.quantscript import listing; print(json.dumps(listing()))")
        self.assertEqual(Path(result["indicators_dir"]), explicit)
        self.assertIn("broken", result["discovery_errors"])
        self.assertIsNone(result["registry_error"])


if __name__ == "__main__":
    unittest.main()
