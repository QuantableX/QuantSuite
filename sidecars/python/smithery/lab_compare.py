"""System comparisons: request validation and frozen, identical inputs for every candidate."""
from __future__ import annotations

import copy
import datetime as dt
import math
import os
from dataclasses import asdict, replace
from pathlib import Path

from .lab import save


def validate_request(doc):
    config = doc.get("config")
    if not isinstance(config, dict) or not all(k in config for k in ("startDate", "endDate", "feeRate", "slippageRate", "indicator")):
        raise ValueError("A system comparison requires a system config with startDate, endDate, feeRate, slippageRate and baseline indicator")
    start, end = dt.date.fromisoformat(config["startDate"]), dt.date.fromisoformat(config["endDate"])
    if start >= end or end >= dt.datetime.now(dt.timezone.utc).date():
        raise ValueError("Use an explicit historical date range ending before today")
    if any(not isinstance(config[k], (int, float)) or not math.isfinite(config[k]) or not 0 <= config[k] < 1
           for k in ("feeRate", "slippageRate")):
        raise ValueError("Costs must be finite fractions between zero and one")
    for field, low, high in (("top", 1, 50), ("budget", 0, 5_000_000), ("seed", 0, 2**31 - 1)):
        value = doc.get(field)
        if value is not None and (type(value) is not int or not low <= value <= high):
            raise ValueError(f"{field} must be a whole number from {low} to {high}")
    sizes = doc.get("sizes")
    if sizes is not None and (not isinstance(sizes, list) or not sizes or any(type(s) is not int or s < 2 for s in sizes)):
        raise ValueError("Aggregate sizes must be whole numbers of at least two")
    reference = doc.get("reference")
    if reference is not None and (not isinstance(reference, list) or not reference
                                  or any(not isinstance(k, str) or not k for k in reference)):
        raise ValueError("The reference must list indicator keys")
    restarts = doc.get("restarts")
    if restarts is not None:
        if not isinstance(restarts, list) or len(restarts) > 6:
            raise ValueError("Restarts must be a list of at most six dates")
        for day in restarts:
            dt.date.fromisoformat(day)


class FrozenService:
    """Reuse byte-identical provider answers across all candidate backtests."""
    def __init__(self, service, names, path, earliest=None):
        self.service, self.names, self.path, self.earliest = service, names, path, earliest
        self.values, self.failures = {}, []

    def __getattr__(self, name):
        target = getattr(self.service, name)
        if name not in self.names:
            return target

        def call(*args, **kwargs):
            args = list(args)
            if self.earliest is not None:
                # Both OHLCV methods take start as positional argument, after
                # ref (get_series) or exchange/pair (get_pair_series).
                index = 1 if name == "get_series" else 2
                if len(args) > index:
                    args[index] = self.earliest
                else:
                    kwargs["start"] = self.earliest
            key = repr((name, args, sorted(kwargs.items())))
            if key not in self.values:
                try:
                    self.values[key] = copy.deepcopy(target(*args, **kwargs))
                except Exception as exc:
                    self.failures.append(str(exc))
                    raise
                number = len(self.values)
                value = self.values[key]
                if hasattr(value, "frame"):
                    self.path.mkdir(parents=True, exist_ok=True)
                    value.frame.to_csv(self.path / f"candles-{number}.csv", float_format="%.17g")
                else:
                    save(self.path / f"ranking-{number}.json", {"request": key, "response": asdict(value)})
            return copy.deepcopy(self.values[key])
        return call


def finite(value):
    return float(value) if value is not None and math.isfinite(float(value)) else None


# ------------------------------------------------------------ saved data
# System comparisons run on the user's saved price data: the copy of the
# systems price cache taken when the run starts (ROTATION_LAB_CACHE in a run).
# Nothing is downloaded and nothing is rewritten. The normal reader would
# re-download a window the saved data does not fully cover (a long warm-up
# reaches back before the first saved day) from whichever exchange spans it,
# and overwrite the saved candles with another exchange's prices.

def bar_window(start, end, timeframe):
    """The first and last bar opening of an inclusive date range."""
    from rotation_lab.data.ohlcv import _TF_MS
    last = dt.datetime.combine(end + dt.timedelta(days=1), dt.time.min) - dt.timedelta(milliseconds=_TF_MS[timeframe])
    return dt.datetime.combine(start, dt.time.min), last


class SavedPrices:
    """Candles from the saved data only; history before the first saved day is absent, as it is there."""

    def __init__(self, cache):
        self.cache = cache

    def get_series(self, ref, start, end, timeframe="1d"):
        from rotation_lab.data.ohlcv import OhlcvSeries, _bars_to_frame, _rows_to_frame, coin_key_for
        key = coin_key_for(ref)
        if timeframe == "1d":
            return OhlcvSeries(coin_ref=ref, frame=_rows_to_frame(self.cache.get_ohlcv_range(key, start, end)))
        bars = self.cache.get_ohlcv_intraday_range(key, timeframe, *bar_window(start, end, timeframe))
        return OhlcvSeries(coin_ref=ref, frame=_bars_to_frame(bars))

    def get_pair_series(self, exchange, pair, start, end, timeframe="1d"):
        from rotation_lab.data.ohlcv import CoinRef, OhlcvSeries, _bars_to_frame
        ref = CoinRef(None, pair, cache_key=f"market:{exchange}:{pair}")
        bars = self.cache.get_ohlcv_intraday_range(ref.cache_key, timeframe, *bar_window(start, end, timeframe))
        return OhlcvSeries(ref, _bars_to_frame(bars))


