"""QuantVoice — QuantHUD's local speech-to-text engine (docs/PLAN-QUANTVOICE.md).

Whisper through ``faster-whisper`` (CTranslate2), the microphone through
``sounddevice``. Driven by the ``hud`` module's Rust side over JSON-RPC on
stdin/stdout, the same wire the other Python sidecars speak.
"""

__version__ = "1.0.0"
