"""Monte Carlo generators — see Docs/02 Monte Carlo Methods.

All generators return full OHLCV frames so indicators are *re-run* on the
counterfactual chart (never merely re-scored — the signal must be regenerated).
Bar geometry (gap, range, body ratios) is transplanted from real bars so
high/low-consuming indicators aren't fed degenerate data.
"""
from __future__ import annotations

import numpy as np
import pandas as pd
from scipy import optimize as sopt
from scipy import stats


# ---------------------------------------------------------------- rebuilders

def rebuild_ohlc(df: pd.DataFrame, order: np.ndarray) -> pd.DataFrame:
    """Chain source bars in a new order, preserving each bar's internal
    geometry (log gap, body, wick ratios). Index is reused from df."""
    o = np.log(df["open"].to_numpy(dtype=float))
    h = np.log(df["high"].to_numpy(dtype=float))
    l = np.log(df["low"].to_numpy(dtype=float))
    c = np.log(df["close"].to_numpy(dtype=float))
    gap = np.r_[0.0, (o - np.r_[np.nan, c[:-1]])[1:]]
    body, hi, lo = c - o, h - o, l - o

    n = len(order)
    oo = np.zeros(n); cc = np.zeros(n); hh = np.zeros(n); ll = np.zeros(n)
    prev_c = c[0]
    for t, i in enumerate(order):
        oo[t] = prev_c + gap[i]
        cc[t] = oo[t] + body[i]
        hh[t] = oo[t] + hi[i]
        ll[t] = oo[t] + lo[i]
        prev_c = cc[t]
    out = pd.DataFrame({"open": np.exp(oo), "high": np.exp(hh),
                        "low": np.exp(ll), "close": np.exp(cc),
                        "volume": df["volume"].to_numpy()[order]},
                       index=df.index[: n])
    return out


def permutation_order(n: int, rng: np.random.Generator) -> np.ndarray:
    """IID shuffle: marginal distribution preserved, ALL temporal structure
    annihilated. The luck-detector null."""
    return rng.permutation(n)


def stationary_bootstrap_order(n: int, rng: np.random.Generator,
                               avg_block: float | None = None) -> np.ndarray:
    """Politis–Romano stationary bootstrap: geometric block lengths preserve
    local dependence while shuffling the macro-arrangement of regimes."""
    if avg_block is None:
        avg_block = max(5.0, n ** (1 / 3))
    p = 1.0 / avg_block
    idx = np.empty(n, dtype=int)
    i = rng.integers(n)
    for t in range(n):
        idx[t] = i
        if rng.random() < p:
            i = rng.integers(n)          # start a new block
        else:
            i = (i + 1) % n              # continue the block (circular)
    return idx


# ---------------------------------------------------------------- GARCH world

def fit_garch_t(r: np.ndarray) -> dict:
    """MLE of GARCH(1,1) with standardized Student-t innovations, variance
    targeting for omega. Falls back to textbook crypto parameters on failure."""
    r = r[np.isfinite(r)]
    v = np.var(r)

    def nll(theta):
        a, b, nu = theta
        if a + b >= 0.999:
            return 1e9
        w = v * (1 - a - b)
        s2 = np.empty_like(r); s2[0] = v
        for t in range(1, len(r)):
            s2[t] = w + a * r[t - 1] ** 2 + b * s2[t - 1]
        z = r / np.sqrt(s2)
        scale = np.sqrt((nu - 2) / nu)
        ll = stats.t.logpdf(z / scale, df=nu) - np.log(scale) - 0.5 * np.log(s2)
        return -np.sum(ll)

    try:
        res = sopt.minimize(nll, x0=[0.08, 0.88, 6.0], method="L-BFGS-B",
                            bounds=[(0.01, 0.30), (0.50, 0.985), (2.6, 30.0)])
        a, b, nu = res.x
    except Exception:  # noqa: BLE001
        a, b, nu = 0.08, 0.88, 6.0
    return {"omega": v * (1 - a - b), "alpha": float(a), "beta": float(b),
            "nu": float(nu), "uncond_var": float(v)}


