"""Command-line interface for codebase indexing operations.

Used by the QuantMCP Tauri backend to trigger indexing and queries directly,
without going through the MCP protocol. Also useful for standalone usage.

Usage:
    python cli.py index <path> [--mode structural|semantic|both] [--provider ollama] [--model nomic-embed-text] [--base-url http://localhost:11434]
    python cli.py search <query> --codebase <name> [--limit 20] [--mode structural|semantic|auto]
    python cli.py lookup <symbol> --codebase <name>
    python cli.py list
    python cli.py stats --codebase <name>
    python cli.py reindex --codebase <name> [--mode structural|semantic|both]
"""

import sys
import os
import json
import argparse

# Add the codebase-index directory to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from utils import (
    walk_codebase,
    detect_language,
    compute_file_hash,
    get_data_dir,
    codebase_name_from_path,
    get_db_path,
    list_db_files,
    format_file_path,
    is_index_stale,
)
from db import (
    init_db,
    open_db,
    get_meta,
    set_meta,
    get_all_meta,
    get_file,
    upsert_file,
    search_fts,
    lookup_symbol as db_lookup_symbol,
    clear_fts,
    clear_chunks,
    clear_vec_index,
    get_db_stats,
)

# Heavy deps (tree-sitter, embeddings) are imported lazily inside
# cmd_index / cmd_search / cmd_reindex so that lightweight commands
# (stats, list, lookup) work even when tree-sitter is not installed.

import time


def _compute_effective_mode(stored_mode, requested_mode):
    """Compute the combined mode after indexing.

    If structural already exists and we're adding semantic (or vice versa), result is 'both'.
    """
    if requested_mode == "both":
        return "both"
    if stored_mode == "both":
        return "both"
    if stored_mode and stored_mode != requested_mode:
        return "both"
    return requested_mode