class NotSaved:
    """Stands in for rankings rebuilt from downloads: only saved snapshots count."""
    name = "saved data"

    def get_top_n(self, on_date, n, **_):
        raise ValueError(f"no saved ranking for {on_date}")


def saved_rankings(cache):
    """The ranking registry over the saved CoinMarketCap snapshots only."""
    from rotation_lab.data.ranking.cmc_snapshot import CmcSnapshotProvider
    from rotation_lab.data.ranking.registry import RankingRegistry

    class SavedSnapshots(CmcSnapshotProvider):
        def _ensure_snapshot(self, snap, resolution=None):
            rows = self.cache.get_ranking(self.name, snap)
            if not rows:
                raise ValueError(f"no saved ranking for {snap}")
            return rows

    return RankingRegistry(cache, cmc=SavedSnapshots(cache), local=NotSaved())


def offline():
    """No research worker downloads anything: a missed path fails loudly instead of fetching."""
    import requests

    def refuse(*_, **__):
        raise RuntimeError("Research runs use the saved price data only; nothing is downloaded")
    requests.sessions.Session.request = refuse


def saved_cache():
    from rotation_lab.data.cache import Cache
    path = Path(os.environ.get("ROTATION_LAB_CACHE", ""))
    if not path.is_file():
        raise ValueError("No saved price data. Run a backtest in QuantSystems first, then start the comparison again.")
    return Cache(path)


def covered(config, registry, prices, day):
    """Whether the saved data holds the system's ranking and every ranked coin's candle on a day.

    Coins without any candle in the week before have no saved prices at all and are
    skipped by the engine, so they do not count as missing."""
    from rotation_lab.data.ohlcv import CoinRef
    if config.mode == "single_asset":
        a = config.single_asset
        return not prices.get_pair_series(a.exchange, a.pair, day, day, a.timeframe).frame.empty
    try:
        coins = registry.get_top_n(day, config.top_n, source=config.ranking_source,
                                   exclude_stablecoins=config.exclude_stablecoins, exclude_wrapped=config.exclude_wrapped,
                                   exclude_top_n=config.exclude_top_n, resolution=config.ranking_resolution).coins
    except Exception:
        return False
    timeframe = config.cadence.value if config.cadence.is_intraday else "1d"
    for coin in coins:
        ref = CoinRef(cg_id=coin.cg_id, symbol=coin.symbol)
        if prices.get_series(ref, day, day, timeframe).frame.empty and \
                not prices.get_series(ref, day - dt.timedelta(days=7), day, timeframe).frame.empty:
            return False
    return bool(coins)


def within_saved_data(config, engine, progress=None):
    """The config ending on the last day the saved data covers (at most its own end)."""
    registry, prices = engine.registry.service, engine.ohlcv.service
    day = config.end_date
    while day > config.start_date and day >= config.end_date - dt.timedelta(days=62):
        if covered(config, registry, prices, day):
            if day != config.end_date and progress:
                progress(f"The saved price data ends on {day.isoformat()}; the comparison ends there")
            return replace(config, end_date=day)
        day -= dt.timedelta(days=1)
    raise ValueError(f"The saved price data does not reach {config.end_date.isoformat()} or the two months before it. "
                     "Run the system in QuantSystems to update its prices, then start the comparison again.")


def frozen_engine(config, candidates, data_dir):
    """A BacktestEngine on the saved price data whose answers are frozen on first use.

    Every OHLCV request starts at the earliest warm-up any candidate (or the
    system itself, or its market filter) needs, so all runs share one copy."""
    from rotation_lab.backtest.engine import BacktestEngine, _fetch_start
    from rotation_lab.config import IndicatorConfig
    from .lab_fast import indicator_for
    timing = replace(config, cadence=config.bar_cadence) if config.mode == "single_asset" else config
    starts = [_fetch_start(timing)]
    starts += [_fetch_start(replace(timing, indicator=indicator_for(config, members))) for members in candidates]
    if config.market_filter and isinstance(config.market_indicator, IndicatorConfig):
        starts.append(_fetch_start(replace(timing, indicator=config.market_indicator)))
    offline()
    cache = saved_cache()
    engine = BacktestEngine(registry=saved_rankings(cache), ohlcv=SavedPrices(cache))
    engine.registry = FrozenService(engine.registry, {"get_top_n"}, data_dir)
    engine.ohlcv = FrozenService(engine.ohlcv, {"get_series", "get_pair_series"}, data_dir, min(starts))
    return engine
