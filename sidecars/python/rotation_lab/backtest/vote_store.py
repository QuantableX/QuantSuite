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

A store that several runs share (``ROTATION_LAB_VOTE_SALT``, set by the System Gauntlet for a
shared store) outlives the run that wrote it, so its keys also name the code: the indicator's
script signature, its own default parameters and the fingerprint of the computing environment
the variable carries. A changed script, version or engine gives other keys and nothing stale
is ever read. The store is only ever a shortcut: a read or write that fails counts as a miss,
the votes are computed as usual and the run's result is the same.
"""
from __future__ import annotations

import hashlib
import os
import pickle
import sqlite3
import zlib
from pathlib import Path

import numpy as np
import pandas as pd

ENV_FOLDER = "ROTATION_LAB_VOTE_CACHE"
ENV_MODE = "ROTATION_LAB_VOTE_CACHE_MODE"
ENV_SALT = "ROTATION_LAB_VOTE_SALT"
_OPEN: dict[tuple[str, str, int], "VoteStore"] = {}
_SALTS: dict[tuple[str, str], str] = {}


def salt(kind: str) -> str:
    """What a shared store's key says about the code behind ``kind``'s votes ("" for a store of
    one run): the environment fingerprint, the script's signature (its own tree and its
    libraries') and the indicator's default parameters (a version's come from its file, not
    from the overrides). An indicator without a readable script is keyed to this process, so
    its votes are never shared."""
    shared = os.environ.get(ENV_SALT)
    if not shared:
        return ""
    hit = _SALTS.get((shared, kind))
    if hit is None:
        signature = defaults = None
        try:
            from smithery import variants
            from .smithery import REGISTRY
            cls = REGISTRY.get(kind)
            if cls is not None:
                signature = variants.source_signature(kind)
                defaults = repr(sorted((str(k), repr(v)) for k, v in cls.default_params().items()))
        except Exception:  # noqa: BLE001 — no signature: no sharing
            signature = None
        if signature is None or defaults is None:
            signature, defaults = f"unsigned:{os.getpid()}", ""
        hit = _SALTS[(shared, kind)] = hashlib.sha256(f"{shared}|{kind}|{signature}|{defaults}".encode()).hexdigest()
    return hit


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


# Inside a batch run (batch_votes.active): (id(window), kind, overrides) -> (window, key), so a
# window met again (the walk after the prefill) is not hashed twice. Entries keep the window alive.
_KEYS: dict | None = None


def window_key(kind: str, overrides: dict, window: pd.DataFrame) -> str:
    """The indicator, its overrides and the window's exact index and values, hashed."""
    memo = _KEYS
    if memo is None:
        return _window_key(kind, overrides, window)
    tag = (id(window), kind, repr(sorted(overrides.items())))
    hit = memo.get(tag)
    if hit is not None and hit[0] is window:
        return hit[1]
    key = _window_key(kind, overrides, window)
    memo[tag] = (window, key)
    return key


def _float_columns(window: pd.DataFrame) -> bool:
    try:   # the block manager's dtypes: DataFrame.dtypes builds a Series each time (slow per window)
        dtypes = window._mgr.get_dtypes()
    except AttributeError:
        dtypes = window.dtypes
    return len(dtypes) > 0 and all(dtype == np.float64 for dtype in dtypes)


def _window_key(kind: str, overrides: dict, window: pd.DataFrame) -> str:
    digest = hashlib.sha256()
    digest.update(repr((kind, sorted(overrides.items()), list(map(str, window.columns)), str(window.index.dtype))).encode())
    code = salt(kind)
    if code:
        digest.update(code.encode())
    digest.update(np.ascontiguousarray(window.index.asi8 if hasattr(window.index, "asi8") else window.index.to_numpy()).tobytes())
    if _float_columns(window):
        # All float: the same bytes column by column, from one array (no pandas lookup per column).
        block = window.to_numpy()
        for j in range(block.shape[1]):
            digest.update(b"float64")
            digest.update(np.ascontiguousarray(block[:, j]).tobytes())
        return digest.hexdigest()
    for column in window.columns:
        values = window[column].to_numpy()
        digest.update(str(values.dtype).encode())
        if values.dtype.kind in "OT":
            # Text (a coin's own candles carry their exchange's name): the values themselves. Their
            # bytes would be pointers, which differ between processes, so such a window never hit
            # outside the process that wrote it.
            digest.update("\x1f".join(map(repr, values.tolist())).encode("utf-8", "backslashreplace"))
        else:
            digest.update(np.ascontiguousarray(values).tobytes())
    return digest.hexdigest()


