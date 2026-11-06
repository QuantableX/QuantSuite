"""Funds: one amount split across several Manual systems, equally or by performance.

A fund reads each member's own net daily equity and never re-simulates a trade, so it inherits exactly what the member
does, its fees included. A split uses only closes up to the rebalance day and applies after that close. No orders: this
module plans and evaluates.
"""

from __future__ import annotations

import datetime as dt
import math
from dataclasses import dataclass, field

import numpy as np
import pandas as pd

METHODS = ("equal", "fixed", "profit", "sharpe", "sortino", "return_drawdown", "inverse_volatility", "kelly", "combined")
REBALANCES = ("never", "weekly", "monthly", "quarterly")
LOOKBACKS = (30, 90, 180, 365)
#: Methods that judge performance: a member losing money over the lookback gets nothing.
PERFORMANCE = ("profit", "sharpe", "sortino", "return_drawdown", "kelly", "combined")
#: Methods with a share of their own, which the cap leaves alone.
UNCAPPED = ("equal", "fixed")
#: The optimization windows of the robustness rule (docs/ROBUSTNESS.md).
ROBUSTNESS_WINDOWS = ((2020, 2022), (2023, 2025), (2021, 2024), (2024, 2026))
DAYS_PER_YEAR = 365


@dataclass(frozen=True)
class FundRule:
    method: str = "equal"
    lookback_days: int = 180
    rebalance: str = "monthly"
    cap_pct: float = 50.0
    kelly_fraction: float = 0.25
    fixed_pct: dict[str, float] = field(default_factory=dict)
    #: Cost of moving money, per side: every dollar leaving a member and every dollar entering one pays it.
    fee_rate: float = 0.0


@dataclass(frozen=True)
class MemberStats:
    """A member over the lookback. ``kelly`` is its full Kelly fraction."""
    profit: float
    sharpe: float
    sortino: float
    max_drawdown: float
    volatility: float
    kelly: float


