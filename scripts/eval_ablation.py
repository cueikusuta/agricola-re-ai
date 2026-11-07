#!/usr/bin/env python3
"""M13 ablation: separate one factor at a time from a fixed control arm.

An *arm* is one `paired_eval` run: a candidate policy/parameter set measured
against the same baseline policy on the same seed blocks. Every arm in one
ablation therefore shares its baseline games, which is what makes the arms
comparable at all. `compare_arms` verifies that and fails closed otherwise.

Two different comparisons live here and are never mixed up:

* per-arm `mean_first_credit_delta` — candidate vs the arm's own baseline.
* per-arm delta vs the *control arm* — candidate A vs candidate B, paired by
  seed block, which is the ablation question the roadmap asks for ("how much
  did this one change contribute?").

Statistics are seed-block bootstrap with a fixed generator seed, matching
`scripts/paired_eval_stats.py`. Nothing here resamples independent games or
treats correlated seats as independent evidence.
"""
from __future__ import annotations

import argparse
import json
import random
import re
import statistics
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from scripts.paired_eval_stats import SEARCH_MODES, validate_and_report  # noqa: E402

# Must stay in step with paired_eval_stats.POLICY_CHOICES, which gained
# "puct" when PR #146 merged; otherwise a PUCT arm cannot be compared.
POLICIES = ("random", "greedy", "eval", "mcts", "puct")
BOOTSTRAP_SEED = 20261106

# Every candidate-side search knob an arm may set (#167 B). Each entry is
# `name -> (paired_eval flag, default, validator kind)`. The defaults mirror
# `MctsPolicy`/`PuctConfig::default()` in Rust. A knob that is not listed here
# cannot be ablated: an unrecognised key is rejected instead of being silently
# dropped before it reaches the binary.
SEARCH_PARAMETERS = {
    "mcts_simulations": ("--mcts-simulations", 64, "simulations"),
    "mcts_tree_depth": ("--mcts-tree-depth", 2, "tree_depth"),
    "puct_simulations": ("--puct-simulations", 32, "simulations"),
    "puct_tree_depth": ("--puct-tree-depth", 2, "tree_depth"),
    "puct_c": ("--puct-c", 1.5, "non_negative"),
    "puct_prior_temp": ("--puct-prior-temp", 3.0, "positive"),
    "puct_value_temp": ("--puct-value-temp", 8.0, "positive"),
    "puct_leaf": ("--puct-leaf", "terminal", "leaf"),
    "puct_root_limit": ("--puct-root-limit", 0, "root_limit"),
    "puct_final_prior": ("--puct-final-prior", 0.0, "final_prior"),
}
# The policy itself plus every search knob: exactly the factors that may differ
# between the control arm and a variant in a one-factor-at-a-time ablation.
CANDIDATE_FACTORS = ("candidate",) + tuple(SEARCH_PARAMETERS)
# Knobs that only exist on the PUCT side.
PUCT_ONLY_PARAMETERS = tuple(
    name for name in SEARCH_PARAMETERS if name.startswith("puct_"))
# Experiment-definition keys that only the specification may set.
SPEC_PARAMETERS = (
    "players", "seed_start", "blocks", "first_block", "baseline", "opponent",
    "extra", "cards", "decision_budget_ms", "search_mode",
    "time_budget_max_simulations",
)


def effective_parameters(options: dict) -> dict:
    """The values an arm actually runs with, defaults filled in.

    Comparing declared (possibly missing) keys would let an arm that omits a
    knob look identical to a control that sets it explicitly, so every
    comparison and every report row uses the effective values.
    """
    return {
        name: options.get(name, spec[1])
        for name, spec in SEARCH_PARAMETERS.items()
    }


def _finite(value: object) -> bool:
    return (
        type(value) in (int, float)
        and value == value
        and value not in (float("inf"), float("-inf"))
    )


