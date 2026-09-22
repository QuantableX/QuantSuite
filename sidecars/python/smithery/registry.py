"""The forge's registry as data: every indicator with its hypothesis,
defaults, parameter space, warm-up and certification — what QuantAlgo's
Indicators page and QuantSystems' indicator list show.

    python -m smithery.registry --json        # the whole registry
    python -m smithery.registry hilbert       # one indicator, human-readable

``CERTIFICATION`` preserves the historical ledger as labeled references.
The latest full run with matching evaluator code and default parameters
supersedes that reference, including failed runs. Reading evidence never
changes a strategy, system configuration or indicator default.
"""
from __future__ import annotations

import datetime as dt
import inspect
import json
import re
import sys
from functools import lru_cache
from pathlib import Path

from .indicators import REGISTRY, WARMUP_BARS

# The luck gate and the score floor of Docs/01: both must hold.
CERTIFY_SCORE = 70.0
CERTIFY_PERM_P = 0.10

# Research scores belong to the private library, never the installer.
from .workspace import library_metadata
_metadata = library_metadata()
CERTIFICATION: dict[str, dict] = _metadata.get("certification", {})
CERTIFICATION_TF: dict[str, dict[str, dict]] = _metadata.get("certification_tf", {})

from .robustness import TIMEFRAMES


@lru_cache(maxsize=2)
def _current_runs(historical: bool = False) -> dict[str, dict[str, dict]]:
    """Latest complete evaluation attempt for the exact current code/params.

    A newer failed full run supersedes an older pass. Smoke tests never do.
    Historical reference scores remain available when no matching current
    artifact exists; they are explicitly marked as historical in the API.
    Registry queries run in a fresh sidecar process, so the cache is per query.
    """
    from .data import OUTPUT_DIR
    from .evidence import code_fingerprint, EVALUATION_VERSION
    fingerprint = code_fingerprint()
    found = {}
    if not OUTPUT_DIR.is_dir():
        return found
    names = {cls.name: key for key, cls in REGISTRY.items()}
    for path in sorted(OUTPUT_DIR.glob("*.json"), key=lambda p: p.stat().st_mtime_ns):
        try:
            run = json.loads(path.read_text(encoding="utf-8"))
            key = names.get(run.get("name"))
            tf = run.get("timeframe")
            if (run.get("kind") != "gauntlet" or run.get("fast") or not key or tf not in TIMEFRAMES
                    or (not historical and (run.get("version") != EVALUATION_VERSION or run.get("code_sha256") != fingerprint))
                    or json.dumps(run.get("params"), sort_keys=True) != json.dumps(REGISTRY[key]().params, sort_keys=True)):
                continue
            found.setdefault(tf, {})[key] = {
                "score": run["score"], "grade": run["grade"].split()[0], "perm_p": run["perm_p"],
                "date": path.name[:10], "report": path.stem, "certified": run["certified"],
                "reasons": run.get("reasons", []), "source": "historical" if historical else "current_run",
            }
        except (OSError, ValueError, KeyError, TypeError):
            continue
    return found


def is_certified(verdict: dict | None) -> bool:
    if not verdict:
        return False
    if verdict.get("certified") is False:
        return False
    p = verdict.get("perm_p")
    return float(verdict.get("score", 0)) >= CERTIFY_SCORE and p is not None and float(p) <= CERTIFY_PERM_P


def certification_table(timeframe: str = "1d") -> dict[str, dict]:
    if timeframe not in TIMEFRAMES:
        raise KeyError(f"unknown certification track '{timeframe}' — one of {TIMEFRAMES}")
    historical = CERTIFICATION if timeframe == "1d" else CERTIFICATION_TF.get(timeframe, {})
    return {**{k: {**v, "source": "historical"} for k, v in historical.items()},
            **_current_runs(historical=True).get(timeframe, {}),
            **_current_runs().get(timeframe, {})}


def verdict_for(key: str, timeframe: str = "1d") -> dict | None:
    verdict = certification_table(timeframe).get(key)
    return {**verdict, "certified": is_certified(verdict), "timeframe": timeframe} if verdict else None


def grade_letter(score: float, perm_p: float | None) -> str:
    """Docs/01's grade bands; a permutation p above the gate caps S/A at B."""
    g = "S" if score >= 85 else "A" if score >= 70 else "B" if score >= 55 else "C" if score >= 40 else "F"
    if perm_p is not None and perm_p > CERTIFY_PERM_P and g in ("S", "A"):
        g = "B"
    return g


