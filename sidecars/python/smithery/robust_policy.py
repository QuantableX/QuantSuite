"""The robustness policy shared by every optimization in the suite — docs/ROBUSTNESS.md.

User rule (2026-11-03): an indicator, an aggregate, a system or a strategy is only
good when a small change does not break it. One policy, used by the System
Gauntlet (smithery.system_gauntlet), the Indicator Gauntlet (smithery.robustness)
and the aggregate search (smithery.lab_aggregates).

The yardstick is the CAGR a variant keeps: ``retention = CAGR(variant) / CAGR(base)``.
CAGR, not the final multiple: a multiple ratio shrinks with every extra year of
history, a CAGR ratio says the same thing over two years and over seven. On
2026-11-03 random 27-indicator aggregates kept 0.8–1.2 of their CAGR under a
one-day delay, ±0.03% price noise or another exchange; LCES Rank 1 kept 0.39–0.57.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

POLICY_VERSION = "robust-2026.11.1"

# retention at which a group's score is 0 and 100 (linear between)
SCORE_ZERO, SCORE_FULL = 0.4, 0.9


@dataclass(frozen=True)
class Gate:
    """When a group of variants breaks the system.

    ``median``: the group's median retention must reach it. ``minimum``: every
    variant must reach it (None: no per-variant floor). ``flips``: the share of
    variants allowed to lose money while the base makes money."""

    median: float | None = 0.5
    minimum: float | None = None
    flips: float = 0.0


# Every group of the System Gauntlet, its axis, what it varies and when it fails.
# A group with gate None is measured and scored but never fails a system alone.
GROUPS: dict[str, dict] = {
    # 1 — parameters: a change of a setting must not change the result too much
    "parameter_jitter": {"axis": "parameters", "gate": Gate(0.5, 0.25, 0.1),
                         "what": "every member's numeric parameters ×(1 ± 15%) at random, all at once"},
    "member_drop": {"axis": "parameters", "gate": Gate(0.5, 0.2, 0.0), "min_members": 6,
                    "what": "each member of an aggregate left out once (gated from 6 members: in a vote of three each voice is a third)"},
    "member_add": {"axis": "parameters", "gate": Gate(0.5, None, 0.1), "min_members": 6,
                   "what": "one more indicator from the library added to the aggregate"},
    "system_settings": {"axis": "parameters", "gate": Gate(0.5, 0.25, 0.0),
                        "what": "the system's own settings: top N ±1, market-filter lengths ±20%"},
    # 2 — time: other windows give other gains, but must not break it
    "start_shift": {"axis": "time", "gate": Gate(0.5, 0.3, 0.0),
                    "what": "the backtest starting 1–11 months later"},
    "eras": {"axis": "time", "gate": None,
             "what": "calendar years, rolling 1-year windows and the worst rolling 2-year CAGR"},
    # 3 — exchange and data: other candles must not turn a winner into a loser
    "price_noise": {"axis": "exchange", "gate": Gate(0.6, 0.5, 0.0),
                    "what": "every open/high/low/close ×(1 + N(0, 0.03%)), three seeds"},
    "venue": {"axis": "exchange", "gate": Gate(0.5, 0.5, 0.0),
              "what": "another exchange's candle on every day it has one (Binance, Gate.io and MEXC never as the other venue)"},
    # 4 — assets: a universal system works on neighbouring universes or assets
    "universe": {"axis": "assets", "gate": Gate(0.4, None, 0.0),
                 "what": "top N −2 and +2, without the largest coin; other assets for a single-asset system"},
    # 5 — Monte Carlo: luck and path risk
    "permutation": {"axis": "monte_carlo", "gate": None,
                    "what": "calendar days shuffled for every coin at once (luck test); p above 0.10 caps the grade at B"},
    "bootstrap": {"axis": "monte_carlo", "gate": None,
                  "what": "stationary block bootstrap of the system's daily returns: drawdown, 1- and 3-year loss risk"},
    # 6 — execution: what live trading changes
    "delay": {"axis": "execution", "gate": Gate(0.5, None, 0.0),
              "what": "every trade one day later (two days shown, not gated)"},
    "fees": {"axis": "execution", "gate": Gate(0.5, None, 0.0),
             "what": "2× the fee (3× shown, not gated)"},
}
AXES = ("parameters", "time", "exchange", "assets", "monte_carlo", "execution")
AXIS_LABELS = {"parameters": "Parameters", "time": "Time", "exchange": "Exchange and data",
               "assets": "Assets and universe", "monte_carlo": "Monte Carlo", "execution": "Execution"}
# Hard limits outside the retention groups
WORST_2Y_CAGR = -0.25          # eras: a rolling 2-year window below −25% a year breaks it
BOOTSTRAP_3Y_LOSS = 0.25       # bootstrap: more than a 25% chance of a 3-year loss breaks it
PERMUTATION_P = 0.10           # permutation: p above it caps the grade at B


def cagr(multiple: float, years: float) -> float:
    """Compound annual growth of a final multiple (−1 for a total loss)."""
    if not (multiple == multiple) or years <= 0:
        return float("nan")
    if multiple <= 0:
        return -1.0
    return float(multiple ** (1.0 / years) - 1.0)


def retention(base_cagr: float, variant_cagr: float) -> float:
    """Share of the base CAGR a variant keeps (NaN when the base makes no money)."""
    if not (base_cagr == base_cagr) or base_cagr <= 0 or not (variant_cagr == variant_cagr):
        return float("nan")
    return float(variant_cagr / base_cagr)


def score_of(value: float) -> float:
    """0-100 from a retention: 0 at 0.4, 100 at 0.9."""
    if not (value == value):
        return float("nan")
    return float(100 * np.clip((value - SCORE_ZERO) / (SCORE_FULL - SCORE_ZERO), 0, 1))


def judge(group: str, base_cagr: float, variant_cagrs: list[float], members: int | None = None) -> dict:
    """A group's retention summary, score and gate verdict."""
    spec = GROUPS[group]
    values = [retention(base_cagr, c) for c in variant_cagrs]
    finite = [v for v in values if v == v]
    out = {"group": group, "axis": spec["axis"], "n": len(values), "retention": values,
           "median": float(np.median(finite)) if finite else float("nan"),
           "min": float(min(finite)) if finite else float("nan"),
           "flips": float(np.mean([c < 0 for c in variant_cagrs])) if variant_cagrs and base_cagr > 0 else 0.0}
    out["score"] = score_of(out["median"])
    gate = spec.get("gate")
    applies = members is None or members >= spec.get("min_members", 0)
    if not applies:
        out["score"] = float("nan")  # shown, not scored: one voice of a small vote is expected to matter
        out["note"] = f"shown only: the gate applies from {spec['min_members']} members"
    gated = gate is not None and applies
    reasons = []
    if gated and finite:
        if gate.median is not None and out["median"] < gate.median:
            reasons.append(f"median retention {out['median']:.2f} < {gate.median}")
        if gate.minimum is not None and out["min"] < gate.minimum:
            reasons.append(f"worst retention {out['min']:.2f} < {gate.minimum}")
        if out["flips"] > gate.flips:
            reasons.append(f"{out['flips']:.0%} of variants lose money (allowed {gate.flips:.0%})")
    out["gated"] = bool(gated)
    out["failed"] = bool(reasons)
    out["reasons"] = reasons
    return out


