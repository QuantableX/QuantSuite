"""Time-indexed universe builder.

Walks the chosen cadence between ``start`` and ``end`` and resolves the
top-N for each rotation date. Returns a sparse "membership" DataFrame
that the engine forward-fills as needed.
"""

from __future__ import annotations

import datetime as dt
import logging
from dataclasses import dataclass

import numpy as np
import pandas as pd

from ..config import Cadence, RankingResolution, RankingSource
from ..data.ranking.base import RankedCoin
from ..data.ranking.registry import RankingRegistry

log = logging.getLogger(__name__)


@dataclass
class UniverseSnapshot:
    on_date: dt.date
    provider: str
    coins: list[RankedCoin]


@dataclass
class UniverseTimeline:
    snapshots: list[UniverseSnapshot]

    def as_membership_frame(self) -> pd.DataFrame:
        """Return a wide frame ``index=date, columns=symbol, values=rank``.

        Used by the GUI's universe browser and for sanity checking.
        """

        rows = []
        for s in self.snapshots:
            for c in s.coins:
                rows.append(
                    {
                        "date": pd.Timestamp(s.on_date),
                        "symbol": c.symbol,
                        "cg_id": c.cg_id,
                        "rank": c.rank,
                        "name": c.name,
                        "market_cap": c.market_cap,
                    }
                )
        if not rows:
            return pd.DataFrame()
        return pd.DataFrame(rows)

    def unique_coins(self) -> list[RankedCoin]:
        seen: dict[tuple[str | None, str], RankedCoin] = {}
        for snap in self.snapshots:
            for c in snap.coins:
                key = (c.cg_id, c.symbol)
                if key not in seen:
                    seen[key] = c
        return list(seen.values())


def cadence_dates(start: dt.date, end: dt.date, cadence: Cadence) -> list[dt.date]:
    """Generate the universe-refresh dates between ``start`` and ``end``.

    Market-cap rankings only exist at daily (or coarser) granularity, so the
    intraday cadences refresh the universe **daily** and the engine
    forward-fills that membership across the intraday bars within each day.
    """

    if end < start:
        return []
    freq = "D" if cadence.is_intraday else cadence.pandas_freq
    rng = pd.date_range(pd.Timestamp(start), pd.Timestamp(end), freq=freq)
    if rng.size == 0:
        return [start]
    out = [d.date() for d in rng]
    # Always include the start so the very first universe is well-defined
    if out[0] > start:
        out.insert(0, start)
    if out[-1] < end:
        out.append(end)
    return sorted(set(out))


def daily_snapshot_days(start: dt.date, end: dt.date, cadence: Cadence) -> list[dt.date]:
    """The CMC days a daily-resolution run over ``[start, end]`` reads."""

    from ..data.ranking.cmc_snapshot import latest_complete_day, snapshot_date

    newest = latest_complete_day()
    return sorted({min(snapshot_date(d, RankingResolution.DAILY), newest)
                   for d in cadence_dates(start, end, cadence)})


def build_universe_timeline(
    registry: RankingRegistry,
    *,
    start: dt.date,
    end: dt.date,
    cadence: Cadence,
    top_n: int,
    source: RankingSource = RankingSource.AUTO,
    exclude_stablecoins: bool = True,
    exclude_wrapped: bool = True,
    exclude_top_n: int = 0,
    resolution: RankingResolution = RankingResolution.WEEKLY,
    progress=None,
    exclude_symbols: tuple[str, ...] = (),
) -> UniverseTimeline:
    """Resolve the top-N at every rotation date.

    A day's universe steers bars from that day's 00:00 UTC. Intraday bars
    close before the day's own snapshot exists, so in weekly mode they read
    the ranking as of the previous day (a Sunday's bars take the Sunday
    before); daily snapshots are already the previous day's.
    """

    dates = cadence_dates(start, end, cadence)
    lag = (dt.timedelta(days=1)
           if cadence.is_intraday and resolution == RankingResolution.WEEKLY
           else dt.timedelta(0))
    snapshots: list[UniverseSnapshot] = []
    for idx, d in enumerate(dates):
        if progress:
            progress(idx, len(dates), d)
        try:
            resolved = registry.get_top_n(
                d - lag, top_n,
                source=source,
                exclude_stablecoins=exclude_stablecoins,
                exclude_wrapped=exclude_wrapped,
                exclude_top_n=exclude_top_n,
                resolution=resolution,
                exclude_symbols=exclude_symbols,
            )
        except Exception as exc:  # noqa: BLE001
            log.warning("Universe lookup failed for %s: %s", d, exc)
            # Missing rankings are not a cash signal. Continuing both retries
            # the same outage for every date and distorts the backtest metrics.
            raise RuntimeError(f"Cannot build the coin list for {d}: {exc}") from exc
        if not resolved.coins:
            raise RuntimeError(f"No coins resolved for {d}; check the ranking source and exclusions.")
        snapshots.append(
            UniverseSnapshot(on_date=d, provider=resolved.provider, coins=resolved.coins)
        )
        if progress:
            progress(idx + 1, len(dates), d)
    return UniverseTimeline(snapshots=snapshots)


