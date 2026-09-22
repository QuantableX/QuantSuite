"""Result parity, Windows spawn and bounded compute-pool regressions."""
import os
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest
from dataclasses import replace
from unittest.mock import patch

import numpy as np
import pandas as pd

from rotation_lab.backtest.engine import BacktestEngine, USD_SYMBOL, _switch_legs
from rotation_lab.backtest.signals import pair_signal
from rotation_lab.benchmark import assert_equal, fixture, run
from rotation_lab.backtest import engine
from smithery import parallel


def _identity(task):
    return task


def _worker_identity(task):
    # A second pmap from inside a worker must stay local.
    nested = parallel.pmap(_identity, [(1,), (2,), (3,), (4,)])
    return task[0], os.getpid(), nested


def _reference_rotation(config, timeline, frames, gate):
    """Original scalar execution rules, independent of the array ranking path."""
    dates = sorted(set().union(*(set(f.index) for f in frames.values())))
    index = pd.DatetimeIndex([t for t in dates if config.start_date <= t.date() <= config.end_date])
    cache = {}

    def signal(a, b):
        if (a, b) not in cache:
            cache[a, b] = (pair_signal(frames[a], frames.get(b), config.indicator)
                           if a in frames and (b is None or b in frames) else pd.Series(dtype=int))
        return cache[a, b]

    equity, current, pending = 1.0, None, None
    equities, holdings, forced = [], [], []
    cost = (1 - config.fee_rate) * (1 - config.slippage_rate)
    for ts in index:
        active = [s for s in timeline.snapshots if pd.Timestamp(s.on_date) <= ts]
        symbols = [c.symbol for c in active[-1].coins] if active else []
        if pending and pending != USD_SYMBOL and pending not in symbols:
            forced.append(ts.date())
            pending = USD_SYMBOL if config.include_usd else (symbols[0] if symbols else None)
        if pending is not None:
            legs = _switch_legs(current, pending)
            if legs:
                equity *= cost ** legs
            current = pending
        ret = 0.0
        if current and current != USD_SYMBOL and current in frames and ts in frames[current].index:
            row = frames[current].loc[ts]
            o, c = row['open'], row['close']
            if o and c and o > 0 and not pd.isna(o) and not pd.isna(c):
                ret = (c - o) / o
        equity *= 1 + ret
        equities.append(equity)
        holdings.append(current)
        if not symbols:
            pending = USD_SYMBOL if config.include_usd else current
        elif gate is not None and int(gate.get(ts, 0)) != 1:
            pending = USD_SYMBOL
        else:
            scores = BacktestEngine._score_universe(symbols, ts, signal, config.include_usd)
            order = symbols + ([USD_SYMBOL] if config.include_usd else [])
            pending = max(order, key=lambda s: (scores.get(s, -1), -order.index(s)))
    return (pd.Series(equities, index=index, name='equity'),
            pd.Series(holdings, index=index, name='held').astype(object), forced)


class ArrayParityTests(unittest.TestCase):
    def test_rotation_matches_scalar_with_changing_rank_missing_bars_gates_and_costs(self):
        config, timeline, frames = fixture(800, ('ema_cross',))
        # Missing candles/assets still cast the original default 0 vote.
        frames['AAA'] = frames['AAA'].iloc[::2]
        del frames['EEE']
        frames['BBB'].iloc[650, frames['BBB'].columns.get_loc('close')] = np.nan
        frames['CCC'].iloc[680, frames['CCC'].columns.get_loc('open')] = 0
        for usd in (True, False):
            config = replace(config, include_usd=usd)
            for gated in (True, False):
                gate = pd.Series((np.arange(len(frames['BTC'])) % 17 < 9).astype(int),
                                 index=frames['BTC'].index) if gated else None
                actual = BacktestEngine(registry=object(), ohlcv=object())._simulate(
                    config, timeline, frames, [], gate)
                expected = _reference_rotation(config, timeline, frames, gate)
                pd.testing.assert_series_equal(actual[0], expected[0], check_exact=True)
                pd.testing.assert_series_equal(actual[1], expected[1], check_exact=True)
                self.assertEqual(actual[2], expected[2])

    def test_all_bearish_gate_never_evaluates_unused_pairs(self):
        config, timeline, frames = fixture(100, ('bvc',))
        gate = pd.Series(0, index=frames['BTC'].index)
        with patch.object(engine, 'pair_signal', side_effect=AssertionError('unused pair')):
            result = BacktestEngine(registry=object(), ohlcv=object())._simulate(
                config, timeline, frames, [], gate)
        self.assertTrue((result[1].dropna() == USD_SYMBOL).all())


