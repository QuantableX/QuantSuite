"""Versioned evidence and immutable research artifacts; no automatic promotion."""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import uuid

EVALUATION_VERSION = "smithery-2026-09-09.1"
MIN_MC_COUNTS = {"perm": 120, "boot": 200, "garch": 100}


def finite_json(value):
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, dict):
        return {k: finite_json(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [finite_json(v) for v in value]
    return value


def code_fingerprint() -> str:
    root = Path(__file__).resolve().parent
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*.py")):
        if "tests" in path.parts:
            continue
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(path.read_bytes())
    from .workspace import indicators_dir
    for path in sorted(indicators_dir().rglob("*")):
        if path.is_file() and path.suffix in (".py", ".json") and "__pycache__" not in path.parts:
            digest.update(path.relative_to(indicators_dir()).as_posix().encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()


def frame_fingerprint(df) -> str:
    import pandas as pd
    return hashlib.sha256(pd.util.hash_pandas_object(df, index=True).values.tobytes()).hexdigest()


def artifact_id() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%H%M%S") + "-" + uuid.uuid4().hex[:8]


def write_json(path: Path, payload: dict) -> None:
    # Exclusive creation: two runs on the same date must never replace evidence.
    with path.open("x", encoding="utf-8") as handle:
        json.dump(finite_json(payload), handle, indent=2, allow_nan=False, default=str)


def certification_gates(scores: dict, perm_p: float | None, counts: dict,
                        *, fast: bool, coverage: bool, required_axes) -> list[str]:
    reasons = []
    if fast:
        reasons.append("Fast run: research only")
    for key, minimum in MIN_MC_COUNTS.items():
        if counts.get(key, 0) < minimum:
            reasons.append(f"{key}: at least {minimum} simulations required")
    if not coverage:
        reasons.append("Required asset / exchange coverage is incomplete")
    for axis in required_axes:
        score = scores.get(axis)
        if score is None or not math.isfinite(float(score)):
            reasons.append(f"{axis}: required check did not complete")
    if perm_p is None or not math.isfinite(perm_p) or not 0 <= perm_p <= .10:
        reasons.append("Permutation luck gate did not pass")
    return reasons
