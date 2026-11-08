"""Fail-closed tests for paired, seat-rotated 95% interval aggregation."""
import copy
import unittest

from scripts.paired_eval_stats import POLICY_CHOICES, validate_and_report


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
                    # #167 A: the budget covers selection plus details.
                    "whole_decision_budget_enforced": False,
                    "detail_reserve_ms": 0,
                    "focus_max_detail_ms": 0.2,
                    "focus_p95_detail_ms": 0.1,
                    "focus_late_decision_count": 0,
                    "focus_max_decision_overshoot_ms": 0.0,
                    "focus_decision_fallback_count": 0,
                    "focus_selection_fallback_count": 0,
                    "focus_detail_fallback_count": 0,
                    "focus_detail_declined_count": 0,
                    "focus_budget_exhausted_before_details_count": 0,
                    "focus_observed_all_decisions_within_budget": False,
                    "focus_observed_all_decisions_within_budget_tolerance": False,
                    "focus_observed_all_decisions_within_10s": True,
                    "focus_late_beyond_tolerance_count": 0,
                    "focus_budget_tolerance_ms": 1.0,
                    "mcts_deadline_active": False,
                    "deterministic_policy_selection": True,
                    "search_mode": "simulation-capped",
                    "search_budget_enforced": False,
                    "mcts_simulations_cap": 64,
                    "time_budget_max_simulations": 100000,
                    "mcts_simulations_per_decision": 0.0,
                    "max_deadline_overshoot_ms": 0,
                    "max_search_elapsed_ms": 0,
                    "policy_select_only": True,
                    "total_wall_ms": 50.0,
                    "replay_hash": "a" * 64,
                    "replay_verified": True,
                })
    return records


def as_time_budgeted(rows):
    """Mark every row as a fixed-wall-clock experiment (#164, #167 A)."""
    for row in rows:
        row["search_mode"] = "time-budgeted"
        row["search_budget_enforced"] = True
        row["whole_decision_budget_enforced"] = True
        row["detail_reserve_ms"] = 100
        row["focus_observed_all_decisions_within_budget"] = True
        row["focus_observed_all_decisions_within_budget_tolerance"] = True
    return rows


def run(rows, blocks=3, players=4, cards="none", search_mode="simulation-capped"):
    return validate_and_report(
        rows, players=players, seed_start=1000, first_block=0, blocks=blocks,
        candidate="eval", baseline="random", opponent="greedy",
        extra=False, cards=cards, search_mode=search_mode, iterations=500,
    )


