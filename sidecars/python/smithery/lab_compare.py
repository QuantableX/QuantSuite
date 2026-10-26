"""System comparisons: request validation and frozen, identical inputs for every candidate."""
from __future__ import annotations

import copy
import datetime as dt
import math
from dataclasses import asdict, replace

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


def frozen_engine(config, candidates, data_dir):
    """A BacktestEngine whose ranking and candle answers are frozen on first use.

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
    engine = BacktestEngine()
    engine.registry = FrozenService(engine.registry, {"get_top_n"}, data_dir)
    engine.ohlcv = FrozenService(engine.ohlcv, {"get_series", "get_pair_series"}, data_dir, min(starts))
    return engine
