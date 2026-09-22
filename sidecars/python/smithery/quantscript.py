"""QuantScript's command line — what the QuantView module runs
(docs/PLAN-QUANTSCRIPT.md). The scripts are the modules of
``smithery.indicators``: this file lists them with what the registry knows
about every class in them, checks a candidate version of one WITHOUT touching
the package — the check runs in a sandbox copy, so a broken script never
lands in the registry that every bot and every backtest imports — and renders
the template a new script starts from.

    python -m smithery.quantscript list
    python -m smithery.quantscript check --file x.py --candidate <path> [--depth quick|full]
    python -m smithery.quantscript lint --candidate <path>
    python -m smithery.quantscript template --key mykey --class MyTrend --name MyTrend
    python -m smithery.quantscript verify --file x.py [--depth quick|full]   # inside the sandbox

Every command prints one JSON document on its last stdout line. ``check``
spawns ``verify`` in the sandbox with the same interpreter; ``verify`` is
what imports the (copied) registry and runs the contract's validators.
``lint`` never imports anything — it is the editor's diagnostics while the
user types, and has to answer in the time an interpreter takes to start.
"""
from __future__ import annotations

import argparse
import ast
import datetime as dt
import hashlib
import importlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import traceback
import warnings
from pathlib import Path

PACKAGE_DIR = Path(__file__).resolve().parent
from .workspace import ensure_indicators
INDICATORS_DIR = ensure_indicators()
REGISTRY_FILE = "__init__.py"

# Files a script may be called: one path segment, `.py`, no leading dot.
SCRIPT_NAME = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_\-]*\.py$")
KEY_PATTERN = re.compile(r"^[a-z][a-z0-9_]{0,39}$")
CLASS_PATTERN = re.compile(r"^[A-Z][A-Za-z0-9_]{0,63}$")

# Read-only files the editor shows next to the scripts: the contract every
# script obeys. Never written by QuantScript.
REFERENCE_FILES = ("contract.py",)

DEPTHS = ("quick", "full")
CHECK_TIMEOUT_S = 300.0


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


# ---------------------------------------------------------------- files

