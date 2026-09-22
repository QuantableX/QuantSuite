"""Time-indexed universe builder.

Walks the chosen cadence between ``start`` and ``end`` and resolves the
top-N for each rotation date. Returns a sparse "membership" DataFrame
that the engine forward-fills as needed.
"""

from __future__ import annotations

import datetime as dt
import logging
from dataclasses import dataclass

import pandas as pd

from ..config import Cadence, RankingSource
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
    progress=None,
) -> UniverseTimeline:
    """Resolve the top-N at every rotation date."""

    dates = cadence_dates(start, end, cadence)
    snapshots: list[UniverseSnapshot] = []
    for idx, d in enumerate(dates):
        if progress:
            progress(idx, len(dates), d)
        try:
            resolved = registry.get_top_n(
                d, top_n,
                source=source,
                exclude_stablecoins=exclude_stablecoins,
                exclude_wrapped=exclude_wrapped,
                exclude_top_n=exclude_top_n,
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
