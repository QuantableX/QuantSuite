"""Daily CoinMarketCap ranking archive: coverage and the backfill job.

The archive is the cache's ``rankings`` table (provider ``cmc``). Weekly
runs read its Sundays, daily runs every day. The job fetches only the
days that are missing, one request per day at the provider's throttle,
and never rewrites a stored day, so it can stop and resume at any time.
"""

from __future__ import annotations

import datetime as dt
import logging
import threading
from typing import Any, Callable, Iterable

from ..cache import Cache
from .cmc_snapshot import CmcSnapshotProvider, latest_complete_day

log = logging.getLogger(__name__)

# The first Sunday of the original weekly archive; daily history starts here.
DAILY_ARCHIVE_START = dt.date(2018, 12, 30)

# A backtest fetches at most this many missing days on its own (~40 s);
# more than that is the archive job's work, not a hidden half-hour wait.
ON_DEMAND_LIMIT = 31

# CMC answering nothing this many days in a row means it is blocking us.
_MAX_CONSECUTIVE_FAILURES = 5


def stored_days(cache: Cache) -> set[dt.date]:
    return set(cache.ranking_dates(CmcSnapshotProvider.name))


def missing_days(cache: Cache, days: Iterable[dt.date]) -> list[dt.date]:
    have = stored_days(cache)
    return sorted({d for d in days if d not in have})


def day_range(start: dt.date, end: dt.date) -> list[dt.date]:
    return [start + dt.timedelta(days=i) for i in range((end - start).days + 1)]


def coverage(cache: Cache, start: dt.date = DAILY_ARCHIVE_START, end: dt.date | None = None) -> dict[str, Any]:
    end = end or latest_complete_day()
    have = sorted(d for d in stored_days(cache) if start <= d <= end)
    total = max((end - start).days + 1, 0)
    return {
        "start": start.isoformat(),
        "end": end.isoformat(),
        "days": len(have),
        "totalDays": total,
        "missingDays": total - len(have),
        "firstDay": have[0].isoformat() if have else None,
        "lastDay": have[-1].isoformat() if have else None,
    }


def download_minutes(days: int, interval: float) -> int:
    return max(1, round(days * interval / 60))


class DailyArchiveJob:
    """Background backfill of the daily archive, one at a time per process.

    ``notify`` receives the status after every day; the RPC layer turns it
    into an ``engine:ranking_archive`` event, which also keeps the engine's
    idle watchdog from stopping the process mid-download.
    """

    def __init__(
        self,
        provider_factory: Callable[[], CmcSnapshotProvider],
        notify: Callable[[dict[str, Any]], None],
    ) -> None:
        self._provider_factory = provider_factory
        self._notify = notify
        self._lock = threading.Lock()
        self._cancel = threading.Event()
        self._thread: threading.Thread | None = None
        self._state: dict[str, Any] = self._idle_state()

    @staticmethod
    def _idle_state() -> dict[str, Any]:
        return {"running": False, "total": 0, "done": 0, "stored": 0, "failed": 0,
                "current": None, "error": None, "cancelled": False}

    def status(self, cache: Cache) -> dict[str, Any]:
        with self._lock:
            state = dict(self._state)
        return {**state, "coverage": coverage(cache)}

    def start(self, start: dt.date | None = None, end: dt.date | None = None) -> dict[str, Any]:
        provider = self._provider_factory()
        start = max(start or DAILY_ARCHIVE_START, DAILY_ARCHIVE_START)
        end = min(end or latest_complete_day(), latest_complete_day())
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                return {**self._state, "coverage": coverage(provider.cache)}
            todo = missing_days(provider.cache, day_range(start, end)) if start <= end else []
            self._cancel.clear()
            self._state = {**self._idle_state(), "running": bool(todo), "total": len(todo)}
            if todo:
                self._thread = threading.Thread(
                    target=self._run, args=(provider, todo), name="ranking-archive", daemon=True)
                self._thread.start()
            state = dict(self._state)
        return {**state, "coverage": coverage(provider.cache)}

    def cancel(self, cache: Cache) -> dict[str, Any]:
        self._cancel.set()
        return self.status(cache)

    def _update(self, provider: CmcSnapshotProvider, **changes: Any) -> None:
        with self._lock:
            self._state.update(changes)
            state = dict(self._state)
        self._notify({**state, "coverage": coverage(provider.cache)})

    def _run(self, provider: CmcSnapshotProvider, todo: list[dt.date]) -> None:
        failures_in_row = 0
        try:
            for day in todo:
                if self._cancel.is_set():
                    self._update(provider, running=False, cancelled=True, current=None)
                    return
                try:
                    stored = provider.fetch_day(day)
                    failures_in_row = 0
                    with self._lock:
                        self._state["stored"] += 1 if stored else 0
                except Exception as exc:  # noqa: BLE001 - one bad day must not end the job
                    failures_in_row += 1
                    log.warning("Daily ranking %s failed: %s", day, exc)
                    with self._lock:
                        self._state["failed"] += 1
                    if failures_in_row >= _MAX_CONSECUTIVE_FAILURES:
                        self._update(provider, running=False, current=None, error=(
                            f"CoinMarketCap stopped answering ({failures_in_row} days in a row failed, "
                            f"last {day}): {exc}"))
                        return
                with self._lock:
                    self._state["done"] += 1
                self._update(provider, current=day.isoformat())
            self._update(provider, running=False, current=None)
        except Exception as exc:  # noqa: BLE001 - report instead of dying silently
            log.exception("Daily ranking archive job crashed")
            self._update(provider, running=False, current=None, error=str(exc))
