"""Local reconstruction ranking provider.

For each candidate coin we pull a daily ``market_caps`` series from
CoinGecko's free ``/coins/{id}/market_chart`` endpoint (no API key
required) and persist it to the cache. The top-N ranking on any date
is then ``argsort_desc(market_cap[date])`` across the candidate set.

The public endpoint covers the past 365 days. Older history is usable
only when already cached. Rankings are limited to today's candidate pool
and can therefore omit delisted coins; use CMC for historical universes.
"""

from __future__ import annotations

import datetime as dt
import logging
import time
from typing import Callable

import requests

from ..cache import Cache, RankingRow, get_default_cache
from ..stablecoins import ExclusionList, build_default_stablecoin_list, build_default_wrapped_list
from .base import RankedCoin, RankingProvider

log = logging.getLogger(__name__)


_CG_MARKETS_URL = "https://api.coingecko.com/api/v3/coins/markets"
_CG_CHART_URL = "https://api.coingecko.com/api/v3/coins/{id}/market_chart"
_CG_MAX_FREE_DAYS = 365
_HEADERS = {
    "User-Agent": "RotationLab/0.1 (+https://github.com/)",
    "Accept": "application/json",
}


class LocalRankingUnavailable(RuntimeError):
    """The provider cannot complete this ranking; do not return partial data."""


