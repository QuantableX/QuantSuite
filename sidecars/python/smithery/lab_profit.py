"""The profit overview: every candidate alone and aggregates of 2+ distinct base
indicators in one saved system, ranked by final capital.

Candidates are evaluated in bulk by ``lab_fast`` (identical to the native
engine) after a native parity check; every candidate the report shows is
re-run natively and must hold the same assets on every bar. Systems the bulk
evaluator cannot express run natively within a smaller budget.
"""
from __future__ import annotations

import datetime as dt
import math
import time
from dataclasses import asdict, replace
from pathlib import Path

from .lab import emit, read, root, save

TRACKS = {"daily": "1d", "1d": "1d", "4h": "4h", "1h": "1h"}
DEFAULTS = {"top": 10, "budget": 50_000, "seed": 42}
NATIVE_BUDGET = 400


# ------------------------------------------------------------------ requests

def saved_systems():
    path = root().parents[1] / "systems/config.json"
    return read(path) if path.is_file() else {}


def resolve_system(ref):
    """A saved system's id, name and config, by id or name."""
    data = saved_systems()
    for system in data.get("systems", []):
        if ref in (system.get("id"), system.get("name")):
            config = (data.get("systemConfigs") or {}).get(system["id"])
            if config:
                return {"id": system["id"], "name": system.get("name") or system["id"]}, config
    raise ValueError(f"No saved system named '{ref}'")


def prepare(request):
    """The run fields of a system comparison: a resolved config with explicit dates."""
    config, system = request.get("config"), request.get("system")
    meta = None
    if system:
        meta, saved = resolve_system(system)
        config = config or saved
    if not isinstance(config, dict):
        raise ValueError("Name a saved system (system) or pass its config")
    config = {k: v for k, v in config.items() if k != "compareTrends"}
    if request.get("start_date"):
        config["startDate"] = request["start_date"]
    if request.get("end_date"):
        config["endDate"] = request["end_date"]
    yesterday = (dt.datetime.now(dt.timezone.utc).date() - dt.timedelta(days=1)).isoformat()
    if str(config.get("endDate") or "") >= yesterday or not config.get("endDate"):
        config["endDate"] = yesterday  # only closed history: the saved end is usually today
    return {"config": config, "system": meta or {"id": None, "name": request.get("system_name") or "Saved system"},
            **{k: request.get(k, DEFAULTS[k]) for k in DEFAULTS}, "sizes": request.get("sizes")}


def default_keys(rows, config):
    """Per base indicator: the version for the system's track, else Standard."""
    by_key = {r["key"]: r for r in rows}
    timeframe = config.single_asset.timeframe if config.mode == "single_asset" else config.cadence.value
    track = TRACKS.get(timeframe)
    keys = []
    for row in rows:
        if row.get("role") != "standard":
            continue
        version = (row.get("versions") or {}).get(f"optimized_{track}") if track else None
        keys.append(version if version in by_key else row["key"])
    return sorted(keys)


# ------------------------------------------------------------- evaluators

class NativeEvaluator:
    """The fallback for systems the bulk evaluator cannot express: one engine run per candidate."""
    single = False
    assets = None

    def __init__(self, config, keys, engine, progress):
        from .lab_fast import indicator_for
        self.config, self.keys, self.engine, self.progress = config, keys, engine, progress
        self.indicator_for = indicator_for
        self.failed, self.usable = {}, list(keys)
        self.cadence = config.bar_cadence if config.mode == "single_asset" else config.cadence
        self._cache = {}

    def config_for(self, members):
        return replace(self.config, indicator=self.indicator_for(self.config, members))

    def _run(self, members):
        members = tuple(members)
        if members not in self._cache:
            result = self.engine.run(self.config_for(members))
            self._cache[members] = None if result.equity_strategy.empty else result
        return self._cache[members]

    def evaluate(self, trials):
        out = []
        for trial in trials:
            try:
                result = self._run(trial)
            except Exception as exc:  # noqa: BLE001 — a failing candidate stays visible as unusable
                for key in trial if len(trial) == 1 else ():
                    self.failed.setdefault(key, f"error: {exc}")
                result = None
            out.append(None if result is None else (float(result.metrics_strategy.net_return_multiplier),
                                                    float(result.metrics_strategy.max_drawdown_pct)))
        return out

    def series(self, members):
        result = self._run(members)
        return result.equity_strategy, result.held_asset, result.metrics_strategy

    def verify(self, members):
        return {"members": list(members), "native": True, "passed": True, "bit_identical": True,
                "holding_mismatches": 0, "capital_relative_error": 0.0}

    def close(self):
        self._cache.clear()


