"""TOTAL — the market-cap-weighted index of the ranked universe.

The higher filter reads the market's regime off this index: positions are
allowed only while TOTAL is bullish by the strategy's own trend signal,
otherwise the book sits in USD.

No free API serves a daily total-market-cap history and the ranking
provider's CMC snapshots are weekly, so TOTAL is rebuilt from what the
engine already holds: daily candles for every coin that was ever in the
ranked top-N and each coin's market cap at every ranking snapshot. At each
bar the members are the latest snapshot's coins that have a candle,
weighted by their snapshot market cap; the index compounds their weighted
open/high/low/close ratios to the previous close. The top-N by market cap
is most of the total market (BTC and ETH alone are the bulk of it), so the
index tracks TOTAL's regime, which is all the filter reads from it.
"""

from __future__ import annotations

from typing import Iterable

import numpy as np
import pandas as pd

from ..config import IndicatorConfig, TotalBreakoutConfig
from .signals import trend_signal
from .universe import UniverseSnapshot

_OHLC = ("open", "high", "low", "close")


def market_index(
    snapshots: Iterable[UniverseSnapshot],
    frames: dict[str, pd.DataFrame],
) -> pd.DataFrame:
    """OHLCV frame of TOTAL over the union of the frames' bars, level 1 at
    the first bar. Bars before the first snapshot use its weights (the
    warm-up stretch); a coin without a candle on a bar drops out of that
    bar and the others are renormalised. Empty when there are no frames."""

    if not frames:
        return pd.DataFrame(columns=[*_OHLC, "volume"])
    syms = list(frames)
    index = pd.DatetimeIndex(sorted(set().union(*[set(f.index) for f in frames.values()])))
    if index.empty:
        return pd.DataFrame(columns=[*_OHLC, "volume"])

    cols = {
        col: pd.DataFrame({sym: frames[sym][col].astype(float) for sym in syms}).reindex(index)
        for col in _OHLC
    }
    prev_close = cols["close"].ffill().shift(1)
    ratios = {col: cols[col] / prev_close for col in _OHLC}

    # Market caps per snapshot date, 0 for non-members; forward-filled to
    # the bars, the leading (warm-up) bars taking the first snapshot.
    rows: dict[pd.Timestamp, dict[str, float]] = {}
    for snap in snapshots:
        row = {sym: 0.0 for sym in syms}
        for coin in snap.coins:
            if coin.symbol in row and coin.market_cap and coin.market_cap > 0:
                row[coin.symbol] = float(coin.market_cap)
        if not any(row.values()):
            # No caps at all: equal weights across the snapshot's members.
            for coin in snap.coins:
                if coin.symbol in row:
                    row[coin.symbol] = 1.0
        rows[pd.Timestamp(snap.on_date)] = row
    if not rows:
        caps = pd.DataFrame(1.0, index=index, columns=syms)
    else:
        caps = pd.DataFrame.from_dict(rows, orient="index").sort_index()
        caps = caps[~caps.index.duplicated(keep="last")]
        caps = caps.reindex(index, method="ffill").bfill().fillna(0.0)

    valid = ratios["close"].notna() & np.isfinite(ratios["close"]) & (ratios["close"] > 0)
    weights = caps.where(valid, 0.0)
    total = weights.sum(axis=1)
    weights = weights.div(total.where(total > 0, np.nan), axis=0).fillna(0.0)

    def blended(col: str) -> pd.Series:
        r = (weights * ratios[col].fillna(0.0)).sum(axis=1)
        return r.where(total > 0, 1.0)

    close_ratio = blended("close")
    level = close_ratio.cumprod()
    prev_level = level.shift(1).fillna(1.0)
    out = pd.DataFrame(index=index)
    out["open"] = prev_level * blended("open")
    out["high"] = prev_level * blended("high")
    out["low"] = prev_level * blended("low")
    out["close"] = level
    # Dollar volume of the members, when the frames carry volume; the
    # volume-reading indicators need something non-zero.
    if all("volume" in frames[sym].columns for sym in syms):
        vol = pd.DataFrame({sym: frames[sym]["volume"].astype(float) for sym in syms}).reindex(index)
        out["volume"] = (weights * (vol * cols["close"]).fillna(0.0)).sum(axis=1)
    else:
        out["volume"] = 1.0
    return out


def market_gate(total: pd.DataFrame, cfg: IndicatorConfig | TotalBreakoutConfig) -> pd.Series:
    """The higher filter: 1 while TOTAL is bullish by ``cfg``'s trend
    signal, 0 otherwise (and 0 through the signal's warm-up)."""

    if total.empty:
        return pd.Series(dtype=int)
    if isinstance(cfg, TotalBreakoutConfig):
        close = total['close']
        slow = close.ewm(span=cfg.trend_length, adjust=False).mean()
        fast = close.ewm(span=cfg.exit_length, adjust=False).mean()
        prior_high = close.shift(1).rolling(cfg.entry_length).max()
        enter = (close > prior_high) & (close > slow) & (slow > slow.shift(1))
        leave = (close < fast) | (close < slow)
        state = 0
        values = []
        for bullish, bearish in zip(enter, leave):
            if bearish:
                state = 0
            elif bullish:
                state = 1
            values.append(state)
        return pd.Series(values, index=close.index, dtype=int)
    return trend_signal(total, cfg)