class LocalReconstructProvider(RankingProvider):
    """Daily-granularity ranking built locally from a candidate pool."""

    name = "local"
    META_KEY_POOL = "local_recon_pool"
    # Bumped whenever a market-cap series lands in the cache; the ranking
    # build and the "no data for this date" markers are keyed against it,
    # so both fall out of date exactly when new data arrives.
    META_KEY_MCAP_REV = "local_recon_mcap_rev"
    META_KEY_BUILT_REV = "local_recon_built_rev"

    def __init__(
        self,
        cache: Cache | None = None,
        *,
        pool_size: int = 50,
        min_request_interval: float = 1.5,
        timeout: float = 20.0,
        session: requests.Session | None = None,
        progress: Callable[[str], None] | None = None,
    ) -> None:
        self.cache = cache or get_default_cache()
        self.pool_size = max(int(pool_size), 10)
        self.min_request_interval = min_request_interval
        self.timeout = timeout
        self._session = session or requests.Session()
        self._last_request_ts: float = 0.0
        self._stable = build_default_stablecoin_list()
        self._wrapped = build_default_wrapped_list()
        self.progress = progress
        # One fetch returns all the public endpoint's available history.
        self._fetched_series: set[str] = set()

    # ------------------------------------------------------------------ #

    def get_top_n(
        self,
        on_date: dt.date,
        n: int,
        *,
        exclude_stablecoins: bool = True,
        exclude_wrapped: bool = True,
    ) -> list[RankedCoin]:
        rows = self.cache.get_ranking(self.name, on_date)
        if not rows and not self._known_uncoverable(on_date):
            self._ensure_built_for(on_date)
            rows = self.cache.get_ranking(self.name, on_date)
            # Only trust the empty result once the provider has produced
            # rankings for *some* date -- otherwise the build itself failed
            # (offline, rate-limited) and the date deserves another try.
            if not rows and self.cache.latest_ranking_date(self.name):
                self._note_uncoverable(on_date)
        return self._filter_top(rows, n, exclude_stablecoins, exclude_wrapped)

    def available_dates(self) -> list[dt.date]:
        return self.cache.ranking_dates(self.name)

    # ------------------------------------------------------------------ #

    def _mcap_rev(self) -> str:
        return self.cache.get_meta(self.META_KEY_MCAP_REV) or "0"

    def _bump_mcap_rev(self) -> None:
        try:
            rev = int(self._mcap_rev())
        except ValueError:
            rev = 0
        self.cache.set_meta(self.META_KEY_MCAP_REV, str(rev + 1))

    @staticmethod
    def _uncoverable_key(day: dt.date) -> str:
        return f"local_recon_nodata:{day.isoformat()}"

    def _known_uncoverable(self, day: dt.date) -> bool:
        """True when an earlier run already found no ranking for ``day``.

        Dates that predate CoinGecko's history can never gain rows, and
        re-running the whole pool for each of them is what made a backtest
        starting before coverage crawl. The marker is tagged with the
        market-cap revision, so it expires as soon as new data lands.
        """

        marked = self.cache.get_meta(self._uncoverable_key(day))
        return marked is not None and marked == self._mcap_rev()

    def _note_uncoverable(self, day: dt.date) -> None:
        self.cache.set_meta(self._uncoverable_key(day), self._mcap_rev())

    # ------------------------------------------------------------------ #

    def _ensure_built_for(self, on_date: dt.date) -> None:
        """Make sure the local reconstruction covers ``on_date``."""

        if on_date < dt.datetime.now(dt.timezone.utc).date() - dt.timedelta(days=_CG_MAX_FREE_DAYS):
            # Cached older series remain useful, but downloading the current
            # pool cannot fill a hole outside the public endpoint's horizon.
            self._rebuild_rankings_around(on_date, allow_fetch=False)
            if not self.cache.get_ranking(self.name, on_date):
                raise LocalRankingUnavailable(
                    f"CoinGecko public history covers only the past {_CG_MAX_FREE_DAYS} days; "
                    f"no cached ranking for {on_date}. Use CMC or cached historical data."
                )
            return

        pool = self._ensure_pool()
        # Fetch each coin's market-cap history once; ranking is derived
        # from the combined cache when we are done.
        for i, (cg_id, symbol, name) in enumerate(pool):
            if self.progress:
                self.progress(f"CoinGecko market caps {symbol or cg_id} ({i + 1}/{len(pool)})")
            self._ensure_market_cap_series(cg_id, symbol, name, on_date)
        self._rebuild_rankings_around(on_date)

    # ------------------------------------------------------------------ #

    def _ensure_pool(self, *, allow_fetch: bool = True) -> list[tuple[str, str, str]]:
        """Return the candidate coin pool, fetching if needed.

        We snapshot the current top ``pool_size`` coins by market cap as
        candidates. This is a known limitation: very old coins that are
        no longer in the current top ``pool_size`` won't appear in
        historical rankings. The CMC scraper handles that case; this
        provider supplies daily rankings within the public one-year horizon.
        """

        cached = self.cache.get_meta(self.META_KEY_POOL)
        if cached:
            try:
                ids = cached.split(",")
                pool: list[tuple[str, str, str]] = []
                for cg_id in ids:
                    m = self.cache.lookup_coin_map(cg_id)
                    if m:
                        pool.append((cg_id, m.get("symbol") or "", m.get("name") or ""))
                if pool:
                    return pool
            except Exception:  # noqa: BLE001
                pass

        if not allow_fetch:
            return []
        rows = self._fetch_top_markets(self.pool_size)
        pool = []
        for r in rows:
            cg_id = r.get("id")
            symbol = (r.get("symbol") or "").upper()
            name = r.get("name") or ""
            if not cg_id:
                continue
            self.cache.upsert_coin_map(cg_id=cg_id, symbol=symbol, name=name)
            pool.append((cg_id, symbol, name))
        if pool:
            self.cache.set_meta(self.META_KEY_POOL, ",".join(p[0] for p in pool))
        return pool

    def _fetch_top_markets(self, n: int) -> list[dict]:
        out: list[dict] = []
        per_page = min(250, n)
        page = 1
        while len(out) < n:
            params = {
                "vs_currency": "usd",
                "order": "market_cap_desc",
                "per_page": per_page,
                "page": page,
            }
            data = self._cg_get(_CG_MARKETS_URL, params)
            if not isinstance(data, list) or not data:
                break
            out.extend(data)
            if len(data) < per_page:
                break
            page += 1
            if page > 10:
                break
        return out[:n]

    # ------------------------------------------------------------------ #

    def _ensure_market_cap_series(
        self,
        cg_id: str,
        symbol: str,
        name: str,
        target: dt.date,
    ) -> None:
        """Cache a daily ``market_cap`` series for ``cg_id``.

        We piggy-back on the ``ohlcv`` table by using ``coin_key`` of
        the form ``mcap:<cg_id>`` and storing the market cap value in
        the ``close`` column. This avoids adding another table while
        still being trivially queryable.
        """

        coin_key = f"mcap:{cg_id}"
        lo, hi = self.cache.ohlcv_coverage(coin_key)
        if hi and hi >= target and lo and lo <= target:
            return  # already covered
        if cg_id in self._fetched_series:
            # A coin whose history simply starts after `target` never
            # satisfies the coverage check, so without this it would be
            # re-fetched (throttled) once per rotation date.
            return
        try:
            series = self._fetch_market_chart(cg_id)
        except requests.HTTPError as exc:
            if exc.response is not None and exc.response.status_code == 404:
                log.warning("CoinGecko has no history for %s: %s", cg_id, exc)
                self._fetched_series.add(cg_id)
                return
            raise
        # Access failures and exhausted retries must abort the whole build.
        # Continuing coin-by-coin turns an outage into many minutes of waits
        # and can permanently cache an incomplete universe.
        self._fetched_series.add(cg_id)

        from ..cache import OhlcvRow

        rows = [
            OhlcvRow(
                coin_key=coin_key,
                day=day,
                open=None, high=None, low=None,
                close=mcap, volume=None,
                source="coingecko-mcap",
                approx=False,
            )
            for day, mcap in series.items()
        ]
        if rows:
            self.cache.upsert_ohlcv(rows)
            self._bump_mcap_rev()

    def _fetch_market_chart(self, cg_id: str) -> dict[dt.date, float]:
        # NOTE: `interval=daily` is paid-tier on the public CG API since
        # 2024. With days=365 the free endpoint already returns daily
        # granularity for ranges > 90 days, which is what we need.
        params = {
            "vs_currency": "usd",
            "days": _CG_MAX_FREE_DAYS,
        }
        data = self._cg_get(_CG_CHART_URL.format(id=cg_id), params)
        if not isinstance(data, dict) or not isinstance(data.get("market_caps"), list):
            raise LocalRankingUnavailable(f"CoinGecko returned invalid market-cap data for {cg_id}.")
        out: dict[dt.date, float] = {}
        for ts_ms, mcap in (data.get("market_caps") or []):
            day = dt.datetime.fromtimestamp(ts_ms / 1000, dt.timezone.utc).date()
            try:
                out[day] = float(mcap)
            except (TypeError, ValueError):
                continue
        return out

    # ------------------------------------------------------------------ #

    def _rebuild_rankings_around(self, target: dt.date, *, allow_fetch: bool = True) -> None:
        """Compute rankings for every day on which we have any data.

        We rebuild only when the cache has changed since the last build
        (tracked in ``meta`` as the market-cap revision). Past rebuilds
        remain valid because the market-cap series for past days is
        immutable, and a newly fetched coin changes every past day's
        ranking, so the rebuild is all-or-nothing.
        """

        rev = self._mcap_rev()
        if self.cache.get_meta(self.META_KEY_BUILT_REV) == rev:
            return  # no new market-cap data since the last build

        pool = self._ensure_pool(allow_fetch=allow_fetch)
        if not pool:
            return

        # Find the union of dates across all pool coins.
        with self.cache._conn() as con:  # using internal helper
            rows = con.execute(
                "SELECT coin_key, day, close FROM ohlcv WHERE coin_key LIKE 'mcap:%'"
            ).fetchall()
        if not rows:
            return

        per_day: dict[dt.date, list[tuple[str, float]]] = {}
        for coin_key, day, mcap in rows:
            if mcap is None:
                continue
            cg_id = coin_key.split(":", 1)[1]
            d = day if isinstance(day, dt.date) else dt.date.fromisoformat(day)
            per_day.setdefault(d, []).append((cg_id, float(mcap)))

        # We only persist rankings up to and including ``target`` plus a
        # small buffer; older rankings remain in cache from earlier runs.
        symbol_lookup = {p[0]: p[1] for p in pool}
        name_lookup = {p[0]: p[2] for p in pool}

        now = dt.datetime.now(dt.timezone.utc).replace(tzinfo=None)
        ranking_rows: list[RankingRow] = []
        for day, items in per_day.items():
            items.sort(key=lambda x: x[1], reverse=True)
            for rank, (cg_id, mcap) in enumerate(items, start=1):
                ranking_rows.append(
                    RankingRow(
                        provider=self.name,
                        snap_date=day,
                        rank=rank,
                        cg_id=cg_id,
                        symbol=symbol_lookup.get(cg_id),
                        name=name_lookup.get(cg_id),
                        market_cap=mcap,
                        price=None,
                        fetched_at=now,
                    )
                )
        self.cache.upsert_ranking(ranking_rows)
        self.cache.set_meta(self.META_KEY_BUILT_REV, rev)

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

    # ----------------------------- http helper -------------------------- #

    def _throttle(self) -> None:
        gap = time.monotonic() - self._last_request_ts
        if gap < self.min_request_interval:
            time.sleep(self.min_request_interval - gap)
        self._last_request_ts = time.monotonic()

    def _cg_get(self, url: str, params: dict):
        # One retry for transient transport/server failures. A rate limit or
        # an access restriction affects the entire provider, not one coin.
        for attempt in range(2):
            self._throttle()
            try:
                r = self._session.get(url, params=params, headers=_HEADERS, timeout=self.timeout)
            except requests.RequestException as exc:
                if attempt == 1:
                    raise LocalRankingUnavailable(f"CoinGecko request failed: {exc}") from exc
                log.info("CG request failed (%s); retrying", exc)
                continue
            if r.status_code in (401, 403):
                raise LocalRankingUnavailable(
                    f"CoinGecko denied access (HTTP {r.status_code}). "
                    "Use CMC or check CoinGecko API access before retrying."
                )
            if r.status_code == 429:
                retry_after = r.headers.get("Retry-After")
                hint = f" Retry-After: {retry_after}." if retry_after else " Try again later."
                raise LocalRankingUnavailable(f"CoinGecko rate limit reached (HTTP 429).{hint}")
            if r.status_code >= 500 and attempt == 0:
                continue
            if r.status_code != 404:
                try:
                    r.raise_for_status()
                except requests.HTTPError as exc:
                    raise LocalRankingUnavailable(f"CoinGecko request failed (HTTP {r.status_code}).") from exc
            else:
                r.raise_for_status()
            try:
                return r.json()
            except ValueError as exc:
                raise LocalRankingUnavailable("CoinGecko returned invalid JSON.") from exc
        raise LocalRankingUnavailable("CoinGecko request failed after retry.")
