"""The bulk evaluator must reproduce the native engine bit for bit."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from smithery.quantscript import template

DEFAULTS = '{"fast": 20, "slow": 100, "band": 0.25, "vol_halflife": 20}'
FIXTURES = {  # key: (fast, slow, band, warm-up bars)
    "k_fast": (5, 20, 0.1, 30), "k_mid": (10, 50, 0.2, 60), "k_mid2": (8, 40, 0.3, 60), "k_slow": (20, 100, 0.25, 120)}
HYSTERESIS = 'return hysteresis_flip(score, float(self.params["band"]))'
# Diverse votes: a contrarian member and a fast-flipping slope member, so even
# member counts meet even splits (the contract allows 0 only before commitment,
# which the different warm-ups cover).
VARIANTS = {"k_fast": 'return -hysteresis_flip(score, float(self.params["band"]))',
            "k_mid2": 'return hysteresis_flip(np.r_[np.nan, np.diff(score)] * 10, float(self.params["band"]) / 10)'}


def fixture_source(key, fast, slow, band, warmup):
    source = template(key, "Fixture" + key.title().replace("_", ""), key)
    assert source.count(DEFAULTS) == 1 and source.count(HYSTERESIS) == 1
    source = source.replace(DEFAULTS, json.dumps({"fast": fast, "slow": slow, "band": band, "vol_halflife": 20}))
    source = source.replace(HYSTERESIS, VARIANTS.get(key, HYSTERESIS))
    return source.replace(f'WARMUP = {{"{key}": 400}}', f'WARMUP = {{"{key}": {warmup}}}')

SCRIPT = r'''
import datetime as dt, itertools, json, sys
from dataclasses import dataclass, replace
from pathlib import Path
from unittest.mock import patch
import numpy as np, pandas as pd
from rotation_lab.backtest.engine import BacktestEngine
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.config import IndicatorConfig, RankingResolution, RunConfig, SingleAssetConfig, TotalBreakoutConfig
from rotation_lab.data.ranking.base import RankedCoin
from smithery.lab_fast import FastSystem

def frame(seed, start='2019-01-01', end='2021-12-31', drift=.001):
    index = pd.date_range(start, end, freq='D')
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(drift + .035 * rng.standard_normal(len(index))) + .3 * np.sin(np.arange(len(index)) / (40 + seed)))
    f = pd.DataFrame({'open': close * (1 + .004 * rng.standard_normal(len(index))), 'high': close * 1.02,
                      'low': close * .98, 'close': close, 'volume': 1000.0}, index=index)
    return f

FRAMES = {'AAA': frame(1), 'BBB': frame(2, drift=.0015), 'CCC': frame(3, drift=-.0005), 'DDD': frame(4),
          'EEE': frame(5, start='2020-03-15'), 'FFF': frame(6, drift=.002)}
FRAMES['BBB'] = FRAMES['BBB'].drop(FRAMES['BBB'].index[400:409])  # a gap inside the window
ORDERS = [['AAA', 'BBB', 'CCC'], ['BBB', 'AAA', 'DDD'], ['FFF', 'BBB', 'GHOST'], ['EEE', 'AAA', 'CCC'],
          ['CCC', 'FFF', 'AAA', 'DDD'], ['AAA', 'EEE', 'BBB']]

@dataclass
class Series:
    frame: pd.DataFrame

class Ohlcv:
    def __init__(self): self.calls = 0
    def get_series(self, ref, start, end, timeframe):
        self.calls += 1
        if ref.symbol not in FRAMES: raise ValueError('no candles')
        f = FRAMES[ref.symbol]
        return Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])
    def get_pair_series(self, exchange, pair, start, end, timeframe='1d'):
        f = FRAMES['FFF']
        return Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])

def timeline(*args, start, end, **kwargs):
    snaps, day, k = [], dt.date(2019, 12, 25), 0
    while day <= end:
        names = ORDERS[(k // 6) % len(ORDERS)]
        coins = [RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s, market_cap=1e9 - i, price=1.0) for i, s in enumerate(names)]
        snaps.append(UniverseSnapshot(on_date=day, provider='test', coins=coins))
        day += dt.timedelta(days=7); k += 1
    return UniverseTimeline(snapshots=snaps)

keys = ['k_fast', 'k_mid', 'k_mid2', 'k_slow', 'k_vol']
params = {'k_slow': {'band': 0.4}}
configs = {
 'gate_usd': RunConfig(top_n=4, start_date=dt.date(2020, 1, 1), end_date=dt.date(2021, 6, 30), ranking_resolution=RankingResolution.WEEKLY,
     indicator=IndicatorConfig(trend='k_mid', params=params), include_usd=True, fee_rate=.001, slippage_rate=.0005,
     market_filter=True, market_indicator=TotalBreakoutConfig(trend_length=50, entry_length=5, exit_length=5)),
 'no_usd_indicator_gate': RunConfig(top_n=4, start_date=dt.date(2020, 2, 1), end_date=dt.date(2021, 9, 30), ranking_resolution=RankingResolution.WEEKLY,
     indicator=IndicatorConfig(trend='k_mid', params=params), include_usd=False, fee_rate=.002,
     market_filter=True, market_indicator=IndicatorConfig(trend='k_slow')),
 'single_long_cash': RunConfig(mode='single_asset', start_date=dt.date(2020, 1, 1), end_date=dt.date(2021, 9, 30),
     single_asset=SingleAssetConfig(exchange='x', pair='FFF/USD', timeframe='1d', direction='long_cash'),
     indicator=IndicatorConfig(trend='k_mid', params=params), fee_rate=.001, slippage_rate=.0005),
 'single_long_short': RunConfig(mode='single_asset', start_date=dt.date(2020, 1, 1), end_date=dt.date(2021, 9, 30),
     single_asset=SingleAssetConfig(exchange='x', pair='FFF/USD', timeframe='1d', direction='long_short'),
     indicator=IndicatorConfig(trend='k_mid', params=params), fee_rate=.001),
}
report = {}
out = Path(sys.argv[1])
if __name__ == '__main__':
    with patch('rotation_lab.backtest.engine.build_universe_timeline', side_effect=timeline):
        for name, config in configs.items():
            engine = BacktestEngine(registry=object(), ohlcv=Ohlcv())
            fast = FastSystem(config, keys, out / name, engine, workers=int(sys.argv[2])).build()
            usable = fast.usable
            trials = [(k,) for k in usable] + [c for n in range(2, len(usable) + 1) for c in itertools.combinations(usable, n)]
            curve, held = fast.simulate(trials)
            checks = [fast.verify(t) for t in trials]
            bulk = fast.metrics(curve)
            parallel = fast.evaluate(trials + [('k_vol', 'k_fast')], chunk=4)
            report[name] = {'failed': fast.failed, 'groups': [g.start for g in fast.groups], 'trials': len(trials),
                'passed': sum(c['passed'] for c in checks), 'bit_identical': sum(c['bit_identical'] for c in checks),
                'bad': [c for c in checks if not c['bit_identical']][:2],
                'bulk_matches_series': bool(all(abs(bulk['net_return_multiplier'][i] - checks[i]['metrics']['net_return_multiplier']) == 0 and
                    abs(bulk['max_drawdown_pct'][i] - checks[i]['metrics']['max_drawdown_pct']) < 1e-12 for i in range(len(trials)))),
                'parallel_matches': bool(parallel[-1] is None if fast.failed else True) and all(
                    parallel[i] == (float(bulk['net_return_multiplier'][i]), float(bulk['max_drawdown_pct'][i])) for i in range(len(trials))),
                'distinct_curves': len({tuple(np.round(c, 12)) for c in curve}),
                'switches': int(sum(int((np.diff(h) != 0).sum()) for h in held))}
            fast.close()
    print(json.dumps(report, default=str))
'''


class FastEvaluatorParityTests(unittest.TestCase):
    def test_singles_and_aggregates_match_the_native_engine_bit_for_bit(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            library = tmp / "library"
            library.mkdir()
            for key, spec in FIXTURES.items():
                (library / f"{key}.py").write_text(fixture_source(key, *spec), encoding="utf-8")
            volume = template("k_vol", "FixtureVolume", "k_vol").replace(
                'close = df["close"].astype(float)', 'close = df["close"].astype(float) * (1 + 0 * df["volume"].astype(float))')
            (library / "k_vol.py").write_text(volume, encoding="utf-8")
            (tmp / "check.py").write_text(SCRIPT, encoding="utf-8")
            env = {**__import__("os").environ, "QUANTSCRIPT_INDICATORS_DIR": str(library), "PYTHONIOENCODING": "utf-8",
                   "PYTHONPATH": str(Path(__file__).resolve().parents[1])}
            proc = subprocess.run([sys.executable, str(tmp / "check.py"), str(tmp / "cubes"), "2"], capture_output=True,
                                  text=True, encoding="utf-8", timeout=900, env=env, cwd=Path(__file__).resolve().parents[1])
            self.assertEqual(proc.returncode, 0, proc.stdout[-3000:] + proc.stderr[-3000:])
            report = json.loads(proc.stdout.strip().splitlines()[-1])
        for name, result in report.items():
            with self.subTest(name):
                self.assertEqual(result["bad"], [])
                self.assertEqual(result["bit_identical"], result["trials"])
                self.assertEqual(result["passed"], result["trials"])
                self.assertTrue(result["bulk_matches_series"])
                self.assertTrue(result["parallel_matches"])
                self.assertGreater(result["distinct_curves"], result["trials"] // 2)
                self.assertGreater(result["switches"], 0)
        rotation = report["gate_usd"]
        self.assertEqual(rotation["trials"], 15)  # 4 singles and all 11 aggregates of 2-4 members
        self.assertIn("k_vol", rotation["failed"])
        self.assertIn("no traded volume", rotation["failed"]["k_vol"])
        self.assertEqual(len(rotation["groups"]), 4)  # warm-ups 30, 60, 120 and the volume fixture's 400 bars
        # Volume exists on the single market's candles, so it is usable there.
        self.assertEqual(report["single_long_cash"]["failed"], {})
        self.assertEqual(report["single_long_cash"]["trials"], 31)


if __name__ == "__main__":
    unittest.main()
