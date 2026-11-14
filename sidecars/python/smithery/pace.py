"""Pacing for GPU work, so the user can game or work while research runs.

The research throttle keeps the user's chosen level ("Leistung auf 10%") in a small JSON file,
{"percent": 10}; SMITHERY_THROTTLE_FILE names it. At a level below 100 the GPU code pauses after
each busy stretch for long enough that the GPU is busy only that share of the time, and its batches
get smaller so no single burst runs long. Without the variable, or at 100, nothing changes: no
pause, no extra synchronisation, the same batch sizes. Pacing never changes a result, only when it
arrives.

GPU jobs may run side by side on CUDA streams (smithery.streams). A busy stretch is then the time
during which any of them runs: the last one to finish pauses for all of them, and no new one starts
during the pause. A stretch that never ends on its own (jobs keep overlapping) is closed after
MAX_STRETCH seconds: new jobs wait until the running ones are done and the pause is over.
"""
from __future__ import annotations

import json
import os
import threading
import time

REREAD_SECONDS = 1.0
MAX_STRETCH = 1.0
_cache = {"path": None, "read": float("-inf"), "level": 100}

# The busy stretch shared by every pacer of this process.
_shared = threading.Condition()
_stretch = {"active": 0, "since": 0.0, "pausing": False}


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
    (``sync``) and, when it ends the busy stretch, pauses so the GPU works about ``level`` percent
    of the time."""

    def __init__(self, sync=None, sleep=time.sleep, clock=time.perf_counter):
        self.sync, self.sleep, self.clock = sync, sleep, clock
        self.started = None
        self.paused = 0.0

    def start(self):
        if level() >= 100:
            self.started = None
            return
        with _shared:
            # No new batch during a pause, nor once the stretch has run long enough.
            while _stretch["pausing"] or (_stretch["active"] and self.clock() - _stretch["since"] >= MAX_STRETCH):
                _shared.wait(0.05)
            if not _stretch["active"]:
                _stretch["since"] = self.clock()
            _stretch["active"] += 1
            self.started = self.clock()

    def done(self):
        if self.started is None:
            return
        if self.sync is not None:
            self.sync()
        self.started = None
        with _shared:
            _stretch["active"] -= 1
            if _stretch["active"]:
                return  # the last batch of the stretch pauses for all of them
            busy = self.clock() - _stretch["since"]
            current = level()
            if current >= 100 or busy <= 0:
                _shared.notify_all()
                return
            wait = busy * (100 - current) / current
            _stretch["pausing"] = True
        try:
            self.paused += wait
            self.sleep(wait)
        finally:
            with _shared:
                _stretch["pausing"] = False
                _shared.notify_all()

    def __del__(self):
        # A batch that raised between start() and done() must not hold the stretch open forever.
        if self.started is not None:
            self.started = None
            try:
                with _shared:
                    _stretch["active"] = max(0, _stretch["active"] - 1)
                    _shared.notify_all()
            except Exception:  # noqa: BLE001 — interpreter shutdown
                pass
