"""The forge mirrors its shelf into the suite's shared candle archive.

The CSV shelf stays the forge's own source; the archive gets the same bars,
exactly, for the venues QuantAlgo reads the same way."""
import os
import re
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pandas as pd

from smithery import data
from smithery.candle_archive import HOLE_BARS, SCHEMA, CandleArchive, covered, missing

RUST_ARCHIVE = Path(__file__).resolve().parents[3] / "modules" / "algo" / "crate" / "src" / "candle_archive.rs"
HOUR_MS = 3_600_000


def frame(start: str, bars: int, close: float = 0.1) -> pd.DataFrame:
    index = pd.date_range(start, periods=bars, freq="h", tz="UTC")
    # Values whose shortest repr is long: a sloppy float parse changes them.
    base = [close + i * 0.1 + 1e-13 * (i % 7) for i in range(bars)]
    return pd.DataFrame(dict(open=base, high=[b * 1.000000000000003 for b in base], low=base,
                             close=base, volume=[b * 123.456789012345 for b in base]), index=index)


class ArchiveMirrorTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        for p in (patch.object(data, "PRICE_DIR", Path(self.folder.name)),
                  patch.dict(os.environ, {"QUANTSUITE_HOME": self.folder.name})):
            p.start()
            self.addCleanup(p.stop)
        self.archive = CandleArchive()
        self.addCleanup(self.archive.close)

    def stored(self, venue, symbol, tf):
        return self.archive.con.execute(
            "SELECT open_ms, open, high, low, close, volume FROM candles"
            " WHERE venue = ? AND symbol = ? AND tf = ? ORDER BY open_ms", (venue, symbol, tf)).fetchall()

    def test_keys_map_to_the_suite_pair_and_only_verified_venues(self):
        self.assertEqual(data.archive_series("BINANCE_BTCUSDT_1h"), ("binance", "BTC/USDT", "1h"))
        self.assertEqual(data.archive_series("COINBASE_BTCUSD_1d"), ("coinbase", "BTC/USD", "1d"))
        self.assertIsNone(data.archive_series("KUCOIN_SOLUSDT_1d"), "ccxt rounds some KuCoin volumes differently")
        self.assertIsNone(data.archive_series("OKX_BTCUSDT_1d"), "OKX daily bars differ between the two fetchers")
        self.assertEqual(data.archive_series("OKX_BTCUSDT_4h"), ("okx", "BTC/USDT", "4h"))
        self.assertIsNone(data.archive_series("BITSTAMP_BTCUSD_1d"))

    def test_mirror_stores_the_exact_bars_once_and_then_only_the_new_tail(self):
        df = frame("2024-01-01", 48)
        data._save("BINANCE_BTCUSDT_1h", df)
        self.assertEqual(data.mirror_to_archive("BINANCE_BTCUSDT_1h", self.archive), 48)
        rows = self.stored("binance", "BTC/USDT", "1h")
        self.assertEqual([r[0] for r in rows], [int(t.value // 1_000_000) for t in df.index])
        for column, i in (("open", 1), ("high", 2), ("close", 4), ("volume", 5)):
            self.assertEqual([r[i] for r in rows], df[column].tolist(), column)  # bit for bit
        self.assertEqual(self.archive.spans("binance", "BTC/USDT", "1h"),
                         [(rows[0][0], rows[-1][0])])

        self.assertEqual(data.mirror_to_archive("BINANCE_BTCUSDT_1h", self.archive), 0)
        data._save("BINANCE_BTCUSDT_1h", pd.concat([df, frame("2024-01-03", 5, close=9.9)]))
        self.assertEqual(data.mirror_to_archive("BINANCE_BTCUSDT_1h", self.archive), 5)
        self.assertEqual(len(self.stored("binance", "BTC/USDT", "1h")), 53)
        self.assertEqual(len(self.archive.spans("binance", "BTC/USDT", "1h")), 1)

    def test_a_stored_bar_is_never_rewritten_and_unverified_series_stay_out(self):
        data._save("BINANCE_ETHUSDT_1h", frame("2024-01-01", 3))
        data.mirror_to_archive("BINANCE_ETHUSDT_1h", self.archive)
        before = self.stored("binance", "ETH/USDT", "1h")
        self.archive.record("binance", "ETH/USDT", "1h", [(before[0][0], 1, 1, 1, 1, 1)], [(before[0][0], before[0][0])])
        self.assertEqual(self.stored("binance", "ETH/USDT", "1h"), before)
        data._save("OKX_BTCUSDT_1d", frame("2024-01-01", 3))
        self.assertEqual(data.mirror_to_archive("OKX_BTCUSDT_1d", self.archive), 0)
        self.assertEqual(self.stored("okx", "BTC/USDT", "1d"), [])

    def test_mirror_shelf_covers_every_verified_csv_and_never_fails_the_refresh(self):
        for key in ("BINANCE_BTCUSDT_1h", "BYBIT_BTCUSDT_4h", "BITSTAMP_BTCUSD_1d", "KUCOIN_BTCUSDT_1h"):
            data._save(key, frame("2024-01-01", 4))
        lines = []
        self.assertEqual(data.mirror_shelf(lines.append), 8)
        self.assertTrue(any("8 bars mirrored" in line for line in lines))
        with patch("smithery.candle_archive.CandleArchive", side_effect=OSError("disk full")):
            self.assertEqual(data.mirror_shelf(lines.append), 0)
        self.assertIn("candle archive unavailable", lines[-1])

    def test_missing_ranges(self):
        self.assertEqual(missing([(10, 19), (30, 39)], 0, 50), [(0, 9), (20, 29), (40, 50)])
        self.assertEqual(missing([(10, 19)], 12, 18), [])

    def test_covered_matches_the_rust_rule(self):
        h = HOUR_MS
        self.assertEqual(covered(0, 99 * h, [50 * h, 51 * h], h, True), [(0, 51 * h)])
        self.assertEqual(covered(0, 99 * h, [50 * h, 51 * h], h, False), [(50 * h, 51 * h)])
        self.assertEqual(covered(0, 99 * h, [0, h, 90 * h, 99 * h], h, False), [(0, h), (90 * h, 99 * h)])
        self.assertEqual(covered(0, 99 * h, [], h, False), [])

    def test_a_long_hole_in_the_shelf_is_left_for_quantalgo_to_fetch(self):
        df = frame("2024-01-01", 100)
        gap = df.index[30:30 + HOLE_BARS + 1]
        data._save("BYBIT_BTCUSDT_1h", df.drop(gap))
        data.mirror_to_archive("BYBIT_BTCUSDT_1h", self.archive)
        first, last = (int(t.value // 1_000_000) for t in (df.index[0], df.index[-1]))
        self.assertEqual(self.archive.spans("bybit", "BTC/USDT", "1h"),
                         [(first, first + 29 * HOUR_MS), (first + 55 * HOUR_MS, last)])

    @unittest.skipUnless(RUST_ARCHIVE.exists(), "QuantAlgo source not in this tree")
    def test_python_and_rust_create_the_same_schema(self):
        rust = RUST_ARCHIVE.read_text(encoding="utf-8")
        rust_schema = re.search(r'const SCHEMA: &str = "(.*?)";', rust, re.S).group(1)
        squash = lambda sql: " ".join(sql.split())  # noqa: E731
        self.assertEqual(squash(rust_schema), squash(SCHEMA))


if __name__ == "__main__":
    unittest.main()
