import sqlite3
import time
from pathlib import Path


def _load_vec_extension(conn: sqlite3.Connection) -> bool:
    """Try to load the sqlite-vec extension. Returns True if successful."""
    try:
        import sqlite_vec

        conn.enable_load_extension(True)
        sqlite_vec.load(conn)
        conn.enable_load_extension(False)
        return True
    except Exception:
        return False


def init_db(db_path: str | Path, dimensions: int = 768) -> sqlite3.Connection:
    """Initialize a codebase database with all required tables.

    Creates meta, files, code_fts, chunks tables.
    Creates vec_index only if sqlite-vec extension is available.
    """
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")

    # Meta table for codebase config
    conn.execute("""
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT
        )
    """)

    # File tracking (both modes)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS files (
            id            INTEGER PRIMARY KEY,
            file_path     TEXT UNIQUE,
            file_hash     TEXT,
            last_modified INTEGER
        )
    """)

    # Structural mode: FTS5 full-text search
    conn.execute("""
        CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(
            file_path   UNINDEXED,
            symbol_name,
            symbol_type,
            language    UNINDEXED,
            content,
            tokenize = "unicode61"
        )
    """)

    # Semantic mode: raw text chunks with metadata
    conn.execute("""
        CREATE TABLE IF NOT EXISTS chunks (
            id         INTEGER PRIMARY KEY,
            file_id    INTEGER REFERENCES files(id),
            file_path  TEXT,
            language   TEXT,
            content    TEXT,
            start_line INTEGER,
            end_line   INTEGER
        )
    """)

    # Try to create vec_index if sqlite-vec is available
    has_vec = _load_vec_extension(conn)
    if has_vec:
        try:
            conn.execute(f"""
                CREATE VIRTUAL TABLE IF NOT EXISTS vec_index USING vec0(
                    embedding float[{dimensions}],
                    chunk_id  INTEGER
                )
            """)
        except Exception:
            pass  # vec_index creation failed, semantic mode won't work

    conn.commit()
    return conn


def open_db(db_path: str | Path) -> sqlite3.Connection:
    """Open an existing codebase database."""
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    _load_vec_extension(conn)
    return conn


# -- Meta table helpers --

def get_meta(conn: sqlite3.Connection, key: str) -> str | None:
    """Get a value from the meta table."""
    row = conn.execute("SELECT value FROM meta WHERE key = ?", (key,)).fetchone()
    return row["value"] if row else None


def set_meta(conn: sqlite3.Connection, key: str, value: str) -> None:
    """Set a value in the meta table (upsert)."""
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = ?",
        (key, value, value),
    )
    conn.commit()


def get_all_meta(conn: sqlite3.Connection) -> dict[str, str]:
    """Get all key-value pairs from the meta table."""
    rows = conn.execute("SELECT key, value FROM meta").fetchall()
    return {row["key"]: row["value"] for row in rows}


# -- File tracking helpers --

def get_file(conn: sqlite3.Connection, file_path: str) -> sqlite3.Row | None:
    """Get a file record by path."""
    return conn.execute(
        "SELECT * FROM files WHERE file_path = ?", (file_path,)
    ).fetchone()


def upsert_file(conn: sqlite3.Connection, file_path: str, file_hash: str) -> int:
    """Insert or update a file record. Returns the file id."""
    now = int(time.time())
    conn.execute(
        """INSERT INTO files (file_path, file_hash, last_modified)
           VALUES (?, ?, ?)
           ON CONFLICT(file_path) DO UPDATE SET
             file_hash = excluded.file_hash,
             last_modified = excluded.last_modified""",
        (file_path, file_hash, now),
    )
    conn.commit()
    row = conn.execute(
        "SELECT id FROM files WHERE file_path = ?", (file_path,)
    ).fetchone()
    return row["id"]


def delete_file(conn: sqlite3.Connection, file_path: str) -> int | None:
    """Delete a file record and return its id, or None if not found."""
    row = get_file(conn, file_path)
    if row is None:
        return None
    file_id = row["id"]
    conn.execute("DELETE FROM files WHERE id = ?", (file_id,))
    conn.commit()
    return file_id


def list_files(conn: sqlite3.Connection) -> list[sqlite3.Row]:
    """List all tracked files."""
    return conn.execute("SELECT * FROM files ORDER BY file_path").fetchall()


# -- FTS helpers --

def clear_fts(conn: sqlite3.Connection) -> None:
    """Clear all FTS entries."""
    conn.execute("DELETE FROM code_fts")
    conn.commit()


def clear_fts_for_file(conn: sqlite3.Connection, file_path: str) -> None:
    """Clear FTS entries for a specific file."""
    conn.execute("DELETE FROM code_fts WHERE file_path = ?", (file_path,))
    conn.commit()


def insert_fts(
    conn: sqlite3.Connection,
    file_path: str,
    symbol_name: str,
    symbol_type: str,
    language: str,
    content: str,
) -> None:
    """Insert a symbol into the FTS index."""
    conn.execute(
        "INSERT INTO code_fts (file_path, symbol_name, symbol_type, language, content) VALUES (?, ?, ?, ?, ?)",
        (file_path, symbol_name, symbol_type, language, content),
    )


def search_fts(conn: sqlite3.Connection, query: str, limit: int = 20) -> list[dict]:
    """Search the FTS index using BM25 ranking."""
    rows = conn.execute(
        """SELECT file_path, symbol_name, symbol_type, language, content, rank
           FROM code_fts
           WHERE code_fts MATCH ?
           ORDER BY rank
           LIMIT ?""",
        (query, limit),
    ).fetchall()
    return [dict(r) for r in rows]


def lookup_symbol(conn: sqlite3.Connection, symbol_name: str) -> list[dict]:
    """Look up a symbol by exact name in the FTS index."""
    rows = conn.execute(
        """SELECT file_path, symbol_name, symbol_type, language, content
           FROM code_fts
           WHERE symbol_name = ?""",
        (symbol_name,),
    ).fetchall()
    return [dict(r) for r in rows]


# -- Chunk helpers --

def clear_chunks(conn: sqlite3.Connection) -> None:
    """Clear all chunks."""
    conn.execute("DELETE FROM chunks")
    conn.commit()


def clear_chunks_for_file(conn: sqlite3.Connection, file_id: int) -> list[int]:
    """Clear chunks for a specific file. Returns deleted chunk ids."""
    rows = conn.execute(
        "SELECT id FROM chunks WHERE file_id = ?", (file_id,)
    ).fetchall()
    chunk_ids = [r["id"] for r in rows]
    if chunk_ids:
        conn.execute("DELETE FROM chunks WHERE file_id = ?", (file_id,))
    return chunk_ids


def insert_chunk(
    conn: sqlite3.Connection,
    file_id: int,
    file_path: str,
    language: str,
    content: str,
    start_line: int,
    end_line: int,
) -> int:
    """Insert a chunk and return its id."""
    cursor = conn.execute(
        """INSERT INTO chunks (file_id, file_path, language, content, start_line, end_line)
           VALUES (?, ?, ?, ?, ?, ?)""",
        (file_id, file_path, language, content, start_line, end_line),
    )
    return cursor.lastrowid


def get_chunks_by_ids(conn: sqlite3.Connection, chunk_ids: list[int]) -> list[dict]:
    """Get chunks by their ids, preserving order."""
    if not chunk_ids:
        return []
    placeholders = ",".join("?" for _ in chunk_ids)
    rows = conn.execute(
        f"SELECT * FROM chunks WHERE id IN ({placeholders})",
        chunk_ids,
    ).fetchall()
    row_map = {r["id"]: dict(r) for r in rows}
    return [row_map[cid] for cid in chunk_ids if cid in row_map]


# -- Vec index helpers --

def clear_vec_index(conn: sqlite3.Connection) -> None:
    """Drop the vec_index table so it can be recreated with correct dimensions."""
    try:
        conn.execute("DROP TABLE IF EXISTS vec_index")
        conn.commit()
    except Exception:
        pass  # vec_index may not exist


def get_db_stats(conn: sqlite3.Connection) -> dict:
    """Get statistics about the database."""
    file_count = conn.execute("SELECT COUNT(*) as cnt FROM files").fetchone()["cnt"]
    fts_count = conn.execute("SELECT COUNT(*) as cnt FROM code_fts").fetchone()["cnt"]
    chunk_count = conn.execute("SELECT COUNT(*) as cnt FROM chunks").fetchone()["cnt"]

    meta = get_all_meta(conn)

    return {
        "file_count": file_count,
        "fts_entry_count": fts_count,
        "chunk_count": chunk_count,
        "mode": meta.get("mode", "unknown"),
        "vector_backend": meta.get("vector_backend", "sqlite"),
        "embed_model": meta.get("embed_model", ""),
        "embed_provider": meta.get("embed_provider", ""),
        "last_indexed": meta.get("last_indexed", ""),
        "codebase_path": meta.get("codebase_path", ""),
        "structural_indexed_at": meta.get("structural_indexed_at", ""),
        "semantic_indexed_at": meta.get("semantic_indexed_at", ""),
    }
