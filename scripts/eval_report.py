#!/usr/bin/env python3
"""Aggregate a design-aligned `eval_harness` run (PR #400 design §3, §5.4).

Input is one run directory written by `bin/eval_harness.rs`:
`run-manifest.json` plus `shard-*/matches.jsonl` (row schema 2).

The report is fail closed about the experiment identity: rows that disagree
with the manifest, miss a block/arm/seat, duplicate a key, or carry a different
condition are rejected instead of being averaged.

Independent sampling unit is the seed block, exactly as the design says: every
block contributes one paired difference per metric (candidate arm mean over
seats minus baseline arm mean over seats). A 95% percentile bootstrap over
blocks (10,000 iterations, seed 20261106) is the provisional interval. Score
margin is a reward/diagnostic, never the adoption yardstick.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import random
import statistics
import sys

SCHEMA_VERSION = 2
REPORT_SCHEMA_VERSION = 1
BOOTSTRAP_ITERATIONS = 10_000
BOOTSTRAP_SEED = 20261106
SATURATION_CEILING = 0.95
REQUIRED_BLOCKS_TARGET = 0.05
# z(0.975) + z(0.80), from the design's required-block formula.
Z_SUM = 1.9599639845 + 0.8416212336
HEX = set("0123456789abcdef")


class RunError(ValueError):
    """A run directory that cannot be aggregated without guessing."""


def finite_number(value: object) -> bool:
    if type(value) is int:
        return abs(value) <= 9007199254740992
    return type(value) is float and math.isfinite(value)


def is_hash(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(char in HEX for char in value)
    )


def percentile(values: list[float], q: float) -> float:
    """Nearest-rank percentile on an already collected sample (no resampling)."""
    if not values:
        raise RunError("cannot take a percentile of an empty sample")
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, math.ceil(q * len(ordered)) - 1))
    return ordered[index]


def bootstrap_ci(deltas: list[float], iterations: int) -> tuple[float, float, float]:
    """Paired block bootstrap. A fresh seeded generator keeps every metric's
    interval reproducible and matches the recorded bootstrap convention."""
    if not deltas:
        raise RunError("no seed blocks to resample")
    point = statistics.mean(deltas)
    generator = random.Random(BOOTSTRAP_SEED)
    n = len(deltas)
    boots = sorted(
        sum(deltas[generator.randrange(n)] for _ in range(n)) / n
        for _ in range(iterations)
    )
    lower = boots[int((iterations - 1) * 0.025)]
    upper = boots[int((iterations - 1) * 0.975)]
    return point, lower, upper


def required_blocks(sample_sd: float | None) -> int | None:
    if sample_sd is None or sample_sd <= 0:
        return None
    return math.ceil((Z_SUM * sample_sd / REQUIRED_BLOCKS_TARGET) ** 2)


def load_run(root: Path) -> tuple[dict, list[dict]]:
    manifest_path = root / "run-manifest.json"
    if not manifest_path.is_file():
        raise RunError(f"missing run-manifest.json under {root}")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise RunError(f"run-manifest.json is not valid JSON: {error}") from error
    if not isinstance(manifest, dict):
        raise RunError("run-manifest.json must be an object")
    paths = sorted(root.glob("shard-*/matches.jsonl"))
    if not paths:
        raise RunError("no shard-*/matches.jsonl files")
    rows: list[dict] = []
    for path in paths:
        with path.open(encoding="utf-8") as file:
            for line_number, line in enumerate(file, start=1):
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError as error:
                    raise RunError(f"{path}:{line_number}: invalid JSON: {error}") from error
                if not isinstance(row, dict):
                    raise RunError(f"{path}:{line_number}: row must be an object")
                rows.append(row)
    return manifest, rows


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise RunError(message)


def _manifest_shape(manifest: dict) -> None:
    _require(manifest.get("schema_version") == 1, "unsupported manifest schema_version")
    _require(manifest.get("harness") == "eval_harness", "manifest is not from eval_harness")
    artifact = manifest.get("artifact")
    _require(isinstance(artifact, dict), "manifest.artifact is missing")
    _require(
        artifact.get("row_schema_version") == SCHEMA_VERSION,
        "manifest does not declare row schema 2",
    )
    for field in ("players", "blocks"):
        _require(type(manifest.get(field)) is int, f"manifest.{field} must be an integer")
    _require(2 <= manifest["players"] <= 4, "manifest.players must be 2..4")
    _require(manifest["blocks"] >= 1, "manifest.blocks must be positive")
    _require(is_hash(manifest.get("condition_id")), "manifest.condition_id must be sha256")
    _require(is_hash(manifest.get("card_set_hash")), "manifest.card_set_hash must be sha256")
    _require(type(manifest.get("card_set_count")) is int, "manifest.card_set_count must be int")
    _require(
        manifest.get("run_mode") in ("smoke", "full"),
        "manifest.run_mode must be smoke or full",
    )
    _require(
        manifest.get("strength_claim_allowed") is False,
        "unregistered runs must not allow strength claims (design §3.3)",
    )
    seeds = manifest.get("seeds")
    _require(isinstance(seeds, dict), "manifest.seeds is missing")
    for field in ("seed_start", "draft_seed_start", "first_block", "blocks"):
        _require(type(seeds.get(field)) is int, f"manifest.seeds.{field} must be int")
    _require(
        seeds["blocks"] == manifest["blocks"],
        "manifest.blocks disagrees with manifest.seeds.blocks",
    )
    arms = manifest.get("arms")
    _require(isinstance(arms, dict), "manifest.arms is missing")
    for arm in ("candidate", "baseline", "opponent"):
        _require(isinstance(arms.get(arm), dict), f"manifest.arms.{arm} is missing")
        _require(
            isinstance(arms[arm].get("policy"), str),
            f"manifest.arms.{arm}.policy is missing",
        )
        _require(
            isinstance(arms[arm].get("game_policy_id"), str),
            f"manifest.arms.{arm}.game_policy_id is missing",
        )
    draft = manifest.get("draft")
    _require(isinstance(draft, dict), "manifest.draft is missing")
    _require(isinstance(draft.get("mode"), str), "manifest.draft.mode is missing")
    _require(
        draft["mode"] in ("none", "draft_10_7_simultaneous"),
        "unsupported draft mode",
    )
    if draft["mode"] != "none":
        for arm in ("candidate", "baseline"):
            _require(
                isinstance(arms[arm].get("draft_policy_id"), str),
                f"manifest.arms.{arm}.draft_policy_id is required for a draft run",
            )
    compute = manifest.get("compute_budget")
    _require(isinstance(compute, dict), "manifest.compute_budget is missing")
    _require(
        compute.get("search_mode") in ("simulation-capped", "time-budgeted"),
        "unsupported search mode",
    )


def _validate_row(
    row: dict,
    manifest: dict,
    *,
    block: int,
    arm: str,
    seat: int,
) -> None:
    players = manifest["players"]
    seeds = manifest["seeds"]
    cards = manifest["cards"]
    draft_mode = manifest["draft"]["mode"]
    label = f"block {block} arm {arm} seat {seat}"
    _require(row.get("schema_version") == SCHEMA_VERSION, f"{label}: row schema is not 2")
    _require(row.get("condition_id") == manifest["condition_id"], f"{label}: condition mismatch")
    _require(row.get("block_index") == block, f"{label}: block index mismatch")
    _require(row.get("arm_id") == arm, f"{label}: arm mismatch")
    _require(row.get("candidate_seat") == seat, f"{label}: seat mismatch")
    _require(row.get("players") == players, f"{label}: players mismatch")
    _require(
        row.get("seed") == seeds["seed_start"] + block,
        f"{label}: seed does not follow seed_start + block",
    )
    if draft_mode == "none":
        _require(row.get("draft_seed") is None, f"{label}: draft_seed must be null")
    else:
        _require(
            row.get("draft_seed") == seeds["draft_seed_start"] + block,
            f"{label}: draft seed does not follow draft_seed_start + block",
        )
    for field in ("additional_action_space",):
        _require(
            row.get(field) == manifest[field],
            f"{label}: {field} disagrees with the manifest",
        )
    for field in ("cards", "card_set_count", "card_set_hash"):
        _require(
            row.get(field) == manifest[field],
            f"{label}: {field} disagrees with the manifest",
        )
    arms = manifest["arms"]
    _require(row.get("focus_policy") == arms[arm]["policy"], f"{label}: focus policy mismatch")
    _require(
        row.get("opponent_policy") == arms["opponent"]["policy"],
        f"{label}: opponent policy mismatch",
    )
    _require(row.get("finished") is True, f"{label}: game did not finish")
    _require(row.get("rounds_completed") == 14, f"{label}: not a 14-round game")
    _require(row.get("replay_verified") is True, f"{label}: replay was not verified")
    _require(is_hash(row.get("replay_hash")), f"{label}: invalid replay hash")
    _require(row.get("policy_select_only") is True, f"{label}: unexpected action layer")

    policy_by_seat = row.get("game_policy_id_by_seat")
    _require(
        isinstance(policy_by_seat, list) and len(policy_by_seat) == players,
        f"{label}: game_policy_id_by_seat must cover every seat",
    )
    for other, policy in enumerate(policy_by_seat):
        expected = (
            arms[arm]["game_policy_id"] if other == seat else arms["opponent"]["game_policy_id"]
        )
        _require(
            policy == expected,
            f"{label}: game policy of seat {other} is {policy!r}, expected {expected!r}",
        )

    scores = row.get("scores_by_seat")
    ranks = row.get("ranks_by_seat")
    _require(
        isinstance(scores, list)
        and isinstance(ranks, list)
        and len(scores) == players
        and len(ranks) == players,
        f"{label}: standings must cover every seat",
    )
    _require(all(type(score) is int for score in scores), f"{label}: scores must be integers")
    _require(
        all(type(rank) is int and 1 <= rank <= players for rank in ranks),
        f"{label}: ranks must be 1..players",
    )
    for i in range(players):
        for j in range(players):
            _require(
                not (scores[i] > scores[j] and ranks[i] >= ranks[j]),
                f"{label}: score/rank order is impossible",
            )
    for score in set(scores):
        group = [ranks[i] for i in range(players) if scores[i] == score]
        next_rank = 1 + sum(other > score for other in scores)
        for rank in sorted(set(group)):
            _require(
                rank == next_rank,
                f"{label}: competition rank sequence is impossible",
            )
            next_rank += group.count(rank)
    firsts = ranks.count(1)
    _require(firsts >= 1, f"{label}: no first place")
    expected_credit = 1.0 / firsts if ranks[seat] == 1 else 0.0
    credit = row.get("first_place_credit")
    _require(finite_number(credit), f"{label}: first_place_credit must be finite")
    _require(abs(credit - expected_credit) <= 1e-9, f"{label}: first-place credit mismatch")
    _require(
        row.get("expected_rank_observation") == ranks[seat],
        f"{label}: expected rank mismatch",
    )
    mean_others = (sum(scores) - scores[seat]) / (players - 1)
    margin = row.get("score_margin")
    _require(finite_number(margin), f"{label}: score_margin must be finite")
    _require(
        abs(margin - (scores[seat] - mean_others)) <= 1e-8,
        f"{label}: score margin mismatch",
    )

    total_wall = row.get("total_wall_ms")
    _require(finite_number(total_wall) and total_wall > 0, f"{label}: invalid game wall time")
    for field in ("focus_max_decision_ms", "focus_p95_decision_ms", "focus_p99_decision_ms"):
        value = row.get(field)
        _require(finite_number(value) and value >= 0, f"{label}: invalid {field}")
    _require(
        row["focus_p95_decision_ms"] <= row["focus_p99_decision_ms"] + 1e-9
        and row["focus_p99_decision_ms"] <= row["focus_max_decision_ms"] + 1e-9,
        f"{label}: focus decision percentiles are out of order",
    )
    _require(
        type(row.get("focus_decisions")) is int and row["focus_decisions"] > 0,
        f"{label}: focus decisions must be positive",
    )
    for field in ("whole_decision_budget_enforced",):
        _require(type(row.get(field)) is bool, f"{label}: {field} must be a boolean")
    _require(
        row.get("search_mode") == manifest["compute_budget"]["search_mode"],
        f"{label}: search mode mismatch",
    )
    _require(
        row.get("decision_budget_ms") == manifest["compute_budget"]["decision_budget_ms"],
        f"{label}: decision budget mismatch",
    )

    _require(row.get("draft_mode") == draft_mode, f"{label}: draft mode mismatch")
    if draft_mode == "none":
        for field in ("draft_replay_hash", "start_player", "final_hands_by_seat"):
            _require(row.get(field) is None, f"{label}: {field} must be null without a draft")
        return
    _require(is_hash(row.get("draft_replay_hash")), f"{label}: invalid draft replay hash")
    start = row.get("start_player")
    _require(type(start) is int and 0 <= start < players, f"{label}: invalid start_player")
    draft_by_seat = row.get("draft_policy_id_by_seat")
    _require(
        isinstance(draft_by_seat, list) and len(draft_by_seat) == players,
        f"{label}: draft_policy_id_by_seat must cover every seat",
    )
    for other, policy in enumerate(draft_by_seat):
        expected = (
            arms[arm]["draft_policy_id"]
            if other == seat
            else arms["baseline"]["draft_policy_id"]
        )
        _require(
            policy == expected,
            f"{label}: draft policy of seat {other} is {policy!r}, expected {expected!r}",
        )
    hands = row.get("final_hands_by_seat")
    _require(
        isinstance(hands, list) and len(hands) == players,
        f"{label}: final hands must cover every seat",
    )
    for hand in hands:
        _require(isinstance(hand, dict), f"{label}: final hand must be an object")
        _require(
            isinstance(hand.get("occupations"), list)
            and len(hand["occupations"]) == 7
            and isinstance(hand.get("minor_improvements"), list)
            and len(hand["minor_improvements"]) == 7,
            f"{label}: final hand is not 7 + 7",
        )
    for field in ("focus_pick_max_ms", "focus_pick_p95_ms"):
        value = row.get(field)
        _require(finite_number(value) and value >= 0, f"{label}: invalid {field}")
    for field in ("pick_timeout_count", "pick_fallback_count"):
        value = row.get(field)
        _require(type(value) is int and value >= 0, f"{label}: invalid {field}")
    _require(
        type(row.get("focus_observed_all_picks_within_budget")) is bool,
        f"{label}: pick-budget flag must be a boolean",
    )


def _uniform(rows: list[dict], field: str) -> object:
    values = {json.dumps(row.get(field), sort_keys=True) for row in rows}
    _require(len(values) == 1, f"game records disagree on {field}")
    return json.loads(next(iter(values)))


def _arm_metrics(games: list[dict], players: int) -> dict:
    credits = [game["first_place_credit"] for game in games]
    ranks = [game["expected_rank_observation"] for game in games]
    margins = [game["score_margin"] for game in games]
    solo = [
        1.0
        if game["expected_rank_observation"] == 1 and game["ranks_by_seat"].count(1) == 1
        else 0.0
        for game in games
    ]
    reach = [1.0 if game["expected_rank_observation"] == 1 else 0.0 for game in games]
    tied = [1.0 if game["ranks_by_seat"].count(1) > 1 else 0.0 for game in games]
    by_seat = [
        statistics.mean(
            game["first_place_credit"]
            for game in games
            if game["candidate_seat"] == seat
        )
        for seat in range(players)
    ]
    lower = math.floor(min(margins) / 5.0) * 5.0
    upper = math.ceil(max(margins) / 5.0) * 5.0
    histogram = []
    for start in range(int(lower), int(upper), 5):
        count = sum(1 for margin in margins if start <= margin < start + 5)
        histogram.append({"lower": start, "upper": start + 5, "count": count})
    return {
        "games": len(games),
        "first_credit": statistics.mean(credits),
        "first_credit_pct": statistics.mean(credits) * 100,
        "solo_first_rate": statistics.mean(solo),
        "first_reach_rate": statistics.mean(reach),
        "tied_first_game_rate": statistics.mean(tied),
        "expected_rank": statistics.mean(ranks),
        "score_margin": {
            "mean": statistics.mean(margins),
            "sd": statistics.stdev(margins) if len(margins) > 1 else None,
            "min": min(margins),
            "max": max(margins),
            "p10": percentile(margins, 0.10),
            "p25": percentile(margins, 0.25),
            "p50": percentile(margins, 0.50),
            "p75": percentile(margins, 0.75),
            "p90": percentile(margins, 0.90),
            "histogram_bin_width": 5,
            "histogram": histogram,
        },
        "first_credit_by_seat": by_seat,
        "first_credit_seat_spread": max(by_seat) - min(by_seat),
        "seat_credit_expected_if_symmetric": 1.0 / players,
    }


def validate_and_report(
    manifest: dict,
    rows: list[dict],
    *,
    iterations: int = BOOTSTRAP_ITERATIONS,
) -> dict:
    if iterations < 100:
        raise RunError("bootstrap iterations must be at least 100")
    _manifest_shape(manifest)
    players = manifest["players"]
    seeds = manifest["seeds"]
    first_block = seeds["first_block"]
    blocks = manifest["blocks"]
    expected = {
        (block, arm, seat)
        for block in range(first_block, first_block + blocks)
        for arm in ("candidate", "baseline")
        for seat in range(players)
    }
    received: dict[tuple[int, str, int], dict] = {}
    for row in rows:
        block = row.get("block_index")
        arm = row.get("arm_id")
        seat = row.get("candidate_seat")
        _require(
            type(block) is int and type(seat) is int and isinstance(arm, str),
            "row identifiers must be block/arm/seat",
        )
        key = (block, arm, seat)
        _require(key in expected, f"unexpected block/arm/seat: {key}")
        _require(key not in received, f"duplicate record: {key}")
        _validate_row(row, manifest, block=block, arm=arm, seat=seat)
        received[key] = row
    missing = sorted(expected - set(received))
    _require(not missing, f"missing game records: {missing[:5]} (and {len(missing)} total)")

    # Condition-uniform fields: a report must never pool them silently.
    for field in (
        "search_mode",
        "decision_budget_ms",
        "detail_reserve_ms",
        "whole_decision_budget_enforced",
        "time_budget_max_simulations",
        "draft_mode",
        "candidate_puct_config_id",
        "baseline_puct_config_id",
        "cards",
        "card_set_hash",
    ):
        _uniform(rows, field)

    values = {"candidate": [], "baseline": []}
    deltas = {"first_credit": [], "expected_rank": [], "score_margin": []}
    for block in range(first_block, first_block + blocks):
        arm_means = {}
        for arm in ("candidate", "baseline"):
            games = [received[(block, arm, seat)] for seat in range(players)]
            values[arm].extend(games)
            arm_means[arm] = {
                "first_credit": statistics.mean(game["first_place_credit"] for game in games),
                "expected_rank": statistics.mean(
                    game["expected_rank_observation"] for game in games
                ),
                "score_margin": statistics.mean(game["score_margin"] for game in games),
            }
        deltas["first_credit"].append(
            arm_means["candidate"]["first_credit"] - arm_means["baseline"]["first_credit"]
        )
        # Negative expected-rank delta means the candidate ranks better.
        deltas["expected_rank"].append(
            arm_means["candidate"]["expected_rank"] - arm_means["baseline"]["expected_rank"]
        )
        deltas["score_margin"].append(
            arm_means["candidate"]["score_margin"] - arm_means["baseline"]["score_margin"]
        )

    intervals = {}
    for metric, samples in deltas.items():
        point, lower, upper = bootstrap_ci(samples, iterations)
        sample_sd = statistics.stdev(samples) if len(samples) > 1 else None
        intervals[metric] = {
            "block_delta_mean": point,
            "paired_block_bootstrap_95ci": [lower, upper],
            "block_delta_sd": sample_sd,
            "required_seed_blocks_estimate_5pp_80pct_power": required_blocks(sample_sd),
            "direction": {
                "first_credit": "positive = candidate wins more first-place credit",
                "expected_rank": "negative = candidate has the better (smaller) rank",
                "score_margin": "positive = candidate has the larger score margin",
            }[metric],
        }

    candidate_metrics = _arm_metrics(values["candidate"], players)
    baseline_metrics = _arm_metrics(values["baseline"], players)
    # A mirror arm in a k-player game should sit near 1/k first-place credit.
    # The report shows the deviation; whether it is a hard failure is a later
    # guard decision (design §3.4 leaves the threshold open).
    mirror_deviation = abs(
        baseline_metrics["first_credit"] - baseline_metrics["seat_credit_expected_if_symmetric"]
    )
    ceiling = max(candidate_metrics["first_credit"], baseline_metrics["first_credit"])
    arms = manifest["arms"]
    candidate_id = arms["candidate"].get("puct_config_id")
    baseline_id = arms["baseline"].get("puct_config_id")
    head_to_head = arms["opponent"]["policy"] == arms["baseline"]["policy"] and (
        arms["candidate"]["policy"] != arms["opponent"]["policy"]
        or (
            candidate_id is not None
            and baseline_id is not None
            and candidate_id != baseline_id
        )
    )
    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "source": {
            "harness": manifest["harness"],
            "experiment_id": manifest.get("experiment_id"),
            "run_id": manifest.get("run_id"),
            "run_mode": manifest["run_mode"],
            "measurement_mode": manifest["run_mode"],
            "strength_claim_allowed": False,
            "code_sha": manifest.get("code_sha"),
            "engine_version": manifest.get("engine_version"),
            "started_at_unix": manifest.get("started_at_unix"),
        },
        "condition": {
            "condition_id": manifest["condition_id"],
            "players": players,
            "cards": manifest["cards"],
            "card_set_count": manifest["card_set_count"],
            "card_set_hash": manifest["card_set_hash"],
            "draft_mode": manifest["draft"]["mode"],
            "additional_action_space": manifest["additional_action_space"],
            "search_mode": manifest["compute_budget"]["search_mode"],
            "decision_budget_ms": manifest["compute_budget"]["decision_budget_ms"],
            "arms": arms,
        },
        "seeds": {
            "seed_start": seeds["seed_start"],
            "draft_seed_start": seeds["draft_seed_start"],
            "first_block": first_block,
            "blocks": blocks,
            "bootstrap": {
                "method": "percentile-block",
                "iterations": iterations,
                "seed": BOOTSTRAP_SEED,
            },
        },
        "completeness": {
            "total_games": len(rows),
            "games_per_arm": blocks * players,
            "missing_records": 0,
            "duplicate_records": 0,
            "all_finished": True,
            "all_replay_verified": True,
        },
        "comparison_design": {
            "head_to_head": head_to_head,
            "mirror_baseline": head_to_head,
            "note": (
                "head_to_head=true means the fixed opponent is the baseline policy, so the "
                "candidate focus seat meets it directly and the baseline arm is a mirror "
                "control. head_to_head=false is a common-opponent benchmark and must not be "
                "reported as a direct result."
            ),
        },
        "metrics": {
            "candidate": candidate_metrics,
            "baseline": baseline_metrics,
        },
        "deltas": intervals,
        "guards": {
            "saturation": {
                "ceiling_limited": ceiling >= SATURATION_CEILING,
                "max_first_credit_pct": ceiling * 100,
                "threshold_pct": SATURATION_CEILING * 100,
                "note": (
                    "ceiling_limited=true means a +5 pp difference cannot be resolved in this "
                    "condition. It is recorded here; the pass/fail decision is a later guard step."
                ),
            },
            "mirror_symmetry": {
                "baseline_first_credit": baseline_metrics["first_credit"],
                "expected_if_symmetric": 1.0 / players,
                "absolute_deviation": mirror_deviation,
                "baseline_seat_credit_spread": baseline_metrics["first_credit_seat_spread"],
                "threshold": None,
                "note": (
                    "Descriptive only: the design asks the mirror arm to sit near 1/k, but does "
                    "not fix a pass/fail threshold (U5). A large deviation means seat/start/"
                    "draft asymmetry should be investigated before interpreting the delta."
                ),
            },
            "direct_comparison": {
                "head_to_head": head_to_head,
                "interpretation_allowed": head_to_head,
            },
        },
        "claim_policy": {
            "strength_claim_allowed": False,
            "reason": (
                "This report computes design metrics; adoption needs the pre-registered "
                "lexicographic decision (chapter 3) and a non-smoke registered run."
                if manifest["run_mode"] == "smoke"
                else "This report computes design metrics; the adoption decision is chapter 3."
            ),
            "smoke_warning": (
                "SMOKE RUN: the numbers are a wiring check, not a strength measurement."
                if manifest["run_mode"] == "smoke"
                else None
            ),
            "fixture_warning": (
                "TEST FIXTURE CARDS: functional verification only; not an official card pool."
                if manifest["cards"] == "fixture_4p"
                else None
            ),
        },
    }
    report["markdown"] = render_markdown(report)
    return report


def _pct(value: float) -> str:
    return f"{value * 100:.2f} pp"


def _num(value: float, digits: int = 3) -> str:
    return f"{value:.{digits}f}"


def render_markdown(report: dict) -> str:
    condition = report["condition"]
    metrics = report["metrics"]
    deltas = report["deltas"]
    lines = [
        "# 対戦AI評価レポート",
        "",
        f"- 実行モード: **{report['source']['measurement_mode']}**",
        f"- condition_id: `{condition['condition_id']}`",
        f"- 人数: {condition['players']} / カード: `{condition['cards']}`"
        f" / ドラフト: `{condition['draft_mode']}`",
        f"- 探索: `{condition['search_mode']}`"
        f"（予算 {condition['decision_budget_ms']} ms）",
        f"- seed: {report['seeds']['seed_start']} から {report['seeds']['blocks']} ブロック"
        f"（1ブロック = {condition['players']} 席 × 2arm）",
        f"- 試合: {report['completeness']['total_games']}"
        f"（arm あたり {report['completeness']['games_per_arm']}）",
        f"- 直接対戦（head_to_head）: {report['comparison_design']['head_to_head']}",
        "",
    ]
    if report["claim_policy"]["smoke_warning"]:
        lines += [f"> ⚠ {report['claim_policy']['smoke_warning']}", ""]
    if report["claim_policy"]["fixture_warning"]:
        lines += [f"> ⚠ {report['claim_policy']['fixture_warning']}", ""]
    lines += [
        "## 指標",
        "",
        "| arm | 首位率 | 単独1位率 | 首位到達率 | 期待順位 | 得点差平均 |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for arm in ("candidate", "baseline"):
        arm_metrics = metrics[arm]
        lines.append(
            f"| {arm} | {_pct(arm_metrics['first_credit'])}"
            f" | {_pct(arm_metrics['solo_first_rate'])}"
            f" | {_pct(arm_metrics['first_reach_rate'])}"
            f" | {_num(arm_metrics['expected_rank'])}"
            f" | {_num(arm_metrics['score_margin']['mean'], 2)} |"
        )
    lines += [
        "",
        "## 差（candidate − baseline、seedブロック単位）",
        "",
        "| 指標 | 点推定 | 95%CI | ブロックSD | 必要ブロック数（5pp/80%） |",
        "|---|---:|---|---:|---:|",
    ]
    labels = {
        "first_credit": "首位率",
        "expected_rank": "期待順位（負 = 候補が良い）",
        "score_margin": "得点差（正 = 候補が良い）",
    }
    for metric in ("first_credit", "expected_rank", "score_margin"):
        interval = deltas[metric]
        lower, upper = interval["paired_block_bootstrap_95ci"]
        needed = interval["required_seed_blocks_estimate_5pp_80pct_power"]
        lines.append(
            f"| {labels[metric]} | {_num(interval['block_delta_mean'])}"
            f" | [{_num(lower)}, {_num(upper)}]"
            f" | {_num(interval['block_delta_sd']) if interval['block_delta_sd'] is not None else '-'}"
            f" | {needed if needed is not None else '-'} |"
        )
    lines += [
        "",
        "首位率のCIは分布の95%区間であり、+5pp目標の達成判定ではない。"
        "候補が採用かどうかは事前登録した辞書式判定（章3）で行う。",
        "",
        "## 席別首位率（0〜1）",
        "",
        "| arm | " + " | ".join(f"席{seat}" for seat in range(condition["players"])) + " | spread |",
        "|---|" + "---:|" * condition["players"] + "---:|",
    ]
    for arm in ("candidate", "baseline"):
        arm_metrics = metrics[arm]
        seats = " | ".join(_num(value) for value in arm_metrics["first_credit_by_seat"])
        lines.append(
            f"| {arm} | {seats} | {_num(arm_metrics['first_credit_seat_spread'])} |"
        )
    guards = report["guards"]
    lines += [
        "",
        "## ガード（記録）",
        "",
        f"- 飽和（首位率 ≥ 95%）: {guards['saturation']['ceiling_limited']}"
        f"（最大 {guards['saturation']['max_first_credit_pct']:.1f}%）",
        f"- 鏡像armの首位率: {_num(guards['mirror_symmetry']['baseline_first_credit'])}"
        f"（対称なら {_num(guards['mirror_symmetry']['expected_if_symmetric'])}、"
        f"絶対偏差 {_num(guards['mirror_symmetry']['absolute_deviation'])}）",
        f"- 直接対戦として解釈可: {guards['direct_comparison']['interpretation_allowed']}",
        "",
        "## 得点差の分布（candidate / baseline）",
        "",
        "| arm | min | p10 | p50 | p90 | max |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for arm in ("candidate", "baseline"):
        margin = metrics[arm]["score_margin"]
        lines.append(
            f"| {arm} | {_num(margin['min'], 2)} | {_num(margin['p10'], 2)}"
            f" | {_num(margin['p50'], 2)} | {_num(margin['p90'], 2)}"
            f" | {_num(margin['max'], 2)} |"
        )
    lines += [
        "",
        "## 注意",
        "",
        f"- {report['claim_policy']['reason']}",
        "- 首位率の独立標本は試合ではなく seed ブロック。4席を独立試合として数えない。",
        "- 期待順位・得点差は補助指標であり、首位率の判定を覆さない（設計 §1.3・§3.3）。",
        "- 異なる condition_id の Run を合算しない（設計 §5.5）。",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--json", type=Path, default=None)
    parser.add_argument("--markdown", type=Path, default=None)
    parser.add_argument("--iterations", type=int, default=BOOTSTRAP_ITERATIONS)
    args = parser.parse_args()
    try:
        manifest, rows = load_run(args.root)
        report = validate_and_report(manifest, rows, iterations=args.iterations)
        json_path = args.json or args.root / "report.json"
        markdown_path = args.markdown or args.root / "report.md"
        json_path.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        markdown_path.write_text(report["markdown"] + "\n", encoding="utf-8")
        print(report["markdown"])
        print(f"wrote {json_path} and {markdown_path}", file=sys.stderr)
        return 0
    except (OSError, RunError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"eval report failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
