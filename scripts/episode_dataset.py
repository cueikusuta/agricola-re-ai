#!/usr/bin/env python3
"""Generate and validate episode-dataset shards on GitHub CPU runners.

This is the M12 learning-data pipeline (Issue #106). It runs the existing
``episode_export`` / ``episode_inspect`` binaries over disjoint seed ranges,
then cross-checks the per-shard replay reports so a corrupt, incomplete or
overlapping shard cannot be aggregated as success.

It deliberately makes **no** win-rate, strength or 95%-CI claim: that is the
job of the paired-evaluation tooling. This pipeline only proves that a batch of
complete games was generated reproducibly and that every record replays to its
recorded final state under one uniform provenance.

Safety limits (chosen so a manual run stays inside the free GitHub-hosted
runner budget) are enforced here, not only in the workflow.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

MAX_EPISODES = 20000
MAX_SHARDS = 16
MAX_SEED = 10**12
EXPORT_TIMEOUT_SECONDS = 3000
INSPECT_TIMEOUT_SECONDS = 900
POLICIES = ("eval", "greedy", "random")
# A verified production card pool exists for two players only (#105/#115), so
# card episodes cannot be generated for 3-4 players yet.
CARD_PLAYERS = (2,)


def shard_folder_name(index: int) -> str:
    """Artifact/directory name for one shard (must match the workflow)."""
    return f"episode-data-shard-{index}"


def plan(episodes: int, players: int, seed_start: int, shards: int) -> list[dict]:
    """Split `episodes` contiguous seeds into `shards` disjoint ranges.

    Raises ValueError for anything that would make the run unverifiable.
    """
    if players not in (2, 3, 4):
        raise ValueError("players must be 2, 3 or 4")
    if not 1 <= shards <= MAX_SHARDS:
        raise ValueError(f"shards must be within 1..{MAX_SHARDS}")
    if not 1 <= episodes <= MAX_EPISODES:
        raise ValueError(f"episodes must be within 1..{MAX_EPISODES}")
    if episodes % shards:
        raise ValueError(f"episodes ({episodes}) must be divisible by shards ({shards})")
    if episodes < shards:
        raise ValueError("each shard must run at least one episode")
    if not isinstance(seed_start, int) or seed_start < 0:
        raise ValueError("seed_start must be a non-negative integer")
    if seed_start + episodes > MAX_SEED:
        raise ValueError(f"seed range exceeds the allowed limit {MAX_SEED}")
    per_shard = episodes // shards
    return [
        {"index": index, "games": per_shard, "seed_start": seed_start + index * per_shard}
        for index in range(shards)
    ]


def build_export_command(
    binary: Path,
    games: int,
    first_seed: int,
    players: int,
    policy: str,
    extra: bool,
    cards: bool,
    sha: str,
    out: Path,
    errors: Path,
) -> list[str]:
    if policy not in POLICIES:
        raise ValueError(f"unsupported policy {policy!r}")
    command = [
        str(binary),
        "--episodes", str(games),
        "--seed", str(first_seed),
        "--players", str(players),
        "--policy", policy,
        "--out", str(out),
        "--code-sha", sha,
        "--errors", str(errors),
    ]
    if cards:
        command.append("--cards")
    if extra:
        command.append("--additional-action-space")
    return command


def build_inspect_command(inspect_binary: Path, jsonl: Path, report: Path) -> list[str]:
    return [str(inspect_binary), "--in", str(jsonl), "--report", str(report)]


def expected_meta(
    index: int, episodes: int, shard_games: int, first_seed: int, players: int,
    policy: str, extra: bool, cards: bool, sha: str, shards: int,
) -> dict:
    return {
        "schema_version": 1,
        "shard_index": index,
        "shards": shards,
        "total_episodes": episodes,
        "episodes": shard_games,
        "seed_start": first_seed,
        "seed_end": first_seed + shard_games,
        "players": players,
        "policy": policy,
        "additional_action_space": extra,
        "cards": cards,
        "commit_sha": sha,
    }


def run_shard(args: argparse.Namespace) -> int:
    rows = plan(args.episodes, args.players, args.seed_start, args.shards)
    if args.cards and args.players not in CARD_PLAYERS:
        raise ValueError("card episodes currently support two players only")
    row = rows[args.shard]
    folder = Path(args.out)
    folder.mkdir(parents=True, exist_ok=True)
    jsonl = folder / "episodes.jsonl"
    errors = folder / "errors.jsonl"
    report = folder / "report.json"
    meta = expected_meta(
        row["index"], args.episodes, row["games"], row["seed_start"], args.players,
        args.policy, args.extra, args.cards, args.sha, args.shards,
    )
    (folder / "meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")

    command = build_export_command(
        args.binary, row["games"], row["seed_start"], args.players, args.policy,
        args.extra, args.cards, args.sha, jsonl, errors,
    )
    try:
        result = subprocess.run(
            command, capture_output=True, text=True,
            timeout=EXPORT_TIMEOUT_SECONDS, check=False,
        )
    except subprocess.TimeoutExpired:
        (folder / "export-error.log").write_text(
            f"episode_export timed out after {EXPORT_TIMEOUT_SECONDS} seconds\n",
            encoding="utf-8",
        )
        return 1
    (folder / "export.log").write_text(result.stdout, encoding="utf-8")
    (folder / "export-error.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        print(f"episode_export exited with status {result.returncode}", file=sys.stderr)
        return 1

    inspect = build_inspect_command(args.inspect_binary, jsonl, report)
    try:
        checked = subprocess.run(
            inspect, capture_output=True, text=True,
            timeout=INSPECT_TIMEOUT_SECONDS, check=False,
        )
    except subprocess.TimeoutExpired:
        (folder / "inspect-error.log").write_text(
            f"episode_inspect timed out after {INSPECT_TIMEOUT_SECONDS} seconds\n",
            encoding="utf-8",
        )
        return 1
    (folder / "inspect.log").write_text(checked.stdout, encoding="utf-8")
    (folder / "inspect-error.log").write_text(checked.stderr, encoding="utf-8")
    if checked.returncode:
        print(f"episode_inspect exited with status {checked.returncode}", file=sys.stderr)
        return 1
    return 0


def _read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read {path}: {exc}") from exc


def _provenance(report: dict) -> dict:
    return {
        "formats": report.get("formats"),
        "engine_versions": report.get("engine_versions"),
        "rules_versions": report.get("rules_versions"),
        "card_modes": report.get("card_modes"),
        "code_shas": report.get("code_shas"),
    }


def aggregate(
    folder: Path, episodes: int, players: int, seed_start: int, shards: int,
    policy: str, extra: bool, cards: bool, sha: str,
) -> dict:
    rows = plan(episodes, players, seed_start, shards)
    used: set[int] = set()
    total = finished = turns = events = 0
    provenance: dict | None = None
    shard_rows = []
    for row in rows:
        part = folder / shard_folder_name(row["index"])
        expected = expected_meta(
            row["index"], episodes, row["games"], row["seed_start"], players,
            policy, extra, cards, sha, shards,
        )
        meta = _read_json(part / "meta.json")
        for field, value in expected.items():
            if meta.get(field) != value:
                raise ValueError(
                    f"shard {row['index']}: metadata mismatch for {field}: "
                    f"{meta.get(field)!r} != {value!r}"
                )
        report = _read_json(part / "report.json")
        if report.get("corrupt"):
            raise ValueError(f"shard {row['index']}: corrupt records {report['corrupt']}")
        if report.get("duplicate_seeds"):
            raise ValueError(
                f"shard {row['index']}: duplicate seeds {report['duplicate_seeds']}"
            )
        if report.get("total") != row["games"] or report.get("finished") != row["games"]:
            raise ValueError(
                f"shard {row['index']}: expected {row['games']} finished records, "
                f"got total={report.get('total')} finished={report.get('finished')}"
            )
        if report.get("player_counts") != [players]:
            raise ValueError(f"shard {row['index']}: player counts {report.get('player_counts')}")
        if report.get("code_shas") != [sha]:
            raise ValueError(f"shard {row['index']}: commit mismatch {report.get('code_shas')}")
        expected_card_mode = "verified_basic" if cards else "none"
        if report.get("card_modes") != [expected_card_mode]:
            raise ValueError(f"shard {row['index']}: card modes {report.get('card_modes')}")
        if not (report.get("seed_min") == row["seed_start"]
                and report.get("seed_max") == row["seed_start"] + row["games"] - 1):
            raise ValueError(
                f"shard {row['index']}: seed range {report.get('seed_min')}.."
                f"{report.get('seed_max')} does not match the planned block"
            )
        seeds = set(range(row["seed_start"], row["seed_start"] + row["games"]))
        if used.intersection(seeds):
            raise ValueError(f"shard {row['index']}: overlapping seed range")
        used.update(seeds)
        shard_provenance = _provenance(report)
        if provenance is None:
            provenance = shard_provenance
        elif shard_provenance != provenance:
            raise ValueError(f"shard {row['index']}: provenance differs from other shards")
        total += report["total"]
        finished += report["finished"]
        turns += report.get("total_turns", 0)
        events += report.get("total_events", 0)
        shard_rows.append({
            "shard": row["index"],
            "seed_start": row["seed_start"],
            "seed_end": row["seed_start"] + row["games"] - 1,
            "episodes": report["total"],
            "turns": report.get("total_turns", 0),
            "events": report.get("total_events", 0),
        })
    if total != episodes or len(used) != episodes:
        raise ValueError("aggregated episode count does not match the requested total")
    return {
        "schema_version": 1,
        "scope": "episode-data-integrity-and-completeness",
        "not_win_rate_or_strength_claim": True,
        "episodes": episodes,
        "finished": finished,
        "players": players,
        "policy": policy,
        "cards": cards,
        "additional_action_space": extra,
        "seed_start": seed_start,
        "seed_end": seed_start + episodes - 1,
        "commit_sha": sha,
        "shards": shards,
        "total_turns": turns,
        "total_events": events,
        "provenance": provenance,
        "shard_details": shard_rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    for command in ("shard", "aggregate"):
        p = sub.add_parser(command)
        p.add_argument("--episodes", type=int, required=True)
        p.add_argument("--players", type=int, required=True)
        p.add_argument("--seed-start", type=int, required=True)
        p.add_argument("--shards", type=int, required=True)
        p.add_argument("--policy", choices=POLICIES, required=True)
        p.add_argument("--extra", action="store_true")
        p.add_argument("--cards", action="store_true")
        p.add_argument("--sha", required=True)
        p.add_argument("--out", required=True)
        if command == "shard":
            p.add_argument("--shard", type=int, required=True)
            p.add_argument("--binary", type=Path, required=True)
            p.add_argument("--inspect-binary", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.cmd == "shard":
            if not 0 <= args.shard < args.shards:
                raise ValueError("shard index out of range")
            return run_shard(args)
        report = aggregate(
            Path(args.out), args.episodes, args.players, args.seed_start, args.shards,
            args.policy, args.extra, args.cards, args.sha,
        )
        print(json.dumps(report, ensure_ascii=False, indent=2))
        (Path(args.out) / "summary.json").write_text(
            json.dumps(report, indent=2) + "\n", encoding="utf-8"
        )
        return 0
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as exc:
        print(f"episode dataset error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