def _pack(votes) -> bytes:
    """The votes as they are, pickled and compressed (a block's index and its three vote values
    shrink to a fraction: a store shared by a round's Gauntlets holds millions of blocks)."""
    return zlib.compress(pickle.dumps(votes, protocol=5), 1)


def _unpack(data: bytes):
    # A pickle starts with 0x80, a zlib stream with 0x78: rows written before the compression still read.
    return pickle.loads(zlib.decompress(data) if data[:1] == b"\x78" else data)


class VoteStore:
    def __init__(self, folder: Path, mode: str):
        if mode not in ("read", "write"):
            raise ValueError(f"vote store mode must be read or write, not {mode!r}")
        folder.mkdir(parents=True, exist_ok=True)
        self.mode = mode
        self.hits = self.misses = self.errors = 0
        self._con = sqlite3.connect(folder / "votes.sqlite", timeout=120, isolation_level=None)
        self._con.execute("PRAGMA journal_mode=WAL")
        # WAL with NORMAL never corrupts the file; a power cut can only lose the newest rows of a cache.
        self._con.execute("PRAGMA synchronous=NORMAL")
        self._con.execute("CREATE TABLE IF NOT EXISTS votes (key TEXT PRIMARY KEY, value BLOB NOT NULL)")

    def _read(self, key: str):
        """The stored votes or None. A store that cannot answer (locked by other runs for longer
        than the timeout, a damaged row) is a miss: the votes are then computed, the same ones."""
        try:
            row = self._con.execute("SELECT value FROM votes WHERE key=?", (key,)).fetchone()
            return None if row is None else _unpack(row[0])
        except (sqlite3.Error, pickle.UnpicklingError, zlib.error, EOFError, AttributeError, ImportError, IndexError):
            self.errors += 1
            return None

    def get(self, key: str):
        """The stored votes of one window key, or None (counted like a lookup through ``wrap``)."""
        out = self._read(key)
        if out is not None:
            self.hits += 1
        return out

    def has(self, key: str) -> bool:
        """Whether the store holds this window key (the votes stay where they are)."""
        try:
            return self._con.execute("SELECT 1 FROM votes WHERE key=?", (key,)).fetchone() is not None
        except sqlite3.Error:
            self.errors += 1
            return False

    def wrap(self, kind: str, overrides: dict, compute):
        """``compute(window)``, answered from the store when the same window was computed before."""

        def cached(window: pd.DataFrame):
            key = window_key(kind, overrides, window)
            stored = self._read(key)
            if stored is not None:
                self.hits += 1
                return stored
            self.misses += 1
            out = compute(window)
            if self.mode == "write":
                self._write([(key, out)])
            return out

        return cached

    def _write(self, items) -> int:
        rows = [(key, _pack(value)) for key, value in items]
        if not rows:
            return 0
        try:
            # One transaction: several runs may write the same store side by side.
            self._con.execute("BEGIN IMMEDIATE")
            self._con.executemany("INSERT OR IGNORE INTO votes(key, value) VALUES (?, ?)", rows)
            self._con.execute("COMMIT")
        except sqlite3.Error:
            self.errors += 1
            try:
                self._con.execute("ROLLBACK")
            except sqlite3.Error:
                pass
            return 0
        return len(rows)

    def put_many(self, items) -> int:
        """Keep ``(key, votes)`` pairs computed elsewhere (a votes-only run's batch memo)."""
        if self.mode != "write":
            return 0
        return self._write(items)

    def close(self) -> None:
        self._con.close()
