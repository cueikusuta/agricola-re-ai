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
                    "card_set_count": 0,
                    "card_set_hash": "0" * 64,
                    "finished": True,
                    "rounds_completed": 14,
                    "scores_by_seat": scores,
                    "ranks_by_seat": ranks,
                    "first_place_credit": 1.0 if ranks[seat] == 1 else 0.0,
                    "expected_rank_observation": ranks[seat],
                    "score_margin": scores[seat] - (sum(scores) - scores[seat]) / (players - 1),
                    "decisions": 70,
                    "max_policy_select_ms": 1.0,
                    "decision_budget_ms": 10000,
                    "p50_policy_select_ms": 0.5,
                    "p95_policy_select_ms": 0.9,
                    "p99_policy_select_ms": 0.99,
                    "timeout_count": 0,
                    "fallback_count": 0,
                    "mcts_simulations_completed": 0,
                    "focus_decisions": 30,
                    "focus_max_decision_ms": 0.8,
                    "focus_p95_decision_ms": 0.6,
                    "focus_p99_decision_ms": 0.7,
                    "focus_timeout_count": 0,
                    "mcts_deadline_active": False,
                    "deterministic_policy_selection": True,
                    "policy_select_only": True,
                    "total_wall_ms": 50.0,
                    "replay_hash": "a" * 64,
                    "replay_verified": True,
                })
    return records


def run(rows, blocks=3, players=4, cards="none"):
    return validate_and_report(
        rows, players=players, seed_start=1000, first_block=0, blocks=blocks,
        candidate="eval", baseline="random", opponent="greedy",
        extra=False, cards=cards, iterations=500,
    )


