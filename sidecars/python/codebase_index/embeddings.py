import httpx


class EmbedProvider:
    """Abstraction over local embedding providers (Ollama / LM Studio).

    Both expose compatible REST APIs for generating embeddings.
    - Ollama:    POST http://localhost:11434/api/embeddings
    - LM Studio: POST http://localhost:1234/v1/embeddings (OpenAI-compatible)
    """

    def __init__(self, provider: str, base_url: str, model: str):
        """
        Args:
            provider: "ollama" or "lmstudio"
            base_url: Base URL of the embedding service
            model: Model name (e.g., "nomic-embed-text")
        """
        self.provider = provider
        self.base_url = base_url.rstrip("/")
        self.model = model
        self._client = httpx.Client(timeout=120.0)

    def embed(self, text: str) -> list[float]:
        """Generate embedding for a single text."""
        if self.provider == "ollama":
            resp = self._client.post(
                f"{self.base_url}/api/embeddings",
                json={"model": self.model, "prompt": text},
            )
            resp.raise_for_status()
            return resp.json()["embedding"]
        else:
            # LM Studio / OpenAI-compatible
            resp = self._client.post(
                f"{self.base_url}/v1/embeddings",
                json={"model": self.model, "input": text},
            )
            resp.raise_for_status()
            return resp.json()["data"][0]["embedding"]

    def embed_batch(self, texts: list[str], batch_size: int = 32) -> list[list[float]]:
        """Generate embeddings for multiple texts.

        For Ollama, we send one at a time (no native batch API).
        For LM Studio / OpenAI-compatible, we can send batches.
        """
        if self.provider == "ollama":
            return [self.embed(text) for text in texts]
        else:
            # OpenAI-compatible batch embedding
            results: list[list[float]] = []
            for i in range(0, len(texts), batch_size):
                batch = texts[i : i + batch_size]
                resp = self._client.post(
                    f"{self.base_url}/v1/embeddings",
                    json={"model": self.model, "input": batch},
                )
                resp.raise_for_status()
                data = resp.json()["data"]
                # Sort by index to preserve order
                data.sort(key=lambda x: x["index"])
                results.extend(d["embedding"] for d in data)
            return results

    def get_dimensions(self, sample_text: str = "hello") -> int:
        """Detect embedding dimensions by embedding a sample text."""
        vec = self.embed(sample_text)
        return len(vec)

    def close(self) -> None:
        """Close the HTTP client."""
        self._client.close()

    def __del__(self) -> None:
        try:
            self._client.close()
        except Exception:
            pass