def _run_index(args) -> dict:
    """Core indexing logic. Returns result dict (may contain 'error' key)."""
    from structural import index_file_structural
    from semantic import index_file_semantic
    from embeddings import EmbedProvider
    from vector_backend import SqliteVecBackend

    path = os.path.abspath(args.path)
    mode = args.mode

    if not os.path.isdir(path):
        return {"error": f"Not a directory: {path}"}

    do_structural = mode in ("structural", "both")
    do_semantic = mode in ("semantic", "both")

    # QuantSuite passes --name (the workspace id's b36 tail) so the DB is keyed
    # by the workspace registry, not the folder basename.
    codebase_name = getattr(args, "name", None) or codebase_name_from_path(path)
    db_path = get_db_path(codebase_name)

    dimensions = 768
    provider_instance = None
    vec_backend = None

    if do_semantic:
        try:
            provider_instance = EmbedProvider(
                provider=args.provider,
                base_url=args.base_url,
                model=args.model,
            )
            dimensions = provider_instance.get_dimensions()
        except Exception as e:
            return {"error": f"Cannot connect to embedding provider: {e}"}

    is_new = not db_path.exists()
    conn = init_db(db_path, dimensions=dimensions)

    if do_semantic:
        vec_backend = SqliteVecBackend(conn)

    # Determine if this is the first time indexing each mode. Falsy covers
    # both None (never indexed) and "" (cleared by cmd_reindex) — treating ""
    # as already-indexed made reindex clear the FTS data and then skip every
    # unchanged file, leaving an empty index.
    is_first_structural = not get_meta(conn, "structural_indexed_at")
    is_first_semantic = not get_meta(conn, "semantic_indexed_at")

    # Compute effective combined mode
    stored_mode = get_meta(conn, "mode")
    effective_mode = _compute_effective_mode(stored_mode, mode)
    set_meta(conn, "mode", effective_mode)
    set_meta(conn, "codebase_path", path)
    set_meta(conn, "vector_backend", "sqlite")

    if do_semantic:
        set_meta(conn, "embed_provider", args.provider)
        set_meta(conn, "embed_model", args.model)
        set_meta(conn, "embed_base_url", args.base_url)
        set_meta(conn, "embed_dimensions", str(dimensions))

    filter_mode = getattr(args, "filter", "everything") or "everything"
    set_meta(conn, "filter_mode", filter_mode)
    all_files = walk_codebase(path, filter_mode=filter_mode)
    if not all_files:
        conn.close()
        return {"error": f"No indexable files found in {path}"}

    indexed = 0
    skipped = 0
    errors = 0
    structural_entries = 0
    semantic_entries = 0

    for file_path in all_files:
        try:
            file_hash = compute_file_hash(file_path)
            existing = get_file(conn, file_path)
            file_unchanged = existing and existing["file_hash"] == file_hash

            # Determine what to skip per mode
            skip_structural = not do_structural or (file_unchanged and not is_first_structural)
            skip_semantic = not do_semantic or (file_unchanged and not is_first_semantic)

            if skip_structural and skip_semantic:
                skipped += 1
                continue

            with open(file_path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()

            language = detect_language(file_path) or "unknown"
            rel_path = format_file_path(file_path, path)
            file_id = upsert_file(conn, file_path, file_hash)

            if do_structural and not skip_structural:
                count = index_file_structural(conn, rel_path, content, language)
                structural_entries += count

            if do_semantic and not skip_semantic:
                count = index_file_semantic(
                    conn, rel_path, file_id, content, language,
                    provider_instance, vec_backend,
                )
                semantic_entries += count

            indexed += 1
        except Exception as e:
            errors += 1

    now = str(int(time.time()))
    set_meta(conn, "last_indexed", now)
    if do_structural:
        set_meta(conn, "structural_indexed_at", now)
    if do_semantic:
        set_meta(conn, "semantic_indexed_at", now)
    conn.commit()
    conn.close()

    if provider_instance:
        provider_instance.close()

    total_entries = structural_entries + semantic_entries

    return {
        "status": "ok",
        "action": "created" if is_new else "updated",
        "codebase": codebase_name,
        "mode": effective_mode,
        "files_indexed": indexed,
        "files_skipped": skipped,
        "errors": errors,
        "total_entries": total_entries,
        "structural_entries": structural_entries,
        "semantic_entries": semantic_entries,
        "db_path": str(db_path),
    }


def cmd_index(args):
    """Index a codebase directory."""
    result = _run_index(args)
    print(json.dumps(result))
    if "error" in result:
        sys.exit(1)


def _auto_refresh_if_stale(codebase_name: str) -> dict | None:
    """Auto-refresh structural index if codebase files have changed.

    Returns the refresh result dict if a refresh was performed, None otherwise.
    """
    db_path = get_db_path(codebase_name)
    if not db_path.exists():
        return None

    conn = open_db(db_path)
    meta = get_all_meta(conn)

    last_indexed = meta.get("last_indexed")
    codebase_path = meta.get("codebase_path", "")
    filter_mode = meta.get("filter_mode", "everything")

    if not last_indexed or not codebase_path or not os.path.isdir(codebase_path):
        conn.close()
        return None

    # Only auto-refresh if structural index exists
    has_structural = bool(meta.get("structural_indexed_at"))
    stored_mode = meta.get("mode", "")
    if not has_structural and stored_mode not in ("structural", "both"):
        conn.close()
        return None

    file_count = conn.execute("SELECT COUNT(*) as cnt FROM files").fetchone()["cnt"]
    conn.close()

    try:
        last_indexed_ts = int(last_indexed)
    except ValueError:
        return None

    if not is_index_stale(codebase_path, last_indexed_ts, file_count, filter_mode):
        return None

    # Stale — do incremental structural re-index
    class IndexArgs:
        pass

    index_args = IndexArgs()
    index_args.path = codebase_path
    index_args.name = codebase_name
    index_args.mode = "structural"
    index_args.provider = "ollama"
    index_args.model = "nomic-embed-text"
    index_args.base_url = "http://localhost:11434"
    index_args.filter = filter_mode

    return _run_index(index_args)


def _search_structural(conn, query, limit):
    """Perform structural (BM25) search and return JSON-ready dict."""
    try:
        results = search_fts(conn, query, limit=limit)
    except Exception as e:
        return {"error": str(e)}

    return {
        "status": "ok",
        "mode": "structural",
        "count": len(results),
        "results": results,
    }


def _search_semantic(conn, query, limit):
    """Perform semantic (vector) search and return JSON-ready dict."""
    from semantic import search_semantic
    from embeddings import EmbedProvider
    from vector_backend import SqliteVecBackend

    meta = get_all_meta(conn)
    try:
        provider = EmbedProvider(
            provider=meta.get("embed_provider", "ollama"),
            base_url=meta.get("embed_base_url", "http://localhost:11434"),
            model=meta.get("embed_model", "nomic-embed-text"),
        )
    except Exception as e:
        return {"error": f"Cannot connect to embedding provider: {e}"}

    vec_backend = SqliteVecBackend(conn)
    try:
        results = search_semantic(conn, query, provider, vec_backend, top_k=limit)
    except Exception as e:
        provider.close()
        return {"error": f"Semantic search error: {e}"}

    provider.close()
    return {
        "status": "ok",
        "mode": "semantic",
        "count": len(results),
        "results": results,
    }


def cmd_search(args):
    """Search an indexed codebase."""
    _auto_refresh_if_stale(args.codebase)

    db_path = get_db_path(args.codebase)
    if not db_path.exists():
        print(json.dumps({"error": f"Codebase '{args.codebase}' not found"}))
        sys.exit(1)

    conn = open_db(db_path)
    stored_mode = get_meta(conn, "mode")
    search_mode = args.mode  # "structural", "semantic", or "auto"

    has_structural = get_meta(conn, "structural_indexed_at") is not None
    has_semantic = get_meta(conn, "semantic_indexed_at") is not None

    # Backward compatibility: if no per-mode timestamps, infer from stored mode
    if not has_structural and not has_semantic:
        if stored_mode in ("structural", "both"):
            has_structural = True
        if stored_mode in ("semantic", "both"):
            has_semantic = True

    if search_mode == "auto":
        # Prefer structural (faster, no external service needed)
        if has_structural:
            search_mode = "structural"
        elif has_semantic:
            search_mode = "semantic"
        else:
            conn.close()
            print(json.dumps({"error": f"Codebase '{args.codebase}' has no index data"}))
            sys.exit(1)

    if search_mode == "structural":
        if not has_structural:
            conn.close()
            print(json.dumps({"error": f"Codebase '{args.codebase}' has no structural index. Index with --mode structural first."}))
            sys.exit(1)
        result = _search_structural(conn, args.query, args.limit)
    elif search_mode == "semantic":
        if not has_semantic:
            conn.close()
            print(json.dumps({"error": f"Codebase '{args.codebase}' has no semantic index. Index with --mode semantic first."}))
            sys.exit(1)
        result = _search_semantic(conn, args.query, args.limit)
    else:
        conn.close()
        print(json.dumps({"error": f"Unknown search mode: {search_mode}"}))
        sys.exit(1)

    conn.close()

    if "error" in result:
        print(json.dumps(result))
        sys.exit(1)

    print(json.dumps(result))


def cmd_lookup(args):
    """Look up a symbol by exact name."""
    _auto_refresh_if_stale(args.codebase)

    db_path = get_db_path(args.codebase)
    if not db_path.exists():
        print(json.dumps({"error": f"Codebase '{args.codebase}' not found"}))
        sys.exit(1)

    conn = open_db(db_path)

    # Check if structural data exists (lookup only works with structural index)
    has_structural = get_meta(conn, "structural_indexed_at") is not None
    stored_mode = get_meta(conn, "mode")
    if not has_structural and stored_mode not in ("structural", "both"):
        conn.close()
        print(json.dumps({"error": "lookup_symbol requires a structural index. Index with --mode structural first."}))
        sys.exit(1)

    results = db_lookup_symbol(conn, args.symbol)
    conn.close()

    print(json.dumps({
        "status": "ok",
        "count": len(results),
        "results": results,
    }))


def cmd_list(args):
    """List all indexed codebases."""
    db_files = list_db_files()
    codebases = []

    for db_file in db_files:
        try:
            conn = open_db(db_file)
            stats = get_db_stats(conn)
            conn.close()
            stats["name"] = db_file.stem
            stats["db_size_bytes"] = db_file.stat().st_size
            codebases.append(stats)
        except Exception as e:
            codebases.append({"name": db_file.stem, "error": str(e)})

    print(json.dumps({
        "status": "ok",
        "count": len(codebases),
        "codebases": codebases,
    }))


def cmd_stats(args):
    """Get stats for a specific codebase."""
    db_path = get_db_path(args.codebase)
    if not db_path.exists():
        print(json.dumps({"error": f"Codebase '{args.codebase}' not found"}))
        sys.exit(1)

    conn = open_db(db_path)
    stats = get_db_stats(conn)
    conn.close()

    stats["name"] = args.codebase
    stats["db_size_bytes"] = db_path.stat().st_size
    print(json.dumps({"status": "ok", **stats}))


def cmd_reindex(args):
    """Force full re-index of a codebase."""
    db_path = get_db_path(args.codebase)
    if not db_path.exists():
        print(json.dumps({"error": f"Codebase '{args.codebase}' not found"}))
        sys.exit(1)

    conn = open_db(db_path)
    meta = get_all_meta(conn)
    codebase_path = meta.get("codebase_path", "")

    if not os.path.isdir(codebase_path):
        conn.close()
        print(json.dumps({"error": f"Codebase path not found: {codebase_path}"}))
        sys.exit(1)

    # Determine which modes to re-index
    reindex_mode = getattr(args, "mode", None)
    if not reindex_mode:
        # Default: re-index whatever modes are currently indexed
        reindex_mode = meta.get("mode", "structural")

    do_structural = reindex_mode in ("structural", "both")
    do_semantic = reindex_mode in ("semantic", "both")

    # Clear only data for the modes being re-indexed
    if do_structural:
        clear_fts(conn)
        if meta.get("structural_indexed_at"):
            set_meta(conn, "structural_indexed_at", "")
    if do_semantic:
        clear_chunks(conn)
        clear_vec_index(conn)
        if meta.get("semantic_indexed_at"):
            set_meta(conn, "semantic_indexed_at", "")

    # Only clear files table if re-indexing all modes
    if do_structural and do_semantic:
        conn.execute("DELETE FROM files")
    conn.commit()
    conn.close()

    # Simulate args for cmd_index
    class IndexArgs:
        pass

    index_args = IndexArgs()
    index_args.path = codebase_path
    index_args.name = args.codebase
    index_args.mode = reindex_mode
    index_args.provider = getattr(args, "provider", None) or meta.get("embed_provider", "ollama")
    index_args.model = getattr(args, "model", None) or meta.get("embed_model", "nomic-embed-text")
    index_args.base_url = getattr(args, "base_url", None) or meta.get("embed_base_url", "http://localhost:11434")
    index_args.filter = getattr(args, "filter", None) or meta.get("filter_mode", "everything")

    cmd_index(index_args)


def main():
    parser = argparse.ArgumentParser(
        description="QuantMCP Codebase Index CLI"
    )
    subparsers = parser.add_subparsers(dest="command", help="Command to run")

    # index
    p_index = subparsers.add_parser("index", help="Index a codebase directory")
    p_index.add_argument("path", help="Path to codebase directory")
    p_index.add_argument("--mode", default="structural", choices=["structural", "semantic", "both"])
    p_index.add_argument("--provider", default="ollama", choices=["ollama", "lmstudio"])
    p_index.add_argument("--model", default="nomic-embed-text")
    p_index.add_argument("--base-url", default="http://localhost:11434")
    p_index.add_argument("--filter", default="everything", choices=["everything", "smart"],
                         help="File filter mode: 'everything' indexes all recognised files, 'smart' skips config/docs/styles")
    p_index.add_argument("--name", default=None,
                         help="Explicit codebase/DB name (default: derived from the folder basename)")

    # search
    p_search = subparsers.add_parser("search", help="Search an indexed codebase")
    p_search.add_argument("query", help="Search query")
    p_search.add_argument("--codebase", required=True, help="Codebase name")
    p_search.add_argument("--limit", type=int, default=20)
    p_search.add_argument("--mode", default="auto", choices=["structural", "semantic", "auto"],
                          help="Search mode (default: auto)")

    # lookup
    p_lookup = subparsers.add_parser("lookup", help="Look up a symbol by name")
    p_lookup.add_argument("symbol", help="Symbol name")
    p_lookup.add_argument("--codebase", required=True, help="Codebase name")

    # list
    subparsers.add_parser("list", help="List all indexed codebases")

    # stats
    p_stats = subparsers.add_parser("stats", help="Get codebase stats")
    p_stats.add_argument("--codebase", required=True, help="Codebase name")

    # reindex
    p_reindex = subparsers.add_parser("reindex", help="Force full re-index")
    p_reindex.add_argument("--codebase", required=True, help="Codebase name")
    p_reindex.add_argument("--mode", default=None, choices=["structural", "semantic", "both"],
                           help="Which mode(s) to re-index (default: all currently indexed modes)")
    p_reindex.add_argument("--provider", default=None, choices=["ollama", "lmstudio"], help="Override embedding provider")
    p_reindex.add_argument("--model", default=None, help="Override embedding model name")
    p_reindex.add_argument("--base-url", default=None, help="Override embedding service base URL")
    p_reindex.add_argument("--filter", default=None, choices=["everything", "smart"],
                           help="File filter mode (default: use stored setting or 'everything')")

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        sys.exit(1)

    commands = {
        "index": cmd_index,
        "search": cmd_search,
        "lookup": cmd_lookup,
        "list": cmd_list,
        "stats": cmd_stats,
        "reindex": cmd_reindex,
    }

    commands[args.command](args)


if __name__ == "__main__":
    main()
