"""Gauntlet evidence as one report: a library's existing verdicts or a research run's.

Nothing is re-tested here. Each base indicator shows its Standard verdicts and,
per track, the version selected for that track (the optimized role file), with
the five native axes, permutation p-values, walk-forward and parameters.
"""
from __future__ import annotations

import json
import math
from pathlib import Path

TRACKS = ("1h", "4h", "1d")
AXES = ("asset", "exchange", "parameter", "temporal", "monte_carlo")
# The native report fields worth keeping as evidence (each report is ~1 KB).
REPORT_FIELDS = ("version", "run_id", "kind", "name", "params", "timeframe", "primary", "primary_sha256",
                 "code_sha256", "indicator_sha256", "seed", "counts", "score", "grade", "scores", "perm_p",
                 "certified", "reasons", "fast", "note")


def finite(value):
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) else None


def trimmed(report):
    return {k: report[k] for k in REPORT_FIELDS if k in report}


def vault_reports(output_dir):
    """Look native reports up by name in a vault's Output folder."""
    output_dir, cache = Path(output_dir), {}

    def lookup(name):
        if not name or Path(name).name != name:
            return None
        if name not in cache:
            path = output_dir / (name + ".json")
            try:
                cache[name] = trimmed(json.loads(path.read_text(encoding="utf-8-sig"))) if path.is_file() else None
            except (OSError, ValueError):
                cache[name] = None
        return cache[name]
    return lookup


def verdict(entry, lookup, reports):
    """One track's verdict with its native axes; None when the track was never run."""
    if not entry:
        return None
    name = entry.get("report")
    native = lookup(name) if name else None
    if native is not None:
        reports[name] = native
    scores = (native or {}).get("scores") or {}
    axes = {a: finite(scores.get(a)) for a in AXES}
    return {"score": finite(entry.get("score")), "grade": entry.get("grade"), "perm_p": finite(entry.get("perm_p")),
            "certified": bool(entry.get("certified")), "report": name, "date": entry.get("date"),
            "evaluation_version": entry.get("evaluation_version") or (native or {}).get("version"),
            "axes": axes, "complete": native is not None and all(v is not None for v in axes.values()),
            "reasons": list((native or {}).get("reasons") or [])}


def overall(per_track):
    """The worst track decides: lowest score, highest p, certified only on every track."""
    values = [per_track.get(tf) for tf in TRACKS]
    if any(v is None or v["score"] is None for v in values):
        return {"score": None, "perm_p": None, "certified": False, "complete": False, "worst": None}
    worst = min(TRACKS, key=lambda tf: per_track[tf]["score"])
    ps = [v["perm_p"] for v in values]
    return {"score": per_track[worst]["score"], "perm_p": max(ps) if all(p is not None for p in ps) else None,
            "certified": all(v["certified"] for v in values), "complete": all(v["complete"] for v in values),
            "worst": worst}


def build(rows, lookup, keys=None, errors=None):
    """Report rows for every Standard indicator (optionally only ``keys``)."""
    by_key = {r["key"]: r for r in rows}
    wanted = None if keys is None else set(keys)
    reports, out = {}, []
    for base in rows:
        if base.get("role") != "standard" or (wanted is not None and base["key"] not in wanted):
            continue
        versions = base.get("versions") or {}
        standard = {tf: verdict((base.get("timeframes") or {}).get(tf), lookup, reports) for tf in TRACKS}
        selected, wf, decision, params = {}, {}, {}, {"standard": base.get("params") or {}}
        for tf in TRACKS:
            version = by_key.get(versions.get("optimized_" + tf) or "")
            evidence = ((version or {}).get("variant") or {}).get("evidence") or {}
            entry = ((version or {}).get("timeframes") or {}).get(tf) or (evidence.get("timeframes") or {}).get(tf)
            if version is None or entry is None:
                # No track version: the Standard verdict is what this track has.
                selected[tf], wf[tf], decision[tf], params[tf] = standard[tf], None, None, params["standard"]
                continue
            selected[tf] = verdict(entry, lookup, reports)
            walk = evidence.get("walkforward") or None
            wf[tf] = None if walk is None else {k: walk.get(k) for k in
                                                ("folds", "wfe", "oos_sharpe", "is_sharpe_mean", "series", "objective", "final_choice")}
            decision[tf] = evidence.get("decision")
            params[tf] = version.get("params") or {}
        out.append({"key": base["key"], "name": base.get("name") or base["key"], "hypothesis": base.get("hypothesis") or "",
                    "script": base.get("script"), "warmup": base.get("warmup_bars"),
                    "versions": {"standard": base["key"], **{tf: versions.get("optimized_" + tf) for tf in TRACKS}},
                    "standard": standard, "selected": selected,
                    "overall": {"standard": overall(standard), "selected": overall(selected)},
                    "wf": wf, "decision": decision, "params": params,
                    "error": (errors or {}).get(base["key"])})
    # Requested keys without a registry row stay visible as errors, never as zeros.
    for key in sorted((wanted or set()) - {r["key"] for r in out}):
        empty = {tf: None for tf in TRACKS}
        out.append({"key": key, "name": key, "hypothesis": "", "script": None, "warmup": None,
                    "versions": {"standard": key}, "standard": empty, "selected": dict(empty),
                    "overall": {"standard": overall(empty), "selected": overall(empty)},
                    "wf": dict(empty), "decision": dict(empty), "params": {},
                    "error": (errors or {}).get(key) or "Not registered in this library"})
    out.sort(key=lambda r: r["key"])
    return {"rows": out, "reports": reports, "summary": summarize(out, reports)}


