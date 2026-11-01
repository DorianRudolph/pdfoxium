#!/usr/bin/env python3
# Copyright (c) 2026 DorianRudolph
# SPDX-License-Identifier: MIT
"""Focused checks for benchmark accounting and exact semantic verification."""
import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("bench_text", Path(__file__).with_name("bench-text.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
profile_spec = importlib.util.spec_from_file_location("matching_samples", Path(__file__).with_name("analyze-matching-samples.py"))
profiles = importlib.util.module_from_spec(profile_spec)
profile_spec.loader.exec_module(profiles)


class BenchmarkChecks(unittest.TestCase):
    def test_sampling_attribution_uses_exclusive_weights_under_matching_root(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "profile.sample.txt"
            path.write_text("""Call graph:
    100 Thread_1 com.apple.main-thread
      100 start
        90 (anonymous namespace)::MatchingPass()
          60 FPDFText_FindStart
            40 cpdf_textpagefind5lower0
            20 15unicode_scalars
          20 FPDFText_FindNext
        10 unrelated_setup
Total number in stack
""")
            result = profiles.analyze(path)
            self.assertEqual(result["matching_samples"], 90)
            self.assertEqual(result["stages"], {"setup": 60, "scan": 20, "harness": 10})
            self.assertEqual(result["inclusive"]["case_folding"], 40)
            self.assertEqual(result["inclusive"]["scalar_decoding_and_position_map"], 20)

    def test_stage_total_includes_buffer_and_document_teardown(self):
        result = bench.totals({"open_ms": 1, "document_close_ms": 2, "pages": [
            {"page_load_ms": 3, "text_load_ms": 4, "buffer_ms": 5,
             "operation_ms": 6, "close_ms": 7, "characters": 10, "results": 8}]})
        self.assertEqual(result["total_ms"], 28)
        self.assertEqual(result["close_ms"], 9)
        self.assertEqual(result["pages"], 1)

    def test_exact_text_and_match_ranges_not_just_counts(self):
        with tempfile.TemporaryDirectory() as temp:
            a, b = Path(temp) / "a", Path(temp) / "b"
            a.write_bytes(struct.pack("<III", 0, 2, 2) + "AB".encode("utf-16-le"))
            b.write_bytes(a.read_bytes())
            self.assertTrue(bench.compare(a, b, "extract")["equal"])
            b.write_bytes(struct.pack("<III", 0, 2, 2) + "AC".encode("utf-16-le"))
            self.assertFalse(bench.compare(a, b, "extract")["equal"])
            a.write_bytes(struct.pack("<IIIII", 0, 10, 1, 2, 3))
            b.write_bytes(struct.pack("<IIIII", 0, 10, 1, 3, 3))
            difference = bench.compare(a, b, "search")
            self.assertFalse(difference["equal"])
            self.assertEqual(difference["differences"][0]["native_ranges"], [(2, 3)])
            b.write_bytes(b.read_bytes()[:-1])
            with self.assertRaises(ValueError):
                bench.records(b, "search")


if __name__ == "__main__":
    unittest.main()
