"""Compiled per-bar loops for batched indicators (Numba), with a plain-Python fallback.

An indicator whose logic steps bar by bar (a state machine, an online learner, a nonlinear
recurrence) keeps that loop as written, over one row at a time, and compiles it::

    from ..jit import njit, prange

    @njit
    def kernel(c, out):              # c, out: [rows, T] float64 numpy arrays
        for s in prange(c.shape[0]):
            ...                      # the scalar loop over bars, unchanged

Numba compiles it without fast-math, so the arithmetic is the IEEE arithmetic of the Python
loop (bit for bit), and ``prange`` spreads the rows over the CPU cores. Without Numba the same
function runs as plain Python. ``NUMBA_NUM_THREADS`` limits the threads (research workers set 1).
"""
from __future__ import annotations

import os

try:
    import numba as _numba
except Exception:  # noqa: BLE001 — Numba is optional; the loops still run, slower
    _numba = None

AVAILABLE = _numba is not None


def njit(fn=None, **options):
    """``numba.njit(parallel=True, cache=True)`` when Numba is there, else the function itself."""
    def wrap(f):
        if _numba is None or os.environ.get("SMITHERY_JIT", "1") == "0":
            return f
        opts = {"parallel": True, "cache": True, **options}
        return _numba.njit(**opts)(f)
    return wrap(fn) if fn is not None else wrap


if _numba is not None:
    prange = _numba.prange
else:  # pragma: no cover - exercised only without Numba
    prange = range


def note() -> str:
    """For result files: whether per-bar loops ran compiled."""
    if _numba is None:
        return "loops: Python (Numba not installed)"
    return f"loops: Numba {_numba.__version__} ({_numba.get_num_threads()} threads)"
