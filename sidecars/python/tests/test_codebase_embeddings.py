"""Code indexing integration against a local embedding fixture (no downloads)."""
import contextlib
import io
import json
import os
import sqlite3
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "codebase_index"))
import cli
from db import open_db, get_all_meta, set_meta
from embeddings import EmbedProvider, MAX_INPUT_BYTES
from semantic import chunk_file
from vector_backend import SqliteVecBackend


class EmbeddingFixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                server = self.server
                if self.headers.get("Authorization") != "Bearer " + server.key:
                    self.send_error(401)
                    return
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                server.inputs.extend(body["input"])
                if server.on_inputs:
                    server.on_inputs(body["input"])
                if server.fail and any("FAIL_ONCE" in text for text in body["input"]):
                    server.fail = False
                    self.send_error(500)
                    return
                data = [{"index": i, "embedding": [float(i + 1)] * server.dims}
                        for i, _ in enumerate(body["input"])]
                payload = json.dumps({"data": list(reversed(data))}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        (self.repo / "one.py").write_text("def authentication():\n    return 'logged in'\n", encoding="utf-8")
        self.server.key = "fixture-key"
        self.server.dims = 3
        self.server.inputs = []
        self.server.fail = False
        self.server.on_inputs = None
        self.env = patch.dict(os.environ, {"QUANTMCP_INDEX_DIR": str(self.root / "indexes")})
        self.env.start()
        self.endpoint()

    def tearDown(self):
        for name in ("QUANTMCP_EMBEDDING_ENDPOINT", "QUANTMCP_EMBEDDING_KEY", "QUANTMCP_EMBEDDING_ERROR"):
            os.environ.pop(name, None)
        self.env.stop()
        self.temp.cleanup()

    def endpoint(self, model="fixture"):
        os.environ["QUANTMCP_EMBEDDING_ENDPOINT"] = json.dumps({
            "url": f"http://127.0.0.1:{self.server.server_port}", "model": model,
            "dims": self.server.dims, "queryPrefix": "Instruct: code search\nQuery:"})
        os.environ["QUANTMCP_EMBEDDING_KEY"] = self.server.key

    def index(self, mode="both"):
        return cli._run_index(SimpleNamespace(path=str(self.repo), name="fixture", mode=mode, filter="everything"))

    def search(self, mode="semantic"):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            try:
                cli.cmd_search(SimpleNamespace(codebase="fixture", query="authentication", mode=mode, limit=3))
            except SystemExit:
                pass
        return json.loads(output.getvalue())

    def db(self):
        return open_db(self.root / "indexes" / "fixture.db")


class EmbeddingTests(EmbeddingFixture):
    def test_structural_then_semantic_query_prefix_and_private_metadata(self):
        self.assertEqual(self.index("structural")["status"], "ok")
        self.assertEqual(self.index("semantic")["status"], "ok")
        self.assertGreater(self.search()["count"], 0)
        self.assertTrue(any(text.startswith("Instruct:") for text in self.server.inputs))
        self.assertTrue(any(text.startswith("def authentication") for text in self.server.inputs))
        with contextlib.closing(self.db()) as conn:
            meta = get_all_meta(conn)
            self.assertEqual(meta["embed_dimensions"], "3")
            self.assertEqual(meta["embed_provider"], "builtin")
            self.assertNotIn("embed_base_url", meta)
            self.assertNotIn(self.server.key, json.dumps(meta))

    def test_changed_model_and_dimensions_rebuild_without_losing_structural(self):
        self.index()
        self.server.dims = 5
        self.server.key = "new-start-key"
        self.endpoint("different-model")
        self.assertGreater(self.search()["count"], 0)
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(get_all_meta(conn)["embed_dimensions"], "5")
            self.assertGreater(conn.execute("SELECT count(*) FROM code_fts").fetchone()[0], 0)
            self.assertIn("float[5]", conn.execute("SELECT sql FROM sqlite_master WHERE name='vec_index'").fetchone()[0])

    def test_unavailable_engine_falls_back_from_semantic_only_to_structural(self):
        self.index("semantic")
        os.environ.pop("QUANTMCP_EMBEDDING_ENDPOINT")
        self.assertIn("Memory settings", self.search()["error"])
        result = self.search("auto")
        self.assertEqual(result["mode"], "structural")
        self.assertGreater(result["count"], 0)

    def test_failed_file_is_retried_even_when_its_hash_is_unchanged(self):
        (self.repo / "one.py").write_text("def FAIL_ONCE():\n    return 1\n", encoding="utf-8")
        self.server.fail = True
        self.assertIn("error", self.index())
        retry = self.index()
        self.assertEqual(retry["status"], "ok")
        self.assertGreater(retry["semantic_entries"], 0)

    def test_search_retries_a_failed_embedding_after_the_file_mtime(self):
        (self.repo / "one.py").write_text("def FAIL_ONCE():\n    return 1\n", encoding="utf-8")
        self.server.fail = True
        self.assertIn("error", self.index())
        # Mtime checks alone would miss this incomplete file after the failed pass.
        with contextlib.closing(self.db()) as conn:
            set_meta(conn, "semantic_indexed_at", "9999999999")
        self.assertGreater(self.search()["count"], 0)
        with contextlib.closing(self.db()) as conn:
            self.assertIsNotNone(conn.execute("SELECT semantic_hash FROM files").fetchone()[0])

    def test_structural_refresh_does_not_hide_semantic_changes(self):
        self.index()
        (self.repo / "one.py").write_text("def replacement():\n    return 2\n", encoding="utf-8")
        self.index("structural")
        self.assertGreater(self.index("semantic")["semantic_entries"], 0)
        with contextlib.closing(self.db()) as conn:
            contents = " ".join(row[0] for row in conn.execute("SELECT content FROM chunks"))
            self.assertIn("replacement", contents)
            self.assertNotIn("authentication", contents)

    def test_semantic_search_auto_refreshes_changed_files_and_deletions(self):
        self.index()
        (self.repo / "two.py").write_text("def new_code():\n    return 3\n", encoding="utf-8")
        with contextlib.closing(self.db()) as conn:
            set_meta(conn, "semantic_indexed_at", "1")
        self.assertGreater(self.search()["count"], 0)
        (self.repo / "one.py").unlink()
        self.search()
        with contextlib.closing(self.db()) as conn:
            paths = [r[0] for r in conn.execute("SELECT file_path FROM files")]
            self.assertEqual(len(paths), 1)
            self.assertTrue(paths[0].endswith("two.py"))

    def test_force_reindex_without_engine_preserves_existing_vectors(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            before = conn.execute("SELECT count(*) FROM vec_index").fetchone()[0]
        os.environ.pop("QUANTMCP_EMBEDDING_KEY")
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
            cli.cmd_reindex(SimpleNamespace(codebase="fixture", mode="semantic", filter=None))
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(before, conn.execute("SELECT count(*) FROM vec_index").fetchone()[0])

    def test_inference_does_not_hold_a_database_write_transaction(self):
        self.index()
        (self.repo / "one.py").write_text("def replacement():\n    return 2\n", encoding="utf-8")
        embed = EmbedProvider.embed_batch
        probes = []

        def checked_embed(provider, texts):
            # A second connection can write while the slow model is running.
            with contextlib.closing(sqlite3.connect(self.root / "indexes" / "fixture.db", timeout=0)) as probe:
                probe.execute("BEGIN IMMEDIATE")
                probe.rollback()
                probes.append(True)
            return embed(provider, texts)

        with patch.object(EmbedProvider, "embed_batch", autospec=True, side_effect=checked_embed):
            self.assertEqual(self.index()["status"], "ok")
        self.assertTrue(probes)

    def test_failed_force_reindex_preserves_old_file_vectors_and_retries(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            before = [tuple(row) for row in conn.execute("SELECT * FROM chunks")]
            vectors = [tuple(row) for row in conn.execute("SELECT * FROM vec_index")]
        (self.repo / "one.py").write_text("def FAIL_ONCE():\n    return 1\n", encoding="utf-8")
        self.server.fail = True
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
            cli.cmd_reindex(SimpleNamespace(codebase="fixture", mode="both", filter=None))
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(before, [tuple(row) for row in conn.execute("SELECT * FROM chunks")])
            self.assertEqual(vectors, [tuple(row) for row in conn.execute("SELECT * FROM vec_index")])
            self.assertIsNone(conn.execute("SELECT semantic_hash FROM files").fetchone()[0])
        self.assertEqual(self.index()["status"], "ok")
        with contextlib.closing(self.db()) as conn:
            self.assertIn("FAIL_ONCE", conn.execute("SELECT content FROM chunks").fetchone()[0])

    def test_failed_vector_write_rolls_back_the_whole_file_replacement(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            before = [tuple(row) for row in conn.execute("SELECT * FROM chunks")]
            vectors = [tuple(row) for row in conn.execute("SELECT * FROM vec_index")]
        (self.repo / "one.py").write_text("def replacement():\n    return 2\n", encoding="utf-8")
        with patch.object(SqliteVecBackend, "insert", side_effect=RuntimeError("fixture write failure")):
            self.assertIn("error", self.index())
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(before, [tuple(row) for row in conn.execute("SELECT * FROM chunks")])
            self.assertEqual(vectors, [tuple(row) for row in conn.execute("SELECT * FROM vec_index")])
            self.assertIsNone(conn.execute("SELECT semantic_hash FROM files").fetchone()[0])
            self.assertEqual(conn.execute("PRAGMA integrity_check").fetchone()[0], "ok")

    def test_empty_file_removes_old_vectors(self):
        self.index()
        (self.repo / "one.py").write_text("", encoding="utf-8")
        self.assertEqual(self.index()["status"], "ok")
        with contextlib.closing(self.db()) as conn:
            self.assertEqual(conn.execute("SELECT count(*) FROM chunks").fetchone()[0], 0)
            self.assertEqual(conn.execute("SELECT count(*) FROM vec_index").fetchone()[0], 0)
            row = conn.execute("SELECT file_hash, semantic_hash FROM files").fetchone()
            self.assertEqual(row[0], row[1])

    def test_unreadable_file_does_not_roll_back_a_completed_file(self):
        self.index()
        first = str(self.repo / "one.py")
        missing = str(self.repo / "removed_during_index.py")
        Path(first).write_text("def completed_update():\n    return 2\n", encoding="utf-8")
        with patch.object(cli, "walk_codebase", return_value=[first, missing]):
            result = self.index("structural")
        self.assertEqual(result["errors"], 1)
        with contextlib.closing(self.db()) as conn:
            self.assertIsNotNone(conn.execute("SELECT structural_hash FROM files").fetchone()[0])
            names = [row[0] for row in conn.execute("SELECT symbol_name FROM code_fts")]
            self.assertIn("completed_update", names)

    def test_long_unicode_lines_are_bounded_without_losing_text(self):
        text = "\u6f22\u5b57\U0001f600" * 3000
        chunks = chunk_file(text)
        self.assertEqual("".join(c["content"] for c in chunks), text)
        self.assertTrue(all(len(c["content"].encode()) <= MAX_INPUT_BYTES for c in chunks))
        provider = EmbedProvider()
        try:
            self.assertEqual(len(provider.embed_batch([c["content"] for c in chunks])), len(chunks))
        finally:
            provider.close()

    def test_invalid_dimensions_are_rejected(self):
        provider = EmbedProvider()
        self.server.dims = 4
        try:
            with self.assertRaisesRegex(ValueError, "dimensions"):
                provider.embed("probe")
        finally:
            provider.close()

    def test_legacy_index_provider_metadata_is_retired(self):
        self.index()
        with contextlib.closing(self.db()) as conn:
            set_meta(conn, "embed_provider", "legacy")
            set_meta(conn, "embed_base_url", "http://obsolete.invalid")
        self.assertGreater(self.search()["count"], 0)
        with contextlib.closing(self.db()) as conn:
            meta = get_all_meta(conn)
            self.assertEqual(meta["embed_provider"], "builtin")
            self.assertNotIn("embed_base_url", meta)


if __name__ == "__main__":
    unittest.main()
