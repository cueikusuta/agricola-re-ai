"""Fail-closed tests for one-factor-at-a-time ablation comparisons."""
import copy
import json
import unittest
from pathlib import Path

from scripts.eval_ablation import check_plan, compare_arms, plan_arms


def arm_rows(blocks=4, players=2, *, credit, seed_base=1000, replay="a",
             search_mode="simulation-capped", search_cap=100000):
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
                    "decision_budget_ms": (
                        10000 if search_mode == "time-budgeted" else 0),
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
                    # #167 A: the budget covers selection plus detail generation,
                    # so every row must describe the whole-decision accounting.
                    "whole_decision_budget_enforced": (
                        search_mode == "time-budgeted"),
                    "detail_reserve_ms": 100 if search_mode == "time-budgeted" else 0,
                    "focus_max_detail_ms": 0.2,
                    "focus_p95_detail_ms": 0.1,
                    "focus_late_decision_count": 0,
                    "focus_late_beyond_tolerance_count": 0,
                    "focus_max_decision_overshoot_ms": 0.0,
                    "focus_budget_tolerance_ms": 1.0,
                    "focus_decision_fallback_count": 0,
                    "focus_selection_fallback_count": 0,
                    "focus_detail_fallback_count": 0,
                    "focus_detail_declined_count": 0,
                    "focus_budget_exhausted_before_details_count": 0,
                    "focus_observed_all_decisions_within_budget": (
                        search_mode == "time-budgeted"),
                    "focus_observed_all_decisions_within_budget_tolerance": (
                        search_mode == "time-budgeted"),
                    "focus_observed_all_decisions_within_10s": True,
                    "mcts_deadline_active": False,
                    "deterministic_policy_selection": True,
                    "search_mode": search_mode,
                    "search_budget_enforced": search_mode == "time-budgeted",
                    "mcts_simulations_cap": 64,
                    "time_budget_max_simulations": search_cap,
                    "mcts_simulations_per_decision": 0.0,
                    "max_deadline_overshoot_ms": 0,
                    "max_search_elapsed_ms": 0,
                    "policy_select_only": True,
                    "total_wall_ms": 50.0,
                    "replay_hash": (replay + f"{block:02d}{seat}").ljust(64, "0"),
                    "replay_verified": True,
                })
    return rows


def options(*, candidate="eval", blocks=4, players=2, seed_start=1000,
            search_mode="simulation-capped"):
    return {
        "players": players,
        "seed_start": seed_start,
        "first_block": 0,
        "blocks": blocks,
        "candidate": candidate,
        "baseline": "random",
        "opponent": "greedy",
        "extra": False,
        "decision_budget_ms": (
            10000 if search_mode == "time-budgeted" else 0),
        "search_mode": search_mode,
        "time_budget_max_simulations": 100000,
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
                per_arm_options={"control": options(), "variant": options(candidate="stockfish")},
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



class AblationSearchModeTests(unittest.TestCase):
    """#164: the mode must reach the validator, and modes never mix."""

    def test_time_budgeted_arms_are_accepted_together(self):
        control = arm_rows(credit=always_wins, search_mode="time-budgeted")
        variant = arm_rows(credit=wins_seat_zero_only, search_mode="time-budgeted")
        report = compare_arms(
            {"control": control, "variant": variant},
            control="control",
            per_arm_options={
                "control": options(search_mode="time-budgeted"),
                "variant": options(search_mode="time-budgeted"),
            },
            iterations=500,
        )
        self.assertEqual(report["search_mode"], "time-budgeted")
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertAlmostEqual(
            by_arm["variant"]["mean_first_credit_delta_pp_vs_control"], -50.0)

    def test_mixing_search_modes_across_arms_is_rejected(self):
        control = arm_rows(credit=always_wins)
        variant = arm_rows(credit=always_wins, search_mode="time-budgeted")
        with self.assertRaisesRegex(ValueError, "disagrees with control"):
            compare_arms(
                {"control": control, "variant": variant},
                control="control",
                per_arm_options={
                    "control": options(),
                    "variant": options(search_mode="time-budgeted"),
                },
                iterations=500,
            )

    def test_declared_mode_must_match_the_rows(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "simulation-capped"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": options(search_mode="time-budgeted"),
                    "variant": options(search_mode="time-budgeted"),
                },
                iterations=500,
            )


