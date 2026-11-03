"""The System Gauntlet: does a saved system survive what other data and live trading change?

docs/ROBUSTNESS.md is the policy; smithery.robust_policy holds its groups, gates and
grades. Every variant runs the unchanged engine on the saved data (offline), changed
in one way at a time:

* parameters — member parameters jittered, members dropped or added, system settings
* time — later starts, calendar eras, rolling windows
* exchange — candle noise, another exchange's candles
* assets — neighbouring universes (top N, without the largest coin)
* Monte Carlo — shuffled calendar days (luck), bootstrap of the daily returns (path risk)
* execution — one and two days of delay, 2× and 3× the fee

Retention is the share of the base CAGR a variant keeps. Member drops, additions and
delays run in bulk (smithery.lab_fast, engine-identical) against its own base; every
other variant is a native engine run against the native base.
"""
from __future__ import annotations

import datetime as dt
import math
import os
import random
import time
from concurrent.futures import ProcessPoolExecutor
from dataclasses import replace
from pathlib import Path

import numpy as np
import pandas as pd

from . import robust_policy as rp
from .monte_carlo import stationary_bootstrap_order

FULL = dict(jitter=16, starts=11, noise=3, permutation=39, bootstrap=1000, add=12)
FAST = dict(jitter=6, starts=4, noise=2, permutation=9, bootstrap=300, add=6)
JITTER = 0.15
NOISE_SIGMA = 0.0003


