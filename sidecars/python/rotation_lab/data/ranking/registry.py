"""Ranking-provider registry with switchable primary + auto fallback."""

from __future__ import annotations

import datetime as dt
import logging
from dataclasses import dataclass
from typing import Callable

from ...config import RankingSource
from ..cache import Cache, get_default_cache
from .base import RankedCoin, RankingProvider
from .cmc_snapshot import CmcSnapshotProvider
from .local_reconstruct import LocalReconstructProvider

log = logging.getLogger(__name__)


@dataclass
class ResolvedRanking:
    """Result of a ranking lookup, tagged with which provider answered."""

    provider: str
    coins: list[RankedCoin]


class RankingRegistry:
    """Holds the available providers and resolves universes by policy.

    When ``source == AUTO`` we try CMC first (faster, weekly snapshots
    are cheap) and fall back to local reconstruction on any failure or
    when CMC returns an empty list. The opposite when the user has
    explicitly picked one provider - we don't silently fall back unless
    they're on AUTO.
    """

    def __init__(
        self,
        cache: Cache | None = None,
        *,
        cmc: CmcSnapshotProvider | None = None,
        local: LocalReconstructProvider | None = None,
        progress: Callable[[str], None] | None = None,
    ) -> None:
        self.cache = cache or get_default_cache()
        self.cmc = cmc or CmcSnapshotProvider(self.cache)
        self.local = local or LocalReconstructProvider(self.cache, progress=progress)
        self.progress = progress

    def get(self, name: str) -> RankingProvider:
        if name == "cmc":
            return self.cmc
        if name == "local":
            return self.local
        raise KeyError(name)

    def get_top_n(
        self,
        on_date: dt.date,
        n: int,
        *,
        source: RankingSource = RankingSource.AUTO,
        exclude_stablecoins: bool = True,
        exclude_wrapped: bool = True,
        exclude_top_n: int = 0,
    ) -> ResolvedRanking:
        order = self._provider_order(source)
        errors: list[str] = []
        skip = max(int(exclude_top_n), 0)
        for prov in order:
            if self.progress:
                self.progress(f"{prov.name.upper()} coin list {on_date}")
            try:
                coins = prov.get_top_n(
                    on_date, n,
                    exclude_stablecoins=exclude_stablecoins,
                    exclude_wrapped=exclude_wrapped,
                )
            except Exception as exc:  # noqa: BLE001
                log.warning("Provider %s failed for %s: %s", prov.name, on_date, exc)
                errors.append(f"{prov.name}: {exc}")
                if source != RankingSource.AUTO:
                    raise
                continue
            if coins:
                # Drop the highest-ranked ``skip`` coins so the evaluated
                # cohort is ranks ``skip + 1 .. n`` (the small-cap slice).
                # ``n`` already counts the excluded coins.
                return ResolvedRanking(provider=prov.name, coins=coins[skip:])
            log.info("Provider %s returned empty for %s; trying fallback", prov.name, on_date)
            errors.append(f"{prov.name}: no ranking data")
        if errors:
            raise RuntimeError(
                f"Ranking unavailable for {on_date}: {'; '.join(errors)}"
            )
        return ResolvedRanking(provider=order[0].name if order else "", coins=[])

    # ------------------------------------------------------------------ #

    def _provider_order(self, source: RankingSource) -> list[RankingProvider]:
        if source == RankingSource.CMC:
            return [self.cmc]
        if source == RankingSource.LOCAL:
            return [self.local]
        return [self.cmc, self.local]
