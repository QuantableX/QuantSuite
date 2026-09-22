"""OHLCV fetcher with a CCXT exchange chain + CoinGecko fallback.

A coin in RotationLab is identified by its CoinGecko id (when known)
plus a symbol like ``BTC``. The fetcher resolves it to a trading pair
(e.g. ``BTC/USDT``) and pulls daily candles via CCXT, walking a chain of
exchanges — Binance first, then Bybit/OKX/Bitfinex/KuCoin/Gate — because
plenty of top-100 coins never list on Binance (LEO is a Bitfinex token,
HYPE trades on Bybit). Later exchanges are only contacted for coins the
earlier ones do not list.

If no exchange in the chain has the pair, we fall back to CoinGecko's
``/coins/{id}/market_chart`` and synthesise an "approx" OHLCV row from
the daily price points (high/low from the daily price array's min/max,
open from the previous close). That fallback is last-resort: the public
CoinGecko API only serves one year of history, so exchange candles are
strongly preferred for backtests.

Everything is cached by ``(coin_key, day)`` in SQLite.
"""

from __future__ import annotations

import datetime as dt
import logging
import time
from dataclasses import dataclass

import pandas as pd
import requests

from .cache import Cache, OhlcvBar, OhlcvRow, get_default_cache

log = logging.getLogger(__name__)

# Intraday timeframe → milliseconds per bar (used to advance the CCXT cursor).
_TF_MS: dict[str, int] = {
    "1m": 60 * 1000,
    "1h": 60 * 60 * 1000,
    "4h": 4 * 60 * 60 * 1000,
    "12h": 12 * 60 * 60 * 1000,
}

_CG_CHART_URL = "https://api.coingecko.com/api/v3/coins/{id}/market_chart"
_CG_MARKETS_URL = "https://api.coingecko.com/api/v3/coins/markets"
_CG_HEADERS = {
    "User-Agent": "RotationLab/0.1",
    "Accept": "application/json",
}

# How deep into the market-cap ranking to look when bridging a ticker to a
# CoinGecko id. 4 x 250 = top 1000, matching the largest top-N the UI allows,
# so no configurable universe can outrun the lookup. Pages are only fetched
# when a symbol is not already mapped in the local coin_map.
_CG_ID_PAGES = 4
_CG_ID_PER_PAGE = 250

# CoinGecko's public API caps market_chart history at one year; asking for
# more (including the old `days=max`) is rejected outright with HTTP 401
# error 10012, which is not a rate limit and never resolves by retrying.
_CG_MAX_FREE_DAYS = 365

# Meta key holding the earliest date any exchange has candles for a coin.
_HISTORY_FLOOR_KEY = "ohlcv_first:{key}"


# USD-ish quote currencies, best first. Used as a preference ranking, not a
# filter on how a venue happens to spell its market ids.
_BINANCE_QUOTES = ("USDT", "USDC", "USD", "BUSD", "FDUSD", "TUSD", "DAI")
_QUOTE_RANK = {q: i for i, q in enumerate(_BINANCE_QUOTES)}

# Exchanges tried in order. Binance has the broadest and deepest coverage,
# but plenty of top-100 coins never list there — and exchange-native tokens
# essentially only trade on their own venue (BGB on Bitget, KCS on KuCoin,
# OKB on OKX), which is why the chain has to include the mid-tier venues.
_EXCHANGE_CHAIN = (
    "binance", "bybit", "okx", "bitget", "kucoin", "gate", "mexc", "bitfinex", "htx",
)


@dataclass(frozen=True)
class CoinRef:
    cg_id: str | None
    symbol: str
    binance_symbol: str | None = None


@dataclass
class OhlcvSeries:
    coin_ref: CoinRef
    frame: pd.DataFrame  # indexed by date, columns: open, high, low, close, volume, source, approx

    @property
    def coin_key(self) -> str:
        if self.coin_ref.cg_id:
            return f"cg:{self.coin_ref.cg_id}"
        return f"sym:{self.coin_ref.symbol.upper()}"