def ranking_known_before_open(cadence: Cadence, resolution: RankingResolution) -> bool:
    """Whether a day's snapshot is complete before that day's first bar opens.

    A daily snapshot is the previous day's (``snapshot_date``), and the intraday
    bars of a weekly-ranked run read the day before (the lag in
    ``build_universe_timeline``). Only daily bars on weekly snapshots read a
    Sunday's own snapshot on that Sunday; there a decision cannot look one
    bar ahead without reading the future."""

    return resolution == RankingResolution.DAILY or cadence.is_intraday


def bar_step(cadence: Cadence) -> pd.Timedelta:
    """The time from one bar's open to the next: the cadence itself intraday, a day otherwise."""

    return pd.Timedelta(cadence.value) if cadence.is_intraday else pd.Timedelta(days=1)


def candle_mask(frames: dict[str, pd.DataFrame], index: pd.DatetimeIndex) -> dict[str, np.ndarray]:
    """Per coin, which bars of ``index`` it has a candle on."""

    return {sym: np.asarray(index.isin(frame.index)) for sym, frame in frames.items()}


def comparable(symbols: list[str], present: dict[str, np.ndarray], bar: int) -> list[str]:
    """``symbols`` (rank order) that have a candle on bar ``bar``.

    A coin without one cannot be judged at that close: it is left out of the
    comparison, it gives nobody a vote and cannot be picked. The backtest
    walk, the bulk evaluators and today's live signal all take their coins
    through here."""

    return [s for s in symbols if s in present and present[s][bar]]


def walk_lists(
    snapshots: list[UniverseSnapshot],
    index: pd.DatetimeIndex,
    frames: dict[str, pd.DataFrame],
    *,
    ahead: bool,
    step: pd.Timedelta,
) -> tuple[list[list[str]], list[list[str]]]:
    """The two coin lists of every bar of a walk, each in rank order.

    ``today[i]``: the coins of the latest snapshot dated on or before bar
    ``i`` — what may be held on it (a holding outside it is exited).

    ``compare[i]``: the coins a decision at the close of bar ``i`` chooses
    among. With ``ahead`` they come from the snapshot that steers the next
    bar (``step`` later): that ranking is complete before the next bar opens
    (``ranking_known_before_open``) and it is the one today's live signal
    reads after the close, so the walk picks what the bot picks. Without
    ``ahead`` they come from the bar's own snapshot. Either way only coins
    with a candle on bar ``i`` (``comparable``).

    Past the last snapshot the newest one stays in force, so the decision
    at the last bar of a run that ends in the past reads its own day's
    ranking; that decision is never traded."""

    stamps = pd.DatetimeIndex([pd.Timestamp(s.on_date) for s in snapshots])
    if index.tz is not None:
        stamps = stamps.tz_localize(index.tz)
    coins = [[c.symbol for c in s.coins] for s in snapshots]
    active = stamps.searchsorted(index, side="right") - 1
    decide = stamps.searchsorted(index + step, side="right") - 1 if ahead else active
    present = candle_mask(frames, index)
    today = [coins[k] if k >= 0 else [] for k in active]
    compare = [comparable(coins[k], present, i) if k >= 0 else [] for i, k in enumerate(decide)]
    return today, compare


def run_ends(lists: list[list[str]]) -> list[int]:
    """For every bar, the first later bar whose list differs (the end of its run of equal lists)."""

    n = len(lists)
    ends = [n] * n
    for i in range(n - 2, -1, -1):
        if lists[i] == lists[i + 1]:
            ends[i] = ends[i + 1]
        else:
            ends[i] = i + 1
    return ends


def market_warmup_snapshots(
    registry: RankingRegistry,
    *,
    warm_start: dt.date,
    start: dt.date,
    cadence: Cadence,
    top_n: int,
    source: RankingSource = RankingSource.AUTO,
    exclude_stablecoins: bool = True,
    exclude_wrapped: bool = True,
    resolution: RankingResolution = RankingResolution.WEEKLY,
) -> list[UniverseSnapshot]:
    """TOTAL's coin list before a run's ``start``: one snapshot per day
    from ``warm_start``, read from the ranking cache alone (never the
    network). A day the cache lacks gets no snapshot, so it keeps the
    one before (``market_index``). Without this, every bar before the
    start took the start's own weights, and TOTAL there depended on when
    a run began; now a day gets the same snapshot whatever the start, as
    far as the cache reaches. The whole top-N, like the market list
    (no excluded head, no excluded symbols)."""

    lookup = getattr(registry, "cached_top_n", None)
    if lookup is None or warm_start >= start:
        return []
    from ..data.ranking.registry import ResolvedRanking
    lag = (dt.timedelta(days=1)
           if cadence.is_intraday and resolution == RankingResolution.WEEKLY
           else dt.timedelta(0))
    out: list[UniverseSnapshot] = []
    day = warm_start
    while day < start:
        resolved = lookup(day - lag, top_n, source=source, exclude_stablecoins=exclude_stablecoins,
                          exclude_wrapped=exclude_wrapped, resolution=resolution)
        if isinstance(resolved, ResolvedRanking) and resolved.coins:
            out.append(UniverseSnapshot(on_date=day, provider=resolved.provider, coins=resolved.coins))
        day += dt.timedelta(days=1)
    return out
