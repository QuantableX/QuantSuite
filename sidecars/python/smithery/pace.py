"""Pacing for GPU work, so the user can game or work while research runs.

The research throttle keeps the user's chosen level ("Leistung auf 10%") in a small JSON file,
{"percent": 10}; SMITHERY_THROTTLE_FILE names it. At a level below 100 the GPU code pauses after
each batch for long enough that the GPU is busy only that share of the time, and its batches get
smaller so no single burst runs long. Without the variable, or at 100, nothing changes: no pause,
no extra synchronisation, the same batch sizes. Pacing never changes a result, only when it arrives.
"""
from __future__ import annotations

import json
import os
import time

REREAD_SECONDS = 1.0
_cache = {"path": None, "read": float("-inf"), "level": 100}


def level() -> int:
    """The current level in percent (1-100); 100 when no throttle file is set or it cannot be read."""
    path = os.environ.get("SMITHERY_THROTTLE_FILE")
    if not path:
        return 100
    now = time.monotonic()
    if path != _cache["path"] or now - _cache["read"] >= REREAD_SECONDS:
        try:
            with open(path, encoding="utf-8") as stream:
                value = int(json.load(stream)["percent"])
        except (OSError, ValueError, KeyError, TypeError):
            value = 100
        _cache.update(path=path, read=now, level=min(100, max(1, value)))
    return _cache["level"]


def batch(step: int, minimum: int = 1) -> int:
    """A batch size scaled down with the level, so a throttled burst stays short."""
    current = level()
    return step if current >= 100 else max(minimum, step * current // 100)


class Pacer:
    """Wrap each GPU batch in start() and done(): done() waits until the batch has finished
    (``sync``) and then pauses so the GPU works about ``level`` percent of the time."""

    def __init__(self, sync=None, sleep=time.sleep, clock=time.perf_counter):
        self.sync, self.sleep, self.clock = sync, sleep, clock
        self.started = None
        self.paused = 0.0

    def start(self):
        self.started = self.clock() if level() < 100 else None

    def done(self):
        if self.started is None:
            return
        if self.sync is not None:
            self.sync()
        busy = self.clock() - self.started
        self.started = None
        current = level()
        if current < 100 and busy > 0:
            wait = busy * (100 - current) / current
            self.paused += wait
            self.sleep(wait)
