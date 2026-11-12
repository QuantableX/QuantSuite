"""Versioned evidence and immutable research artifacts; no automatic promotion."""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import uuid

# 2026-11-03: six axes (optimization windows), universal optimization and hard gates
# (docs/ROBUSTNESS.md). Older verdicts are history, not certifications under this policy.
EVALUATION_VERSION = "smithery-2026-11-03.1"
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


_EVALUATOR: dict[tuple, str] = {}
_CLOSURE: dict[tuple, list] = {}
GAUNTLET_MODULE = "robustness"


def evaluator_files() -> list[Path]:
    """The Gauntlet's own code: robustness.py and every smithery module it imports, followed
    through relative imports (function-level ones included). A research report, the lab or the
    aggregate search is not part of it, so editing those leaves every certification standing."""
    import ast
    root = Path(__file__).resolve().parent

    def source(module: str) -> Path | None:
        for path in (root / f"{module.replace('.', '/')}.py", root / module.replace(".", "/") / "__init__.py"):
            if path.is_file():
                return path
        return None

    seen: dict[str, Path] = {}
    todo = [GAUNTLET_MODULE]
    while todo:
        module = todo.pop()
        path = source(module)
        if module in seen or path is None:
            continue
        seen[module] = path
        package = module.split(".") if path.name == "__init__.py" else module.split(".")[:-1]
        for node in ast.walk(ast.parse(path.read_bytes())):
            if not isinstance(node, ast.ImportFrom) or not node.level:
                continue
            base = package[:len(package) - node.level + 1] if node.level > 1 else package
            target = [*base, *(node.module.split(".") if node.module else [])]
            if node.module:
                todo.append(".".join(target))
            todo.extend(".".join([*target, alias.name]) for alias in node.names)
    return sorted(seen.values())


def evaluator_fingerprint() -> str:
    """The Gauntlet's own code (evaluator_files, without the library):
    a change to the evaluator invalidates every indicator's current runs."""
    root = Path(__file__).resolve().parent
    # The import closure is parsed again only when a module of the package changed: parsing it
    # took ~0.4 s, and a pass over the reports asks once per indicator (minutes per describe_all).
    package = tuple((str(p), s.st_mtime_ns, s.st_size) for p in sorted(root.rglob("*.py"))
                    if "__pycache__" not in p.parts for s in (p.stat(),))
    files = _CLOSURE.get(package)
    if files is None:
        files = evaluator_files()
        _CLOSURE.clear()
        _CLOSURE[package] = files
    state = tuple((str(p), p.stat().st_mtime_ns, p.stat().st_size) for p in files)
    if state not in _EVALUATOR:
        digest = hashlib.sha256()
        for path in files:
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(path.read_bytes())
        _EVALUATOR.clear()
        _EVALUATOR[state] = digest.hexdigest()
    return _EVALUATOR[state]


def indicator_fingerprint(cls, evaluator: str | None = None) -> str | None:
    """What a gauntlet run of one indicator depends on: the evaluator and the
    signature of the indicator's own script (with its REQUIRES closure) —
    editing or installing another script leaves it untouched. None when the
    class does not come from the library."""
    from .variants import script_signature, stem_of
    stem = stem_of(cls)
    signature = script_signature(stem) if stem else None
    if signature is None:
        return None
    return hashlib.sha256(f"{evaluator or evaluator_fingerprint()}:{signature}".encode()).hexdigest()


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
