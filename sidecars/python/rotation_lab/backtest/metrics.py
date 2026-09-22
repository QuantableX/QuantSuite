"""Performance metrics matching Pine's ``f_PerformanceMetrics``.

Returned in the same order as the Pine table so the GUI's layout can
match visually:

0  Mean (All Returns) %
1  Mean (Positive Returns) %
2  Mean (Negative Returns) %
3  STDEV (All Returns) %
4  STDEV (Positive Returns) %
5  STDEV (Negative Returns) %
6  Sharpe Ratio (sqrt(255) annualised)
7  Sortino Ratio
8  Omega Ratio
9  Maximum Drawdown %
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

import numpy as np
import pandas as pd


_DEFAULT_PERIODS_PER_YEAR = 255.0


@dataclass
class PerformanceMetrics:
    mean_all_pct: float = float("nan")
    mean_pos_pct: float = float("nan")
    mean_neg_pct: float = float("nan")
    stddev_all_pct: float = float("nan")
    stddev_pos_pct: float = float("nan")
    stddev_neg_pct: float = float("nan")
    sharpe: float = float("nan")
    sortino: float = float("nan")
    omega: float = float("nan")
    max_drawdown_pct: float = float("nan")
    # Pine QuantFolio's "Net Return (multiplied)" row: final equity
    # multiplier (1.0 = breakeven, 2.0 = doubled, ...).
    net_return_multiplier: float = float("nan")

    def as_row(self) -> list[float]:
        return [
            self.mean_all_pct, self.mean_pos_pct, self.mean_neg_pct,
            self.stddev_all_pct, self.stddev_pos_pct, self.stddev_neg_pct,
            self.sharpe, self.sortino, self.omega,
            self.max_drawdown_pct,
            self.net_return_multiplier,
        ]


METRIC_LABELS: tuple[str, ...] = (
    "Mean (All Returns) %",
    "Mean (Positive Returns) %",
    "Mean (Negative Returns) %",
    "STDEV (All Returns) %",
    "STDEV (Positive Returns) %",
    "STDEV (Negative Returns) %",
    "Sharpe Ratio",
    "Sortino Ratio",
    "Omega Ratio",
    "Maximum Drawdown %",
    "Net Return (multiplier)",
)


def max_drawdown(equity: pd.Series) -> float:
    """Return max drawdown as a non-negative fraction (0..1)."""

    if equity.empty:
        return float("nan")
    peak = equity.cummax()
    dd = (peak - equity) / peak
    return float(dd.max() or 0.0)


def compute_metrics(
    equity: pd.Series,
    periods_per_year: float = _DEFAULT_PERIODS_PER_YEAR,
) -> PerformanceMetrics:
    """Compute Pine-style metrics from an equity curve.

    ``periods_per_year`` sets the Sharpe / Sortino annualisation factor
    (``sqrt(periods_per_year)``): 255 for daily bars, calendar bar counts
    for intraday timeframes (e.g. 365*24 for 1h).
    """

    annualisation = math.sqrt(max(periods_per_year, 1.0))
    equity = equity.dropna()
    if equity.size < 2:
        # Even with too few bars we can still report the final equity
        # value if there is one (matches Pine's behaviour of showing
        # the multiplier as soon as the strategy has at least one bar).
        net = float(equity.iloc[-1]) if equity.size else float("nan")
        return PerformanceMetrics(net_return_multiplier=net)

    # Pine's f_PerformanceMetrics walks `(base[i] - base[i+1])/base[i+1]`
    # which is the simple pct_change forward. We approximate the same
    # with pandas pct_change.
    returns = equity.pct_change().dropna()
    if returns.empty:
        return PerformanceMetrics()

    pos = returns[returns > 0]
    neg = returns[returns < 0]

    mean_all = returns.mean()
    mean_pos = pos.mean() if not pos.empty else float("nan")
    mean_neg = neg.mean() if not neg.empty else float("nan")

    # Pine uses biased population stdev (divide by N), pandas std() is sample
    # by default. Match by passing ddof=0.
    std_all = returns.std(ddof=0)
    std_pos = pos.std(ddof=0) if not pos.empty else float("nan")
    std_neg = neg.std(ddof=0) if not neg.empty else float("nan")

    sharpe = (mean_all / std_all) * annualisation if std_all and not np.isnan(std_all) else float("nan")
    sortino = (mean_all / std_neg) * annualisation if std_neg and not np.isnan(std_neg) else float("nan")
    omega = (pos.sum() / abs(neg.sum())) if neg.size and abs(neg.sum()) > 0 else float("nan")
    mdd = max_drawdown(equity)

    return PerformanceMetrics(
        mean_all_pct=mean_all * 100,
        mean_pos_pct=mean_pos * 100,
        mean_neg_pct=mean_neg * 100,
        stddev_all_pct=std_all * 100,
        stddev_pos_pct=std_pos * 100,
        stddev_neg_pct=std_neg * 100,
        sharpe=sharpe,
        sortino=sortino,
        omega=omega,
        max_drawdown_pct=mdd * 100,
        net_return_multiplier=float(equity.iloc[-1]),
    )