def summarize(rows, reports):
    counts = {}
    for role in ("standard", "selected"):
        counts[role] = {"all": sum(r["overall"][role]["certified"] for r in rows),
                        **{tf: sum(bool((r[role][tf] or {}).get("certified")) for r in rows) for tf in TRACKS}}
    dates = sorted(n[:10] for n in reports if len(n) >= 10 and n[:4].isdigit())
    incomplete = sorted({v["report"] for r in rows for role in ("standard", "selected") for tf in TRACKS
                         for v in [r[role][tf]] if v and v["report"] and not v["complete"]})
    versions = sorted({(reports[n] or {}).get("version") for n in reports} - {None})
    protocol = sorted({json.dumps({"seed": rep.get("seed"), "counts": rep.get("counts"), "fast": rep.get("fast")}, sort_keys=True)
                       for rep in reports.values()})
    best = max((r for r in rows if r["overall"]["selected"]["score"] is not None),
               key=lambda r: (r["overall"]["selected"]["score"], r["key"]), default=None)
    return {"indicators": len(rows), "counts": counts, "reports": len(reports),
            "complete_reports": sum(all(finite((rep.get("scores") or {}).get(a)) is not None for a in AXES)
                                    for rep in reports.values()),
            "incomplete_reports": incomplete, "report_dates": [dates[0], dates[-1]] if dates else None,
            "evaluation_versions": versions, "protocols": [json.loads(p) for p in protocol],
            "missing_walkforward": sum(r["wf"][tf] is None for r in rows for tf in TRACKS if r["versions"].get(tf)),
            "track_versions": sum(1 for r in rows for tf in TRACKS if r["versions"].get(tf)),
            "errors": sum(bool(r["error"]) for r in rows),
            "best": None if best is None else {"key": best["key"], "name": best["name"],
                                               "score": best["overall"]["selected"]["score"]}}


def from_run(results):
    """A research run's Gauntlet results (registry rows with embedded native evidence)."""
    rows = results.get("rows", [])
    evidence = {}
    for row in rows:
        for tf, entry in (row.get("timeframes") or {}).items():
            native = (row.get("evidence") or {}).get(tf)
            if entry and entry.get("report") and native:
                evidence[entry["report"]] = trimmed(native)
    keys = sorted({r["base_key"] for r in rows if r.get("base_key")})
    built = build(rows, evidence.get, keys or None, errors=results.get("errors"))
    return {"kind": "gauntlet", "format": 2, "source": "run", **built, "protocol": results.get("protocol")}


def overview(registry, output_dir, keys=None):
    """The library's existing verdicts; ``registry`` is ``smithery.registry --json``."""
    built = build(registry["indicators"], vault_reports(output_dir), keys)
    return {"kind": "gauntlet_overview", "format": 2, "source": "library", **built,
            "library": {"generated_at": registry.get("generated_at"), "contract_version": registry.get("contract_version"),
                        "engine_version": registry.get("version")}}
