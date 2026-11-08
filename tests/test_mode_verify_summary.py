"""Regression tests for the fail-closed mode-verification aggregator (PR #181 review P1).

Every test builds an artifact tree on disk, so it exercises exactly what the
workflow step reads. A passing aggregator must accept a complete, consistent set
and reject every empty, duplicated, missing, mismatched or failing variant.
"""
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from scripts.mode_verify_summary import (
    CHECK_NAMES,
    MODES,
    AggregationError,
    markdown,
    validate,
)

SEEDS = 4
SEED_START = 1000
POLICY = "eval"
SHA = "a" * 64


def game_report(seed: int, players: int, extra: bool, policy: str = POLICY) -> dict:
    return {
        "schema_version": 1,
        "seed": seed,
        "players": players,
        "additional_action_space_enabled": extra,
        "policy": policy,
        "finished": True,
        "rounds_completed": 14,
        "actions": 60,
        "state_hash": "b" * 64,
        "scores": [20] * players,
        "ranks": list(range(1, players + 1)),
        "elapsed_millis": 5,
        "checks": [
            {"name": name, "passed": True, "detail": "ok"} for name in CHECK_NAMES
        ],
    }


def summary(
    players: int,
    extra: bool,
    *,
    seeds: int = SEEDS,
    seed_start: int = SEED_START,
    policy: str = POLICY,
    commit_sha: str = SHA,
) -> dict:
    return {
        "schema_version": 1,
        "players": players,
        "additional_action_space_enabled": extra,
        "policy": policy,
        "seeds_requested": seeds,
        "seeds_run": seeds,
        "seed_start": seed_start,
        "shard": 0,
        "shards": 1,
        "passed_games": seeds,
        "failed_games": 0,
        "elapsed_seconds": 1.0,
        "mean_game_millis": 10.0,
        "checks": {name: {"passed": seeds, "failed": 0} for name in CHECK_NAMES},
        "failures": [],
        "commit_sha": commit_sha,
        "binary_version": "0.1.0",
    }


class AggregatorFixture(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)

    def mode_dir(self, players: int, extra: bool) -> Path:
        path = self.root / f"mode-{players}p-extra-{str(extra).lower()}"
        path.mkdir(parents=True, exist_ok=True)
        return path

    def write_mode(
        self,
        players: int,
        extra: bool,
        *,
        summary_data: dict | None = None,
        games: list[dict] | None = None,
        games_text: str | None = None,
    ) -> Path:
        directory = self.mode_dir(players, extra)
        (directory / "summary.json").write_text(
            json.dumps(
                summary_data if summary_data is not None else summary(players, extra)
            ),
            encoding="utf-8",
        )
        if games_text is not None:
            (directory / "games.jsonl").write_text(games_text, encoding="utf-8")
        else:
            rows = (
                games
                if games is not None
                else [
                    game_report(SEED_START + index, players, extra)
                    for index in range(SEEDS)
                ]
            )
            (directory / "games.jsonl").write_text(
                "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
            )
        return directory

    def write_all(self) -> None:
        for players, extra in MODES:
            self.write_mode(players, extra)

    def run_validator(self) -> dict:
        return validate(
            self.root, seeds=SEEDS, seed_start=SEED_START, policy=POLICY, commit_sha=SHA
        )


class AcceptsCompleteEvidence(AggregatorFixture):
    def test_complete_evidence_is_accepted(self) -> None:
        self.write_all()
        report = self.run_validator()
        self.assertEqual(report["total_games"], SEEDS * len(MODES))
        self.assertEqual(
            [row["players"] for row in report["modes"]], [2, 2, 3, 3, 4, 4]
        )
        self.assertEqual(
            [row["additional_action_space_enabled"] for row in report["modes"]],
            [False, True, False, True, False, True],
        )
        text = markdown(report)
        self.assertIn(str(report["total_games"]), text)
        self.assertIn(SHA, text)


class RejectsIncompleteOrEmptyEvidence(AggregatorFixture):
    def test_missing_mode_is_rejected(self) -> None:
        self.write_all()
        (self.mode_dir(3, True) / "summary.json").unlink()
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("missing mode summaries", str(caught.exception))

    def test_zero_game_summary_is_rejected(self) -> None:
        self.write_all()
        zero = summary(2, False, seeds=0)
        zero["seeds_requested"] = 0
        self.write_mode(2, False, summary_data=zero, games=[])
        with self.assertRaises(AggregationError):
            self.run_validator()

    def test_missing_games_journal_is_rejected(self) -> None:
        self.write_all()
        (self.mode_dir(4, True) / "games.jsonl").unlink()
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("games.jsonl", str(caught.exception))

    def test_empty_games_journal_is_rejected(self) -> None:
        self.write_all()
        self.write_mode(4, True, games_text="")
        with self.assertRaises(AggregationError):
            self.run_validator()

    def test_duplicate_summary_for_one_mode_is_rejected(self) -> None:
        self.write_all()
        duplicate = self.root / "duplicate-shard"
        duplicate.mkdir()
        (duplicate / "summary.json").write_text(
            json.dumps(summary(2, False)), encoding="utf-8"
        )
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("duplicate summary", str(caught.exception))

    def test_unexpected_mode_is_rejected(self) -> None:
        self.write_all()
        extra = self.root / "stray"
        extra.mkdir()
        (extra / "summary.json").write_text(
            json.dumps(summary(5, False)), encoding="utf-8"
        )
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("unsupported mode", str(caught.exception))

    def test_malformed_summary_is_rejected(self) -> None:
        self.write_all()
        (self.mode_dir(2, True) / "summary.json").write_text("{", encoding="utf-8")
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("invalid JSON", str(caught.exception))


