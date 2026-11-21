"""Fail-closed tests for the design-aligned eval_harness report."""
import copy
import json
import unittest

from scripts.eval_report import RunError, load_run, validate_and_report

HEX_A = "a" * 64
HEX_B = "b" * 64


def make_run(
    blocks=3,
    players=4,
    *,
    draft=False,
    opponent=None,
    run_mode="full",
    registered=False,
    official_final_mode=False,
    candidate_credit="all",
    candidate_policy="eval_veg",
):
    opponent_policy = opponent or "greedy"
    manifest = {
        "schema_version": 1,
        "harness": "eval_harness",
        "experiment_id": "eval-test",
        "run_id": "run-test",
        "condition_id": HEX_A,
        "players": players,
        "additional_action_space": False,
        "run_mode": run_mode,
        "strength_claim_allowed": False,
        "preregistration": {"registered": registered},
        "official_final_mode": official_final_mode,
        "official_final_mode_blocked_by": ["test fixture"] if not official_final_mode else [],
        "cards": "fixture_4p" if draft else "none",
        "card_set_count": 80 if draft else 0,
        "card_set_hash": HEX_B,
        "draft": {
            "mode": "draft_10_7_simultaneous" if draft else "none",
            "ruleset_id": "eval-4p-10-7-right-v1" if draft else None,
            "deck_version": "eval-fixture-4p-v1" if draft else None,
        },
        "arms": {
            "candidate": {
                "game_policy_id": candidate_policy,
                "policy": candidate_policy,
                "puct_config_id": None,
                "draft_policy_id": "synergy" if draft else None,
            },
            "baseline": {
                "game_policy_id": "greedy",
                "policy": "greedy",
                "puct_config_id": None,
                "draft_policy_id": "card_value" if draft else None,
            },
            "opponent": {
                "game_policy_id": "greedy" if opponent_policy == "greedy" else opponent_policy,
                "policy": opponent_policy,
            },
        },
        "compute_budget": {
            "search_mode": "simulation-capped",
            "decision_budget_ms": 10000,
        },
        "seeds": {
            "seed_start": 1000,
            "draft_seed_start": 2000,
            "first_block": 0,
            "blocks": blocks,
        },
        "artifact": {
            "matches": "shard-0/matches.jsonl",
            "row_schema_version": 2,
            "shards": 1,
        },
    }
    rows = []
    for block in range(blocks):
        for arm in ("candidate", "baseline"):
            for seat in range(players):
                scores = [20] * players
                ranks = [2] * players
                if arm == "candidate":
                    if candidate_credit == "all" or (
                        candidate_credit == "half" and seat < players // 2
                    ):
                        winner = seat
                    else:
                        winner = (seat + 1) % players
                else:
                    # The mirror arm rotates the winning seat by block, so the
                    # focus seat wins exactly one of the four rotations.
                    winner = block % players
                scores[winner], ranks[winner] = 30, 1
                ranks = [
                    1 + sum(other > scores[i] for other in scores)
                    for i in range(players)
                ]
                # The helper above ignores resource tiebreaks; with distinct
                # winners and equal losers it produces the engine's ranks.
                focus_id = (
                    manifest["arms"]["candidate"]["game_policy_id"]
                    if arm == "candidate"
                    else manifest["arms"]["baseline"]["game_policy_id"]
                )
                opponent_id = manifest["arms"]["opponent"]["game_policy_id"]
                draft_by_seat = None
                draft_fields = {
                    "draft_mode": "none",
                    "draft_ruleset_id": None,
                    "draft_deck_version": None,
                    "draft_policy_id_by_seat": None,
                    "draft_replay_hash": None,
                    "start_player": None,
                    "final_hands_by_seat": None,
                    "discarded_private_ids": None,
                    "focus_pick_max_ms": None,
                    "focus_pick_p95_ms": None,
                    "pick_timeout_count": None,
                    "pick_fallback_count": None,
                    "focus_observed_all_picks_within_budget": None,
                }
                if draft:
                    draft_by_seat = [
                        manifest["arms"][arm]["draft_policy_id"]
                        if other == seat
                        else manifest["arms"]["baseline"]["draft_policy_id"]
                        for other in range(players)
                    ]
                    draft_fields = {
                        "draft_mode": "draft_10_7_simultaneous",
                        "draft_ruleset_id": "eval-4p-10-7-right-v1",
                        "draft_deck_version": "eval-fixture-4p-v1",
                        "draft_policy_id_by_seat": draft_by_seat,
                        "draft_replay_hash": "c" * 64,
                        "start_player": block % players,
                        "final_hands_by_seat": [
                            {
                                "occupations": [f"FIX-OCC-{seat}-{index}" for index in range(7)],
                                "minor_improvements": [
                                    f"FIX-MIN-{seat}-{index}" for index in range(7)
                                ],
                            }
                            for seat in range(players)
                        ],
                        "discarded_private_ids": ["FIX-OCC-39"],
                        "focus_pick_max_ms": 1.5,
                        "focus_pick_p95_ms": 1.0,
                        "pick_timeout_count": 0,
                        "pick_fallback_count": 0,
                        "focus_observed_all_picks_within_budget": True,
                    }
                mean_other = (sum(scores) - scores[seat]) / (players - 1)
                row = {
                    "schema_version": 2,
                    "condition_id": HEX_A,
                    "block_index": block,
                    "seed": 1000 + block,
                    "draft_seed": 2000 + block if draft else None,
                    "arm_id": arm,
                    "focus_policy": manifest["arms"][arm]["policy"],
                    "opponent_policy": opponent_policy,
                    "candidate_seat": seat,
                    "players": players,
                    "additional_action_space": False,
                    "cards": manifest["cards"],
                    "card_set_count": manifest["card_set_count"],
                    "card_set_hash": HEX_B,
                    "candidate_puct_config_id": "puct-default",
                    "baseline_puct_config_id": "puct-default",
                    "focus_puct_config_id": None,
                    "game_policy_id_by_seat": [
                        focus_id if other == seat else opponent_id
                        for other in range(players)
                    ],
                    "finished": True,
                    "rounds_completed": 14,
                    "replay_verified": True,
                    "replay_hash": "d" * 64,
                    "policy_select_only": True,
                    "scores_by_seat": scores,
                    "ranks_by_seat": ranks,
                    "first_place_credit": 1.0 if ranks[seat] == 1 else 0.0,
                    "expected_rank_observation": ranks[seat],
                    "score_margin": scores[seat] - mean_other,
                    "decisions": 70,
                    "focus_decisions": 30,
                    "decision_budget_ms": 10000,
                    "whole_decision_budget_enforced": False,
                    "detail_reserve_ms": 0,
                    "time_budget_max_simulations": 100000,
                    "search_mode": "simulation-capped",
                    "focus_max_decision_ms": 0.8,
                    "focus_p95_decision_ms": 0.6,
                    "focus_p99_decision_ms": 0.7,
                    "total_wall_ms": 100.0,
                }
                row.update(draft_fields)
                rows.append(row)
    return manifest, rows


