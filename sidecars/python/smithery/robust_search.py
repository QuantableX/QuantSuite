"""The aggregate search's fight against overfitting (docs/ROBUSTNESS.md, 2026-11-03).

* ``pool_gate`` — only indicators that pass the Indicator Gauntlet on the system's
  track under the current evaluation (registry.robust_on_track).
* ``Worlds`` — the same candidates in several price worlds: the saved candles, ±0.03%
  noise (two seeds), another exchange's candles when they were prepared, and every
  trade one day later. A candidate scores its WORST world on the search window, so a
  fit to one exact history no longer wins.
* ``walk_forward`` — the search sees only the bars before ``cut``; its leaders run on
  the blind bars after it and must beat random aggregates of the same size there.
* ``finalists`` — the System Gauntlet on the best leaders that passed.

Every world is an engine-identical bulk evaluator (smithery.lab_fast); with PyTorch and
CUDA present the bit-identical GPU evaluator (smithery.lab_gpu) scores it instead.
"""
from __future__ import annotations

import math
import random
from pathlib import Path

import numpy as np

from . import robust_policy as rp

NOISE_SEEDS = (1, 2)
NOISE_SIGMA = 0.0003
BASELINE_PER_SIZE = 300


def pool_gate(keys, timeframe: str, certified=None):
    """(kept, excluded): keys that pass the Indicator Gauntlet on the track under the current evaluation.

    ``certified`` is the list recorded when the run was created (lab.create): a frozen research
    run's vault holds the price shelf only, so its own lookup would find no verdict at all."""
    if certified is not None:
        allowed = set(certified)
        kept = [k for k in keys if k in allowed]
    else:
        from .registry import robust_on_track
        kept = [k for k in keys if robust_on_track(k, timeframe)]
    return kept, [k for k in keys if k not in set(kept)]


def gate_candidates(keys, reference, timeframe: str, certified=None):
    """(kept, pool, excluded) for a robust search. The reference is the control and is always
    evaluated; its members, like every other key, are searched only when they pass themselves."""
    pool, excluded = pool_gate(keys, timeframe, certified)
    allowed = set(pool)
    return [k for k in keys if k in allowed or k in reference], pool, excluded


def certified_pool(raw_config: dict, indicators=None, reference=None) -> dict:
    """The candidates a robust search may use, decided where the user's evidence is visible."""
    from dataclasses import replace
    from rotation_lab.rpc import _build_config
    from .evidence import EVALUATION_VERSION
    from .lab_profit import TRACKS, default_keys
    from .registry import describe_all, robust_on_track
    config = replace(_build_config(raw_config), compare_trends=())
    rows = describe_all()["indicators"]
    saved = config.indicator.aggregate if config.indicator.trend == "aggregate" else (config.indicator.trend,)
    keys = list(dict.fromkeys([*(indicators or default_keys(rows, config)), *(reference or saved)]))
    timeframe = config.single_asset.timeframe if config.mode == "single_asset" else config.cadence.value
    track = TRACKS.get(timeframe, "1d")
    return {"track": track, "evaluation": EVALUATION_VERSION, "candidates": len(keys),
            "keys": [k for k in keys if robust_on_track(k, track)]}


