#!/usr/bin/env python3
"""Generate M14 G0 evidence for one evaluated mode (#167 C).

The G0 gate no longer trusts declarations: it recomputes scale, replay coverage
and block completeness from the matches files, and checks the digest of those
files. This script is the supported way to produce evidence, because it is the
component that computes the digest from the artifact instead of asking a human
to type a number.

Usage:

    python scripts/g0_evidence.py \
        --mode cardless_2p --players 2 --card-mode none \
        --matches ablation/cardless_2p/shard-0001/matches.jsonl \
        --artifact-url https://github.com/OWNER/REPO/actions/runs/123 \
        --code-sha "$(git rev-parse HEAD)" \
        --out .work/g0/cardless_2p.json

`--matches` may be repeated for a sharded run; the digest is taken over the
concatenation in the order given. Paths are written relative to the evidence
file so the gate can resolve them when it re-reads the artifact.

Attested checks (`m8_rule_evidence` and friends) are optional and can only be
supplied from a file with `--checks`; the script records that file's digest so
the gate can bind the claims to the same code revision and matches bytes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

EVIDENCE_SCHEMA_VERSION = 2
CHECKS_SCHEMA_VERSION = 2
SUPPORTED_CARD_MODES = ("none", "verified_basic")


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def normalise_card_mode(value: str) -> str:
    return "verified_basic" if value in ("verified", "verified_basic_2p") else value


def count_rows(files: list[Path]) -> tuple[int, int]:
    """Rows and distinct (block, arm, seat) keys, used for a sanity echo."""
    rows = 0
    keys = set()
    for path in files:
        with path.open(encoding="utf-8") as handle:
            for line in handle:
                if not line.strip():
                    continue
                row = json.loads(line)
                rows += 1
                keys.add((row.get("block_index"), row.get("arm_id"), row.get("candidate_seat")))
    return rows, len(keys)


def build(
    *,
    mode: str,
    players: int,
    card_mode: str,
    matches: list[Path],
    artifact_url: str,
    code_sha: str,
    checks: Path | None,
    relative_to: Path | None = None,
) -> dict:
    card_mode = normalise_card_mode(card_mode)
    if card_mode not in SUPPORTED_CARD_MODES:
        raise ValueError(
            f"card mode {card_mode!r} is not verified; unverified cards cannot enter "
            "an evaluation that gates learning")
    if not matches:
        raise ValueError("at least one --matches file is required")
    payload = b"".join(path.read_bytes() for path in matches)
    digest = sha256_hex(payload)
    rows, distinct_keys = count_rows(matches)
    if distinct_keys != rows:
        raise ValueError(
            f"duplicate (block, arm, seat) rows detected: {rows} rows, "
            f"{distinct_keys} distinct keys")
    def recorded(path: Path) -> str:
        # The gate resolves declared paths relative to the evidence file, so write
        # them that way; an absolute --out then needs no special casing.
        if relative_to is None:
            return str(path)
        return os.path.relpath(path.resolve(), relative_to.resolve())

    mode_entry = {
        "mode": mode,
        "players": players,
        "card_mode": card_mode,
        "artifact_url": artifact_url,
        "matches_sha256": digest,
        "matches_paths": [recorded(path) for path in matches],
    }
    if checks is not None:
        checks_bytes = checks.read_bytes()
        mode_entry["checks_path"] = recorded(checks)
        mode_entry["checks_sha256"] = sha256_hex(checks_bytes)
    evidence = {
        "schema_version": EVIDENCE_SCHEMA_VERSION,
        "code_sha": code_sha,
        "modes": [mode_entry],
    }
    evidence["generator_note"] = (
        "matches_sha256 is computed from the artifact bytes; the gate recomputes it "
        "and derives scale/replay/completeness from the rows.")
    return evidence


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", required=True)
    parser.add_argument("--players", type=int, required=True, choices=(2, 3, 4))
    parser.add_argument("--card-mode", required=True)
    parser.add_argument("--matches", type=Path, action="append", default=[],
                        help="matches JSONL; repeat for sharded runs")
    parser.add_argument("--artifact-url", required=True)
    parser.add_argument("--code-sha", required=True)
    parser.add_argument("--checks", type=Path, default=None,
                        help="attested checks file (schema_version 2)")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        evidence = build(
            mode=args.mode,
            players=args.players,
            card_mode=args.card_mode,
            matches=args.matches,
            artifact_url=args.artifact_url,
            code_sha=args.code_sha,
            checks=args.checks,
            relative_to=args.out.parent,
        )
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(
            json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({
            "out": str(args.out),
            "mode": args.mode,
            "matches_sha256": evidence["modes"][0]["matches_sha256"],
            "checks_sha256": evidence["modes"][0].get("checks_sha256"),
        }, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"g0_evidence failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
