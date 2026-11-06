"""Funds: the split methods (the card's worked example), the simulation and its guarantees."""
import unittest

import numpy as np
import pandas as pd

from rotation_lab import funds as f


def stats(profit, sharpe, drawdown, volatility):
    # The worked example estimates full Kelly as profit / volatility squared.
    return f.MemberStats(profit, sharpe, sharpe, drawdown, volatility, profit / volatility ** 2)


EXAMPLE = {
    "A": stats(0.80, 1.6, 0.30, 0.50),
    "B": stats(0.40, 1.2, 0.20, 0.33),
    "C": stats(0.20, 1.0, 0.15, 0.20),
    "D": stats(0.60, 1.0, 0.45, 0.60),
    "E": stats(-0.10, -0.2, 0.35, 0.50),
}
EXPECTED = {
    "equal": [100, 100, 100, 100, 100],
    "profit": [200, 100, 50, 150, 0],
    "sharpe": [167, 125, 104, 104, 0],
    "return_drawdown": [182, 136, 91, 91, 0],
    "inverse_volatility": [73, 111, 182, 61, 73],
    "kelly": [118, 136, 185, 61, 0],
    "combined": [159, 134, 116, 91, 0],
}


def curves(days=800, seed=1, drifts=(0.002, 0.001, -0.0005), start="2022-01-01"):
    rng = np.random.default_rng(seed)
    index = pd.date_range(start, periods=days, freq="D")
    return pd.DataFrame({f"m{i}": np.cumprod(1 + d + 0.02 * rng.standard_normal(days)) for i, d in enumerate(drifts)}, index=index)


class WorkedExample(unittest.TestCase):
    def test_the_cards_table(self):
        for method, dollars in EXPECTED.items():
            rule = f.FundRule(method=method, cap_pct=100)
            split = f.split_dollars(f.weights_from_stats(EXAMPLE, rule), 500)
            self.assertEqual([split[k] for k in "ABCDE"], dollars, method)

    def test_combined_points(self):
        raw = f._raw_weights(EXAMPLE, f.FundRule(method="combined"))
        self.assertEqual([raw[k] for k in "ABCD"], [13, 11, 9.5, 7.5])

    def test_kelly_scales_down_and_never_levers(self):
        weights = f.weights_from_stats(EXAMPLE, f.FundRule(method="kelly", cap_pct=100))
        self.assertAlmostEqual(sum(weights.values()), 1.0)


class Rules(unittest.TestCase):
    def test_fixed_shares_leave_the_rest_in_cash(self):
        rule = f.FundRule(method="fixed", fixed_pct={"A": 30, "B": 20})
        weights = f.weights_from_stats({"A": EXAMPLE["A"], "B": EXAMPLE["B"]}, rule)
        self.assertEqual(weights, {"A": 0.3, "B": 0.2})

    def test_kelly_below_100_percent_keeps_cash(self):
        small = {"A": f.MemberStats(0.1, 1, 1, 0.1, 0.5, 0.4), "B": f.MemberStats(0.1, 1, 1, 0.1, 0.5, 0.4)}
        weights = f.weights_from_stats(small, f.FundRule(method="kelly", kelly_fraction=0.25, cap_pct=100))
        self.assertAlmostEqual(sum(weights.values()), 0.2)
        doubled = f.weights_from_stats(small, f.FundRule(method="kelly", kelly_fraction=0.5, cap_pct=100))
        self.assertAlmostEqual(doubled["A"], 2 * weights["A"])

    def test_all_negative_holds_cash(self):
        losers = {k: f.MemberStats(-0.1, -1, -1, 0.2, 0.5, -0.4) for k in "AB"}
        for method in f.PERFORMANCE:
            self.assertEqual(sum(f.weights_from_stats(losers, f.FundRule(method=method)).values()), 0.0, method)

    def test_inverse_volatility_keeps_a_loser(self):
        weights = f.weights_from_stats(EXAMPLE, f.FundRule(method="inverse_volatility", cap_pct=100))
        self.assertGreater(weights["E"], 0)

    def test_the_cap_redistributes_and_rest_stays_in_cash(self):
        weights = f.weights_from_stats(EXAMPLE, f.FundRule(method="profit", cap_pct=35))
        self.assertAlmostEqual(weights["A"], 0.35)
        self.assertAlmostEqual(sum(weights.values()), 1.0)
        self.assertTrue(all(v <= 0.35 + 1e-9 for v in weights.values()))
        two = {"A": EXAMPLE["A"], "E": EXAMPLE["E"]}
        only_one = f.weights_from_stats(two, f.FundRule(method="profit", cap_pct=50))
        self.assertAlmostEqual(only_one["A"], 0.5)
        self.assertEqual(only_one["E"], 0.0)

    def test_equal_ignores_the_cap(self):
        self.assertEqual(f.weights_from_stats({"A": EXAMPLE["A"]}, f.FundRule(method="equal", cap_pct=50)), {"A": 1.0})

    def test_unknown_names_are_refused(self):
        with self.assertRaises(ValueError):
            f.weights_from_stats(EXAMPLE, f.FundRule(method="nope"))
        with self.assertRaises(ValueError):
            f.is_rebalance_day(pd.Timestamp("2026-01-02"), pd.Timestamp("2026-01-01"), "daily")

    def test_stats_from_returns(self):
        s = f.stats_of(np.array([0.1, -0.1, 0.1]))
        self.assertAlmostEqual(s.profit, 1.1 * 0.9 * 1.1 - 1)
        self.assertAlmostEqual(s.max_drawdown, 0.1)
        self.assertGreater(s.sharpe, 0)
        self.assertGreater(s.sortino, s.sharpe)


