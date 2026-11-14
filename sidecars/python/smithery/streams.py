"""GPU jobs side by side: each in a worker thread on its own CUDA stream.

A Gauntlet hands an indicator small jobs: one parameter variant on one chart, or the few coins
that share a time grid. One such job keeps a per-series kernel on a handful of GPU threads while
the rest of the GPU waits, so the jobs run side by side instead: K worker threads, each with a
CUDA stream of its own (torch's current stream per thread; smithery.jit and the scripts' own
kernels launch on it). Every job computes exactly what it computed alone, so results are the
same bit for bit; only the order in which the GPU does the work changes.

- ``SMITHERY_GPU_STREAMS`` sets K (default 8; 1 = one job after another, as before).
- The jobs in flight together hold at most ``SMITHERY_GPU_CELLS`` series x bars (the working set
  one call had before); a larger job runs alone.
- A job that starts more jobs (a variant's call with several time grids) runs them itself, one
  after another: no threads inside threads.
- Without CUDA (or K = 1) it is the plain loop: the CPU path never changes.
"""
from __future__ import annotations

import os
import threading
from concurrent.futures import ThreadPoolExecutor

DEFAULT_STREAMS = 8

_LOCAL = threading.local()
_POOL: dict = {"executor": None, "size": 0}
_POOL_LOCK = threading.Lock()


def width() -> int:
    """K: how many GPU jobs may run side by side (SMITHERY_GPU_STREAMS, default 8)."""
    try:
        value = int(os.environ.get("SMITHERY_GPU_STREAMS", DEFAULT_STREAMS))
    except ValueError:
        value = DEFAULT_STREAMS
    return max(1, value)


def inside() -> bool:
    """Whether this thread is one of the stream workers (its jobs already run side by side)."""
    return getattr(_LOCAL, "stream", None) is not None


def _executor(size: int) -> ThreadPoolExecutor:
    with _POOL_LOCK:
        if _POOL["executor"] is None or _POOL["size"] != size:
            if _POOL["executor"] is not None:
                _POOL["executor"].shutdown(wait=True)
            _POOL["executor"] = ThreadPoolExecutor(max_workers=size, thread_name_prefix="smithery-gpu")
            _POOL["size"] = size
        return _POOL["executor"]


class _Budget:
    """At most ``limit`` cells in flight; a job larger than the limit waits until it runs alone."""

    def __init__(self, limit: int):
        self.limit = max(1, int(limit))
        self.used = 0
        self.cond = threading.Condition()

    def acquire(self, cells: int) -> int:
        cells = max(0, int(cells))
        with self.cond:
            while self.used and self.used + cells > self.limit:
                self.cond.wait()
            self.used += cells
        return cells

    def release(self, cells: int) -> None:
        with self.cond:
            self.used -= cells
            self.cond.notify_all()


def _cell_limit() -> int:
    from .batch import DEFAULT_GPU_CELLS
    try:
        return int(os.environ.get("SMITHERY_GPU_CELLS", DEFAULT_GPU_CELLS))
    except ValueError:
        return DEFAULT_GPU_CELLS


def gpu_map(fn, items, cells=None) -> list:
    """``[fn(item) for item in items]``, the items side by side on CUDA streams when the GPU is
    there and there is more than one. ``cells(item)`` (optional) is the job's series x bars, for
    the working-set limit. Results come back in order; if jobs raise, the first one's exception
    (in item order) is raised, as the loop would."""
    items = list(items)
    size = min(width(), len(items))
    if size <= 1 or inside() or not _cuda():
        return [fn(item) for item in items]
    import torch
    budget = _Budget(_cell_limit())
    caller = torch.cuda.current_stream()

    def run(item):
        stream = getattr(_LOCAL, "stream", None)
        if stream is None:
            stream = _LOCAL.stream = torch.cuda.Stream()
        held = budget.acquire(cells(item) if cells is not None else 0)
        try:
            # GPU data the caller made on its own stream is ready before this stream reads it.
            stream.wait_stream(caller)
            with torch.cuda.stream(stream):
                try:
                    return True, fn(item)
                except Exception as exc:  # noqa: BLE001 — raised in order below
                    return False, exc
                finally:
                    stream.synchronize()
        finally:
            budget.release(held)

    results = list(_executor(width()).map(run, items))
    from .batch import release_gpu_cache
    release_gpu_cache()
    for ok, value in results:
        if not ok:
            raise value
    return [value for _, value in results]


def _cuda() -> bool:
    try:
        from .batch import gpu_available
        return bool(gpu_available())
    except Exception:  # noqa: BLE001
        return False
