"""Anchored walk-forward with plateau-medoid selection — see Docs/03.

We never take the argmax of a noisy surface: within each training window the
top-quartile region is identified and its MEDOID (the parameterization most
surrounded by other good ones) is carried into the test segment.
"""
from __future__ import annotations

import itertools

import numpy as np
import pandas as pd

from . import parallel as par
from .contract import TrendIndicator
from . import execution, quality

MAX_COMBOS = 48


def objective(bt: dict) -> float:
    """Compound growth, absolute drawdown, Sharpe, Sortino and Omega."""
    if "pnl" in bt:
        bt = execution.metrics(np.expm1(bt["pnl"]), turnover=bt.get("flips_per_year", 0) * bt.get("years", 1))
    return quality.score(bt)


def param_grid(ind: TrendIndicator, n_per: int = 4,
               rng: np.random.Generator | None = None) -> list[dict]:
    axes = {}
    for p, (lo, hi) in ind.param_space.items():
        base = ind.params[p]
        vals = np.geomspace(lo, hi, n_per) if lo > 0 else np.linspace(lo, hi, n_per)
        if isinstance(base, int):
            vals = sorted({int(round(v)) for v in vals})
        else:
            vals = sorted({round(float(v), 4) for v in vals})
        axes[p] = vals
    combos = [dict(zip(axes, c)) for c in itertools.product(*axes.values())]
    if len(combos) > MAX_COMBOS:
        rng = rng or np.random.default_rng(7)
        combos = [combos[i] for i in rng.choice(len(combos), MAX_COMBOS, replace=False)]
    defaults = {key: ind.params[key] for key in axes}
    if defaults not in combos:
        combos = combos[:MAX_COMBOS - 1] + [defaults]
    return combos


def _medoid(top: list[tuple[dict, float]], space: dict) -> dict:
    """Parameterization of the top-quartile region minimizing summed normalized
    distance to its peers — the point most likely to survive regime drift."""
    def norm(p):
        return np.array([(p[k] - lo) / (hi - lo or 1) for k, (lo, hi) in space.items()])
    pts = [norm(p) for p, _ in top]
    dists = [sum(np.linalg.norm(a - b) for b in pts) for a in pts]
    return top[int(np.argmin(dists))][0]


def walk_forward(ind: TrendIndicator, df: pd.DataFrame, cost_bps: float,
                 n_folds: int = 4, purge: int | None = None,
                 mode: str = "long_only") -> dict:
    from .indicators import REGISTRY, WARMUP_BARS
    if not isinstance(n_folds, int) or not 2 <= n_folds <= 12:
        raise ValueError("Walk-forward needs 2 to 12 folds")
    n = len(df)
    warmup = next((WARMUP_BARS[k] for k, cls in REGISTRY.items() if type(ind) is cls), 60)
    purge = max(warmup, int(0.02 * n)) if purge is None else purge
    if not isinstance(purge, int) or purge < warmup:
        raise ValueError(f"Purge must be at least the indicator's {warmup}-bar warm-up")
    test_start = max(int(n * 0.4), warmup + purge + 60)
    if n - test_start < n_folds * 30:
        raise ValueError("Not enough history for training, purge and at least 30 bars per test fold")
    bounds = np.linspace(test_start, n, n_folds + 1, dtype=int)
    combos = param_grid(ind)

    oos_pnl, folds, is_sharpes = [], [], []
    held = 0.0
    turnover = 0.0
    for f in range(n_folds):
        t0, t1 = bounds[f], bounds[f + 1]
        train = df.iloc[:t0 - purge]
        test = df.iloc[t0:t1]

        bts = par.pmap(par.eval_execution,
                       [(ind.with_params(**c), train, cost_bps, mode) for c in combos])
        scored = [(c, objective(bt), bt["sharpe"]) for c, bt in zip(combos, bts)]
        scored.sort(key=lambda x: -x[1])
        ranked = quality.pareto_order(bts)
        if not ranked:
            raise ValueError("No profitable training candidate with finite risk metrics")
        top = [(combos[i], objective(bts[i])) for i in ranked[:max(1, len(ranked) // 4)]]
        choice = _medoid(top, ind.param_space)
        # WFE compares the selected medoid to itself, not the discarded peak.
        selected_sharpe = next(sh for c, _, sh in scored if c == choice)
        is_sharpes.append(selected_sharpe)

        # evaluate the chosen params on the untouched test segment; the signal
        # is computed on data up to test-end but scored only inside the fold
        chosen = ind.with_params(**choice)
        full = df.iloc[: t1]
        bt_sig = chosen.signal(full)
        segment = execution.evaluate(full, bt_sig, start=int(t0), stop=int(t1),
                                     cost_bps=cost_bps, mode=mode, return_series=True,
                                     initial_position=held, liquidate=f == n_folds - 1)
        held = float(segment["final_position"])
        turnover += segment["turnover"]
        oos_pnl.append(segment["returns"])
        folds.append({"fold": f + 1, "train_bars": len(train), "test_bars": len(test),
                      "chosen": choice, "is_best_sharpe": scored[0][2],
                      "is_selected_sharpe": selected_sharpe, "purge_bars": purge,
                      "train_end": str(train.index[-1]), "test_start": str(test.index[0]),
                      "test_end": str(test.index[-1]), "candidates": len(combos)})

    stitched = pd.concat(oos_pnl)
    oos_metrics = execution.metrics(stitched, turnover=turnover)
    oos_sharpe = oos_metrics["sharpe"]
    is_mean = float(np.mean(is_sharpes))
    return {"folds": folds, "oos_sharpe": oos_sharpe, "is_sharpe_mean": is_mean,
            "wfe": oos_sharpe / is_mean if is_mean > 0 else np.nan,
            "oos_total_log_ret": float(np.log1p(stitched).sum()),
            "oos_metrics": oos_metrics,
            "objective": quality.OBJECTIVE_VERSION, "execution_mode": mode,
            "final_choice": folds[-1]["chosen"]}
