"""Private scripts outside the shipped runtime, with no seeded examples.

The desktop app passes the same explicit path to every Python consumer.
Standalone Python resolves relative to the sidecar installation or checkout.
"""
from __future__ import annotations

import os
import json
import warnings
from pathlib import Path


def indicators_dir() -> Path:
    explicit = os.environ.get("QUANTSCRIPT_INDICATORS_DIR")
    if explicit:
        return Path(explicit).expanduser().resolve()
    return Path(__file__).resolve().parents[3] / "QuantScript" / "indicators"


def ensure_indicators() -> Path:
    target = indicators_dir()
    # Existing read-only libraries can still be used for execution.
    if not target.is_dir():
        target.mkdir(parents=True, exist_ok=True)
    return target


def library_metadata() -> dict:
    """Optional historical research and variant labels owned by this library."""
    path = indicators_dir() / "library.json"
    if not path.is_file():
        return {}
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(doc, dict) or any(not isinstance(value, dict) for value in doc.values()):
            raise ValueError("expected an object of metadata tables")
        return doc
    except (OSError, ValueError) as exc:
        warnings.warn(f"Could not read private library metadata at {path}: {exc}")
        return {}
