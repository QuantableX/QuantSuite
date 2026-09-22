import sqlite3
import struct
from abc import ABC, abstractmethod


class VectorBackend(ABC):
    """Abstract interface for vector storage and similarity search.

    Designed so that swapping from SQLite-vec to Qdrant later requires
    zero changes to MCP tools or indexing logic.
    """

    @abstractmethod
    def insert(self, chunk_id: int, embedding: list[float]) -> None:
        """Insert a vector embedding associated with a chunk_id."""
        ...

    @abstractmethod
    def search(self, query_embedding: list[float], top_k: int = 10) -> list[int]:
        """Search for the most similar vectors. Returns chunk_ids ordered by similarity."""
        ...

    @abstractmethod
    def delete_by_chunk_ids(self, chunk_ids: list[int]) -> None:
        """Delete vectors by their associated chunk_ids."""
        ...

    @abstractmethod
    def clear(self) -> None:
        """Clear all vectors."""
        ...


def _serialize_float32(vec: list[float]) -> bytes:
    """Serialize a list of floats to a compact binary format for sqlite-vec."""
    return struct.pack(f"{len(vec)}f", *vec)


class SqliteVecBackend(VectorBackend):
    """Vector backend using sqlite-vec extension.

    Uses the vec0 virtual table for KNN similarity search.
    """

    def __init__(self, conn: sqlite3.Connection):
        self.conn = conn

    def insert(self, chunk_id: int, embedding: list[float]) -> None:
        """Insert a vector into vec_index."""
        blob = _serialize_float32(embedding)
        self.conn.execute(
            "INSERT INTO vec_index (embedding, chunk_id) VALUES (?, ?)",
            (blob, chunk_id),
        )

    def search(self, query_embedding: list[float], top_k: int = 10) -> list[int]:
        """KNN search using sqlite-vec. Returns chunk_ids ordered by distance (closest first)."""
        blob = _serialize_float32(query_embedding)
        rows = self.conn.execute(
            """SELECT chunk_id, distance
               FROM vec_index
               WHERE embedding MATCH ?
               ORDER BY distance
               LIMIT ?""",
            (blob, top_k),
        ).fetchall()
        return [row[0] for row in rows]

    def delete_by_chunk_ids(self, chunk_ids: list[int]) -> None:
        """Delete vectors by chunk_ids."""
        if not chunk_ids:
            return
        placeholders = ",".join("?" for _ in chunk_ids)
        self.conn.execute(
            f"DELETE FROM vec_index WHERE chunk_id IN ({placeholders})",
            chunk_ids,
        )

    def clear(self) -> None:
        """Clear all vectors from vec_index."""
        try:
            self.conn.execute("DELETE FROM vec_index")
            self.conn.commit()
        except Exception:
            pass


class QdrantBackend(VectorBackend):
    """Placeholder for future Qdrant vector backend.

    To implement:
    1. pip install qdrant-client
    2. Implement using qdrant_client.QdrantClient
    3. One Qdrant collection per codebase
    4. Set vector_backend="qdrant" in meta table
    """

    def __init__(self, url: str = "http://localhost:6333", collection: str = "codebase"):
        raise NotImplementedError(
            "QdrantBackend is not yet implemented. "
            "Use vector_backend='sqlite' for now. "
            "To add Qdrant support, implement this class using qdrant-client."
        )

    def insert(self, chunk_id: int, embedding: list[float]) -> None:
        raise NotImplementedError

    def search(self, query_embedding: list[float], top_k: int = 10) -> list[int]:
        raise NotImplementedError

    def delete_by_chunk_ids(self, chunk_ids: list[int]) -> None:
        raise NotImplementedError

    def clear(self) -> None:
        raise NotImplementedError