class Simulation(unittest.TestCase):
    def test_one_member_with_equal_and_no_rebalance_is_that_system(self):
        frame = curves(drifts=(0.001,))
        run = f.run_fund(frame, f.FundRule(method="equal", rebalance="never", fee_rate=0.001))
        expected = frame["m0"] / frame["m0"].iloc[0]
        self.assertTrue(np.allclose(run.equity.to_numpy(), expected.to_numpy(), rtol=1e-12, atol=0))
        self.assertEqual(run.cost, 0.0)

    def test_equal_without_rebalance_is_the_mean_of_the_curves(self):
        frame = curves()
        run = f.run_fund(frame, f.FundRule(method="equal", rebalance="never"))
        expected = (frame / frame.iloc[0]).mean(axis=1)
        self.assertTrue(np.allclose(run.equity.to_numpy(), expected.to_numpy()))

    def test_a_later_price_never_changes_an_earlier_split(self):
        frame = curves()
        rule = f.FundRule(method="sharpe", lookback_days=90, rebalance="monthly")
        whole = f.run_fund(frame, rule)
        changed = frame.copy()
        cut = len(frame) - 100
        changed.iloc[cut:] = changed.iloc[cut:] * 3.7
        other = f.run_fund(changed, rule)
        before = [r for r in whole.rebalances if r.day < frame.index[cut]]
        again = [r for r in other.rebalances if r.day < frame.index[cut]]
        self.assertTrue(before)
        self.assertEqual([r.values for r in before], [r.values for r in again])
        self.assertEqual(whole.equity.iloc[:cut].tolist(), other.equity.iloc[:cut].tolist())

    def test_a_split_decided_at_a_close_applies_after_it(self):
        index = pd.date_range("2024-01-30", periods=4, freq="D")  # the month turns at row 2
        frame = pd.DataFrame({"a": [1.0, 1.0, 2.0, 2.0], "b": [1.0, 1.0, 1.0, 2.0]}, index=index)
        run = f.run_fund(frame, f.FundRule(method="fixed", fixed_pct={"a": 50, "b": 50}, rebalance="monthly"))
        # Feb 1 closes at 1.5 for 50/50; b doubling on Feb 2 acts on the new split: 0.75 + 1.5 = 2.25.
        self.assertAlmostEqual(run.equity.iloc[2], 1.5)
        self.assertAlmostEqual(run.equity.iloc[3], 2.25)

    def test_moving_money_costs_fees(self):
        frame = curves()
        free = f.run_fund(frame, f.FundRule(method="fixed", fixed_pct={"m0": 50, "m1": 50}, rebalance="monthly"))
        paid = f.run_fund(frame, f.FundRule(method="fixed", fixed_pct={"m0": 50, "m1": 50}, rebalance="monthly", fee_rate=0.002))
        self.assertLess(paid.equity.iloc[-1], free.equity.iloc[-1])
        self.assertGreater(paid.cost, 0)

    def test_until_the_lookback_is_full_the_fund_is_equal(self):
        frame = curves()
        run = f.run_fund(frame, f.FundRule(method="sharpe", lookback_days=365, rebalance="monthly", cap_pct=100))
        early = run.rebalances[0]
        shares = np.array(list(early.values.values())) / sum(early.values.values())
        self.assertTrue(np.allclose(shares, 1 / 3))

    def test_a_negative_market_goes_to_cash_and_stays_flat(self):
        index = pd.date_range("2023-01-01", periods=400, freq="D")
        falling = pd.DataFrame({"a": 0.999 ** np.arange(400), "b": 0.998 ** np.arange(400)}, index=index)
        run = f.run_fund(falling, f.FundRule(method="profit", lookback_days=30, rebalance="monthly"))
        self.assertEqual(sum(run.target_weights.values()), 0.0)
        self.assertAlmostEqual(run.equity.iloc[-1], run.equity.iloc[-20])

    def test_a_late_start_feeds_the_lookback_only(self):
        frame = curves()
        start = frame.index[400]
        run = f.run_fund(frame, f.FundRule(method="sharpe", lookback_days=90, rebalance="never", cap_pct=100), start)
        self.assertEqual(run.equity.index[0], start)
        self.assertEqual(run.equity.iloc[0], 1.0)
        # m0 has the best drift: the opening split read the bars before the start.
        self.assertGreater(run.final_weights["m0"], 0)

    def test_the_common_window_names_the_member_that_limits_it(self):
        a = pd.Series(1.0, index=pd.date_range("2022-01-01", periods=100, freq="D"))
        b = pd.Series(1.0, index=pd.date_range("2022-02-01", periods=100, freq="D"))
        frame, limited = f.aligned_curves({"a": a, "b": b})
        self.assertEqual(frame.index[0], pd.Timestamp("2022-02-01"))
        self.assertEqual(limited, "b")

    def test_an_intraday_member_is_read_at_the_daily_close(self):
        hours = pd.date_range("2024-01-01", periods=48, freq="h", tz="UTC")
        daily = f.daily_curve(pd.Series(np.arange(48, dtype=float) + 1, index=hours))
        self.assertEqual(daily.tolist(), [24.0, 48.0])

    def test_next_rebalance_dates(self):
        import datetime as dt
        self.assertEqual(f.next_rebalance(dt.date(2026, 11, 6), "monthly"), dt.date(2026, 12, 1))
        self.assertEqual(f.next_rebalance(dt.date(2026, 12, 6), "monthly"), dt.date(2027, 1, 1))
        self.assertEqual(f.next_rebalance(dt.date(2026, 11, 6), "quarterly"), dt.date(2027, 1, 1))
        self.assertEqual(f.next_rebalance(dt.date(2026, 2, 6), "quarterly"), dt.date(2026, 4, 1))
        self.assertEqual(f.next_rebalance(dt.date(2026, 11, 6), "weekly"), dt.date(2026, 11, 9))
        self.assertIsNone(f.next_rebalance(dt.date(2026, 11, 6), "never"))


class Robustness(unittest.TestCase):
    def test_every_lookback_and_window_is_compared_with_equal(self):
        frame = curves(days=2400, start="2020-01-01")
        report = f.robustness(frame, f.FundRule(method="sharpe", rebalance="monthly"))
        self.assertEqual([r["lookbackDays"] for r in report["lookbacks"]], list(f.LOOKBACKS))
        self.assertEqual([r["window"] for r in report["windows"]], ["2020-2022", "2023-2025", "2021-2024", "2024-2026"])
        self.assertTrue(all(r["beatsEqual"] is not None for r in report["lookbacks"]))

    def test_equal_is_the_baseline_and_not_compared_with_itself(self):
        report = f.robustness(curves(days=800), f.FundRule(method="equal"))
        self.assertTrue(all(r["beatsEqual"] is None for r in report["lookbacks"]))

    def test_a_window_outside_the_history_is_marked_not_faked(self):
        frame = curves(days=500, start="2024-01-01")
        report = f.robustness(frame, f.FundRule(method="sharpe"))
        early = next(r for r in report["windows"] if r["window"] == "2020-2022")
        self.assertIsNone(early["beatsEqual"])


if __name__ == "__main__":
    unittest.main()
