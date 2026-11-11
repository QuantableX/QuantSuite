"""Member votes for many pairs at once, ahead of the engine's pair-by-pair walk.

``prefill(pairs, cfg)`` cuts every pair's ratio frame into the history blocks ``bounded`` uses
(``signals.block_windows``), computes each batched indicator on all those windows in one call
per time grid (``smithery.batch.signals``: the GPU when CUDA is there, else compiled or
vectorised CPU), and stores the votes under the vote store's content key. ``member_signal``
looks a window up there first, so a run gives the same votes as before, by construction; a
window that is missing (an unbatched script, an error) is computed as before.

``SMITHERY_BATCH=0`` turns the prefill off.
"""
from __future__ import annotations

import os
from contextlib import contextmanager

import numpy as np
import pandas as pd

from . import signals, vote_store

_STATS: dict = {}


def enabled() -> bool:
    return os.environ.get("SMITHERY_BATCH", "1").strip() != "0"


@contextmanager
def active():
    """A batch memo for the duration of a run (nested runs share the outer one)."""
    if signals._BATCH_MEMO is not None or not enabled():
        yield signals._BATCH_MEMO
        return
    signals._BATCH_MEMO = {}
    signals._FRAME_MEMO, signals._WINDOW_MEMO, vote_store._KEYS = {}, {}, {}
    try:
        yield signals._BATCH_MEMO
    finally:
        signals._BATCH_MEMO = None
        signals._FRAME_MEMO = signals._WINDOW_MEMO = vote_store._KEYS = None


def stats() -> dict:
    """Counts since the process started: windows computed per device, windows retried."""
    return dict(_STATS)


def prefill(pairs, cfg, device: str = "auto") -> int:
    """Compute the votes of ``cfg``'s members for every (a, b) frame pair (b None = USD) ahead.
    Returns the number of windows added to the memo. No-op without an active memo."""
    memo = signals._BATCH_MEMO
    if memo is None or not pairs:
        return 0
    from smithery import batch as B
    from .smithery import REGISTRY, WARMUP_BARS
    kinds = signals.aggregate_members(cfg) if cfg.trend == "aggregate" else [cfg.trend]
    kinds = [k for k in kinds if k in REGISTRY and REGISTRY[k].has_batch()]
    if not kinds:
        return 0
    frames = []
    for a, b in pairs:
        if a is None:
            continue
        try:
            frames.append(signals.ratio_frame(a, b))
        except Exception:  # noqa: BLE001 — the engine reports it when it computes the pair
            continue
    added = 0
    for kind in kinds:
        overrides = signals.indicator_overrides(kind, cfg.params.get(kind))
        try:
            indicator = REGISTRY[kind](**overrides)
        except Exception:  # noqa: BLE001 — the engine raises the same error itself
            continue
        bars = WARMUP_BARS.get(kind, 0)
        keys, windows = [], []
        for frame in frames:
            for _, window in signals.block_windows(frame, bars):
                key = vote_store.window_key(kind, overrides, window)
                if key not in memo:
                    keys.append(key)
                    windows.append(window)
        if not windows:
            continue
        # One window per key (pairs can repeat a window, e.g. coin vs USD in several runs).
        unique = dict(zip(keys, windows))
        stats = {}
        results = B.signals(indicator, list(unique.values()), device=device, stats=stats)
        for (key, window), result in zip(unique.items(), results):
            if isinstance(result, np.ndarray):
                memo[key] = pd.Series(result, index=window.index, name=indicator.name)
                added += 1
        for name, value in stats.items():
            if isinstance(value, int):
                _STATS[name] = _STATS.get(name, 0) + value
    return added
