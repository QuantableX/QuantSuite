"""The aggregate comparison: a variable-size search against the system's current aggregate.

Any size from two distinct base indicators upward is allowed; size is part of
the search, not a preference. The stages are fixed before results are seen,
the top candidates and the best of the small sizes are re-run natively, and the
leader and the current aggregate are compared from independent later starts.
"""
from __future__ import annotations

import time
from dataclasses import replace
from itertools import islice

from .lab import emit, save
from .lab_profit import DEFAULTS, NATIVE_BUDGET, NativeEvaluator, Subjects, default_keys, describe

RESTARTS = ("2023-01-01", "2025-01-01")
BUDGET = 200_000


def run(path, doc):
    import datetime as dt
    from rotation_lab.rpc import _build_config
    from .lab_compare import frozen_engine, validate_request
    from .lab_fast import FastSystem, unsupported_reason
    from .lab_search import Search, ordered
    from .registry import describe_all

    validate_request(doc)
    progress = lambda message, fraction=None: emit(path, "progress", message=message, fraction=fraction)
    config = replace(_build_config(doc["config"]), compare_trends=())
    rows = describe_all()["indicators"]
    registry = {r["key"]: r for r in rows}
    saved = tuple(dict.fromkeys(config.indicator.aggregate)) if config.indicator.trend == "aggregate" else (config.indicator.trend,)
    reference = tuple(sorted(dict.fromkeys(doc.get("reference") or saved)))
    missing = sorted(set(reference) - registry.keys())
    if missing:
        raise ValueError(f"The reference contains indicators outside the library: {missing}")
    keys = sorted(set(doc["indicators"] or default_keys(rows, config)) | set(reference))
    unknown = sorted(set(keys) - registry.keys())
    if unknown:
        raise ValueError(f"Unknown indicators: {unknown}")
    bases = {k: registry[k]["base_key"] for k in keys}
    if len({bases[k] for k in reference}) != len(reference):
        raise ValueError("The reference casts two votes for one base indicator")
    top, budget, seed = doc.get("top") or DEFAULTS["top"], doc.get("budget", BUDGET), doc.get("seed", DEFAULTS["seed"])
    started = time.monotonic()
    engine = frozen_engine(config, [(k,) for k in keys], path / "data")
    reason = unsupported_reason(config)
    progress(f"{len(keys)} candidates from {len(set(bases.values()))} base indicators; reference {' + '.join(reference)}")
    evaluator = NativeEvaluator(config, keys, engine, progress) if reason else \
        FastSystem(config, keys, path / "cube", engine, progress=progress).build()
    if reason:
        budget = min(budget, NATIVE_BUDGET)
    try:
        if any(k in evaluator.failed for k in reference):
            raise ValueError("The reference cannot be evaluated: " + "; ".join(evaluator.failed[k] for k in reference if k in evaluator.failed))
        checks = [{"subject": "reference", **evaluator.verify(reference)}]
        if not checks[0]["passed"]:
            save(path / "verification.json", checks)
            raise ValueError("The bulk evaluator differs from the native engine; nothing was ranked")
        usable = [k for k in keys if k not in evaluator.failed]
        sizes = sorted(set(doc.get("sizes") or [])) or None
        n_bases = len({bases[k] for k in usable})
        max_size = min(max(sizes) if sizes else n_bases, n_bases)
        allowed = (lambda s: s in sizes) if sizes else (lambda s: 2 <= s <= max_size)
        plan = {"objective": "historical final capital, any aggregate size", "seed": seed, "budget": budget, "top": top,
                "reference": list(reference), "candidates": usable, "bases": n_bases, "sizes": sizes or [2, max_size],
                "stages": ["Control: the reference", "Reference neighbours: add, remove or swap one member (all)",
                           "Two additions to the reference (all if they fit 30% of the budget left, else a seeded sample)",
                           "Pairs (all if they fit 20% of the budget left, else a seeded sample)",
                           "Balanced samples of every size, more for smaller sizes",
                           "Local search: neighbours of the leaders and of the best of each size, 4 rounds"],
                "rule": "Two or more distinct base indicators, no preferred size; each base casts at most one vote.",
                "restarts": [d for d in (doc.get("restarts") or RESTARTS)],
                "validation": "Retrospective search on examined history; no claim of out-of-sample performance or a global optimum."}
        save(path / "plan.json", plan)
        search = Search(evaluator.evaluate, usable, {k: bases[k] for k in usable}, seed=seed, budget=budget, progress=progress)
        search.run("Control: the reference", [reference])
        search.run("Reference neighbours", [t for t in search.neighbors(reference) if allowed(len(t))])
        ref_bases = {bases[k] for k in reference}
        additions = [k for k in usable if bases[k] not in ref_bases]
        if allowed(len(reference) + 2):
            two = (tuple(sorted(reference + (a, b))) for i, a in enumerate(additions) for b in additions[i + 1:] if bases[a] != bases[b])
            cap = int(search.remaining() * 0.3)
            pool = list(islice(two, 2_000_000))
            search.run("Two additions to the reference", ordered(pool, f"{seed}|two")[:cap], cap=cap)
        if allowed(2):
            search.stage_exhaustive_or_sample("Pairs", 2, 0.2)
        search.stage_balanced("Balanced sizes", [s for s in range(3, max_size + 1) if allowed(s)], 0.45)
        search.stage_adaptive("Local search, round", 4, 1.0, extra_seeds=[reference])
        progress(f"Search done: {search.aggregates:,} aggregates in {time.monotonic() - started:,.0f} s")
        leaders = search.leaders(top)
        best = search.best_by_size()
        verify = [t for t, _ in leaders] + [best[s][0] for s in sorted(best) if s <= 5]
        subjects = Subjects(evaluator, registry, config)
        native = engine.run(evaluator.config_for(reference))
        subjects.add("reference", "reference", "Now", None, reference, native.equity_strategy, native.held_asset,
                     native.metrics_strategy, {"native": True, **{k: checks[0][k] for k in ("passed", "holding_mismatches", "capital_relative_error")}})
        verified = []
        for rank, trial in enumerate(dict.fromkeys(verify), 1):
            label = f"#{rank}" if trial in {t for t, _ in leaders} else f"size {len(trial)}"
            progress(f"Verifying {label} natively")
            check = evaluator.verify(trial)
            verified.append({"subject": label, **{k: v for k, v in check.items() if k not in ("metrics", "fast_metrics")}})
            if not check["passed"]:
                save(path / "verification.json", checks + verified)
                raise ValueError(f"{label} differs from its native run; nothing was reported")
            in_top = trial in {t for t, _ in leaders}
            equity, held, metrics = evaluator.series(trial)
            subjects.add(f"top{rank}" if in_top else f"size{len(trial)}", "aggregate" if in_top else "size_best",
                         f"#{[t for t, _ in leaders].index(trial) + 1}" if in_top else f"B{len(trial)}",
                         [t for t, _ in leaders].index(trial) + 1 if in_top else None, trial, equity, held, metrics,
                         {k: check[k] for k in ("passed", "holding_mismatches", "capital_relative_error", "bit_identical") if k in check})
        save(path / "verification.json", checks + verified)
        # Independent restarts: the engine starts again at each date (not a cropped curve).
        restarts = []
        lead = leaders[0][0] if leaders else reference
        for day in plan["restarts"]:
            start = dt.date.fromisoformat(day)
            if not config.start_date < start < config.end_date:
                continue
            progress(f"Independent restart from {day}")
            row = {"start": day}
            for name, members in (("leader", lead), ("reference", reference)):
                result = engine.run(replace(evaluator.config_for(members), start_date=start))
                m = result.metrics_strategy
                row[name] = {"net_return_multiplier": m.net_return_multiplier, "max_drawdown_pct": m.max_drawdown_pct, "cagr_pct": m.cagr_pct}
            restarts.append(row)
        counts = search.counts()
        coverage = {"keys": len(keys), "bases": len(set(bases.values())), "usable": len(usable),
                    "unsupported": dict(sorted(evaluator.failed.items())), "aggregates": search.aggregates,
                    "by_size": {s: c for s, c in counts.items() if s >= 2}, "stages": search.stages, "sizes": plan["sizes"],
                    "budget": budget, "verified": len(verified) + 1, "verified_passed": sum(v["passed"] for v in verified) + 1,
                    "evaluator": "native" if reason else "bulk", "native_reason": reason,
                    "best_by_size": [{"size": s, "tested": counts.get(s, 0), "members": list(t), "net_return_multiplier": v[0],
                                      "max_drawdown_pct": v[1]} for s, (t, v) in best.items()],
                    "seconds": round(time.monotonic() - started)}
        rows_out = [{"id": f"leader-{i}", "name": " + ".join(t), "members": list(t), "size": len(t),
                     "metrics": {"net_return_multiplier": v[0], "max_drawdown_pct": v[1]}} for i, (t, v) in enumerate(search.leaders(100))]
        dates = [d.strftime("%Y-%m-%d") if (d.hour, d.minute) == (0, 0) else d.strftime("%Y-%m-%d %H:%M")
                 for d in native.equity_strategy.index]
        save(path / "results.json", {"kind": "aggregates", "format": 2, "system": describe(config, doc.get("system") or {}),
                                     "reference": list(reference), "dates": dates, "assets": subjects.assets,
                                     "subjects": subjects.items, "rows": rows_out, "restarts": restarts,
                                     "coverage": coverage, "plan": plan,
                                     "validation": "Retrospective variable-size search in the saved system; not Gauntlet certification."})
        progress("Report ready")
    finally:
        evaluator.close()
