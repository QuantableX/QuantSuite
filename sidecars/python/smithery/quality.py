"""Shared research selection across return, drawdown and all three ratios.

No retrospective weights: these fixed transforms are recorded in every run.
Pareto layers precede the scalar tie-breaker. Invalid/cash-only paths cannot
win by manufacturing a zero drawdown or undefined ratios.
"""
from __future__ import annotations
import math

OBJECTIVE_VERSION = "growth-risk-2026-09-13.1"
DIMENSIONS = ("cagr", "sharpe", "sortino", "omega", "max_dd")


def vector(row: dict) -> tuple:
    values = tuple(float(row[k]) for k in DIMENSIONS)
    if not all(math.isfinite(v) for v in values):
        raise ValueError("Quality metrics must all be finite")
    growth, sharpe, sortino, omega, dd = values
    if growth <= -1 or omega < 0 or abs(dd) > 1:
        raise ValueError("Invalid performance metrics")
    return growth, sharpe, sortino, omega, -abs(dd)


def score(row: dict) -> float:
    try:
        growth, sharpe, sortino, omega, neg_dd = vector(row)
    except (KeyError, TypeError, ValueError):
        return -math.inf
    if growth <= 0 or omega <= 1 or row.get("bankrupt", False):
        return -math.inf
    # Ratios are bounded to prevent one unstable denominator overwhelming
    # actual compound growth. Every desirable dimension stays monotonic.
    return (2 * math.log1p(growth)
            + math.tanh(sharpe / 2)
            + .5 * math.tanh(sortino / 3)
            + .5 * math.tanh(math.log(omega))
            + 2 * neg_dd
            - .02 * math.log1p(max(0, row.get("turnover_per_year", 0))))


def pareto_order(rows: list[dict]) -> list[int]:
    valid = [i for i, row in enumerate(rows) if math.isfinite(score(row))]
    vectors = {i: vector(rows[i]) for i in valid}
    ordered = []
    while valid:
        front = [i for i in valid if not any(
            all(a >= b for a, b in zip(vectors[j], vectors[i]))
            and any(a > b for a, b in zip(vectors[j], vectors[i]))
            for j in valid if i != j)]
        ordered.extend(sorted(front, key=lambda i: (-score(rows[i]), i)))
        valid = [i for i in valid if i not in front]
    return ordered


def select(rows: list[dict], limit: int, *, max_drawdown: float = .30,
           min_cagr: float = 0.0) -> list[int]:
    if not 0 < max_drawdown < 1:
        raise ValueError("Drawdown target must be between zero and one")
    ranked = pareto_order(rows)
    eligible = [i for i in ranked if rows[i]["cagr"] >= min_cagr]
    feasible = [i for i in eligible if abs(rows[i]["max_dd"]) <= max_drawdown]
    # Keep the remaining trade-offs inspectable when the target is unmet.
    return (feasible + [i for i in eligible if i not in feasible])[:limit]