class RejectsMismatchedEvidence(AggregatorFixture):
    def assert_rejected(
        self, *, mode: tuple[int, bool], message: str, **changes
    ) -> None:
        self.write_all()
        players, extra = mode
        self.write_mode(players, extra, summary_data=summary(players, extra, **changes))
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn(message, str(caught.exception))

    def test_seed_start_mismatch_is_rejected(self) -> None:
        self.assert_rejected(
            mode=(2, False), seed_start=SEED_START + 1, message="seed_start"
        )

    def test_policy_mismatch_is_rejected(self) -> None:
        self.assert_rejected(mode=(3, True), policy="greedy", message="policy")

    def test_commit_sha_mismatch_is_rejected(self) -> None:
        self.assert_rejected(mode=(4, False), commit_sha="c" * 64, message="commit_sha")

    def test_seeds_requested_mismatch_is_rejected(self) -> None:
        self.assert_rejected(
            mode=(2, True), seeds=SEEDS + 1, message="seeds_requested"
        )

    def test_sharded_summary_is_rejected(self) -> None:
        self.write_all()
        data = summary(3, False)
        data["shards"] = 4
        self.write_mode(3, False, summary_data=data)
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("shards", str(caught.exception))

    def test_check_totals_must_match_seeds_run(self) -> None:
        self.write_all()
        data = summary(3, True)
        data["checks"]["replay_matches_log"] = {"passed": SEEDS - 1, "failed": 1}
        self.write_mode(3, True, summary_data=data)
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("replay_matches_log", str(caught.exception))

    def test_journal_line_count_mismatch_is_rejected(self) -> None:
        self.write_all()
        self.write_mode(
            2,
            False,
            games=[
                game_report(SEED_START + index, 2, False)
                for index in range(SEEDS - 1)
            ],
        )
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("games != summary seeds_run", str(caught.exception))

    def test_duplicate_seed_in_journal_is_rejected(self) -> None:
        self.write_all()
        self.write_mode(
            4, True, games=[game_report(SEED_START, 4, True) for _ in range(SEEDS)]
        )
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("duplicate seeds", str(caught.exception))

    def test_seed_outside_the_dispatched_range_is_rejected(self) -> None:
        self.write_all()
        self.write_mode(
            2,
            True,
            games=[
                game_report(SEED_START + index, 2, True)
                for index in range(SEEDS - 1)
            ]
            + [game_report(SEED_START + 999, 2, True)],
        )
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("seed set mismatch", str(caught.exception))

    def test_failed_check_inside_a_game_is_rejected(self) -> None:
        self.write_all()
        rows = [game_report(SEED_START + index, 3, False) for index in range(SEEDS)]
        rows[2]["checks"][1]["passed"] = False
        rows[2]["checks"][1]["detail"] = "replay failed"
        self.write_mode(3, False, games=rows)
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("failed", str(caught.exception))

    def test_unfinished_game_is_rejected(self) -> None:
        self.write_all()
        rows = [game_report(SEED_START + index, 2, False) for index in range(SEEDS)]
        rows[0]["finished"] = False
        self.write_mode(2, False, games=rows)
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("did not finish", str(caught.exception))

    def test_game_mode_disagreeing_with_summary_is_rejected(self) -> None:
        self.write_all()
        rows = [game_report(SEED_START + index, 3, True) for index in range(SEEDS)]
        rows[1]["additional_action_space_enabled"] = False
        self.write_mode(3, True, games=rows)
        with self.assertRaises(AggregationError) as caught:
            self.run_validator()
        self.assertIn("mode disagrees", str(caught.exception))


class RejectsInvalidInputs(AggregatorFixture):
    def test_seed_range_overflow_is_rejected(self) -> None:
        self.write_all()
        with self.assertRaises(AggregationError):
            validate(
                self.root,
                seeds=2,
                seed_start=2**64 - 1,
                policy=POLICY,
                commit_sha=SHA,
            )

    def test_zero_seeds_is_rejected(self) -> None:
        self.write_all()
        with self.assertRaises(AggregationError):
            validate(
                self.root, seeds=0, seed_start=SEED_START, policy=POLICY, commit_sha=SHA
            )

    def test_unknown_policy_is_rejected(self) -> None:
        self.write_all()
        with self.assertRaises(AggregationError):
            validate(
                self.root,
                seeds=SEEDS,
                seed_start=SEED_START,
                policy="mcts",
                commit_sha=SHA,
            )


if __name__ == "__main__":
    unittest.main()
