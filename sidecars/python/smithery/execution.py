"""Execution-aware research metrics, separate from the legacy gauntlet score.

A close-derived signal can first trade at the following open. The position
held BEFORE that open receives the overnight gap. Fees/slippage are charged
per unit of changed exposure. Simple portfolio returns are compounded;
negating a price log return is not the return of a short position.
"""
from __future__ import annotations

import numpy as np
import pandas as pd

from .backtest import periods_per_year


def metrics(returns: pd.Series, *, turnover: float = 0.0) -> dict:
    if len(returns) < 2 or not np.isfinite(returns.to_numpy()).all():
        raise ValueError("At least two finite return observations are required")
    bankrupt = bool((returns <= -1).any())
    wealth = (1 + returns.clip(lower=-1)).cumprod()
    peak = wealth.cummax().clip(lower=1.0)
    ppy = periods_per_year(returns.index)
    years = len(returns) / ppy
    sd = float(returns.std())
    downside = float(np.sqrt(np.mean(np.minimum(returns.to_numpy(), 0.0) ** 2)))
    end = float(wealth.iloc[-1])
    gains = float(returns.clip(lower=0).sum())
    losses = float(-returns.clip(upper=0).sum())
    return {
        "omega": gains / losses if losses > 0 else (float("inf") if gains > 0 else 0.0),
        "bars": len(returns), "years": float(years),
        "return": end - 1.0,
        "cagr": float(end ** (1.0 / years) - 1.0),
        "sharpe": float(returns.mean() / sd * np.sqrt(ppy)) if sd > 0 else 0.0,
        "sortino": float(returns.mean() / downside * np.sqrt(ppy)) if downside > 0 else 0.0,
        "max_dd": float((wealth / peak - 1.0).min()),
        "turnover_per_year": float(turnover / years),
        "bankrupt": bankrupt,
    }


def evaluate(df: pd.DataFrame, signal: pd.Series, *, start: int = 1,
             stop: int | None = None, cost_bps: float = 20.0,
             mode: str = "long_only", return_series: bool = False,
             initial_position: float = 0.0, liquidate: bool = True) -> dict:
    """Round-trip cost in bps; the evaluation starts and ends flat.

    `signal` may include warm-up history, but only [start:stop] earns PnL.
    No funding, borrowing, leverage, volume or market-impact assumptions
    are hidden here: long_short is a diagnostic until those are supplied.
    """
    stop = len(df) if stop is None else stop
    if not (1 <= start < stop <= len(df)) or stop - start < 2:
        raise ValueError("Evaluation needs a preceding signal bar and at least two scored bars")
    if not np.isfinite(cost_bps) or not 0 <= cost_bps < 10_000:
        raise ValueError("Round-trip costs must be finite and in [0, 10000) bps")
    if mode not in ("long_only", "long_short"):
        raise ValueError(f"Unknown execution mode: {mode}")
    if not df.index.is_monotonic_increasing or not df.index.is_unique:
        raise ValueError("Candles must be ordered and unique")
    prices = df[["open", "close"]].to_numpy(dtype=float)
    if not np.isfinite(prices).all() or (prices <= 0).any():
        raise ValueError("Prices must be finite and positive")
    sig = signal.reindex(df.index)
    if sig.isna().any() or not sig.isin((-1, 0, 1)).all():
        raise ValueError("Signal must provide -1, 0 or +1 on every candle")
    target = sig.clip(lower=0) if mode == "long_only" else sig
    position = target.shift(1).iloc[start:stop].to_numpy(dtype=float)
    allowed = (0.0, 1.0) if mode == "long_only" else (-1.0, 0.0, 1.0)
    if initial_position not in allowed:
        raise ValueError("Invalid initial position")
    previous = np.r_[initial_position, position[:-1]]
    opens = df["open"].iloc[start:stop].to_numpy(dtype=float)
    closes = df["close"].iloc[start:stop].to_numpy(dtype=float)
    prior_close = df["close"].shift(1).iloc[start:stop].to_numpy(dtype=float)
    gap = opens / prior_close - 1.0
    intrabar = closes / opens - 1.0
    turns = np.abs(position - previous)
    one_way = cost_bps / 20_000.0
    gross = (1 + previous * gap) * (1 - turns * one_way) * (1 + position * intrabar)
    if liquidate:
        gross[-1] *= 1 - abs(position[-1]) * one_way
    # A short can be wiped out by a single move. It cannot recover afterwards.
    ruined = (1 + previous * gap <= 0) | (1 + position * intrabar <= 0)
    if ruined.any():
        first = int(np.flatnonzero(ruined)[0])
        gross[first] = 0.0
        gross[first + 1:] = 1.0
    pnl = pd.Series(gross - 1.0, index=df.index[start:stop])
    turnover = float(turns.sum() + (abs(position[-1]) if liquidate else 0))
    result = metrics(pnl, turnover=turnover)
    result["turnover"] = turnover
    result["final_position"] = 0.0 if liquidate or ruined.any() else float(position[-1])
    result.update(mode=mode, cost_bps=float(cost_bps),
                  start=str(pnl.index[0]), end=str(pnl.index[-1]))
    if return_series:
        result["returns"] = pnl
    return result
