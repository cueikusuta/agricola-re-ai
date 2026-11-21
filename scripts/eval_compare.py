#!/usr/bin/env python3
"""Compare two design-aligned `eval_harness` runs (PR #400 design §2.5, §5.5).

Each run already compares a candidate against the same frozen baseline inside
its own condition. This script answers the next question: *given two candidate
versions measured under the same condition, which one is better?*

Comparability is fail closed. Both runs must share the `condition_id`, the
player/card/draft/budget conditions, the seed blocks and the bootstrap
convention, and both must be head-to-head. Otherwise the runs answer different
questions and are not differenced.

The paired difference is computed per seed block from each run's
candidate-minus-baseline series. The result is a first-layer *signal*; adoption
still requires the pre-registered lexicographic rule (and a registered run), so
the default final answer is "undetermined".
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import statistics
import sys

# Allow running as `python3 scripts/eval_compare.py` from the repository root.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from scripts.eval_report import (
    BOOTSTRAP_ITERATIONS,
    BOOTSTRAP_SEED,
    REQUIRED_BLOCKS_TARGET,
    RunError,
    block_series,
    bootstrap_ci,
    layer1_category,
    load_run,
    required_blocks,
    validate_and_report,
    validate_rows,
)


def _comparable(a_manifest: dict, a_report: dict, b_manifest: dict, b_report: dict) -> dict:
    condition_fields = (
        "condition_id",
        "players",
        "cards",
        "card_set_hash",
        "card_set_count",
        "additional_action_space",
    )
    checks = {}
    for field in condition_fields:
        checks[field] = a_manifest.get(field) == b_manifest.get(field)
    checks["draft_mode"] = (
        a_manifest["draft"]["mode"] == b_manifest["draft"]["mode"]
    )
    checks["search_mode"] = (
        a_manifest["compute_budget"]["search_mode"]
        == b_manifest["compute_budget"]["search_mode"]
    )
    checks["decision_budget_ms"] = (
        a_manifest["compute_budget"]["decision_budget_ms"]
        == b_manifest["compute_budget"]["decision_budget_ms"]
    )
    checks["baseline_policy"] = (
        a_manifest["arms"]["baseline"]["game_policy_id"]
        == b_manifest["arms"]["baseline"]["game_policy_id"]
    )
    checks["opponent_policy"] = (
        a_manifest["arms"]["opponent"]["game_policy_id"]
        == b_manifest["arms"]["opponent"]["game_policy_id"]
    )
    for field in ("seed_start", "draft_seed_start", "first_block", "blocks"):
        checks[f"seeds.{field}"] = (
            a_manifest["seeds"][field] == b_manifest["seeds"][field]
        )
    checks["head_to_head"] = (
        a_report["comparison_design"]["head_to_head"]
        and b_report["comparison_design"]["head_to_head"]
    )
    failed = sorted(name for name, ok in checks.items() if not ok)
    return {
        "comparable": not failed,
        "checks": checks,
        "failed_checks": failed,
        "reason": (
            "the runs share one condition, the same seed blocks and a head-to-head "
            "design"
            if not failed
            else "the runs are not comparable: " + ", ".join(failed)
        ),
    }


def compare_runs(
    a_manifest: dict,
    a_rows: list[dict],
    b_manifest: dict,
    b_rows: list[dict],
    *,
    iterations: int = BOOTSTRAP_ITERATIONS,
) -> dict:
    if iterations < 100:
        raise RunError("bootstrap iterations must be at least 100")
    a_received = validate_rows(a_manifest, a_rows)
    b_received = validate_rows(b_manifest, b_rows)
    a_report = validate_and_report(a_manifest, a_rows, iterations=iterations)
    b_report = validate_and_report(b_manifest, b_rows, iterations=iterations)
    comparability = _comparable(a_manifest, a_report, b_manifest, b_report)
    if not comparability["comparable"]:
        raise RunError(comparability["reason"])
    _, a_deltas = block_series(a_manifest, a_received)
    _, b_deltas = block_series(b_manifest, b_received)
    cross = {}
    for metric in ("first_credit", "expected_rank", "score_margin"):
        samples = [
            a_value - b_value
            for a_value, b_value in zip(a_deltas[metric], b_deltas[metric], strict=True)
        ]
        point, lower, upper = bootstrap_ci(samples, iterations)
        sample_sd = statistics.stdev(samples) if len(samples) > 1 else None
        cross[metric] = {
            "a_minus_b_block_mean": point,
            "paired_block_bootstrap_95ci": [lower, upper],
            "block_sd": sample_sd,
            "required_seed_blocks_estimate_5pp_80pct_power": (
                required_blocks(sample_sd) if metric == "first_credit" else None
            ),
            "direction": {
                "first_credit": "positive = run A's candidate has the higher first-place credit",
                "expected_rank": "negative = run A's candidate has the better (smaller) rank",
                "score_margin": "positive = run A's candidate has the larger score margin",
            }[metric],
        }
    credit = cross["first_credit"]
    layer1 = layer1_category(
        credit["a_minus_b_block_mean"],
        *credit["paired_block_bootstrap_95ci"],
    )
    rank = cross["expected_rank"]
    result = {
        "schema_version": 1,
        "comparison": "run_vs_run_paired_block",
        "runs": {
            "a": {
                "run_id": a_manifest.get("run_id"),
                "run_mode": a_manifest["run_mode"],
                "candidate_game_policy_id": a_manifest["arms"]["candidate"][
                    "game_policy_id"
                ],
                "candidate_deltas": a_report["deltas"],
            },
            "b": {
                "run_id": b_manifest.get("run_id"),
                "run_mode": b_manifest["run_mode"],
                "candidate_game_policy_id": b_manifest["arms"]["candidate"][
                    "game_policy_id"
                ],
                "candidate_deltas": b_report["deltas"],
            },
        },
        "condition_id": a_manifest["condition_id"],
        "seeds": {
            "seed_start": a_manifest["seeds"]["seed_start"],
            "draft_seed_start": a_manifest["seeds"]["draft_seed_start"],
            "first_block": a_manifest["seeds"]["first_block"],
            "blocks": a_manifest["seeds"]["blocks"],
        },
        "comparability": comparability,
        "cross_run_deltas": cross,
        "decision": {
            "layer1_signal": layer1,
            "layer1_evidence": {
                "a_minus_b_pp": credit["a_minus_b_block_mean"] * 100,
                "ci95_pp": [
                    value * 100 for value in credit["paired_block_bootstrap_95ci"]
                ],
            },
            "layer2_rank_reference": {
                "reference_allowed": layer1 == "hold",
                "status": "descriptive_only",
                "point": rank["a_minus_b_block_mean"],
                "ci95": rank["paired_block_bootstrap_95ci"],
                "note": "the second layer never replaces the first-layer rule (U5)",
            },
            "final": "undetermined",
            "final_reason": (
                "run-to-run comparisons are exploratory here; adoption needs a "
                "pre-registered experiment, and the manifest of these runs is not registered"
            ),
            "exploratory_only": True,
            "target_pp": REQUIRED_BLOCKS_TARGET * 100,
        },
        "claim_policy": {
            "strength_claim_allowed": False,
            "note": (
                "This is a comparison of two measured candidates under one condition. "
                "It is not an adoption decision and not an official-rules claim."
            ),
        },
    }
    result["markdown"] = render_markdown(result)
    return result


def render_markdown(result: dict) -> str:
    runs = result["runs"]
    cross = result["cross_run_deltas"]
    labels = {
        "first_credit": "首位率（正 = A が良い）",
        "expected_rank": "期待順位（負 = A が良い）",
        "score_margin": "得点差（正 = A が良い）",
    }
    lines = [
        "# 候補どうしの比較（同一条件・seedブロック対応）",
        "",
        f"- condition_id: `{result['condition_id']}`",
        f"- A: `{runs['a']['candidate_game_policy_id']}`（{runs['a']['run_mode']}）",
        f"- B: `{runs['b']['candidate_game_policy_id']}`（{runs['b']['run_mode']}）",
        f"- seed: {result['seeds']['seed_start']} から {result['seeds']['blocks']} ブロック",
        f"- 比較可能性: {result['comparability']['comparable']}"
        f" — {result['comparability']['reason']}",
        "",
        "## 各Runの candidate − baseline",
        "",
        "| 指標 | A: 点推定 | A: 95%CI | B: 点推定 | B: 95%CI |",
        "|---|---:|---|---:|---|",
    ]
    for metric in ("first_credit", "expected_rank", "score_margin"):
        a = runs["a"]["candidate_deltas"][metric]
        b = runs["b"]["candidate_deltas"][metric]
        lines.append(
            f"| {metric} | {a['block_delta_mean']:.3f}"
            f" | [{a['paired_block_bootstrap_95ci'][0]:.3f}, "
            f"{a['paired_block_bootstrap_95ci'][1]:.3f}]"
            f" | {b['block_delta_mean']:.3f}"
            f" | [{b['paired_block_bootstrap_95ci'][0]:.3f}, "
            f"{b['paired_block_bootstrap_95ci'][1]:.3f}] |"
        )
    lines += [
        "",
        "## A − B（seedブロックで対応）",
        "",
        "| 指標 | 点推定 | 95%CI | ブロックSD |",
        "|---|---:|---|---:|",
    ]
    for metric in ("first_credit", "expected_rank", "score_margin"):
        item = cross[metric]
        sd = item["block_sd"]
        lines.append(
            f"| {labels[metric]} | {item['a_minus_b_block_mean']:.3f}"
            f" | [{item['paired_block_bootstrap_95ci'][0]:.3f}, "
            f"{item['paired_block_bootstrap_95ci'][1]:.3f}]"
            f" | {sd:.3f} |"
            if sd is not None
            else f"| {labels[metric]} | {item['a_minus_b_block_mean']:.3f}"
            f" | [{item['paired_block_bootstrap_95ci'][0]:.3f}, "
            f"{item['paired_block_bootstrap_95ci'][1]:.3f}] | - |"
        )
    decision = result["decision"]
    lines += [
        "",
        "## 判定",
        "",
        f"- 第一層シグナル: `{decision['layer1_signal']}`"
        f"（A−B {decision['layer1_evidence']['a_minus_b_pp']:.2f} pp、"
        f"95%CI [{decision['layer1_evidence']['ci95_pp'][0]:.2f}, "
        f"{decision['layer1_evidence']['ci95_pp'][1]:.2f}] pp）",
        f"- 第二層（期待順位）: 参照可 "
        f"{decision['layer2_rank_reference']['reference_allowed']}"
        f" / {decision['layer2_rank_reference']['status']}",
        f"- 最終: `{decision['final']}` — {decision['final_reason']}",
        "",
        "## 注意",
        "",
        "- A−B は同一 seed ブロックで対応させた差。独立標本はブロック。",
        "- これは探索的な候補比較であり、採用判定ではない。",
        "- 条件（condition_id・seed範囲・予算・相手）が違う Run は比較できない。",
        "",
    ]
    return "\n".join(lines)


def load_pair(a_root: Path, b_root: Path) -> tuple[dict, list[dict], dict, list[dict]]:
    a_manifest, a_rows = load_run(a_root)
    b_manifest, b_rows = load_run(b_root)
    return a_manifest, a_rows, b_manifest, b_rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--a", type=Path, required=True)
    parser.add_argument("--b", type=Path, required=True)
    parser.add_argument("--json", type=Path, default=None)
    parser.add_argument("--markdown", type=Path, default=None)
    parser.add_argument("--iterations", type=int, default=BOOTSTRAP_ITERATIONS)
    args = parser.parse_args()
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    try:
        a_manifest, a_rows, b_manifest, b_rows = load_pair(args.a, args.b)
        result = compare_runs(
            a_manifest, a_rows, b_manifest, b_rows, iterations=args.iterations
        )
        json_path = args.json or args.a / "compare.json"
        markdown_path = args.markdown or args.a / "compare.md"
        json_path.write_text(
            json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        markdown_path.write_text(result["markdown"] + "\n", encoding="utf-8")
        print(result["markdown"])
        print(f"wrote {json_path} and {markdown_path}", file=sys.stderr)
        return 0
    except (OSError, RunError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"eval compare failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
