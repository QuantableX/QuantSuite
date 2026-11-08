"""The GPU evaluator must give the CPU evaluator's curves bit for bit, throttled or not.

Runs only where PyTorch with CUDA is importable (the research GPU Python); elsewhere it is skipped,
and the app keeps the CPU evaluator."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from smithery.quantscript import template
from tests.test_research_fast import FIXTURES, SCRIPT, fixture_source

GPU_MAIN = r'''
import os
from smithery import pace
from smithery.lab_gpu import GpuSystem

throttle = Path(sys.argv[3])
paused = []
done = pace.Pacer.done
def recording_done(self):
    before = self.paused
    done(self)
    paused.append(self.paused - before)
pace.Pacer.done = recording_done

systems = {'gate_usd': configs['gate_usd'], 'no_gate': replace(configs['gate_usd'], market_filter=False)}
if __name__ == '__main__':
    with patch('rotation_lab.backtest.engine.build_universe_timeline', side_effect=timeline):
        for name, config in systems.items():
            engine = BacktestEngine(registry=object(), ohlcv=Ohlcv())
            fast = FastSystem(config, keys, out / name, engine, workers=int(sys.argv[2])).build()
            usable = fast.usable
            gpu = GpuSystem(out / name)
            # The GPU scores singles and aggregates in separate calls, as its callers do.
            kinds = {'singles': [(k,) for k in usable],
                     'aggregates': [c for n in range(2, len(usable) + 1) for c in itertools.combinations(usable, n)]}
            for kind, trials in kinds.items():
                trials = trials * 20  # several batches once throttled
                cpu = fast.simulate(trials)[0]
                cpu_late = fast.simulate(trials, delay=1)[0]
                os.environ.pop('SMITHERY_THROTTLE_FILE', None)
                paused.clear()
                full = gpu.simulate(trials)
                full_pause = sum(paused)
                throttle.write_text(json.dumps({'percent': 10}), encoding='utf-8')
                os.environ['SMITHERY_THROTTLE_FILE'] = str(throttle)
                pace._cache['read'] = float('-inf')
                paused.clear()
                slow = gpu.simulate(trials)
                late = gpu.simulate(trials, delay=1)
                report[f'{name} {kind}'] = {
                    'trials': len(trials), 'full_identical': bool(np.array_equal(full, cpu)),
                    'throttled_identical': bool(np.array_equal(slow, cpu)),
                    'late_identical': bool(np.array_equal(late, cpu_late)),
                    'full_pause': full_pause, 'throttled_pause': sum(paused), 'throttled_batches': len(paused),
                    'distinct_curves': len({tuple(np.round(c, 12)) for c in cpu})}
                os.environ.pop('SMITHERY_THROTTLE_FILE', None)
            fast.close()
    print(json.dumps(report, default=str))
'''


def gpu_available():
    try:
        import torch
        return bool(torch.cuda.is_available())
    except Exception:  # noqa: BLE001
        return False


@unittest.skipUnless(gpu_available(), "PyTorch with CUDA is not installed in this Python")
class GpuParityTests(unittest.TestCase):
    def test_gpu_curves_equal_the_cpu_evaluator_bit_for_bit_at_full_speed_and_throttled(self):
        script = SCRIPT.split("report = {}")[0] + "report = {}\nout = Path(sys.argv[1])\n" + GPU_MAIN
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            library = tmp / "library"
            library.mkdir()
            for key, spec in FIXTURES.items():
                (library / f"{key}.py").write_text(fixture_source(key, *spec), encoding="utf-8")
            volume = template("k_vol", "FixtureVolume", "k_vol").replace(
                'close = df["close"].astype(float)', 'close = df["close"].astype(float) * (1 + 0 * df["volume"].astype(float))')
            (library / "k_vol.py").write_text(volume, encoding="utf-8")
            (tmp / "check.py").write_text(script, encoding="utf-8")
            env = {**__import__("os").environ, "QUANTSCRIPT_INDICATORS_DIR": str(library), "PYTHONIOENCODING": "utf-8",
                   "PYTHONPATH": str(Path(__file__).resolve().parents[1])}
            env.pop("SMITHERY_THROTTLE_FILE", None)
            proc = subprocess.run([sys.executable, str(tmp / "check.py"), str(tmp / "cubes"), "2", str(tmp / "level.json")],
                                  capture_output=True, text=True, encoding="utf-8", timeout=900, env=env,
                                  cwd=Path(__file__).resolve().parents[1])
            self.assertEqual(proc.returncode, 0, proc.stdout[-3000:] + proc.stderr[-3000:])
            report = json.loads(proc.stdout.strip().splitlines()[-1])
        self.assertEqual(set(report), {f"{n} {k}" for n in ("gate_usd", "no_gate") for k in ("singles", "aggregates")})
        for name, result in report.items():
            with self.subTest(name):
                self.assertTrue(result["full_identical"])
                self.assertTrue(result["throttled_identical"])
                self.assertTrue(result["late_identical"])
                self.assertEqual(result["full_pause"], 0)
                self.assertGreater(result["throttled_pause"], 0)
                self.assertGreater(result["throttled_batches"], 2)
                self.assertGreater(result["distinct_curves"], 3)


if __name__ == "__main__":
    unittest.main()