def overall_verdict(key: str) -> dict | None:
    """THE score of an indicator — one number, earned on every track (user,
    2026-09-08: "robust enough for every timeframe"). It is the worst track:
    the score is the minimum over the 1d / 4h / 1h / 1m verdicts, the permutation
    p the worst of them, the grade follows from those, and the indicator is
    certified only when all configured tracks certified it. A track not yet run
    counts as not certified — an edge that has not been measured on minute
    bars has not been shown to hold there."""
    per = {tf: certification_table(tf).get(key) for tf in TIMEFRAMES}
    have = {tf: v for tf, v in per.items() if v}
    if not have:
        return None
    worst_tf = min(have, key=lambda tf: (float(have[tf]["score"]), tf))
    score = float(have[worst_tf]["score"])
    ps = [float(v["perm_p"]) for v in have.values() if v.get("perm_p") is not None]
    worst_p = max(ps) if ps else None
    all_tracks = len(have) == len(TIMEFRAMES)
    return {
        "score": int(round(score)),
        "grade": grade_letter(score, worst_p),
        "perm_p": worst_p,
        "date": max(v["date"] for v in have.values()),
        "report": have[worst_tf]["report"],
        "certified": all_tracks and all(is_certified(v) for v in have.values()),
        "capped": worst_p is not None and worst_p > CERTIFY_PERM_P,
        "tracks_run": len(have),
        "tracks": len(TIMEFRAMES),
        "worst_track": worst_tf,
        "timeframe": "all",
        "source": "current_run" if all(v.get("source") == "current_run" for v in have.values()) else "historical",
    }


def certified_keys(timeframe: str = "all") -> list[str]:
    """Registry keys certified on a track — or, by default, on EVERY track
    (the overall verdict) — best score first."""
    if timeframe == "all":
        verdicts = {k: overall_verdict(k) for k in REGISTRY}
        keys = [k for k, v in verdicts.items() if v and v["certified"]]
        return sorted(keys, key=lambda k: -verdicts[k]["score"])
    table = certification_table(timeframe)
    keys = [k for k, v in table.items() if k in REGISTRY and is_certified(v)]
    return sorted(keys, key=lambda k: -table[k]["score"])


def hypothesis_of(cls) -> str:
    """The "Hypothesis: …" paragraph of the indicator's docstring — the class's
    when it states one (the packs of 2026-09-09 keep many classes per module),
    else the module's, else the first paragraph of whichever exists."""
    pattern = r"Hypothesis(?:\s*\([^)]*\))?\s*[:—-]\s*(.*?)(?:\n\s*\n|\Z)"
    docs = [d for d in (inspect.getdoc(cls), inspect.getdoc(sys.modules[cls.__module__])) if d]
    match = next((m for d in docs if (m := re.search(pattern, d, re.S))), None)
    text = match.group(1) if match else (docs[-1] if docs else "").split("\n\n")[0]
    return re.sub(r"\s+", " ", text).strip()


def describe(key: str) -> dict:
    from .indicators import VARIANTS
    cls = REGISTRY[key]
    params = cls().params
    lineage = VARIANTS.get(key)
    base_key = lineage["base_key"] if lineage else key
    return {
        "key": key,
        "base_key": base_key,
        "variant": lineage,
        "name": cls.name,
        "hypothesis": hypothesis_of(REGISTRY[base_key]),
        "params": {k: (list(v) if isinstance(v, tuple) else v) for k, v in params.items()},
        "param_space": {k: [float(lo), float(hi)] for k, (lo, hi) in cls.param_space.items()},
        "warmup_bars": int(WARMUP_BARS.get(key, 400)),
        # THE score: the worst track, certified only on every track …
        "certification": overall_verdict(key),
        # … and each track's own verdict behind it
        "timeframes": {tf: verdict_for(key, tf) for tf in TIMEFRAMES},
    }


def describe_all() -> dict:
    from . import __version__
    from .variants import UNAVAILABLE

    return {
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "package_dir": str(Path(__file__).resolve().parent),
        "version": __version__,
        "timeframes": list(TIMEFRAMES),
        "indicators": [describe(k) for k in REGISTRY],
        "unavailable_variants": list(UNAVAILABLE),
    }


def _main(argv: list[str]) -> int:
    # A Windows console defaults to cp1252, which cannot encode the verdict
    # glyphs (⚔️ 🔨) or the hypotheses' math — the human-readable listing
    # used to die with UnicodeEncodeError. JSON is ASCII-escaped and unaffected.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if "--json" in argv:
        print(json.dumps(describe_all()))
        return 0
    keys = [a for a in argv if not a.startswith("-")] or list(REGISTRY)
    for key in keys:
        if key not in REGISTRY:
            print(f"unknown indicator '{key}' — known: {sorted(REGISTRY)}", file=sys.stderr)
            return 2
        d = describe(key)
        cert = d["certification"]
        verdict = (f"{cert['score']}/{cert['grade']} p={cert['perm_p']} ({cert['date']})"
                   + (" ⚔️ certified" if cert["certified"] else " 🔨 forge")) if cert else "uncertified"
        print(f"{key:10s} {d['name']:22s} warm-up {d['warmup_bars']:>5}  {verdict}")
        print(f"           {d['hypothesis']}")
        print(f"           params {d['params']}")
    return 0


if __name__ == "__main__":
    sys.exit(_main(sys.argv[1:]))
