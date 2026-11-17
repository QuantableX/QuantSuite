"""BacktestEngine.prefetch: the requests of a run, in its order, without computing a signal.

A research run's preparation adds missing days to the saved data with the downloader. What a
downloader keeps is what it was asked for, so prefetch must ask for exactly what run asks for."""
import datetime as dt
import unittest
from dataclasses import dataclass
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import IndicatorConfig, RunConfig, TotalBreakoutConfig
from rotation_lab.data.ranking.base import RankedCoin

SYMBOLS = ("BTC", "AAA", "BBB", "CCC")
CAPS = {"BTC": 9e9, "AAA": 3e9, "BBB": 1e9, "CCC": 5e8}


def _frame(seed: int, n: int = 1000) -> pd.DataFrame:
    rng = np.random.default_rng(seed)
    close = np.exp(np.cumsum(rng.normal(0.001, 0.03, n))) * 100
    index = pd.date_range("2022-01-01", periods=n, freq="D")
    return pd.DataFrame({"open": close * (1 + rng.normal(0, 0.005, n)), "high": close * 1.02,
                         "low": close * 0.98, "close": close, "volume": 1.0}, index=index)


FRAMES = {sym: _frame(i) for i, sym in enumerate(SYMBOLS)}


@dataclass
class _Series:
    frame: pd.DataFrame


class _Recording:
    """A data source that answers from FRAMES and records every request."""

    def __init__(self):
        self.calls = []

    def get_series(self, ref, start, end, timeframe):
        self.calls.append(("series", ref.symbol, start, end, timeframe))
        f = FRAMES[ref.symbol]
        return _Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])

    def get_path(self, ref, start, end, timeframe="1d"):
        self.calls.append(("path", ref.symbol, start, end, timeframe))
        return None


def _timeline(start, exclude_top_n=0):
    coins = [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=CAPS[s], price=1.0)
             for i, s in enumerate(SYMBOLS)][exclude_top_n:]
    return UniverseTimeline(snapshots=[UniverseSnapshot(on_date=start, provider="test", coins=coins)])


def _requests(method, config):
    source = _Recording()
    engine = BacktestEngine(registry=object(), ohlcv=source)
    with patch("rotation_lab.backtest.engine.build_universe_timeline",
               side_effect=lambda *a, exclude_top_n=0, **k: _timeline(config.start_date, exclude_top_n)), \
            patch("rotation_lab.backtest.engine.market_warmup_snapshots", return_value=[]):
        result = getattr(engine, method)(config)
    return source.calls, result


class PrefetchTests(unittest.TestCase):
    def config(self, **values):
        base = dict(top_n=4, start_date=dt.date(2024, 1, 1), end_date=dt.date(2024, 9, 30),
                    indicator=IndicatorConfig(trend="ema_cross"))
        return RunConfig(**{**base, **values})

    def test_the_same_requests_as_a_run_and_no_signal(self):
        cases = {
            "plain": self.config(),
            # SCES: the head is excluded from trading but read for TOTAL; the BTC benchmark is then a
            # request of its own after the rotation.
            "excluded head with TOTAL": self.config(exclude_top_n=1, market_filter=True,
                                                    market_indicator=TotalBreakoutConfig()),
            "compared signals": self.config(compare_trends=("ema_cross",)),
        }
        for name, config in cases.items():
            with self.subTest(name):
                ran, result = _requests("run", config)
                with patch("rotation_lab.backtest.engine.BacktestEngine._rotations",
                           side_effect=AssertionError("prefetch computed a rotation")):
                    fetched, nothing = _requests("prefetch", config)
                self.assertIsNone(nothing)
                self.assertFalse(result.equity_strategy.empty)
                self.assertEqual(fetched, ran)
                if config.exclude_top_n:
                    self.assertEqual(ran[-1][:2], ("series", "BTC"))  # the benchmark's own request


if __name__ == "__main__":
    unittest.main()
