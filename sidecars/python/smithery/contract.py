"""The indicator contract: TrendIndicator ABC + mechanical law validators.

Laws (Docs/00):
  1. Causality       — signal at t depends only on bars <= t (no repainting).
  2. Scale invariance — multiplying prices by a constant leaves the signal unchanged.
  3. Adaptivity      — hyper-parameters are dimensionless; internals self-tune.
  4. Hysteresis      — flips require evidence, not sign noise.
"""
from __future__ import annotations

from abc import ABC, abstractmethod

import numpy as np
import pandas as pd

OHLCV = ("open", "high", "low", "close", "volume")


class TrendIndicator(ABC):
    """Causal binary trend regime classifier.

    Subclasses define:
      name        — unique registry key
      default_params() — dict of dimensionless hyper-parameters
      param_space — {param: (low, high)} region the gauntlet perturbs
      _compute(df) — returns np.ndarray of {-1, 0, +1}
    """

    name: str = "abstract"
    param_space: dict[str, tuple[float, float]] = {}

    def __init__(self, **overrides):
        self.params = {**self.default_params(), **overrides}

    @classmethod
    def default_params(cls) -> dict:
        return {}

    def with_params(self, **p) -> "TrendIndicator":
        return type(self)(**{**self.params, **p})

    def signal(self, df: pd.DataFrame) -> pd.Series:
        """Validate input, compute, and post-check the output range."""
        if not isinstance(df.index, pd.DatetimeIndex):
            raise TypeError(f"{self.name}: df must have a DatetimeIndex")
        missing = [c for c in OHLCV[:4] if c not in df.columns]
        if missing:
            raise ValueError(f"{self.name}: missing columns {missing}")
        if not df.index.is_monotonic_increasing or not df.index.is_unique:
            raise ValueError(f"{self.name}: timestamps must be ordered and unique")
        prices = df[list(OHLCV[:4])].to_numpy(dtype=float)
        if not np.isfinite(prices).all() or (prices <= 0).any():
            raise ValueError(f"{self.name}: OHLC prices must be finite and positive")
        if df.empty:
            return pd.Series(index=df.index, dtype=float, name=self.name)
        raw = np.asarray(self._compute(df), dtype=float)
        if raw.shape != (len(df),):
            raise ValueError(f"{self.name}: signal length mismatch")
        bad = ~np.isin(raw, (-1.0, 0.0, 1.0))
        if bad.any():
            raise ValueError(f"{self.name}: non-ternary values at {bad.sum()} bars")
        # 0 only allowed during warm-up: once nonzero, never zero again
        nz = np.flatnonzero(raw != 0)
        if len(nz) and (raw[nz[0]:] == 0).any():
            raise ValueError(f"{self.name}: signal returns to 0 after warm-up (Law: no neutral cop-out)")
        return pd.Series(raw, index=df.index, name=self.name)

    @abstractmethod
    def _compute(self, df: pd.DataFrame) -> np.ndarray: ...


# ---------------------------------------------------------------- validators

def validate_causality(ind: TrendIndicator, df: pd.DataFrame,
                       cuts: int = 4, warmup_grace: int = 0) -> tuple[bool, str]:
    """Recompute the signal on truncated prefixes; the overlapping history must
    be bit-identical, else the indicator repaints (Law 1 violation)."""
    full = ind.signal(df).to_numpy()
    n = len(df)
    prefixes = {int(n * frac) for frac in np.linspace(0.55, 0.95, cuts)}
    # Long-prefix tests miss warm-up repainting: some detectors used to
    # return all-zero for n<64 but retrospectively commit on bar 40.
    committed = np.flatnonzero(full != 0)
    if len(committed):
        first = int(committed[0]) + 1
        prefixes.update((first, first + 1, first + 5))
    for k in sorted(k for k in prefixes if 0 < k < n):
        pre = ind.signal(df.iloc[:k]).to_numpy()
        mism = int((pre[warmup_grace:] != full[warmup_grace:k]).sum())
        if mism:
            return False, f"REPAINTS: {mism} historical bars changed when truncated at {k}/{n}"
    return True, "causal (no repainting across truncation checks)"


def validate_scale_invariance(ind: TrendIndicator, df: pd.DataFrame,
                              factor: float = 1000.0) -> tuple[bool, str]:
    """Law 2: price * constant must produce an identical signal stream."""
    a = ind.signal(df).to_numpy()
    scaled = df.copy()
    for c in ("open", "high", "low", "close"):
        scaled[c] = scaled[c] * factor
    b = ind.signal(scaled).to_numpy()
    mism = int((a != b).sum())
    if mism:
        return False, f"SCALE-DEPENDENT: {mism} bars differ after x{factor:g} price scaling"
    return True, f"scale-invariant under x{factor:g}"


def hysteresis_flip(raw_score: np.ndarray, enter: float) -> np.ndarray:
    """Shared hysteresis machine: emit +1/-1 when |score| crosses `enter` with a
    sign change, otherwise HOLD the previous signal. 0 until first commitment."""
    sig = np.zeros(len(raw_score))
    cur = 0.0
    for i, s in enumerate(raw_score):
        if np.isfinite(s):
            if s > enter and cur <= 0:
                cur = 1.0
            elif s < -enter and cur >= 0:
                cur = -1.0
        sig[i] = cur
    return sig