# ---------------------------------------------------------------- report data

def describe(config, meta):
    """How the system works, in words, for the report."""
    from rotation_lab.config import TotalBreakoutConfig
    if config.mode == "single_asset":
        market = config.single_asset
        direction = {"long_cash": "long or cash", "long_short": "long or short"}.get(market.direction, market.direction)
        text = f"{market.pair} on {market.exchange}, {market.timeframe} bars, {direction}"
        logic = (f"Every candidate replaces only the trend signal of the saved system: {market.pair} is held "
                 f"{direction} by the signal of the previous bar, executed at the next open.")
        flt = None
    else:
        text = f"Top {config.top_n} coins{' + USD' if config.include_usd else ''} · {config.cadence.value} rotation"
        flt = None
        if config.market_filter:
            m = config.market_indicator
            flt = (f"TOTAL breakout filter {m.trend_length}/{m.entry_length}/{m.exit_length}" if isinstance(m, TotalBreakoutConfig)
                   else f"Market filter: {m.trend} on TOTAL")
        logic = ("Every candidate replaces only the trend signal of the saved system: the same historical "
                 f"top {config.top_n} ranking, pairwise coin/coin{' and coin/USD' if config.include_usd else ''} "
                 "comparisons, the same market filter and next-open execution.")
    return {"id": meta.get("id"), "name": meta.get("name"), "mode": config.mode, "start": config.start_date.isoformat(),
            "end": config.end_date.isoformat(), "description": text, "filter": flt, "logic": logic,
            "fee_rate": config.fee_rate, "slippage_rate": config.slippage_rate}


def annual(equity):
    """Calendar-year returns of one continuous curve, starting from the initial capital."""
    out, previous = {}, 1.0
    for year, values in equity.groupby(equity.index.year):
        end = float(values.iloc[-1])
        out[str(year)] = 100 * (end / previous - 1) if previous > 0 else None
        previous = end
    return out


class Subjects:
    """The candidates the report shows, with curves, holdings and verification."""

    def __init__(self, evaluator, registry, config):
        self.evaluator, self.registry, self.config = evaluator, registry, config
        self.single = config.mode == "single_asset"
        if self.single:
            base, quote = config.single_asset.pair.split("/")
            self.assets = [quote, base, f"Short {base}"]
        else:
            self.assets = list(evaluator.assets or [])
        self.items = []

    def index(self, label):
        if label is None or (isinstance(label, float) and math.isnan(label)):
            return -1
        label = str(label)
        if label not in self.assets:
            self.assets.append(label)
        return self.assets.index(label)

    def add(self, sid, group, code, rank, members, equity, held, metrics, verified):
        cash = {None, "USD", self.assets[0] if self.single else "USD"}
        labels = [None if v is None or (isinstance(v, float) and math.isnan(v)) else str(v) for v in held]
        params = self.config.indicator.params
        # The base indicator's name; the version (Standard, Optimized 1D…) is listed separately.
        rows = [self.registry.get(k, {}) for k in members]
        names = [self.registry.get(r.get("base_key"), r).get("name") or k for k, r in zip(members, rows)]
        versions = [((r.get("variant") or {}).get("label") or ("Standard" if r.get("role") == "standard" else r.get("role") or ""))
                    for r in rows]
        item = {"id": sid, "group": group, "code": code, "rank": rank, "members": list(members),
                "names": names, "versions": versions,
                "params": {k: {**(self.registry.get(k, {}).get("params") or {}), **(params.get(k) or {})} for k in members},
                "certification": {k: self.registry.get(k, {}).get("certification") for k in members},
                "metrics": {k: (None if v is None or not math.isfinite(v) else v) for k, v in asdict(metrics).items()},
                "equity": [float(v) for v in equity.to_numpy()],
                "held": [self.index(v) for v in labels],
                "annual": annual(equity),
                "cash_pct": 100 * sum(v in cash for v in labels) / max(1, len(labels)),
                "verified": verified}
        self.items.append(item)
        return item


