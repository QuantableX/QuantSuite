"""Semantic indexing using text chunking and vector embeddings.

Splits source files into overlapping chunks, embeds them via a local
AI provider (Ollama / LM Studio), and stores vectors for KNN search.
"""

import sqlite3

from db import clear_chunks_for_file, insert_chunk, get_chunks_by_ids
from embeddings import EmbedProvider
from vector_backend import VectorBackend


def chunk_file(
    content: str,
    chunk_size: int = 50,
    overlap: int = 10,
) -> list[dict]:
    """Split file content into overlapping chunks.

    Args:
        content: The full file content
        chunk_size: Number of lines per chunk
        overlap: Number of overlapping lines between chunks

    Returns:
        List of dicts with keys: content, start_line, end_line
    """
    lines = content.split("\n")
    total_lines = len(lines)

    if total_lines == 0:
        return []

    chunks: list[dict] = []
    step = max(chunk_size - overlap, 1)

    i = 0
    while i < total_lines:
        end = min(i + chunk_size, total_lines)
        chunk_content = "\n".join(lines[i:end])

        if chunk_content.strip():  # Skip empty chunks
            chunks.append({
                "content": chunk_content,
                "start_line": i + 1,  # 1-indexed
                "end_line": end,
            })

        if end >= total_lines:
            break
        i += step

    return chunks


def index_file_semantic(
    conn: sqlite3.Connection,
    file_path: str,
    file_id: int,
    content: str,
    language: str,
    embed_provider: EmbedProvider,
    vector_backend: VectorBackend,
    chunk_size: int = 50,
    overlap: int = 10,
) -> int:
    """Index a single file for semantic search.

    Chunks the file, generates embeddings, and stores both the chunks
    and their vectors.

    Returns the number of chunks indexed.
    """
    # Clear existing chunks and vectors for this file
    old_chunk_ids = clear_chunks_for_file(conn, file_id)
    if old_chunk_ids:
        vector_backend.delete_by_chunk_ids(old_chunk_ids)

    # Split into chunks
    chunks = chunk_file(content, chunk_size=chunk_size, overlap=overlap)
    if not chunks:
        return 0

    # Prepare texts for batch embedding
    texts = [c["content"] for c in chunks]

    # Generate embeddings
    embeddings = embed_provider.embed_batch(texts)

    # Store chunks and vectors
    for chunk_data, embedding in zip(chunks, embeddings):
        chunk_id = insert_chunk(
            conn,
            file_id=file_id,
            file_path=file_path,
            language=language,
            content=chunk_data["content"],
            start_line=chunk_data["start_line"],
            end_line=chunk_data["end_line"],
        )
        vector_backend.insert(chunk_id, embedding)

    conn.commit()
    return len(chunks)


def search_semantic(
    conn: sqlite3.Connection,
    query: str,
    embed_provider: EmbedProvider,
    vector_backend: VectorBackend,
    top_k: int = 10,
) -> list[dict]:
    """Search the semantic index using vector similarity.

    Embeds the query, performs KNN search, and returns matching chunks
    with their metadata.
    """
    # Embed the query
    query_embedding = embed_provider.embed(query)

    # KNN search
    chunk_ids = vector_backend.search(query_embedding, top_k=top_k)

    # Fetch chunk data
    chunks = get_chunks_by_ids(conn, chunk_ids)

    return chunks