class PairedStatsTests(unittest.TestCase):
    def test_complete_seat_blocks_and_effect_size(self):
        report = run(fixture())
        self.assertEqual(report["games_per_arm"], 12)
        self.assertEqual(report["total_games"], 24)
        self.assertEqual(report["mean_first_credit_delta"], 1.0)
        self.assertEqual(report["paired_block_bootstrap_95ci"], [1.0, 1.0])
        self.assertFalse(report["candidate_10s_observed_pass"])
        self.assertFalse(report["candidate_focus_deadline_control"])

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

    def test_mcts_focus_is_accepted_with_budget_metrics(self):
        rows = fixture()
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "mcts"
                row["mcts_simulations_completed"] = 240
                row["mcts_deadline_active"] = True
                row["deterministic_policy_selection"] = False
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="mcts", baseline="random", opponent="greedy",
            extra=False, iterations=500,
        )
        self.assertEqual(report["total_games"], 24)
        self.assertEqual(report["decision_budget_ms"], 10000)
        self.assertEqual(report["candidate_mcts_simulations_completed"], 12 * 240)
        self.assertEqual(report["candidate_timeout_rate"], 0.0)
        self.assertEqual(report["focus_decision_timing_includes_detail"], True)
        self.assertEqual(report["mean_first_credit_delta_pp"], report["mean_first_credit_delta"] * 100)
        self.assertEqual(report["candidate_first_credit_pct"], report["candidate_first_credit"] * 100)
        self.assertTrue(report["candidate_focus_deadline_control"])
        self.assertTrue(report["candidate_focus_measured_within_budget"])
        self.assertTrue(report["candidate_10s_observed_pass"])
        self.assertEqual(report["deadline_enforcement"], "cooperative_in_policy_checks")

    def test_mcts_over_budget_disables_full_decision_flag(self):
        rows = fixture()
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "mcts"
                row["mcts_deadline_active"] = True
                row["deterministic_policy_selection"] = False
                row["focus_max_decision_ms"] = 12000.0
                row["focus_p99_decision_ms"] = 11999.0
                row["focus_p95_decision_ms"] = 11000.0
                row["focus_timeout_count"] = 5
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="mcts", baseline="random", opponent="greedy",
            extra=False, iterations=500,
        )
        self.assertFalse(report["candidate_10s_observed_pass"])
        self.assertTrue(report["candidate_focus_deadline_control"])
        self.assertFalse(report["candidate_focus_measured_within_budget"])
        self.assertEqual(report["candidate_focus_timeout_count"], 12 * 5)
        self.assertEqual(report["candidate_max_focus_decision_ms"], 12000.0)

    def test_invalid_selection_timing_and_counters_are_rejected(self):
        cases = (
            ("p95_policy_select_ms", 2.0, "timing"),
            ("p50_policy_select_ms", 0.95, "out of order"),
            ("timeout_count", 999, "timeout_count"),
            ("fallback_count", -1, "fallback_count"),
            ("mcts_simulations_completed", -1, "mcts simulations"),
            ("decision_budget_ms", 70000, "budget"),
            ("focus_decisions", 999, "focus decisions"),
            ("focus_p99_decision_ms", 2.0, "out of order"),
            ("focus_timeout_count", 999, "focus_timeout_count"),
            ("mcts_deadline_active", True, "policy mode flags"),
            ("deterministic_policy_selection", False, "policy mode flags"),
        )
        for field, value, expected_error in cases:
            with self.subTest(field=field):
                rows = fixture()
                rows[0][field] = value
                with self.assertRaisesRegex(ValueError, expected_error):
                    run(rows)

    def test_deadline_flag_must_match_policy_and_budget(self):
        rows = fixture()
        row = rows[0]
        row["focus_policy"] = "eval"
        row["decision_budget_ms"] = 10000
        row["mcts_deadline_active"] = True
        row["deterministic_policy_selection"] = False
        with self.assertRaisesRegex(ValueError, "deadline flag"):
            run(rows)

    def test_mixed_decision_budgets_are_rejected(self):
        rows = fixture()
        rows[1]["decision_budget_ms"] = 5000
        with self.assertRaisesRegex(ValueError, "inconsistent decision budgets"):
            run(rows)

    def test_rates_use_focus_decision_denominator(self):
        for players in (2, 4):
            with self.subTest(players=players):
                rows = fixture(blocks=2, players=players)
                for row in rows:
                    row["decisions"] = 100
                    row["focus_decisions"] = 25
                    row["timeout_count"] = 5
                    row["fallback_count"] = 2
                    row["focus_timeout_count"] = 1
                report = validate_and_report(
                    rows, players=players, seed_start=1000, first_block=0, blocks=2,
                    candidate="eval", baseline="random", opponent="greedy",
                    extra=False, iterations=200,
                )
                games_per_arm = 2 * players
                focus_total = games_per_arm * 25
                self.assertEqual(report["candidate_focus_decisions"], focus_total)
                self.assertEqual(report["all_player_decisions"], 2 * games_per_arm * 100)
                self.assertEqual(
                    report["candidate_timeout_rate"], (games_per_arm * 5) / focus_total
                )
                self.assertEqual(
                    report["candidate_measured_timeout_rate"],
                    (games_per_arm * 1) / focus_total,
                )
                self.assertNotEqual(
                    report["candidate_timeout_rate"],
                    (games_per_arm * 5) / (games_per_arm * 100),
                )

    def test_timeout_and_fallback_accounting_is_consistent(self):
        rows = fixture()
        for row in rows:
            row["focus_decisions"] = 30
            row["focus_timeout_count"] = 3
            row["timeout_count"] = 4
            row["fallback_count"] = 1
        report = run(rows)
        games_per_arm = 12
        focus_total = games_per_arm * 30
        self.assertEqual(report["candidate_focus_timeout_count"], games_per_arm * 3)
        self.assertLessEqual(
            report["candidate_focus_timeout_count"], report["candidate_focus_decisions"]
        )
        self.assertEqual(
            report["candidate_measured_timeout_rate"], (games_per_arm * 3) / focus_total
        )
        self.assertEqual(report["candidate_timeout_rate"], (games_per_arm * 4) / focus_total)
        self.assertEqual(report["candidate_fallback_rate"], (games_per_arm * 1) / focus_total)

    def test_zero_budget_is_deterministic_and_disables_10s_flags(self):
        rows = fixture()
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "mcts"
            row["decision_budget_ms"] = 0
            row["focus_timeout_count"] = 0
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="mcts", baseline="random", opponent="greedy",
            extra=False, iterations=200,
        )
        self.assertFalse(report["candidate_focus_deadline_control"])
        self.assertFalse(report["candidate_focus_measured_within_budget"])
        self.assertFalse(report["candidate_10s_observed_pass"])

    def test_equal_score_tiebreak_ranks_are_allowed(self):
        rows = fixture()
        # Two equal scores may have different ranks due to the resource
        # tiebreaker, which is intentionally not exposed in this JSONL.
        rows[0]["scores_by_seat"][1] = rows[0]["scores_by_seat"][0]
        rows[0]["ranks_by_seat"][2:] = [3, 3]
        rows[0]["score_margin"] = rows[0]["scores_by_seat"][0] - (
            sum(rows[0]["scores_by_seat"]) - rows[0]["scores_by_seat"][0]
        ) / 3
        report = run(rows)
        self.assertEqual(report["total_games"], 24)



    def test_impossible_competition_rank_gaps_are_rejected(self):
        # No combination of resource tiebreaks can produce these gaps.
        cases = (
            ([30, 20, 20, 20], [1, 4, 4, 4]),
            ([30, 20, 20, 20], [1, 2, 4, 4]),
            ([30, 30, 20, 20], [1, 1, 4, 4]),
            ([30, 30, 30, 30], [1, 1, 4, 4]),
        )
        for scores, ranks in cases:
            with self.subTest(scores=scores, ranks=ranks):
                rows = fixture()
                row = rows[0]
                row["scores_by_seat"] = scores
                row["ranks_by_seat"] = ranks
                row["expected_rank_observation"] = ranks[0]
                row["first_place_credit"] = 1 / ranks.count(1)
                row["score_margin"] = scores[0] - (sum(scores) - scores[0]) / 3
                with self.assertRaisesRegex(ValueError, "impossible tiebreak rank"):
                    run(rows)

    def test_valid_resource_tiebreak_competition_ranks_remain_allowed(self):
        # Skipping positions after *actual* ties is correct competition ranking.
        cases = (
            ([30, 20, 20, 20], [1, 2, 3, 3]),
            ([30, 20, 20, 20], [1, 2, 2, 4]),
            ([30, 30, 20, 20], [1, 1, 3, 3]),
            ([30, 30, 30, 30], [1, 1, 3, 3]),
        )
        for scores, ranks in cases:
            with self.subTest(scores=scores, ranks=ranks):
                rows = fixture()
                row = rows[0]
                row["scores_by_seat"] = scores
                row["ranks_by_seat"] = ranks
                row["expected_rank_observation"] = ranks[0]
                row["first_place_credit"] = 1 / ranks.count(1)
                row["score_margin"] = scores[0] - (sum(scores) - scores[0]) / 3
                self.assertEqual(run(rows)["total_games"], 24)


