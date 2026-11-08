"""An optional store of indicator votes per history block, addressed by content.

``signals.bounded`` computes a Smithery indicator's votes block by block, each block from
its own window (the warm-up plus the block). A block's votes are a pure function of the
indicator, its parameter overrides and the window's candles, so a store keyed by exactly
those returns the same result the computation would: different candles or parameters
give another key and are computed as usual.

Off unless a run turns it on: ``ROTATION_LAB_VOTE_CACHE`` names a folder and
``ROTATION_LAB_VOTE_CACHE_MODE`` is ``write`` (compute what is missing and keep it) or
``read`` (use what is there, compute the rest without keeping it). The System Gauntlet
uses it for variants that see the base run's candles; regular backtests and live never do.
"""
from __future__ import annotations

import hashlib
import os
import pickle
import sqlite3
from pathlib import Path

import numpy as np
import pandas as pd

ENV_FOLDER = "ROTATION_LAB_VOTE_CACHE"
ENV_MODE = "ROTATION_LAB_VOTE_CACHE_MODE"
_OPEN: dict[tuple[str, str, int], "VoteStore"] = {}


def active() -> "VoteStore | None":
    """The store this process is told to use, or None."""
    folder = os.environ.get(ENV_FOLDER)
    if not folder:
        return None
    mode = os.environ.get(ENV_MODE, "read")
    key = (folder, mode, os.getpid())
    if key not in _OPEN:
        _OPEN[key] = VoteStore(Path(folder), mode)
    return _OPEN[key]


def close_all() -> None:
    """Close this process's stores (their folder can then be removed)."""
    for store in _OPEN.values():
        store.close()
    _OPEN.clear()


def window_key(kind: str, overrides: dict, window: pd.DataFrame) -> str:
    """The indicator, its overrides and the window's exact index and values, hashed."""
    digest = hashlib.sha256()
    digest.update(repr((kind, sorted(overrides.items()), list(map(str, window.columns)), str(window.index.dtype))).encode())
    digest.update(np.ascontiguousarray(window.index.asi8 if hasattr(window.index, "asi8") else window.index.to_numpy()).tobytes())
    for column in window.columns:
        values = window[column].to_numpy()
        digest.update(str(values.dtype).encode())
        # Object columns would hash their pointers, which differ between processes: they simply never hit.
        digest.update(np.ascontiguousarray(values).tobytes())
    return digest.hexdigest()


class VoteStore:
    def __init__(self, folder: Path, mode: str):
        if mode not in ("read", "write"):
            raise ValueError(f"vote store mode must be read or write, not {mode!r}")
        folder.mkdir(parents=True, exist_ok=True)
        self.mode = mode
        self.hits = self.misses = 0
        self._con = sqlite3.connect(folder / "votes.sqlite", timeout=60, isolation_level=None)
        self._con.execute("PRAGMA journal_mode=WAL")
        self._con.execute("CREATE TABLE IF NOT EXISTS votes (key TEXT PRIMARY KEY, value BLOB NOT NULL)")

    def wrap(self, kind: str, overrides: dict, compute):
        """``compute(window)``, answered from the store when the same window was computed before."""

        def cached(window: pd.DataFrame):
            key = window_key(kind, overrides, window)
            row = self._con.execute("SELECT value FROM votes WHERE key=?", (key,)).fetchone()
            if row is not None:
                self.hits += 1
                return pickle.loads(row[0])
            self.misses += 1
            out = compute(window)
            if self.mode == "write":
                self._con.execute("INSERT OR IGNORE INTO votes(key, value) VALUES (?, ?)",
                                  (key, pickle.dumps(out, protocol=5)))
            return out

        return cached

    def close(self) -> None:
        self._con.close()
