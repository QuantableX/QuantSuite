"""Structural indexing using Tree-sitter for AST parsing and FTS5 for BM25 search.

Extracts symbols (functions, classes, variables, imports) from source files
and indexes them for full-text search. Zero AI/network dependencies.
"""

import re
import sqlite3

from tree_sitter import Language, Parser

from db import clear_fts_for_file, insert_fts

# Map language name to its tree-sitter module import
_LANGUAGE_MODULES: dict[str, str] = {
    "python": "tree_sitter_python",
    "javascript": "tree_sitter_javascript",
    "typescript": "tree_sitter_typescript",
    "rust": "tree_sitter_rust",
    "go": "tree_sitter_go",
    "java": "tree_sitter_java",
    "c": "tree_sitter_c",
    "cpp": "tree_sitter_cpp",
    "c_sharp": "tree_sitter_c_sharp",
    "ruby": "tree_sitter_ruby",
    "php": "tree_sitter_php",
    "swift": "tree_sitter_swift",
    "kotlin": "tree_sitter_kotlin",
    "scala": "tree_sitter_scala",
    "lua": "tree_sitter_lua",
    "bash": "tree_sitter_bash",
    "html": "tree_sitter_html",
    "css": "tree_sitter_css",
    "toml": "tree_sitter_toml",
    "yaml": "tree_sitter_yaml",
    "json": "tree_sitter_json",
}

# Cache for loaded parsers
_PARSER_CACHE: dict[str, Parser] = {}

# Languages with tree-sitter packages available
TREE_SITTER_LANGUAGES: set[str] = set(_LANGUAGE_MODULES.keys())

# Tree-sitter node types that represent symbols, per language
SYMBOL_NODES: dict[str, dict[str, list[str]]] = {
    "python": {
        "function": ["function_definition"],
        "class": ["class_definition"],
        "import": ["import_statement", "import_from_statement"],
        "variable": ["assignment", "augmented_assignment"],
    },
    "javascript": {
        "function": ["function_declaration", "method_definition", "arrow_function"],
        "class": ["class_declaration"],
        "import": ["import_statement"],
        "variable": ["variable_declaration", "lexical_declaration"],
    },
    "typescript": {
        "function": ["function_declaration", "method_definition", "arrow_function"],
        "class": ["class_declaration"],
        "import": ["import_statement"],
        "variable": ["variable_declaration", "lexical_declaration"],
        "interface": ["interface_declaration"],
    },
    "rust": {
        "function": ["function_item"],
        "class": ["struct_item", "enum_item", "impl_item"],
        "import": ["use_declaration"],
        "variable": ["let_declaration", "const_item", "static_item"],
    },
    "go": {
        "function": ["function_declaration", "method_declaration"],
        "class": ["type_declaration"],
        "import": ["import_declaration"],
        "variable": ["var_declaration", "const_declaration", "short_var_declaration"],
    },
    "java": {
        "function": ["method_declaration", "constructor_declaration"],
        "class": ["class_declaration", "interface_declaration", "enum_declaration"],
        "import": ["import_declaration"],
        "variable": ["field_declaration", "local_variable_declaration"],
    },
    "c": {
        "function": ["function_definition"],
        "class": ["struct_specifier", "enum_specifier", "union_specifier"],
        "import": ["preproc_include"],
        "variable": ["declaration"],
    },
    "cpp": {
        "function": ["function_definition"],
        "class": ["class_specifier", "struct_specifier", "enum_specifier"],
        "import": ["preproc_include", "using_declaration"],
        "variable": ["declaration"],
    },
    "c_sharp": {
        "function": ["method_declaration", "constructor_declaration"],
        "class": ["class_declaration", "interface_declaration", "struct_declaration", "enum_declaration"],
        "import": ["using_directive"],
        "variable": ["field_declaration", "variable_declaration"],
    },
    "ruby": {
        "function": ["method"],
        "class": ["class", "module"],
        "import": ["call"],  # require/include
        "variable": ["assignment"],
    },
    "php": {
        "function": ["function_definition", "method_declaration"],
        "class": ["class_declaration", "interface_declaration", "trait_declaration"],
        "import": ["namespace_use_declaration"],
        "variable": ["property_declaration"],
    },
    "swift": {
        "function": ["function_declaration"],
        "class": ["class_declaration", "struct_declaration", "enum_declaration", "protocol_declaration"],
        "import": ["import_declaration"],
        "variable": ["property_declaration"],
    },
    "kotlin": {
        "function": ["function_declaration"],
        "class": ["class_declaration", "object_declaration"],
        "import": ["import_header"],
        "variable": ["property_declaration"],
    },
    "scala": {
        "function": ["function_definition"],
        "class": ["class_definition", "object_definition", "trait_definition"],
        "import": ["import_declaration"],
        "variable": ["val_definition", "var_definition"],
    },
    "lua": {
        "function": ["function_declaration", "local_function"],
        "variable": ["variable_declaration", "local_variable_declaration"],
    },
}

