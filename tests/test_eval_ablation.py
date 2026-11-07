"""Fail-closed tests for one-factor-at-a-time ablation comparisons."""
import copy
import unittest

from scripts.eval_ablation import compare_arms


def arm_rows(blocks=4, players=2, *, credit, seed_base=1000, replay="a"):
    """Rows for one arm.

    `credit(block, seat)` decides whether the candidate focus seat wins.
    The shared baseline always wins in seat 0 only, so the baseline games are
    identical across arms unless a test deliberately changes them.
    """
    rows = []
    for block in range(blocks):
        for arm in ("candidate", "baseline"):
            for seat in range(players):
                scores = [20] * players
                ranks = [2] * players
                value = credit(block, seat) if arm == "candidate" else (1.0 if seat == 0 else 0.0)
                if value == 1.0:
                    scores[seat], ranks[seat] = 30, 1
                else:
                    # Somebody has to hold first place; the other seat takes it.
                    other = (seat + 1) % players
                    scores[other], ranks[other] = 30, 1
                rows.append({
                    "schema_version": 1,
                    "block_index": block,
                    "seed": seed_base + block,
                    "arm_id": arm,
                    "focus_policy": "eval" if arm == "candidate" else "random",
                    "opponent_policy": "greedy",
                    "candidate_seat": seat,
                    "players": players,
                    "additional_action_space": False,
                    "cards": "none",
                    "card_set_count": 0,
                    "card_set_hash": "0" * 64,
                    "mcts_tree_depth": 2,
                    "finished": True,
                    "rounds_completed": 14,
                    "scores_by_seat": scores,
                    "ranks_by_seat": ranks,
                    "first_place_credit": 1.0 if ranks[seat] == 1 else 0.0,
                    "expected_rank_observation": ranks[seat],
                    "score_margin": scores[seat] - (sum(scores) - scores[seat]) / (players - 1),
                    "decisions": 70,
                    "max_policy_select_ms": 1.0,
                    "decision_budget_ms": 0,
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
                    "search_mode": "simulation-capped",
                    "search_budget_enforced": False,
                    "mcts_simulations_cap": 64,
                    "time_budget_max_simulations": 100000,
                    "mcts_simulations_per_decision": 0.0,
                    "max_deadline_overshoot_ms": 0,
                    "max_search_elapsed_ms": 0,
                    "policy_select_only": True,
                    "total_wall_ms": 50.0,
                    "replay_hash": (replay + f"{block:02d}{seat}").ljust(64, "0"),
                    "replay_verified": True,
                })
    return rows


def options(*, candidate="eval", blocks=4, players=2, seed_start=1000):
    return {
        "players": players,
        "seed_start": seed_start,
        "first_block": 0,
        "blocks": blocks,
        "candidate": candidate,
        "baseline": "random",
        "opponent": "greedy",
        "extra": False,
        "decision_budget_ms": 0,
        "mcts_simulations": 64,
        "mcts_tree_depth": 2,
    }


def always_wins(block, seat):
    return 1.0


def wins_seat_zero_only(block, seat):
    return 1.0 if seat == 0 else 0.0