class PairedStatsCardConditionTests(unittest.TestCase):
    """The card condition is validated, reported, and never silently pooled."""

    def verified_rows(self, **overrides):
        rows = fixture(blocks=2, players=2)
        for row in rows:
            row["cards"] = "verified_basic_2p"
            row["card_set_count"] = 30
            row["card_set_hash"] = "b" * 64
        for key, value in overrides.items():
            for row in rows:
                row[key] = value
        return rows

    def test_cardless_report_states_the_condition_explicitly(self):
        report = run(fixture(), cards="none")
        self.assertEqual(report["cards"], "none")
        self.assertEqual(report["card_set_count"], 0)
        self.assertIn("cardless", report["mode"])

    def test_verified_card_report_carries_the_deck_identity(self):
        report = validate_and_report(
            self.verified_rows(), players=2, seed_start=1000, first_block=0,
            blocks=2, candidate="eval", baseline="random", opponent="greedy",
            extra=False, cards="verified_basic_2p", iterations=500,
        )
        self.assertEqual(report["cards"], "verified_basic_2p")
        self.assertEqual(report["card_set_count"], 30)
        self.assertEqual(report["card_set_hash"], "b" * 64)
        self.assertIn("verified_card", report["mode"])

    def test_card_condition_must_match_the_argument(self):
        # Cardless rows passed as a verified experiment, and vice versa.
        with self.assertRaisesRegex(ValueError, "invalid metadata|card condition|card set"):
            validate_and_report(
                fixture(blocks=2, players=2), players=2, seed_start=1000,
                first_block=0, blocks=2, candidate="eval", baseline="random",
                opponent="greedy", extra=False, cards="verified_basic_2p",
                iterations=500,
            )
        with self.assertRaisesRegex(ValueError, "invalid metadata|card condition|card set"):
            validate_and_report(
                self.verified_rows(), players=2, seed_start=1000, first_block=0,
                blocks=2, candidate="eval", baseline="random", opponent="greedy",
                extra=False, cards="none", iterations=500,
            )

    def test_verified_deck_is_two_player_only(self):
        with self.assertRaisesRegex(ValueError, "two players only"):
            validate_and_report(
                fixture(blocks=2, players=4), players=4, seed_start=1000,
                first_block=0, blocks=2, candidate="eval", baseline="random",
                opponent="greedy", extra=False, cards="verified_basic_2p",
                iterations=500,
            )

    def test_unknown_card_condition_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unsupported card condition"):
            validate_and_report(
                fixture(blocks=2, players=2), players=2, seed_start=1000,
                first_block=0, blocks=2, candidate="eval", baseline="random",
                opponent="greedy", extra=False, cards="all_840",
                iterations=500,
            )

    def test_a_different_deck_inside_one_experiment_is_rejected(self):
        rows = self.verified_rows()
        rows[-1]["card_set_hash"] = "c" * 64
        with self.assertRaisesRegex(ValueError, "inconsistent card set"):
            validate_and_report(
                rows, players=2, seed_start=1000, first_block=0, blocks=2,
                candidate="eval", baseline="random", opponent="greedy",
                extra=False, cards="verified_basic_2p", iterations=500,
            )

    def test_malformed_deck_hash_is_rejected(self):
        for bad in ("", "z" * 64, "b" * 63, None, 30):
            with self.subTest(bad=bad):
                with self.assertRaisesRegex(ValueError, "invalid card set metadata"):
                    validate_and_report(
                        self.verified_rows(card_set_hash=bad), players=2,
                        seed_start=1000, first_block=0, blocks=2,
                        candidate="eval", baseline="random", opponent="greedy",
                        extra=False, cards="verified_basic_2p", iterations=500,
                    )


if __name__ == "__main__":
    unittest.main()
