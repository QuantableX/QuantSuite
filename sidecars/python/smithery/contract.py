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

# The contract's API generation. Scripts, their parameter versions and the
# store's packages name the generation they were written for; version files
# stay valid across QuantSuite builds of the same generation. 2 = parameter
# schema, REQUIRES and per-indicator versions (2026-09-23).
CONTRACT_VERSION = 2

PARAM_TYPES = ("int", "float", "bool", "choice", "list")


def _derived_entry(name: str, default, tested: tuple | None) -> dict:
    """What an editor can know about a parameter without a declaration."""
    if isinstance(default, bool):
        kind = "bool"
    elif isinstance(default, int):
        kind = "int"
    elif isinstance(default, float):
        kind = "float"
    elif isinstance(default, (tuple, list)):
        kind = "list"
    else:
        kind = "choice"
    entry = {"type": kind, "label": name.replace("_", " ").capitalize(), "help": ""}
    if kind == "int":
        entry["step"] = 1
    if kind == "choice":
        entry["choices"] = [default]
    if tested is not None:
        # The region the gauntlet perturbs: evidence exists inside it; the
        # user may still leave it (no hard bound unless the script sets one).
        entry["tested"] = [float(tested[0]), float(tested[1])]
    return entry


class TrendIndicator(ABC):
    """Causal binary trend regime classifier.

    Subclasses define:
      name        — unique registry key
      default_params() — dict of dimensionless hyper-parameters
      param_space — {param: (low, high)} region the gauntlet perturbs
      param_schema — optional {param: {type, min, max, step, choices, label,
                     help}} for editors; min / max / choices are hard bounds
      _compute(df) — returns np.ndarray of {-1, 0, +1}
    """

    name: str = "abstract"
    param_space: dict[str, tuple[float, float]] = {}
    param_schema: dict[str, dict] = {}

    def __init__(self, **overrides):
        self.params = {**self.default_params(), **overrides}

    @classmethod
    def default_params(cls) -> dict:
        return {}

    def with_params(self, **p) -> "TrendIndicator":
        return type(self)(**{**self.params, **p})

    @classmethod
    def schema(cls) -> dict[str, dict]:
        """Every parameter of the defaults with its editor description: the
        script's declaration over one derived from the default's type and the
        tested range (param_space)."""
        out = {}
        for name, default in cls.default_params().items():
            entry = {**_derived_entry(name, default, cls.param_space.get(name)),
                     **cls.param_schema.get(name, {})}
            entry["default"] = list(default) if isinstance(default, tuple) else default
            out[name] = entry
        return out

    @classmethod
    def check_params(cls, values: dict) -> list[str]:
        """Problems with a parameter override — unknown names, wrong types,
        non-finite numbers, values outside the script's declared bounds.
        Empty when the values can be used."""
        problems: list[str] = []
        schema = cls.schema()
        if not isinstance(values, dict):
            return ["parameters must be an object of name → value"]
        for name, value in values.items():
            entry = schema.get(name)
            if entry is None:
                problems.append(f"{name}: unknown parameter")
                continue
            kind = entry["type"]
            if kind == "bool":
                if not isinstance(value, bool):
                    problems.append(f"{name}: expected true or false")
                continue
            if kind == "choice":
                if value not in entry.get("choices", []):
                    problems.append(f"{name}: expected one of {entry.get('choices', [])}")
                continue
            if kind == "list":
                default = entry["default"]
                if (not isinstance(value, (list, tuple)) or len(value) != len(default)
                        or not all(isinstance(v, (int, float)) and not isinstance(v, bool)
                                   and np.isfinite(v) for v in value)):
                    problems.append(f"{name}: expected {len(default)} numbers")
                continue
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not np.isfinite(value):
                problems.append(f"{name}: expected a finite number")
                continue
            if kind == "int" and float(value) != int(value):
                problems.append(f"{name}: expected a whole number")
                continue
            lo, hi = entry.get("min"), entry.get("max")
            if lo is not None and value < lo:
                problems.append(f"{name}: at least {lo}")
            if hi is not None and value > hi:
                problems.append(f"{name}: at most {hi}")
        return problems

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
