"""Run configuration dataclasses for RotationLab.

Mirrors the Pine indicator's inputs so that backtest results stay
comparable with the TradingView version, while adding the new
dynamic-universe knobs (top-N size, refresh cadence, data source).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, timedelta
from enum import Enum
from pathlib import Path
from typing import Literal


class Cadence(str, Enum):
    """Bar resolution / rotation cadence.

    The intraday members (``1m`` / ``1h`` / ``4h`` / ``12h``) drive both the candle
    resolution *and* the rotation step. The daily / weekly / monthly members
    keep the original behaviour: bars are daily candles and only the
    universe-refresh frequency changes.
    """

    MINUTE_1 = "1m"
    HOUR_1 = "1h"
    HOUR_4 = "4h"
    HOUR_12 = "12h"
    DAILY = "daily"
    WEEKLY = "weekly"
    MONTHLY = "monthly"

    @property
    def is_intraday(self) -> bool:
        return self in (Cadence.MINUTE_1, Cadence.HOUR_1, Cadence.HOUR_4, Cadence.HOUR_12)

    @property
    def pandas_freq(self) -> str:
        return {
            "1m": "1min",
            "1h": "1h",
            "4h": "4h",
            "12h": "12h",
            "daily": "D",
            "weekly": "W-MON",
            "monthly": "MS",
        }[self.value]

    @property
    def ccxt_timeframe(self) -> str:
        """Candle resolution to fetch from the OHLCV provider.

        Intraday cadences fetch true intraday candles; daily / weekly /
        monthly keep rotating on **daily** candles (only their
        universe-refresh frequency differs), so they all map to ``1d``.
        """

        return {"1m": "1m", "1h": "1h", "4h": "4h", "12h": "12h"}.get(self.value, "1d")

    @property
    def bars_per_year(self) -> float:
        """Calendar bars per year, used to annualise Sharpe / Sortino.

        Daily / weekly / monthly run on daily bars and keep the original
        255 trading-day convention; intraday uses calendar bar counts.
        """

        return {
            "1m": 365.0 * 24.0 * 60.0,
            "1h": 365.0 * 24.0,
            "4h": 365.0 * 6.0,
            "12h": 365.0 * 2.0,
        }.get(self.value, 255.0)


class RankingSource(str, Enum):
    CMC = "cmc"
    LOCAL = "local"
    AUTO = "auto"


SourceKind = Literal["open", "high", "low", "close", "hl2", "hlc3", "ohlc4"]


@dataclass(frozen=True)
class EmaCrossConfig:
    """Parameters for the 12/21 EMA band cross (QuantFolio's f_ema_cross).

    Signal is ``1`` when ``EMA(src, fast) >= EMA(src, slow)``, else ``0``.
    Defaults to the canonical 12 / 21 band.
    """

    src: SourceKind = "close"
    fast_length: int = 12
    slow_length: int = 21


TrendKind = Literal[
    "aroon",
    "dmi",
    "vortex",
    "tsi",
    "hull",
    "alma",
    "frama",
    "vidya",
    "regression",
    "median_mad",
    "bollinger_trend",
    "keltner_risk",
    "ichimoku",
    "rsi_trend",
    "stochastic_trend",
    "chandelier",
    "roc_trend",
    "efficiency_breakout",
    "bq_adaptive_envelope",
    "lyro_rmd",

    "supertrend", "donchian", "kama_band", "ema_atr", "dual_momentum",
    "defensive_trend", "bq_volatility_gate", "lyro_ha",
    "ema_cross", "kalman", "hmm", "wavelet", "hilbert", "vratio", "council",
    "sprt", "page", "bocpd", "fractal", "slopes", "extremes", "ordinal",
    # the forge round of 2026-09-07
    "imm", "ensemble", "mk", "bvc", "dc", "rkalman", "runs", "council2",
    # the forge round of 2026-09-08
    "dcl", "bretrace", "consensus", "rankbreak",
    # the third forge round of 2026-09-08
    "scale", "swing", "consensus2",
    "ensemble_original", "ensemble_wf", "council2_original", "scale_original", "robust",
    # the average of the ``IndicatorConfig.aggregate`` members' signals
    "aggregate",
]


@dataclass(frozen=True)
class IndicatorConfig:
    """Trend-signal bundle: the classic EMA cross, a Smithery indicator, or
    the aggregate of several.

    ``trend`` selects the pairwise signal engine. ``ema_cross`` is the
    original 12/21 band cross; the other kinds run the corresponding
    Smithery indicator (the suite's ``smithery`` package) at its
    defaults on the same A/B ratio series. Read current and historical
    evidence per timeframe from ``smithery.registry``. A USD gauntlet score
    does not certify performance on ratios or in a historical portfolio.

    ``aggregate`` runs every member on the same series and averages their
    ±1 signals (members still warming up do not vote): the pair is bullish
    while the average is above 0, bearish below 0, and holds its verdict at
    exactly 0. ``ema_cross`` may be a member and uses ``ema_cross``.
    """

    trend: TrendKind = "ema_cross"
    ema_cross: EmaCrossConfig = field(default_factory=EmaCrossConfig)
    aggregate: tuple[TrendKind, ...] = ()


@dataclass(frozen=True)
class TotalBreakoutConfig:
    """TOTAL-only breakout regime. Research option, not a drawdown guarantee."""

    trend: Literal['total_breakout'] = 'total_breakout'
    trend_length: int = 100
    entry_length: int = 5
    exit_length: int = 5

    def __post_init__(self):
        for name, minimum in [('trend_length',2),('entry_length',2),('exit_length',2)]:
            value = getattr(self,name)
            if not isinstance(value,int) or not minimum <= value <= 5000:
                raise ValueError(f'TOTAL breakout {name} must be an integer from {minimum} to 5000')


@dataclass(frozen=True)
class RunConfig:
    """Top-level configuration for a single backtest or live evaluation."""

    top_n: int = 5
    # Number of highest-ranked coins to *exclude* from the universe before
    # evaluation. ``top_n`` still counts these excluded coins, so the
    # evaluated cohort is ranks ``exclude_top_n + 1 .. top_n``. LCES leaves
    # this at 0 (draws the top of the ranking); SCES sets it (e.g. 5 with
    # top_n=10 → ranks 6..10, the small-cap slice).
    exclude_top_n: int = 0
    cadence: Cadence = Cadence.DAILY
    start_date: date = field(default_factory=lambda: date.today() - timedelta(days=365 * 3))
    end_date: date = field(default_factory=date.today)
    ranking_source: RankingSource = RankingSource.AUTO
    exclude_stablecoins: bool = True
    exclude_wrapped: bool = True
    indicator: IndicatorConfig = field(default_factory=IndicatorConfig)
    # Further trend signals a backtest runs next to ``indicator`` — same
    # universe, same candles, one rotation simulation each — so their
    # equity curves and metrics sit side by side in one result. Live
    # evaluation ignores it. ``ema_cross`` here uses ``indicator.ema_cross``.
    compare_trends: tuple[TrendKind, ...] = ()
    # The higher filter: positions only while TOTAL — the market-cap-weighted
    # index of the ranked top-N (``backtest.market``) — is bullish by the
    # selected market indicator; otherwise USD. Applies to backtests and live.
    market_filter: bool = False
    # None preserves the original behavior: each strategy gates itself.
    # A separate bundle changes only TOTAL, never the pairwise coin ranking.
    market_indicator: IndicatorConfig | TotalBreakoutConfig | None = None
    # Allow USD ("cash") to compete with crypto assets; mirrors Pine.
    include_usd: bool = True
    # Trading costs, as fractions per side (0.001 = 0.1%). An asset
    # switch (A -> B) sells to USD then buys, so it pays each cost twice.
    fee_rate: float = 0.001
    slippage_rate: float = 0.0
    # Polite request throttling (seconds between calls to a given host).
    min_request_interval: float = 1.2


def default_cache_path() -> Path:
    """Return a per-user cache database path.

    Lives next to the package by default so the tool is self-contained
    when running from the repo checkout. Users can override via
    ``ROTATION_LAB_CACHE`` env var.
    """

    import os

    env = os.environ.get("ROTATION_LAB_CACHE")
    if env:
        return Path(env)
    return Path(__file__).resolve().parent.parent / "_cache" / "rotation_lab.sqlite"
