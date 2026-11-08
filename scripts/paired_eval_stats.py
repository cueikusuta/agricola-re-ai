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

POLICY_CHOICES = ("random", "greedy", "eval", "eval_veg", "mcts", "puct")
SEARCH_POLICIES = ("mcts", "puct")
# #164: fixed-simulation and fixed-wall-clock searches are different
# experiments. Only these names may appear, and never mixed in one report.
SEARCH_MODES = ("simulation-capped", "time-budgeted")


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
    cards: str = "none",
    search_mode: str = "simulation-capped",
    iterations: int = 10000,
) -> dict:
    if players not in (2, 3, 4) or blocks < 1 or iterations < 100:
        raise ValueError("invalid experimental settings")
    if candidate == baseline or any(
        name not in POLICY_CHOICES
        for name in (candidate, baseline, opponent)
    ):
        raise ValueError("unsupported/identical policy")
    # The card condition is a separate experiment, never pooled with cardless
    # games. `verified_basic_2p` is the only deck this harness can build, and
    # it exists for two players only.
    if cards not in ("none", "verified_basic_2p"):
        raise ValueError("unsupported card condition")
    if cards != "none" and players != 2:
        raise ValueError("the verified card pool supports two players only")
    if search_mode not in SEARCH_MODES:
        raise ValueError("unsupported search mode")
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
            or row.get("cards") != cards
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
        # Every game in one experiment must provably use the same deck, and a
        # cardless experiment must report an empty pool rather than a hash of
        # unknown provenance.
        card_count = row.get("card_set_count")
        card_hash = row.get("card_set_hash")
        if (
            type(card_count) is not int
            or card_count < 0
            or not isinstance(card_hash, str)
            or len(card_hash) != 64
            or any(char not in "0123456789abcdef" for char in card_hash)
        ):
            raise ValueError(f"invalid card set metadata: {key}")
        if (cards == "none") != (card_count == 0):
            raise ValueError(f"card condition does not match the deck size: {key}")
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
        # A finished 14-round game always took measurable time; a zero here is a
        # placeholder, not a measurement, and would make throughput divide by zero.
        if not row["total_wall_ms"] > 0:
            raise ValueError(f"non-positive game wall time: {key}")
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
        # #164: the search mode is explicit, and the deadline flag must follow it
        # rather than merely "a budget was configured".
        if row.get("search_mode") not in SEARCH_MODES:
            raise ValueError(f"invalid search mode: {key}")
        time_budgeted = row["search_mode"] == "time-budgeted"
        if (
            type(row.get("mcts_deadline_active")) is not bool
            or type(row.get("deterministic_policy_selection")) is not bool
            or type(row.get("search_budget_enforced")) is not bool
            or row["deterministic_policy_selection"] != (not row["mcts_deadline_active"])
            or row["search_budget_enforced"] != time_budgeted
        ):
            raise ValueError(f"invalid policy mode flags: {key}")
        if row["mcts_deadline_active"] != (
            row.get("focus_policy") in SEARCH_POLICIES and time_budgeted
        ):
            raise ValueError(f"deadline flag does not match policy/search mode: {key}")
        # A simulation-capped run applies no deadline, so a "10 s" claim from it
        # is meaningless; a time-budgeted run must have a positive budget.
        if time_budgeted and row["decision_budget_ms"] <= 0:
            raise ValueError(f"time-budgeted run without a budget: {key}")
        for numeric in ("max_deadline_overshoot_ms", "max_search_elapsed_ms"):
            value = row.get(numeric)
            if not finite_number(value) or value < 0:
                raise ValueError(f"invalid {numeric}: {key}")
        for numeric in ("mcts_simulations_cap", "time_budget_max_simulations",
                        "mcts_simulations_per_decision"):
            value = row.get(numeric)
            if not finite_number(value) or value < 0:
                raise ValueError(f"invalid {numeric}: {key}")
        # The search cap decides how much searching a time-budgeted arm may do,
        # so it is an experiment condition, not a result (#169 re-review). It is
        # reported so a caller can compare it with the plan and with other arms.
        if (
            type(row.get("time_budget_max_simulations")) is not int
            or row["time_budget_max_simulations"] < 1
        ):
            raise ValueError(f"invalid time_budget_max_simulations: {key}")

        # #167 A: the budget covers the whole decision, so rows must say whether
        # that was enforced and how the detail half behaved. A record that claims
        # whole-decision enforcement in simulation-capped mode (or the reverse)
        # cannot be pooled with the rest.
        if type(row.get("whole_decision_budget_enforced")) is not bool:
            raise ValueError(f"invalid whole_decision_budget_enforced: {key}")
        reserve = row.get("detail_reserve_ms")
        if type(reserve) is not int or reserve < 0:
            raise ValueError(f"invalid detail_reserve_ms: {key}")
        # Mode-relative checks only apply to rows that actually claim the mode
        # under test; a row claiming another mode is rejected by the
        # cross-record mode check with its own error.
        if row.get("search_mode") == search_mode:
            if row["whole_decision_budget_enforced"] != (search_mode == "time-budgeted"):
                raise ValueError(
                    f"whole-decision budget flag does not match the search mode: {key}"
                )
            if search_mode == "time-budgeted":
                if not 0 < reserve < row["decision_budget_ms"]:
                    raise ValueError(
                        f"time-budgeted rows need a detail reserve inside the budget: {key}"
                    )
            elif reserve != 0:
                raise ValueError(
                    f"simulation-capped rows apply no budget, so no reserve: {key}"
                )
        for numeric in ("focus_max_detail_ms", "focus_p95_detail_ms"):
            value = row.get(numeric)
            if not finite_number(value) or value < 0:
                raise ValueError(f"invalid timing {numeric}: {key}")
        if row["focus_p95_detail_ms"] > row["focus_max_detail_ms"] + 1e-9:
            raise ValueError(f"detail percentiles out of order: {key}")
        if row["focus_max_detail_ms"] > row["focus_max_decision_ms"] + 1e-6:
            raise ValueError(
                f"detail time cannot exceed the whole decision time: {key}"
            )
        for field in (
            "focus_late_decision_count",
            "focus_decision_fallback_count",
            "focus_selection_fallback_count",
            "focus_detail_fallback_count",
            "focus_budget_exhausted_before_details_count",
        ):
            value = row.get(field)
            if type(value) is not int or not 0 <= value <= row["focus_decisions"]:
                raise ValueError(f"invalid {field}: {key}")
        if row["focus_selection_fallback_count"] + row["focus_detail_fallback_count"] != (
            row["focus_decision_fallback_count"]
        ):
            raise ValueError(f"fallback counts do not add up: {key}")
        if (
            type(row.get("focus_max_decision_overshoot_ms")) not in (int, float)
            or row.get("focus_max_decision_overshoot_ms") < 0
        ):
            raise ValueError(f"invalid focus_max_decision_overshoot_ms: {key}")
        if row["focus_late_decision_count"] == 0 and row[
            "focus_max_decision_overshoot_ms"
        ] > 0:
            raise ValueError(f"overshoot without a late decision: {key}")
        # The two "observed within" booleans are a convenience copy of the raw
        # counters. They are type-checked here and *recomputed* for the report
        # from `focus_late_decision_count` / `focus_max_decision_ms`, so a row
        # that claims compliance its own timings contradict cannot pass it on.
        for flag in (
            "focus_observed_all_decisions_within_budget",
            "focus_observed_all_decisions_within_budget_tolerance",
            "focus_observed_all_decisions_within_10s",
        ):
            if type(row.get(flag)) is not bool:
                raise ValueError(f"invalid {flag}: {key}")
        if (
            type(row.get("focus_detail_declined_count")) is not int
            or not 0
            <= row["focus_detail_declined_count"]
            <= row["focus_detail_fallback_count"]
        ):
            raise ValueError(f"invalid focus_detail_declined_count: {key}")
        if (
            type(row.get("focus_late_beyond_tolerance_count")) is not int
            or not 0
            <= row["focus_late_beyond_tolerance_count"]
            <= row["focus_late_decision_count"]
        ):
            raise ValueError(f"invalid focus_late_beyond_tolerance_count: {key}")
        tolerance = row.get("focus_budget_tolerance_ms")
        if not finite_number(tolerance) or tolerance < 0:
            raise ValueError(f"invalid focus_budget_tolerance_ms: {key}")
        if row["focus_late_beyond_tolerance_count"] > 0 and (
            row["focus_max_decision_overshoot_ms"] <= tolerance + 1e-9
        ):
            raise ValueError(
                f"an over-tolerance decision needs a larger overshoot: {key}"
            )
        received[key] = row
    if set(received) != expected:
        raise ValueError(f"missing {len(expected - set(received))} game records")
    search_modes = {row["search_mode"] for row in rows}
    if len(search_modes) != 1:
        raise ValueError("inconsistent search modes across game records")
    if next(iter(search_modes)) != search_mode:
        raise ValueError(
            f"game records are {next(iter(search_modes))!r} but the report was "
            f"asked for {search_mode!r}"
        )
    budgets = {row["decision_budget_ms"] for row in rows}
    if len(budgets) != 1:
        raise ValueError("inconsistent decision budgets across game records")
    caps = {row["time_budget_max_simulations"] for row in rows}
    if len(caps) != 1:
        raise ValueError("inconsistent time_budget_max_simulations across game records")
    card_hashes = {(row["cards"], row["card_set_count"], row["card_set_hash"]) for row in rows}
    if len(card_hashes) != 1:
        raise ValueError("inconsistent card set across game records")
    card_set_count, card_set_hash = next(iter(card_hashes))[1:]
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
    # Two different questions, deliberately kept apart (#164, #167 A review P1).
    #
    # 1. `*_focus_search_deadline_control`: did the *search* run under a
    #    deadline? Only a search policy in time-budgeted mode can answer yes.
    # 2. `*_whole_decision_budget_enforced`: did the *decision* (selection plus
    #    details) run under one clock? Since #167 A the budget applies to the
    #    focus seat whatever its policy, so this is not restricted to MCTS/PUCT.
    #
    # The 10 s observation follows (2), not (1). The previous version keyed it to
    # (1), which both under-reported budgeted heuristic seats and contradicted
    # its own explanation that named only MCTS.
    candidate_search_deadline = (
        candidate in SEARCH_POLICIES and search_mode == "time-budgeted")
    baseline_search_deadline = (
        baseline in SEARCH_POLICIES and search_mode == "time-budgeted")
    budget_enforced = rows[0]["whole_decision_budget_enforced"]
    candidate_budget_enforced = budget_enforced and candidate_focus_decisions > 0
    baseline_budget_enforced = budget_enforced and baseline_focus_decisions > 0
    # A cooperative deadline can only be observed between simulations, so
    # "within budget" uses the same tolerance the rows record (1 % of the
    # budget, 1 ms floor). The strict overshoot stays in the report.
    tolerance_ms = max(g["focus_budget_tolerance_ms"] for g in rows)
    candidate_measured_within = (
        budget_enabled and candidate_max_focus <= budget_ms + tolerance_ms)
    baseline_measured_within = (
        budget_enabled and baseline_max_focus <= budget_ms + tolerance_ms)
    candidate_10s_observed_pass = (
        candidate_budget_enforced and budget_within_10s and candidate_measured_within
    )
    baseline_10s_observed_pass = (
        baseline_budget_enforced and budget_within_10s and baseline_measured_within
    )
    # Backwards-compatible aliases: the old names described the search deadline.
    candidate_deadline_control = candidate_search_deadline
    baseline_deadline_control = baseline_search_deadline

    # Roadmap 3.1 asks for shared/tied first place as separate reference
    # values, and 6.6 asks for games/sec and decisions/sec. All of these are
    # derived from already-validated fields; nothing is re-sampled.
    def first_place_breakdown(games: list[dict]) -> dict:
        solo = [
            1.0 if g["expected_rank_observation"] == 1 and g["ranks_by_seat"].count(1) == 1 else 0.0
            for g in games
        ]
        reach = [1.0 if g["expected_rank_observation"] == 1 else 0.0 for g in games]
        tied = [1.0 if g["ranks_by_seat"].count(1) > 1 else 0.0 for g in games]
        return {
            "solo_first_rate": statistics.mean(solo),
            "first_reach_rate": statistics.mean(reach),
            "tied_first_game_rate": statistics.mean(tied),
        }

    candidate_breakdown = first_place_breakdown(values["candidate"])
    baseline_breakdown = first_place_breakdown(values["baseline"])
    by_seat = {
        arm: [
            statistics.mean(
                received[(block, arm, seat)]["first_place_credit"]
                for block in range(first_block, first_block + blocks)
            )
            for seat in range(players)
        ]
        for arm in ("candidate", "baseline")
    }
    # The denominator is summed per-game elapsed wall time (not measured OS
    # CPU time), appropriate for the serial-run throughput approximation.
    # Keep the historical `total_cpu_seconds` key for report compatibility.
    total_cpu_seconds = sum(g["total_wall_ms"] for g in rows) / 1000.0
    return {
        "schema_version": 1,
        "mode": "cardless_seat_rotated_candidate_vs_fixed_opponents"
        if cards == "none"
        else "verified_card_seat_rotated_candidate_vs_fixed_opponents",
        "players": players,
        "cards": cards,
        "card_set_count": card_set_count,
        "card_set_hash": card_set_hash,
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
        "candidate_solo_first_rate": candidate_breakdown["solo_first_rate"],
        "baseline_solo_first_rate": baseline_breakdown["solo_first_rate"],
        "candidate_first_reach_rate": candidate_breakdown["first_reach_rate"],
        "baseline_first_reach_rate": baseline_breakdown["first_reach_rate"],
        "candidate_tied_first_game_rate": candidate_breakdown["tied_first_game_rate"],
        "baseline_tied_first_game_rate": baseline_breakdown["tied_first_game_rate"],
        "candidate_first_credit_by_seat": by_seat["candidate"],
        "baseline_first_credit_by_seat": by_seat["baseline"],
        "candidate_seat_credit_spread": max(by_seat["candidate"]) - min(by_seat["candidate"]),
        "baseline_seat_credit_spread": max(by_seat["baseline"]) - min(by_seat["baseline"]),
        "total_cpu_seconds": total_cpu_seconds,
        "games_per_second": len(rows) / total_cpu_seconds,
        "decisions_per_second": (candidate_decisions + baseline_decisions) / total_cpu_seconds,
        "throughput_note": (
            "games_per_second and decisions_per_second use the summed per-game "
            "wall time of this shard set as the CPU denominator. They are single-"
            "process throughput measurements, not parallel speedup or latency."
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
        "time_budget_max_simulations": rows[0]["time_budget_max_simulations"],
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
        "whole_decision_budget_enforced": rows[0]["whole_decision_budget_enforced"],
        "detail_reserve_ms": rows[0]["detail_reserve_ms"],
        "candidate_max_detail_ms": max(
            g["focus_max_detail_ms"] for g in values["candidate"]
        ),
        "baseline_max_detail_ms": max(
            g["focus_max_detail_ms"] for g in values["baseline"]
        ),
        "candidate_p95_detail_ms": max(
            g["focus_p95_detail_ms"] for g in values["candidate"]
        ),
        "candidate_late_decision_rate": sum(
            g["focus_late_decision_count"] for g in values["candidate"]
        ) / candidate_focus_decisions,
        "baseline_late_decision_rate": sum(
            g["focus_late_decision_count"] for g in values["baseline"]
        ) / baseline_focus_decisions,
        "candidate_decision_fallback_rate": sum(
            g["focus_decision_fallback_count"] for g in values["candidate"]
        ) / candidate_focus_decisions,
        "baseline_decision_fallback_rate": sum(
            g["focus_decision_fallback_count"] for g in values["baseline"]
        ) / baseline_focus_decisions,
        "candidate_max_decision_overshoot_ms": max(
            g["focus_max_decision_overshoot_ms"] for g in values["candidate"]
        ),
        "baseline_max_decision_overshoot_ms": max(
            g["focus_max_decision_overshoot_ms"] for g in values["baseline"]
        ),
        # Recomputed from raw counters, never copied from the row flags: the
        # artifact cannot certify a budget it did not measure.
        "candidate_observed_all_decisions_within_budget": (
            rows[0]["whole_decision_budget_enforced"]
            and candidate_focus_decisions > 0
            and sum(
                g["focus_late_decision_count"] for g in values["candidate"]
            ) == 0
        ),
        "candidate_observed_all_decisions_within_10s": (
            candidate_max_focus <= 10000.0
        ),
        "candidate_observed_all_decisions_within_budget_tolerance": (
            rows[0]["whole_decision_budget_enforced"]
            and candidate_focus_decisions > 0
            and sum(
                g["focus_late_beyond_tolerance_count"] for g in values["candidate"]
            ) == 0
        ),
        "candidate_late_decision_count": sum(
            g["focus_late_decision_count"] for g in values["candidate"]
        ),
        "candidate_late_beyond_tolerance_count": sum(
            g["focus_late_beyond_tolerance_count"] for g in values["candidate"]
        ),
        "budget_tolerance_ms": max(
            g["focus_budget_tolerance_ms"] for g in rows
        ),
        "baseline_late_decision_count": sum(
            g["focus_late_decision_count"] for g in values["baseline"]
        ),
        "candidate_detail_fallback_count": sum(
            g["focus_detail_fallback_count"] for g in values["candidate"]
        ),
        "candidate_detail_declined_count": sum(
            g["focus_detail_declined_count"] for g in values["candidate"]
        ),
        "candidate_budget_exhausted_before_details_count": sum(
            g["focus_budget_exhausted_before_details_count"] for g in values["candidate"]
        ),
        "baseline_detail_fallback_count": sum(
            g["focus_detail_fallback_count"] for g in values["baseline"]
        ),
        "whole_decision_budget_note": (
            "whole_decision_budget_enforced=true means the reported budget bounds "
            "selection and detail generation together: the search deadline is the "
            "budget minus detail_reserve_ms. observed_all_decisions_within_* are "
            "measurements of the recorded decisions, not an OS real-time guarantee, "
            "and they are false in simulation-capped mode because no clock applied."
        ),
        "maximum_focus_p95_decision_ms": max(g["focus_p95_decision_ms"] for g in rows),
        "maximum_focus_p99_decision_ms": max(g["focus_p99_decision_ms"] for g in rows),
        "candidate_max_focus_decision_ms": candidate_max_focus,
        "baseline_max_focus_decision_ms": baseline_max_focus,
        "candidate_focus_timeout_count": candidate_focus_timeouts,
        "baseline_focus_timeout_count": baseline_focus_timeouts,
        "candidate_focus_deadline_control": candidate_deadline_control,
        "baseline_focus_deadline_control": baseline_deadline_control,
        # #167 A review P1: the search deadline and the whole-decision budget are
        # different questions and are reported separately.
        "candidate_focus_search_deadline_control": candidate_search_deadline,
        "baseline_focus_search_deadline_control": baseline_search_deadline,
        "candidate_whole_decision_budget_enforced": candidate_budget_enforced,
        "baseline_whole_decision_budget_enforced": baseline_budget_enforced,
        "candidate_10s_observed_pass_basis": (
            "whole-decision budget (selection + details) enforced for the focus "
            "seat, a positive budget <= 10000 ms, and the measured maximum focus "
            "decision within budget + tolerance"
        ),
        "candidate_focus_measured_within_budget": candidate_measured_within,
        "baseline_focus_measured_within_budget": baseline_measured_within,
        "candidate_10s_observed_pass": candidate_10s_observed_pass,
        "baseline_10s_observed_pass": baseline_10s_observed_pass,
        "search_mode": search_mode,
        "search_budget_enforced": search_mode == "time-budgeted",
        "candidate_max_deadline_overshoot_ms": max(
            g["max_deadline_overshoot_ms"] for g in values["candidate"]
        ),
        "baseline_max_deadline_overshoot_ms": max(
            g["max_deadline_overshoot_ms"] for g in values["baseline"]
        ),
        "candidate_simulations_per_decision": statistics.mean(
            g["mcts_simulations_per_decision"] for g in values["candidate"]
        ),
        "baseline_simulations_per_decision": statistics.mean(
            g["mcts_simulations_per_decision"] for g in values["baseline"]
        ),
        "timeout_rate_semantics": (
            "timed_out means the wall-clock deadline ended that decision. In "
            "time-budgeted mode that is the normal case and therefore close to "
            "1.0; in simulation-capped mode no deadline is applied, so the rate "
            "must be 0.0."
        ),
        "search_mode_note": (
            "simulation-capped and time-budgeted are different experiments and "
            "must not be pooled: the first caps the number of simulation attempts and is "
            "reproducible, the second fixes wall-clock cost and is not. "
            "deterministic_policy_selection is true only when no deadline was "
            "enforced for the focus seat."
        ),
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
            "guarantee. It is true only when the whole-decision budget was "
            "enforced for the focus seat (time-budgeted mode; selection *and* "
            "detail generation under one clock, whatever the focus policy), a "
            "positive budget <= 10000 ms was applied, and the measured maximum "
            "focus decision stayed within budget + tolerance. It is not "
            "restricted to MCTS/PUCT: since #167 A a budgeted heuristic seat is "
            "also bounded and therefore also observable. *"
            "focus_search_deadline_control is the narrower question 'did the "
            "search itself run under a deadline'. A 0 ms budget means no deadline "
            "(deterministic by simulation count) and makes every 10 s flag false. "
            "Official rule fidelity still requires independent gates."
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
    p.add_argument("--candidate", choices=POLICY_CHOICES, required=True)
    p.add_argument("--baseline", choices=POLICY_CHOICES, required=True)
    p.add_argument("--opponent", choices=POLICY_CHOICES, required=True)
    p.add_argument("--extra", action="store_true")
    p.add_argument("--cards", choices=("none", "verified_basic_2p"), default="none")
    p.add_argument("--search-mode", choices=SEARCH_MODES, default="simulation-capped")
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    try:
        report = validate_and_report(
            load_records(args.root), players=args.players,
            seed_start=args.seed_start, first_block=args.first_block,
            blocks=args.blocks, candidate=args.candidate,
            baseline=args.baseline, opponent=args.opponent,
            extra=args.extra, cards=args.cards, search_mode=args.search_mode,
        )
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
        print(f"paired evaluation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
