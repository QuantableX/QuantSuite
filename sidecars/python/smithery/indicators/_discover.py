"""Scripts written in the suite register themselves (QuantScript).

Every indicator, including the initial examples, lives in an editable script.
A script declares

    REGISTER = {"mykey": MyIndicator}   # registry key -> TrendIndicator class
    WARMUP = {"mykey": 400}             # optional: bars of warm-up per key

and is picked up here; nobody edits ``__init__`` for it. A script that fails
to import, or registers a key the registry already has, lands in
``DISCOVERY_ERRORS`` instead of breaking the registry for every bot and every
backtest — QuantScript shows the message next to the file.
"""
from __future__ import annotations

import importlib
import inspect
import re
import sys
from pathlib import Path

# The forge's key alphabet (QuantAlgo's `valid_key`): lowercase, digits,
# underscores, at most 40 characters, a letter first.
KEY_PATTERN = re.compile(r"^[a-z][a-z0-9_]{0,39}$")


def discover(package_dir: str | Path, package: str, known: set[str], contract: type,
             modules=None) -> tuple[dict, dict, dict[str, str], dict[str, list[str]]]:
    """Collect every script, including dependencies imported by earlier scripts.

    Returns ``(registry, warmup, errors, discovered)``: the keys the scripts
    add, their warm-up bars, one message per script that could not be taken
    (keyed by the module's stem — the script's file name without ``.py``),
    and which script registered which keys. Scripts are user-owned; failures
    stay local to their files and dependencies.
    """
    registry: dict = {}
    warmup: dict = {}
    errors: dict[str, str] = {}
    discovered: dict[str, list[str]] = {}
    modules = sys.modules if modules is None else modules
    for path in sorted(Path(package_dir).glob("*.py")):
        stem = path.stem
        if stem.startswith("_"):
            continue
        name = f"{package}.{stem}"
        try:
            module = modules[name] if name in modules else importlib.import_module(name)
        except Exception as e:  # noqa: BLE001 — one broken script must not sink the registry
            errors[stem] = f"{type(e).__name__}: {e}"
            continue
        register = getattr(module, "REGISTER", None)
        if register is None:
            continue
        if not isinstance(register, dict):
            errors[stem] = "REGISTER must be a dict of registry key -> indicator class"
            continue
        declared = getattr(module, "WARMUP", {})
        problems: list[str] = []
        taken: list[str] = []
        for key, cls in register.items():
            if not isinstance(key, str) or not KEY_PATTERN.match(key):
                problems.append(f"{key!r} is not a registry key (a letter, then lowercase letters, digits, underscores)")
            elif key in known or key in registry:
                problems.append(f"key '{key}' is already registered")
            elif not (inspect.isclass(cls) and issubclass(cls, contract)):
                problems.append(f"'{key}' must map to a TrendIndicator subclass")
            else:
                registry[key] = cls
                taken.append(key)
                bars = declared.get(key, 400) if isinstance(declared, dict) else 400
                try:
                    warmup[key] = max(1, int(bars))
                except (TypeError, ValueError):
                    warmup[key] = 400
        if taken:
            discovered[stem] = taken
        if problems:
            errors[stem] = "; ".join(problems)
    return registry, warmup, errors, discovered