def stats_of(returns: np.ndarray) -> MemberStats:
    """The figures a method reads, from a member's daily simple returns."""
    r = np.asarray(returns, dtype=float)
    if not len(r):
        return MemberStats(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    curve = np.cumprod(1 + r)
    peak = np.maximum.accumulate(np.concatenate(([1.0], curve)))[1:]
    mean, std = float(r.mean()), float(r.std())
    downside = float(np.sqrt(np.mean(np.minimum(r, 0.0) ** 2)))
    year = math.sqrt(DAYS_PER_YEAR)
    return MemberStats(
        profit=float(curve[-1] - 1),
        sharpe=mean / std * year if std > 0 else 0.0,
        sortino=mean / downside * year if downside > 0 else 0.0,
        max_drawdown=float(((peak - curve) / peak).max()),
        volatility=std * year,
        kelly=mean / (std * std) if std > 0 else 0.0,
    )


def _rank_points(values: dict[str, float], high_is_good: bool) -> dict[str, float]:
    """The best gets as many points as there are members, the worst one; ties share."""
    ordered = sorted(values, key=lambda k: values[k], reverse=not high_is_good)
    points: dict[str, float] = {}
    i = 0
    while i < len(ordered):
        j = i
        while j + 1 < len(ordered) and values[ordered[j + 1]] == values[ordered[i]]:
            j += 1
        for k in ordered[i:j + 1]:
            points[k] = (i + 1 + j + 1) / 2
        i = j + 1
    return points


def _raw_weights(stats: dict[str, MemberStats], rule: FundRule) -> dict[str, float]:
    method = rule.method
    if method == "profit":
        return {k: s.profit for k, s in stats.items()}
    if method == "sharpe":
        return {k: s.sharpe for k, s in stats.items()}
    if method == "sortino":
        return {k: s.sortino for k, s in stats.items()}
    if method == "return_drawdown":
        return {k: s.profit / s.max_drawdown if s.max_drawdown > 0 else 0.0 for k, s in stats.items()}
    if method == "inverse_volatility":
        return {k: 1 / s.volatility if s.volatility > 0 else 0.0 for k, s in stats.items()}
    if method == "kelly":
        return {k: s.kelly * rule.kelly_fraction for k, s in stats.items()}
    if method == "combined":
        profit = _rank_points({k: s.profit for k, s in stats.items()}, True)
        sharpe = _rank_points({k: s.sharpe for k, s in stats.items()}, True)
        drawdown = _rank_points({k: s.max_drawdown for k, s in stats.items()}, False)
        return {k: profit[k] + sharpe[k] + drawdown[k] for k in stats}
    raise ValueError(f"Unknown fund method {method!r}.")


def _capped(weights: dict[str, float], cap: float) -> dict[str, float]:
    """No member above ``cap``: what is cut goes to the others by their share, or stays in cash when nobody can take it."""
    out = dict(weights)
    for _ in range(len(out) + 1):
        over = [k for k, v in out.items() if v > cap + 1e-12]
        if not over:
            break
        spare = sum(out[k] - cap for k in over)
        for k in over:
            out[k] = cap
        takers = {k: v for k, v in out.items() if 0 < v < cap - 1e-12}
        total = sum(takers.values())
        if total <= 0:
            break
        for k, v in takers.items():
            out[k] = v + spare * v / total
    return out


def weights_from_stats(stats: dict[str, MemberStats], rule: FundRule) -> dict[str, float]:
    """Each member's share of the fund, summing to at most 1; the rest is cash."""
    if rule.method not in METHODS:
        raise ValueError(f"Unknown fund method {rule.method!r}.")
    if not stats:
        return {}
    if rule.method == "equal":
        return {k: 1 / len(stats) for k in stats}
    if rule.method == "fixed":
        return {k: max(float(rule.fixed_pct.get(k, 0.0)), 0.0) / 100 for k in stats}
    raw = _raw_weights(stats, rule)
    if rule.method in PERFORMANCE:
        raw = {k: v if stats[k].profit > 0 and v > 0 else 0.0 for k, v in raw.items()}
    raw = {k: max(v, 0.0) for k, v in raw.items()}
    total = sum(raw.values())
    if total <= 0:
        return {k: 0.0 for k in stats}
    # Kelly below 100% in total keeps the rest in cash; above it, never leverage.
    scale = 1.0 if rule.method == "kelly" and total <= 1 else 1 / total
    weights = {k: v * scale for k, v in raw.items()}
    return _capped(weights, rule.cap_pct / 100) if rule.cap_pct < 100 else weights


def split_dollars(weights: dict[str, float], amount: float) -> dict[str, int]:
    """Whole dollars that add up exactly: the largest remainders get the leftover dollars."""
    raw = {k: amount * v for k, v in weights.items()}
    out = {k: math.floor(v + 1e-9) for k, v in raw.items()}
    left = round(sum(raw.values())) - sum(out.values())
    for k in sorted(raw, key=lambda k: raw[k] - out[k], reverse=True)[:max(left, 0)]:
        out[k] += 1
    return out


def daily_curve(equity: pd.Series) -> pd.Series:
    """A member's equity at each UTC daily close. An intraday member is read at its last bar of the day."""
    series = equity.dropna().astype(float)
    if series.empty:
        return series
    index = pd.DatetimeIndex(series.index)
    if index.tz is not None:
        index = index.tz_convert("UTC").tz_localize(None)
    series = pd.Series(series.to_numpy(), index=index)
    return series.groupby(series.index.normalize()).last()


def aligned_curves(curves: dict[str, pd.Series]) -> tuple[pd.DataFrame, str | None]:
    """The members' common window, and the member whose late start limits it."""
    daily = {k: daily_curve(c) for k, c in curves.items()}
    usable = {k: c for k, c in daily.items() if len(c)}
    if not usable:
        return pd.DataFrame(), None
    frame = pd.DataFrame(usable).sort_index().dropna()
    starts = {k: c.index[0] for k, c in usable.items()}
    latest = max(starts.values())
    limited = [k for k, s in starts.items() if s == latest]
    return frame, (limited[0] if len(limited) < len(starts) else None)


def is_rebalance_day(day: pd.Timestamp, previous: pd.Timestamp | None, rebalance: str) -> bool:
    """The first close of a new week, month or quarter."""
    if previous is None or rebalance == "never":
        return False
    if rebalance == "weekly":
        return day.isocalendar()[:2] != previous.isocalendar()[:2]
    if rebalance == "monthly":
        return (day.year, day.month) != (previous.year, previous.month)
    if rebalance == "quarterly":
        return (day.year, day.quarter) != (previous.year, previous.quarter)
    raise ValueError(f"Unknown rebalance period {rebalance!r}.")


def next_rebalance(day: dt.date, rebalance: str) -> dt.date | None:
    """The day of the next planned rebalance after ``day``, None when the fund never rebalances."""
    if rebalance == "never":
        return None
    if rebalance == "weekly":
        return day + dt.timedelta(days=7 - day.weekday())
    if rebalance == "monthly":
        return dt.date(day.year + day.month // 12, day.month % 12 + 1, 1)
    quarter_month = ((day.month - 1) // 3 + 1) * 3 + 1
    return dt.date(day.year + (quarter_month - 1) // 12, (quarter_month - 1) % 12 + 1, 1)


def split_on(returns: pd.DataFrame, at: int, rule: FundRule) -> dict[str, float]:
    """The split decided at the close of row ``at``, from that close and the lookback before it only."""
    members = list(returns.columns)
    if rule.method in UNCAPPED:
        return weights_from_stats({k: MemberStats(0, 0, 0, 0, 0, 0) for k in members}, rule)
    if at + 1 < rule.lookback_days:
        return weights_from_stats({k: MemberStats(0, 0, 0, 0, 0, 0) for k in members}, FundRule(method="equal"))
    window = returns.iloc[at + 1 - rule.lookback_days:at + 1].to_numpy()
    stats = {k: stats_of(window[:, i]) for i, k in enumerate(members)}
    return weights_from_stats(stats, rule)


@dataclass(frozen=True)
class Rebalance:
    day: pd.Timestamp
    values: dict[str, float]
    cash: float
    cost: float


@dataclass(frozen=True)
class FundRun:
    equity: pd.Series          # fund value per 1 of amount
    rebalances: list[Rebalance]
    final_weights: dict[str, float]    # what the fund holds after the last close
    target_weights: dict[str, float]   # what the rule would split into at the last close
    cost: float                # total cost of moving money, per 1 of amount


def run_fund(curves: pd.DataFrame, rule: FundRule, start: pd.Timestamp | None = None) -> FundRun:
    """The fund over ``curves`` (aligned daily member equity). From ``start`` on it holds money; bars before it only
    feed the lookback. Fees of moving money are charged when it moves."""
    if curves.empty:
        return FundRun(pd.Series(dtype=float), [], {}, {}, 0.0)
    members = list(curves.columns)
    returns = curves.pct_change().fillna(0.0)
    days = curves.index
    first = 0 if start is None else int(days.searchsorted(pd.Timestamp(start)))
    if first >= len(days):
        return FundRun(pd.Series(dtype=float), [], {}, {}, 0.0)
    # The first split comes from the bars before the fund's first day; none means Equal.
    opening = split_on(returns, first - 1, rule) if first > 0 else split_on(returns, -1, rule)
    held = np.array([opening[k] for k in members])
    cash = 1.0 - float(held.sum())
    values, rebalances = [1.0], []
    spent = 0.0
    columns = returns.to_numpy()
    for t in range(first + 1, len(days)):
        held = held * (1 + columns[t])
        total = float(held.sum()) + cash
        if is_rebalance_day(days[t], days[t - 1], rule.rebalance):
            target = split_on(returns, t, rule)
            wanted = np.array([target[k] for k in members]) * total
            moved = float(np.abs(wanted - held).sum())
            cost = moved * rule.fee_rate
            total -= cost
            spent += cost
            held = np.array([target[k] for k in members]) * total
            cash = total - float(held.sum())
            rebalances.append(Rebalance(days[t], dict(zip(members, held.tolist())), cash, cost))
        values.append(total)
    last_total = float(held.sum()) + cash
    final = {k: float(v) / last_total for k, v in zip(members, held)} if last_total > 0 else {k: 0.0 for k in members}
    return FundRun(pd.Series(values, index=days[first:], name="fund"), rebalances, final, split_on(returns, len(days) - 1, rule), spent)


def correlation(curves: pd.DataFrame) -> pd.DataFrame:
    """Between the members' daily returns."""
    return curves.pct_change().dropna().corr()


def cagr(equity: pd.Series) -> float | None:
    if len(equity) < 2 or equity.iloc[0] <= 0 or equity.iloc[-1] <= 0:
        return None
    years = (equity.index[-1] - equity.index[0]).days / 365.25
    return float((equity.iloc[-1] / equity.iloc[0]) ** (1 / years) - 1) if years > 0 else None


def max_drawdown(equity: pd.Series) -> float | None:
    if equity.empty:
        return None
    peak = equity.cummax()
    return float(((peak - equity) / peak).max())


def robustness(curves: pd.DataFrame, rule: FundRule) -> dict:
    """The chosen method against Equal: at every lookback, and over the rule's windows. Weighting is itself an
    optimization, so a result that only holds at one lookback or in one window says so here."""
    equal = FundRule(method="equal", rebalance=rule.rebalance, fee_rate=rule.fee_rate)

    def compare(run_rule, start=None, end=None):
        frame = curves if end is None else curves.loc[:end]
        mine, base = run_fund(frame, run_rule, start), run_fund(frame, equal, start)
        a, b = cagr(mine.equity), cagr(base.equity)
        return {"cagr": a, "equalCagr": b, "maxDrawdown": max_drawdown(mine.equity), "equalMaxDrawdown": max_drawdown(base.equity),
                "beatsEqual": None if a is None or b is None else a > b}

    lookbacks = []
    for days in LOOKBACKS:
        changed = FundRule(rule.method, days, rule.rebalance, rule.cap_pct, rule.kelly_fraction, rule.fixed_pct, rule.fee_rate)
        lookbacks.append({"lookbackDays": days, **compare(changed)})
    windows = []
    for first, last in ROBUSTNESS_WINDOWS:
        start, end = pd.Timestamp(first, 1, 1), pd.Timestamp(last, 12, 31)
        if curves.index[0] > start + pd.Timedelta(days=90) or curves.index[-1] < end - pd.Timedelta(days=366):
            windows.append({"window": f"{first}-{last}", "cagr": None, "equalCagr": None, "maxDrawdown": None,
                            "equalMaxDrawdown": None, "beatsEqual": None})
            continue
        windows.append({"window": f"{first}-{last}", **compare(rule, start, end)})
    return {"lookbacks": lookbacks, "windows": windows,
            "beatsEqualEverywhere": all(row["beatsEqual"] for row in lookbacks + windows if row["beatsEqual"] is not None)}
