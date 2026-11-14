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

import numpy as np

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


# --------------------------------------------------------------------------- per-series loops on the GPU
#
# A loop that steps bar by bar runs on the GPU as one thread per series (all series in one launch):
#
#     from ..jit import per_series
#
#     def _row(s, price, widths, out):      # plain Python: series s of [series x bars] arrays
#         for t in range(price.shape[1]):
#             ...                          # the scalar loop over bars, as written for the CPU
#
#     _gpu_rows = per_series(_row)          # call it with torch tensors on the GPU (zero copy)
#
# numba-cuda compiles the function on first use (it needs ``pip install numba-cuda[cu13]`` with
# CUDA wheels of torch's CUDA version, see requirements-gpu-kernels.txt). Plain IEEE arithmetic;
# libdevice's exp/log/tanh and fused multiply-adds can move the last bit, so parity reports every
# bar that differs, as for every GPU path. Without numba-cuda ``available()`` is False and callers
# keep their CPU loop (and say so).

_CUDA: dict = {}


def cuda_module():
    """numba-cuda's ``cuda`` when GPU kernels can run (installed, a CUDA GPU, SMITHERY_GPU not 0)."""
    if "mod" not in _CUDA:
        mod, reason = None, ""
        if os.environ.get("SMITHERY_GPU", "1").strip() == "0":
            reason = "SMITHERY_GPU=0"
        else:
            try:
                # torch first: its CUDA libraries (nvJitLink) are the ones the process keeps.
                import torch  # noqa: F401
                from numba import cuda as mod
                if not mod.is_available():
                    mod, reason = None, "numba-cuda finds no CUDA GPU"
            except Exception as exc:  # noqa: BLE001 — optional: the CPU loops still run
                mod, reason = None, f"numba-cuda not installed ({type(exc).__name__})"
        _CUDA["mod"], _CUDA["reason"] = mod, reason
    return _CUDA["mod"]


def gpu_kernels() -> bool:
    """Whether per-series GPU kernels can run in this process."""
    return cuda_module() is not None


def gpu_kernels_reason() -> str:
    cuda_module()
    return _CUDA.get("reason", "")


class _PerSeries:
    def __init__(self, row):
        self.row = row
        self.kernels: dict = {}

    def _kernel(self, count):
        if count not in self.kernels:
            cuda = cuda_module()
            names = ", ".join(f"a{i}" for i in range(count))
            source = "\n".join([f"def kernel({names}):",
                                "    s = cuda.grid(1)",
                                "    if s < a0.shape[0]:",
                                f"        row(s, {names})", ""])
            scope = {"cuda": cuda, "row": cuda.jit(device=True)(self.row)}
            exec(source, scope)
            self.kernels[count] = cuda.jit(scope["kernel"])
        return self.kernels[count]

    def __call__(self, *args):
        """Run ``row(s, *args)`` for every series s (rows of the first argument, a torch tensor on
        the GPU); tensors pass without a copy, other arguments as scalars; torch's current stream."""
        cuda = cuda_module()
        if cuda is None:
            raise RuntimeError(f"GPU kernels unavailable: {gpu_kernels_reason()}")
        import torch
        rows = int(args[0].shape[0])
        if rows == 0:
            return
        values = [cuda.as_cuda_array(a.detach()) if torch.is_tensor(a) else a for a in args]
        stream = cuda.external_stream(torch.cuda.current_stream().cuda_stream)
        threads = 32
        self._kernel(len(args))[(rows + threads - 1) // threads, threads, stream](*values)


def per_series(row):
    """A GPU kernel from a per-series loop ``row(s, *arrays_and_scalars)``: one thread per series."""
    return _PerSeries(row)


def note() -> str:
    """For result files: whether per-bar loops ran compiled."""
    if _numba is None:
        return "loops: Python (Numba not installed)"
    return f"loops: Numba {_numba.__version__} ({_numba.get_num_threads()} threads)"


@njit(parallel=False)
def _leaf_sum(a, start, n):
    """numpy's pairwise summation for n <= 128: plain loop below 8, eight accumulators above."""
    if n < 8:
        res = 0.
        for i in range(n):
            res += a[start + i]
        return res
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


@njit(parallel=False)
def pairwise_sum(a, start, n):
    """numpy's pairwise summation of a[start:start+n] (umath loops), for compiled loops that must
    reproduce ``x.sum()`` / ``x.mean()`` bit for bit. Above 128 numpy splits at n//2 rounded down
    to a multiple of 8 and adds left + right; walked here with an explicit stack (Numba's on-disk
    cache cannot link a recursive function called from other cached functions)."""
    if n <= 128:
        return _leaf_sum(a, start, n)
    st_start = np.empty(64, dtype=np.int64)
    st_n = np.empty(64, dtype=np.int64)
    st_state = np.zeros(64, dtype=np.int64)
    st_left = np.zeros(64)
    sp = 0
    st_start[0] = start
    st_n[0] = n
    ret = 0.
    while True:
        s0 = st_start[sp]
        n0 = st_n[sp]
        if n0 <= 128:
            ret = _leaf_sum(a, s0, n0)
            sp -= 1
            if sp < 0:
                return ret
            continue
        n2 = n0 // 2
        n2 -= n2 % 8
        state = st_state[sp]
        if state == 0:                      # evaluate the left half
            st_state[sp] = 1
            sp += 1
            st_start[sp] = s0
            st_n[sp] = n2
            st_state[sp] = 0
        elif state == 1:                    # left done: keep it, evaluate the right half
            st_left[sp] = ret
            st_state[sp] = 2
            sp += 1
            st_start[sp] = s0 + n2
            st_n[sp] = n0 - n2
            st_state[sp] = 0
        else:                               # right done: left + right
            ret = st_left[sp] + ret
            st_state[sp] = 0
            sp -= 1
            if sp < 0:
                return ret
