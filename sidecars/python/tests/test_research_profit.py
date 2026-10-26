"""The profit overview: seeded search, native verification, and the report."""
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

from smithery.lab_compare import validate_request
from smithery.lab_search import Search
from tests.test_research_fast import FIXTURES, fixture_source


def fake_evaluate(trials):
    # Deterministic "capital" from the members: more members of 'a*' bases pay more.
    return [(1.0 + sum(len(k) for k in t) / 10 + 0.01 * len(t), 10.0 + len(t)) for t in trials]


class SearchTests(unittest.TestCase):
    def setUp(self):
        self.keys = [f"{b}{v}" for b in "abcdefgh" for v in ("", "_opt")]
        self.bases = {k: k.split("_")[0] for k in self.keys}

    def search(self, budget, seed=42):
        return Search(fake_evaluate, self.keys, self.bases, seed=seed, budget=budget)

    def test_one_vote_per_base_and_budget(self):
        s = self.search(120)
        s.run("Single indicators", [(k,) for k in self.keys], singles=True)
        s.stage_exhaustive_or_sample("Pairs", 2, 0.35)
        s.stage_exhaustive_or_sample("Triples", 3, 0.4)
        s.stage_balanced("Larger aggregates", range(4, 9), 0.4)
        s.stage_adaptive("Local search, round", 3, 1.0)
        self.assertLessEqual(s.aggregates, 120)
        self.assertEqual(len(s.results(size=1)), len(self.keys))
        for trial in s.seen:
            self.assertEqual(len({self.bases[k] for k in trial}), len(trial))
            self.assertEqual(trial, tuple(sorted(trial)))
        self.assertEqual(sum(st["tested"] for st in s.stages if st["stage"] != "Single indicators"), s.aggregates)

    def test_small_spaces_are_exhaustive_and_runs_reproduce(self):
        s = self.search(10_000)
        s.stage_exhaustive_or_sample("Pairs", 2, 0.35)
        self.assertEqual(s.counts()[2], 28 * 4)  # 28 base pairs, two versions each
        runs = []
        for _ in range(2):
            t = self.search(300, seed=7)
            t.stage_exhaustive_or_sample("Pairs", 2, 0.35)
            t.stage_balanced("Larger aggregates", range(3, 9), 0.5)
            t.stage_adaptive("Local search, round", 2, 1.0)
            runs.append((list(t.seen), t.stages))
        self.assertEqual(runs[0], runs[1])
        other = self.search(300, seed=8)
        other.stage_exhaustive_or_sample("Pairs", 2, 0.35)
        other.stage_balanced("Larger aggregates", range(3, 9), 0.5)
        self.assertNotEqual(list(other.seen), runs[0][0][:len(other.seen)])

    def test_neighbours_add_remove_and_swap_distinct_bases(self):
        s = self.search(1000)
        found = set(s.neighbors(("a", "b_opt", "c")))
        self.assertIn(("a", "b_opt"), {tuple(sorted(t)) for t in found})
        self.assertIn(("a", "b_opt", "c", "d"), {tuple(sorted(t)) for t in found})
        self.assertIn(("a", "c", "e_opt"), {tuple(sorted(t)) for t in found})
        self.assertTrue(all(len({self.bases[k] for k in t}) == len(t) for t in found))
        self.assertFalse(any(len(t) < 2 for t in set(s.neighbors(("a", "b")))))

    def test_requests_need_closed_history_and_valid_limits(self):
        doc = {"config": {"startDate": "2020-01-01", "endDate": "2021-01-01", "feeRate": .001, "slippageRate": 0,
                          "indicator": {"trend": "fixture"}}, "indicators": [], "top": 10, "budget": 1000}
        validate_request(doc)
        for field, value in (("feeRate", float("nan")), ("endDate", "2999-01-01"), ("startDate", "2022-01-01")):
            bad = copy.deepcopy(doc); bad["config"][field] = value
            with self.assertRaises(ValueError):
                validate_request(bad)
        for field, value in (("top", 0), ("budget", -1), ("sizes", [1])):
            with self.assertRaises(ValueError):
                validate_request({**doc, field: value})


