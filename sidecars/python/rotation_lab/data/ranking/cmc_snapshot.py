"""CoinMarketCap historical snapshot provider.

CMC publishes weekly archives covering every Sunday from 28 Apr 2013
onwards. The visible HTML page at::

    https://coinmarketcap.com/historical/YYYYMMDD/

is rendered client-side these days, but the same data is exposed by
CMC's **public** internal JSON endpoint, which requires no API key:

    https://api.coinmarketcap.com/data-api/v3/cryptocurrency/listing/historical
        ?convert=USD&date=YYYY-MM-DD&limit=100&start=1

We use that endpoint first and fall back to HTML scraping (legacy
``__NEXT_DATA__`` blob then plain ``<table>``) for resilience if CMC
ever changes the API shape.

The endpoint answers every calendar day, not only Sundays (checked
2026-10-18: weekday values differ from the surrounding Sundays; today's
date is empty until the day is over). Weekly resolution reads the
Sunday on or before a date; daily resolution reads the previous day.
"""

from __future__ import annotations

import datetime as dt
import json
import logging
import re
import threading
import time
from typing import Iterable

import requests
from bs4 import BeautifulSoup

from ...config import RankingResolution
from ..cache import Cache, RankingRow, get_default_cache
from ..stablecoins import ExclusionList, build_default_stablecoin_list, build_default_wrapped_list
from .base import RankedCoin, RankingProvider

log = logging.getLogger(__name__)

_API_URL = "https://api.coinmarketcap.com/data-api/v3/cryptocurrency/listings/historical"
_BASE_URL = "https://coinmarketcap.com/historical/{ymd}/"
_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/124.0.0.0 Safari/537.36"
    ),
    "Accept": "application/json, text/plain, */*",
    "Accept-Language": "en-US,en;q=0.9",
    "Origin": "https://coinmarketcap.com",
    "Referer": "https://coinmarketcap.com/",
}

# Snapshot archive starts on this Sunday.
_ARCHIVE_FIRST_SUNDAY = dt.date(2013, 4, 28)


def latest_snapshot_sunday(today: dt.date | None = None) -> dt.date:
    """Return the most recent Sunday on or before ``today``.

    CMC snapshots are published every Sunday; for dates after the most
    recent Sunday we forward-fill.
    """

    today = today or dt.date.today()
    # weekday(): Mon=0..Sun=6
    offset = (today.weekday() - 6) % 7
    return today - dt.timedelta(days=offset)


def snapshot_for(target: dt.date) -> dt.date:
    """The CMC snapshot date that should be used for ``target``."""

    sunday = latest_snapshot_sunday(target)
    return max(sunday, _ARCHIVE_FIRST_SUNDAY)


def latest_complete_day(today: dt.date | None = None) -> dt.date:
    """The newest day CMC has a finished snapshot for: yesterday (UTC)."""

    today = today or dt.datetime.now(dt.timezone.utc).date()
    return today - dt.timedelta(days=1)


def snapshot_date(target: dt.date, resolution: RankingResolution) -> dt.date:
    """The snapshot that answers ``target`` at the given resolution.

    Daily takes the previous day: its ranking is complete before
    ``target``'s bar opens, so neither a daily close nor an intraday bar
    sees a market cap from its own day.
    """

    if resolution == RankingResolution.DAILY:
        return max(target - dt.timedelta(days=1), _ARCHIVE_FIRST_SUNDAY)
    return snapshot_for(target)


def iter_snapshot_dates(start: dt.date, end: dt.date) -> Iterable[dt.date]:
    """All Sunday snapshot dates covering ``[start, end]`` (inclusive)."""

    d = snapshot_for(start)
    end_snap = snapshot_for(end)
    while d <= end_snap:
        yield d
        d += dt.timedelta(days=7)


class CmcSnapshotScraperError(RuntimeError):
    """Raised when CMC HTML cannot be parsed.

    Caught by the provider registry to trigger fallback.
    """


