"""TOTAL (the cap-weighted index of the ranked universe) and the higher
filter that keeps the rotation in USD while TOTAL is bearish."""
import datetime as dt
import unittest
from dataclasses import asdict, dataclass, replace
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import USD_SYMBOL, BacktestEngine
from rotation_lab.backtest.market import market_gate, market_index
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import EmaCrossConfig, IndicatorConfig, RunConfig, TotalBreakoutConfig
from rotation_lab.data.ranking.base import RankedCoin

SYMBOLS = ("AAA", "BBB", "CCC")


def _frame(seed: int, n: int = 1000, drift: float = 0.001) -> pd.DataFrame:
    rng = np.random.default_rng(seed)
    close = np.exp(np.cumsum(rng.normal(drift, 0.03, n))) * 100
    index = pd.date_range("2022-01-01", periods=n, freq="D")
    return pd.DataFrame({
        "open": close * (1 + rng.normal(0, 0.005, n)),
        "high": close * 1.02, "low": close * 0.98, "close": close, "volume": 1.0,
    }, index=index)


FRAMES = {sym: _frame(i) for i, sym in enumerate(SYMBOLS)}
CAPS = {"AAA": 3e9, "BBB": 1e9, "CCC": 5e8}


def _cut(frames, start=dt.date(2024, 1, 1)):
    """The candles a run with no warm-up (the EMA cross) fetches."""
    return {sym: f.loc[f.index >= pd.Timestamp(start)] for sym, f in frames.items()}


def _coins(symbols=SYMBOLS):
    return [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=CAPS[s], price=1.0)
            for i, s in enumerate(symbols)]


def _timeline(start: dt.date, symbols=SYMBOLS) -> UniverseTimeline:
    return UniverseTimeline(snapshots=[UniverseSnapshot(on_date=start, provider="test", coins=_coins(symbols))])


@dataclass
class _Series:
    frame: pd.DataFrame


class _Ohlcv:
    def get_series(self, ref, start, end, timeframe):
        f = FRAMES[ref.symbol]
        return _Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])


def _run(trend: str, *, market_filter: bool, compare=(), exclude_top_n=0, timeline=None, market_indicator=None):
    config = RunConfig(
        top_n=3, exclude_top_n=exclude_top_n,
        start_date=dt.date(2024, 1, 1), end_date=dt.date(2024, 9, 30),
        indicator=IndicatorConfig(trend=trend), compare_trends=compare, market_filter=market_filter,
        market_indicator=market_indicator,
    )
    engine = BacktestEngine(registry=object(), ohlcv=_Ohlcv())
    with patch("rotation_lab.backtest.engine.build_universe_timeline",
               side_effect=timeline or (lambda *a, **k: _timeline(config.start_date))):
        return engine.run(config)


class MarketIndexTests(unittest.TestCase):
    def test_one_coin_is_that_coin_normalised_to_its_first_close(self):
        f = FRAMES["AAA"]
        total = market_index([UniverseSnapshot(dt.date(2022, 1, 1), "t", _coins(("AAA",)))], {"AAA": f})
        for col in ("open", "high", "low", "close"):
            np.testing.assert_allclose(total[col].to_numpy()[1:], (f[col] / f["close"].iloc[0]).to_numpy()[1:])
        self.assertEqual(total["close"].iloc[0], 1.0)

    def test_two_coins_blend_returns_by_market_cap(self):
        a, b = FRAMES["AAA"], FRAMES["BBB"]
        total = market_index([UniverseSnapshot(dt.date(2022, 1, 1), "t", _coins(("AAA", "BBB")))],
                             {"AAA": a, "BBB": b})
        ra, rb = a["close"].pct_change(), b["close"].pct_change()
        expected = (0.75 * ra + 0.25 * rb).iloc[1:]
        np.testing.assert_allclose(total["close"].pct_change().iloc[1:].to_numpy(), expected.to_numpy())
        self.assertTrue((total["high"] >= total["close"]).all())
        self.assertTrue((total["low"] <= total["close"]).all())

    def test_weights_follow_the_snapshots_and_missing_bars_drop_out(self):
        a = FRAMES["AAA"]
        b = FRAMES["BBB"].iloc[:500]  # BBB stops trading half way
        snaps = [UniverseSnapshot(dt.date(2022, 1, 1), "t", _coins(("AAA", "BBB"))),
                 UniverseSnapshot(dt.date(2023, 1, 1), "t", _coins(("AAA",)))]
        total = market_index(snaps, {"AAA": a, "BBB": b})
        late = total["close"].pct_change().loc["2024-06-01":]
        np.testing.assert_allclose(late.to_numpy(), a["close"].pct_change().loc["2024-06-01":].to_numpy())
        self.assertTrue(np.isfinite(total[["open", "high", "low", "close"]].to_numpy()).all())


