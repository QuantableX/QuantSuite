"""Exercise real CLI processes against isolated indexes and a local model fixture."""
import contextlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest

import test_codebase_embeddings as fixtures
from locking import codebase_lock, IndexBusyError

CLI = Path(__file__).resolve().parents[1] / "codebase_index" / "cli.py"


class ConcurrencyTests(fixtures.EmbeddingFixture):
    def setUp(self):
        super().setUp()
        self.children = []
        self.release = threading.Event()

    def tearDown(self):
        self.release.set()
        for child in self.children:
            if child.poll() is None:
                child.terminate()
            child.communicate(timeout=10)
        super().tearDown()

    def start(self, *args, code=None):
        command = [sys.executable, "-B"]
        command += ["-c", code] if code else [str(CLI)]
        child = subprocess.Popen(
            [*command, *args], cwd=CLI.parent, env=os.environ.copy(),
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8",
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        self.children.append(child)
        return child

    def result(self, child, success=True):
        stdout, stderr = child.communicate(timeout=15)
        self.assertEqual(child.returncode, 0 if success else 1, stderr or stdout)
        self.assertEqual(stderr, "", "CLI should return actionable JSON, not a traceback")
        return json.loads(stdout)

    def hold(self):
        path = self.root / "indexes" / "fixture.db"
        child = self.start(str(path), code=(
            "import sys; from pathlib import Path; from locking import codebase_lock\n"
            "with codebase_lock(Path(sys.argv[1])):\n"
            " print('ready', flush=True)\n"
            " sys.stdin.readline()\n"
        ))
        self.assertEqual(child.stdout.readline().strip(), "ready")
        return child

    def test_overlapping_index_and_reindex_wait_without_blocking_other_workspaces(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            before = [tuple(row) for row in conn.execute("SELECT * FROM chunks")]
        (self.repo / "one.py").write_text("def slow_replacement():\n    return 2\n", encoding="utf-8")
        entered = threading.Event()

        def pause_once(inputs):
            if any("slow_replacement" in text for text in inputs) and not entered.is_set():
                entered.set()
                self.release.wait(15)

        self.server.on_inputs = pause_once
        first = self.start("index", str(self.repo), "--name", "fixture", "--mode", "semantic")
        self.assertTrue(entered.wait(5), "the first writer reached embedding inference")
        second = self.start("reindex", "--codebase", "fixture", "--mode", "semantic")
        # Deliberately exceed the five-second SQLite timeout from the bug report.
        time.sleep(5.3)
        self.assertIsNone(second.poll(), "the overlapping reindex queues instead of clearing data or failing")
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(before, [tuple(row) for row in conn.execute("SELECT * FROM chunks")])
            conn.execute("BEGIN IMMEDIATE")
            conn.rollback()  # Inference holds no SQLite write lock.
        other = self.start("index", str(self.repo), "--name", "independent", "--mode", "structural")
        self.assertEqual(self.result(other)["status"], "ok")
        self.assertGreater(self.result(self.start("stats", "--codebase", "fixture"))["chunk_count"], 0)
        self.release.set()
        self.assertEqual(self.result(first)["status"], "ok")
        self.assertEqual(self.result(second)["status"], "ok")
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(conn.execute("PRAGMA integrity_check").fetchone()[0], "ok")
            self.assertEqual(conn.execute("PRAGMA foreign_key_check").fetchall(), [])
            self.assertEqual(conn.execute("SELECT count(*) FROM chunks").fetchone()[0],
                             conn.execute("SELECT count(*) FROM vec_index").fetchone()[0])
            self.assertIn("slow_replacement", conn.execute("SELECT content FROM chunks").fetchone()[0])

    def test_all_refreshing_commands_share_the_lock_and_report_busy_cleanly(self):
        self.index()
        holder = self.hold()
        wrapper = (
            "import cli; from functools import partial; "
            "cli.codebase_lock=partial(cli.codebase_lock, timeout=0.1); cli.main()"
        )
        commands = [
            ("index", str(self.repo), "--name", "fixture"),
            ("reindex", "--codebase", "fixture"),
            ("search", "authentication", "--codebase", "fixture"),
            ("lookup", "authentication", "--codebase", "fixture"),
        ]
        for args in commands:
            with self.subTest(command=args[0]):
                result = self.result(self.start(*args, code=wrapper), success=False)
                self.assertIn("Another indexing operation", result["error"])
        holder.communicate("release\n", timeout=5)
        self.assertGreater(self.result(self.start("search", "authentication", "--codebase", "fixture"))["count"], 0)

    def test_process_exit_releases_lock_without_deleting_the_lock_file(self):
        self.index()
        holder = self.hold()
        holder.terminate()
        holder.communicate(timeout=5)
        self.assertTrue((self.root / "indexes" / "fixture.db.lock").exists())
        result = self.result(self.start("reindex", "--codebase", "fixture", "--mode", "semantic"))
        self.assertEqual(result["status"], "ok")

    def test_external_sqlite_writer_gets_actionable_error_without_destroying_chunks(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            before = [tuple(row) for row in conn.execute("SELECT * FROM chunks")]
            conn.execute("UPDATE files SET semantic_hash=NULL")
            result = self.result(self.start("reindex", "--codebase", "fixture", "--mode", "semantic"), success=False)
            self.assertIn("database is busy", result["error"])
            conn.rollback()
            self.assertEqual(before, [tuple(row) for row in conn.execute("SELECT * FROM chunks")])

    def test_lock_timeout_then_release_can_be_retried(self):
        path = self.root / "indexes" / "fixture.db"
        holder = self.hold()
        with self.assertRaises(IndexBusyError):
            with codebase_lock(path, timeout=0):
                self.fail("lock already held")
        holder.communicate("release\n", timeout=5)
        with codebase_lock(path, timeout=0):
            pass


if __name__ == "__main__":
    unittest.main()
