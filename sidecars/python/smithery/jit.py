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


@njit(parallel=False)
def pairwise_sum(a, start, n):
    """numpy's pairwise summation of a[start:start+n] (umath loops), for compiled loops that must
    reproduce ``x.sum()`` / ``x.mean()`` bit for bit."""
    if n < 8:
        res = 0.
        for i in range(n):
            res += a[start + i]
        return res
    if n <= 128:
        r0 = a[start]
        r1 = a[start + 1]
        r2 = a[start + 2]
        r3 = a[start + 3]
        r4 = a[start + 4]
        r5 = a[start + 5]
        r6 = a[start + 6]
        r7 = a[start + 7]
        i = 8
        stop = n - (n % 8)
        while i < stop:
            r0 += a[start + i]
            r1 += a[start + i + 1]
            r2 += a[start + i + 2]
            r3 += a[start + i + 3]
            r4 += a[start + i + 4]
            r5 += a[start + i + 5]
            r6 += a[start + i + 6]
            r7 += a[start + i + 7]
            i += 8
        res = ((r0 + r1) + (r2 + r3)) + ((r4 + r5) + (r6 + r7))
        while i < n:
            res += a[start + i]
            i += 1
        return res
    n2 = n // 2
    n2 -= n2 % 8
    return pairwise_sum(a, start, n2) + pairwise_sum(a, start + n2, n - n2)

