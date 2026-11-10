"""Worker processes start with one OpenBLAS thread (card 4e80395e, part D)."""
import os
import subprocess
import sys
import unittest
from unittest.mock import patch

from smithery import threads


class OneThreadTests(unittest.TestCase):
    def test_children_get_one_thread_and_a_set_value_stays(self):
        with patch.dict(os.environ, clear=False):
            os.environ.pop(threads.NAME, None)
            threads.one_for_children()
            self.assertEqual(os.environ[threads.NAME], "1")
            # A child process is started with it, so its numpy reads it at import.
            seen = subprocess.run([sys.executable, "-c", f"import os; print(os.environ.get({threads.NAME!r}))"],
                                  capture_output=True, text=True, check=True).stdout.strip()
            self.assertEqual(seen, "1")
            os.environ[threads.NAME] = "6"
            threads.one_for_children()
            self.assertEqual(os.environ[threads.NAME], "6")

    def test_an_environment_for_a_child(self):
        self.assertEqual(threads.with_one({"A": "b"}), {"A": "b", threads.NAME: "1"})
        self.assertEqual(threads.with_one({threads.NAME: "4"})[threads.NAME], "4")

    def test_a_research_run_starts_its_worker_with_one_thread(self):
        from pathlib import Path
        from smithery import lab
        with patch.dict(os.environ, clear=False):
            os.environ.pop(threads.NAME, None)
            self.assertEqual(lab.env_for(Path("run"))[threads.NAME], "1")


if __name__ == "__main__":
    unittest.main()
