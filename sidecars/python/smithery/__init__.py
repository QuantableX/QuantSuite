"""The Indicator Smithery — the forge, as a suite sidecar package.

Causal binary trend-regime classifiers (+1 / -1, 0 only during warm-up),
the contract they obey, the price shelf, the vectorised backtester, the
Monte Carlo generators, the five-axis robustness gauntlet and the
walk-forward optimiser. Forged in the Obsidian vault "Indicator Smithery",
which stays the documentation, price-shelf and report home
(``smithery.data`` finds it); the code lives here once and is run by
QuantSystems (``rotation_lab.backtest.smithery`` is a shim onto this
package) and by QuantAlgo strategies (``quantalgo.regime``).

    from smithery.indicators import REGISTRY
    from smithery.registry import describe_all

PLAN-QUANTALGO §6–§7.
"""

__version__ = "0.2.0"
