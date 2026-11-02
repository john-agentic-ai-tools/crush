#!/usr/bin/env python3
"""Tests for compare.py. Run: python perf/test_compare.py"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compare  # noqa: E402
import compare_gpu  # noqa: E402


class Spread(unittest.TestCase):
    def test_one_stray_run_does_not_set_the_iqr(self):
        steady = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        self.assertEqual(compare.iqr_pct(steady), 0.0)
        self.assertEqual(compare.iqr_pct(steady[:-1] + [3.0]), 0.0)

    def test_iqr_grows_with_real_variation(self):
        self.assertGreater(compare.iqr_pct([1.0, 1.2, 1.4, 1.6, 1.8]), 20)

    def test_one_value_has_no_spread(self):
        self.assertEqual(compare.iqr_pct([2.0]), 0.0)


class GpuCheck(unittest.TestCase):
    GOOD = 'tile_count=2048 backend="CUDA"\nGPU decompression succeeded (1 bytes)'

    def test_a_completed_gpu_run_passes(self):
        self.assertIsNone(compare_gpu.backend_check(self.GOOD, "CUDA"))

    def test_a_fallback_is_a_failure(self):
        log = self.GOOD + "\nGPU decompression failed, falling back to CPU: mismatch"
        self.assertIn("CPU", compare_gpu.backend_check(log, "CUDA"))

    def test_the_wrong_backend_is_a_failure(self):
        self.assertIn("expected", compare_gpu.backend_check(self.GOOD, "wgpu"))

    def test_colour_codes_do_not_hide_the_backend(self):
        log = 'tile_count\x1b[0m=2048 \x1b[3mbackend\x1b[0m="CUDA"\nGPU decompression succeeded'
        self.assertIsNone(compare_gpu.backend_check(log, "CUDA"))


if __name__ == "__main__":
    unittest.main()
