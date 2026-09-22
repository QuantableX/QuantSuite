"""JSON-RPC server for QuantVoice.

QuantHUD's Rust side spawns ``python -m quantvoice`` as a long-lived child and
drives it over newline-delimited JSON on stdin/stdout — the wire
``rotation_lab.rpc`` and ``quantalgo`` speak.

* Request  (Rust → Python):     ``{"id": 1, "method": "start", "params": {...}}``
* Response (Python → Rust):     ``{"id": 1, "result": {...}}`` or ``{"id": 1, "error": "..."}``
* Notification (Python → Rust): ``{"method": "level", "params": {...}}`` (no ``id``)

Methods
-------
``ping``       → the engine's state (model, device, loaded, recording, …)
``configure``  → ``{model, device, language, live, inputDevice}``; loads the model in the background
``start``      → begin recording (works while the model still loads)
``stop``       → stop, transcribe in the background → ``result`` notification
``cancel``     → stop without transcribing
``devices``    → the input devices
``unload``     → free the model

Notifications: ``ready``, ``state {phase, detail?, model, device, compute, recording, transcribing}``,
``level {rms, peak, bands}``, ``interim {text}``, ``download {model, file, done, total}``,
``result {text, language, probability, durationSec, elapsedSec, empty}``, ``error {message}``.
"""

from __future__ import annotations

import json
import os
import sys
import threading
import traceback
from typing import Any

from . import __version__
from .engine import Engine

_WRITE_LOCK = threading.Lock()


def _write(obj: dict[str, Any]) -> None:
    with _WRITE_LOCK:
        sys.stdout.write(json.dumps(obj, ensure_ascii=False, allow_nan=False) + "\n")
        sys.stdout.flush()


def _notify(method: str, params: dict[str, Any]) -> None:
    _write({"method": method, "params": params})


def _deps() -> dict[str, str]:
    out: dict[str, str] = {}
    for name in ("faster_whisper", "ctranslate2", "sounddevice", "numpy"):
        try:
            mod = __import__(name)
            out[name] = str(getattr(mod, "__version__", "ok"))
        except Exception as exc:  # noqa: BLE001
            out[name] = f"error: {exc}"
    return out


def main() -> None:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]
        except Exception:  # noqa: BLE001
            pass
    engine = Engine(_notify)
    _notify("ready", {"version": __version__, "python": sys.version.split()[0], "deps": _deps(), "info": engine.info()})

    methods = {
        "ping": lambda _p: {"version": __version__, "python": sys.version.split()[0], "deps": _deps(), "info": engine.info(), "phase": engine.idle_phase()},
        "configure": engine.configure,
        "start": engine.start,
        "stop": engine.stop,
        "cancel": engine.cancel,
        "devices": lambda _p: {"devices": engine.devices()},
        "unload": lambda _p: engine.unload(),
        "transcribe_file": engine.transcribe_file,
    }

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            _write({"id": None, "error": f"invalid JSON: {exc}"})
            continue
        req_id = request.get("id")
        method = request.get("method")
        params = request.get("params") or {}
        handler = methods.get(method)
        if handler is None:
            _write({"id": req_id, "error": f"unknown method: {method}"})
            continue
        try:
            _write({"id": req_id, "result": handler(params)})
        except Exception as exc:  # noqa: BLE001 - everything goes back over the wire
            _write({"id": req_id, "error": str(exc), "trace": traceback.format_exc()})

    # stdin closed: the app is gone. Drop the microphone and leave — without
    # the interpreter's teardown, which trips over CUDA's destructors
    # (a fail-fast 0xC0000409 on exit, harmless but noisy in the logs).
    try:
        engine.cancel({})
    except Exception:  # noqa: BLE001
        pass
    sys.stdout.flush()
    os._exit(0)


if __name__ == "__main__":
    main()
