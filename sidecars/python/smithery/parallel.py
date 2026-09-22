"""Shared process pool for the gauntlet, walk-forward and rotation pairs.

The RNG streams stay serial (simulated charts are generated in the parent,
in the same draw order as the original serial code) so certifications remain
bit-identical run-to-run; only the expensive signal+backtest evaluations fan
out across cores. The pool is a lazy singleton reused across axes and across
indicators in a `--indicator all` run, amortizing Windows spawn cost.
"""
from __future__ import annotations

import atexit
import multiprocessing as mp
import os
import sys
import threading
from itertools import islice
from concurrent.futures import ProcessPoolExecutor

import pandas as pd

from . import backtest

WORKERS = max(1, min(30, int(os.environ.get("SMITHERY_WORKERS", (os.cpu_count() or 4) - 2))))
_POOL: ProcessPoolExecutor | None = None
_POOL_WORKERS = 0


def _worker_init() -> None:
    # stdout can be the app's JSON-RPC pipe. User indicator prints belong on stderr.
    sys.stdout = sys.stderr
    parent = mp.parent_process()
    if parent is not None:
        def watch_parent():
            parent.join()
            # The app may terminate an engine on cancel/timeout. Its workers
            # must not remain alive consuming CPU or holding the RPC pipe open.
            os._exit(0)
        threading.Thread(target=watch_parent, daemon=True).start()


def shutdown_pool() -> None:
    global _POOL, _POOL_WORKERS
    if _POOL is not None:
        _POOL.shutdown(wait=True, cancel_futures=True)
        _POOL = None
        _POOL_WORKERS = 0


atexit.register(shutdown_pool)


def pool_ready() -> bool:
    return _POOL is not None


def pool(workers: int = WORKERS) -> ProcessPoolExecutor:
    global _POOL, _POOL_WORKERS
    if _POOL is not None and _POOL_WORKERS < workers:
        shutdown_pool()
    if _POOL is None:
        # Each process owns one CPU task. Avoid a second layer of BLAS threads
        # on every core; explicit user settings still take precedence.
        for name in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS"):
            os.environ.setdefault(name, "1")
        _POOL = ProcessPoolExecutor(max_workers=workers, mp_context=mp.get_context("spawn"),
                                    initializer=_worker_init)
        _POOL_WORKERS = workers
    return _POOL


def pmap(fn, tasks: list, min_parallel: int = 4, *, max_workers: int | None = None) -> list:
    """Ordered parallel map; falls back to serial for tiny task lists."""
    tasks = list(tasks)
    workers = min(WORKERS, max_workers) if max_workers is not None else WORKERS
    if workers <= 1 or mp.current_process().name != "MainProcess" or len(tasks) < min_parallel:
        return [fn(t) for t in tasks]
    frame_rows = max((sum(len(value) for value in task if isinstance(value, pd.DataFrame))
                      for task in tasks), default=0)
    # Multi-million-bar minute histories cannot be copied to 30 workers.
    capacity = max(1, 500_000 // max(frame_rows, 1))
    if capacity == 1:
        return [fn(t) for t in tasks]
    workers = min(workers, capacity, len(tasks))
    executor = pool(workers)
    # Bound in-flight copies even when reusing a larger, already-warm pool.
    # Keep the same pool for a short final batch instead of respawning it.
    return [result for start in range(0, len(tasks), workers)
            for result in executor.map(fn, tasks[start:start + workers], chunksize=1)]


def simulated_results(ind, frames, cost: float, bars: int) -> list[tuple[float, float]]:
    """Generate and evaluate bounded batches; keep only scalar MC results.

    Generators consume the parent's RNG in the original order. Full minute
    histories keep the same calendar horizon without retaining hundreds
    of multi-million-row charts simultaneously.
    """
    iterator = iter(frames)
    capacity = max(1, min(WORKERS, 500_000 // max(bars, 1)))
    results = []
    while True:
        batch = list(islice(iterator, capacity))
        if not batch:
            return results
        results.extend(pmap(eval_total_sharpe, [(ind, frame, cost) for frame in batch]))
        del batch


# ---------------------------------------------------------------- workers
# Top-level so they pickle under the Windows spawn start method. Indicator
# instances pickle cheaply (they carry only their params dict).

def eval_backtest(task) -> dict:
    """(ind, df, cost_bps) -> full backtest dict."""
    ind, df, cost = task
    return backtest.run(df, ind.signal(df), cost_bps=cost)


def eval_execution(task) -> dict:
    from .execution import evaluate
    ind, df, cost, mode = task
    return evaluate(df, ind.signal(df), cost_bps=cost, mode=mode)


def eval_total_sharpe(task) -> tuple[float, float]:
    """(ind, df, cost_bps) -> (total_log_ret, sharpe). Lean MC worker."""
    ind, df, cost = task
    bt = backtest.run(df, ind.signal(df), cost_bps=cost)
    return bt["total_log_ret"], bt["sharpe"]


def eval_sharpe_signal(task):
    """(ind, df, cost_bps) -> (sharpe, signal ndarray, index). For axes that
    need both performance and signal agreement against a base stream."""
    ind, df, cost = task
    sig = ind.signal(df)
    return backtest.sharpe_only(df, sig, cost), sig.to_numpy(), sig.index


def eval_signal(task):
    """(ind, df) -> signal ndarray."""
    ind, df = task
    return ind.signal(df).to_numpy()


def rehydrate(values, index) -> pd.Series:
    return pd.Series(values, index=index)
