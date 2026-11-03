"""Universal optimization: parameters chosen on a basket of assets, and how far they drift.

User rule (2026-11-03): an indicator is optimized universally, not for BTC alone, and
re-optimizing it on another window must not change it too much (docs/ROBUSTNESS.md).

* ``basket_choice`` — the plateau medoid (smithery.optimize) on the basket MEDIAN of
  every metric: a parameterization must be good on most assets, not great on one.
* ``walk_forward_universal`` — the anchored walk-forward of smithery.optimize with
  calendar folds and the basket choice; its out-of-sample result is the median over
  the assets.
* ``window_stability`` — re-optimizes on rolling calendar windows (three years,
  stepping one year) and measures how far the chosen parameters move and how they do
  in the year after each window.

Every candidate's signal is computed once per asset on the whole history (the
contract makes signals causal: a bar sees only the bars before it); windows and
folds are segments of it, scored with ``execution.evaluate``'s start/stop, so only
their own bars earn PnL.
"""
from __future__ import annotations

import math

import numpy as np
import pandas as pd

from . import execution, quality
from . import parallel as par
from .contract import TrendIndicator
from .optimize import _medoid, objective, param_grid

METRICS = ("cagr", "sharpe", "sortino", "omega", "max_dd", "turnover_per_year")


def _median_row(rows: list[dict]) -> dict:
    """The basket's median of every quality metric (invalid rows count as failures)."""
    out = {}
    for key in METRICS:
        values = [float(r.get(key, np.nan)) for r in rows]
        values = [v for v in values if math.isfinite(v)]
        out[key] = float(np.median(values)) if len(values) * 2 > len(rows) else float("nan")
    out["bankrupt"] = sum(bool(r.get("bankrupt")) for r in rows) * 2 > len(rows)
    return out


def _ts(value, tz):
    """A timestamp in the index's time zone (naive indexes get naive timestamps)."""
    t = pd.Timestamp(value)
    if tz is not None:
        return t.tz_localize(tz) if t.tz is None else t.tz_convert(tz)
    return t.tz_convert("UTC").tz_localize(None) if t.tz is not None else t


def _slice(df: pd.DataFrame, lo=None, hi=None) -> pd.DataFrame:
    idx = df.index
    mask = np.ones(len(df), dtype=bool)
    if lo is not None:
        mask &= idx >= _ts(lo, idx.tz)
    if hi is not None:
        mask &= idx < _ts(hi, idx.tz)
    return df.loc[mask]


class SignalTable:
    """Every combo's signal on every asset, computed once on the whole history."""

    def __init__(self, ind: TrendIndicator, frames: dict[str, pd.DataFrame], combos: list[dict]):
        self.ind, self.frames, self.combos = ind, frames, combos
        names = list(frames)
        tasks = [(ind.with_params(**c), frames[n]) for c in combos for n in names]
        raw = par.pmap(par.eval_signal, tasks)
        self.signals = {}
        for k, arr in enumerate(raw):
            c, n = divmod(k, len(names))
            self.signals[(c, names[n])] = par.rehydrate(arr, frames[names[n]].index)

    def segment(self, combo: int, name: str, cost: float, lo, hi, mode: str, min_bars: int = 20) -> dict | None:
        df = self.frames[name]
        start = int(np.searchsorted(df.index, _ts(lo, df.index.tz))) if lo is not None else 1
        stop = int(np.searchsorted(df.index, _ts(hi, df.index.tz))) if hi is not None else len(df)
        start = max(start, 1)
        if stop - start < min_bars:
            return None
        try:
            return execution.evaluate(df, self.signals[(combo, name)], start=start, stop=stop, cost_bps=cost, mode=mode)
        except ValueError:
            return None


