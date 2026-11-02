#!/usr/bin/env python3
"""Tests for compare.py. Run: python perf/test_compare.py"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compare  # noqa: E402


class Spread(unittest.TestCase):
    def test_one_stray_run_does_not_set_the_iqr(self):
        steady = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        self.assertEqual(compare.iqr_pct(steady), 0.0)
        self.assertEqual(compare.iqr_pct(steady[:-1] + [3.0]), 0.0)

    def test_iqr_grows_with_real_variation(self):
        self.assertGreater(compare.iqr_pct([1.0, 1.2, 1.4, 1.6, 1.8]), 20)

    def test_one_value_has_no_spread(self):
        self.assertEqual(compare.iqr_pct([2.0]), 0.0)


if __name__ == "__main__":
    unittest.main()
