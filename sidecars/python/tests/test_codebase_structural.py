"""Structural symbol extraction on deeply nested sources (no recursion limit)."""
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "codebase_index"))
import cli
from db import open_db
import structural
from structural import extract_symbols, extract_symbols_treesitter


def else_if_chain(branches: int) -> str:
    """A settings setter like the src\\Settings.cpp that failed to index."""
    lines = ["#include <string>", "", "void Settings::set(const std::string& key, int v) {"]
    lines.append('  if (key == "k0") a0 = v;')
    lines += [f'  else if (key == "k{i}") a{i} = v;' for i in range(1, branches)]
    lines.append("}")
    return "\n".join(lines) + "\n"


def stream_chain(operands: int) -> str:
    chain = " << ".join(f'"v{i}"' for i in range(operands))
    return f"void Settings::save(std::ostream& out) {{\n  out << {chain};\n}}\n"


def tree_depth(content: str, language: str) -> int:
    tree = structural._get_parser(language).parse(content.encode("utf-8"))
    deepest, stack = 0, [(tree.root_node, 0)]
    while stack:
        node, depth = stack.pop()
        deepest = max(deepest, depth)
        stack.extend((child, depth + 1) for child in node.children)
    return deepest


@unittest.skipIf(structural._get_parser("cpp") is None, "tree-sitter-cpp is not installed")
class DeepTreeTests(unittest.TestCase):
    def assert_function(self, symbols, prefix):
        functions = [s for s in symbols if s["type"] == "function"]
        self.assertEqual(len(functions), 1, symbols)
        self.assertTrue(functions[0]["content"].startswith(prefix), functions[0]["content"][:80])

    def test_long_else_if_chain_keeps_its_function(self):
        content = else_if_chain(5000)
        self.assertGreater(tree_depth(content, "cpp"), sys.getrecursionlimit())
        symbols = extract_symbols_treesitter(content, "cpp")
        self.assertEqual([s["type"] for s in symbols], ["import", "function"])
        self.assert_function(symbols, "void Settings::set(")

    def test_long_stream_chain_keeps_its_function(self):
        content = stream_chain(5000)
        self.assertGreater(tree_depth(content, "cpp"), sys.getrecursionlimit())
        self.assert_function(extract_symbols_treesitter(content, "cpp"), "void Settings::save(")

    def test_only_top_level_symbols_in_source_order(self):
        content = (
            "import os\n\n"
            "class Store:\n"
            "    def load(self):\n"
            "        return os.getcwd()\n\n"
            "def main():\n"
            "    return Store().load()\n"
        )
        symbols = extract_symbols_treesitter(content, "python")
        self.assertEqual(
            [(s["type"], s["name"]) for s in symbols],
            [("import", "os"), ("class", "Store"), ("function", "main")],
        )

    def test_parser_failure_falls_back_to_regex(self):
        content = "void apply(int v) {\n  value = v;\n}\n"
        with patch.object(structural, "extract_symbols_treesitter", side_effect=RecursionError("deep")):
            symbols = extract_symbols(content, "cpp")
        self.assertEqual([(s["type"], s["name"]) for s in symbols], [("function", "apply")])

    def test_deep_file_indexes_without_error(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            repo = root / "repo"
            (repo / "src").mkdir(parents=True)
            (repo / "src" / "Settings.cpp").write_text(else_if_chain(5000), encoding="utf-8")
            (repo / "main.py").write_text("def main():\n    return 0\n", encoding="utf-8")
            with patch.dict(os.environ, {"QUANTMCP_INDEX_DIR": str(root / "indexes")}):
                result = cli._run_index(SimpleNamespace(
                    path=str(repo), name="deep", mode="structural", filter="everything",
                ))
                conn = open_db(cli.get_db_path("deep"))
                try:
                    rows = conn.execute(
                        "SELECT symbol_type FROM code_fts WHERE file_path LIKE '%Settings.cpp'"
                    ).fetchall()
                    unhashed = conn.execute(
                        "SELECT COUNT(*) FROM files WHERE structural_hash IS NULL"
                    ).fetchone()[0]
                finally:
                    conn.close()

        self.assertEqual(result["status"], "ok", result)
        self.assertEqual((result["errors"], result["files_indexed"]), (0, 2), result)
        self.assertEqual(sorted(r[0] for r in rows), ["function", "import"])
        self.assertEqual(unhashed, 0)


if __name__ == "__main__":
    unittest.main()