SCRIPT = r'''
import datetime as dt, json, sys
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import patch
import numpy as np, pandas as pd
from rotation_lab.backtest.universe import UniverseSnapshot, UniverseTimeline
from rotation_lab.data.ohlcv import OhlcvFetcher
from rotation_lab.data.ranking.base import RankedCoin
from smithery import lab
from smithery.lab_profit import run
from smithery.lab_report import render

def frame(seed, start='2018-06-01', end='2021-12-31', drift=.001):
    index = pd.date_range(start, end, freq='D')
    rng = np.random.default_rng(seed)
    close = 100 * np.exp(np.cumsum(drift + .035 * rng.standard_normal(len(index))) + .3 * np.sin(np.arange(len(index)) / (40 + seed)))
    return pd.DataFrame({'open': close * (1 + .004 * rng.standard_normal(len(index))), 'high': close * 1.02,
                         'low': close * .98, 'close': close, 'volume': 1000.0}, index=index)

FRAMES = {'AAA': frame(1), 'BBB': frame(2, drift=.0015), 'CCC': frame(3, drift=-.0005), 'DDD': frame(4), 'FFF': frame(6, drift=.002)}
ORDERS = [['AAA', 'BBB', 'CCC'], ['BBB', 'AAA', 'DDD'], ['FFF', 'BBB', 'CCC'], ['CCC', 'FFF', 'AAA', 'DDD']]

@dataclass
class Series:
    frame: pd.DataFrame

def get_series(self, ref, start, end, timeframe='1d'):
    f = FRAMES[ref.symbol]
    return Series(f.loc[(f.index >= pd.Timestamp(start)) & (f.index <= pd.Timestamp(end))])

def get_pair_series(self, exchange, pair, start, end, timeframe='1d'):
    return get_series(self, type('R', (), {'symbol': 'FFF'})(), start, end)

def timeline(*args, start, end, **kwargs):
    snaps, day, k = [], dt.date(2019, 12, 29), 0
    while day <= end:
        names = ORDERS[(k // 5) % len(ORDERS)]
        snaps.append(UniverseSnapshot(on_date=day, provider='test', coins=[RankedCoin(rank=i + 1, cg_id=None, symbol=s, name=s,
                     market_cap=1e9 - i, price=1.0) for i, s in enumerate(names)]))
        day += dt.timedelta(days=7); k += 1
    return UniverseTimeline(snapshots=snaps)

systems = {
 'rotation': {'mode': 'rotation', 'topN': 4, 'startDate': '2020-01-01', 'endDate': '2021-06-30', 'rankingResolution': 'weekly',
              'includeUsd': True, 'feeRate': .001, 'slippageRate': .0005, 'marketFilter': True,
              'marketIndicator': {'trend': 'total_breakout', 'trendLength': 50, 'entryLength': 5, 'exitLength': 5},
              'indicator': {'trend': 'aggregate', 'aggregate': ['k_mid', 'k_slow'], 'params': {'k_slow': {'band': 0.4}}}},
 'single': {'mode': 'single_asset', 'singleAsset': {'exchange': 'x', 'pair': 'FFF/USD', 'timeframe': '1d', 'direction': 'long_cash'},
            'startDate': '2020-01-01', 'endDate': '2021-06-30', 'feeRate': .001, 'slippageRate': .0005,
            'indicator': {'trend': 'k_mid'}},
}
out = {}
def aggregates():
    from smithery.lab_aggregates import run as compare
    doc = lab.create({'kind': 'aggregates', 'title': 'Aggregates', 'config': systems['rotation'], 'top': 3, 'budget': 60,
                      'seed': 3, 'restarts': ['2021-01-01', '2030-01-01'], 'system_name': 'Fixture rotation'})
    path = lab.folder(doc['id'])
    compare(path, lab.read(path / 'run.json'))
    results = lab.read(path / 'results.json')
    page = render({**doc, 'status': 'done'}, results)
    return {'reference': results['reference'], 'budget': doc['budget'],
            'subjects': [(s['code'], s['group'], s['members'], s['verified'].get('passed')) for s in results['subjects']],
            'stages': [st['stage'] for st in results['coverage']['stages']], 'aggregates': results['coverage']['aggregates'],
            'best_by_size': [b['size'] for b in results['coverage']['best_by_size']],
            'restarts': results['restarts'], 'verified': results['coverage']['verified'], 'passed': results['coverage']['verified_passed'],
            'rows_have_singles': any(r['size'] == 1 for r in results['rows']), 'page': 'The ten most profitable aggregates' in page,
            'remote': bool(__import__('re').search(r"(?i)(?:src|href)\s*=\s*[\"']?(?:https?:)?//", page))}
if __name__ == '__main__':
    with patch.object(OhlcvFetcher, 'get_series', get_series), patch.object(OhlcvFetcher, 'get_pair_series', get_pair_series), \
         patch('rotation_lab.backtest.engine.build_universe_timeline', side_effect=timeline):
        if sys.argv[1:] == ['aggregates']:
            print(json.dumps(aggregates(), default=str))
            sys.exit(0)
        for name, config in systems.items():
            doc = lab.create({'kind': 'profit', 'title': f'Profit {name}', 'config': config, 'top': 3, 'budget': 40, 'seed': 5,
                              'system_name': f'Fixture {name}'})
            path = lab.folder(doc['id'])
            run(path, lab.read(path / 'run.json'))
            results = lab.read(path / 'results.json')
            page = render({**doc, 'status': 'done'}, results)
            (path / 'report.html').write_text(page, encoding='utf-8')
            out[name] = {'subjects': [(s['code'], s['group'], s['members'], s['verified'].get('passed')) for s in results['subjects']],
                         'coverage': {k: results['coverage'][k] for k in ('keys', 'usable', 'singles', 'aggregates', 'by_size', 'stages', 'unsupported', 'evaluator', 'verified', 'verified_passed')},
                         'bars': len(results['dates']), 'equity_lengths': sorted({len(s['equity']) for s in results['subjects']}),
                         'annual_compounds': [abs(np.prod([1 + v / 100 for v in s['annual'].values()]) / s['metrics']['net_return_multiplier'] - 1) for s in results['subjects']],
                         'end': results['system']['end'], 'plan': (path / 'plan.json').is_file(), 'cube_removed': not (path / 'cube').exists(),
                         'page': len(page), 'remote': bool(__import__('re').search(r"(?i)(?:src|href)\s*=\s*[\"']?(?:https?:)?//", page))}
    print(json.dumps(out, default=str))
'''