def grade(score: float, perm_p: float | None) -> str:
    """The Indicator Gauntlet's bands (Docs/01); a permutation p above the gate caps S/A at B."""
    if not (score == score):
        return "F"
    g = "S" if score >= 85 else "A" if score >= 70 else "B" if score >= 55 else "C" if score >= 40 else "F"
    if perm_p is not None and perm_p > PERMUTATION_P and g in ("S", "A"):
        g = "B"
    return g


def verdict(axis_scores: dict[str, float], failures: list[str], perm_p: float | None,
            complete: bool, base_cagr: float) -> dict:
    """The overall verdict: robust only with no failed gate, grade B or better and every axis measured."""
    finite = [v for v in axis_scores.values() if v == v]
    score = float(np.mean(finite)) if finite else float("nan")
    g = grade(score, perm_p)
    if not (base_cagr == base_cagr) or base_cagr <= 0:
        failures = ["the base backtest makes no money: there is no edge to protect", *failures]
    robust = not failures and g in ("S", "A", "B") and complete
    status = "robust" if robust else "not robust" if failures or g not in ("S", "A", "B") else "incomplete"
    return {"score": score, "grade": g, "robust": robust, "status": status, "failures": failures,
            "complete": complete, "perm_p": perm_p, "policy": POLICY_VERSION}


def years_between(index) -> float:
    """Calendar years spanned by a daily index (first to last bar)."""
    if len(index) < 2:
        return 0.0
    return max((index[-1] - index[0]).days, 1) / 365.25


def safe_float(value) -> float | None:
    """JSON-safe float (None for NaN/inf)."""
    try:
        value = float(value)
    except (TypeError, ValueError):
        return None
    return value if math.isfinite(value) else None


def jsonable(value):
    """Results as strict JSON: NaN and infinities become None, numpy scalars plain numbers."""
    if isinstance(value, dict):
        return {str(k): jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [jsonable(v) for v in value]
    if isinstance(value, (bool, np.bool_)):
        return bool(value)
    if isinstance(value, (int, np.integer)):
        return int(value)
    if isinstance(value, (float, np.floating)):
        return safe_float(value)
    return value
