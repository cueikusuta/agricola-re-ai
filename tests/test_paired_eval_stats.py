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
                    "replay_hash": "abcdef0123456789abcdef",
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


if __name__ == "__main__":
    unittest.main()
