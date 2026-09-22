"""Vectorized ±1 backtester with a per-flip cost model and the metric suite.

The only way signals meet returns: pos = signal.shift(1). Never score unshifted.
"""
from __future__ import annotations

import numpy as np
import pandas as pd

SECONDS_PER_YEAR = 365.25 * 24 * 3600
DEFAULT_COST_BPS = {"crypto": 10.0}   # round-trip


def periods_per_year(index: pd.DatetimeIndex) -> float:
    """Empirical bars/year — robust to gaps and outages (crypto ≈ 365)."""
    span = (index[-1] - index[0]).total_seconds()
    return (len(index) - 1) * SECONDS_PER_YEAR / max(span, 1.0)


def run(df: pd.DataFrame, signal: pd.Series, cost_bps: float = 10.0) -> dict:
    """Backtest a ±1 signal. cost_bps = round-trip cost; a full flip (Δpos=2)
    pays one round-trip, entering from flat pays half."""
    ret = np.log(df["close"]).diff().fillna(0.0)
    pos = signal.reindex(df.index).ffill().fillna(0.0).shift(1).fillna(0.0)
    turns = pos.diff().abs().fillna(0.0)
    costs = turns * (cost_bps / 1e4) / 2.0
    pnl = pos * ret - costs

    ppy = periods_per_year(df.index)
    live = pnl[pos != 0]
    mu, sd = pnl.mean(), pnl.std()
    downside = pnl[pnl < 0].std()
    eq = pnl.cumsum()
    dd = eq - eq.cummax()
    bh_eq = ret.cumsum()
    bh_dd = bh_eq - bh_eq.cummax()
    years = max(len(df) / ppy, 1e-9)
    flips = float((turns > 0).sum())

    return {
        "n_bars": len(df),
        "years": years,
        "total_log_ret": float(eq.iloc[-1]),
        "cagr": float(np.expm1(eq.iloc[-1] / years)),
        "sharpe": float(mu / sd * np.sqrt(ppy)) if sd > 0 else 0.0,
        "sortino": float(mu / downside * np.sqrt(ppy)) if downside and downside > 0 else 0.0,
        "max_dd": float(np.expm1(dd.min())),
        "calmar": float(np.expm1(eq.iloc[-1] / years) / abs(np.expm1(dd.min()))) if dd.min() < 0 else 0.0,
        "hit_rate": float((live > 0).mean()) if len(live) else 0.0,
        "exposure": float((pos != 0).mean()),
        "flips_per_year": flips / years,
        "bh_sharpe": float(ret.mean() / ret.std() * np.sqrt(ppy)) if ret.std() > 0 else 0.0,
        "bh_total_log_ret": float(bh_eq.iloc[-1]),
        "bh_max_dd": float(np.expm1(bh_dd.min())),
        "t_stat": float(mu / sd * np.sqrt(len(pnl))) if sd > 0 else 0.0,
        "pnl": pnl,          # Series — consumed by MC / temporal axes
    }


def sharpe_only(df: pd.DataFrame, signal: pd.Series, cost_bps: float) -> float:
    ret = np.log(df["close"]).diff().fillna(0.0)
    pos = signal.shift(1).fillna(0.0)
    pnl = pos * ret - pos.diff().abs().fillna(0.0) * (cost_bps / 1e4) / 2.0
    sd = pnl.std()
    return float(pnl.mean() / sd * np.sqrt(periods_per_year(df.index))) if sd > 0 else 0.0
