"""Bounded variable-size research through the unchanged native system engine."""
from __future__ import annotations

import copy
import datetime as dt
import itertools
import math
import random
from dataclasses import asdict, replace

from .lab import emit, save


def validate_request(doc):
    config = doc.get("config")
    if not isinstance(config, dict) or not all(k in config for k in ("startDate", "endDate", "feeRate", "slippageRate", "indicator")):
        raise ValueError("Profit comparison requires a system config with startDate, endDate, feeRate, slippageRate and baseline indicator")
    start, end = dt.date.fromisoformat(config["startDate"]), dt.date.fromisoformat(config["endDate"])
    if start >= end or end >= dt.datetime.now(dt.timezone.utc).date():
        raise ValueError("Use an explicit historical date range ending before today")
    if any(not isinstance(config[k], (int, float)) or not math.isfinite(config[k]) or not 0 <= config[k] < 1
           for k in ("feeRate", "slippageRate")):
        raise ValueError("Costs must be finite fractions between zero and one")
    samples = doc.get("samples_per_size", 20)
    if type(samples) is not int or not 1 <= samples <= 1000:
        raise ValueError("samples_per_size must be 1..1000")
    sizes = doc.get("sizes")
    if sizes is not None and (not isinstance(sizes, list) or any(type(s) is not int or s < 2 for s in sizes)):
        raise ValueError("Aggregate sizes must be integers of at least two")
    if not doc["indicators"]:
        raise ValueError("Select candidate keys for a profit comparison")


def combinations(keys, bases, sizes, samples, seed=42):
    """No cloned-base votes. Enumerate small spaces; seed finite larger samples."""
    groups = {}
    for key in sorted(set(keys)):
        groups.setdefault(bases[key], []).append(key)
    sizes = sorted(set(sizes if sizes is not None else range(2, len(groups) + 1)))
    if any(s > len(groups) for s in sizes):
        raise ValueError("An aggregate cannot contain more members than distinct base indicators")
    if len(keys) + len(sizes) * samples > 5000:
        raise ValueError("Comparison exceeds 5000 candidates; reduce samples_per_size or choose fewer sizes")
    rng = random.Random(seed)
    result = [(k,) for k in keys]
    coverage = []
    for size in sizes:
        # Enumerating all variant combinations can be enormous. A bounded,
        # uniform base-group proposal with explicit coverage is reproducible.
        possible = math.comb(len(keys), size)
        selected = set()
        if possible <= samples:
            selected = {c for c in itertools.combinations(sorted(keys), size) if len({bases[k] for k in c}) == size}
        else:
            for _ in range(samples * 30):
                choice = tuple(sorted(rng.choice(groups[b]) for b in rng.sample(sorted(groups), size)))
                selected.add(choice)
                if len(selected) >= samples:
                    break
        result.extend(sorted(selected))
        coverage.append({"size": size, "tested": len(selected), "exhaustive": possible <= samples})
    return result, coverage


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
                    self.path.mkdir(exist_ok=True)
                    value.frame.to_csv(self.path / f"candles-{number}.csv", float_format="%.17g")
                else:
                    save(self.path / f"ranking-{number}.json", {"request": key, "response": asdict(value)})
            return copy.deepcopy(self.values[key])
        return call


def finite(value):
    return float(value) if value is not None and math.isfinite(float(value)) else None


def compare(path, doc):
    from rotation_lab.rpc import _build_config
    from rotation_lab.backtest.engine import BacktestEngine, _fetch_start
    from rotation_lab.config import IndicatorConfig
    from .registry import describe_all
    from .lab import digest

    validate_request(doc)
    registry = {r["key"]: r for r in describe_all()["indicators"]}
    missing = set(doc["indicators"]) - registry.keys()
    if missing:
        raise ValueError(f"Unknown indicators: {sorted(missing)}")
    bases = {k: registry[k]["base_key"] for k in doc["indicators"]}
    candidates, coverage = combinations(doc["indicators"], bases, doc.get("sizes"), doc["samples_per_size"])
    base = replace(_build_config(doc["config"]), compare_trends=())
    configs = [("baseline", (), base)]
    for i, members in enumerate(candidates):
        indicator = IndicatorConfig(trend=members[0] if len(members) == 1 else "aggregate",
                                    aggregate=members if len(members) > 1 else (), params=base.indicator.params)
        configs.append((f"candidate-{i}", members, replace(base, indicator=indicator)))
    # Freeze the plan before any performance is seen.
    save(path / "selection-plan.json", {"objective": "historical end capital", "seed": 42,
         "candidates": [{"id": key, "members": members} for key, members, _ in configs], "coverage": coverage,
         "validation": "Retrospective search; no claim of pristine out-of-sample or global optimum"})
    earliest = min(_fetch_start(cfg) for _, _, cfg in configs)
    if base.market_filter and isinstance(base.market_indicator, IndicatorConfig):
        earliest = min(earliest, _fetch_start(replace(base, indicator=base.market_indicator)))
    engine = BacktestEngine()
    engine.registry = FrozenService(engine.registry, {"get_top_n"}, path / "data")
    engine.ohlcv = FrozenService(engine.ohlcv, {"get_series", "get_pair_series"}, path / "data", earliest)
    rows = []
    result_doc = {"kind": "profit", "rows": rows, "coverage": coverage, "config": doc["config"],
                  "validation": "Retrospective historical profit; not Gauntlet certification"}
    for index, (key, members, cfg) in enumerate(configs):
        emit(path, "progress", message=f"System backtest {index + 1}/{len(configs)}", members=members)
        try:
            result = engine.run(cfg)
            if engine.ohlcv.failures or engine.registry.failures:
                raise ValueError("Market input failed; comparison cannot rank unequal data coverage")
            if result.equity_strategy.empty or result.metrics_strategy is None:
                raise ValueError("No primary system curve: " + "; ".join(result.notes))
            metrics = {k: finite(v) for k, v in asdict(result.metrics_strategy).items()}
            row = {"id": key, "name": "Saved system baseline" if key == "baseline" else " + ".join(members),
                   "members": members, "size": len(members), "metrics": metrics,
                   "equity": [[str(t), finite(v)] for t, v in result.equity_strategy.items()],
                   "parameters": {k: {**registry[k]["params"], **cfg.indicator.params.get(k, {})} for k in members},
                   "certification": {k: registry[k]["certification"] for k in members}, "notes": result.notes}
        except Exception as exc:
            row = {"id": key, "name": " + ".join(members) or "Baseline", "members": members,
                   "size": len(members), "error": str(exc)}
        rows.append(row)
        save(path / "results.json", result_doc)
        if engine.ohlcv.failures or engine.registry.failures or (key == "baseline" and row.get("error")):
            raise ValueError(row["error"])
    rows.sort(key=lambda r: (r.get("metrics", {}).get("net_return_multiplier") or -math.inf), reverse=True)
    save(path / "results.json", result_doc)
    save(path / "data-hashes.json", {f.name: digest(f) for f in (path / "data").glob("*") if f.is_file()})
