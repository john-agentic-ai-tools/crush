#!/usr/bin/env python3
"""Tests for the gate report in compare.py. Run: python perf/test_compare.py"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import compare  # noqa: E402


def row(tool, cores, comp, decomp, ratio=3.7, data="real", spread=5.0):
    return {"cores": cores, "data": data, "tool": tool, "comp_mbs": comp,
            "decomp_mbs": decomp, "ratio": ratio, "comp_spread_pct": spread,
            "decomp_spread_pct": spread}


def passing():
    """Sprint well clear of every gate, by 10% or more."""
    return [
        row("zstd-3", "1", 100, 1000), row("zstd-3", "4", 400, 1000),
        row("zstd-3", "all", 800, 1000),
        row("crush-sprint", "1", 120, 800), row("crush-sprint", "4", 300, 2400),
        row("crush-sprint", "all", 500, 3600),
        row("zstd-3", "1", 100, 1000, ratio=1.3, data="text"),
        row("crush-sprint", "1", 120, 900, ratio=1.4, data="text"),
        row("crush", "1", 100, 500), row("crush", "4", 300, 2000),
        row("crush", "all", 500, 2500),
    ]


def find(rows, criterion, cores=None):
    return next(r for r in rows if r["criterion"] == criterion
                and (cores is None or r["cores"] == cores))


class Multiples(unittest.TestCase):
    def test_multiple_divides_tool_by_reference(self):
        rows = compare.gate_report(passing())
        self.assertAlmostEqual(find(rows, "SC-002", "1")["multiple"], 1.2)
        self.assertAlmostEqual(find(rows, "SC-003", "4")["multiple"], 2.4)
        self.assertAlmostEqual(find(rows, "SC-003", "all")["multiple"], 3.6)

    def test_ratio_is_a_multiple_too(self):
        data = passing()
        for r in data:
            if r["tool"] == "crush-sprint" and r["data"] == "real":
                r["ratio"] = 3.663
        rows = compare.gate_report(data)
        self.assertAlmostEqual(find(rows, "SC-001", "1")["multiple"], 3.663 / 3.7)

    def test_sc005_is_each_core_set_against_one_core(self):
        rows = compare.gate_report(passing())
        self.assertAlmostEqual(find(rows, "SC-005", "4")["multiple"], 4.0)
        self.assertAlmostEqual(find(rows, "SC-005", "all")["multiple"], 5.0)

    def test_sc010_needs_speed_and_ratio_on_text(self):
        data = passing()
        self.assertEqual(find(compare.gate_report(data), "SC-010")["verdict"], "pass")
        for r in data:
            if r["tool"] == "crush-sprint" and r["data"] == "text":
                r["ratio"] = 1.2
        self.assertEqual(find(compare.gate_report(data), "SC-010")["verdict"], "fail")

    def test_ungated_rows_are_reported_with_no_verdict(self):
        rows = compare.gate_report(passing())
        reported = [r for r in rows if r["verdict"] == "reported"]
        self.assertEqual({(r["metric"], r["cores"]) for r in reported},
                         {("compress", "4"), ("compress", "all"), ("decompress", "1")})

    def test_a_missing_measurement_is_not_a_pass(self):
        data = [r for r in passing() if not (r["tool"] == "zstd-3" and r["cores"] == "4")]
        self.assertEqual(find(compare.gate_report(data), "SC-003", "4")["verdict"], "missing")


class UnderTenPercent(unittest.TestCase):
    def data(self, multiple):
        data = passing()
        for r in data:
            if r["tool"] == "crush-sprint" and r["cores"] == "4" and r["data"] == "real":
                r["decomp_mbs"] = 1000 * multiple
        return data

    def test_a_clear_win_passes_on_one_build(self):
        self.assertEqual(find(compare.gate_report(self.data(2.4)), "SC-003", "4")["verdict"],
                         "pass")

    def test_a_win_under_10_percent_on_one_build_is_provisional(self):
        self.assertEqual(find(compare.gate_report(self.data(2.1)), "SC-003", "4")["verdict"],
                         "provisional")

    def test_exactly_10_percent_is_clear(self):
        self.assertEqual(find(compare.gate_report(self.data(2.2)), "SC-003", "4")["verdict"],
                         "pass")

    def builds(self, multiples):
        data = [r for r in self.data(2.4) if r["tool"] != "crush-sprint"]
        for label, m in multiples.items():
            data += [dict(r, tool=f"crush-sprint@{label}", decomp_mbs=1000 * m)
                     if r["cores"] == "4" and r["data"] == "real"
                     else dict(r, tool=f"crush-sprint@{label}")
                     for r in self.data(2.4) if r["tool"] == "crush-sprint"]
        return data

    def test_a_narrow_win_on_every_build_passes(self):
        rows = compare.gate_report(self.builds({"a": 2.1, "b": 2.05, "c": 2.08}))
        row_4 = find(rows, "SC-003", "4")
        self.assertEqual(row_4["verdict"], "pass")
        self.assertAlmostEqual(row_4["multiple"], 2.05)

    def test_one_build_below_target_fails_the_gate(self):
        rows = compare.gate_report(self.builds({"a": 2.1, "b": 1.9, "c": 2.08}))
        self.assertEqual(find(rows, "SC-003", "4")["verdict"], "fail")

    def test_a_ratio_is_exact_and_needs_no_rotation(self):
        self.assertEqual(find(compare.gate_report(passing()), "SC-001", "1")["verdict"], "pass")


class Spread(unittest.TestCase):
    def test_one_stray_run_does_not_set_the_iqr(self):
        steady = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        self.assertEqual(compare.iqr_pct(steady), 0.0)
        self.assertEqual(compare.iqr_pct(steady[:-1] + [3.0]), 0.0)

    def test_iqr_grows_with_real_variation(self):
        self.assertGreater(compare.iqr_pct([1.0, 1.2, 1.4, 1.6, 1.8]), 20)

    def test_one_value_has_no_spread(self):
        self.assertEqual(compare.iqr_pct([2.0]), 0.0)


class Naming(unittest.TestCase):
    def test_a_failed_gate_names_its_metric_and_multiple(self):
        data = passing()
        for r in data:
            if r["tool"] == "crush-sprint" and r["cores"] == "all":
                r["decomp_mbs"] = 2510
        text = compare.format_gate_report(compare.gate_report(data))
        failed = [line for line in text.splitlines() if line.startswith("FAIL")]
        self.assertEqual(len(failed), 1)
        self.assertIn("SC-003", failed[0])
        self.assertIn("decompress", failed[0])
        self.assertIn("all", failed[0])
        self.assertIn("2.51x", failed[0])
        self.assertIn("3.00x", failed[0])

    def test_a_ratio_shows_enough_digits_to_explain_a_miss(self):
        data = passing()
        for r in data:
            if r["tool"] == "crush-sprint" and r["data"] == "real":
                r["ratio"] = 3.7 * 0.9899
        text = compare.format_gate_report(compare.gate_report(data))
        self.assertIn("FAIL SC-001 ratio 1 core on real: 0.9899x, target 0.9900x", text.splitlines())

    def test_the_summary_counts_outcomes(self):
        text = compare.format_gate_report(compare.gate_report(passing()))
        self.assertRegex(text, r"\b0 failed\b")


if __name__ == "__main__":
    unittest.main()