def validate_parameters(label: str, options: dict) -> None:
    """Reject unknown, mistyped, or out-of-range search parameters.

    The ranges mirror `paired_eval`'s own CLI validation, so an ablation cannot
    declare an arm that the binary would then reject halfway through a run.
    """
    allowed = set(SEARCH_PARAMETERS) | set(SPEC_PARAMETERS) | {"candidate", "baseline"}
    for key in options:
        if key in allowed:
            continue
        raise ValueError(f"arm {label!r} has an unknown parameter: {key!r}")
    for name, (_, _, kind) in SEARCH_PARAMETERS.items():
        if name not in options:
            continue
        value = options[name]
        if kind in ("simulations", "tree_depth", "root_limit"):
            if type(value) is not int:
                raise ValueError(f"arm {label!r}: {name} must be an integer")
        if kind == "simulations" and not 1 <= value <= 100000:
            raise ValueError(f"arm {label!r}: {name} must be 1..=100000")
        if kind == "tree_depth" and not 1 <= value <= 6:
            raise ValueError(f"arm {label!r}: {name} must be 1..=6")
        if kind == "root_limit" and not 0 <= value <= 100:
            raise ValueError(f"arm {label!r}: {name} must be 0..=100")
        if kind in ("non_negative", "positive", "final_prior"):
            if not _finite(value):
                raise ValueError(f"arm {label!r}: {name} must be a finite number")
            if kind == "non_negative" and not 0.0 <= value <= 100.0:
                raise ValueError(f"arm {label!r}: {name} must be 0..=100")
            if kind == "positive" and value <= 0.0:
                raise ValueError(f"arm {label!r}: {name} must be > 0")
            if kind == "final_prior" and not 0.0 <= value <= 10.0:
                raise ValueError(f"arm {label!r}: {name} must be 0..=10")
        if kind == "leaf" and value not in ("heuristic", "terminal"):
            raise ValueError(
                f"arm {label!r}: {name} must be 'heuristic' or 'terminal'")


def check_puct_parameters_are_effective(
    label: str, options: dict, control: str, control_options: dict
) -> None:
    """Fail closed on a PUCT knob change in an arm that cannot use it.

    `paired_eval` accepts the PUCT flags on every run, so a mislabelled arm could
    otherwise "run" a knob no seat reads and report a delta of zero as if it were
    evidence that the knob does not matter.
    """
    if "puct" in (options.get("candidate"), options.get("baseline")):
        return
    arm_values = effective_parameters(options)
    control_values = effective_parameters(control_options)
    changed = [
        name for name in PUCT_ONLY_PARAMETERS
        if arm_values[name] != control_values[name]
    ]
    if changed:
        raise ValueError(
            f"arm {label!r} changes PUCT-only parameters {changed} but neither "
            f"its candidate nor its baseline is 'puct' (control {control!r})"
        )


def baseline_signature(rows: list[dict]) -> dict[tuple[int, int], tuple]:
    """Per (block, seat) fingerprint of the shared baseline games.

    Arms may only be compared if their baseline games are literally the same
    ones. A differing replay hash means the "control" differed, and any delta
    between arms would be confounded.
    """
    return {
        (row["block_index"], row["candidate_seat"]): (
            row["replay_hash"],
            row["first_place_credit"],
            tuple(row["scores_by_seat"]),
        )
        for row in rows
        if row["arm_id"] == "baseline"
    }


def candidate_credit_by_block(rows: list[dict]) -> dict[int, float]:
    """Seat-average candidate first-place credit per seed block."""
    grouped: dict[int, list[float]] = {}
    for row in rows:
        if row["arm_id"] == "candidate":
            grouped.setdefault(row["block_index"], []).append(row["first_place_credit"])
    return {block: statistics.mean(values) for block, values in grouped.items()}


def _bootstrap_interval(deltas: list[float], iterations: int) -> list[float]:
    generator = random.Random(BOOTSTRAP_SEED)
    blocks = len(deltas)
    boots = sorted(
        sum(deltas[generator.randrange(blocks)] for _ in range(blocks)) / blocks
        for _ in range(iterations)
    )
    return [boots[int((iterations - 1) * .025)], boots[int((iterations - 1) * .975)]]