def calibrate_drift_regimes(r: np.ndarray, window: int = 63) -> dict:
    """Two-state Markov drift calibrated to the empirical rolling-drift
    distribution and the empirical persistence of drift-sign runs."""
    m = pd.Series(r).rolling(window).mean().dropna().to_numpy()
    mu_bull, mu_bear = np.quantile(m, 0.75), np.quantile(m, 0.25)
    signs = np.sign(m - np.median(m))
    runs = np.diff(np.flatnonzero(np.r_[1, np.diff(signs) != 0, 1]))
    exp_len = float(np.mean(runs)) if len(runs) else 63.0
    p_stay = 1.0 - 1.0 / max(exp_len, 2.0)
    return {"mu": (mu_bear, mu_bull), "p_stay": float(np.clip(p_stay, 0.9, 0.995))}


def garch_synthetic(df: pd.DataFrame, n_bars: int, rng: np.random.Generator,
                    garch: dict | None = None, regimes: dict | None = None) -> pd.DataFrame:
    """A plausible alternative chart: GARCH(1,1)-t vol + persistent Markov
    drift regimes + Brownian-bridge-style wick synthesis."""
    r = np.diff(np.log(df["close"].to_numpy(dtype=float)))
    g = garch or fit_garch_t(r)
    reg = regimes or calibrate_drift_regimes(r)

    scale = np.sqrt((g["nu"] - 2) / g["nu"])
    eps = stats.t.rvs(df=g["nu"], size=n_bars, random_state=rng) * scale
    s2 = np.empty(n_bars); s2[0] = g["uncond_var"]
    z = np.empty(n_bars, dtype=int); z[0] = rng.integers(2)
    rr = np.empty(n_bars)
    for t in range(n_bars):
        if t > 0:
            innov_prev = rr[t - 1] - reg["mu"][z[t - 1]]
            s2[t] = g["omega"] + g["alpha"] * innov_prev ** 2 + g["beta"] * s2[t - 1]
            z[t] = z[t - 1] if rng.random() < reg["p_stay"] else 1 - z[t - 1]
        rr[t] = reg["mu"][z[t]] + np.sqrt(s2[t]) * eps[t]

    c = np.log(float(df["close"].iloc[0])) + np.cumsum(rr)
    o = np.r_[c[0] - rr[0], c[:-1]]
    sig = np.sqrt(s2)
    wick_hi = np.abs(rng.normal(0, 0.5, n_bars)) * sig
    wick_lo = np.abs(rng.normal(0, 0.5, n_bars)) * sig
    h = np.maximum(o, c) + wick_hi
    l = np.minimum(o, c) - wick_lo
    # The synthetic chart keeps the source's bar spacing (its median step, so
    # an outage in the first bars cannot change it): the backtester
    # annualises from the index, and a daily index under hourly bars would
    # scale every Sharpe by the wrong root. Daily series give exactly the old
    # calendar-day index.
    step = pd.Series(df.index[:500]).diff().median()
    if not isinstance(step, pd.Timedelta) or pd.isna(step) or step <= pd.Timedelta(0):
        step = pd.Timedelta(days=1)
    idx = pd.date_range(df.index[0], periods=n_bars, freq=step)
    return pd.DataFrame({"open": np.exp(o), "high": np.exp(h), "low": np.exp(l),
                         "close": np.exp(c), "volume": 1.0}, index=idx)


# ---------------------------------------------------------------- noise

def noise_inject(df: pd.DataFrame, amp_frac: float, rng: np.random.Generator) -> pd.DataFrame:
    """Observed = true × exp(iid noise), amplitude = amp_frac of local per-bar
    vol — emulating bid/ask bounce, bad prints, vendor discrepancies."""
    c = np.log(df["close"].to_numpy(dtype=float))
    sig = pd.Series(np.diff(c, prepend=c[0])).rolling(63).std().bfill().to_numpy()
    eps = rng.normal(0.0, amp_frac * np.maximum(sig, 1e-9))
    out = df.copy()
    noisy = np.exp(c + eps)
    out["close"] = noisy
    out["high"] = np.maximum(df["high"].to_numpy(), noisy)
    out["low"] = np.minimum(df["low"].to_numpy(), noisy)
    return out