def basket_choice(table: SignalTable, costs: dict[str, float], lo, hi, mode: str = "long_only",
                  min_bars: int = 120) -> tuple[int | None, dict]:
    """(index of the chosen combo, its basket-median metrics) from the plateau of the basket medians on [lo, hi)."""
    rows = []
    for c in range(len(table.combos)):
        seg = [table.segment(c, n, costs[n], lo, hi, mode, min_bars) for n in table.frames]
        seg = [s for s in seg if s is not None]
        rows.append(_median_row(seg) if seg else {k: float("nan") for k in METRICS})
    ranked = quality.pareto_order(rows)
    if not ranked:
        return None, {}
    top = [(table.combos[i], objective(rows[i])) for i in ranked[:max(1, len(ranked) // 4)]]
    choice = _medoid(top, table.ind.param_space)
    index = table.combos.index(choice)
    return index, rows[index]


def _oos(table: SignalTable, combo: int, costs, lo, hi, mode) -> dict:
    out = {}
    for name in table.frames:
        seg = table.segment(combo, name, costs[name], lo, hi, mode)
        if seg is not None:
            out[name] = seg
    return out


def _warm(ind: TrendIndicator, base: pd.DataFrame) -> tuple[int, pd.Timedelta]:
    from .indicators import REGISTRY, WARMUP_BARS
    warmup = next((WARMUP_BARS[k] for k, cls in REGISTRY.items() if type(ind) is cls), 60)
    span = (base.index[-1] - base.index[0]) / max(1, len(base) - 1)
    return warmup, span


def walk_forward_universal(ind: TrendIndicator, frames: dict[str, pd.DataFrame], costs: dict[str, float],
                           primary: str, n_folds: int = 4, mode: str = "long_only") -> dict:
    """Anchored walk-forward on calendar folds of the primary series with the basket choice."""
    if not 2 <= n_folds <= 12:
        raise ValueError("Walk-forward needs 2 to 12 folds")
    base = frames[primary]
    n = len(base)
    warmup, span = _warm(ind, base)
    purge = max(warmup, int(0.02 * n))
    test_start = max(int(n * 0.4), warmup + purge + 60)
    if n - test_start < n_folds * 30:
        raise ValueError("Not enough history for training, purge and at least 30 bars per test fold")
    bounds = np.linspace(test_start, n, n_folds + 1, dtype=int)
    table = SignalTable(ind, frames, param_grid(ind))
    train_start = base.index[0] + warmup * span
    folds, oos_sharpes, is_sharpes = [], [], []
    for f in range(n_folds):
        t0 = base.index[bounds[f]]
        t1 = base.index[bounds[f + 1] - 1] + span
        train_end = base.index[max(0, bounds[f] - purge)]
        index, metrics = basket_choice(table, costs, train_start, train_end, mode)
        if index is None:
            raise ValueError("No profitable training candidate on the basket")
        oos = _oos(table, index, costs, t0, t1, mode)
        fold_sharpe = float(np.median([m["sharpe"] for m in oos.values()])) if oos else float("nan")
        is_sharpes.append(metrics.get("sharpe", float("nan")))
        oos_sharpes.append(fold_sharpe)
        folds.append({"fold": f + 1, "chosen": table.combos[index], "train_end": str(train_end), "test_start": str(t0),
                      "test_end": str(t1), "is_basket_sharpe": metrics.get("sharpe"), "oos_basket_sharpe": fold_sharpe,
                      "assets": sorted(oos), "candidates": len(table.combos)})
    finite_oos = [v for v in oos_sharpes if math.isfinite(v)]
    finite_is = [v for v in is_sharpes if math.isfinite(v)]
    oos_sharpe = float(np.median(finite_oos)) if finite_oos else float("nan")
    is_mean = float(np.mean(finite_is)) if finite_is else float("nan")
    return {"folds": folds, "oos_sharpe": oos_sharpe, "is_sharpe_mean": is_mean,
            "wfe": oos_sharpe / is_mean if is_mean and is_mean > 0 else float("nan"),
            "objective": quality.OBJECTIVE_VERSION + "+basket-median", "execution_mode": mode,
            "final_choice": folds[-1]["chosen"], "universal": sorted(frames)}


def param_distance(a: dict, b: dict, space: dict) -> float:
    """Mean normalized distance of two parameterizations (log scale for positive ranges)."""
    parts = []
    for key, (lo, hi) in space.items():
        if key not in a or key not in b:
            continue
        x, y = float(a[key]), float(b[key])
        if lo > 0 and hi > lo:
            parts.append(abs(math.log(x / y)) / math.log(hi / lo))
        elif hi > lo:
            parts.append(abs(x - y) / (hi - lo))
    return float(np.mean(parts)) if parts else 0.0


def window_stability(ind: TrendIndicator, frames: dict[str, pd.DataFrame], costs: dict[str, float],
                     primary: str, years: int = 3, step_years: int = 1, max_windows: int = 5,
                     mode: str = "long_only") -> dict:
    """Re-optimize on rolling calendar windows; drift of the choice and the year after each window."""
    base = frames[primary]
    warmup, span = _warm(ind, base)
    first, last = base.index[0] + warmup * span, base.index[-1]
    starts = []
    s = first
    while s + pd.DateOffset(years=years + 1) <= last + pd.Timedelta(days=1):
        starts.append(s)
        s = s + pd.DateOffset(years=step_years)
    starts = starts[-max_windows:]
    if len(starts) < 2:
        return {"windows": [], "drift": float("nan"), "oos_positive": float("nan"), "oos_median_sharpe": float("nan")}
    table = SignalTable(ind, frames, param_grid(ind, n_per=3))
    windows, choices = [], []
    for s in starts:
        e = s + pd.DateOffset(years=years)
        index, metrics = basket_choice(table, costs, s, e, mode)
        if index is None:
            windows.append({"start": str(s.date()), "end": str(e.date()), "choice": None, "oos_sharpe": None})
            continue
        choice = table.combos[index]
        oos = _oos(table, index, costs, e, e + pd.DateOffset(years=1), mode)
        sh = float(np.median([m["sharpe"] for m in oos.values()])) if oos else float("nan")
        windows.append({"start": str(s.date()), "end": str(e.date()), "choice": choice,
                        "is_basket_sharpe": metrics.get("sharpe"), "oos_sharpe": sh, "oos_assets": sorted(oos)})
        choices.append(choice)
    drifts = [param_distance(a, b, ind.param_space) for a, b in zip(choices, choices[1:])]
    oos = [w["oos_sharpe"] for w in windows if w.get("oos_sharpe") is not None and math.isfinite(w["oos_sharpe"])]
    return {"windows": windows, "drift": float(np.mean(drifts)) if drifts else float("nan"),
            "oos_positive": float(np.mean([v > 0 for v in oos])) if oos else float("nan"),
            "oos_median_sharpe": float(np.median(oos)) if oos else float("nan"),
            "failed_windows": sum(w["choice"] is None for w in windows)}