class AblationTests(unittest.TestCase):
    def test_variant_below_control_reports_a_negative_paired_delta(self):
        control = arm_rows(credit=always_wins)
        variant = arm_rows(credit=wins_seat_zero_only)
        report = compare_arms(
            {"control": control, "variant": variant},
            control="control",
            per_arm_options={"control": options(), "variant": options()},
            iterations=500,
        )
        self.assertEqual(report["mode"], "ablation_vs_control_arm")
        self.assertTrue(report["baseline_games_are_shared"])
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertTrue(by_arm["control"]["is_control"])
        self.assertAlmostEqual(by_arm["control"]["candidate_first_credit_pct"], 100.0)
        self.assertAlmostEqual(by_arm["variant"]["candidate_first_credit_pct"], 50.0)
        self.assertAlmostEqual(
            by_arm["control"]["mean_first_credit_delta_pp_vs_control"], 0.0)
        self.assertAlmostEqual(
            by_arm["variant"]["mean_first_credit_delta_pp_vs_control"], -50.0)
        low, high = by_arm["variant"]["paired_block_bootstrap_95ci_pp_vs_control"]
        self.assertLessEqual(low, high)
        self.assertAlmostEqual(low, -50.0)
        self.assertAlmostEqual(high, -50.0)

    def test_identical_arms_produce_exactly_zero(self):
        control = arm_rows(credit=wins_seat_zero_only)
        report = compare_arms(
            {"control": control, "same": copy.deepcopy(control)},
            control="control",
            per_arm_options={"control": options(), "same": options()},
            iterations=500,
        )
        for row in report["arms"]:
            self.assertAlmostEqual(row["mean_first_credit_delta_pp_vs_control"], 0.0)
            self.assertEqual(
                row["paired_block_bootstrap_95ci_pp_vs_control"], [0.0, 0.0])

    def test_control_delta_against_its_own_baseline_stays_separate(self):
        # The control arm still reports its own candidate-vs-baseline result;
        # that number must not be confused with the ablation delta.
        control = arm_rows(credit=always_wins)
        variant = arm_rows(credit=always_wins)
        report = compare_arms(
            {"control": control, "variant": variant},
            control="control",
            per_arm_options={"control": options(), "variant": options()},
            iterations=500,
        )
        for row in report["arms"]:
            self.assertGreater(
                row["mean_first_credit_delta_pp_vs_own_baseline"], 0.0)
            self.assertAlmostEqual(
                row["mean_first_credit_delta_pp_vs_control"], 0.0)

    def test_a_different_baseline_game_breaks_comparability(self):
        control = arm_rows(credit=always_wins)
        variant = arm_rows(credit=always_wins)
        for row in variant:
            if row["arm_id"] == "baseline" and row["candidate_seat"] == 1:
                row["replay_hash"] = "f" * 64
                break
        with self.assertRaisesRegex(ValueError, "same baseline games"):
            compare_arms(
                {"control": control, "variant": variant},
                control="control",
                per_arm_options={"control": options(), "variant": options()},
                iterations=500,
            )

    def test_different_deck_identity_breaks_arm_comparability(self):
        control = arm_rows(credit=always_wins)
        variant = copy.deepcopy(control)
        for row in variant:
            row["card_set_hash"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "different card-set identity"):
            compare_arms(
                {"control": control, "variant": variant},
                control="control",
                per_arm_options={"control": options(), "variant": options()},
                iterations=500,
            )

    def test_arms_on_different_seeds_are_rejected(self):
        control = arm_rows(credit=always_wins)
        variant = arm_rows(credit=always_wins, seed_base=2000)
        with self.assertRaisesRegex(ValueError, "disagrees with control"):
            compare_arms(
                {"control": control, "variant": variant},
                control="control",
                per_arm_options={
                    "control": options(),
                    "variant": options(seed_start=2000),
                },
                iterations=500,
            )

    def test_missing_control_or_single_arm_is_rejected(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "control arm"):
            compare_arms(
                {"variant": rows}, control="control",
                per_arm_options={"variant": options()}, iterations=500)
        with self.assertRaisesRegex(ValueError, "at least a control"):
            compare_arms(
                {"control": rows}, control="control",
                per_arm_options={"control": options()}, iterations=500)
        with self.assertRaisesRegex(ValueError, "no declared options"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)}, control="control",
                per_arm_options={"control": options()}, iterations=500)

    def test_arm_that_compares_a_policy_with_itself_is_rejected(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "compares a policy with itself"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)}, control="control",
                per_arm_options={"control": options(), "variant": options(candidate="random")},
                iterations=500,
            )

    def test_unknown_policy_and_invalid_iterations_are_rejected(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "unsupported policy"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)}, control="control",
                per_arm_options={"control": options(), "variant": options(candidate="puct")},
                iterations=500,
            )
        with self.assertRaisesRegex(ValueError, "bootstrap iterations"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)}, control="control",
                per_arm_options={"control": options(), "variant": options()},
                iterations=10,
            )

    def test_multi_factor_change_is_rejected(self):
        rows = arm_rows(credit=always_wins)
        variant_options = options(candidate="mcts")
        variant_options["mcts_simulations"] = 2
        variant_rows = copy.deepcopy(rows)
        for row in variant_rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = "mcts"
        with self.assertRaisesRegex(ValueError, "multiple candidate factors"):
            compare_arms(
                {"control": rows, "variant": variant_rows},
                control="control",
                per_arm_options={"control": options(), "variant": variant_options},
                iterations=500,
            )

    def test_empty_arm_is_rejected(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "must contain game records"):
            compare_arms(
                {"control": rows, "variant": []}, control="control",
                per_arm_options={"control": options(), "variant": options()},
                iterations=500,
            )


if __name__ == "__main__":
    unittest.main()