class PoolTests(unittest.TestCase):
    @classmethod
    def tearDownClass(cls):
        parallel.shutdown_pool()

    def test_single_worker_and_large_payloads_stay_serial(self):
        with patch.object(parallel, 'pool', side_effect=AssertionError('must stay serial')):
            self.assertEqual(parallel.pmap(_identity, [(i,) for i in range(5)], max_workers=1),
                             [(i,) for i in range(5)])
            frame = pd.DataFrame({'close': np.ones(250_001)})
            tasks = [(frame, frame)] * 4
            self.assertEqual(len(parallel.pmap(_identity, tasks)), 4)

    def test_spawn_preserves_order_and_nested_maps_do_not_spawn(self):
        tasks = [(i,) for i in range(8)]
        results = parallel.pmap(_worker_identity, tasks, max_workers=2)
        self.assertEqual([r[0] for r in results], list(range(8)))
        self.assertTrue(all(r[1] != os.getpid() for r in results))
        self.assertLessEqual(len({r[1] for r in results}), 2)
        self.assertTrue(all(r[2] == [(1,), (2,), (3,), (4,)] for r in results))

    def test_parallel_engine_equals_serial_including_skipped_indicators(self):
        data = fixture(7_000, ('ema_cross', 'dc', 'bvc'))
        with patch.dict(os.environ, ROTATION_LAB_WORKERS='1'):
            serial = run(engine, data)
        with patch.dict(os.environ, ROTATION_LAB_WORKERS='2'), \
             patch.object(engine, '_PARALLEL_MIN_SECONDS', 0), \
             patch.object(parallel, 'pmap', wraps=parallel.pmap) as mapped:
            actual = run(engine, data)
        self.assertTrue(mapped.called)
        self.assertGreater(parallel._POOL_WORKERS, 1)
        assert_equal(actual, serial)
        self.assertEqual([s['key'] for s in actual.skipped_strategies], ['bvc'])

    @unittest.skipUnless(os.name == 'nt', 'Windows parent-termination regression')
    def test_workers_exit_when_the_app_terminates_their_parent(self):
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        kernel.WaitForSingleObject.restype = wintypes.DWORD
        kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        source = textwrap.dedent('''
            import json, multiprocessing as mp, os, time
            from smithery.parallel import pool
            if __name__ == '__main__':
                executor = pool(2)
                # Wait until both initializers (including watchdogs) have run.
                futures = [executor.submit(os.getpid) for _ in range(2)]
                [f.result() for f in futures]
                [executor.submit(time.sleep, 120) for _ in range(2)]
                print(json.dumps([p.pid for p in mp.active_children()]), flush=True)
                time.sleep(120)
        ''')
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / 'parent.py'
            script.write_text(source, encoding='utf-8')
            env = {**os.environ, 'PYTHONPATH': str(Path(engine.__file__).parents[2])}
            parent = subprocess.Popen([sys.executable, str(script)], env=env,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                creationflags=subprocess.CREATE_NO_WINDOW)
            handles = []
            try:
                # Bound startup even if worker initialization regresses.
                from concurrent.futures import ThreadPoolExecutor
                with ThreadPoolExecutor(1) as reader:
                    line = reader.submit(parent.stdout.readline)
                    try:
                        pids = json.loads(line.result(timeout=15))
                    except Exception:
                        parent.kill()
                        raise
                self.assertEqual(len(pids), 2)
                handles = [kernel.OpenProcess(0x100001, False, pid) for pid in pids]
                self.assertTrue(all(handles))
                parent.kill()
                parent.wait(timeout=5)
                self.assertTrue(all(kernel.WaitForSingleObject(handle, 5000) == 0
                                    for handle in handles))
            finally:
                if parent.poll() is None:
                    parent.kill()
                parent.wait(timeout=5)
                for handle in handles:
                    if handle:
                        if kernel.WaitForSingleObject(handle, 0) != 0:
                            kernel.TerminateProcess(handle, 1)
                        kernel.CloseHandle(handle)
                parent.stdout.close()
                parent.stderr.close()


if __name__ == '__main__':
    unittest.main()
