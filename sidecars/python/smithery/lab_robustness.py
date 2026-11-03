"""Research kind 'robustness': the System Gauntlet on a saved system (docs/ROBUSTNESS.md).

Runs in the frozen research folder like every system run: the engine, the private
library, the saved prices (cache.sqlite) and the other-venue candles (venues/) are
copies taken before the worker started; nothing is downloaded.
"""
from __future__ import annotations

from dataclasses import replace

from .lab import emit, save


def run(path, doc):
    from rotation_lab.rpc import _build_config
    from .lab_compare import frozen_engine, within_saved_data
    from .lab_profit import describe
    from .prices import venue_cache_path
    from . import system_gauntlet as sg

    progress = lambda message: emit(path, "progress", message=message)  # noqa: E731
    config = replace(_build_config(doc["config"]), compare_trends=())
    members = sg.members_of(config)
    config = within_saved_data(config, frozen_engine(config, [members], path / "data"), lambda m, f=None: progress(m))
    venue = venue_cache_path(tuple(doc.get("venues") or ()))
    progress(f"System Gauntlet ({'fast' if doc.get('fast') else 'full'}) on {doc.get('system', {}).get('name') or 'the system'}")
    result = sg.run(config, data_dir=path / "data", venue_path=venue if venue.is_file() else None,
                    fast=bool(doc.get("fast")), seed=int(doc.get("seed") or 42), progress=progress,
                    cube_dir=path / "cube")
    result["system"] = describe(config, doc.get("system") or {})
    result["venues"] = list(doc.get("venues") or ())
    save(path / "results.json", result)
    v = result["verdict"]
    save(path / "verdict.json", {k: v.get(k) for k in ("status", "grade", "score", "robust", "failures", "complete", "perm_p", "policy")})
    progress(f"Verdict: {v['status']} · grade {v['grade']}" + (f" · {len(v['failures'])} failed gates" if v["failures"] else ""))