# -------------------------------------------------------------------- worker

def run(path, doc):
    from rotation_lab.rpc import _build_config
    from .lab_compare import frozen_engine, validate_request
    from .lab_fast import FastSystem, unsupported_reason
    from .lab_search import Search
    from .registry import describe_all

    validate_request(doc)
    progress = lambda message, fraction=None: emit(path, "progress", message=message, fraction=fraction)
    config = replace(_build_config(doc["config"]), compare_trends=())
    rows = describe_all()["indicators"]
    registry = {r["key"]: r for r in rows}
    keys = doc["indicators"] or default_keys(rows, config)
    missing = sorted(set(keys) - registry.keys())
    if missing:
        raise ValueError(f"Unknown indicators: {missing}")
    bases = {k: registry[k]["base_key"] for k in keys}
    top, budget, seed = doc.get("top") or DEFAULTS["top"], doc.get("budget", DEFAULTS["budget"]), doc.get("seed", DEFAULTS["seed"])
    started = time.monotonic()
    engine = frozen_engine(config, [(k,) for k in keys], path / "data")
    reason = unsupported_reason(config)
    progress(f"{len(keys)} candidates from {len(set(bases.values()))} base indicators; "
             + ("native evaluation: " + reason if reason else "building the bulk evaluator"))
    evaluator = NativeEvaluator(config, keys, engine, progress) if reason else \
        FastSystem(config, keys, path / "cube", engine, progress=progress).build()
    if reason:
        budget = min(budget, NATIVE_BUDGET)
    try:
        # The current system, natively: the reference line and a parity check.
        native = engine.run(config)
        if native.equity_strategy.empty:
            raise ValueError("The saved system produced no equity curve: " + "; ".join(native.notes))
        baseline_members = tuple(dict.fromkeys(config.indicator.aggregate)) if config.indicator.trend == "aggregate" \
            else (config.indicator.trend,)
        checks = []
        expressible = not reason and all(k in evaluator.key_start and k not in evaluator.failed for k in baseline_members)
        if expressible:
            checks.append({"subject": "current system", **evaluator.verify(baseline_members)})
        usable = [k for k in keys if k not in evaluator.failed]
        if not usable:
            raise ValueError("No candidate can be evaluated in this system: " + "; ".join(sorted(set(evaluator.failed.values()))[:3]))
        if not reason:
            checks.append({"subject": "parity guard", **evaluator.verify((usable[0],))})
        if not all(c["passed"] for c in checks):
            save(path / "verification.json", checks)
            raise ValueError("The bulk evaluator differs from the native engine; nothing was ranked")
        sizes = sorted(set(doc.get("sizes") or [])) or None
        n_bases = len({bases[k] for k in usable})
        max_size = min(max(sizes) if sizes else n_bases, n_bases)
        allowed = (lambda s: s in sizes) if sizes else (lambda s: 2 <= s <= max_size)
        plan = {"objective": "historical final capital", "seed": seed, "budget": budget, "top": top,
                "candidates": usable, "bases": n_bases, "sizes": sizes or [2, max_size],
                "stages": ["Single indicators (all)", "Pairs (all if they fit 35% of the budget left, else a seeded sample)",
                           "Triples (all if they fit 40% of the budget left, else a seeded sample)",
                           "Larger aggregates (seeded samples, more for smaller sizes)",
                           "Local search: add, remove or swap one member of the leaders, 3 rounds"],
                "rule": "Each base indicator casts at most one vote; singles are ranked separately.",
                "validation": "Retrospective search on examined history; no claim of out-of-sample performance or a global optimum."}
        save(path / "plan.json", plan)
        search = Search(evaluator.evaluate, usable, {k: bases[k] for k in usable}, seed=seed, budget=budget, progress=progress)
        search.run("Single indicators", [(k,) for k in usable], singles=True)
        if allowed(2):
            search.stage_exhaustive_or_sample("Pairs", 2, 0.35)
        if allowed(3):
            search.stage_exhaustive_or_sample("Triples", 3, 0.4)
        search.stage_balanced("Larger aggregates", [s for s in range(4, max_size + 1) if allowed(s)], 0.4)
        search.stage_adaptive("Local search, round", 3, 1.0)
        progress(f"Search done: {len(search.seen):,} candidates in {time.monotonic() - started:,.0f} s")

        # The shown candidates: top aggregates and top singles (one version per base).
        aggregates = [t for t, _ in search.leaders(top)]
        single_rows = sorted(search.results(size=1), key=lambda r: (-r[1][0], r[0]))
        singles, used = [], set()
        for trial, _ in single_rows:
            if bases[trial[0]] not in used:
                used.add(bases[trial[0]])
                singles.append(trial)
            if len(singles) == top:
                break
        subjects = Subjects(evaluator, registry, config)
        subjects.add("current", "baseline", "Now", None, baseline_members, native.equity_strategy, native.held_asset,
                     native.metrics_strategy, {"native": True, "passed": True, "holding_mismatches": 0, "capital_relative_error": 0.0})
        verified = []
        for group, code, trials in (("aggregate", "A", aggregates), ("single", "S", singles)):
            for rank, trial in enumerate(trials, 1):
                progress(f"Verifying {code}{rank} natively")
                check = evaluator.verify(trial)
                verified.append({"subject": f"{code}{rank}", **{k: v for k, v in check.items() if k not in ("metrics", "fast_metrics")}})
                if not check["passed"]:
                    save(path / "verification.json", checks + verified)
                    raise ValueError(f"{code}{rank} differs from its native run; nothing was reported")
                equity, held, metrics = evaluator.series(trial)
                subjects.add(f"{group}{rank}", group, f"{code}{rank}", rank, trial, equity, held, metrics,
                             {k: check[k] for k in ("passed", "holding_mismatches", "capital_relative_error", "bit_identical") if k in check})
        save(path / "verification.json", checks + verified)
        leaders = [{"id": f"leader-{i}", "name": " + ".join(t), "members": list(t), "size": len(t),
                    "metrics": {"net_return_multiplier": v[0], "max_drawdown_pct": v[1]}}
                   for i, (t, v) in enumerate(search.leaders(100) + [(t, v) for t, v in single_rows[:100]])]
        dates = [d.strftime("%Y-%m-%d") if (d.hour, d.minute) == (0, 0) else d.strftime("%Y-%m-%d %H:%M")
                 for d in native.equity_strategy.index]
        coverage = {"keys": len(keys), "bases": len(set(bases.values())), "usable": len(usable),
                    "unsupported": dict(sorted(evaluator.failed.items())), "singles": len(search.results(size=1)),
                    "aggregates": search.aggregates, "by_size": search.counts(), "stages": search.stages,
                    "sizes": plan["sizes"], "budget": budget, "dedupe_bases": True,
                    "verified": len(verified), "verified_passed": sum(v["passed"] for v in verified),
                    "evaluator": "native" if reason else "bulk", "native_reason": reason,
                    "best_by_size": [{"size": s, "members": list(t), "net_return_multiplier": v[0], "max_drawdown_pct": v[1]}
                                     for s, (t, v) in search.best_by_size().items()],
                    "seconds": round(time.monotonic() - started),
                    "note": (f"Natively evaluated within {budget} aggregates: {reason}." if reason else None)}
        save(path / "results.json", {"kind": "profit", "format": 2, "system": describe(config, doc.get("system") or {}),
                                     "dates": dates, "assets": subjects.assets, "subjects": subjects.items,
                                     "rows": leaders, "coverage": coverage, "plan": plan,
                                     "validation": "Retrospective historical profit in the saved system; not Gauntlet certification."})
        progress(f"Report ready: {len(aggregates)} aggregates and {len(singles)} singles verified natively")
    finally:
        evaluator.close()
