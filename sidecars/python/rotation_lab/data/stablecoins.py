"""Stablecoin and wrapped-token exclusion lists.

Maintained statically (so the tool works offline) but augmented from
CoinGecko's ``category=stablecoin`` / ``category=wrapped-tokens``
listings when the network is available. The merged set is cached.
"""

from __future__ import annotations

import logging
from typing import Iterable

import requests

log = logging.getLogger(__name__)

# Canonical curated set. Symbols are uppercase; we also keep CoinGecko ids
# because symbols collide (e.g. several "USDT" forks).
CURATED_STABLECOIN_SYMBOLS: frozenset[str] = frozenset(
    {
        "USDT", "USDC", "DAI", "BUSD", "TUSD", "USDP", "FRAX", "LUSD",
        "GUSD", "USDD", "SUSD", "USDE", "FDUSD", "PYUSD", "USDS", "USDJ",
        "USTC", "UST", "EURS", "EURT", "EURC", "MIM", "CUSD", "RSV",
        "USDN", "VAI", "OUSD", "USDX", "HUSD", "USDK", "USDH", "USDB",
        "USDG", "USDF", "USDR", "USDL", "USDV", "USDY", "USDC.E",
    }
)

CURATED_STABLECOIN_CG_IDS: frozenset[str] = frozenset(
    {
        "tether", "usd-coin", "dai", "binance-usd", "true-usd",
        "paxos-standard", "frax", "liquity-usd", "gemini-dollar",
        "usdd", "nusd", "ethena-usde", "first-digital-usd", "paypal-usd",
        "terrausd-wormhole", "magic-internet-money", "celo-dollar",
        "reserve", "neutrino", "vai", "origin-dollar", "usdx",
        "husd", "usdk", "stasis-eurs", "tether-eurt", "euro-coin",
        "usds", "blackrock-usd-institutional-digital-liquidity-fund",
    }
)

CURATED_WRAPPED_SYMBOLS: frozenset[str] = frozenset(
    {
        "WBTC", "WETH", "STETH", "WSTETH", "RETH", "CBBTC", "CBETH",
        "TBTC", "RENBTC", "WBNB", "WMATIC", "WAVAX", "WSOL", "WPOL",
        "HBTC", "BTCB", "JITOSOL", "MSOL", "BSOL", "WEETH", "WEEHT",
        "WBETH", "ANKRETH", "RETH2", "OSETH", "FRXETH", "SFRXETH",
        "LSETH", "SWETH", "PUFETH",
    }
)

CURATED_WRAPPED_CG_IDS: frozenset[str] = frozenset(
    {
        "wrapped-bitcoin", "weth", "staked-ether", "wrapped-steth",
        "rocket-pool-eth", "coinbase-wrapped-btc", "coinbase-wrapped-staked-eth",
        "tbtc", "renbtc", "wbnb", "wmatic", "wrapped-avax", "wrapped-solana",
        "binance-bitcoin", "binance-peg-bitcoin", "jito-staked-sol",
        "msol", "blazestake-staked-sol", "wrapped-eeth", "wrapped-beacon-eth",
        "ankreth", "stakewise-v3-oseth", "frax-ether", "staked-frax-ether",
        "liquid-staked-ethereum", "sweth", "puffer-finance-pufeth",
    }
)


_CG_CATEGORY_URL = "https://api.coingecko.com/api/v3/coins/markets"


def fetch_coingecko_category(category: str, *, timeout: float = 15.0) -> list[dict]:
    """Fetch all coins from a CoinGecko market category (paginated).

    Returns a list of raw market dicts (or empty on failure). Failures
    are logged but never raised so the GUI can keep running offline.
    """

    out: list[dict] = []
    page = 1
    while True:
        params = {
            "vs_currency": "usd",
            "category": category,
            "order": "market_cap_desc",
            "per_page": 250,
            "page": page,
        }
        try:
            r = requests.get(_CG_CATEGORY_URL, params=params, timeout=timeout)
            r.raise_for_status()
            batch = r.json()
        except (requests.RequestException, ValueError) as exc:
            log.warning("CoinGecko category fetch failed (%s page %d): %s",
                        category, page, exc)
            break
        if not isinstance(batch, list) or not batch:
            break
        out.extend(batch)
        if len(batch) < 250:
            break
        page += 1
        if page > 20:
            break
    return out


class ExclusionList:
    """Holds the set of symbols/ids to filter out of the universe."""

    def __init__(self, symbols: Iterable[str], cg_ids: Iterable[str]) -> None:
        self._symbols = {s.upper() for s in symbols}
        self._cg_ids = {i.lower() for i in cg_ids}

    def contains_symbol(self, symbol: str | None) -> bool:
        return bool(symbol) and symbol.upper() in self._symbols

    def contains_cg_id(self, cg_id: str | None) -> bool:
        return bool(cg_id) and cg_id.lower() in self._cg_ids

    def matches(self, *, symbol: str | None = None, cg_id: str | None = None) -> bool:
        return self.contains_symbol(symbol) or self.contains_cg_id(cg_id)

    def merge(self, *, symbols: Iterable[str] = (), cg_ids: Iterable[str] = ()) -> "ExclusionList":
        return ExclusionList(self._symbols | {s.upper() for s in symbols},
                             self._cg_ids | {i.lower() for i in cg_ids})

    def __len__(self) -> int:  # pragma: no cover - trivial
        return len(self._symbols) + len(self._cg_ids)


def build_default_stablecoin_list(*, sync_online: bool = False) -> ExclusionList:
    base = ExclusionList(CURATED_STABLECOIN_SYMBOLS, CURATED_STABLECOIN_CG_IDS)
    if sync_online:
        rows = fetch_coingecko_category("stablecoins")
        base = base.merge(
            symbols=(r.get("symbol", "") for r in rows),
            cg_ids=(r.get("id", "") for r in rows),
        )
    return base


def build_default_wrapped_list(*, sync_online: bool = False) -> ExclusionList:
    base = ExclusionList(CURATED_WRAPPED_SYMBOLS, CURATED_WRAPPED_CG_IDS)
    if sync_online:
        # CoinGecko has separate categories; we try a few common slugs and
        # tolerate failures because not every slug exists on every CG version.
        for slug in ("wrapped-tokens", "liquid-staking-tokens"):
            rows = fetch_coingecko_category(slug)
            base = base.merge(
                symbols=(r.get("symbol", "") for r in rows),
                cg_ids=(r.get("id", "") for r in rows),
            )
    return base
