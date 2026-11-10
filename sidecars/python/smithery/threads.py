"""One OpenBLAS thread for worker processes.

numpy's OpenBLAS reserves a buffer per hardware thread when it is first
imported: about 0.75 GB of committed memory per Python process on a
32-thread PC (791 MB against 52 MB after ``import numpy, pandas``,
measured 2026-11-10). The research and engine workers are many processes
that each run one indicator at a time, so those threads are never used,
while a few dozen workers commit tens of GB.

The variable is read at that first import. In a spawned worker that happens
before any pool initializer runs, so it has to be in the environment the
worker is started with: the process that starts workers sets it for its
children. Its own numpy is already loaded and keeps its threads. A value
the user set is left alone.
"""
from __future__ import annotations

import os

NAME = "OPENBLAS_NUM_THREADS"


def one_for_children() -> None:
    """Every process started from here on gets one OpenBLAS thread (unless the variable is already set)."""
    os.environ.setdefault(NAME, "1")


def with_one(env: dict) -> dict:
    """``env`` for a child process, with one OpenBLAS thread unless it says otherwise."""
    env.setdefault(NAME, "1")
    return env