def compare_arms(
    arms: dict[str, list[dict]],
    *,
    control: str,
    per_arm_options: dict[str, dict],
    iterations: int = 10000,
) -> dict:
    """Validate every arm, verify comparability, then bootstrap arm vs control."""
    if control not in arms:
        raise ValueError(f"control arm {control!r} is missing")
    if len(arms) < 2:
        raise ValueError("an ablation needs at least a control and one variant")
    if iterations < 100:
        raise ValueError("invalid bootstrap iterations")
    if any(not rows for rows in arms.values()):
        raise ValueError("every arm must contain game records")

    for label in arms:
        if label not in per_arm_options:
            raise ValueError(f"arm {label!r} has no declared options")
        validate_parameters(label, per_arm_options[label])

    reports = {}
    for label, rows in arms.items():
        options = per_arm_options[label]
        if options["candidate"] not in POLICIES or options["baseline"] not in POLICIES:
            raise ValueError(f"arm {label!r} uses an unsupported policy")
        if options["candidate"] == options["baseline"]:
            raise ValueError(f"arm {label!r} compares a policy with itself")
        reports[label] = validate_and_report(
            rows,
            players=options["players"],
            seed_start=options["seed_start"],
            first_block=options["first_block"],
            blocks=options["blocks"],
            candidate=options["candidate"],
            baseline=options["baseline"],
            opponent=options["opponent"],
            extra=options["extra"],
            search_mode=options["search_mode"],
            iterations=iterations,
        )

    # Comparability. Arms are only interpretable together when they share the
    # player count, opponent, seed range, extra-tile setting, card condition,
    # decision budget and baseline policy.
    controls = reports[control]
    for label, report in reports.items():
        for field in (
            "players",
            "opponent",
            "seed_start",
            "first_block",
            "seed_blocks",
            "additional_action_space",
            "baseline",
            "decision_budget_ms",
            # #164: a fixed-simulation arm and a fixed-wall-clock arm answer
            # different questions, so they may not share an ablation table.
            "search_mode",
        ):
            if report[field] != controls[field]:
                raise ValueError(
                    f"arm {label!r} disagrees with control {control!r} on {field}"
                )
        if report.get("cards") != controls.get("cards"):
            raise ValueError(f"arm {label!r} disagrees with the control on cards")

    # Only one candidate-side factor may differ from the control in a
    # one-factor-at-a-time ablation. A no-change arm is allowed as a sanity
    # check, but changing both the policy and its search parameters confounds
    # any causal attribution.
    control_options = per_arm_options[control]
    control_values = effective_parameters(control_options)
    changed_factors = {}
    for label, options in per_arm_options.items():
        values = effective_parameters(options)
        changes = [
            field for field in ("candidate",)
            if options.get(field) != control_options.get(field)
        ]
        changes.extend(
            name for name in SEARCH_PARAMETERS
            if values[name] != control_values[name]
        )
        if label in arms and len(changes) > 1:
            raise ValueError(f"arm {label!r} changes multiple candidate factors: {changes}")
        if label in arms and changes:
            check_puct_parameters_are_effective(
                label, options, control, control_options)
        changed_factors[label] = changes

    # Deck identity must also match between arms. The paired report checks
    # within-arm consistency (after #160); this cross-arm guard prevents a
    # different card-set revision from being mistaken for a policy effect.
    control_decks = {
        (row.get("cards"), row.get("card_set_count"), row.get("card_set_hash"))
        for row in arms[control]
    }
    for label, rows in arms.items():
        decks = {
            (row.get("cards"), row.get("card_set_count"), row.get("card_set_hash"))
            for row in rows
        }
        if decks != control_decks:
            raise ValueError(f"arm {label!r} uses a different card-set identity")

    control_baseline = baseline_signature(arms[control])
    for label, rows in arms.items():
        if baseline_signature(rows) != control_baseline:
            raise ValueError(
                f"arm {label!r} did not replay the same baseline games as {control!r}"
            )

    control_credit = candidate_credit_by_block(arms[control])
    rows_out = []
    for label in arms:
        report = reports[label]
        by_block = candidate_credit_by_block(arms[label])
        if set(by_block) != set(control_credit):
            raise ValueError(f"arm {label!r} covers different seed blocks than the control")
        deltas = [by_block[block] - control_credit[block] for block in sorted(by_block)]
        options = per_arm_options[label]
        rows_out.append(
            {
                "arm": label,
                "is_control": label == control,
                "changed_factors_vs_control": changed_factors[label],
                "candidate": options["candidate"],
                "search_parameters": effective_parameters(options),
                "changed_search_parameters": {
                    name: {
                        "control": control_values[name],
                        "arm": effective_parameters(options)[name],
                    }
                    for name in changed_factors[label]
                    if name in SEARCH_PARAMETERS
                },
                "candidate_first_credit_pct": report["candidate_first_credit_pct"],
                "candidate_solo_first_rate": report.get("candidate_solo_first_rate"),
                "mean_first_credit_delta_pp_vs_own_baseline": report[
                    "mean_first_credit_delta_pp"
                ],
                "paired_block_bootstrap_95ci_pp_vs_own_baseline": report[
                    "paired_block_bootstrap_95ci_pp"
                ],
                "mean_first_credit_delta_pp_vs_control": statistics.mean(deltas) * 100.0,
                "paired_block_bootstrap_95ci_pp_vs_control": [
                    bound * 100.0 for bound in _bootstrap_interval(deltas, iterations)
                ],
                "candidate_expected_rank": report["candidate_expected_rank"],
                "candidate_score_margin": report["candidate_score_margin"],
                "games_per_arm": report["games_per_arm"],
                "candidate_focus_decisions": report["candidate_focus_decisions"],
                "candidate_focus_timeout_rate": report["candidate_timeout_rate"],
                "candidate_focus_fallback_rate": report["candidate_fallback_rate"],
                "candidate_max_focus_decision_ms": report[
                    "candidate_max_focus_decision_ms"
                ],
            }
        )

    control_row = next(row for row in rows_out if row["is_control"])
    return {
        "schema_version": 1,
        "mode": "ablation_vs_control_arm",
        "units": "percentage points (pp); 1.0 fraction = 100 pp",
        "control": control,
        "players": controls["players"],
        "opponent": controls["opponent"],
        "baseline_policy": controls["baseline"],
        "seed_start": controls["seed_start"],
        "first_block": controls["first_block"],
        "seed_blocks": controls["seed_blocks"],
        "cards": controls.get("cards", "none"),
        "decision_budget_ms": controls["decision_budget_ms"],
        "search_mode": controls.get("search_mode", "simulation-capped"),
        "bootstrap_iterations": iterations,
        "bootstrap_seed": BOOTSTRAP_SEED,
        "baseline_games_are_shared": True,
        "control_candidate_first_credit_pct": control_row["candidate_first_credit_pct"],
        "arms": rows_out,
        "caveat": (
            "Deltas vs the control arm are paired by seed block and share the "
            "same baseline games; that is what makes one-factor-at-a-time "
            "attribution possible. A positive delta is not a +5pp goal result "
            "unless its interval excludes 0 and the point estimate reaches "
            "+5pp under the pre-registered primary comparison."
        ),
    }