def coin_key_for(ref: CoinRef) -> str:
    if ref.cg_id:
        return f"cg:{ref.cg_id}"
    return f"sym:{ref.symbol.upper()}"


class CoinGeckoIdResolver:
    """Bridges a ticker symbol to a real CoinGecko coin id.

    Rankings scraped from CoinMarketCap carry no CoinGecko id at all (see
    ``CmcSnapshotProvider``, which stores its own slug under a ``cmc:``
    prefix instead). Without this bridge ``CoinRef.cg_id`` is always None
    on the CMC path, the CoinGecko OHLCV fallback can never fire, and any
    coin that Binance does not list ends up with no data whatsoever.

    Resolution order: the local ``coin_map`` table first, then one pass
    over CoinGecko's market-cap-ordered listing (highest cap wins when
    several coins share a ticker), which is persisted back into
    ``coin_map`` so later runs answer from SQLite.
    """

    def __init__(
        self,
        cache: Cache,
        session: requests.Session,
        throttle: "callable[[], None] | None" = None,
        *,
        timeout: float = 20.0,
    ) -> None:
        self.cache = cache
        self._session = session
        self._throttle = throttle
        self._timeout = timeout
        # Per-process memo, including negative hits, so a run with many
        # unlisted coins does not re-query for each one.
        self._memo: dict[str, str | None] = {}
        self._listing_loaded = False

    def resolve(self, symbol: str | None) -> str | None:
        if not symbol:
            return None
        sym = symbol.upper()
        if sym in self._memo:
            return self._memo[sym]

        found = self.cache.lookup_cg_id_by_symbol(sym)
        if found:
            self._memo[sym] = found
            return found

        if not self._listing_loaded:
            self._load_listing()
            found = self.cache.lookup_cg_id_by_symbol(sym)

        self._memo[sym] = found
        if not found:
            log.info("No CoinGecko id found for symbol %s", sym)
        return found

    def _load_listing(self) -> None:
        """Fetch the market-cap-ordered coin listing once per process."""

        self._listing_loaded = True
        seen: set[str] = set()
        for page in range(1, _CG_ID_PAGES + 1):
            if self._throttle:
                self._throttle()
            try:
                r = self._session.get(
                    _CG_MARKETS_URL,
                    params={
                        "vs_currency": "usd",
                        "order": "market_cap_desc",
                        "per_page": _CG_ID_PER_PAGE,
                        "page": page,
                    },
                    headers=_CG_HEADERS,
                    timeout=self._timeout,
                )
                if r.status_code == 429:
                    log.warning("CoinGecko rate-limited the id listing (page %s)", page)
                    return
                r.raise_for_status()
                entries = r.json() or []
            except Exception as exc:  # noqa: BLE001
                log.warning("CoinGecko id listing failed (page %s): %s", page, exc)
                return
            if not entries:
                return
            page_rows: list[dict] = []
            for entry in entries:
                cg_id = entry.get("id")
                sym = (entry.get("symbol") or "").upper()
                if not cg_id or not sym or sym in seen:
                    continue
                # Ordered by market cap, so the first entry for a ticker is
                # the one a top-N ranking actually means.
                seen.add(sym)
                page_rows.append(
                    {"cg_id": cg_id, "symbol": sym, "name": entry.get("name") or None}
                )
            # One transaction per page instead of one per coin.
            self.cache.upsert_coin_map_many(page_rows)


