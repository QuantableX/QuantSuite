"""The GPU evaluator must give the CPU evaluator's curves bit for bit, throttled or not.

Runs only where PyTorch with CUDA is importable (the research GPU Python); elsewhere it is skipped,
and the app keeps the CPU evaluator."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

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
            # What the searches call: evaluate, with singles and aggregates mixed in one call, a search
            # window, a delay, small chunks, and a key listed twice (it votes twice, as on the CPU).
            mixed = [t for pair in itertools.zip_longest(kinds['singles'], kinds['aggregates']) for t in pair if t]
            mixed.append((usable[0], usable[0], usable[1]))
            cut = len(fast.dates) * 2 // 3
            report[f'{name} evaluate'] = {
                'mixed': fast.evaluate(mixed) == gpu.evaluate(mixed),
                'window': fast.evaluate(mixed, end=cut) == gpu.evaluate(mixed, end=cut),
                'late': fast.evaluate(mixed, delay=1) == gpu.evaluate(mixed, delay=1, chunk=7),
                'duplicate': bool(np.array_equal(fast.simulate([mixed[-1]])[0], gpu.simulate([mixed[-1]]))),
                'trials': len(mixed)}
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
        self.assertEqual(set(report), {f"{n} {k}" for n in ("gate_usd", "no_gate") for k in ("singles", "aggregates", "evaluate")})
        for name in ("gate_usd evaluate", "no_gate evaluate"):
            result = report.pop(name)
            with self.subTest(name):
                self.assertGreater(result["trials"], 10)
                for check in ("mixed", "window", "late", "duplicate"):
                    self.assertTrue(result[check], check)
        for name, result in report.items():
            with self.subTest(name):
                self.assertTrue(result["full_identical"])
                self.assertTrue(result["throttled_identical"])
                self.assertTrue(result["late_identical"])
                self.assertEqual(result["full_pause"], 0)
                self.assertGreater(result["throttled_pause"], 0)
                self.assertGreater(result["throttled_batches"], 2)
                self.assertGreater(result["distinct_curves"], 3)


@unittest.skipUnless(gpu_available(), "PyTorch with CUDA is not installed in this Python")
class GpuSearchTests(unittest.TestCase):
    def test_the_searches_find_and_rank_the_same_on_the_gpu_as_on_the_cpu(self):
        from tests import test_research_profit as profit
        runner = profit.ProfitRunTests("test_runs_read_only_the_saved_price_data")
        runs = {}
        for flag in ("0", "1"):
            with patch.dict(os.environ, {"SMITHERY_GPU": flag}):
                runs[flag] = {"profit": runner.run_script(), "aggregates": runner.run_script("aggregates"),
                              "robust": runner.run_script("robust")}
        cpu, gpu = runs["0"], runs["1"]
        off = "CPU: the GPU is switched off (SMITHERY_GPU=0)"
        self.assertEqual(cpu["profit"]["rotation"]["coverage"]["scoring"], off)
        self.assertEqual((cpu["aggregates"]["scoring"], cpu["robust"]["scoring"]), (off, off))
        for scoring in (gpu["profit"]["rotation"]["coverage"]["scoring"], gpu["aggregates"]["scoring"], gpu["robust"]["scoring"]):
            self.assertRegex(scoring, r"^GPU \(.+\), bit-identical")
        self.assertEqual(gpu["profit"]["single"]["coverage"]["scoring"], "CPU: the GPU scores rotation systems with USD only")
        for run in (cpu, gpu):
            for name in run["profit"].values():
                name["coverage"].pop("scoring")
                name.pop("plan_scoring")
                name.pop("page")  # the page shows the run's seconds
            for kind in ("aggregates", "robust"):
                run[kind].pop("scoring")
        for kind in ("profit", "aggregates", "robust"):
            with self.subTest(kind):
                self.assertEqual(gpu[kind], cpu[kind])


class ScoringChoiceTests(unittest.TestCase):
    """Where a run scores and why: the same on every machine, CUDA or not."""

    def rotation(self, usd=True):
        from smithery.lab_fast import FastSystem
        system = FastSystem.__new__(FastSystem)
        system.single, system.config, system.folder = False, SimpleNamespace(include_usd=usd), "nowhere"
        return system

    def test_every_reason_for_the_cpu_is_named(self):
        from smithery import lab_gpu
        with patch.dict(os.environ, {"SMITHERY_GPU": "0"}):
            self.assertEqual(lab_gpu.cpu_reason(self.rotation()), "the GPU is switched off (SMITHERY_GPU=0)")
        with patch.dict(os.environ, {"SMITHERY_GPU": "1"}):
            self.assertIn("no bulk evaluator", lab_gpu.cpu_reason(object()))
            self.assertEqual(lab_gpu.cpu_reason(self.rotation(usd=False)), "the GPU scores rotation systems with USD only")
            with patch.dict(sys.modules, {"torch": None}):
                self.assertIn("PyTorch is not installed", lab_gpu.cpu_reason(self.rotation()))
            no_cuda = SimpleNamespace(cuda=SimpleNamespace(is_available=lambda: False))
            with patch.dict(sys.modules, {"torch": no_cuda}):
                self.assertEqual(lab_gpu.cpu_reason(self.rotation()), "PyTorch finds no CUDA GPU")

    def test_a_run_says_where_it_scores_and_never_falls_back_silently(self):
        from smithery import lab_gpu
        said = []
        with patch.dict(os.environ, {"SMITHERY_GPU": "0"}):
            twin, note = lab_gpu.scorer(self.rotation(), said.append)
        self.assertIsNone(twin)
        self.assertEqual(note, "CPU: the GPU is switched off (SMITHERY_GPU=0)")
        self.assertEqual(said, ["Scoring on the CPU: the GPU is switched off (SMITHERY_GPU=0)"])
        # The GPU is there but cannot take the cube (out of memory): the CPU, and the reason is shown.
        said.clear()
        with patch.object(lab_gpu, "cpu_reason", return_value=None), \
                patch.object(lab_gpu, "GpuSystem", side_effect=RuntimeError("CUDA out of memory")):
            out, note = lab_gpu.twins({"saved": self.rotation(), "noise 1": self.rotation()}, said.append)
        self.assertEqual(out, {})
        self.assertEqual(note, "CPU: the GPU could not take the cube (RuntimeError: CUDA out of memory)")
        self.assertEqual(said, [f"Scoring on the {note}"])


if __name__ == "__main__":
    unittest.main()
