"""RotationLab - survivorship-bias-free crypto rotation backtester.

A Python port of the Pine ``rotation_system_nr.pine`` indicator where the
asset universe is recomputed at every rotation step from point-in-time
market-cap rankings (excluding stablecoins/wrapped tokens), so backtests
do not cherry-pick the assets that happened to survive.
"""

__version__ = "0.1.0"
__all__ = ["__version__"]