def find(rows, arm, seat=0, block=0):
    return next(
        row
        for row in rows
        if row["arm_id"] == arm
        and row["block_index"] == block
        and row["candidate_seat"] == seat
    )


class EvalReportTests(unittest.TestCase):
    def test_reports_metrics_and_paired_block_intervals(self):
        manifest, rows = make_run()
        report = validate_and_report(manifest, rows)
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["completeness"]["total_games"], 24)
        self.assertAlmostEqual(report["metrics"]["candidate"]["first_credit"], 1.0)
        self.assertAlmostEqual(report["metrics"]["baseline"]["first_credit"], 0.25)
        self.assertAlmostEqual(report["metrics"]["candidate"]["expected_rank"], 1.0)
        self.assertAlmostEqual(report["metrics"]["baseline"]["expected_rank"], 1.75)
        self.assertAlmostEqual(
            report["deltas"]["first_credit"]["block_delta_mean"], 0.75
        )
        self.assertEqual(
            report["deltas"]["first_credit"]["paired_block_bootstrap_95ci"],
            [0.75, 0.75],
        )
        self.assertAlmostEqual(
            report["deltas"]["expected_rank"]["block_delta_mean"], -0.75
        )
        self.assertGreater(
            report["deltas"]["score_margin"]["block_delta_mean"], 0
        )
        self.assertEqual(report["guards"]["saturation"]["status"], "fail")
        self.assertEqual(report["decision"]["final"], "undetermined")
        self.assertEqual(report["decision"]["layer1_signal"], "adopted_target_met")
        self.assertTrue(report["comparison_design"]["head_to_head"])
        self.assertIn("| candidate |", report["markdown"])

    def test_report_is_deterministic_for_a_fixed_seed(self):
        manifest, rows = make_run()
        first = validate_and_report(manifest, rows)
        second = validate_and_report(manifest, rows)
        self.assertEqual(first, second)
        self.assertEqual(first["seeds"]["bootstrap"]["seed"], 20261106)
        self.assertEqual(first["seeds"]["bootstrap"]["iterations"], 10000)

    def test_smoke_runs_cannot_claim_strength(self):
        manifest, rows = make_run(run_mode="smoke")
        report = validate_and_report(manifest, rows)
        self.assertEqual(report["source"]["measurement_mode"], "smoke")
        self.assertFalse(report["claim_policy"]["strength_claim_allowed"])
        self.assertIn("SMOKE", report["markdown"])

    def test_fixture_runs_are_labelled_functional_only(self):
        manifest, rows = make_run(draft=True)
        report = validate_and_report(manifest, rows)
        self.assertIn("FIXTURE", report["markdown"])
        self.assertEqual(report["condition"]["draft_mode"], "draft_10_7_simultaneous")
        self.assertEqual(report["metrics"]["candidate"]["games"], 12)

    def test_common_opponent_is_not_head_to_head(self):
        manifest, rows = make_run(opponent="eval")
        report = validate_and_report(manifest, rows)
        self.assertFalse(report["comparison_design"]["head_to_head"])
        self.assertEqual(report["guards"]["direct_comparison"]["status"], "fail")

    def test_missing_and_duplicate_records_are_rejected(self):
        manifest, rows = make_run()
        with self.assertRaises(RunError):
            validate_and_report(manifest, rows[:-1])
        with self.assertRaises(RunError):
            validate_and_report(manifest, rows + [copy.deepcopy(rows[0])])

    def test_condition_and_seed_mismatches_are_rejected(self):
        manifest, rows = make_run()
        changed = copy.deepcopy(rows)
        changed[0]["condition_id"] = "f" * 64
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

        changed = copy.deepcopy(rows)
        changed[0]["seed"] = changed[0]["seed"] + 1
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

    def test_unverified_replay_and_bad_credit_are_rejected(self):
        manifest, rows = make_run()
        changed = copy.deepcopy(rows)
        changed[0]["replay_verified"] = False
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

        changed = copy.deepcopy(rows)
        changed[0]["first_place_credit"] = 0.0
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

    def test_draft_rows_need_the_full_hand_and_pick_record(self):
        manifest, rows = make_run(draft=True)
        changed = copy.deepcopy(rows)
        changed[0]["final_hands_by_seat"][0]["occupations"].pop()
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

        changed = copy.deepcopy(rows)
        changed[0]["draft_replay_hash"] = "not-a-hash"
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

        changed = copy.deepcopy(rows)
        changed[0]["draft_mode"] = "none"
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

    def test_mixed_conditions_are_rejected(self):
        manifest, rows = make_run()
        changed = copy.deepcopy(rows)
        changed[0]["search_mode"] = "time-budgeted"
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

        changed = copy.deepcopy(rows)
        changed[0]["game_policy_id_by_seat"][0] = "random"
        with self.assertRaises(RunError):
            validate_and_report(manifest, changed)

    def test_manifest_requires_the_design_alignment_flags(self):
        manifest, rows = make_run()
        changed = copy.deepcopy(manifest)
        changed["strength_claim_allowed"] = True
        with self.assertRaises(RunError):
            validate_and_report(changed, rows)

        changed = copy.deepcopy(manifest)
        changed["artifact"]["row_schema_version"] = 1
        with self.assertRaises(RunError):
            validate_and_report(changed, rows)

    def test_load_run_reads_the_manifest_and_shards(self):
        import tempfile
        from pathlib import Path

        manifest, rows = make_run()
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "shard-0").mkdir()
            (root / "run-manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            with (root / "shard-0" / "matches.jsonl").open("w", encoding="utf-8") as file:
                for row in rows:
                    file.write(json.dumps(row) + "\n")
            loaded_manifest, loaded_rows = load_run(root)
            self.assertEqual(loaded_manifest["condition_id"], manifest["condition_id"])
            self.assertEqual(len(loaded_rows), len(rows))
            report = validate_and_report(loaded_manifest, loaded_rows)
            self.assertEqual(report["completeness"]["total_games"], len(rows))


    def test_unregistered_runs_stay_undetermined_even_with_a_clear_signal(self):
        manifest, rows = make_run()
        report = validate_and_report(manifest, rows)
        self.assertFalse(report["decision"]["preregistered"])
        self.assertEqual(report["decision"]["layer1_signal"], "adopted_target_met")
        self.assertEqual(report["decision"]["final"], "undetermined")
        self.assertIn("pre-registered", report["decision"]["final_reason"])
        self.assertIn("undetermined", report["markdown"])

    def test_registered_and_guarded_run_applies_the_lexicographic_rule(self):
        manifest, rows = make_run(
            candidate_credit="half", registered=True, official_final_mode=True
        )
        report = validate_and_report(manifest, rows)
        self.assertTrue(report["decision"]["preregistered"])
        self.assertTrue(report["decision"]["guards_passed"])
        self.assertEqual(report["decision"]["layer1_signal"], "adopted_target_met")
        self.assertEqual(report["decision"]["final"], "adopted_target_met")
        self.assertFalse(
            report["decision"]["layer2_rank_reference"]["reference_allowed"]
        )

    def test_registered_smoke_run_is_blocked_by_the_smoke_guard(self):
        manifest, rows = make_run(
            candidate_credit="half",
            registered=True,
            official_final_mode=True,
            run_mode="smoke",
        )
        report = validate_and_report(manifest, rows)
        self.assertIn("smoke_mode", report["decision"]["unresolved_guards"])
        self.assertEqual(report["decision"]["final"], "undetermined")

    def test_registered_fixture_run_is_blocked_by_the_final_mode_guard(self):
        manifest, rows = make_run(draft=True, registered=True)
        report = validate_and_report(manifest, rows)
        self.assertIn(
            "official_final_mode", report["decision"]["unresolved_guards"]
        )
        self.assertEqual(report["decision"]["final"], "undetermined")

if __name__ == "__main__":
    unittest.main()
