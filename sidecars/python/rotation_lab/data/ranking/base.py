"""Abstract ranking provider interface.

A :class:`RankingProvider` returns a list of :class:`RankedCoin` for any
historical date - typically already filtered down to the desired top-N
after exclusion of stablecoins/wrapped tokens.
"""

from __future__ import annotations

import datetime as dt
from abc import ABC, abstractmethod
from dataclasses import dataclass


@dataclass(frozen=True)
class RankedCoin:
    rank: int
    cg_id: str | None
    symbol: str
    name: str | None
    market_cap: float | None
    price: float | None


class RankingProvider(ABC):
    """Abstract base for any historical top-N source."""

    #: Stable short name persisted in the cache (``provider`` column).
    name: str = "base"

    @abstractmethod
    def get_top_n(
        self,
        on_date: dt.date,
        n: int,
        *,
        exclude_stablecoins: bool = True,
        exclude_wrapped: bool = True,
    ) -> list[RankedCoin]:
        """Return up to ``n`` coins ranked at ``on_date``.

        Implementations should consult their local cache first and only
        hit the network when needed. The returned list must be sorted
        ascending by rank.
        """

    def available_dates(self) -> list[dt.date]:
        """Optional list of snapshot dates the provider currently knows
        about. Used by the GUI's "Historical Universe" tab. Default: []."""

        return []
