#!/usr/bin/env python3
"""Aggregate replay-verified, seat-rotated cardless AI comparisons.

Independent sampling unit: seed block (all seats, both arms together).
A 95% percentile bootstrap interval is a provisional design, not evidence
that an unverified mode is official-rules accurate.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import random
import statistics
import sys


def finite_number(value: object) -> bool:
    """Reject NaN, infinities, booleans, and non-numeric JSON values."""
    if type(value) is int:
        return abs(value) <= 9007199254740992
    return type(value) is float and math.isfinite(value)


def validate_and_report(
    rows: list[dict],
    *,
    players: int,
    seed_start: int,
    first_block: int,
    blocks: int,
    candidate: str,
    baseline: str,
    opponent: str,
    extra: bool,
    iterations: int = 10000,
) -> dict:
    if players not in (2, 3, 4) or blocks < 1 or iterations < 100:
        raise ValueError("invalid experimental settings")
    search_policies = ("mcts", "puct")
    if candidate == baseline or any(
        name not in ("random", "greedy", "eval", "mcts", "puct")
        for name in (candidate, baseline, opponent)
    ):
        raise ValueError("unsupported/identical policy")
    expected = {
        (block, arm, seat)
        for block in range(first_block, first_block + blocks)
        for arm in ("candidate", "baseline")
        for seat in range(players)
    }
    received = {}
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("game record must be an object")
        if any(type(row.get(field)) is not int for field in ("block_index", "candidate_seat", "seed")):
            raise ValueError("block, seat and seed must be integers")
        key = (row.get("block_index"), row.get("arm_id"), row.get("candidate_seat"))
        if key not in expected:
            raise ValueError(f"unexpected block/arm/seat: {key}")
        if key in received:
            raise ValueError(f"duplicate record: {key}")
        block, arm, seat = key
        if (
            type(row.get("schema_version")) is not int
            or row["schema_version"] != 1
            or row.get("seed") != seed_start + block
            or row.get("players") != players
            or row.get("cards") != "none"
            or row.get("additional_action_space") is not extra
            or row.get("focus_policy") != (candidate if arm == "candidate" else baseline)
            or row.get("opponent_policy") != opponent
            or row.get("finished") is not True
            or row.get("rounds_completed") != 14
            or row.get("replay_verified") is not True
            or not isinstance(row.get("replay_hash"), str)
            or len(row["replay_hash"]) != 64
            or any(char not in "0123456789abcdef" for char in row["replay_hash"])
            or row.get("policy_select_only") is not True
        ):
            raise ValueError(f"invalid metadata/verification: {key}")
        scores = row.get("scores_by_seat")
        ranks = row.get("ranks_by_seat")
        if (
            not isinstance(scores, list)
            or not isinstance(ranks, list)
            or len(scores) != players
            or len(ranks) != players
            or not all(type(x) is int for x in scores + ranks)
            or any(not 1 <= rank <= players for rank in ranks)
            or any(not -(2**31) <= score < 2**31 for score in scores)
        ):
            raise ValueError(f"invalid standings: {key}")
        # Scores must outrank lower scores; equal scores may use the
        # engine's resource tiebreaker, which is not present in this JSONL.
        if any(
            scores[i] > scores[j] and ranks[i] >= ranks[j]
            for i in range(players) for j in range(players)
        ):
            raise ValueError(f"score/rank ordering mismatch: {key}")
        # src/score.rs assigns competition ranks: one plus the number of
        # players with a higher score, or an equal score and more resources.
        # Equal resources share a rank; the next distinct rank must skip
        # exactly the number of players who held the preceding rank.
        for score in set(scores):
            group_ranks = [ranks[i] for i in range(players) if scores[i] == score]
            next_rank = 1 + sum(other > score for other in scores)
            for rank in sorted(set(group_ranks)):
                if rank != next_rank:
                    raise ValueError(f"impossible tiebreak rank sequence: {key}")
                next_rank += group_ranks.count(rank)
        # Recompute first-place credit from the engine's recorded ranks.
        firsts = ranks.count(1)
        if firsts < 1:
            raise ValueError(f"missing first place: {key}")
        credit = 1.0 / firsts if ranks[seat] == 1 else 0.0
        first_place_credit = row.get("first_place_credit")
        if (
            not finite_number(first_place_credit)
            or not 0 <= first_place_credit <= 1
            or abs(first_place_credit - credit) > 1e-9
        ):
            raise ValueError(f"first-place credit mismatch: {key}")
        mean_other = (sum(scores) - scores[seat]) / (players - 1)
        score_margin = row.get("score_margin")
        if not finite_number(score_margin) or abs(score_margin - (scores[seat] - mean_other)) > 1e-8:
            raise ValueError(f"margin mismatch: {key}")
        if (
            type(row.get("expected_rank_observation")) is not int
            or row["expected_rank_observation"] != ranks[seat]
        ):
            raise ValueError(f"rank mismatch: {key}")
        for numeric in ("total_wall_ms", "max_policy_select_ms"):
            value = row.get(numeric)
            if not finite_number(value) or value < 0:
                raise ValueError(f"invalid timing {numeric}: {key}")
        if row["max_policy_select_ms"] > row["total_wall_ms"]:
            raise ValueError(f"selection time exceeds game wall time: {key}")
        if type(row.get("decisions")) is not int or row["decisions"] <= 0:
            raise ValueError(f"invalid decisions: {key}")
        if type(row.get("decision_budget_ms")) is not int or not 0 <= row["decision_budget_ms"] <= 60000:
            raise ValueError(f"invalid decision budget: {key}")
        for numeric in ("p50_policy_select_ms", "p95_policy_select_ms", "p99_policy_select_ms"):
            value = row.get(numeric)
            if (
                not finite_number(value)
                or value < 0
                or value > row["max_policy_select_ms"] + 1e-9
            ):
                raise ValueError(f"invalid timing {numeric}: {key}")
        if not (
            row["p50_policy_select_ms"] <= row["p95_policy_select_ms"] + 1e-9
            and row["p95_policy_select_ms"] <= row["p99_policy_select_ms"] + 1e-9
        ):
            raise ValueError(f"timing percentiles out of order: {key}")
        for field in ("timeout_count", "fallback_count"):
            value = row.get(field)
            if type(value) is not int or not 0 <= value <= row["decisions"]:
                raise ValueError(f"invalid {field}: {key}")
        simulations = row.get("mcts_simulations_completed")
        if type(simulations) is not int or simulations < 0:
            raise ValueError(f"invalid mcts simulations: {key}")
        if type(row.get("focus_decisions")) is not int or not 0 <= row["focus_decisions"] <= row["decisions"]:
            raise ValueError(f"invalid focus decisions: {key}")
        for numeric in ("focus_max_decision_ms", "focus_p95_decision_ms", "focus_p99_decision_ms"):
            value = row.get(numeric)
            if not finite_number(value) or value < 0:
                raise ValueError(f"invalid timing {numeric}: {key}")
        if not (
            row["focus_p95_decision_ms"] <= row["focus_p99_decision_ms"] + 1e-9
            and row["focus_p99_decision_ms"] <= row["focus_max_decision_ms"] + 1e-9
        ):
            raise ValueError(f"focus decision percentiles out of order: {key}")
        if (
            type(row.get("focus_timeout_count")) is not int
            or not 0 <= row["focus_timeout_count"] <= row["focus_decisions"]
        ):
            raise ValueError(f"invalid focus_timeout_count: {key}")
        if (
            type(row.get("mcts_deadline_active")) is not bool
            or type(row.get("deterministic_policy_selection")) is not bool
            or row["deterministic_policy_selection"] != (not row["mcts_deadline_active"])
        ):
            raise ValueError(f"invalid policy mode flags: {key}")
        if row["mcts_deadline_active"] != (
            row.get("focus_policy") in search_policies and row["decision_budget_ms"] > 0
        ):
            raise ValueError(f"deadline flag does not match policy/budget: {key}")
        received[key] = row
    if set(received) != expected:
        raise ValueError(f"missing {len(expected - set(received))} game records")
    budgets = {row["decision_budget_ms"] for row in rows}
    if len(budgets) != 1:
        raise ValueError("inconsistent decision budgets across game records")
    values = defaultdict(list)
    deltas = []
    for block in range(first_block, first_block + blocks):
        arm_scores = {}
        for arm in ("candidate", "baseline"):
            games = [received[(block, arm, seat)] for seat in range(players)]
            arm_scores[arm] = statistics.mean(g["first_place_credit"] for g in games)
            values[arm].extend(games)
        deltas.append(arm_scores["candidate"] - arm_scores["baseline"])
    point = statistics.mean(deltas)
    generator = random.Random(20261106)
    boots = sorted(
        sum(deltas[generator.randrange(blocks)] for _ in range(blocks)) / blocks
        for _ in range(iterations)
    )
    lower = boots[int((iterations - 1) * .025)]
    upper = boots[int((iterations - 1) * .975)]
    sample_sd = statistics.stdev(deltas) if blocks > 1 else None
    needed = (
        math.ceil(((1.9599639845 + .8416212336) * sample_sd / .05) ** 2)
        if sample_sd is not None and sample_sd > 0 else None
    )
    max_selection = max(g["max_policy_select_ms"] for g in rows)
    candidate_decisions = sum(g["decisions"] for g in values["candidate"])
    baseline_decisions = sum(g["decisions"] for g in values["baseline"])
    candidate_focus_decisions = sum(g["focus_decisions"] for g in values["candidate"])
    baseline_focus_decisions = sum(g["focus_decisions"] for g in values["baseline"])
    if candidate_focus_decisions == 0 or baseline_focus_decisions == 0:
        raise ValueError("focus decisions must be positive to form rates")
    candidate_focus_timeouts = sum(g["focus_timeout_count"] for g in values["candidate"])
    baseline_focus_timeouts = sum(g["focus_timeout_count"] for g in values["baseline"])
    budget_ms = rows[0]["decision_budget_ms"]
    budget_enabled = budget_ms > 0
    budget_within_10s = 0 < budget_ms <= 10000
    candidate_max_focus = max(g["focus_max_decision_ms"] for g in values["candidate"])
    baseline_max_focus = max(g["focus_max_decision_ms"] for g in values["baseline"])
    candidate_deadline_control = candidate in search_policies and budget_enabled
    baseline_deadline_control = baseline in search_policies and budget_enabled
    candidate_measured_within = budget_enabled and candidate_max_focus <= budget_ms
    baseline_measured_within = budget_enabled and baseline_max_focus <= budget_ms
    candidate_10s_observed_pass = (
        candidate_deadline_control and budget_within_10s and candidate_measured_within
    )
    baseline_10s_observed_pass = (
        baseline_deadline_control and budget_within_10s and baseline_measured_within
    )
    return {
        "schema_version": 1,
        "mode": "cardless_seat_rotated_candidate_vs_fixed_opponents",
        "players": players,
        "additional_action_space": extra,
        "candidate": candidate,
        "baseline": baseline,
        "opponent": opponent,
        "seed_start": seed_start,
        "first_block": first_block,
        "seed_blocks": blocks,
        "games_per_arm": blocks * players,
        "total_games": len(rows),
        "candidate_first_credit": statistics.mean(
            g["first_place_credit"] for g in values["candidate"]
        ),
        "baseline_first_credit": statistics.mean(
            g["first_place_credit"] for g in values["baseline"]
        ),
        "mean_first_credit_delta": point,
        "paired_block_bootstrap_95ci": [lower, upper],
        "bootstrap_iterations": iterations,
        "bootstrap_seed": 20261106,
        "required_seed_blocks_estimate_5pp_80pct_power": needed,
        "candidate_expected_rank": statistics.mean(
            g["expected_rank_observation"] for g in values["candidate"]
        ),
        "baseline_expected_rank": statistics.mean(
            g["expected_rank_observation"] for g in values["baseline"]
        ),
        "candidate_score_margin": statistics.mean(
            g["score_margin"] for g in values["candidate"]
        ),
        "baseline_score_margin": statistics.mean(
            g["score_margin"] for g in values["baseline"]
        ),
        "maximum_policy_selection_ms": max_selection,
        "first_credit_units": "probability fraction; 1.0 = 100 percentage points (pp)",
        "candidate_first_credit_pct": statistics.mean(
            g["first_place_credit"] for g in values["candidate"]
        ) * 100,
        "baseline_first_credit_pct": statistics.mean(
            g["first_place_credit"] for g in values["baseline"]
        ) * 100,
        "mean_first_credit_delta_pp": point * 100,
        "paired_block_bootstrap_95ci_pp": [lower * 100, upper * 100],
        "decision_budget_ms": rows[0]["decision_budget_ms"],
        "candidate_timeout_rate": sum(g["timeout_count"] for g in values["candidate"]) / candidate_focus_decisions,
        "baseline_timeout_rate": sum(g["timeout_count"] for g in values["baseline"]) / baseline_focus_decisions,
        "candidate_fallback_rate": sum(g["fallback_count"] for g in values["candidate"]) / candidate_focus_decisions,
        "baseline_fallback_rate": sum(g["fallback_count"] for g in values["baseline"]) / baseline_focus_decisions,
        "candidate_measured_timeout_rate": candidate_focus_timeouts / candidate_focus_decisions,
        "baseline_measured_timeout_rate": baseline_focus_timeouts / baseline_focus_decisions,
        "candidate_focus_decisions": candidate_focus_decisions,
        "baseline_focus_decisions": baseline_focus_decisions,
        "all_player_decisions": candidate_decisions + baseline_decisions,
        "candidate_mcts_simulations_completed": sum(g["mcts_simulations_completed"] for g in values["candidate"]),
        "baseline_mcts_simulations_completed": sum(g["mcts_simulations_completed"] for g in values["baseline"]),
        "maximum_p95_policy_select_ms": max(g["p95_policy_select_ms"] for g in rows),
        "maximum_p99_policy_select_ms": max(g["p99_policy_select_ms"] for g in rows),
        "measurement_scope": "focus-seat whole decision: action-space selection plus detail generation; rates use focus-only decisions as denominator",
        "focus_decision_timing_includes_detail": True,
        "maximum_focus_p95_decision_ms": max(g["focus_p95_decision_ms"] for g in rows),
        "maximum_focus_p99_decision_ms": max(g["focus_p99_decision_ms"] for g in rows),
        "candidate_max_focus_decision_ms": candidate_max_focus,
        "baseline_max_focus_decision_ms": baseline_max_focus,
        "candidate_focus_timeout_count": candidate_focus_timeouts,
        "baseline_focus_timeout_count": baseline_focus_timeouts,
        "candidate_focus_deadline_control": candidate_deadline_control,
        "baseline_focus_deadline_control": baseline_deadline_control,
        "candidate_focus_measured_within_budget": candidate_measured_within,
        "baseline_focus_measured_within_budget": baseline_measured_within,
        "candidate_10s_observed_pass": candidate_10s_observed_pass,
        "baseline_10s_observed_pass": baseline_10s_observed_pass,
        "deadline_enforcement": "cooperative_in_policy_checks",
        "first_credit_goal_5pp_point_estimate_met": point >= .05,
        "improvement_ci_lower_above_zero": lower > 0,
        "provisional_goal_and_positive_ci": point >= .05 and lower > 0,
        "caveat": (
            "First-place credit values are probability fractions (0.05 = 5 "
            "percentage points); *_pp fields are percentage points. Focus "
            "decision timing includes action-space selection plus detail "
            "generation for the arm ordered as candidate/baseline. "
            "*_10s_observed_pass is an observed check, not an OS-enforced hard "
            "guarantee: MCTS stops at cooperative deadline checks, so it is true "
            "only when the arm is MCTS, a positive budget <= 10000 ms was "
            "applied, and the measured maximum focus decision stayed within it. "
            "A 0 ms budget means no deadline (deterministic MCTS by simulation "
            "count) and makes all 10s flags false. Official rule fidelity still "
            "requires independent gates."
        ),
    }


def load_records(folder: Path) -> list[dict]:
    rows = []
    paths = sorted(folder.glob("shard-*/matches.jsonl"))
    if not paths:
        raise ValueError("no matches.jsonl shards")
    for path in paths:
        with path.open(encoding="utf-8") as file:
            for line in file:
                if line.strip():
                    rows.append(json.loads(line))
    return rows


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--players", type=int, required=True)
    p.add_argument("--seed-start", type=int, required=True)
    p.add_argument("--first-block", type=int, default=0)
    p.add_argument("--blocks", type=int, required=True)
    p.add_argument("--candidate", choices=("random", "greedy", "eval", "mcts"), required=True)
    p.add_argument("--baseline", choices=("random", "greedy", "eval", "mcts"), required=True)
    p.add_argument("--opponent", choices=("random", "greedy", "eval", "mcts"), required=True)
    p.add_argument("--extra", action="store_true")
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    try:
        report = validate_and_report(
            load_records(args.root), players=args.players,
            seed_start=args.seed_start, first_block=args.first_block,
            blocks=args.blocks, candidate=args.candidate,
            baseline=args.baseline, opponent=args.opponent,
            extra=args.extra,
        )
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
        print(f"paired evaluation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
