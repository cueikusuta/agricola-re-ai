#!/usr/bin/env python3
"""Run and aggregate cardless Agricola batch smoke tests on GitHub CPU runners.

This intentionally measures completion/throughput only, not AI win rates or
confidence intervals. The current --batch CLI emits no per-game record.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import re
import subprocess
import sys

SHARDS = 4
MAX_GAMES = 20000
MAX_SEED = 10**12
SUMMARY_PATTERN = re.compile(
    r"^games: (\d+), additional_action_space_enabled: (true|false), "
    r"mean_game_seconds: ([0-9.eE+-]+), score_mean: ([0-9.eE+-]+), "
    r"score_min: ([0-9.eE+-]+), score_max: ([0-9.eE+-]+)$", re.MULTILINE
)
HISTOGRAM_PATTERN = re.compile(r"^score_histogram: (\{[^\n]+\})$", re.MULTILINE)


def plan(total_games: int, players: int, seed_start: int, shard_index: int) -> tuple[int, int]:
    if total_games < SHARDS * 2 or total_games > MAX_GAMES or total_games % SHARDS:
        raise ValueError(f"games must be divisible by {SHARDS}, within {SHARDS*2}..{MAX_GAMES}")
    if players not in (2, 3, 4):
        raise ValueError("players must be 2, 3 or 4")
    if not 0 <= shard_index < SHARDS:
        raise ValueError("shard index out of range")
    if not 0 <= seed_start <= MAX_SEED or seed_start + total_games > MAX_SEED:
        raise ValueError("seed range exceeds the allowed limit")
    shard_games = total_games // SHARDS
    return shard_games, seed_start + shard_index * shard_games


def parse_batch_output(content: str, expected_games: int, players: int, extra: bool) -> dict:
    m = SUMMARY_PATTERN.search(content)
    h = HISTOGRAM_PATTERN.search(content)
    if not m or not h:
        raise ValueError("missing batch summary or score histogram")
    count, flag, mean_time, mean_score, min_score, max_score = m.groups()
    if int(count) != expected_games:
        raise ValueError(f"games mismatch: {count} != {expected_games}")
    if (flag == "true") != extra:
        raise ValueError("additional action setting mismatch")
    try:
        hist = json.loads(h.group(1))
    except json.JSONDecodeError as exc:
        raise ValueError(f"malformed score histogram: {exc}") from exc
    if not isinstance(hist, dict) or not hist:
        raise ValueError("empty or non-object score histogram")
    if not all(str(k).lstrip("-").isdigit() and type(v) is int and v >= 0 for k, v in hist.items()):
        raise ValueError("invalid histogram keys/counts")
    if sum(hist.values()) != expected_games * players:
        raise ValueError("histogram count does not match games * players")
    values = list(map(float, (mean_time, mean_score, min_score, max_score)))
    if not all(math.isfinite(v) for v in values) or values[0] < 0:
        raise ValueError("non-finite/negative timing or score")
    actual_score_mean = sum(int(k) * v for k, v in hist.items()) / (expected_games * players)
    if abs(values[1] - actual_score_mean) > 0.015:
        raise ValueError("reported score average disagrees with histogram")
    if int(min(hist, key=lambda k: int(k))) != int(values[2]):
        raise ValueError("reported score minimum disagrees with histogram")
    if int(max(hist, key=lambda k: int(k))) != int(values[3]):
        raise ValueError("reported score maximum disagrees with histogram")
    return {"games": expected_games, "mean_game_seconds": values[0], "histogram": hist}


def run_shard(args: argparse.Namespace) -> int:
    games, first_seed = plan(args.games, args.players, args.seed_start, args.shard)
    if args.policy not in ("random", "greedy", "eval"):
        raise ValueError("unsupported policy")
    folder = Path(args.out)
    folder.mkdir(parents=True, exist_ok=True)
    meta = {
        "schema_version": 1, "shard_index": args.shard, "shards": SHARDS,
        "total_games": args.games, "games": games, "seed_start": first_seed,
        "players": args.players, "policy": args.policy,
        "additional_action_space": args.extra, "commit_sha": args.sha,
    }
    (folder / "meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")
    command = [str(args.binary), "--batch", str(games), "--players", str(args.players),
               "--seed", str(first_seed)]
    if args.policy == "greedy":
        command.append("--greedy")
    elif args.policy == "eval":
        command.append("--eval")
    if args.extra:
        command.append("--additional-action-space")
    try:
        result = subprocess.run(command, capture_output=True, text=True,
                                timeout=3000, check=False)
    except subprocess.TimeoutExpired:
        (folder / "error.log").write_text("Rust batch timed out after 3000 seconds\n", encoding="utf-8")
        return 1
    (folder / "batch.log").write_text(result.stdout, encoding="utf-8")
    (folder / "error.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        print(f"batch exited with status {result.returncode}", file=sys.stderr)
        return 1
    try:
        parse_batch_output(result.stdout, games, args.players, args.extra)
    except ValueError as exc:
        print(f"invalid batch output: {exc}", file=sys.stderr)
        return 1
    return 0


def aggregate(folder: Path, total_games: int, players: int, seed_start: int,
              policy: str, extra: bool, sha: str) -> dict:
    plan(total_games, players, seed_start, 0)
    expected_shard_games = total_games // SHARDS
    records = []
    used = set()
    hist = {}
    elapsed = 0.0
    for i in range(SHARDS):
        part = folder / f"mass-smoke-shard-{i}"
        meta = json.loads((part / "meta.json").read_text(encoding="utf-8"))
        for field, value in {
            "shard_index": i, "shards": SHARDS, "total_games": total_games,
            "games": expected_shard_games, "seed_start": seed_start + i * expected_shard_games,
            "players": players, "policy": policy, "additional_action_space": extra,
            "commit_sha": sha,
        }.items():
            if meta.get(field) != value:
                raise ValueError(f"shard {i}: metadata mismatch for {field}")
        start = meta["seed_start"]
        seeds = set(range(start, start + expected_shard_games))
        if used.intersection(seeds):
            raise ValueError(f"shard {i}: overlapping seed range")
        used.update(seeds)
        batch = parse_batch_output((part / "batch.log").read_text(encoding="utf-8"),
                                   expected_shard_games, players, extra)
        elapsed += batch["mean_game_seconds"] * expected_shard_games
        for score, count in batch["histogram"].items():
            hist[score] = hist.get(score, 0) + count
        records.append({"shard": i, "first_seed": start, "games": expected_shard_games,
                        "mean_game_seconds": batch["mean_game_seconds"]})
    if len(used) != total_games or sum(hist.values()) != total_games * players:
        raise ValueError("missing or duplicated games after aggregation")
    return {
        "schema_version": 1, "scope": "completion-and-throughput-only",
        "not_win_rate_or_replay_proof": True, "games": total_games,
        "players": players, "additional_action_space": extra, "policy": policy,
        "seed_start": seed_start, "commit_sha": sha,
        "mean_game_seconds": elapsed / total_games, "score_histogram": hist,
        "shards": records,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    for command in ("shard", "aggregate"):
        p = sub.add_parser(command)
        p.add_argument("--games", type=int, required=True)
        p.add_argument("--players", type=int, required=True)
        p.add_argument("--seed-start", type=int, required=True)
        p.add_argument("--policy", choices=("random", "greedy", "eval"), required=True)
        p.add_argument("--extra", action="store_true")
        p.add_argument("--sha", required=True)
        p.add_argument("--out", required=True)
        if command == "shard":
            p.add_argument("--shard", type=int, required=True)
            p.add_argument("--binary", type=Path, required=True)
    args = ap.parse_args()
    try:
        if args.cmd == "shard":
            return run_shard(args)
        report = aggregate(Path(args.out), args.games, args.players,
                           args.seed_start, args.policy, args.extra, args.sha)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        (Path(args.out) / "summary.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8")
        return 0
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as exc:
        print(f"mass smoke error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