# Regex fallback patterns for languages without tree-sitter support
REGEX_PATTERNS: dict[str, list[tuple[str, str]]] = {
    # (pattern, symbol_type)
    "python": [
        (r"^(?:async\s+)?def\s+(\w+)", "function"),
        (r"^class\s+(\w+)", "class"),
        (r"^(?:from\s+\S+\s+)?import\s+(.+)", "import"),
    ],
    "javascript": [
        (r"(?:export\s+)?(?:async\s+)?function\s+(\w+)", "function"),
        (r"(?:export\s+)?class\s+(\w+)", "class"),
        (r"(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?(?:\([^)]*\)|[\w]+)\s*=>", "function"),
        (r"^import\s+(.+)", "import"),
    ],
    "typescript": [
        (r"(?:export\s+)?(?:async\s+)?function\s+(\w+)", "function"),
        (r"(?:export\s+)?class\s+(\w+)", "class"),
        (r"(?:export\s+)?interface\s+(\w+)", "interface"),
        (r"(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?(?:\([^)]*\)|[\w]+)\s*=>", "function"),
        (r"^import\s+(.+)", "import"),
    ],
    "rust": [
        (r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", "function"),
        (r"(?:pub\s+)?struct\s+(\w+)", "class"),
        (r"(?:pub\s+)?enum\s+(\w+)", "class"),
        (r"(?:pub\s+)?trait\s+(\w+)", "class"),
        (r"impl(?:<[^>]+>)?\s+(\w+)", "class"),
        (r"^use\s+(.+);", "import"),
    ],
    "go": [
        (r"func\s+(?:\([^)]+\)\s+)?(\w+)", "function"),
        (r"type\s+(\w+)\s+struct", "class"),
        (r"type\s+(\w+)\s+interface", "class"),
    ],
    "java": [
        (r"(?:public|private|protected)?\s*(?:static\s+)?(?:\w+\s+)+(\w+)\s*\(", "function"),
        (r"(?:public\s+)?(?:abstract\s+)?class\s+(\w+)", "class"),
        (r"(?:public\s+)?interface\s+(\w+)", "class"),
        (r"^import\s+(.+);", "import"),
    ],
    "c": [
        (r"(?:\w+[\s*]+)+(\w+)\s*\([^)]*\)\s*\{", "function"),
        (r"(?:typedef\s+)?struct\s+(\w+)", "class"),
        (r"#include\s+(.+)", "import"),
    ],
    "cpp": [
        (r"(?:\w+[\s*]+)+(\w+)\s*\([^)]*\)\s*(?:const\s*)?\{", "function"),
        (r"class\s+(\w+)", "class"),
        (r"struct\s+(\w+)", "class"),
        (r"#include\s+(.+)", "import"),
    ],
}

# Add fallback patterns for other languages using generic patterns
_GENERIC_PATTERNS: list[tuple[str, str]] = [
    (r"(?:def|func|function|fn|fun)\s+(\w+)", "function"),
    (r"(?:class|struct|interface|trait|enum|type)\s+(\w+)", "class"),
    (r"(?:import|require|use|include|from)\s+(.+)", "import"),
]


def _get_parser(language: str):
    """Try to get a tree-sitter parser for the given language."""
    if language in _PARSER_CACHE:
        return _PARSER_CACHE[language]

    module_name = _LANGUAGE_MODULES.get(language)
    if not module_name:
        return None

    try:
        import importlib
        mod = importlib.import_module(module_name)

        # Some packages (like typescript) have sub-languages
        if language == "typescript" and hasattr(mod, "language_typescript"):
            lang = Language(mod.language_typescript())
        elif hasattr(mod, "language"):
            lang = Language(mod.language())
        else:
            return None

        parser = Parser(lang)
        _PARSER_CACHE[language] = parser
        return parser
    except Exception:
        return None


def _extract_symbol_name(node, language: str) -> str:
    """Extract the name from a tree-sitter symbol node."""
    # Try common field names for the symbol's name
    name_node = node.child_by_field_name("name")
    if name_node:
        return name_node.text.decode("utf-8", errors="replace")

    # For some node types, the name is in a specific child position
    # e.g., Python assignment: first child is the target
    if node.type in ("assignment", "augmented_assignment"):
        if node.child_count > 0:
            left = node.children[0]
            return left.text.decode("utf-8", errors="replace")

    # For import statements, return the full text
    if "import" in node.type or "use" in node.type or "include" in node.type:
        return node.text.decode("utf-8", errors="replace").strip()

    # Fallback: return the first identifier child
    for child in node.children:
        if child.type == "identifier" or child.type == "type_identifier":
            return child.text.decode("utf-8", errors="replace")

    # Last resort: first 60 chars of the node text
    text = node.text.decode("utf-8", errors="replace").strip()
    return text[:60] if len(text) > 60 else text


def _get_symbol_type(node_type: str, symbol_nodes: dict[str, list[str]]) -> str | None:
    """Determine the symbol type from a tree-sitter node type."""
    for sym_type, node_types in symbol_nodes.items():
        if node_type in node_types:
            return sym_type
    return None


def extract_symbols_treesitter(
    content: str, language: str
) -> list[dict[str, str]]:
    """Extract symbols from source code using tree-sitter.

    Returns a list of dicts with keys: name, type, content
    """
    parser = _get_parser(language)
    if parser is None:
        return []

    symbol_defs = SYMBOL_NODES.get(language, {})
    if not symbol_defs:
        return []

    source_bytes = content.encode("utf-8")
    tree = parser.parse(source_bytes)
    symbols: list[dict[str, str]] = []

    def walk(node, depth: int = 0):
        # Only extract top-level and class-level symbols (depth <= 1)
        sym_type = _get_symbol_type(node.type, symbol_defs)
        if sym_type and depth <= 1:
            name = _extract_symbol_name(node, language)
            node_text = node.text.decode("utf-8", errors="replace")
            symbols.append({
                "name": name,
                "type": sym_type,
                "content": node_text,
            })

        # Recurse into children for nested symbols (e.g., methods in classes)
        for child in node.children:
            walk(child, depth + 1)

    walk(tree.root_node)
    return symbols


def extract_symbols_regex(
    content: str, language: str
) -> list[dict[str, str]]:
    """Fallback: extract symbols using regex patterns."""
    patterns = REGEX_PATTERNS.get(language, _GENERIC_PATTERNS)
    symbols: list[dict[str, str]] = []
    lines = content.split("\n")

    for i, line in enumerate(lines):
        stripped = line.strip()
        if not stripped or stripped.startswith(("#", "//", "/*", "*", "<!--")):
            continue

        for pattern, sym_type in patterns:
            m = re.match(pattern, stripped)
            if m:
                name = m.group(1).strip()
                # Grab context: the matching line + a few following lines
                context_end = min(i + 15, len(lines))
                context = "\n".join(lines[i:context_end])
                symbols.append({
                    "name": name,
                    "type": sym_type,
                    "content": context,
                })
                break  # Only match first pattern per line

    return symbols


def extract_symbols(content: str, language: str) -> list[dict[str, str]]:
    """Extract symbols using tree-sitter with regex fallback."""
    symbols = extract_symbols_treesitter(content, language)
    if symbols:
        return symbols
    return extract_symbols_regex(content, language)


def index_file_structural(
    conn: sqlite3.Connection,
    file_path: str,
    content: str,
    language: str,
) -> int:
    """Index a single file for structural search.

    Extracts symbols and inserts them into the FTS5 index.
    Returns the number of symbols indexed.
    """
    # Clear any existing FTS entries for this file
    clear_fts_for_file(conn, file_path)

    # Extract symbols
    symbols = extract_symbols(content, language)

    # Insert each symbol into FTS
    for sym in symbols:
        insert_fts(
            conn,
            file_path=file_path,
            symbol_name=sym["name"],
            symbol_type=sym["type"],
            language=language,
            content=sym["content"],
        )

    # If no symbols were extracted, index the whole file content
    # so it's still searchable via BM25
    if not symbols:
        # Split into reasonable chunks for FTS
        lines = content.split("\n")
        chunk_size = 50
        for i in range(0, len(lines), chunk_size):
            chunk = "\n".join(lines[i : i + chunk_size])
            if chunk.strip():
                insert_fts(
                    conn,
                    file_path=file_path,
                    symbol_name=f"lines_{i+1}_{min(i+chunk_size, len(lines))}",
                    symbol_type="code",
                    language=language,
                    content=chunk,
                )

    return len(symbols)