def read_arm(folder: Path) -> list[dict]:
    paths = sorted(folder.glob("shard-*/matches.jsonl"))
    if not paths:
        raise ValueError(f"no matches.jsonl under {folder}")
    rows = []
    for path in paths:
        with path.open(encoding="utf-8") as file:
            for line in file:
                if line.strip():
                    rows.append(json.loads(line))
    return rows


def arm_command(binary: Path, arm: dict, output: Path) -> list[str]:
    """The exact `paired_eval` invocation for one arm.

    Kept separate from `run_arm` so a test can prove that every declared knob
    reaches the binary instead of only that `subprocess.run` was called.
    """
    command = [
        str(binary),
        "--players", str(arm["players"]),
        "--seed-start", str(arm["seed_start"]),
        "--block-start", str(arm["first_block"]),
        "--blocks", str(arm["blocks"]),
        "--candidate", arm["candidate"],
        "--baseline", arm["baseline"],
        "--opponent", arm["opponent"],
        "--additional-action-space", "true" if arm["extra"] else "false",
        "--search-mode", arm["search_mode"],
        "--time-budget-max-simulations", str(arm["time_budget_max_simulations"]),
        "--decision-budget-ms", str(arm["decision_budget_ms"]),
    ]
    # Every search knob reaches the binary explicitly, defaults included, so what
    # ran is exactly what the arm declared.
    values = effective_parameters(arm)
    for name, (flag, _, _) in SEARCH_PARAMETERS.items():
        command.extend([flag, str(values[name])])
    command.extend(["--output", str(output)])
    return command


