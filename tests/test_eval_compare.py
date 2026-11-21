"""Fail-closed tests for run-to-run candidate comparison."""
import copy
import unittest

from scripts.eval_compare import compare_runs
from scripts.eval_report import RunError
from tests.test_eval_report import make_run


class EvalCompareTests(unittest.TestCase):
    def test_paired_block_difference_between_two_candidates(self):
        a_manifest, a_rows = make_run(candidate_credit="all", candidate_policy="best")
        b_manifest, b_rows = make_run(candidate_credit="half", candidate_policy="worse")
        result = compare_runs(a_manifest, a_rows, b_manifest, b_rows)
        self.assertTrue(result["comparability"]["comparable"])
        self.assertAlmostEqual(
            result["cross_run_deltas"]["first_credit"]["a_minus_b_block_mean"], 0.5
        )
        self.assertEqual(
            result["cross_run_deltas"]["first_credit"][
                "paired_block_bootstrap_95ci"
            ],
            [0.5, 0.5],
        )
        self.assertEqual(result["decision"]["layer1_signal"], "adopted_target_met")
        self.assertEqual(result["decision"]["final"], "undetermined")
        self.assertIn("A − B", result["markdown"])

    def test_comparison_is_deterministic(self):
        a_manifest, a_rows = make_run(candidate_credit="all", candidate_policy="best")
        b_manifest, b_rows = make_run(candidate_credit="half", candidate_policy="worse")
        first = compare_runs(a_manifest, a_rows, b_manifest, b_rows)
        second = compare_runs(a_manifest, a_rows, b_manifest, b_rows)
        self.assertEqual(first, second)

    def test_different_conditions_are_not_compared(self):
        a_manifest, a_rows = make_run(candidate_policy="best")
        b_manifest, b_rows = make_run(candidate_policy="worse")
        changed = copy.deepcopy(b_manifest)
        changed["condition_id"] = "f" * 64
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, changed, b_rows)

        changed = copy.deepcopy(b_manifest)
        changed["cards"] = "fixture_4p"
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, changed, b_rows)

    def test_different_seed_blocks_are_not_compared(self):
        a_manifest, a_rows = make_run(candidate_policy="best")
        b_manifest, b_rows = make_run(candidate_policy="worse")
        changed = copy.deepcopy(b_manifest)
        changed["seeds"]["seed_start"] += 1
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, changed, b_rows)

        changed = copy.deepcopy(b_manifest)
        changed["seeds"]["blocks"] = 2
        rows = [row for row in b_rows if row["block_index"] < 2]
        changed["blocks"] = 2
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, changed, rows)

    def test_common_opponent_runs_are_not_differenced(self):
        a_manifest, a_rows = make_run(candidate_policy="best", opponent="eval")
        b_manifest, b_rows = make_run(candidate_policy="worse", opponent="eval")
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, b_manifest, b_rows)

    def test_incomplete_runs_are_rejected_before_comparison(self):
        a_manifest, a_rows = make_run(candidate_policy="best")
        b_manifest, b_rows = make_run(candidate_policy="worse")
        with self.assertRaises(RunError):
            compare_runs(a_manifest, a_rows, b_manifest, b_rows[:-1])


if __name__ == "__main__":
    unittest.main()