class ProfitRunTests(unittest.TestCase):
    def run_script(self, *args):
        """Synthetic rotation and single-asset systems in a fresh home and library."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            library = tmp / "library"
            library.mkdir()
            for key, spec in FIXTURES.items():
                (library / f"{key}.py").write_text(fixture_source(key, *spec), encoding="utf-8")
            (tmp / "check.py").write_text(SCRIPT, encoding="utf-8")
            root = Path(__file__).resolve().parents[1]
            env = {**os.environ, "QUANTSCRIPT_INDICATORS_DIR": str(library), "QUANTSUITE_HOME": str(tmp / "home"),
                   "ROTATION_LAB_CACHE": str(tmp / "cache.sqlite"), "PYTHONIOENCODING": "utf-8", "PYTHONPATH": str(root),
                   "SMITHERY_WORKERS": "2"}
            proc = subprocess.run([sys.executable, str(tmp / "check.py"), *args], capture_output=True, text=True,
                                  encoding="utf-8", timeout=900, env=env, cwd=root)
            self.assertEqual(proc.returncode, 0, proc.stdout[-3000:] + proc.stderr[-3000:])
            return json.loads(proc.stdout.strip().splitlines()[-1])

    def test_profit_overview_ranks_verifies_and_renders(self):
        report = self.run_script()
        for name, r in report.items():
            with self.subTest(name):
                groups = [s[1] for s in r["subjects"]]
                self.assertEqual(groups[0], "baseline")
                self.assertEqual(groups.count("single"), 3)
                self.assertEqual(groups.count("aggregate"), 3)
                self.assertTrue(all(s[3] for s in r["subjects"]))
                self.assertEqual(r["coverage"]["verified"], r["coverage"]["verified_passed"])
                self.assertEqual(r["coverage"]["evaluator"], "bulk")
                self.assertEqual(r["coverage"]["singles"], r["coverage"]["usable"])
                self.assertLessEqual(r["coverage"]["aggregates"], 40)
                self.assertEqual(r["equity_lengths"], [r["bars"]])
                self.assertLess(max(r["annual_compounds"]), 1e-9)
                self.assertTrue(r["plan"] and r["cube_removed"])
                self.assertFalse(r["remote"])
                self.assertEqual(r["end"], "2021-06-30")
        # Four fixtures, all usable: 4 singles and every one of the 11 aggregates within the budget.
        self.assertEqual(report["rotation"]["coverage"]["by_size"], {"1": 4, "2": 6, "3": 4, "4": 1})

    def test_aggregate_comparison_searches_sizes_verifies_and_restarts(self):
        r = self.run_script("aggregates")
        self.assertEqual(r["reference"], ["k_mid", "k_slow"])  # the saved aggregate
        self.assertEqual(r["budget"], 60)
        groups = [s[1] for s in r["subjects"]]
        self.assertEqual(groups[0], "reference")
        self.assertEqual(groups.count("aggregate"), 3)
        self.assertTrue(all(s[3] for s in r["subjects"]))
        self.assertEqual(r["verified"], r["passed"])
        self.assertEqual(r["stages"][:2], ["Control: the reference", "Reference neighbours"])
        self.assertIn("Pairs", r["stages"])
        self.assertLessEqual(r["aggregates"], 60)
        self.assertEqual(r["best_by_size"], [2, 3, 4])
        self.assertFalse(r["rows_have_singles"])
        # Only the restart inside the period runs, natively from a fresh start.
        self.assertEqual([x["start"] for x in r["restarts"]], ["2021-01-01"])
        self.assertGreater(r["restarts"][0]["leader"]["net_return_multiplier"], 0)
        self.assertTrue(r["page"])
        self.assertFalse(r["remote"])

if __name__ == "__main__":
    unittest.main()
