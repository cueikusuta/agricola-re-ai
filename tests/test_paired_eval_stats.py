"""Fail-closed tests for paired, seat-rotated 95% interval aggregation."""
import copy
import unittest

from scripts.paired_eval_stats import validate_and_report


def fixture(blocks=3, players=4):
    records = []
    for block in range(blocks):
        for arm in ("candidate", "baseline"):
            for seat in range(players):
                # Candidate wins, baseline loses. Change seat, not underlying result.
                scores = [20] * players
                ranks = [2] * players
                if arm == "candidate":
                    scores[seat], ranks[seat] = 30, 1
                else:
                    opponent = (seat + 1) % players
                    scores[opponent], ranks[opponent] = 30, 1
                records.append({
                    "schema_version": 1,
                    "block_index": block,
                    "seed": 1000 + block,
                    "arm_id": arm,
                    "focus_policy": "eval" if arm == "candidate" else "random",
                    "opponent_policy": "greedy",
                    "candidate_seat": seat,
                    "players": players,
                    "additional_action_space": False,
                    "cards": "none",
                    "finished": True,
                    "rounds_completed": 14,
                    "scores_by_seat": scores,
                    "ranks_by_seat": ranks,
                    "first_place_credit": 1.0 if ranks[seat] == 1 else 0.0,
                    "expected_rank_observation": ranks[seat],
                    "score_margin": scores[seat] - (sum(scores) - scores[seat]) / (players - 1),
                    "decisions": 70,
                    "max_policy_select_ms": 1.0,
                    "policy_select_only": True,
                    "total_wall_ms": 50.0,
                    "replay_hash": "a" * 64,
                    "replay_verified": True,
                })
    return records


def run(rows, blocks=3, players=4):
    return validate_and_report(
        rows, players=players, seed_start=1000, first_block=0, blocks=blocks,
        candidate="eval", baseline="random", opponent="greedy",
        extra=False, iterations=500,
    )


class PairedStatsTests(unittest.TestCase):
    def test_complete_seat_blocks_and_effect_size(self):
        report = run(fixture())
        self.assertEqual(report["games_per_arm"], 12)
        self.assertEqual(report["total_games"], 24)
        self.assertEqual(report["mean_first_credit_delta"], 1.0)
        self.assertEqual(report["paired_block_bootstrap_95ci"], [1.0, 1.0])
        self.assertFalse(report["full_decision_10s_enforced"])

    def test_missing_and_duplicate_records(self):
        rows = fixture()
        with self.assertRaisesRegex(ValueError, "missing"):
            run(rows[:-1])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            run(rows + [copy.deepcopy(rows[0])])

    def test_seed_mismatch_and_unverified_replay(self):
        rows = fixture()
        rows[0]["seed"] = 555
        with self.assertRaisesRegex(ValueError, "metadata"):
            run(rows)
        rows = fixture()
        rows[0]["replay_verified"] = False
        with self.assertRaisesRegex(ValueError, "metadata"):
            run(rows)

    def test_credits_and_margins_recomputed(self):
        rows = fixture()
        rows[0]["first_place_credit"] = .5
        with self.assertRaisesRegex(ValueError, "credit"):
            run(rows)
        rows = fixture()
        rows[0]["score_margin"] += 1
        with self.assertRaisesRegex(ValueError, "margin"):
            run(rows)

    def test_nonfinite_and_nonnumeric_effect_sizes_are_rejected(self):
        for key, error in (("first_place_credit", "credit"), ("score_margin", "margin")):
            for bad in (float("nan"), float("inf"), -float("inf"), "0", True, None):
                with self.subTest(key=key, bad=repr(bad)):
                    rows = fixture()
                    rows[0][key] = bad
                    with self.assertRaisesRegex(ValueError, error):
                        run(rows)

    def test_non_sha256_hash_and_untrusted_policy_timing_are_rejected(self):
        for bad_hash in ("abc", "g" * 64, "A" * 64, ""):
            with self.subTest(hash=bad_hash):
                rows = fixture()
                rows[0]["replay_hash"] = bad_hash
                with self.assertRaisesRegex(ValueError, "metadata"):
                    run(rows)
        rows = fixture()
        rows[0]["policy_select_only"] = False
        with self.assertRaisesRegex(ValueError, "metadata"):
            run(rows)
        rows = fixture()
        rows[0]["max_policy_select_ms"] = 100.0
        with self.assertRaisesRegex(ValueError, "selection time"):
            run(rows)

    def test_score_order_must_be_consistent_with_rank(self):
        rows = fixture()
        rows[0]["ranks_by_seat"][0] = 2
        rows[0]["ranks_by_seat"][1] = 1
        with self.assertRaisesRegex(ValueError, "score/rank ordering"):
            run(rows)

    def test_boolean_or_nonnumeric_identifiers_are_not_valid(self):
        for key, value in (("block_index", True), ("candidate_seat", False), ("seed", "1000")):
            with self.subTest(key=key):
                rows = fixture()
                rows[0][key] = value
                with self.assertRaisesRegex(ValueError, "integers"):
                    run(rows)

    def test_nonnumeric_or_nonfinite_timing_is_rejected(self):
        for value in (float("nan"), float("inf"), "5.0", None, False):
            with self.subTest(value=repr(value)):
                rows = fixture()
                rows[0]["max_policy_select_ms"] = value
                with self.assertRaisesRegex(ValueError, "timing"):
                    run(rows)


    def test_oversized_integer_metrics_are_rejected_without_overflow(self):
        for field, error in (("first_place_credit", "credit"), ("score_margin", "margin"),
                             ("total_wall_ms", "timing")):
            with self.subTest(field=field):
                rows = fixture()
                rows[0][field] = 10**1000
                with self.assertRaisesRegex(ValueError, error):
                    run(rows)

    def test_strict_integer_rank_schema_and_score_range(self):
        cases = (
            ("expected_rank_observation", True, "rank"),
            ("schema_version", True, "metadata"),
        )
        for field, value, expected_error in cases:
            with self.subTest(field=field):
                rows = fixture()
                rows[0][field] = value
                with self.assertRaisesRegex(ValueError, expected_error):
                    run(rows)
        rows = fixture()
        rows[0]["scores_by_seat"][0] = 10**1000
        with self.assertRaisesRegex(ValueError, "standings"):
            run(rows)

    def test_equal_score_tiebreak_ranks_are_allowed(self):
        rows = fixture()
        # Two equal scores may have different ranks due to the resource
        # tiebreaker, which is intentionally not exposed in this JSONL.
        rows[0]["scores_by_seat"][1] = rows[0]["scores_by_seat"][0]
        rows[0]["score_margin"] = rows[0]["scores_by_seat"][0] - (
            sum(rows[0]["scores_by_seat"]) - rows[0]["scores_by_seat"][0]
        ) / 3
        report = run(rows)
        self.assertEqual(report["total_games"], 24)



if __name__ == "__main__":
    unittest.main()
