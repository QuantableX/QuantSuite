"""The candle of an A/B ratio (card 4e80395e, part C)."""
import unittest

import numpy as np
import pandas as pd

from rotation_lab.backtest.signals import ratio_frame


def frame(rows, index):
    return pd.DataFrame(rows, columns=["open", "high", "low", "close"], index=index, dtype=float)


class RatioCandleTests(unittest.TestCase):
    def setUp(self):
        self.index = pd.date_range("2024-01-01", periods=3, freq="D")
        # A and B move together on day 1, A alone on day 2, and on day 3 B's low is far below its body.
        self.a = frame([[100, 110, 95, 105], [105, 126, 104, 124], [124, 125, 120, 121]], self.index)
        self.b = frame([[50, 55, 47.5, 52.5], [52.5, 53, 52, 52.5], [52.5, 53, 40, 52]], self.index)

    def test_high_and_low_come_from_the_same_field_of_both_coins(self):
        r = ratio_frame(self.a, self.b)
        self.assertEqual(list(r.columns), ["open", "close", "high", "low"])
        np.testing.assert_allclose(r["open"], [2.0, 2.0, 124 / 52.5])
        np.testing.assert_allclose(r["close"], [2.0, 124 / 52.5, 121 / 52])
        # Day 1: both coins have the same shape, so the ratio did not move: a flat candle,
        # where high(A)/low(B) would have drawn 110/47.5 = 2.32 over 95/55 = 1.73.
        np.testing.assert_allclose([r["high"].iloc[0], r["low"].iloc[0]], [2.0, 2.0])
        # Day 2: A's own move shows: high(A)/high(B) above the close.
        np.testing.assert_allclose(r["high"].iloc[1], 126 / 53)
        np.testing.assert_allclose(r["low"].iloc[1], 2.0)  # low(A)/low(B) = 2.0, the open

    def test_the_candle_always_holds_its_open_and_close(self):
        r = ratio_frame(self.a, self.b)
        # Day 3: low(A)/low(B) = 3.0 lies above the whole body, so the low is the body's own low.
        np.testing.assert_allclose(r["low"].iloc[2], 121 / 52)
        self.assertTrue((r["high"] >= r[["open", "close"]].max(axis=1)).all())
        self.assertTrue((r["low"] <= r[["open", "close"]].min(axis=1)).all())

    def test_against_usd_it_is_the_coins_own_candle(self):
        pd.testing.assert_frame_equal(ratio_frame(self.a, None), self.a)

    def test_only_common_days(self):
        r = ratio_frame(self.a, self.b.drop(self.index[1]))
        self.assertEqual(list(r.index), [self.index[0], self.index[2]])
        self.assertTrue(ratio_frame(self.a, self.b.iloc[:0]).empty)


if __name__ == "__main__":
    unittest.main()