def run_arm(binary: Path, arm: dict, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(arm_command(binary, arm, output), check=True)


def plan_arms(spec: dict) -> dict[str, dict]:
    """Turn an arm specification into the per-arm option dicts that run.

    Fail closed on unknown keys at both levels: a typo in a specification key
    (for example a misspelt PUCT knob) must not silently produce an arm that runs
    with default parameters and looks like a valid ablation.
    """
    # Search knobs may be set once for the whole specification; anything else
    # outside the documented keys is a typo that would otherwise run silently.
    unknown_spec = (
        set(spec) - set(SPEC_PARAMETERS) - set(SEARCH_PARAMETERS) - {"arms"}
    )
    if unknown_spec:
        raise ValueError(
            f"unknown specification parameter(s): {sorted(unknown_spec)}")
    arm_keys = set(SEARCH_PARAMETERS) | set(SPEC_PARAMETERS) | {
        "label", "candidate", "baseline"}
    per_arm: dict[str, dict] = {}
    for arm in spec["arms"]:
        label = arm["label"]
        if not isinstance(label, str) or not re.fullmatch(r"[A-Za-z0-9_-]+", label):
            raise ValueError(f"unsafe arm label: {label!r}")
        if label in per_arm:
            raise ValueError(f"duplicate arm label: {label!r}")
        unknown_arm = set(arm) - arm_keys
        if unknown_arm:
            raise ValueError(
                f"arm {label!r} has an unknown parameter: {sorted(unknown_arm)}")
        per_arm[label] = {
            "players": spec["players"],
            "seed_start": spec["seed_start"],
            "first_block": spec.get("first_block", 0),
            "blocks": spec["blocks"],
            "candidate": arm["candidate"],
            "baseline": arm.get("baseline", spec["baseline"]),
            "opponent": spec["opponent"],
            "extra": spec.get("extra", False),
            "decision_budget_ms": arm.get(
                "decision_budget_ms", spec.get("decision_budget_ms", 0)
            ),
            # #164: the search mode is part of the experiment definition and
            # must reach the binary and the validator unchanged, otherwise a
            # time-budgeted arm is rejected as simulation-capped.
            "search_mode": arm.get(
                "search_mode", spec.get("search_mode", "simulation-capped")
            ),
            "time_budget_max_simulations": arm.get(
                "time_budget_max_simulations",
                spec.get("time_budget_max_simulations", 100000),
            ),
        }
        # Per-arm search knobs override the specification-level default, which
        # itself falls back to the Rust default. Recording the effective value
        # here means an arm can never run with something it did not declare.
        for name, (_, default, _) in SEARCH_PARAMETERS.items():
            per_arm[label][name] = arm.get(name, spec.get(name, default))
        validate_parameters(label, per_arm[label])
        if per_arm[label]["search_mode"] not in SEARCH_MODES:
            raise ValueError(
                f"arm {label!r} has an unsupported search mode: "
                f"{per_arm[label]['search_mode']!r}"
            )
    return per_arm


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--arms", type=Path, required=True,
                        help="JSON file describing the arms; see docs")
    parser.add_argument("--control", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path("ablation"),
                        help="working folder; each arm is written to <root>/<label>")
    parser.add_argument("--binary", type=Path, default=Path("target/release/paired_eval"))
    parser.add_argument("--run", action="store_true",
                        help="invoke the binary; omit to aggregate existing output")
    parser.add_argument("--iterations", type=int, default=10000)
    args = parser.parse_args()
    try:
        spec = json.loads(args.arms.read_text(encoding="utf-8"))
        per_arm = plan_arms(spec)
        arms = {}
        for label, options in per_arm.items():
            folder = args.root / label
            if args.run:
                run_arm(args.binary, options, folder / "shard-0001" / "matches.jsonl")
            arms[label] = read_arm(folder)
        report = compare_arms(
            arms, control=args.control, per_arm_options=per_arm,
            iterations=args.iterations,
        )
        args.output.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError,
            subprocess.CalledProcessError) as exc:
        print(f"ablation failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