class CmcSnapshotProvider(RankingProvider):
    """Free ranking provider reading CMC's weekly or daily snapshots."""

    name = "cmc"

    # One request clock per process: the daily-archive job and a backtest
    # can fetch at the same time and must not double CMC's request rate.
    _throttle_lock = threading.Lock()
    _last_request_ts: float = 0.0

    def __init__(
        self,
        cache: Cache | None = None,
        *,
        min_request_interval: float = 1.2,
        timeout: float = 20.0,
        ttl_seconds: float = 60 * 60 * 24,
        session: requests.Session | None = None,
    ) -> None:
        self.cache = cache or get_default_cache()
        self.min_request_interval = min_request_interval
        self.timeout = timeout
        self.ttl_seconds = ttl_seconds
        self._session = session or requests.Session()
        self._stable = build_default_stablecoin_list()
        self._wrapped = build_default_wrapped_list()

    # ------------------------------------------------------------------ #

    def get_top_n(
        self,
        on_date: dt.date,
        n: int,
        *,
        exclude_stablecoins: bool = True,
        exclude_wrapped: bool = True,
        resolution: RankingResolution = RankingResolution.WEEKLY,
    ) -> list[RankedCoin]:
        snap = snapshot_date(on_date, resolution)
        if resolution == RankingResolution.DAILY:
            # A local "today" can already be tomorrow in UTC; CMC's newest
            # finished day is yesterday in UTC.
            snap = min(snap, latest_complete_day())
        rows = self._ensure_snapshot(snap, resolution)
        return self._filter_top(rows, n, exclude_stablecoins, exclude_wrapped)

    def available_dates(self) -> list[dt.date]:
        return self.cache.ranking_dates(self.name)

    def fetch_day(self, day: dt.date) -> int:
        """Fetch and store one day's snapshot for the daily archive.

        Never rewrites a stored day; returns the number of rows stored.
        """

        if self.cache.get_ranking(self.name, day, limit=1):
            return 0
        rows = self._fetch_snapshot(day)
        return self.cache.upsert_ranking(rows)

    # ------------------------------------------------------------------ #

    def _ensure_snapshot(
        self,
        snap: dt.date,
        resolution: RankingResolution = RankingResolution.WEEKLY,
    ) -> list[RankingRow]:
        # Past snapshots are immutable -> cache forever. Weekly keeps its
        # rule (the newest Sunday refreshes once a day); a daily snapshot
        # is only ever asked for once its day is over, so only yesterday
        # counts as current.
        if resolution == RankingResolution.DAILY:
            is_current = snap >= latest_complete_day()
        else:
            is_current = snap >= latest_snapshot_sunday()
        fresh = self.cache.is_ranking_fresh(self.name, snap, self.ttl_seconds)
        if not is_current or fresh:
            cached = self.cache.get_ranking(self.name, snap)
            if cached:
                return cached
        try:
            rows = self._fetch_snapshot(snap)
        except Exception as exc:  # noqa: BLE001 - we want to log & re-raise typed
            cached = self.cache.get_ranking(self.name, snap)
            if cached:
                log.warning("CMC fetch failed (%s); using stale cache: %s", snap, exc)
                return cached
            raise CmcSnapshotScraperError(f"Could not fetch CMC snapshot {snap}: {exc}") from exc
        self.cache.upsert_ranking(rows)
        return rows

    # ------------------------------------------------------------------ #

    def _throttle(self) -> None:
        cls = type(self)
        with cls._throttle_lock:
            gap = time.monotonic() - cls._last_request_ts
            if gap < self.min_request_interval:
                time.sleep(self.min_request_interval - gap)
            cls._last_request_ts = time.monotonic()

    def _http_get(self, url: str, *, params: dict | None = None) -> requests.Response:
        self._throttle()
        r = self._session.get(url, params=params, headers=_HEADERS, timeout=self.timeout)
        r.raise_for_status()
        return r

    # ------------------------------------------------------------------ #

    def _fetch_snapshot(self, snap: dt.date) -> list[RankingRow]:
        # Try the JSON API first (no key, public). On failure, fall back
        # to scraping the rendered HTML.
        try:
            parsed = self._fetch_via_api(snap)
        except Exception as exc:  # noqa: BLE001
            log.info("CMC data-api failed for %s (%s); falling back to HTML", snap, exc)
            parsed = None

        if not parsed:
            url = _BASE_URL.format(ymd=snap.strftime("%Y%m%d"))
            html = self._http_get(url).text
            parsed = self._parse_next_data(html, snap) or self._parse_html_table(html, snap)

        if not parsed:
            raise CmcSnapshotScraperError(
                f"Snapshot {snap} could not be parsed from CMC API or HTML"
            )
        return parsed

    # ------------------------------------------------------------------ #

    def _fetch_via_api(self, snap: dt.date) -> list[RankingRow] | None:
        """Hit CMC's internal data-api JSON endpoint.

        Returns None when the structure is unrecognised so the caller
        can fall back to HTML parsing.
        """

        params = {
            "convert": "USD",
            "date": snap.isoformat(),
            "limit": 100,
            "start": 1,
        }
        r = self._http_get(_API_URL, params=params)
        try:
            payload = r.json()
        except ValueError:
            return None

        # The current shape is ``{"data": [...coins...]}`` but older
        # snapshots used ``{"data": {"cryptoCurrencyList": [...]}}``,
        # so handle both.
        raw = payload.get("data") if isinstance(payload, dict) else None
        if isinstance(raw, list):
            listing = raw
        elif isinstance(raw, dict):
            listing = raw.get("cryptoCurrencyList") or raw.get("data") or []
        else:
            listing = []
        if not isinstance(listing, list) or not listing:
            return None

        now = dt.datetime.utcnow()
        rows: list[RankingRow] = []
        for entry in listing:
            try:
                rank = int(entry.get("cmcRank") or entry.get("rank") or 0)
            except (TypeError, ValueError):
                continue
            if rank <= 0:
                continue
            symbol = (entry.get("symbol") or "").upper() or None
            name = entry.get("name")
            slug = entry.get("slug")
            quotes = entry.get("quotes") or []
            usd = next(
                (q for q in quotes if (q or {}).get("name") == "USD"),
                quotes[0] if quotes else {},
            )
            price = _to_float(usd.get("price")) if isinstance(usd, dict) else None
            mcap = _to_float(usd.get("marketCap") or usd.get("market_cap")) if isinstance(usd, dict) else None
            rows.append(
                RankingRow(
                    provider=self.name, snap_date=snap, rank=rank, cg_id=None,
                    symbol=symbol, name=name, market_cap=mcap, price=price,
                    fetched_at=now,
                )
            )
            if slug:
                self.cache.upsert_coin_map(
                    cg_id=f"cmc:{slug}", symbol=symbol, name=name,
                    cmc_slug=slug, last_seen=snap,
                )
        rows.sort(key=lambda r: r.rank)
        # Dedupe ranks just in case
        seen: set[int] = set()
        deduped: list[RankingRow] = []
        for r in rows:
            if r.rank in seen:
                continue
            seen.add(r.rank)
            deduped.append(r)
        return deduped or None

    # ------------------------------------------------------------------ #

    def _parse_next_data(self, html: str, snap: dt.date) -> list[RankingRow] | None:
        """Parse the embedded ``__NEXT_DATA__`` JSON.

        Returns None when the structure is missing or unrecognised.
        """

        m = re.search(
            r'<script id="__NEXT_DATA__"[^>]*>(.*?)</script>', html, re.DOTALL
        )
        if not m:
            return None
        try:
            payload = json.loads(m.group(1))
        except json.JSONDecodeError:
            return None

        listing = _walk_for_listing(payload)
        if not listing:
            return None

        now = dt.datetime.utcnow()
        rows: list[RankingRow] = []
        for entry in listing:
            try:
                rank_val = entry.get("cmcRank") or entry.get("rank")
                if rank_val is None:
                    continue
                rank = int(rank_val)
                symbol = (entry.get("symbol") or "").upper() or None
                name = entry.get("name")
                quotes = entry.get("quote") or {}
                usd = (quotes.get("USD") if isinstance(quotes, dict) else None) or {}
                if not usd and isinstance(quotes, list):
                    usd = next((q for q in quotes if q.get("name") == "USD"), {})
                price = _to_float(usd.get("price"))
                mcap = _to_float(usd.get("marketCap") or usd.get("market_cap"))
                cg_id = entry.get("slug") or None
                rows.append(
                    RankingRow(
                        provider=self.name,
                        snap_date=snap,
                        rank=rank,
                        cg_id=None,
                        symbol=symbol,
                        name=name,
                        market_cap=mcap,
                        price=price,
                        fetched_at=now,
                    )
                )
                # CMC slug stored separately so a later mapping step can
                # bridge it to CoinGecko ids.
                if cg_id:
                    self.cache.upsert_coin_map(
                        cg_id=f"cmc:{cg_id}", symbol=symbol, name=name,
                        cmc_slug=cg_id, last_seen=snap,
                    )
            except Exception:  # noqa: BLE001 - defensive against schema drift
                continue
        rows.sort(key=lambda r: r.rank)
        # Re-rank densely in case of duplicates / gaps.
        deduped: list[RankingRow] = []
        seen_ranks: set[int] = set()
        for r in rows:
            if r.rank in seen_ranks:
                continue
            seen_ranks.add(r.rank)
            deduped.append(r)
        return deduped or None

    # ------------------------------------------------------------------ #

    def _parse_html_table(self, html: str, snap: dt.date) -> list[RankingRow] | None:
        """Fallback HTML parser, used if ``__NEXT_DATA__`` is missing."""

        soup = BeautifulSoup(html, "lxml")
        table = soup.find("table")
        if not table:
            return None
        rows: list[RankingRow] = []
        now = dt.datetime.utcnow()
        for tr in table.find_all("tr"):
            cells = tr.find_all("td")
            if len(cells) < 5:
                continue
            try:
                rank = int(cells[0].get_text(strip=True))
            except ValueError:
                continue
            name_cell = cells[1].get_text(" ", strip=True)
            symbol_cell = cells[2].get_text(strip=True)
            mcap = _parse_money(cells[3].get_text(strip=True))
            price = _parse_money(cells[4].get_text(strip=True))
            rows.append(
                RankingRow(
                    provider=self.name,
                    snap_date=snap,
                    rank=rank,
                    cg_id=None,
                    symbol=symbol_cell.upper() or None,
                    name=name_cell or None,
                    market_cap=mcap,
                    price=price,
                    fetched_at=now,
                )
            )
        return rows or None

    # ------------------------------------------------------------------ #

    def _filter_top(
        self,
        rows: list[RankingRow],
        n: int,
        exclude_stablecoins: bool,
        exclude_wrapped: bool,
    ) -> list[RankedCoin]:
        excl: ExclusionList | None = None
        if exclude_stablecoins:
            excl = self._stable
        if exclude_wrapped:
            excl = (excl or ExclusionList((), ())).merge(
                symbols=self._wrapped._symbols, cg_ids=self._wrapped._cg_ids
            )
        out: list[RankedCoin] = []
        for r in rows:
            if excl and excl.matches(symbol=r.symbol, cg_id=r.cg_id):
                continue
            out.append(
                RankedCoin(
                    rank=len(out) + 1,
                    cg_id=r.cg_id,
                    symbol=r.symbol or "",
                    name=r.name,
                    market_cap=r.market_cap,
                    price=r.price,
                )
            )
            if len(out) >= n:
                break
        return out


# ------------------------------- helpers -------------------------------- #


def _to_float(value) -> float | None:
    if value is None:
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


_money_re = re.compile(r"[\$,\s]")


def _parse_money(text: str) -> float | None:
    if not text:
        return None
    stripped = _money_re.sub("", text).replace("--", "").strip()
    if not stripped:
        return None
    try:
        return float(stripped)
    except ValueError:
        return None


def _walk_for_listing(payload) -> list[dict] | None:
    """Walk the ``__NEXT_DATA__`` payload to find a list of coin dicts.

    CMC's exact JSON layout has changed several times; we look for any
    list whose items carry both ``symbol`` and ``cmcRank``/``rank``.
    """

    best: list[dict] | None = None
    stack: list = [payload]
    seen: set[int] = set()
    while stack:
        node = stack.pop()
        oid = id(node)
        if oid in seen:
            continue
        seen.add(oid)
        if isinstance(node, list):
            if (
                node
                and isinstance(node[0], dict)
                and "symbol" in node[0]
                and ("cmcRank" in node[0] or "rank" in node[0])
            ):
                if best is None or len(node) > len(best):
                    best = node
            else:
                stack.extend(node)
        elif isinstance(node, dict):
            stack.extend(node.values())
    return best
