"""Votes come from bounded, fixed history: the same day gives the same vote however early the data starts."""

import unittest

import numpy as np
import pandas as pd

from rotation_lab.backtest.signals import HISTORY_BLOCK, block_start, bounded


def frame(first, last):
    index = pd.date_range(first, last, freq="D")
    close = 100 + np.sin(np.arange(len(index)) / 9.0) * 10 + np.arange(len(index)) * 0.01
    return pd.DataFrame({"open": close, "high": close + 1, "low": close - 1, "close": close}, index=index)


def remembers_forever(window):
    """Worst case: every vote is decided by the first bar it was given (a verdict that never resets)."""
    return pd.Series(np.where(window["close"].iloc[0] > 100, 1, -1), index=window.index)


class BoundedHistoryTests(unittest.TestCase):
    def test_a_day_s_vote_does_not_depend_on_how_early_the_history_starts(self):
        long = frame("2018-05-11", "2026-10-26")
        short = long[long.index >= "2021-05-11"]
        a, b = bounded(long, remembers_forever, 600), bounded(short, remembers_forever, 600)
        # From the first block whose 600-day warm-up the shorter history fully covers, every vote is identical.
        first = block_start(short.index[0])
        while first - pd.Timedelta(days=600) < short.index[0]:
            first += HISTORY_BLOCK
        common = a.index[a.index >= first]
        pd.testing.assert_series_equal(a[common], b[common])
        # Unbounded, the two histories disagree on every day.
        self.assertTrue((remembers_forever(long)[common] != remembers_forever(short)[common]).all())

    def test_votes_never_use_history_older_than_the_warm_up_before_their_block(self):
        data = frame("2016-01-01", "2026-10-26")
        seen = []

        def record(window):
            seen.append((window.index[0], window.index[-1]))
            return pd.Series(0, index=window.index)

        votes = bounded(data, record, 600)
        self.assertEqual(list(votes.index), list(data.index))
        for first, last in seen:
            self.assertGreaterEqual(first, block_start(last) - pd.Timedelta(days=600))
            self.assertLess(last - first, pd.Timedelta(days=600) + HISTORY_BLOCK)

    def test_blocks_are_fixed_calendar_years_from_one_origin(self):
        self.assertEqual(block_start("2026-10-26"), block_start("2026-10-25"))
        self.assertEqual((block_start("2026-10-26") - pd.Timestamp("1970-01-01")) % HISTORY_BLOCK, pd.Timedelta(0))
        self.assertEqual(block_start(pd.Timestamp("2026-10-26", tz="UTC")).tz is not None, True)


if __name__ == "__main__":
    unittest.main()