class HigherFilterTests(unittest.TestCase):
    def test_breakout_uses_prior_high_and_is_prefix_invariant(self):
        close=pd.Series([10.,11.,12.,13.,12.,11.,14.,15.],index=pd.date_range('2024-01-01',periods=8))
        total=pd.DataFrame({'close':close})
        cfg=TotalBreakoutConfig(trend_length=3,entry_length=3,exit_length=2)
        sig=market_gate(total,cfg)
        self.assertEqual(sig.tolist(),[0,0,0,1,0,0,1,1])
        pd.testing.assert_series_equal(sig.iloc[:6],market_gate(total.iloc[:6],cfg))
        pd.testing.assert_series_equal(sig,market_gate(total*123,cfg))
        self.assertEqual(_run('ema_cross',market_filter=True,market_indicator=cfg).config.market_indicator,cfg)

    def test_breakout_roundtrip_and_invalid_lengths(self):
        from rotation_lab.rpc import _build_config
        cfg=_build_config({'marketFilter':True,'marketIndicator':{
            'trend':'total_breakout','trendLength':30,'entryLength':40,'exitLength':5}})
        self.assertEqual(cfg.market_indicator,TotalBreakoutConfig(trend_length=30,entry_length=40,exit_length=5))
        self.assertEqual(_build_config(asdict(cfg)),cfg)
        with self.assertRaisesRegex(ValueError,'exit_length'):
            _build_config({'marketIndicator':{'trend':'total_breakout','exitLength':1}})

    def test_independent_filter_is_shared_by_comparisons_and_matches_solo(self):
        ind = IndicatorConfig(trend='defensive_trend')
        together = _run('ema_cross', market_filter=True, compare=('extremes',), market_indicator=ind)
        solo = _run('extremes', market_filter=True, market_indicator=ind)
        pd.testing.assert_series_equal(together.strategies[0].market_gate, together.strategies[1].market_gate)
        pd.testing.assert_series_equal(together.strategies[1].held_asset, solo.held_asset)
        np.testing.assert_allclose(together.strategies[1].equity_strategy, solo.equity_strategy, rtol=1e-12)
        lagged = together.strategies[0].market_gate.reindex(together.held_asset.index).shift(1).fillna(0)
        self.assertTrue((together.held_asset[lagged == 0].dropna() == USD_SYMBOL).all())

    def test_filter_history_does_not_change_pairwise_signals(self):
        from rotation_lab.backtest.signals import pair_signal
        def observed(ind):
            calls=[]
            def capture(a,b,cfg):
                sig=pair_signal(a,b,cfg)
                calls.append((a.copy(), None if b is None else b.copy(), cfg, sig))
                return sig
            with patch('rotation_lab.backtest.engine.market_gate', side_effect=lambda f,c: pd.Series(1,index=f.index)), \
                 patch('rotation_lab.backtest.engine.pair_signal',side_effect=capture):
                result=_run('ema_cross',market_filter=True,market_indicator=ind)
            return result,calls
        same, old=observed(None)
        independent, new=observed(IndicatorConfig(trend='defensive_trend'))
        pd.testing.assert_series_equal(same.held_asset,independent.held_asset)
        pd.testing.assert_series_equal(same.equity_strategy,independent.equity_strategy)
        self.assertEqual(len(old),len(new))
        for a,b in zip(old,new):
            pd.testing.assert_frame_equal(a[0],b[0])
            self.assertEqual(a[2],b[2])
            pd.testing.assert_series_equal(a[3],b[3])

    def test_disabled_filter_ignores_independent_signal(self):
        plain=_run('ema_cross',market_filter=False)
        configured=_run('ema_cross',market_filter=False,market_indicator=IndicatorConfig(trend='nope'))
        pd.testing.assert_series_equal(plain.equity_strategy,configured.equity_strategy)

    def test_unknown_market_indicator_fails_before_ranking_or_candles(self):
        with self.assertRaisesRegex(ValueError,'unknown trend signal'):
            BacktestEngine(registry=object(),ohlcv=object()).run(RunConfig(
                market_filter=True,market_indicator=IndicatorConfig(trend='nope')))

    def test_independent_aggregate_and_ema_parameters_roundtrip(self):
        from rotation_lab.rpc import _build_config
        cfg=_build_config({'indicator':{'trend':'hilbert'}, 'marketFilter':True,
            'marketIndicator':{'trend':'aggregate','aggregate':['ema_cross','bretrace'],
                               'emaCross':{'fastLength':5,'slowLength':60,'src':'close'}}})
        self.assertEqual(cfg.indicator.trend,'hilbert')
        self.assertEqual(cfg.market_indicator.ema_cross,EmaCrossConfig(fast_length=5,slow_length=60))
        self.assertEqual(_build_config(asdict(cfg)),cfg)
        self.assertIsNone(_build_config({'marketFilter':True}).market_indicator)

    def test_live_market_signal_changes_gate_but_not_scores_or_ranking_history(self):
        from rotation_lab.rpc import _method_live
        resolved=SimpleNamespace(coins=_coins(),provider='test')
        def run(market):
            with patch('rotation_lab.rpc.RankingRegistry') as reg, \
                 patch('rotation_lab.rpc.get_default_cache'), \
                 patch('rotation_lab.rpc.OhlcvFetcher',return_value=_Ohlcv()), \
                 patch('rotation_lab.rpc._notify'), \
                 patch('rotation_lab.backtest.market.market_gate',
                       side_effect=lambda f,c: pd.Series(int(c.trend=='ema_cross'),index=f.index)) as gate:
                reg.return_value.get_top_n.return_value=resolved
                result=_method_live({'asOf':'2024-09-01','config':{
                    'indicator':{'trend':'ema_cross'},'marketFilter':True,'marketIndicator':market}})
                return result,gate.call_args
        same,_=run(None)
        independent,args=run({'trend':'defensive_trend'})
        self.assertTrue(same['marketFilter']['bullish'])
        self.assertFalse(independent['marketFilter']['bullish'])
        self.assertEqual(independent['best'],'USD')
        self.assertEqual(same['scores'],independent['scores'])
        self.assertEqual(same['scoreMatrix'],independent['scoreMatrix'])
        self.assertEqual(args.args[1].trend,'defensive_trend')
        self.assertLess(args.args[0].index[0],pd.Timestamp('2024-01-01'))

    def test_bearish_total_means_usd_the_next_bar(self):
        result = _run("ema_cross", market_filter=True)
        run = result.strategies[0]
        self.assertIsNotNone(run.market_gate)
        expected = market_gate(market_index(result.universe.snapshots, _cut(FRAMES)), IndicatorConfig(trend="ema_cross"))
        pd.testing.assert_series_equal(run.market_gate, expected)
        lagged = expected.reindex(run.held_asset.index).shift(1).fillna(0)
        off = run.held_asset[lagged == 0]
        self.assertGreater(len(off), 0)
        self.assertTrue((off.dropna() == USD_SYMBOL).all())
        # The filter is not chatter in the notes; the holdings show it.
        self.assertFalse(any("Higher filter" in n for n in result.notes))

    def test_unfiltered_run_is_unchanged_and_differs_from_the_filtered_one(self):
        plain = _run("ema_cross", market_filter=False)
        filtered = _run("ema_cross", market_filter=True)
        self.assertIsNone(plain.strategies[0].market_gate)
        self.assertFalse(plain.held_asset.equals(filtered.held_asset))
        self.assertGreater((filtered.held_asset == USD_SYMBOL).sum(), (plain.held_asset == USD_SYMBOL).sum())

    def test_every_compared_indicator_gates_itself_on_total(self):
        result = _run("ema_cross", market_filter=True, compare=("extremes",))
        gates = [r.market_gate for r in result.strategies]
        self.assertTrue(all(g is not None for g in gates))
        self.assertFalse(gates[0].equals(gates[1]))

    def test_excluded_head_is_in_total_but_never_traded(self):
        def timeline(*_a, exclude_top_n=0, **_k):
            start = dt.date(2024, 1, 1)
            return _timeline(start) if exclude_top_n == 0 else _timeline(start, ("BBB", "CCC"))
        result = _run("ema_cross", market_filter=True, exclude_top_n=1, timeline=timeline)
        held = set(result.held_asset.dropna())
        self.assertNotIn("AAA", held)
        self.assertNotIn("AAA", result.buy_and_hold)
        full = market_gate(market_index(_timeline(dt.date(2024, 1, 1)).snapshots, _cut(FRAMES)), IndicatorConfig(trend="ema_cross"))
        pd.testing.assert_series_equal(result.strategies[0].market_gate, full)


if __name__ == "__main__":
    unittest.main()