def file_entry(path: Path) -> dict:
    stat = path.stat()
    data = path.read_bytes()
    return {
        "file": path.name,
        "stem": path.stem,
        "path": str(path),
        "size": stat.st_size,
        "modified": dt.datetime.fromtimestamp(stat.st_mtime, dt.timezone.utc).isoformat(timespec="seconds"),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def module_outline(text: str) -> tuple[str, list[dict]]:
    """The first paragraph of the module docstring and every top-level class
    with its line and bases — from the AST, so a file whose import fails
    still has an outline."""
    tree = ast.parse(text)
    doc = ast.get_docstring(tree) or ""
    summary = re.sub(r"\s+", " ", doc.split("\n\n")[0]).strip()
    classes = [
        {"class_name": node.name, "line": node.lineno, "bases": [ast.unparse(b) for b in node.bases]}
        for node in tree.body
        if isinstance(node, ast.ClassDef)
    ]
    return summary, classes


def script_name(file: str) -> str:
    if not SCRIPT_NAME.match(file):
        raise ValueError(f"'{file}' is not a script name (one `.py` file name, no path)")
    return file


# ---------------------------------------------------------------- list

def _registry():
    """The registry and its describer, or the error that kept them from
    importing — a syntax error in one module must not hide the whole listing
    from the editor that exists to fix it."""
    try:
        from .indicators import DISCOVERED, DISCOVERY_ERRORS, REGISTRY
        from .registry import certified_keys, describe
    except Exception as e:  # noqa: BLE001 — reported, never raised
        return None, None, {}, {}, [], f"{type(e).__name__}: {e}"
    return REGISTRY, describe, dict(DISCOVERY_ERRORS), dict(DISCOVERED), certified_keys(), None


def listing() -> dict:
    from . import __version__

    registry, describe, discovery_errors, discovered, certified, registry_error = _registry()
    by_class: dict[tuple[str, str], list[str]] = {}
    for key, cls in (registry or {}).items():
        by_class.setdefault((cls.__module__, cls.__name__), []).append(key)

    scripts = []
    paths = [p for p in INDICATORS_DIR.glob("*.py") if p.name != REGISTRY_FILE]
    paths.append(PACKAGE_DIR / "indicators" / REGISTRY_FILE)
    for path in sorted(paths):
        stem = path.stem
        if stem.startswith("_") and path.name != REGISTRY_FILE:
            continue
        entry = file_entry(path)
        entry["kind"] = "registry" if path.name == REGISTRY_FILE else "script"
        entry["editable"] = path.name != REGISTRY_FILE
        try:
            summary, classes = module_outline(path.read_text(encoding="utf-8"))
            entry["syntax_error"] = None
        except SyntaxError as e:
            summary, classes = "", []
            entry["syntax_error"] = f"line {e.lineno}: {e.msg}"
        except UnicodeDecodeError as e:
            summary, classes = "", []
            entry["syntax_error"] = f"not UTF-8: {e.reason}"
        entry["summary"] = summary
        modname = "smithery.indicators" if path.name == REGISTRY_FILE else f"smithery.indicators.{stem}"
        rows = []
        for cls in classes:
            keys = by_class.get((modname, cls["class_name"]), [])
            if not keys:
                rows.append({**cls, "key": None})
                continue
            for key in keys:
                row = {**cls, "key": key}
                try:
                    d = describe(key)
                    row.update({k: d[k] for k in ("name", "hypothesis", "params", "param_space",
                                                  "warmup_bars", "certification", "timeframes")})
                except Exception as e:  # noqa: BLE001 — one odd class must not hide the file
                    row["error"] = f"{type(e).__name__}: {e}"
                rows.append(row)
        entry["classes"] = rows
        entry["registered"] = sum(1 for r in rows if r["key"])
        entry["discovery_error"] = discovery_errors.get(stem)
        # How the registry knows the file: through the explicit imports of
        # __init__ (the forge's own modules — never deleted from here),
        # through REGISTER (the user's scripts — theirs to delete), or not
        # at all. Unknown while the registry cannot be read.
        entry["registration"] = (
            "unknown" if registry_error else
            "discovered" if stem in discovered else
            "explicit" if entry["registered"] else
            "none"
        )
        scripts.append(entry)

    reference = []
    for name in REFERENCE_FILES:
        path = PACKAGE_DIR / name
        if not path.is_file():
            continue
        entry = file_entry(path)
        try:
            summary, classes = module_outline(path.read_text(encoding="utf-8"))
        except (SyntaxError, UnicodeDecodeError):
            summary, classes = "", []
        entry.update({"kind": "reference", "editable": False, "summary": summary, "syntax_error": None,
                      "classes": [{**c, "key": None} for c in classes], "registered": 0, "discovery_error": None,
                      "registration": "none"})
        reference.append(entry)

    return {
        "generated_at": utc_now(),
        "version": __version__,
        "python": sys.version.split()[0],
        "executable": sys.executable,
        "package_dir": str(PACKAGE_DIR),
        "indicators_dir": str(INDICATORS_DIR),
        "registry_error": registry_error,
        "registry_keys": len(registry or {}),
        "certified": list(certified),
        "discovery_errors": discovery_errors,
        "scripts": scripts,
        "reference": reference,
    }


# ---------------------------------------------------------------- check (the sandbox)

def _last_json(text: str) -> dict | None:
    for line in reversed(text.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                continue
    return None


def check(file: str, candidate: Path, depth: str = "quick", frame_bars: int | None = None,
          timeout: float = CHECK_TIMEOUT_S) -> dict:
    """Verify a candidate version of a script without touching the package.

    Syntax first (a precise line and column, no process needed); then the
    whole package is copied to a temporary directory, the candidate is put
    in that copy, and ``verify`` runs there in a fresh interpreter. The
    result says what would happen if the candidate were saved: ``blocking``
    is what must not be saved — a syntax error, an import that fails (the
    registry would die for every consumer), a script that cannot register.
    Failed contract laws are reported and left to the author.
    """
    t0 = time.time()
    name = script_name(file)
    if depth not in DEPTHS:
        raise ValueError(f"depth must be one of {DEPTHS}")
    text = Path(candidate).read_text(encoding="utf-8")
    result: dict = {
        "file": name,
        "depth": depth,
        "syntax": {"ok": True},
        "import": {"ok": True},
        "discovery_errors": {},
        "indicators": [],
        "registry_keys": None,
        "sandbox": None,
        "stderr": "",
        "ok": False,
        "blocking": True,
    }
    try:
        ast.parse(text, filename=name)
    except SyntaxError as e:
        result["syntax"] = {"ok": False, "line": e.lineno, "column": e.offset, "message": e.msg,
                            "text": (e.text or "").rstrip()}
        result["elapsed_s"] = round(time.time() - t0, 2)
        return result

    with tempfile.TemporaryDirectory(prefix="quantscript-") as tmp:
        sandbox = Path(tmp) / "smithery"
        shutil.copytree(PACKAGE_DIR, sandbox, ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "tests"))
        # Verify the user\'s actual scripts, including dependencies and deletions.
        shutil.rmtree(sandbox / "indicators")
        shutil.copytree(INDICATORS_DIR, sandbox / "indicators", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        for internal in ("__init__.py", "_discover.py"):
            shutil.copy2(PACKAGE_DIR / "indicators" / internal, sandbox / "indicators" / internal)
        (sandbox / "indicators" / name).write_text(text, encoding="utf-8")
        env = {**os.environ,
               "PYTHONPATH": tmp + (os.pathsep + os.environ["PYTHONPATH"] if os.environ.get("PYTHONPATH") else ""),
               "QUANTSCRIPT_INDICATORS_DIR": str(sandbox / "indicators"),
               "PYTHONDONTWRITEBYTECODE": "1",
               "PYTHONIOENCODING": "utf-8"}
        cmd = [sys.executable, "-m", "smithery.quantscript", "verify", "--file", name, "--depth", depth]
        if frame_bars:
            cmd += ["--frame-bars", str(int(frame_bars))]
        try:
            proc = subprocess.run(cmd, cwd=tmp, env=env, capture_output=True, text=True,
                                  encoding="utf-8", errors="replace", timeout=timeout)
        except subprocess.TimeoutExpired:
            result["import"] = {"ok": False, "message": f"the check did not finish within {timeout:.0f}s"}
            result["elapsed_s"] = round(time.time() - t0, 2)
            return result
        doc = _last_json(proc.stdout)
        result["stderr"] = proc.stderr[-4000:] if proc.stderr.strip() else ""
        if doc is None:
            result["import"] = {"ok": False,
                                "message": "the sandbox printed no verdict (exit code %s)" % proc.returncode,
                                "traceback": proc.stderr[-4000:]}
            result["elapsed_s"] = round(time.time() - t0, 2)
            return result
        for k in ("import", "discovery_errors", "indicators", "registry_keys", "sandbox"):
            if k in doc:
                result[k] = doc[k]

    stem = name[:-3]
    own_error = result["discovery_errors"].get(stem)
    result["blocking"] = not result["import"]["ok"] or own_error is not None
    result["ok"] = not result["blocking"] and all(i.get("ok") for i in result["indicators"])
    result["elapsed_s"] = round(time.time() - t0, 2)
    return result


# ---------------------------------------------------------------- verify (inside the sandbox)

def synthetic_frame(bars: int, seed: int = 7):
    """A deterministic daily OHLCV frame with alternating drift regimes —
    long enough for a rule to warm up, trending enough for one to commit."""
    import numpy as np
    import pandas as pd

    rng = np.random.default_rng(seed)
    regime = 120
    drift = np.repeat(np.resize([0.003, -0.003, 0.002, -0.0025], bars // regime + 1), regime)[:bars]
    r = drift + rng.normal(0, 0.02, bars)
    close = 100.0 * np.exp(np.cumsum(r))
    open_ = np.concatenate([[100.0], close[:-1]]) * np.exp(rng.normal(0, 0.003, bars))
    wick = np.abs(rng.normal(0, 0.01, bars))
    high = np.maximum(open_, close) * (1 + wick)
    low = np.minimum(open_, close) * (1 - wick)
    volume = np.exp(rng.normal(10, 0.5, bars))
    index = pd.date_range("2015-01-01", periods=bars, freq="D", tz="UTC")
    return pd.DataFrame({"open": open_, "high": high, "low": low, "close": close, "volume": volume}, index=index)


def verify(file: str, depth: str = "quick", frame_bars: int | None = None) -> dict:
    """Runs where ``smithery`` resolves — the sandbox when ``check`` spawned
    it. Imports the registry, then puts every registered class of the file
    through the contract: one ``signal()`` pass (ternary, no neutral
    cop-out, commits within the frame) and, at full depth, the causality
    and scale-invariance validators."""
    import numpy as np

    out: dict = {"sandbox": str(PACKAGE_DIR), "import": {"ok": True}, "discovery_errors": {},
                 "registry_keys": 0, "indicators": []}
    try:
        from .indicators import DISCOVERY_ERRORS, REGISTRY, WARMUP_BARS
    except Exception as e:  # noqa: BLE001
        out["import"] = {"ok": False, "message": f"{type(e).__name__}: {e}", "traceback": traceback.format_exc()}
        return out
    out["discovery_errors"] = dict(DISCOVERY_ERRORS)
    out["registry_keys"] = len(REGISTRY)

    name = script_name(file)
    if name == REGISTRY_FILE:
        return out
    stem = name[:-3]
    modname = f"{__package__}.indicators.{stem}"
    if modname not in sys.modules and stem not in DISCOVERY_ERRORS:
        # A helper module, or a script without REGISTER: it still has to import.
        try:
            importlib.import_module(modname)
        except Exception as e:  # noqa: BLE001
            out["import"] = {"ok": False, "message": f"{type(e).__name__}: {e}", "traceback": traceback.format_exc()}
            return out

    from .contract import validate_causality, validate_scale_invariance

    keys = [k for k, cls in REGISTRY.items() if cls.__module__ == modname]
    for key in keys:
        cls = REGISTRY[key]
        t0 = time.time()
        row: dict = {"key": key, "class_name": cls.__name__, "name": getattr(cls, "name", cls.__name__),
                     "warmup_bars": int(WARMUP_BARS.get(key, 400)), "checks": [], "ok": True,
                     "committed_at": None, "bars": None, "error": None}
        try:
            ind = cls()
            bars = int(frame_bars) if frame_bars else min(2500, max(800, row["warmup_bars"] + 300))
            df = synthetic_frame(bars)
            row["bars"] = bars
            sig = ind.signal(df).to_numpy()
            nz = np.flatnonzero(sig != 0)
            committed = int(nz[0]) if len(nz) else None
            row["committed_at"] = committed
            if committed is None:
                row["checks"].append({"law": "contract", "ok": True, "warning": True,
                                      "message": f"no ±1 within {bars} synthetic bars (warm-up {row['warmup_bars']})"})
            else:
                late = committed > row["warmup_bars"]
                row["checks"].append({"law": "contract", "ok": True, "warning": late,
                                      "message": f"ternary, commits at bar {committed}"
                                      + (f" — later than the declared warm-up of {row['warmup_bars']}" if late else "")})
            if depth == "full":
                for law, fn in (("causality", validate_causality), ("scale", validate_scale_invariance)):
                    ok, message = fn(ind, df)
                    row["checks"].append({"law": law, "ok": bool(ok), "warning": False, "message": message})
        except Exception as e:  # noqa: BLE001 — the contract raises on violations
            row["ok"] = False
            row["error"] = f"{type(e).__name__}: {e}"
            row["traceback"] = traceback.format_exc()[-3000:]
        row["ok"] = row["ok"] and all(c["ok"] for c in row["checks"])
        row["elapsed_s"] = round(time.time() - t0, 2)
        out["indicators"].append(row)
    return out


# ---------------------------------------------------------------- lint

# Base classes a script's indicator may inherit from; a class built on one
# of them is held to the contract's shape.
CONTRACT_BASES = ("TrendIndicator", "CheckedTrend")


def _marker(severity: str, line, column, message: str, end_line=None, end_column=None) -> dict:
    return {
        "severity": severity,
        "line": int(line or 1),
        "column": int(column or 1),
        "end_line": int(end_line) if end_line else None,
        "end_column": int(end_column) if end_column else None,
        "message": message,
        "source": "python",
    }


def _contract_hints(tree: ast.Module) -> list[dict]:
    """What the parser cannot see but the contract will: an indicator class
    without ``_compute`` or ``name``, a REGISTER that is not a dict of
    registry keys."""
    hints: list[dict] = []
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            bases = [ast.unparse(b) for b in node.bases]
            if not any(b.split(".")[-1] in CONTRACT_BASES for b in bases):
                continue
            methods = {n.name for n in node.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
            attrs = {t.id for n in node.body if isinstance(n, ast.Assign) for t in n.targets if isinstance(t, ast.Name)}
            if "_compute" not in methods:
                hints.append(_marker("warning", node.lineno, node.col_offset + 1,
                                     f"{node.name} has no _compute(self, df) — the contract's one abstract method"))
            if "name" not in attrs:
                hints.append(_marker("warning", node.lineno, node.col_offset + 1,
                                     f'{node.name} sets no `name = "..."` — reports and the roster would call it "abstract"'))
        elif isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "REGISTER" for t in node.targets):
            if not isinstance(node.value, ast.Dict):
                hints.append(_marker("warning", node.lineno, node.col_offset + 1,
                                     "REGISTER must be a dict literal: {\"key\": IndicatorClass}"))
                continue
            for key in node.value.keys:
                if isinstance(key, ast.Constant) and isinstance(key.value, str) and not KEY_PATTERN.match(key.value):
                    hints.append(_marker("warning", key.lineno, key.col_offset + 1,
                                         f"'{key.value}' is not a registry key (a letter, then lowercase letters, digits, underscores)"))
    return hints


def lint(candidate: Path) -> dict:
    """Diagnostics for the editor: syntax errors with their span, the
    parser's warnings (an invalid escape, a deprecated construct), and the
    contract's shape. No import, no numpy — fast enough to run as the user
    types."""
    name = candidate.name
    text = candidate.read_text(encoding="utf-8")
    markers: list[dict] = []
    tree = None
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        try:
            tree = compile(text, name, "exec", ast.PyCF_ONLY_AST)
            # The AST parses; the compiler still refuses e.g. `return`
            # outside a function or a misplaced `nonlocal`.
            compile(tree, name, "exec")
        except SyntaxError as e:
            markers.append(_marker("error", e.lineno, e.offset, e.msg, e.end_lineno, e.end_offset))
        except ValueError as e:  # a null byte, an encoding declaration the parser rejects
            markers.append(_marker("error", 1, 1, str(e)))
    for w in caught:
        if issubclass(w.category, (SyntaxWarning, DeprecationWarning)):
            markers.append(_marker("warning", getattr(w, "lineno", 1), 1, str(w.message)))
    if isinstance(tree, ast.Module) and not any(m["severity"] == "error" for m in markers):
        markers.extend(_contract_hints(tree))
    markers.sort(key=lambda m: (m["line"], m["column"]))
    return {"ok": not any(m["severity"] == "error" for m in markers), "markers": markers}


# ---------------------------------------------------------------- template

TEMPLATE = '''"""{name} — a QuantScript indicator.

Hypothesis: write the hypothesis BEFORE tuning — which market behaviour this
rule captures, and why it should persist out of sample.

Mechanism: a fast and a slow exponential average of the close; their distance,
measured in units of the market's own volatility, is the score. The verdict
flips only when the score crosses the band with a sign change (hysteresis)
and holds otherwise — +1 in an uptrend regime, -1 in a downtrend regime, 0
only while warming up.

The contract (smithery/contract.py): causal, scale-invariant, dimensionless
parameters, hysteresis. Certification comes from the forge's gauntlet in
QuantAlgo → Smithery, never from this file.
"""
from __future__ import annotations

import numpy as np
import pandas as pd

from ..contract import TrendIndicator, hysteresis_flip


class {class_name}(TrendIndicator):
    name = "{name}"
    # The region the gauntlet perturbs — one range per dimensionless parameter.
    param_space = {{"fast": (5, 60), "slow": (20, 300), "band": (0.05, 1.5)}}

    @classmethod
    def default_params(cls):
        return {{"fast": 20, "slow": 100, "band": 0.25, "vol_halflife": 20}}

    def _compute(self, df: pd.DataFrame) -> np.ndarray:
        close = df["close"].astype(float)
        fast = close.ewm(span=int(self.params["fast"])).mean()
        slow = close.ewm(span=int(self.params["slow"])).mean()
        r = np.log(close).diff()
        vol = r.ewm(halflife=int(self.params["vol_halflife"])).std().shift(1)
        # Distance of the fast average from the slow one, relative to price
        # and in units of volatility: dimensionless, so a rescaled price
        # series gives the same verdict (scale invariance). `np.array` copies —
        # pandas hands out read-only views under copy-on-write.
        score = np.array((fast - slow) / slow / vol.clip(lower=1e-9), dtype=float)
        score[: int(self.params["slow"])] = np.nan
        return hysteresis_flip(score, float(self.params["band"]))


# QuantScript registers this script through these two names — nobody edits
# indicators/__init__.py for it (see indicators/_discover.py).
REGISTER = {{"{key}": {class_name}}}
WARMUP = {{"{key}": 400}}
'''


def template(key: str, class_name: str, name: str) -> str:
    if not KEY_PATTERN.match(key):
        raise ValueError(f"'{key}' is not a registry key (a letter, then lowercase letters, digits, underscores; at most 40)")
    if not CLASS_PATTERN.match(class_name):
        raise ValueError(f"'{class_name}' is not a class name (CapitalizedWords, letters and digits)")
    name = re.sub(r"[\"\\\r\n]", "", name).strip() or class_name
    return TEMPLATE.format(key=key, class_name=class_name, name=name)


# ---------------------------------------------------------------- main

def _main(argv: list[str]) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(prog="python -m smithery.quantscript")
    sub = ap.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="every script with its classes and the registry's verdicts")

    p = sub.add_parser("check", help="verify a candidate version of a script in a sandbox copy")
    p.add_argument("--file", required=True, help="the script's file name, e.g. extremes.py")
    p.add_argument("--candidate", required=True, help="path of the candidate content")
    p.add_argument("--depth", default="quick", choices=DEPTHS)
    p.add_argument("--frame-bars", type=int, default=None)
    p.add_argument("--timeout", type=float, default=CHECK_TIMEOUT_S)

    p = sub.add_parser("verify", help="(internal) the check itself, run where smithery resolves")
    p.add_argument("--file", required=True)
    p.add_argument("--depth", default="quick", choices=DEPTHS)
    p.add_argument("--frame-bars", type=int, default=None)

    p = sub.add_parser("lint", help="syntax and contract-shape diagnostics for the editor (no import)")
    p.add_argument("--candidate", required=True, help="path of the candidate content")

    p = sub.add_parser("template", help="the source a new script starts from")
    p.add_argument("--key", required=True)
    p.add_argument("--class", dest="class_name", required=True)
    p.add_argument("--name", default=None)

    args = ap.parse_args(argv)
    try:
        if args.command == "list":
            print(json.dumps(listing(), default=str))
            return 0
        if args.command == "check":
            doc = check(args.file, Path(args.candidate), depth=args.depth, frame_bars=args.frame_bars,
                        timeout=args.timeout)
            print(json.dumps(doc, default=str))
            return 0
        if args.command == "verify":
            print(json.dumps(verify(args.file, depth=args.depth, frame_bars=args.frame_bars), default=str))
            return 0
        if args.command == "lint":
            print(json.dumps(lint(Path(args.candidate))))
            return 0
        if args.command == "template":
            print(json.dumps({"key": args.key, "class_name": args.class_name,
                              "source": template(args.key, args.class_name, args.name or args.class_name)}))
            return 0
    except Exception as e:  # noqa: BLE001 — one line the caller can show
        print(json.dumps({"event": "error", "message": f"{type(e).__name__}: {e}",
                          "traceback": traceback.format_exc()}))
        return 2
    return 2


if __name__ == "__main__":
    sys.exit(_main(sys.argv[1:]))
