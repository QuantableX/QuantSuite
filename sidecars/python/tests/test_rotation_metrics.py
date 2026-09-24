"""CAGR of the rotation backtest's performance metrics."""

import math
import unittest

import pandas as pd

from rotation_lab.backtest.metrics import cagr, compute_metrics


class CagrTest(unittest.TestCase):
    def test_uses_calendar_years_of_a_daily_index(self):
        # Two calendar years of daily bars quadrupling: +100 % a year, although
        # the 255-bar Sharpe convention would count almost three years.
        idx = pd.date_range("2020-01-01", periods=731, freq="D")
        equity = pd.Series([1.0 * 4 ** (i / 730.5) for i in range(731)], index=idx)
        self.assertAlmostEqual(cagr(equity, 255.0), 1.0, places=3)
        self.assertAlmostEqual(compute_metrics(equity, 255.0).cagr_pct, 100.0, places=1)

    def test_relative_to_the_first_value(self):
        idx = pd.DatetimeIndex(["2021-01-01", "2022-01-01"])
        equity = pd.Series([2.0, 1.0], index=idx)
        self.assertAlmostEqual(cagr(equity), 0.5 ** (365.25 / 365) - 1.0, places=9)

    def test_non_datetime_index_counts_bars(self):
        equity = pd.Series([1.0, 1.1, 1.21])
        self.assertAlmostEqual(cagr(equity, periods_per_year=1.0), 0.1, places=9)

    def test_undefined_cases_are_nan(self):
        idx = pd.DatetimeIndex(["2021-01-01", "2021-01-01"])
        self.assertTrue(math.isnan(cagr(pd.Series([1.0, 2.0], index=idx))))
        self.assertTrue(math.isnan(cagr(pd.Series([1.0]))))
        self.assertTrue(math.isnan(cagr(pd.Series([1.0, 0.0]))))


if __name__ == "__main__":
    unittest.main()