class PairedStatsReportDiagnosticsTests(unittest.TestCase):
    """Roadmap 3.1 (reference first-place values) and 6.6 (throughput)."""

    def test_single_winner_reports_solo_first_and_no_ties(self):
        report = run(fixture())
        self.assertEqual(report["candidate_solo_first_rate"], 1.0)
        self.assertEqual(report["candidate_first_reach_rate"], 1.0)
        self.assertEqual(report["candidate_tied_first_game_rate"], 0.0)
        self.assertEqual(report["baseline_solo_first_rate"], 0.0)
        self.assertEqual(report["baseline_first_reach_rate"], 0.0)

    def test_shared_first_is_reported_without_becoming_a_solo_win(self):
        rows = fixture()
        for row in rows:
            row["scores_by_seat"] = [30] * 4
            row["ranks_by_seat"] = [1, 1, 1, 1]
            row["first_place_credit"] = 0.25
            row["expected_rank_observation"] = 1
            row["score_margin"] = 0.0
        report = run(rows)
        self.assertEqual(report["mean_first_credit_delta"], 0.0)
        self.assertEqual(report["candidate_solo_first_rate"], 0.0)
        self.assertEqual(report["candidate_first_reach_rate"], 1.0)
        self.assertEqual(report["candidate_tied_first_game_rate"], 1.0)
        self.assertEqual(report["candidate_first_credit_pct"], 25.0)

    def test_per_seat_credit_exposes_a_seat_bias(self):
        rows = fixture()
        # Only seat 0 may win, so the seat spread must show the imbalance that
        # the aggregated 25% would hide.
        for row in rows:
            scores = [20] * 4
            ranks = [2] * 4
            scores[0], ranks[0] = 30, 1
            row["scores_by_seat"] = scores
            row["ranks_by_seat"] = ranks
            row["expected_rank_observation"] = ranks[row["candidate_seat"]]
            row["first_place_credit"] = 1.0 if ranks[row["candidate_seat"]] == 1 else 0.0
            row["score_margin"] = scores[row["candidate_seat"]] - (
                sum(scores) - scores[row["candidate_seat"]]
            ) / 3
        report = run(rows)
        self.assertEqual(
            report["candidate_first_credit_by_seat"], [1.0, 0.0, 0.0, 0.0])
        self.assertEqual(report["candidate_seat_credit_spread"], 1.0)
        # The bias is in the game, not in one arm: whichever arm sits in seat 0
        # wins, so both arms report the same seat profile.
        self.assertEqual(report["baseline_first_credit_by_seat"], [1.0, 0.0, 0.0, 0.0])
        self.assertEqual(report["baseline_seat_credit_spread"], 1.0)

    def test_seat_balanced_blocks_have_no_spread(self):
        report = run(fixture())
        self.assertEqual(report["candidate_first_credit_by_seat"], [1.0] * 4)
        self.assertEqual(report["candidate_seat_credit_spread"], 0.0)

    def test_throughput_uses_the_summed_game_wall_time(self):
        # 24 games x 50 ms = 1.2 s of game time, 70 decisions per game.
        report = run(fixture())
        self.assertAlmostEqual(report["total_cpu_seconds"], 1.2)
        self.assertAlmostEqual(report["games_per_second"], 20.0)
        self.assertAlmostEqual(report["decisions_per_second"], 1400.0)
        self.assertIn("throughput", report["throughput_note"])

    def test_zero_game_wall_time_is_rejected(self):
        rows = fixture()
        rows[0]["total_wall_ms"] = 0.0
        with self.assertRaisesRegex(ValueError, "non-positive game wall time"):
            run(rows)


class PairedStatsTests(unittest.TestCase):
    def test_cli_and_validator_accept_every_search_policy(self):
        # Guards against the CLI choices drifting from the validator (a
        # workflow_dispatch with candidate=puct previously failed here).
        for name in ("mcts", "puct"):
            self.assertIn(name, POLICY_CHOICES)
        for name in ("random", "greedy", "eval"):
            self.assertIn(name, POLICY_CHOICES)

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
        rows = as_time_budgeted(fixture())
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "mcts"
                row["mcts_simulations_completed"] = 240
                row["mcts_deadline_active"] = True
                row["deterministic_policy_selection"] = False
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="mcts", baseline="random", opponent="greedy",
            extra=False, search_mode="time-budgeted", iterations=500,
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

    def test_puct_focus_is_accepted_with_budget_metrics(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "puct"
                row["mcts_simulations_completed"] = 256
                row["search_simulations_completed"] = 256
                row["mcts_deadline_active"] = True
                row["deterministic_policy_selection"] = False
                row["puct_leaf"] = "heuristic"
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="puct", baseline="random", opponent="greedy",
            extra=False, search_mode="time-budgeted", iterations=500,
        )
        self.assertEqual(report["total_games"], 24)
        self.assertTrue(report["candidate_focus_deadline_control"])
        self.assertEqual(report["candidate_mcts_simulations_completed"], 12 * 256)

    def test_mcts_over_budget_disables_full_decision_flag(self):
        rows = as_time_budgeted(fixture())
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
            extra=False, search_mode="time-budgeted", iterations=500,
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



