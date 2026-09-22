"""The forge's indicators, for the rotation engine — a shim.

The Indicator Smithery lives once, in ``sidecars/python/smithery``
(PLAN-QUANTALGO §6); this package used to be a hand-copied duplicate of it.
The engine runs with ``sidecars/python`` as its working directory, so the
suite package imports directly. ``signals.py`` keeps importing ``REGISTRY``
from here; the warm-up sizes moved to the forge alongside the registry.
"""

from smithery.indicators import REGISTRY, WARMUP_BARS  # noqa: F401
from smithery.indicators import *  # noqa: F401,F403
from smithery.indicators import __all__  # noqa: F401