def default_workers() -> int:
    return max(1, min(8, int(os.environ.get("SMITHERY_ROBUST_WORKERS", (os.cpu_count() or 4) // 4))))


# ----------------------------------------------------------------- native runs
_ENGINE = {}


def _init_worker():
    os.environ["ROTATION_LAB_WORKERS"] = "1"
    from .lab_compare import offline
    offline()


def _engine_for(spec):
    from rotation_lab.backtest.engine import BacktestEngine
    from .lab_compare import SavedPrices, saved_cache, saved_rankings
    from .prices import PricesVariant
    if "cache" not in _ENGINE:
        _ENGINE["cache"] = saved_cache()
    cache = _ENGINE["cache"]
    return BacktestEngine(registry=saved_rankings(cache), ohlcv=PricesVariant(SavedPrices(cache), spec))


def native_job(job):
    """One engine run: (label, config, price spec, keep equity) -> summary."""
    label, config, spec, keep = job
    try:
        result = _engine_for(spec).run(config)
        equity = result.equity_strategy
        if equity is None or len(equity) < 2:
            raise ValueError("; ".join(result.notes[-2:]) or "no equity curve")
        multiple = float(equity.iloc[-1])
        years = rp.years_between(equity.index)
        out = {"label": label, "multiple": multiple, "years": years, "cagr": rp.cagr(multiple, years),
               "max_drawdown_pct": float(result.metrics_strategy.max_drawdown_pct)}
        if keep:
            out["equity"] = equity
            out["held"] = result.held_asset
        return out
    except Exception as exc:  # noqa: BLE001 — one broken variant is recorded, the rest still run
        return {"label": label, "error": f"{type(exc).__name__}: {exc}"[:400]}


def run_native(jobs, workers=None, progress=None, stage=""):
    workers = workers or default_workers()
    results = []
    if workers <= 1 or len(jobs) <= 1:
        _init_worker()
        for k, job in enumerate(jobs, 1):
            results.append(native_job(job))
            if progress:
                progress(f"{stage}: {k} of {len(jobs)}")
        return results
    import multiprocessing as mp
    with ProcessPoolExecutor(max_workers=min(workers, len(jobs)), mp_context=mp.get_context("spawn"),
                             initializer=_init_worker) as pool:
        for k, out in enumerate(pool.map(native_job, jobs), 1):
            results.append(out)
            if progress:
                progress(f"{stage}: {k} of {len(jobs)}")
    return results


# ------------------------------------------------------------------- variants
def members_of(config) -> tuple[str, ...]:
    ind = config.indicator
    return tuple(ind.aggregate) if ind.trend == "aggregate" else (ind.trend,)


def jittered(config, rows: dict, rng: random.Random):
    """The config with every member's numeric parameters ×(1 ± JITTER), clipped to their space."""
    params = {k: dict(v) for k, v in (config.indicator.params or {}).items()}
    ema = config.indicator.ema_cross
    for key in members_of(config):
        if key == "ema_cross":
            fast = max(2, round(ema.fast_length * (1 + rng.uniform(-JITTER, JITTER))))
            slow = max(fast + 1, round(ema.slow_length * (1 + rng.uniform(-JITTER, JITTER))))
            ema = replace(ema, fast_length=fast, slow_length=slow)
            continue
        row = rows.get(key)
        if not row:
            continue
        base = {**(row.get("params") or {}), **params.get(key, {})}
        space = row.get("param_space") or {}
        changed = dict(params.get(key, {}))
        for name, value in base.items():
            if isinstance(value, bool) or not isinstance(value, (int, float)) or name not in space:
                continue
            lo, hi = space[name]
            v = float(value) * (1 + rng.uniform(-JITTER, JITTER))
            v = min(max(v, lo), hi)
            changed[name] = int(round(v)) if isinstance(value, int) else round(v, 6)
        if changed:
            params[key] = changed
    return replace(config, indicator=replace(config.indicator, params=params, ema_cross=ema))


def settings_variants(config) -> list[tuple[str, object]]:
    """The system's own settings: top N ±1 and the market filter's lengths ±20%."""
    from rotation_lab.config import TotalBreakoutConfig
    out = []
    if config.mode != "single_asset":
        for d in (-1, 1):
            n = config.top_n + d
            if n >= 2:
                out.append((f"top {n}", replace(config, top_n=n)))
        m = config.market_indicator
        if config.market_filter and isinstance(m, TotalBreakoutConfig):
            for f in (0.8, 1.2):
                out.append((f"market filter lengths ×{f}", replace(config, market_indicator=replace(
                    m, trend_length=max(2, round(m.trend_length * f)), entry_length=max(2, round(m.entry_length * f)),
                    exit_length=max(2, round(m.exit_length * f))))))
    return out


def universe_variants(config) -> list[tuple[str, object]]:
    if config.mode == "single_asset":
        return []
    out = []
    for n in (config.top_n - 2, config.top_n + 2):
        if n >= 2 and n != config.top_n:
            out.append((f"top {n}", replace(config, top_n=n)))
    # top_n counts the excluded coins too: one more excluded and one more counted keeps the size.
    out.append((f"without the {config.exclude_top_n + 1} largest" if config.exclude_top_n else "without the largest coin",
                replace(config, exclude_top_n=config.exclude_top_n + 1, top_n=config.top_n + 1)))
    return out


# ------------------------------------------------------------------- analysis
def eras(equity: pd.Series) -> dict:
    """Calendar years, rolling 1-year returns and the worst rolling 2-year CAGR."""
    eq = equity.copy()
    if eq.index.tz is not None:
        eq.index = eq.index.tz_localize(None)
    years, prev = {}, float(eq.iloc[0])
    for y in sorted(set(eq.index.year)):
        cur = eq[eq.index.year == y]
        years[str(y)] = float(cur.iloc[-1] / prev - 1)
        prev = float(cur.iloc[-1])
    daily = eq.resample("D").last().ffill()
    one = (daily / daily.shift(365) - 1).dropna()
    two = ((daily / daily.shift(730)) ** 0.5 - 1).dropna()
    positive = float((one >= 0).mean()) if len(one) else float("nan")
    worst2 = float(two.min()) if len(two) else float("nan")
    score = float(100 * np.clip((positive - 0.5) / 0.45, 0, 1)) if positive == positive else float("nan")
    failed = worst2 == worst2 and worst2 < rp.WORST_2Y_CAGR
    return {"group": "eras", "axis": "time", "years": years, "rolling_1y_positive": positive,
            "worst_1y": float(one.min()) if len(one) else float("nan"), "worst_2y_cagr": worst2, "score": score,
            "gated": True, "failed": bool(failed),
            "reasons": [f"worst rolling 2-year CAGR {worst2:.0%} < {rp.WORST_2Y_CAGR:.0%}"] if failed else []}


def bootstrap(equity: pd.Series, n: int, seed: int) -> dict:
    """Stationary block bootstrap (mean block 20 days) of the daily returns: path risk."""
    r = equity.pct_change().dropna().to_numpy()
    rng = np.random.default_rng(seed)
    T = len(r)
    if T < 60:
        return {"group": "bootstrap", "axis": "monte_carlo", "score": float("nan"), "gated": False, "failed": False, "reasons": []}
    years = T / 365.25
    cagrs, dds, loss1, loss3 = [], [], [], []
    for _ in range(n):
        order = stationary_bootstrap_order(T, rng, 20.0)
        path = np.cumprod(1 + r[order])
        cagrs.append(rp.cagr(path[-1], years))
        peak = np.maximum.accumulate(path)
        dds.append(float(((peak - path) / peak).max()))
        # Forward horizons: fresh block-bootstrap paths of the horizon's length (simulated forward tests).
        for horizon, bucket in ((365, loss1), (1095, loss3)):
            bucket.append(np.prod(1 + r[_block_indices(T, horizon, rng, 20.0)]) < 1)
    p3 = float(np.mean(loss3)) if loss3 else float("nan")
    score = float(100 * np.clip((rp.BOOTSTRAP_3Y_LOSS - p3) / (rp.BOOTSTRAP_3Y_LOSS - 0.02), 0, 1)) if p3 == p3 else float("nan")
    failed = p3 == p3 and p3 > rp.BOOTSTRAP_3Y_LOSS
    return {"group": "bootstrap", "axis": "monte_carlo", "n": n,
            "cagr_p5": float(np.percentile(cagrs, 5)), "cagr_p50": float(np.percentile(cagrs, 50)),
            "dd_p50": float(np.percentile(dds, 50)), "dd_p95": float(np.percentile(dds, 95)),
            "loss_1y": float(np.mean(loss1)) if loss1 else float("nan"), "loss_3y": p3, "score": score,
            "gated": True, "failed": bool(failed),
            "reasons": [f"{p3:.0%} chance of a 3-year loss > {rp.BOOTSTRAP_3Y_LOSS:.0%}"] if failed else []}


def _block_indices(T: int, length: int, rng: np.random.Generator, avg_block: float) -> np.ndarray:
    """Stationary bootstrap indices of any length into T observations (blocks wrap around)."""
    out = np.empty(length, dtype=np.int64)
    out[0] = rng.integers(0, T)
    jumps = rng.random(length) < 1.0 / avg_block
    starts = rng.integers(0, T, size=length)
    for i in range(1, length):
        out[i] = starts[i] if jumps[i] else (out[i - 1] + 1) % T
    return out


def permutation(base_cagr: float, shuffled_cagrs: list[float]) -> dict:
    """p of the luck test: shuffled markets at least as good as the real one."""
    finite = [c for c in shuffled_cagrs if c == c]
    if not finite:
        return {"group": "permutation", "axis": "monte_carlo", "p": None, "score": float("nan"),
                "gated": False, "failed": False, "reasons": []}
    beats = sum(c >= base_cagr for c in finite)
    p = (1 + beats) / (1 + len(finite))
    return {"group": "permutation", "axis": "monte_carlo", "p": p, "n": len(finite),
            "median_shuffled_cagr": float(np.median(finite)),
            "score": float(np.clip(100 * (1 - (p - 0.01) / 0.19), 0, 100)), "gated": False, "failed": False,
            "reasons": [], "caps_grade": p > rp.PERMUTATION_P}


# ------------------------------------------------------------------------ run
def run(config, *, data_dir: Path, venue_path: Path | None = None, fast: bool = False, seed: int = 42,
        workers: int | None = None, progress=None, cube_dir: Path | None = None) -> dict:
    """The full System Gauntlet for one RunConfig on the saved data (ROTATION_LAB_CACHE)."""
    say = progress or (lambda message: None)
    counts = FAST if fast else FULL
    started = time.monotonic()
    rng = random.Random(seed)
    config = replace(config, compare_trends=())
    members = members_of(config)
    from .registry import describe_all
    rows = {r["key"]: r for r in describe_all()["indicators"]}

    say("Base run")
    base = run_native([("base", config, None, True)], workers=1)[0]
    if "error" in base:
        raise ValueError(f"The system does not run on the saved data: {base['error']}")
    base_cagr = base["cagr"]
    groups: dict[str, dict] = {}
    details: dict[str, list] = {}
    complete = True

    # Native variants, all in one pool.
    jobs: list[tuple] = []
    plan: dict[str, list[str]] = {}

    def add(group, label, cfg, spec=None):
        jobs.append((f"{group}|{label}", cfg, spec, False))
        plan.setdefault(group, []).append(label)

    for i in range(counts["jitter"]):
        add("parameter_jitter", f"jitter {i + 1}", jittered(config, rows, rng))
    for label, cfg in settings_variants(config):
        add("system_settings", label, cfg)
    total_days = (config.end_date - config.start_date).days
    # Later starts spread over the first year; each run keeps at least two years.
    steps = (30, 90, 180, 270) if fast else tuple(30 * k for k in range(1, counts["starts"] + 1))
    for d in (d for d in steps if total_days - d >= 730):
        add("start_shift", f"start +{d} days", replace(config, start_date=config.start_date + dt.timedelta(days=d)))
    for s in range(1, counts["noise"] + 1):
        add("price_noise", f"noise seed {s}", config, {"kind": "noise", "seed": s, "sigma": NOISE_SIGMA})
    if venue_path is not None and Path(venue_path).is_file():
        add("venue", "other exchanges", config, {"kind": "venue", "path": str(venue_path)})
    else:
        complete = False
        details["venue"] = [{"label": "other exchanges", "error": "no candles from another exchange were prepared"}]
    for label, cfg in universe_variants(config):
        add("universe", label, cfg)
    for label, f in (("2× fee", 2), ("3× fee", 3)):
        add("fees", label, replace(config, fee_rate=config.fee_rate * f))
    window = {"start": config.start_date.isoformat(), "end": config.end_date.isoformat()}
    for s in range(1, counts["permutation"] + 1):
        add("permutation", f"shuffle {s}", config, {"kind": "shuffle", "seed": s, **window})

    say(f"{len(jobs)} variant runs")
    results = run_native(jobs, workers=workers, progress=say, stage="Variant runs")
    by_group: dict[str, list[dict]] = {}
    for job, out in zip(jobs, results):
        group = job[0].split("|", 1)[0]
        by_group.setdefault(group, []).append(out)
    for group, outs in by_group.items():
        details[group] = [{k: v for k, v in o.items() if k not in ("equity", "held")} for o in outs]

    def cagrs(group, labels=None):
        return [o.get("cagr", float("nan")) for o in by_group.get(group, [])
                if labels is None or o["label"].split("|", 1)[1] in labels]

    n_members = len(members)
    for group in ("parameter_jitter", "system_settings", "start_shift", "price_noise", "venue", "universe"):
        if by_group.get(group):
            groups[group] = rp.judge(group, base_cagr, cagrs(group), members=n_members)
    groups["fees"] = rp.judge("fees", base_cagr, cagrs("fees", {"2× fee"}), members=n_members)
    groups["fees"]["also"] = {"3× fee": rp.retention(base_cagr, (cagrs("fees", {"3× fee"}) or [float("nan")])[0])}
    groups["permutation"] = permutation(base_cagr, cagrs("permutation"))
    groups["eras"] = eras(base["equity"])
    groups["bootstrap"] = bootstrap(base["equity"], counts["bootstrap"], seed)

    # Bulk runs: member drops and additions, delays (engine-identical, against their own base).
    bulk = _bulk(config, members, rows, data_dir, cube_dir, counts, rng, say)
    if bulk.get("error"):
        complete = False
        details["bulk"] = [{"error": bulk["error"]}]
    else:
        fb = bulk["base_cagr"]
        if n_members > 1:
            groups["member_drop"] = rp.judge("member_drop", fb, bulk["drop"], members=n_members)
            details["member_drop"] = bulk["drop_details"]
            if bulk["add"]:
                groups["member_add"] = rp.judge("member_add", fb, bulk["add"], members=n_members)
                details["member_add"] = bulk["add_details"]
        groups["delay"] = rp.judge("delay", fb, [bulk["delay"][1]], members=n_members)
        groups["delay"]["also"] = {"2 days": rp.retention(fb, bulk["delay"][2])}
        details["delay"] = [{"label": f"{d} day{'s' if d > 1 else ''} later", "cagr": c} for d, c in bulk["delay"].items()]

    axes = {}
    for axis in rp.AXES:
        scores = [g["score"] for g in groups.values() if g.get("axis") == axis and g.get("score") == g.get("score")]
        axes[axis] = float(np.mean(scores)) if scores else float("nan")
        if not scores:
            complete = False
    failures = [f"{g['group']}: {r}" for g in groups.values() for r in g.get("reasons", []) if g.get("failed")]
    perm_p = groups["permutation"].get("p")
    verdict = rp.verdict(axes, failures, perm_p, complete, base_cagr)
    equity = base["equity"]
    dates = [d.strftime("%Y-%m-%d") for d in equity.index]
    return rp.jsonable({"kind": "robustness", "format": 1, "policy": rp.POLICY_VERSION, "seed": seed, "fast": bool(fast),
            "members": list(members), "config_window": window,
            "base": {"multiple": base["multiple"], "cagr": base_cagr, "years": base["years"],
                     "max_drawdown_pct": base["max_drawdown_pct"]},
            "axes": axes, "groups": groups, "details": details, "verdict": verdict,
            "equity": {"dates": dates, "values": [float(v) for v in equity.to_numpy()]},
            "groups_meta": {k: {"axis": v["axis"], "what": v["what"],
                                "gate": None if v.get("gate") is None else vars(v["gate"])} for k, v in rp.GROUPS.items()},
            "seconds": round(time.monotonic() - started)})


def _bulk(config, members, rows, data_dir, cube_dir, counts, rng, say) -> dict:
    """Member drops/additions and delays on the engine-identical bulk evaluator."""
    from .lab_compare import frozen_engine
    from .lab_fast import FastSystem, unsupported_reason
    from .lab_profit import default_keys
    reason = unsupported_reason(config)
    if reason:
        return {"error": f"bulk runs unavailable: {reason}"}
    bases = {k: r.get("base_key", k) for k, r in rows.items()}
    member_bases = {bases.get(m, m) for m in members}
    pool = [k for k in default_keys(list(rows.values()), config) if bases.get(k, k) not in member_bases]
    extra = rng.sample(pool, min(counts["add"], len(pool))) if len(members) > 1 else []
    keys = list(dict.fromkeys([*members, *extra]))
    say(f"Bulk evaluator for {len(keys)} indicators")
    engine = frozen_engine(config, [(k,) for k in keys], Path(data_dir) / "bulk")
    folder = Path(cube_dir or Path(data_dir).parent / "cube")
    system = FastSystem(config, keys, folder, engine, progress=lambda m, f=None: say(m)).build()
    try:
        bad = [m for m in members if m in system.failed]
        if bad:
            return {"error": f"members not evaluable in bulk: {bad}"}
        extra = [k for k in extra if k not in system.failed]
        trials = [tuple(members)]
        drops = [tuple(x for x in members if x != m) for m in members] if len(members) > 2 else []
        adds = [tuple(members) + (k,) for k in extra]
        curve, _ = system.simulate(trials + drops + adds)
        years = rp.years_between(system.dates)
        cag = [rp.cagr(float(c[-1]), years) for c in curve]
        out = {"base_cagr": cag[0], "drop": cag[1:1 + len(drops)], "add": cag[1 + len(drops):],
               "drop_details": [{"label": f"without {m}", "cagr": c} for m, c in zip(members, cag[1:1 + len(drops)])],
               "add_details": [{"label": f"with {k}", "cagr": c} for k, c in zip(extra, cag[1 + len(drops):])]}
        out["delay"] = {}
        for d in (1, 2):
            c, _ = system.simulate(trials, delay=d)
            out["delay"][d] = rp.cagr(float(c[0, -1]), years)
        return out
    finally:
        system.close(remove=True)
