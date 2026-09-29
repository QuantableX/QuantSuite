"""Client for the suite-owned, leased built-in embedding engine.

The Rust host supplies a fresh endpoint and key through the child environment.
Neither credentials nor the transient endpoint belong in index metadata.
"""
import json
import math
import os
from urllib.parse import urlparse

import httpx

MAX_INPUT_BYTES = 3000  # comfortably below the engine's 4096-token context
SETUP_HINT = "Download the built-in embedding engine in Memory settings first."


class EmbedProvider:
    def __init__(self):
        raw = os.environ.get("QUANTMCP_EMBEDDING_ENDPOINT")
        key = os.environ.get("QUANTMCP_EMBEDDING_KEY")
        if not raw or not key:
            raise ValueError(os.environ.get("QUANTMCP_EMBEDDING_ERROR") or SETUP_HINT)
        endpoint = json.loads(raw)
        url = urlparse(endpoint["url"])
        if url.scheme != "http" or url.hostname != "127.0.0.1" or not url.port or url.username or url.password:
            raise ValueError("The built-in engine must use its local loopback endpoint")
        self.base_url = endpoint["url"].rstrip("/")
        self.model = endpoint["model"]
        self.dimensions = int(endpoint["dims"])
        if not 0 < self.dimensions <= 16384:
            raise ValueError("Invalid built-in embedding dimensions")
        self.query_prefix = endpoint.get("queryPrefix", "")
        self._client = httpx.Client(timeout=120.0, trust_env=False,
                                    headers={"Authorization": f"Bearer {key}"})

    def embed(self, text: str, *, query: bool = False) -> list[float]:
        text = (self.query_prefix if query else "") + text
        text = text.encode("utf-8")[:MAX_INPUT_BYTES].decode("utf-8", errors="ignore")
        return self.embed_batch([text])[0]

    def _batch(self, texts: list[str]) -> list[list[float]]:
        response = self._client.post(f"{self.base_url}/v1/embeddings",
                                     json={"input": texts, "encoding_format": "float"})
        response.raise_for_status()
        data = sorted(response.json()["data"], key=lambda item: item["index"])
        if len(data) != len(texts):
            raise ValueError("Built-in engine returned an incomplete embedding batch")
        result = []
        for i, item in enumerate(data):
            vector = item["embedding"]
            if (item["index"] != i or len(vector) != self.dimensions
                    or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in vector)
                    or not any(v != 0 for v in vector)):
                raise ValueError("Built-in engine returned invalid vectors or dimensions")
            result.append(vector)
        return result

    def embed_batch(self, texts: list[str], batch_size: int = 8) -> list[list[float]]:
        results, batch, size = [], [], 0
        for text in texts:
            text_size = len(text.encode("utf-8"))
            if text_size > MAX_INPUT_BYTES:
                raise ValueError("Code chunk exceeds the embedding context budget")
            if batch and (len(batch) >= batch_size or size + text_size > MAX_INPUT_BYTES):
                results.extend(self._batch(batch))
                batch, size = [], 0
            batch.append(text)
            size += text_size
        if batch:
            results.extend(self._batch(batch))
        return results

    def get_dimensions(self) -> int:
        # Validate the endpoint before changing any stored index data.
        return len(self.embed("dimension probe"))

    def close(self) -> None:
        self._client.close()
