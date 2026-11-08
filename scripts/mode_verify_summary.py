#!/usr/bin/env python3
"""Fail-closed aggregation for `mode-verification.yml` (PR #181 review P1).

The previous workflow step only checked that six `summary.json` files existed
and that each reported `failed_games == 0`. That accepts an empty run: six
files claiming `seeds_run=0` would pass, and no evidence would be produced.

This module validates the whole artifact set instead:

* exactly the six supported modes (2/3/4 players x additional tile OFF/ON),
  with no duplicate and no unexpected summary;
* every summary agrees with the dispatched inputs (seeds, seed start, policy,
  execution SHA) and covers a single unsharded run;
* every mode has a per-game `games.jsonl` whose line count, seed set and mode
  fields match the summary, and whose games all passed every check.

Anything missing, duplicated, inconsistent or empty fails with a non-zero exit.
The rules are unit-tested in `tests/test_mode_verify_summary.py`.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import math
from pathlib import Path
import sys

SCHEMA_VERSION = 1
PLAYERS = (2, 3, 4)
EXTRA = (False, True)
MODES = tuple((players, extra) for players in PLAYERS for extra in EXTRA)
CHECK_NAMES = (
    "completed_fourteen_rounds",
    "final_state_invariants",
    "replay_matches_log",
    "save_restore_round_trip",
    "observation_structural_privacy",
    "seed_reproducible",
)
MODE_LABEL = {
    (players, extra): f"{players}P extra={'ON' if extra else 'OFF'}"
    for players, extra in MODES
}


class AggregationError(Exception):
    """Raised when the evidence set cannot be trusted."""


def mode_label(key: tuple[int, bool]) -> str:
    return MODE_LABEL.get(key, f"{key[0]}P extra={key[1]}")


def read_json(path: Path) -> dict:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise AggregationError(f"{path}: {error}") from error
    try:
        data = json.loads(text)
    except json.JSONDecodeError as error:
        raise AggregationError(f"{path}: invalid JSON: {error}") from error
    if not isinstance(data, dict):
        raise AggregationError(f"{path}: expected a JSON object")
    return data


def mode_of(data: dict, path: Path) -> tuple[int, bool]:
    players = data.get("players")
    extra = data.get("additional_action_space_enabled")
    if type(players) is not int or type(extra) is not bool:
        raise AggregationError(f"{path}: players/additional tile must be int/bool")
    key = (players, extra)
    if key not in MODE_LABEL:
        raise AggregationError(f"{path}: unsupported mode {mode_label(key)}")
    return key


def is_number(value: object) -> bool:
    return type(value) in (int, float) and math.isfinite(value)


def check_summary(data: dict, path: Path, key: tuple[int, bool], expected: dict) -> dict:
    def require(condition: bool, message: str) -> None:
        if not condition:
            raise AggregationError(f"{path} [{mode_label(key)}]: {message}")

    require(data.get("schema_version") == SCHEMA_VERSION, "schema_version mismatch")
    require(
        data.get("seeds_requested") == expected["seeds"],
        "seeds_requested != dispatch input",
    )
    require(data.get("seeds_run") == expected["seeds"], "seeds_run != dispatch input")
    require(
        type(data.get("seeds_run")) is int and data["seeds_run"] > 0,
        "seeds_run must be a positive integer",
    )
    for field in ("passed_games", "failed_games"):
        require(
            type(data.get(field)) is int and data[field] >= 0,
            f"{field} must be int >= 0",
        )
    require(
        data["passed_games"] + data["failed_games"] == data["seeds_run"],
        "passed+failed != seeds_run",
    )
    require(data["failed_games"] == 0, f"{data['failed_games']} games failed")
    require(data["passed_games"] == data["seeds_run"], "not every game passed")
    require(data.get("shard") == 0, "shard != 0 (aggregation expects one unsharded run)")
    require(data.get("shards") == 1, "shards != 1 (aggregation expects one unsharded run)")
    require(
        data.get("seed_start") == expected["seed_start"],
        "seed_start != dispatch input",
    )
    require(data.get("policy") == expected["policy"], "policy != dispatch input")
    require(
        data.get("commit_sha") == expected["commit_sha"],
        "commit_sha != execution SHA",
    )
    require(is_number(data.get("mean_game_millis")), "mean_game_millis must be finite")
    require(data["mean_game_millis"] >= 0, "mean_game_millis must be >= 0")
    checks = data.get("checks")
    require(isinstance(checks, dict), "checks must be an object")
    require(set(checks) == set(CHECK_NAMES), f"check names mismatch: {sorted(checks)}")
    for name in CHECK_NAMES:
        totals = checks[name]
        require(isinstance(totals, dict), f"check {name} must be an object")
        require(totals.get("failed") == 0, f"check {name} has failures")
        require(
            totals.get("passed") == data["seeds_run"],
            f"check {name} passed count != seeds_run",
        )
    require(data.get("failures") == [], "failure list is not empty")
    return data


def check_games(
    journal: Path,
    key: tuple[int, bool],
    summary: dict,
    expected_seeds: set[int],
) -> None:
    try:
        lines = journal.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise AggregationError(f"{journal}: {error}") from error
    if len(lines) != summary["seeds_run"]:
        raise AggregationError(
            f"{journal}: {len(lines)} games != summary seeds_run {summary['seeds_run']}"
        )
    seen: Counter[int] = Counter()
    for index, line in enumerate(lines, start=1):
        try:
            report = json.loads(line)
        except json.JSONDecodeError as error:
            raise AggregationError(f"{journal}:{index}: invalid JSON: {error}") from error
        if not isinstance(report, dict):
            raise AggregationError(f"{journal}:{index}: expected an object")
        seed = report.get("seed")
        if type(seed) is not int:
            raise AggregationError(f"{journal}:{index}: seed must be an integer")
        seen[seed] += 1
        if report.get("players") != key[0] or (
            report.get("additional_action_space_enabled") is not key[1]
        ):
            raise AggregationError(f"{journal}:{index}: mode disagrees with its summary")
        if report.get("policy") != summary["policy"]:
            raise AggregationError(
                f"{journal}:{index}: policy disagrees with its summary"
            )
        if report.get("finished") is not True:
            raise AggregationError(f"{journal}:{index}: game did not finish")
        if report.get("rounds_completed") != 14:
            raise AggregationError(f"{journal}:{index}: rounds_completed != 14")
        state_hash = report.get("state_hash")
        if not isinstance(state_hash, str) or len(state_hash) != 64:
            raise AggregationError(f"{journal}:{index}: missing final state hash")
        checks = report.get("checks")
        if not isinstance(checks, list):
            raise AggregationError(f"{journal}:{index}: checks must be a list")
        names = [check.get("name") for check in checks if isinstance(check, dict)]
        if sorted(names) != sorted(CHECK_NAMES):
            raise AggregationError(f"{journal}:{index}: check set mismatch")
        for check in checks:
            if check.get("passed") is not True:
                raise AggregationError(
                    f"{journal}:{index}: {check.get('name')} failed: {check.get('detail')}"
                )
    duplicates = sorted(seed for seed, count in seen.items() if count > 1)
    if duplicates:
        raise AggregationError(f"{journal}: duplicate seeds {duplicates[:5]}")
    if set(seen) != expected_seeds:
        missing = sorted(expected_seeds - set(seen))[:5]
        extra = sorted(set(seen) - expected_seeds)[:5]
        raise AggregationError(
            f"{journal}: seed set mismatch (missing {missing}, extra {extra})"
        )


def collect(directory: Path) -> dict[tuple[int, bool], Path]:
    found: dict[tuple[int, bool], Path] = {}
    for path in sorted(directory.rglob("summary.json")):
        key = mode_of(read_json(path), path)
        if key in found:
            raise AggregationError(
                f"duplicate summary for {mode_label(key)}: {found[key]} and {path}"
            )
        found[key] = path
    return found


def validate(
    directory: Path,
    *,
    seeds: int,
    seed_start: int,
    policy: str,
    commit_sha: str,
) -> dict:
    if type(seeds) is not int or seeds <= 0:
        raise AggregationError("seeds must be a positive integer")
    if type(seed_start) is not int or seed_start < 0:
        raise AggregationError("seed_start must be an unsigned integer")
    if seed_start + seeds - 1 > 2**64 - 1:
        raise AggregationError("seed range overflows u64")
    if policy not in ("eval", "greedy", "random"):
        raise AggregationError("unsupported policy")
    expected = {
        "seeds": seeds,
        "seed_start": seed_start,
        "policy": policy,
        "commit_sha": commit_sha,
    }
    expected_seeds = set(range(seed_start, seed_start + seeds))
    found = collect(directory)
    missing = sorted(set(MODES) - set(found))
    if missing:
        raise AggregationError(
            f"missing mode summaries: {[mode_label(key) for key in missing]}"
        )
    rows = []
    for key in MODES:
        path = found[key]
        summary = check_summary(read_json(path), path, key, expected)
        journal = path.with_name("games.jsonl")
        check_games(journal, key, summary, expected_seeds)
        rows.append(
            {
                "players": key[0],
                "additional_action_space_enabled": key[1],
                "seeds_run": summary["seeds_run"],
                "passed_games": summary["passed_games"],
                "failed_games": summary["failed_games"],
                "mean_game_millis": float(summary["mean_game_millis"]),
                "commit_sha": summary["commit_sha"],
                "policy": summary["policy"],
                "seed_start": summary["seed_start"],
                "summary_path": str(path),
            }
        )
    return {
        "schema_version": SCHEMA_VERSION,
        "seeds": seeds,
        "seed_start": seed_start,
        "policy": policy,
        "commit_sha": commit_sha,
        "modes": rows,
        "total_games": sum(row["seeds_run"] for row in rows),
    }


def markdown(report: dict) -> str:
    lines = [
        "# Mode-wide simulator verification evidence",
        "",
        f"- commit: `{report['commit_sha']}`",
        f"- policy (every seat): `{report['policy']}`",
        f"- seeds per mode: {report['seeds']} (start {report['seed_start']})",
        f"- total games: {report['total_games']}",
        "",
        "| mode | additional tile | seeds | passed | failed | mean ms/game |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for row in report["modes"]:
        lines.append(
            "| {}P | {} | {} | {} | {} | {:.1f} |".format(
                row["players"],
                "ON" if row["additional_action_space_enabled"] else "OFF",
                row["seeds_run"],
                row["passed_games"],
                row["failed_games"],
                row["mean_game_millis"],
            )
        )
    lines.append("")
    lines.append(
        "Every game additionally proved 14-round completion, final-state invariants,"
        " full-log replay with per-action invariants, save/restore round-trip,"
        " structural observation privacy and fixed-seed reproducibility."
    )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--seeds", type=int, required=True)
    parser.add_argument("--seed-start", type=int, required=True)
    parser.add_argument("--policy", required=True)
    parser.add_argument("--commit-sha", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--json-output", type=Path)
    args = parser.parse_args(argv)
    try:
        report = validate(
            args.root,
            seeds=args.seeds,
            seed_start=args.seed_start,
            policy=args.policy,
            commit_sha=args.commit_sha,
        )
    except AggregationError as error:
        print(f"mode verification aggregation failed: {error}", file=sys.stderr)
        return 1
    text = markdown(report)
    args.output.write_text(text, encoding="utf-8")
    if args.json_output:
        args.json_output.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