class OhlcvFetcher:
    """Resolves OHLCV for arbitrary coin references."""

    def __init__(
        self,
        cache: Cache | None = None,
        *,
        min_request_interval: float = 1.2,
        ccxt_exchange: str = "binance",
        ccxt_exchanges: "tuple[str, ...] | None" = None,
    ) -> None:
        self.cache = cache or get_default_cache()
        self.min_request_interval = min_request_interval
        self._last_request_ts: float = 0.0
        # Primary exchange first, then the rest of the chain. Later exchanges
        # are only ever contacted for coins the earlier ones do not list.
        names = ccxt_exchanges or (ccxt_exchange,) + tuple(
            e for e in _EXCHANGE_CHAIN if e != ccxt_exchange
        )
        self._exchange_names: tuple[str, ...] = tuple(names)
        self._exchange_name = self._exchange_names[0]
        self._exchanges: dict[str, object] = {}
        self._markets: dict[str, dict] = {}
        self._market_index_cache: dict[str, dict[str, dict[str, str]]] = {}
        self._session = requests.Session()
        self._id_resolver = CoinGeckoIdResolver(
            self.cache, self._session, self._throttle_cg
        )

    # ------------------------------------------------------------------ #

    def get_series(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
        timeframe: str = "1d",
    ) -> OhlcvSeries:
        if timeframe != "1d":
            return self._get_series_intraday(ref, start, end, timeframe)
        key = coin_key_for(ref)
        cached_rows = self.cache.get_ohlcv_range(key, start, end)
        # The current UTC day is still forming, so whatever an earlier run
        # cached for it is a partial candle (close = price at fetch time,
        # incomplete high/low/volume). It never counts as settled and is
        # re-fetched and replaced until the day is over.
        settled_days = {r.day for r in cached_rows if r.day < dt.datetime.utcnow().date()}

        expected_days = set(_business_days_between(start, end))
        missing = expected_days - settled_days
        if missing and not self._skip_fetch(key, missing, settled_days, end):
            self._populate(ref, start, end)
            cached_rows = self.cache.get_ohlcv_range(key, start, end)

        df = _rows_to_frame(cached_rows)
        return OhlcvSeries(coin_ref=ref, frame=df)

    def _skip_fetch(
        self,
        key: str,
        missing: set[dt.date],
        cached_days: set[dt.date],
        end: dt.date,
    ) -> bool:
        """True when every absent day predates the coin's first ever candle.

        A coin that listed mid-window can never have candles for the days
        before it existed, so those days stay "missing" forever and would
        otherwise make every single run re-walk the whole exchange chain.

        The cut-off is not inferred from what happens to be cached — that
        would let a 180-day Live run convince a later 3-year backtest that
        no deeper history exists. It is only ever the earliest date an
        exchange actually reported for this coin (see `_note_history_floor`),
        and the cached series must also be current and hole-free: anything
        absent in the middle or at the end is a real gap and still fetches.
        """

        if not cached_days:
            return False
        if max(cached_days) < min(end, dt.date.today()) - dt.timedelta(days=2):
            return False
        raw = self.cache.get_meta(_HISTORY_FLOOR_KEY.format(key=key))
        if not raw:
            return False
        try:
            floor = dt.date.fromisoformat(raw)
        except ValueError:
            return False
        return all(d < floor for d in missing)

    def _note_history_floor(self, key: str, requested_start: dt.date, first_day: dt.date) -> None:
        """Record that an exchange has nothing before ``first_day``.

        Only called for exchange candles: CoinGecko's one-year window is an
        API restriction, not a fact about the coin, and must never be
        mistaken for a listing date.
        """

        if first_day <= requested_start:
            return
        meta_key = _HISTORY_FLOOR_KEY.format(key=key)
        prev = self.cache.get_meta(meta_key)
        if prev:
            try:
                if dt.date.fromisoformat(prev) <= first_day:
                    return
            except ValueError:
                pass
        self.cache.set_meta(meta_key, first_day.isoformat())

    # ------------------------------------------------------------------ #

    def _populate(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
    ) -> None:
        # 1) try the CCXT exchange chain
        try:
            rows = self._fetch_ccxt(ref, start, end)
        except Exception as exc:  # noqa: BLE001
            log.info("CCXT fetch failed for %s: %s", ref.symbol, exc)
            rows = []

        if rows:
            # The best exchange's earliest candle is a fact about the coin:
            # remember it so later runs stop hunting for history that no
            # exchange has.
            self._note_history_floor(
                coin_key_for(ref), start, min(r.day for r in rows)
            )

        if not rows:
            # 2) fallback: CoinGecko market chart (approx OHLC). The id is
            #    resolved from the ticker when the ranking provider did not
            #    supply one, which is always the case on the CMC path.
            cg_id = self._coingecko_id(ref)
            if cg_id:
                try:
                    rows = self._fetch_coingecko(ref, start, end, cg_id)
                except Exception as exc:  # noqa: BLE001
                    log.warning(
                        "CoinGecko OHLCV fallback failed for %s (%s): %s",
                        ref.symbol, cg_id, exc
                    )

        # Everything fetched is written back, not just the days that were
        # absent: upsert is INSERT OR REPLACE, so this is what corrects a
        # day that an earlier run cached while it was still forming.
        if rows:
            self.cache.upsert_ohlcv(rows)

    # --------------------------- intraday ------------------------------- #

    def _get_series_intraday(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
        tf: str,
    ) -> OhlcvSeries:
        """Resolve intraday (1m/1h/4h/12h) candles via the CCXT chain only.

        CoinGecko's free tier can't serve reliable intraday OHLC, so coins
        that no exchange in the chain lists simply return an empty frame at
        intraday resolution (the engine notes the gap and skips them).
        """

        key = coin_key_for(ref)
        start_dt = dt.datetime.combine(start, dt.time.min)
        end_dt = dt.datetime.combine(end, dt.time.max)

        lo, hi = self.cache.intraday_coverage(key, tf)
        bar_ms = _TF_MS.get(tf, 60 * 60 * 1000)
        bar_delta = dt.timedelta(milliseconds=bar_ms)
        # Fetch when the cache doesn't already span the requested window.
        needs_fetch = (
            lo is None
            or hi is None
            or lo > start_dt + bar_delta
            or hi < end_dt - bar_delta
        )
        if not needs_fetch:
            # Spanning is not the same as covering: fetching Jan-Feb and
            # then May-Jun leaves coverage Jan-Jun with a hole in the
            # middle that MIN/MAX cannot see. Count the bars in the part
            # of the window the cache claims, and refetch if any are gone.
            inner_lo = max(lo, start_dt)
            inner_hi = min(hi, end_dt)
            expected = int((inner_hi - inner_lo).total_seconds() * 1000 // bar_ms) + 1
            cached_bars = self.cache.intraday_bar_count(key, tf, inner_lo, inner_hi)
            needs_fetch = cached_bars < expected
        if needs_fetch:
            try:
                bars = self._fetch_ccxt_intraday(ref, start, end, tf)
            except Exception as exc:  # noqa: BLE001
                log.info("CCXT intraday fetch failed for %s (%s): %s", ref.symbol, tf, exc)
                bars = []
            if bars:
                self.cache.upsert_ohlcv_intraday(bars)

        cached = self.cache.get_ohlcv_intraday_range(key, tf, start_dt, end_dt)
        return OhlcvSeries(coin_ref=ref, frame=_bars_to_frame(cached))

    def _fetch_ccxt_intraday(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
        tf: str,
    ) -> list[OhlcvBar]:
        # Same delisted-stub hazard as the daily path: prefer the candidate
        # whose candles actually reach the end of the window.
        target_end = min(end, dt.date.today())
        best: tuple[str, str, list[OhlcvBar]] | None = None
        best_last: dt.datetime | None = None

        for name, pair in self._resolve_pairs(ref):
            bars = self._fetch_ccxt_intraday_one(name, pair, ref, start, end, tf)
            if not bars:
                continue
            last_ts = max(b.ts for b in bars)
            if best_last is None or last_ts > best_last:
                best, best_last = (name, pair, bars), last_ts
            if last_ts.date() >= target_end - dt.timedelta(days=3):
                break

        if best is None:
            return []
        name, pair, bars = best
        if ref.cg_id and name == self._exchange_name:
            self.cache.upsert_coin_map(
                cg_id=ref.cg_id, symbol=ref.symbol, binance_symbol=pair
            )
        return bars

    def _fetch_ccxt_intraday_one(
        self,
        name: str,
        pair: str,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
        tf: str,
    ) -> list[OhlcvBar]:
        ex = self._ensure_exchange(name)
        if ex is None:
            return []

        since_ms = int(
            dt.datetime.combine(start, dt.time.min).replace(tzinfo=dt.timezone.utc).timestamp() * 1000
        )
        end_ms = int(
            dt.datetime.combine(end, dt.time.max).replace(tzinfo=dt.timezone.utc).timestamp() * 1000
        )
        bar_ms = _TF_MS.get(tf, 60 * 60 * 1000)

        coin_key = coin_key_for(ref)
        out: list[OhlcvBar] = []
        cursor = since_ms
        # Three years of minutes need ~1,600 pages of 1,000, or ~5,300
        # on venues that cap responses at 300. Never silently truncate.
        page_limit = 10_000 if tf == "1m" else 5000
        for _ in range(page_limit):
            try:
                batch = ex.fetch_ohlcv(pair, timeframe=tf, since=cursor, limit=1000)
            except Exception as exc:  # noqa: BLE001
                log.info("CCXT fetch_ohlcv error %s @ %s: %s", pair, cursor, exc)
                break
            if not batch:
                break
            for ts_ms, o, h, l, c, v in batch:
                if ts_ms > end_ms:
                    break
                ts = dt.datetime.utcfromtimestamp(ts_ms / 1000)
                out.append(
                    OhlcvBar(
                        coin_key=coin_key, tf=tf, ts=ts,
                        open=float(o) if o is not None else None,
                        high=float(h) if h is not None else None,
                        low=float(l) if l is not None else None,
                        close=float(c) if c is not None else None,
                        volume=float(v) if v is not None else None,
                        source=f"ccxt:{name}:{pair}",
                        approx=False,
                    )
                )
            last_ts = batch[-1][0]
            if last_ts >= end_ms or last_ts == cursor:
                break
            cursor = last_ts + bar_ms
        else:
            raise RuntimeError(f"{name} {pair} {tf}: page limit reached before the requested end; history is incomplete")
        return out

    # ----------------------------- CCXT --------------------------------- #

    def _ensure_exchange(self, name: str | None = None):
        """Lazily construct an exchange client and load its markets."""

        name = name or self._exchange_name
        if name not in self._exchanges:
            import ccxt

            try:
                cls = getattr(ccxt, name)
            except AttributeError:
                log.warning("Unknown CCXT exchange %r; skipping", name)
                self._exchanges[name] = None
                self._markets[name] = {}
                return None
            ex = cls({"enableRateLimit": True, "timeout": 20000})
            try:
                self._markets[name] = ex.load_markets()
            except Exception as exc:  # noqa: BLE001
                log.warning("CCXT load_markets failed for %s: %s", name, exc)
                self._markets[name] = {}
            self._exchanges[name] = ex
        return self._exchanges[name]

    def _market_index(self, name: str) -> dict[str, dict[str, str]]:
        """``base -> {"spot": market, "swap": market}`` for one exchange.

        Built from CCXT's market metadata rather than by guessing market id
        strings: venues spell things differently (``XMR/USDT:USDT`` for a
        perpetual, odd quote assets, inactive leftovers), and matching on
        ``base``/``quote``/``spot`` is the part that stays true everywhere.
        """

        cached = self._market_index_cache.get(name)
        if cached is not None:
            return cached

        self._ensure_exchange(name)
        markets = self._markets.get(name) or {}
        index: dict[str, dict[str, str]] = {}
        best_rank: dict[tuple[str, str], int] = {}
        for market in markets.values():
            if not isinstance(market, dict) or market.get("active") is False:
                continue
            base = (market.get("base") or "").upper()
            quote = (market.get("quote") or "").upper()
            symbol = market.get("symbol")
            if not base or not symbol or quote not in _QUOTE_RANK:
                continue
            kind = "spot" if market.get("spot") else "swap" if market.get("swap") else None
            if kind is None:
                continue
            rank = _QUOTE_RANK[quote]
            if best_rank.get((base, kind), 99) <= rank:
                continue
            best_rank[(base, kind)] = rank
            index.setdefault(base, {})[kind] = symbol

        self._market_index_cache[name] = index
        return index

    def _pair_on(self, name: str, ref: CoinRef, kind: str) -> str | None:
        """This coin's best ``kind`` ("spot"/"swap") market on one exchange."""

        if kind == "spot" and name == self._exchange_name and ref.binance_symbol:
            return ref.binance_symbol
        return self._market_index(name).get(ref.symbol.upper(), {}).get(kind)

    def _resolve_pairs(self, ref: CoinRef):
        """Yield ``(exchange, pair)`` candidates in priority order.

        Spot markets across the whole chain come first; perpetual swaps are
        only offered once no venue has spot at all (newer tokens often list
        a perp before a spot book, and a perp's daily closes track spot far
        more closely than a synthesised CoinGecko approximation does).

        A generator on purpose: each exchange's ``load_markets`` only runs
        when the consumer actually asks for the next candidate, so the
        common case (a coin listed on Binance) still costs exactly one
        market load and never touches the rest of the chain.
        """

        seen: set[tuple[str, str]] = set()
        for kind in ("spot", "swap"):
            for name in self._exchange_names:
                pair = self._pair_on(name, ref, kind)
                if pair and (name, pair) not in seen:
                    seen.add((name, pair))
                    yield name, pair

    def _fetch_ccxt(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
    ) -> list[OhlcvRow]:
        """Daily candles from whichever exchange in the chain covers best.

        Not simply "the first one that returns anything": exchanges keep
        delisted pairs in ``load_markets`` (Binance still lists XMR/USDT
        years after pulling it), so the first hit can be a stale stub that
        ends mid-window. Candidates are ranked by how far their history
        reaches, and the walk stops as soon as one spans the whole window.
        """

        target_end = min(end, dt.date.today())
        best: tuple[str, str, list[OhlcvRow]] | None = None
        best_score: tuple | None = None

        for name, pair in self._resolve_pairs(ref):
            rows = self._fetch_ccxt_one(name, pair, ref, start, end)
            if not rows:
                continue
            days = [r.day for r in rows]
            first_day, last_day = min(days), max(days)
            # Reaching further forward wins; then starting earlier; then
            # sheer bar count as a tie-break.
            score = (last_day, -first_day.toordinal(), len(rows))
            if best_score is None or score > best_score:
                best, best_score = (name, pair, rows), score
            spans_window = (
                first_day <= start + dt.timedelta(days=3)
                and last_day >= target_end - dt.timedelta(days=3)
            )
            if spans_window:
                break

        if best is None:
            return []
        name, pair, rows = best
        if name != self._exchange_name:
            log.info(
                "%s: %s bars from %s (%s), not on %s",
                ref.symbol, len(rows), name, pair, self._exchange_name,
            )
        # Only the primary exchange's pair belongs in the coin_map column,
        # which is Binance-specific by schema.
        if ref.cg_id and name == self._exchange_name:
            self.cache.upsert_coin_map(
                cg_id=ref.cg_id, symbol=ref.symbol, binance_symbol=pair
            )
        return rows

    def _fetch_ccxt_one(
        self,
        name: str,
        pair: str,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
    ) -> list[OhlcvRow]:
        ex = self._ensure_exchange(name)
        if ex is None:
            return []

        since_ms = int(
            dt.datetime.combine(start, dt.time.min).replace(tzinfo=dt.timezone.utc).timestamp() * 1000
        )
        end_ms = int(
            dt.datetime.combine(end, dt.time.max).replace(tzinfo=dt.timezone.utc).timestamp() * 1000
        )

        coin_key = coin_key_for(ref)
        out: list[OhlcvRow] = []
        cursor = since_ms
        first_page = True
        # Paginate (exchanges return up to ~1000 candles per call).
        for _ in range(50):
            try:
                batch = ex.fetch_ohlcv(pair, timeframe="1d", since=cursor, limit=1000)
            except Exception as exc:  # noqa: BLE001
                log.info("CCXT fetch_ohlcv error %s@%s %s: %s", pair, name, cursor, exc)
                break
            if not batch:
                # Binance clamps `since` to the listing date, but several
                # exchanges just return nothing when asked for candles that
                # predate it. Retry once without `since` to pick up whatever
                # history the coin does have.
                if first_page:
                    first_page = False
                    try:
                        batch = ex.fetch_ohlcv(pair, timeframe="1d", limit=1000)
                    except Exception as exc:  # noqa: BLE001
                        log.info("CCXT no-since retry failed %s@%s: %s", pair, name, exc)
                        break
                if not batch:
                    break
            first_page = False
            for ts_ms, o, h, l, c, v in batch:
                if ts_ms > end_ms or ts_ms < since_ms:
                    continue
                day = dt.datetime.utcfromtimestamp(ts_ms / 1000).date()
                out.append(
                    OhlcvRow(
                        coin_key=coin_key, day=day,
                        open=float(o) if o is not None else None,
                        high=float(h) if h is not None else None,
                        low=float(l) if l is not None else None,
                        close=float(c) if c is not None else None,
                        volume=float(v) if v is not None else None,
                        source=f"ccxt:{name}:{pair}",
                        approx=False,
                    )
                )
            last_ts = batch[-1][0]
            if last_ts >= end_ms or last_ts == cursor:
                break
            cursor = last_ts + 24 * 60 * 60 * 1000
        return out

    # ----------------------------- CoinGecko ---------------------------- #

    def _throttle_cg(self) -> None:
        gap = time.monotonic() - self._last_request_ts
        if gap < self.min_request_interval:
            time.sleep(self.min_request_interval - gap)
        self._last_request_ts = time.monotonic()

    def _coingecko_id(self, ref: CoinRef) -> str | None:
        """The CoinGecko id to fetch this ref with, resolving by ticker.

        ``cmc:``-prefixed values are CoinMarketCap slugs, not CoinGecko
        ids, so they are treated as absent.
        """

        if ref.cg_id and not ref.cg_id.startswith("cmc:"):
            return ref.cg_id
        return self._id_resolver.resolve(ref.symbol)

    def _fetch_coingecko(
        self,
        ref: CoinRef,
        start: dt.date,
        end: dt.date,
        cg_id: str | None = None,
    ) -> list[OhlcvRow]:
        # Rows stay keyed by `coin_key_for(ref)` even when the id was
        # resolved here, so the cache lookup in get_series still matches.
        cg_id = cg_id or ref.cg_id
        assert cg_id
        self._throttle_cg()
        # `interval=daily` is paid-tier since 2024 -> drop it; the endpoint
        # returns daily granularity on its own for ranges > 90 days. The
        # window counts back from *today* (not `end`), and one year is all
        # the public API will serve — beyond that the coin simply starts
        # later than the requested window.
        span_days = max((dt.date.today() - start).days + 2, 1)
        days = min(span_days, _CG_MAX_FREE_DAYS)
        if span_days > _CG_MAX_FREE_DAYS:
            log.info(
                "CoinGecko history for %s truncated to %s days (public API limit)",
                ref.symbol, _CG_MAX_FREE_DAYS,
            )
        params = {"vs_currency": "usd", "days": str(days)}
        url = _CG_CHART_URL.format(id=cg_id)

        # A 429 used to drop the coin outright, which is indistinguishable
        # from "this coin has no data" downstream. Back off and retry once.
        r = None
        for attempt in range(2):
            r = self._session.get(
                url, params=params, headers=_CG_HEADERS, timeout=20,
            )
            if r.status_code != 429:
                break
            wait = float(r.headers.get("Retry-After") or 8)
            if attempt == 0:
                log.info(
                    "CoinGecko rate-limited %s; retrying in %.0fs", ref.symbol, wait
                )
                time.sleep(wait)
            else:
                log.warning(
                    "CoinGecko still rate-limited for %s; giving up this run",
                    ref.symbol,
                )
                time.sleep(wait)
                return []
        assert r is not None
        r.raise_for_status()
        data = r.json()
        prices = data.get("prices") or []
        volumes = data.get("total_volumes") or []
        vol_lookup: dict[dt.date, float] = {}
        for ts_ms, vol in volumes:
            day = dt.datetime.utcfromtimestamp(ts_ms / 1000).date()
            try:
                vol_lookup[day] = float(vol)
            except (TypeError, ValueError):
                continue

        # Group prices by day (daily endpoint should already be daily,
        # but be defensive).
        per_day: dict[dt.date, list[float]] = {}
        for ts_ms, price in prices:
            day = dt.datetime.utcfromtimestamp(ts_ms / 1000).date()
            try:
                per_day.setdefault(day, []).append(float(price))
            except (TypeError, ValueError):
                continue

        coin_key = coin_key_for(ref)
        rows: list[OhlcvRow] = []
        prev_close: float | None = None
        for day in sorted(per_day):
            if day < start or day > end:
                # still emit close to satisfy "open = prev close" continuity
                last = per_day[day][-1]
                prev_close = last
                continue
            samples = per_day[day]
            close = samples[-1]
            high = max(samples)
            low = min(samples)
            open_ = prev_close if prev_close is not None else samples[0]
            rows.append(
                OhlcvRow(
                    coin_key=coin_key, day=day,
                    open=open_, high=high, low=low, close=close,
                    volume=vol_lookup.get(day),
                    source="coingecko", approx=True,
                )
            )
            prev_close = close
        return rows


# ------------------------------ helpers --------------------------------- #


def _business_days_between(start: dt.date, end: dt.date) -> list[dt.date]:
    """Daily crypto markets, so just all calendar days inclusive."""

    if end < start:
        return []
    out = []
    d = start
    while d <= end:
        out.append(d)
        d += dt.timedelta(days=1)
    return out


def _rows_to_frame(rows: list[OhlcvRow]) -> pd.DataFrame:
    if not rows:
        return pd.DataFrame(columns=["open", "high", "low", "close", "volume", "source", "approx"])
    df = pd.DataFrame(
        [
            {
                "day": r.day, "open": r.open, "high": r.high, "low": r.low,
                "close": r.close, "volume": r.volume, "source": r.source,
                "approx": r.approx,
            }
            for r in rows
        ]
    )
    df["day"] = pd.to_datetime(df["day"])
    df = df.sort_values("day").set_index("day")
    return df


def _bars_to_frame(bars: list[OhlcvBar]) -> pd.DataFrame:
    if not bars:
        return pd.DataFrame(columns=["open", "high", "low", "close", "volume", "source", "approx"])
    df = pd.DataFrame(
        [
            {
                "ts": b.ts, "open": b.open, "high": b.high, "low": b.low,
                "close": b.close, "volume": b.volume, "source": b.source,
                "approx": b.approx,
            }
            for b in bars
        ]
    )
    df["ts"] = pd.to_datetime(df["ts"])
    df = df.sort_values("ts").set_index("ts")
    return df
