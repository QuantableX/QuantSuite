"""Load editable scripts; no indicator implementation or fixed registry here."""
from ..contract import TrendIndicator as _TrendIndicator
from ._discover import discover as _discover
from ..workspace import ensure_indicators as _ensure_indicators

# No fallback to bundled implementations after a user deletes a script.
__path__ = [str(_ensure_indicators())]
REGISTRY, WARMUP_BARS, DISCOVERY_ERRORS, DISCOVERED = _discover(
    __path__[0], __name__, set(), _TrendIndicator)
for _class in REGISTRY.values():
    globals()[_class.__name__] = _class
from ..variants import load_variants as _load_variants
VARIANTS = _load_variants(REGISTRY, WARMUP_BARS, DISCOVERY_ERRORS)
__all__ = ["REGISTRY", "WARMUP_BARS", "DISCOVERY_ERRORS", "DISCOVERED", "VARIANTS",
           *{cls.__name__ for cls in REGISTRY.values() if cls.__module__ != "smithery.variants"}]