class PairedStatsSearchModeTests(unittest.TestCase):
    """#164: fixed simulation counts and fixed wall-clock budgets never mix."""

    def test_simulation_capped_report_is_labelled_and_cannot_claim_10s(self):
        report = run(fixture())
        self.assertEqual(report["search_mode"], "simulation-capped")
        self.assertFalse(report["search_budget_enforced"])
        # No deadline is enforced in this mode, so a 10 s claim is meaningless
        # even though decision_budget_ms is 10000.
        self.assertFalse(report["candidate_focus_deadline_control"])
        self.assertFalse(report["candidate_10s_observed_pass"])
        self.assertIn("simulation-capped", report["search_mode_note"])

    def test_time_budgeted_report_carries_search_cost_measurements(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            row["mcts_simulations_per_decision"] = 500.0
            row["max_deadline_overshoot_ms"] = 7
            row["max_search_elapsed_ms"] = 9993
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="eval", baseline="random", opponent="greedy",
            extra=False, search_mode="time-budgeted", iterations=500,
        )
        self.assertEqual(report["search_mode"], "time-budgeted")
        self.assertTrue(report["search_budget_enforced"])
        self.assertEqual(report["candidate_simulations_per_decision"], 500.0)
        self.assertEqual(report["candidate_max_deadline_overshoot_ms"], 7)
        self.assertEqual(report["baseline_max_deadline_overshoot_ms"], 7)

    def test_mixed_search_modes_are_rejected(self):
        rows = as_time_budgeted(fixture())
        rows[0]["search_mode"] = "simulation-capped"
        rows[0]["search_budget_enforced"] = False
        rows[0]["mcts_deadline_active"] = False
        rows[0]["deterministic_policy_selection"] = True
        with self.assertRaisesRegex(ValueError, "inconsistent search modes"):
            run(rows, search_mode="time-budgeted")

    def test_unknown_search_mode_is_rejected(self):
        rows = fixture()
        rows[0]["search_mode"] = "budgeted-ish"
        with self.assertRaisesRegex(ValueError, "invalid search mode"):
            run(rows)

    def test_time_budgeted_requires_a_positive_budget(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            row["decision_budget_ms"] = 0
        with self.assertRaisesRegex(ValueError, "time-budgeted run without a budget"):
            run(rows, search_mode="time-budgeted")

    def test_deadline_flag_must_follow_the_search_mode(self):
        def mcts_candidate(rows):
            for row in rows:
                if row["arm_id"] == "candidate":
                    row["focus_policy"] = "mcts"
                    row["mcts_deadline_active"] = True
                    row["deterministic_policy_selection"] = False
            return rows

        def report(rows):
            return validate_and_report(
                rows, players=4, seed_start=1000, first_block=0, blocks=3,
                candidate="mcts", baseline="random", opponent="greedy",
                extra=False, search_mode="time-budgeted", iterations=500,
            )

        # Valid: MCTS focus in time-budgeted mode.
        valid = report(mcts_candidate(as_time_budgeted(fixture())))
        self.assertEqual(valid["candidate_focus_deadline_control"], True)

        # Now claim an enforced deadline in simulation-capped mode: not allowed.
        capped = as_time_budgeted(fixture())
        for row in capped:
            row["search_mode"] = "simulation-capped"
            row["search_budget_enforced"] = False
        with self.assertRaisesRegex(ValueError, "deadline flag"):
            report(mcts_candidate(capped))

    def test_negative_search_measurements_are_rejected(self):
        for field in ("max_deadline_overshoot_ms", "max_search_elapsed_ms",
                      "mcts_simulations_per_decision"):
            with self.subTest(field=field):
                rows = fixture()
                rows[0][field] = -1
                with self.assertRaisesRegex(ValueError, "invalid"):
                    run(rows)


class PairedStatsTenSecondFlagTests(unittest.TestCase):
    """#167 A review P1: the 10 s flag and its explanation must agree.

    The whole-decision budget bounds the focus seat whatever its policy, while
    the search deadline only exists for a search policy. The old flag required a
    search policy and its explanation mentioned only MCTS, so a budgeted
    heuristic seat could never pass and the text did not describe the check.
    """

    def test_a_budgeted_heuristic_focus_is_observable(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "eval"
                row["mcts_deadline_active"] = False
                row["deterministic_policy_selection"] = True
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="eval", baseline="random", opponent="greedy",
            extra=False, search_mode="time-budgeted", iterations=500,
        )
        self.assertTrue(report["candidate_whole_decision_budget_enforced"])
        self.assertFalse(report["candidate_focus_search_deadline_control"])
        self.assertFalse(report["candidate_focus_deadline_control"])
        self.assertTrue(report["candidate_10s_observed_pass"])
        self.assertIn("whole-decision budget", report["candidate_10s_observed_pass_basis"])

    def test_a_search_focus_reports_both_questions(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "puct"
                row["mcts_deadline_active"] = True
                row["deterministic_policy_selection"] = False
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="puct", baseline="random", opponent="greedy",
            extra=False, search_mode="time-budgeted", iterations=500,
        )
        self.assertTrue(report["candidate_focus_search_deadline_control"])
        self.assertTrue(report["candidate_whole_decision_budget_enforced"])
        self.assertTrue(report["candidate_10s_observed_pass"])

    def test_simulation_capped_never_claims_a_ten_second_observation(self):
        report = run(fixture())
        self.assertFalse(report["candidate_whole_decision_budget_enforced"])
        self.assertFalse(report["candidate_10s_observed_pass"])
        self.assertIn("not an OS-enforced hard guarantee", report["caveat"])
        self.assertNotIn("only when the arm is MCTS", report["caveat"])


class PairedStatsWholeDecisionBudgetTests(unittest.TestCase):
    """#167 A: the budget must be reported as covering the whole decision."""

    def test_simulation_capped_rows_cannot_claim_a_whole_decision_budget(self):
        rows = fixture()
        for row in rows:
            row["whole_decision_budget_enforced"] = True
        with self.assertRaisesRegex(ValueError, "whole-decision budget flag"):
            run(rows)

    def test_time_budgeted_rows_need_a_reserve_inside_the_budget(self):
        rows = as_time_budgeted(fixture())
        rows[0]["detail_reserve_ms"] = 0
        with self.assertRaisesRegex(ValueError, "detail reserve inside the budget"):
            run(rows, search_mode="time-budgeted")

    def test_a_reserve_larger_than_the_budget_is_rejected(self):
        rows = as_time_budgeted(fixture())
        for row in rows:
            row["detail_reserve_ms"] = row["decision_budget_ms"] + 1
        with self.assertRaisesRegex(ValueError, "detail reserve inside the budget"):
            run(rows, search_mode="time-budgeted")

    def test_detail_time_cannot_exceed_the_whole_decision_time(self):
        rows = as_time_budgeted(fixture())
        rows[0]["focus_max_detail_ms"] = rows[0]["focus_max_decision_ms"] + 1.0
        with self.assertRaisesRegex(ValueError, "detail time cannot exceed"):
            run(rows, search_mode="time-budgeted")

    def test_fallback_subcounts_must_add_up(self):
        rows = fixture()
        rows[0]["focus_decision_fallback_count"] = 2
        rows[0]["focus_detail_fallback_count"] = 1
        with self.assertRaisesRegex(ValueError, "fallback counts do not add up"):
            run(rows)

    def test_overshoot_without_a_late_decision_is_rejected(self):
        rows = fixture()
        rows[0]["focus_max_decision_overshoot_ms"] = 3.0
        with self.assertRaisesRegex(ValueError, "overshoot without a late decision"):
            run(rows)

    def test_a_row_claiming_compliance_it_did_not_measure_is_not_trusted(self):
        # The row flag says "within budget" while its own counters say two
        # decisions were late. The report must follow the counters.
        rows = as_time_budgeted(fixture())
        rows[0]["focus_late_decision_count"] = 2
        rows[0]["focus_max_decision_overshoot_ms"] = 4.0
        report = run(rows, search_mode="time-budgeted")
        self.assertFalse(report["candidate_observed_all_decisions_within_budget"])
        self.assertEqual(report["candidate_late_decision_count"], 2)
        self.assertEqual(report["whole_decision_budget_enforced"], True)
        self.assertGreater(report["detail_reserve_ms"], 0)

    def test_simulation_capped_report_states_no_whole_decision_budget(self):
        report = run(fixture())
        self.assertFalse(report["whole_decision_budget_enforced"])
        self.assertEqual(report["detail_reserve_ms"], 0)
        self.assertFalse(report["candidate_observed_all_decisions_within_budget"])
        self.assertTrue(report["candidate_observed_all_decisions_within_10s"])


class ComparisonDesignTests(unittest.TestCase):
    """#106 C-2/C-4: a report must say what was compared, and whether the
    condition can resolve a +5 pp difference at all."""

    def test_common_opponent_is_not_head_to_head(self):
        report = run(fixture())
        design = report["comparison_design"]
        self.assertFalse(design["head_to_head"])
        self.assertFalse(design["mirror_baseline"])
        self.assertEqual(design["candidate_opponent_policies"], ["greedy"])
        self.assertEqual(design["baseline_opponent_policies"], ["greedy"])

    def test_head_to_head_is_detected_when_the_opponent_is_the_baseline(self):
        rows = fixture()
        for row in rows:
            row["opponent_policy"] = "random"
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="eval", baseline="random", opponent="random",
            extra=False, cards="none", search_mode="simulation-capped",
            iterations=500,
        )
        design = report["comparison_design"]
        self.assertTrue(design["head_to_head"])
        self.assertTrue(design["mirror_baseline"])
        self.assertEqual(design["baseline_opponent_policies"], ["random"])

    def test_candidate_equal_to_the_opponent_is_a_mirror_not_head_to_head(self):
        rows = fixture()
        for row in rows:
            row["opponent_policy"] = "eval"
        report = validate_and_report(
            rows, players=4, seed_start=1000, first_block=0, blocks=3,
            candidate="eval", baseline="random", opponent="eval",
            extra=False, cards="none", search_mode="simulation-capped",
            iterations=500,
        )
        self.assertFalse(report["comparison_design"]["head_to_head"])

    def test_a_ceiling_condition_is_flagged_as_undiscriminating(self):
        report = run(fixture())
        saturation = report["saturation"]
        self.assertTrue(saturation["ceiling_limited"])
        self.assertEqual(saturation["max_first_credit_pct"], 100.0)

    def test_a_balanced_condition_is_not_flagged(self):
        players = 4
        rows = fixture(blocks=4)
        for row in rows:
            if row["block_index"] % 2 == 1:
                # The focus seat loses this block, so both arms sit at 50%.
                seat = row["candidate_seat"]
                scores = [20] * players
                ranks = [2] * players
                winner = (seat + 1) % players
                scores[winner], ranks[winner] = 30, 1
                row["scores_by_seat"] = scores
                row["ranks_by_seat"] = ranks
                row["first_place_credit"] = 1.0 if ranks[seat] == 1 else 0.0
                row["expected_rank_observation"] = ranks[seat]
                row["score_margin"] = (
                    scores[seat] - (sum(scores) - scores[seat]) / (players - 1))
        report = run(rows, blocks=4)
        saturation = report["saturation"]
        self.assertFalse(saturation["ceiling_limited"])
        self.assertAlmostEqual(saturation["max_first_credit_pct"], 50.0)


if __name__ == "__main__":
    unittest.main()
if __name__ == "__main__":
    unittest.main()
