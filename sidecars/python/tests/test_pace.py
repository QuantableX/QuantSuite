"""GPU pacing: the research throttle's level slows GPU work and never changes it."""
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from smithery import pace


class Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class PaceTests(unittest.TestCase):
    def setUp(self):
        pace._cache.update(path=None, read=float("-inf"), level=100)
        self.tmp = tempfile.TemporaryDirectory()
        self.file = Path(self.tmp.name) / "level.json"

    def tearDown(self):
        pace._cache.update(path=None, read=float("-inf"), level=100)
        self.tmp.cleanup()

    def set_level(self, value):
        self.file.write_text(json.dumps({"percent": value}), encoding="utf-8")
        pace._cache["read"] = float("-inf")  # read again at once, as after REREAD_SECONDS

    def test_no_throttle_file_means_full_speed_and_no_synchronisation(self):
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("SMITHERY_THROTTLE_FILE", None)
            self.assertEqual(pace.level(), 100)
            self.assertEqual(pace.batch(1024, 16), 1024)
            synced, slept = [], []
            pacer = pace.Pacer(sync=lambda: synced.append(1), sleep=slept.append)
            pacer.start()
            pacer.done()
            self.assertEqual((synced, slept, pacer.paused), ([], [], 0.0))

    def test_unreadable_or_odd_files_mean_full_speed_or_a_clamped_level(self):
        with patch.dict(os.environ, {"SMITHERY_THROTTLE_FILE": str(self.file)}):
            self.assertEqual(pace.level(), 100)  # missing
            self.file.write_text("not json", encoding="utf-8")
            pace._cache["read"] = float("-inf")
            self.assertEqual(pace.level(), 100)
            self.set_level(0)
            self.assertEqual(pace.level(), 1)
            self.set_level(250)
            self.assertEqual(pace.level(), 100)

    def test_a_level_pauses_so_the_gpu_works_that_share_of_the_time(self):
        clock, slept, synced = Clock(), [], []
        with patch.dict(os.environ, {"SMITHERY_THROTTLE_FILE": str(self.file)}):
            for percent, busy, expected in ((10, 0.2, 1.8), (25, 0.4, 1.2), (50, 0.3, 0.3)):
                self.set_level(percent)
                pacer = pace.Pacer(sync=lambda: synced.append(1), sleep=slept.append, clock=clock)
                pacer.start()
                clock.now += busy
                pacer.done()
                self.assertAlmostEqual(slept[-1], expected)
                self.assertAlmostEqual(busy / (busy + slept[-1]), percent / 100)
            self.assertEqual(len(synced), 3)  # it waits for the GPU before it measures
            self.assertEqual(pace.batch(1024, 16), 512)
            self.set_level(1)
            self.assertEqual(pace.batch(1024, 16), 16)

    def test_back_to_full_takes_effect_at_the_next_batch(self):
        clock, slept = Clock(), []
        with patch.dict(os.environ, {"SMITHERY_THROTTLE_FILE": str(self.file)}):
            self.set_level(10)
            pacer = pace.Pacer(sleep=slept.append, clock=clock)
            pacer.start()
            clock.now += 0.1
            self.set_level(100)  # "Leistung wieder voll" while a batch runs
            pacer.done()
            pacer.start()
            clock.now += 0.1
            pacer.done()
            self.assertEqual(slept, [])
            self.assertEqual(pace.batch(1024, 16), 1024)

    def test_the_level_is_read_at_most_once_a_second(self):
        with patch.dict(os.environ, {"SMITHERY_THROTTLE_FILE": str(self.file)}):
            self.set_level(25)
            self.assertEqual(pace.level(), 25)
            self.file.write_text(json.dumps({"percent": 50}), encoding="utf-8")
            self.assertEqual(pace.level(), 25)
            pace._cache["read"] -= pace.REREAD_SECONDS
            self.assertEqual(pace.level(), 50)


if __name__ == "__main__":
    unittest.main()