def default_cut(dates, blind_days: int | None = None) -> int:
    """First bar of the blind window: the last two years (a third of a short history)."""
    n = len(dates)
    if blind_days:
        days = int(blind_days)
    else:
        span = (dates[-1] - dates[0]).days
        days = 730 if span >= 4 * 365 else max(180, span // 3)
    import pandas as pd
    cut = int(dates.searchsorted(dates[-1] - pd.Timedelta(days=days)))
    if not 60 <= cut <= n - 60:
        raise ValueError("The window is too short for a search window and a blind window of 60 bars each")
    return cut


class Worlds:
    """The search's price worlds, each a built bulk evaluator over the same candidates."""

    def __init__(self, config, keys, path: Path, base, venue_path: Path | None, progress=None):
        from .lab_compare import frozen_engine
        from .lab_fast import FastSystem
        say = progress or (lambda m, f=None: None)
        self.systems = {"saved": base}
        specs = {f"noise {s}": {"kind": "noise", "seed": s, "sigma": NOISE_SIGMA} for s in NOISE_SEEDS}
        if venue_path is not None and Path(venue_path).is_file():
            specs["other exchange"] = {"kind": "venue", "path": str(venue_path)}
        for name, spec in specs.items():
            say(f"Price world '{name}': computing every candidate's votes")
            engine = frozen_engine(config, [(k,) for k in keys], path / "data-worlds" / name.replace(" ", "-"), prices=spec)
            self.systems[name] = FastSystem(config, keys, path / ("cube-" + name.replace(" ", "-")), engine,
                                            progress=say).build()
        self.complete = "other exchange" in self.systems
        self.dates = base.dates
        # Votes were built with every worker; scoring shares them, one pool per world plus the delay.
        from .lab_fast import default_workers
        share = max(2, default_workers() // (len(self.systems) + 1))
        for system in self.systems.values():
            system.workers = share
        from . import lab_gpu
        self.gpu, self.scoring = lab_gpu.twins(self.systems, say)

    @property
    def names(self):
        return [*self.systems, "one day later"]

    @property
    def failed(self):
        bad = {}
        for system in self.systems.values():
            bad.update(system.failed)
        return bad

    def curves(self, trials) -> dict[str, np.ndarray]:
        """Full equity curves of the trials in every world."""
        out = {}
        for name, system in self.systems.items():
            out[name] = self.gpu[name].simulate(trials) if name in self.gpu else system.simulate(trials)[0]
        base = "saved"
        out["one day later"] = self.gpu[base].simulate(trials, delay=1) if base in self.gpu else \
            self.systems[base].simulate(trials, delay=1)[0]
        return out

    def evaluate(self, trials, end: int):
        """[(worst world's capital multiple, worst world's max drawdown %)] on the bars before ``end``."""
        trials = [tuple(t) for t in trials]
        out = [None] * len(trials)
        bad = self.failed
        todo = [i for i, t in enumerate(trials) if not any(k in bad for k in t)]
        if not todo:
            return out
        caps, dds = [], []
        if self.gpu:
            from .lab_fast import FastSystem
            for name, curve in self.curves([trials[i] for i in todo]).items():
                m = FastSystem.metrics(curve[:, :end])
                caps.append(m["net_return_multiplier"])
                dds.append(m["max_drawdown_pct"])
        else:
            jobs = [(s, 0) for s in self.systems.values()] + [(self.systems["saved"], 1)]
            for system, delay in jobs:
                values = system.evaluate([trials[i] for i in todo], delay=delay, end=end)
                caps.append(np.array([v[0] for v in values]))
                dds.append(np.array([v[1] for v in values]))
        worst_cap, worst_dd = np.min(caps, axis=0), np.max(dds, axis=0)
        for i, c, d in zip(todo, worst_cap, worst_dd):
            out[i] = (float(c), float(d))
        return out

    def close(self):
        for name, system in self.systems.items():
            if name != "saved":
                system.close(remove=True)


def _blind(curves: dict[str, np.ndarray], cut: int, years: float) -> dict:
    """Per trial: each world's CAGR on the blind bars (the continuous run's equity after ``cut``)."""
    per = {name: [rp.cagr(float(c[-1] / c[cut - 1]), years) for c in curve] for name, curve in curves.items()}
    worst = np.min(np.array(list(per.values())), axis=0)
    return {"worlds": per, "worst": worst.tolist()}


def walk_forward(worlds: Worlds, leaders, pool, bases, cut: int, seed: int, progress=None) -> dict:
    """Leaders on the blind window against random aggregates of the same size from the pool."""
    say = progress or (lambda m, f=None: None)
    dates = worlds.dates
    years = rp.years_between(dates[cut - 1:])
    trials = [tuple(t) for t, _ in leaders]
    say(f"Walk-forward: {len(trials)} leaders on the blind window {dates[cut].date()} → {dates[-1].date()}")
    lead = _blind(worlds.curves(trials), cut, years)
    rng = random.Random(seed)
    by_base = {}
    for k in pool:
        by_base.setdefault(bases[k], []).append(k)
    names = sorted(by_base)
    baseline = {}
    for size in sorted({len(t) for t in trials}):
        if size > len(names):
            continue
        sample = list(dict.fromkeys(tuple(sorted(rng.choice(by_base[b]) for b in rng.sample(names, size)))
                                    for _ in range(BASELINE_PER_SIZE)))
        say(f"Walk-forward: {len(sample)} random aggregates of size {size} on the blind window")
        baseline[size] = _blind(worlds.curves(sample), cut, years)["worst"]
    rows = []
    for k, (t, value) in enumerate(leaders):
        ref = baseline.get(len(t))
        worst = lead["worst"][k]
        pct = float(np.mean([r < worst for r in ref]) * 100) if ref else None
        rows.append({"members": list(t), "size": len(t), "search_capital": value[0], "search_dd": value[1],
                     "blind_worst_cagr": worst, "blind_worlds": {n: v[k] for n, v in lead["worlds"].items()},
                     "random_median_cagr": float(np.median(ref)) if ref else None, "percentile": pct,
                     "passed": pct is not None and pct >= 50 and worst > 0})
    return {"cut": str(dates[cut].date()), "start": str(dates[0].date()), "end": str(dates[-1].date()),
            "leaders": rows, "baseline_sizes": sorted(baseline),
            "passed": sum(r["passed"] for r in rows)}


def finalists(config, rows, top: int, data_dir: Path, venue_path, seed: int, progress=None) -> list[dict]:
    """The System Gauntlet (fast) on the best leaders that passed the walk-forward."""
    from dataclasses import replace
    from rotation_lab.config import IndicatorConfig
    from . import system_gauntlet as sg
    say = progress or (lambda m, f=None: None)
    out = []
    for k, row in enumerate([r for r in rows if r["passed"]][:top], 1):
        members = tuple(row["members"])
        cfg = replace(config, indicator=IndicatorConfig(trend="aggregate", aggregate=members,
                                                        params=config.indicator.params,
                                                        ema_cross=config.indicator.ema_cross))
        say(f"System Gauntlet on finalist {k} ({len(members)} members)")
        res = sg.run(cfg, data_dir=Path(data_dir) / f"finalist-{k}", venue_path=venue_path, fast=True, seed=seed,
                     progress=lambda m: say(f"Finalist {k}: {m}"), cube_dir=Path(data_dir) / f"finalist-{k}-cube")
        out.append({"members": list(members), "verdict": res["verdict"], "axes": res["axes"],
                    "groups": {g: {kk: v.get(kk) for kk in ("median", "min", "score", "failed", "reasons")}
                               for g, v in res["groups"].items()},
                    "base": res["base"], "walk_forward": row})
    return out
