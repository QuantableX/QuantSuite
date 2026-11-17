"""The certification's bookkeeping: the vault's ledger under Gauntlets that finish together."""
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

ENGINE = str(Path(__file__).resolve().parents[1])
LEDGER = "# Indicator Registry\n\n| Date | Indicator | Score | Grade | Report |\n|---|---|---|---|---|\n| 2026-01-01 | Old | 50 | C | [[old]] |\n"


class LedgerTests(unittest.TestCase):
    def test_gauntlets_finishing_together_lose_no_ledger_row(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "vault" / "Docs").mkdir(parents=True)
            (root / "library").mkdir()
            ledger = root / "vault" / "Docs" / "04 Indicator Registry.md"
            ledger.write_text(LEDGER, encoding="utf-8")
            writer = textwrap.dedent("""
                import sys
                from types import SimpleNamespace
                from smithery.robustness import Gauntlet
                me = sys.argv[1]
                for n in range(25):
                    Gauntlet._append_ledger(SimpleNamespace(ind=SimpleNamespace(name=me)), "2026-11-17", 70 + n % 9,
                                            "A", f"{me} {n}.md")
            """)
            env = {**os.environ, "SMITHERY_VAULT": str(root / "vault"), "QUANTSCRIPT_INDICATORS_DIR": str(root / "library"),
                   "PYTHONPATH": ENGINE, "PYTHONDONTWRITEBYTECODE": "1", "PYTHONIOENCODING": "utf-8"}
            procs = [subprocess.Popen([sys.executable, "-c", writer, f"w{i}"], env=env, stderr=subprocess.PIPE, text=True)
                     for i in range(6)]
            for proc in procs:
                _, err = proc.communicate(timeout=300)
                self.assertEqual(proc.returncode, 0, err)
            text = ledger.read_text(encoding="utf-8")
            rows = [line for line in text.splitlines() if line.startswith("| 2026-11-17 |")]
            self.assertEqual(len(rows), 6 * 25)
            self.assertEqual({row.split("[[")[1] for row in rows}, {f"w{i} {n}]] |" for i in range(6) for n in range(25)})
            self.assertTrue(text.startswith("# Indicator Registry\n\n| Date | Indicator |"))
            self.assertIn("| 2026-01-01 | Old | 50 | C | [[old]] |", text)
            self.assertEqual(list((root / "vault" / "Docs").glob(".*.tmp")), [])


if __name__ == "__main__":
    unittest.main()
