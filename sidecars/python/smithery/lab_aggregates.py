"""The aggregate comparison: a variable-size search against the system's current aggregate.

Any size from two distinct base indicators upward is allowed; size is part of
the search, not a preference. The stages are fixed before results are seen,
the top candidates and the best of the small sizes are re-run natively, and the
leader and the current aggregate are compared from independent later starts.

Robust by default (docs/ROBUSTNESS.md, 2026-11-03; smithery.robust_search): only
indicators that pass the Indicator Gauntlet, each candidate scored by its worst
price world on a search window, the leaders confirmed on the blind years after it
against random aggregates, and the System Gauntlet on the finalists. Nothing found
on the whole history seeds a validated search.
"""
from __future__ import annotations

import time
from dataclasses import replace
from itertools import islice

from .lab import emit, save
from .lab_profit import DEFAULTS, NATIVE_BUDGET, NativeEvaluator, Subjects, default_keys, describe, distinct_keys

RESTARTS = ("2023-01-01", "2025-01-01")
BUDGET = 5_000_000


def run(path, doc):
    import datetime as dt
    from rotation_lab.rpc import _build_config
    from .lab_compare import frozen_engine, validate_request, within_saved_data
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
    # The user's saved aggregates evaluated like this system: ranked as they are, and searched around.
    seeds = [(s.get("name") or "", tuple(sorted(dict.fromkeys(s.get("members") or [])))) for s in doc.get("seeds") or []]
    seeds = [(n, m) for n, m in seeds if len(m) >= 2 and set(m) <= registry.keys()
             and len({registry[k]["base_key"] for k in m}) == len(m)]
    seed_keys = [k for _, m in seeds for k in m]
    # The best aggregates of earlier comparisons with the same settings: nothing found before is lost.
    earlier = [tuple(sorted(dict.fromkeys(m))) for m in doc.get("earlier") or []]
    earlier = [m for m in earlier if len(m) >= 2 and set(m) <= registry.keys()
               and len({registry[k]["base_key"] for k in m}) == len(m)]
    keys = list(dict.fromkeys([*(doc["indicators"] or default_keys(rows, config)), *reference, *seed_keys,
                               *(k for m in earlier for k in m)]))
    unknown = sorted(set(keys) - registry.keys())
    if unknown:
        raise ValueError(f"Unknown indicators: {unknown}")
    keys, duplicates = distinct_keys(keys, registry, config, keep=[*reference, *seed_keys])
    robust = bool(doc.get("robust"))
    gated, excluded = robust and doc.get("pool", "robust") == "robust", []
    if gated:
        from .lab_profit import TRACKS
        from .robust_search import gate_candidates
        from .evidence import EVALUATION_VERSION
        timeframe = config.single_asset.timeframe if config.mode == "single_asset" else config.cadence.value
        track = TRACKS.get(timeframe, "1d")
        recorded = (doc.get("pool_certified") or {}).get("keys")
        keys, searchable, excluded = gate_candidates(keys, reference, track, recorded)
        progress(f"{len(searchable)} of {len(searchable) + len(excluded)} indicators pass the Indicator Gauntlet "
                 f"on the {track} track ({EVALUATION_VERSION})")
        if len(searchable) < 2:
            raise ValueError(f"Only {len(searchable)} indicators pass the Indicator Gauntlet on the {track} track under "
                             f"the current evaluation ({EVALUATION_VERSION}). Re-certify the library in QuantScript's "
                             "forge first, or run with pool='all' for research (its results are marked not robust).")
    seeds = list({m: n for n, m in ((n, tuple(sorted({duplicates.get(k, k) for k in m}))) for n, m in seeds)}.items())
    saved_as = {m: n for m, n in seeds}
    earlier = list(dict.fromkeys(tuple(sorted({duplicates.get(k, k) for k in m})) for m in earlier))
    bases = {k: registry[k]["base_key"] for k in keys}
    if len({bases[k] for k in reference}) != len(reference):
        raise ValueError("The reference casts two votes for one base indicator")
    top, budget, seed = doc.get("top") or DEFAULTS["top"], doc.get("budget", BUDGET), doc.get("seed", DEFAULTS["seed"])
    started = time.monotonic()
    engine = frozen_engine(config, [(k,) for k in keys], path / "data")
    config = within_saved_data(config, engine, progress)
    reason = unsupported_reason(config)
    progress(f"{len(keys)} candidates from {len(set(bases.values()))} base indicators; reference {' + '.join(reference)}")
    if robust and reason:
        raise ValueError(f"The robust search needs the bulk evaluator: {reason}")
    evaluator = NativeEvaluator(config, keys, engine, progress) if reason else \
        FastSystem(config, keys, path / "cube", engine, progress=progress).build()
    if reason:
        budget = min(budget, NATIVE_BUDGET)
    worlds = cut = venue_path = None
    if robust:
        from .prices import venue_cache_path
        from .robust_search import Worlds, default_cut
        cut = default_cut(evaluator.dates, doc.get("blind_days"))
        venue_path = venue_cache_path(tuple(doc.get("venues") or ()))
        venue_path = venue_path if venue_path.is_file() else None
        progress(f"Search window {evaluator.dates[0].date()} → {evaluator.dates[cut - 1].date()}; "
                 f"blind window {evaluator.dates[cut].date()} → {evaluator.dates[-1].date()}")
        worlds = Worlds(config, keys, path, evaluator, venue_path, progress)
    try:
        if any(k in evaluator.failed for k in reference):
            raise ValueError("The reference cannot be evaluated: " + "; ".join(evaluator.failed[k] for k in reference if k in evaluator.failed))
        checks = [{"subject": "reference", **evaluator.verify(reference)}]
        if not checks[0]["passed"]:
            save(path / "verification.json", checks)
            raise ValueError("The bulk evaluator differs from the native engine; nothing was ranked")
        usable = [k for k in keys if k not in evaluator.failed]
        if gated:
            usable = [k for k in usable if k in set(searchable)]
        sizes = sorted(set(doc.get("sizes") or [])) or None
        n_bases = len({bases[k] for k in usable})
        max_size = min(max(sizes) if sizes else n_bases, n_bases)
        allowed = (lambda s: s in sizes) if sizes else (lambda s: 2 <= s <= max_size)
        plan = {"objective": ("worst price world's final capital on the search window, any aggregate size" if robust
                              else "historical final capital, any aggregate size"), "seed": seed, "budget": budget, "top": top,
                "reference": list(reference), "candidates": usable, "bases": n_bases, "sizes": sizes or [2, max_size],
                "saved": [{"name": n, "members": list(m)} for m, n in seeds],
                "stages": ["Control: the reference",
                           "Your saved aggregates with the same settings (all)",
                           "Neighbours of your saved aggregates: add, remove or swap one member (all)",
                           "Leaders of your earlier comparisons with the same settings (top 20 of each, all)",
                           "Reference neighbours: add, remove or swap one member (all)",
                           "Two additions to the reference (all if they fit 30% of the budget left, else a seeded sample)",
                           "Pairs (all if they fit 20% of the budget left, else a seeded sample)",
                           "Pairs: all of them",
                           "Growth by size: every one-indicator extension of the best 200 of the size before, from "
                           "triples on, while bigger sizes still improve (stops after 4 sizes without a better best)",
                           "Balanced samples of every size, more for smaller sizes (10% of the budget left)",
                           "Local search until stable: every neighbour (add, remove or swap one member) of the top 25, "
                           "the best of each size up to 16 and the saved systems, round after round, until a full round "
                           "leaves the top 25 unchanged (each is then a local optimum for single steps) or the budget ends"],
                "rule": "Two or more distinct base indicators, no preferred size; each base casts at most one vote.",
                "restarts": [d for d in (doc.get("restarts") or RESTARTS)],
                "validation": "Retrospective search on examined history; no claim of out-of-sample performance or a global optimum."}
        save(path / "plan.json", plan)
        evaluate = (lambda trials: worlds.evaluate(trials, cut)) if robust else evaluator.evaluate
        search = Search(evaluate, usable, {k: bases[k] for k in [*usable, *reference]}, seed=seed, budget=budget, progress=progress)
        search.run("Control: the reference", [reference])
        if seeds:
            search.run("Your saved aggregates", [m for m, _ in seeds])
            search.run("Neighbours of your saved aggregates",
                       [t for m, _ in seeds for t in search.neighbors(m) if allowed(len(t))])
        if earlier:
            search.run("Leaders of your earlier comparisons", [m for m in earlier if allowed(len(m))])
        if not gated:  # the reference was found on the whole history: a gated search never starts from it
            search.run("Reference neighbours", [t for t in search.neighbors(reference) if allowed(len(t))])
        ref_bases = {bases[k] for k in reference}
        additions = [k for k in usable if bases[k] not in ref_bases]
        if allowed(len(reference) + 2) and not gated:
            two = (tuple(sorted(reference + (a, b))) for i, a in enumerate(additions) for b in additions[i + 1:] if bases[a] != bases[b])
            cap = int(search.remaining() * 0.3)
            pool = list(islice(two, 2_000_000))
            search.run("Two additions to the reference", ordered(pool, f"{seed}|two")[:cap], cap=cap)
        if allowed(2):
            search.run("Pairs", search.all_of_size(2))
        if not sizes:
            # Size by size from the complete pairs: every extension of the best 200 of the size before.
            search.stage_grow("Growth, size", 3, 200, 4)
            progress(f"Growth by size done up to {search.grown} members")
        search.stage_balanced("Balanced sizes", [s for s in range(3, max_size + 1) if allowed(s)], 0.1)
        search.stage_until_stable("Local search until stable, round", 25,
                                  extra_seeds=[] if gated else [reference, *(m for m, _ in seeds), *earlier])
        progress("Local search " + ("stable: no single step improves the top 25" if search.stable
                                    else f"stopped at the budget after {search.rounds} rounds"))
        progress(f"Search done: {search.aggregates:,} aggregates in {time.monotonic() - started:,.0f} s")
        leaders = search.leaders(top)
        best = search.best_by_size()
        robust_block = None
        if robust:
            from .robust_search import finalists as run_finalists, walk_forward
            distinct, marks = [], set()
            for trial, value in search.leaders(400):
                mark = (round(value[0], 9), round(value[1], 9))
                if mark not in marks:
                    marks.add(mark)
                    distinct.append((trial, value))
                if len(distinct) == 50:
                    break
            wf = walk_forward(worlds, distinct, usable, bases, cut, seed, progress)
            fin = run_finalists(config, wf["leaders"], int(doc.get("gauntlet_top") or 3), path / "finalists",
                                venue_path, seed, progress)
            robust_block = {"policy": "docs/ROBUSTNESS.md", "worlds": worlds.names, "venue_measured": worlds.complete,
                            "pool_gated": gated, "pool_excluded": len(excluded), "pool": len(usable),
                            "walk_forward": wf, "finalists": fin,
                            "robust_finalists": [f["members"] for f in fin if f["verdict"]["robust"]]}
            save(path / "verdict.json", {"status": "robust" if robust_block["robust_finalists"] and gated else "not robust",
                                         "robust": bool(robust_block["robust_finalists"]) and gated,
                                         "robust_finalists": robust_block["robust_finalists"], "pool_gated": gated,
                                         "walk_forward_passed": wf["passed"], "finalists": len(fin)})
            progress(f"Walk-forward: {wf['passed']} of {len(wf['leaders'])} leaders beat random aggregates on the blind "
                     f"window; {len(robust_block['robust_finalists'])} of {len(fin)} finalists are robust")
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
                    "unsupported": dict(sorted(evaluator.failed.items())), "duplicates": duplicates, "aggregates": search.aggregates,
                    "by_size": {s: c for s, c in counts.items() if s >= 2}, "stages": search.stages, "sizes": plan["sizes"],
                    "budget": budget, "verified": len(verified) + 1, "verified_passed": sum(v["passed"] for v in verified) + 1,
                    "evaluator": "native" if reason else "bulk", "native_reason": reason,
                    "best_by_size": [{"size": s, "tested": counts.get(s, 0), "members": list(t), "net_return_multiplier": v[0],
                                      "max_drawdown_pct": v[1]} for s, (t, v) in best.items()],
                    "local_search": {"stable": search.stable, "rounds": search.rounds, "top": 25},
                    "growth": {"beam": 200, "up_to": getattr(search, "grown", None)},
                    "seconds": round(time.monotonic() - started)}
        for item in subjects.items:
            item["saved_as"] = saved_as.get(tuple(sorted(item["members"])))
        rows_out = [{"id": f"leader-{i}", "name": " + ".join(t), "members": list(t), "size": len(t), "saved_as": saved_as.get(t),
                     "metrics": {"net_return_multiplier": v[0], "max_drawdown_pct": v[1]}} for i, (t, v) in enumerate(search.leaders(100))]
        dates = [d.strftime("%Y-%m-%d") if (d.hour, d.minute) == (0, 0) else d.strftime("%Y-%m-%d %H:%M")
                 for d in native.equity_strategy.index]
        save(path / "results.json", {"kind": "aggregates", "format": 2, "system": describe(config, doc.get("system") or {}),
                                     "robust": robust_block,
                                     "reference": list(reference), "dates": dates, "assets": subjects.assets,
                                     "subjects": subjects.items, "rows": rows_out, "restarts": restarts,
                                     "coverage": coverage, "plan": plan,
                                     "validation": "Retrospective variable-size search in the saved system; not Gauntlet certification."})
        progress("Report ready")
    finally:
        if worlds is not None:
            worlds.close()
        evaluator.close()