class AblationPuctParameterTests(unittest.TestCase):
    """#167 B: every PUCT knob is an arm parameter and reaches the binary."""

    def puct_options(self, candidate="puct", **overrides):
        result = options(candidate=candidate)
        result["puct_simulations"] = 8
        result.update(overrides)
        return result

    def policy_options(self, candidate, **overrides):
        result = options(candidate=candidate)
        result.update(overrides)
        return result

    def rows_for(self, candidate, credit=None, **kwargs):
        rows = arm_rows(credit=credit or always_wins, **kwargs)
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = candidate
        return rows

    def test_every_search_parameter_reaches_the_command_line(self):
        from pathlib import Path

        from scripts.eval_ablation import SEARCH_PARAMETERS, arm_command

        arm = self.puct_options(
            puct_simulations=9,
            puct_tree_depth=3,
            puct_c=2.5,
            puct_prior_temp=1.25,
            puct_value_temp=4.0,
            puct_leaf="heuristic",
            puct_root_limit=7,
            puct_final_prior=0.5,
        )
        command = arm_command(Path("paired_eval"), arm, Path("out.jsonl"))
        for name, (flag, _, _) in SEARCH_PARAMETERS.items():
            self.assertIn(flag, command, f"{name} flag is missing")
            value = command[command.index(flag) + 1]
            expected = arm.get(name, SEARCH_PARAMETERS[name][1])
            if isinstance(expected, float):
                self.assertAlmostEqual(float(value), expected)
            else:
                self.assertEqual(value, str(expected))

    def test_defaults_are_passed_explicitly(self):
        from pathlib import Path

        from scripts.eval_ablation import arm_command

        command = arm_command(
            Path("paired_eval"), options(candidate="mcts"), Path("out.jsonl"))
        # An arm that declares nothing still shows the values it actually ran
        # with, so the artifact cannot disagree with the run.
        self.assertEqual(command[command.index("--puct-leaf") + 1], "terminal")
        self.assertEqual(command[command.index("--mcts-simulations") + 1], "64")
        self.assertEqual(command[command.index("--puct-c") + 1], "1.5")

    def test_a_puct_arm_reports_its_parameters_and_changed_factors(self):
        rows = self.rows_for("puct")
        control = self.puct_options()
        variant = self.puct_options(puct_prior_temp=1.0)
        report = compare_arms(
            {"control": rows, "warm": copy.deepcopy(rows)},
            control="control",
            per_arm_options={"control": control, "warm": variant},
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertEqual(
            by_arm["warm"]["changed_search_parameters"],
            {"puct_prior_temp": {"control": 3.0, "arm": 1.0}},
        )
        self.assertAlmostEqual(
            by_arm["warm"]["search_parameters"]["puct_prior_temp"], 1.0)
        self.assertAlmostEqual(
            by_arm["control"]["search_parameters"]["puct_c"], 1.5)
        self.assertEqual(by_arm["control"]["changed_search_parameters"], {})

    def test_leaf_evaluation_and_exploration_constant_are_separate_factors(self):
        rows = self.rows_for("puct")
        report = compare_arms(
            {
                "control": rows,
                "leaf": copy.deepcopy(rows),
                "explore": copy.deepcopy(rows),
            },
            control="control",
            per_arm_options={
                "control": self.puct_options(),
                "leaf": self.puct_options(puct_leaf="heuristic"),
                "explore": self.puct_options(puct_c=3.0),
            },
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertEqual(
            list(by_arm["leaf"]["changed_search_parameters"]), ["puct_leaf"])
        self.assertEqual(
            list(by_arm["explore"]["changed_search_parameters"]), ["puct_c"])

    def test_two_puct_knobs_at_once_are_rejected(self):
        rows = self.rows_for("puct")
        with self.assertRaisesRegex(ValueError, "multiple candidate factors"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": self.puct_options(),
                    "variant": self.puct_options(puct_c=3.0, puct_leaf="heuristic"),
                },
                iterations=500,
            )

    def test_puct_parameter_changes_in_a_non_puct_arm_are_rejected(self):
        rows = self.rows_for("mcts")
        with self.assertRaisesRegex(ValueError, "PUCT-only parameters"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": self.policy_options("mcts"),
                    "variant": self.policy_options("mcts", puct_c=3.0),
                },
                iterations=500,
            )

    def test_unknown_arm_parameter_is_rejected(self):
        rows = arm_rows(credit=always_wins)
        with self.assertRaisesRegex(ValueError, "unknown parameter"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": options(),
                    "variant": self.policy_options("eval", mcts_simulatoins=2),
                },
                iterations=500,
            )

    def test_invalid_parameter_values_are_rejected(self):
        cases = (
            ({"puct_leaf": "rollout"}, "heuristic"),
            ({"puct_prior_temp": 0.0}, "must be > 0"),
            ({"puct_value_temp": -1.0}, "must be > 0"),
            ({"puct_c": -0.5}, "must be 0..=100"),
            ({"puct_c": float("inf")}, "finite"),
            ({"puct_c": float("nan")}, "finite"),
            ({"puct_root_limit": 101}, "must be 0..=100"),
            ({"puct_final_prior": 11.0}, "must be 0..=10"),
            ({"puct_simulations": 0}, "must be 1..=100000"),
            ({"puct_simulations": 8.5}, "must be an integer"),
            ({"mcts_tree_depth": 7}, "must be 1..=6"),
        )
        rows = arm_rows(credit=always_wins)
        for override, message in cases:
            with self.assertRaisesRegex(ValueError, message):
                compare_arms(
                    {"control": rows, "variant": copy.deepcopy(rows)},
                    control="control",
                    per_arm_options={
                        "control": options(),
                        "variant": self.policy_options("eval", **override),
                    },
                    iterations=500,
                )

    def test_mcts_and_puct_arms_stay_comparable_on_the_same_baseline(self):
        # Every arm needs its own candidate rows because the recorded
        # focus_policy must match the arm's declared candidate.
        report = compare_arms(
            {"control": self.rows_for("mcts"), "puct": self.rows_for("puct")},
            control="control",
            per_arm_options={
                # Only the policy differs: a PUCT simulation count that the MCTS
                # control cannot use would be a second factor, and the guard
                # below must reject that instead of attributing it to PUCT.
                "control": self.policy_options("mcts"),
                "puct": self.policy_options("puct"),
            },
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertTrue(report["baseline_games_are_shared"])
        self.assertIn("puct_c", by_arm["puct"]["search_parameters"])
        self.assertEqual(by_arm["puct"]["candidate"], "puct")
        self.assertAlmostEqual(
            by_arm["puct"]["mean_first_credit_delta_pp_vs_control"], 0.0)




class AblationParameterEffectivenessTests(unittest.TestCase):
    """#167 B review P1: only parameters that can change the run are factors.

    `paired_eval` accepts every flag on every run, so a mislabelled arm would
    otherwise "change" a knob that no seat reads (or that the wall clock
    overrides) and report a zero delta as if it were evidence.
    """

    def opts(self, **overrides):
        result = options()
        result.update(overrides)
        return result

    def rows_for(self, candidate, baseline="random", search_mode="simulation-capped"):
        rows = arm_rows(credit=always_wins, search_mode=search_mode)
        for row in rows:
            policy = candidate if row["arm_id"] == "candidate" else baseline
            row["focus_policy"] = policy
            # The aggregator requires the deadline flags to follow the mode and
            # the focus policy, so keep the fixture self-consistent.
            search_policy = policy in ("mcts", "puct")
            deadline = search_policy and search_mode == "time-budgeted"
            row["mcts_deadline_active"] = deadline
            row["deterministic_policy_selection"] = not deadline
        return rows

    def test_mcts_parameter_change_is_rejected_without_an_mcts_policy(self):
        rows = self.rows_for("puct")
        with self.assertRaisesRegex(ValueError, "MCTS-only parameters"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": self.opts(candidate="puct"),
                    "variant": self.opts(candidate="puct", mcts_tree_depth=4),
                },
                iterations=500,
            )

    def test_puct_parameter_change_is_rejected_without_a_puct_policy(self):
        rows = self.rows_for("mcts")
        with self.assertRaisesRegex(ValueError, "PUCT-only parameters"):
            compare_arms(
                {"control": rows, "variant": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": self.opts(candidate="mcts"),
                    "variant": self.opts(candidate="mcts", puct_prior_temp=1.0),
                },
                iterations=500,
            )

    def test_a_simulation_count_in_time_budgeted_mode_is_rejected(self):
        # The wall clock stops the search there; the count is only a safety cap.
        for candidate, knob in (("mcts", "mcts_simulations"),
                                ("puct", "puct_simulations")):
            rows = self.rows_for(candidate, search_mode="time-budgeted")
            with self.assertRaisesRegex(ValueError, "safety cap"):
                compare_arms(
                    {"control": rows, "variant": copy.deepcopy(rows)},
                    control="control",
                    per_arm_options={
                        "control": self.opts(
                            candidate=candidate, search_mode="time-budgeted"),
                        "variant": self.opts(
                            candidate=candidate,
                            search_mode="time-budgeted",
                            **{knob: 7},
                        ),
                    },
                    iterations=500,
                )

    def test_a_tree_depth_in_time_budgeted_mode_is_still_a_valid_factor(self):
        rows = self.rows_for("mcts", search_mode="time-budgeted")
        report = compare_arms(
            {"control": rows, "variant": copy.deepcopy(rows)},
            control="control",
            per_arm_options={
                "control": self.opts(candidate="mcts", search_mode="time-budgeted"),
                "variant": self.opts(
                    candidate="mcts", search_mode="time-budgeted", mcts_tree_depth=4),
            },
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertEqual(
            by_arm["variant"]["effective_changed_search_parameters"],
            ["mcts_tree_depth"],
        )

    def test_a_simulation_count_in_simulation_capped_mode_is_a_valid_factor(self):
        rows = self.rows_for("mcts")
        report = compare_arms(
            {"control": rows, "variant": copy.deepcopy(rows)},
            control="control",
            per_arm_options={
                "control": self.opts(candidate="mcts"),
                "variant": self.opts(candidate="mcts", mcts_simulations=8),
            },
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertEqual(
            by_arm["variant"]["changed_search_parameters"],
            {"mcts_simulations": {"control": 64, "arm": 8}},
        )
        self.assertEqual(
            by_arm["variant"]["effective_changed_search_parameters"],
            ["mcts_simulations"],
        )

    def test_an_ineffective_change_is_rejected_for_candidate_and_baseline(self):
        for candidate, baseline, knob in (
            ("puct", "eval", "mcts_simulations"),
            ("eval", "puct", "mcts_simulations"),
        ):
            rows = self.rows_for(candidate, baseline=baseline)
            with self.assertRaisesRegex(ValueError, "MCTS-only parameters"):
                compare_arms(
                    {"control": rows, "variant": copy.deepcopy(rows)},
                    control="control",
                    per_arm_options={
                        "control": self.opts(
                            candidate=candidate, baseline=baseline),
                        "variant": self.opts(
                            candidate=candidate,
                            baseline=baseline,
                            mcts_simulations=8,
                        ),
                    },
                    iterations=500,
                )


class AblationSharedConditionTests(unittest.TestCase):
    """#169 review: an arm may not change a condition the comparison depends on.

    The search cap is the case the reviewer found: in time-budgeted mode
    `time_budget_max_simulations` decides how much search is allowed, so 1 vs
    100000 is not a one-factor ablation of anything.
    """

    def opts(self, **overrides):
        result = options()
        result.update(overrides)
        return result

    def tiny_cap_opts(self):
        return self.opts(candidate="puct", search_mode="time-budgeted") | {
            "time_budget_max_simulations": 1,
            "puct_simulations": 32,
        }

    def wide_cap_opts(self):
        return self.opts(candidate="puct", search_mode="time-budgeted") | {
            "time_budget_max_simulations": 100000,
            "puct_simulations": 32,
        }

    def rows_for(self, candidate, baseline="random",
                 search_mode="simulation-capped", cards="none", card_set=None):
        rows = arm_rows(credit=always_wins, search_mode=search_mode)
        for row in rows:
            policy = candidate if row["arm_id"] == "candidate" else baseline
            row["focus_policy"] = policy
            search_policy = policy in ("mcts", "puct")
            deadline = search_policy and search_mode == "time-budgeted"
            row["mcts_deadline_active"] = deadline
            row["deterministic_policy_selection"] = not deadline
            row["cards"] = cards
            if card_set is not None:
                card_hash, card_count = card_set
                row["card_set_hash"] = card_hash
                row["card_set_count"] = card_count
        return rows

    def test_a_search_cap_mismatch_is_rejected_before_running(self):
        with self.assertRaisesRegex(ValueError, "time_budget_max_simulations"):
            check_plan(
                {"control": self.tiny_cap_opts(), "wide": self.wide_cap_opts()},
                "control",
                arms={"control", "wide"},
            )

    def test_aggregation_also_rejects_a_search_cap_mismatch(self):
        rows = self.rows_for("puct", search_mode="time-budgeted")
        with self.assertRaisesRegex(ValueError, "time_budget_max_simulations"):
            compare_arms(
                {"control": rows, "wide": copy.deepcopy(rows)},
                control="control",
                per_arm_options={
                    "control": self.tiny_cap_opts(),
                    "wide": self.wide_cap_opts(),
                },
                iterations=500,
            )

    def test_one_factor_comparison_with_a_shared_cap_is_accepted(self):
        per_arm = {
            "control": self.wide_cap_opts(),
            "explore": self.wide_cap_opts() | {"puct_c": 3.0},
        }
        changed, effective = check_plan(
            per_arm, "control", arms={"control", "explore"})
        self.assertEqual(changed["explore"], ["puct_c"])
        self.assertEqual(effective["explore"], ["puct_c"])

    def test_cards_reach_the_command_line(self):
        from pathlib import Path

        from scripts.eval_ablation import arm_command

        command = arm_command(
            Path("paired_eval"),
            self.opts(candidate="eval", cards="verified"),
            Path("out.jsonl"),
        )
        self.assertEqual(command[command.index("--cards") + 1], "verified_basic_2p")
        plain = arm_command(
            Path("paired_eval"), self.opts(candidate="eval"), Path("out.jsonl"))
        self.assertEqual(plain[plain.index("--cards") + 1], "none")

    def test_an_arm_cannot_change_the_card_condition(self):
        with self.assertRaisesRegex(ValueError, "cards"):
            check_plan(
                {
                    "control": self.opts(candidate="eval"),
                    "carded": self.opts(candidate="eval", cards="verified"),
                },
                "control",
                arms={"control", "carded"},
            )

    def test_a_carded_ablation_is_accepted_and_reports_its_condition(self):
        rows = self.rows_for(
            "eval", cards="verified_basic_2p", card_set=("a" * 64, 14))
        report = compare_arms(
            {"control": rows, "variant": copy.deepcopy(rows)},
            control="control",
            per_arm_options={
                "control": self.opts(candidate="eval", cards="verified"),
                "variant": self.opts(candidate="eval", cards="verified_basic_2p"),
            },
            iterations=500,
        )
        self.assertEqual(report["cards"], "verified_basic_2p")

    def test_cards_at_three_players_are_rejected(self):
        from scripts.eval_ablation import plan_arms

        spec = {
            "players": 3,
            "seed_start": 900,
            "blocks": 2,
            "baseline": "random",
            "opponent": "greedy",
            "cards": "verified",
            "arms": [{"label": "control", "candidate": "puct"}],
        }
        with self.assertRaisesRegex(ValueError, "two players only"):
            plan_arms(spec)

    def test_an_unknown_card_condition_is_rejected(self):
        from scripts.eval_ablation import plan_arms

        spec = {
            "players": 2,
            "seed_start": 900,
            "blocks": 2,
            "baseline": "random",
            "opponent": "greedy",
            "cards": "all_840",
            "arms": [{"label": "control", "candidate": "puct"}],
        }
        with self.assertRaisesRegex(ValueError, "unsupported card condition"):
            plan_arms(spec)

    def test_a_search_cap_must_be_a_positive_integer(self):
        from scripts.eval_ablation import plan_arms

        spec = {
            "players": 2,
            "seed_start": 900,
            "blocks": 2,
            "baseline": "random",
            "opponent": "greedy",
            "search_mode": "time-budgeted",
            "decision_budget_ms": 200,
            "time_budget_max_simulations": 0,
            "arms": [{"label": "control", "candidate": "puct"}],
        }
        with self.assertRaisesRegex(ValueError, "positive integer"):
            plan_arms(spec)


class AblationObservedArtifactTests(unittest.TestCase):
    """#169 re-review P1: the saved artifacts must match the declared plan.

    The aggregation path (`--run` absent) reads existing matches files, so a
    stale or hand-edited control run can claim any condition. These tests feed
    rows straight into `compare_arms`, which is exactly what that path does.
    """

    def opts(self, **overrides):
        result = options(candidate="puct", search_mode="time-budgeted")
        result.update(overrides)
        return result

    def rows(self, search_cap, candidate="puct", baseline="random"):
        rows = arm_rows(credit=always_wins, search_mode="time-budgeted",
                        search_cap=search_cap)
        for row in rows:
            if row["arm_id"] == "candidate":
                row["focus_policy"] = candidate
                deadline = candidate in ("mcts", "puct")
                row["mcts_deadline_active"] = deadline
                row["deterministic_policy_selection"] = not deadline
            else:
                row["focus_policy"] = baseline
                row["mcts_deadline_active"] = False
                row["deterministic_policy_selection"] = True
        return rows

    def test_artifacts_with_a_cap_that_contradicts_the_plan_are_rejected(self):
        # The plan says 100000 for both arms, but the stored control matches were
        # produced with a cap of 1: different search volumes, same baseline rows.
        with self.assertRaisesRegex(ValueError, "do not match the declared experiment"):
            compare_arms(
                {"control": self.rows(1), "variant": self.rows(100000)},
                control="control",
                per_arm_options={
                    "control": self.opts(),
                    "variant": self.opts(),
                },
                iterations=500,
            )

    def test_a_plan_that_lies_about_the_executed_cap_is_rejected(self):
        # Both arms really ran with 100000 but the plan claims 1: the reverse
        # direction must fail just as loudly.
        with self.assertRaisesRegex(ValueError, "do not match the declared experiment"):
            compare_arms(
                {"control": self.rows(100000), "variant": self.rows(100000)},
                control="control",
                per_arm_options={
                    "control": self.opts(time_budget_max_simulations=1),
                    "variant": self.opts(time_budget_max_simulations=1),
                },
                iterations=500,
            )

    def test_a_single_arm_with_mixed_caps_is_rejected(self):
        rows = self.rows(100000)
        rows[0]["time_budget_max_simulations"] = 1
        with self.assertRaisesRegex(ValueError, "do not match the declared experiment"):
            compare_arms(
                {"control": rows, "variant": self.rows(100000)},
                control="control",
                per_arm_options={"control": self.opts(), "variant": self.opts()},
                iterations=500,
            )

    def test_an_artifact_without_the_cap_field_is_rejected(self):
        rows = self.rows(100000)
        for row in rows:
            row.pop("time_budget_max_simulations")
        with self.assertRaisesRegex(ValueError, "do not match the declared experiment"):
            compare_arms(
                {"control": rows, "variant": self.rows(100000)},
                control="control",
                per_arm_options={"control": self.opts(), "variant": self.opts()},
                iterations=500,
            )

    def test_matching_artifacts_still_allow_a_one_factor_comparison(self):
        report = compare_arms(
            {"control": self.rows(100000), "explore": self.rows(100000)},
            control="control",
            per_arm_options={
                "control": self.opts(),
                "explore": self.opts(puct_c=3.0),
            },
            iterations=500,
        )
        by_arm = {row["arm"]: row for row in report["arms"]}
        self.assertEqual(
            by_arm["explore"]["changed_search_parameters"],
            {"puct_c": {"control": 1.5, "arm": 3.0}},
        )
        self.assertEqual(report["time_budget_max_simulations"], 100000)

    def test_a_simulation_capped_ablation_is_checked_too(self):
        rows = arm_rows(credit=always_wins, search_cap=100000)
        lying = arm_rows(credit=always_wins, search_cap=1)
        with self.assertRaisesRegex(ValueError, "do not match the declared experiment"):
            compare_arms(
                {"control": lying, "variant": rows},
                control="control",
                per_arm_options={
                    "control": options(),
                    "variant": options(),
                },
                iterations=500,
            )


class AblationSpecificationTests(unittest.TestCase):
    """#167 B: a specification key is validated before any game runs."""

    def spec(self, **overrides):
        base = {
            "players": 2,
            "seed_start": 900,
            "blocks": 2,
            "baseline": "random",
            "opponent": "greedy",
            "arms": [{"label": "control", "candidate": "puct"}],
        }
        base.update(overrides)
        return base

    def test_unknown_specification_key_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unknown specification parameter"):
            plan_arms(self.spec(puct_prior_temperature=1.0))

    def test_arm_level_parameter_overrides_the_specification(self):
        spec = self.spec(puct_c=2.0)
        spec["arms"] = [
            {"label": "control", "candidate": "puct"},
            {"label": "tight", "candidate": "puct", "puct_c": 0.5},
        ]
        per_arm = plan_arms(spec)
        self.assertEqual(per_arm["control"]["puct_c"], 2.0)
        self.assertEqual(per_arm["tight"]["puct_c"], 0.5)
        # Unset knobs fall back to the Rust defaults, never to "unset".
        self.assertEqual(per_arm["control"]["puct_leaf"], "terminal")
        self.assertEqual(per_arm["control"]["puct_value_temp"], 8.0)

    def test_unknown_arm_key_is_rejected_before_running(self):
        spec = self.spec()
        spec["arms"] = [{"label": "control", "candidate": "puct", "puct_temperature": 2}]
        with self.assertRaisesRegex(ValueError, "unknown parameter"):
            plan_arms(spec)

    def test_duplicate_and_unsafe_labels_are_rejected(self):
        spec = self.spec()
        spec["arms"] = [
            {"label": "same", "candidate": "puct"},
            {"label": "same", "candidate": "puct"},
        ]
        with self.assertRaisesRegex(ValueError, "duplicate arm label"):
            plan_arms(spec)
        spec["arms"] = [{"label": "bad/label", "candidate": "puct"}]
        with self.assertRaisesRegex(ValueError, "unsafe arm label"):
            plan_arms(spec)

    def test_unsupported_search_mode_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unsupported search mode"):
            plan_arms(self.spec(search_mode="budgeted"))

    def test_invalid_parameter_value_is_rejected_before_running(self):
        spec = self.spec()
        spec["arms"] = [{"label": "control", "candidate": "puct", "puct_c": -1.0}]
        with self.assertRaisesRegex(ValueError, "must be 0..=100"):
            plan_arms(spec)


class CommittedHeadToHeadPlanTests(unittest.TestCase):
    """#106 C-2/C-4: the committed experiment definition is machine-checked.

    The earlier 2-player comparison ranked PUCT and MCTS against a common
    `greedy` field and saturated at ~100%, so it could not resolve a +5 pp
    difference. This plan makes the baseline AI the fixed opponent instead:
    the candidate seat meets MCTS directly and the baseline arm is a mirror
    control.
    """

    def load(self):
        repository = Path(__file__).resolve().parent.parent
        return json.loads(
            (repository / "docs" / "m13-head-to-head-arms.json").read_text(encoding="utf-8"))

    def test_the_committed_plan_is_a_valid_one_factor_ablation(self):
        per_arm = plan_arms(self.load())
        changed, effective = check_plan(
            per_arm, "control_puct", arms=set(per_arm))
        for label, factors in changed.items():
            self.assertLessEqual(len(factors), 1, f"{label} changes {factors}")
        self.assertEqual(set(changed), set(per_arm))
        self.assertEqual(effective, changed)

    def test_the_committed_plan_is_head_to_head_on_a_time_budget(self):
        per_arm = plan_arms(self.load())
        control = per_arm["control_puct"]
        # Direct match: the fixed opponent is the baseline AI.
        self.assertEqual(control["baseline"], "mcts")
        self.assertEqual(control["opponent"], "mcts")
        self.assertNotEqual(control["candidate"], control["baseline"])
        # Primary comparison is the shared wall-clock budget, not a sim count.
        self.assertEqual(control["search_mode"], "time-budgeted")
        self.assertGreater(control["decision_budget_ms"], 0)
        for label, options in per_arm.items():
            self.assertEqual(options["search_mode"], "time-budgeted", label)
            self.assertEqual(options["decision_budget_ms"],
                             control["decision_budget_ms"], label)
            self.assertEqual(options["cards"], "none", label)

    def test_the_committed_plan_covers_the_required_factors(self):
        per_arm = plan_arms(self.load())
        changed, _ = check_plan(per_arm, "control_puct", arms=set(per_arm))
        factors = {name for values in changed.values() for name in values}
        self.assertIn("puct_prior_temp", factors, "prior contribution")
        self.assertIn("puct_c", factors, "exploration constant")
        self.assertIn("puct_leaf", factors, "leaf evaluation")
        self.assertIn("puct_tree_depth", factors, "search shape")

    def test_a_simulation_count_is_not_used_as_a_factor_on_a_time_budget(self):
        spec = self.load()
        spec["arms"] = [
            {"label": "control_puct", "candidate": "puct"},
            {"label": "more_sims", "candidate": "puct", "puct_simulations": 128},
        ]
        per_arm = plan_arms(spec)
        with self.assertRaisesRegex(ValueError, "simulation count is only a safety cap"):
            check_plan(per_arm, "control_puct", arms=set(per_arm))


if __name__ == "__main__":
    unittest.main()
