"""No-network tests for the M12 episode-dataset pipeline (Issue #106)."""
from pathlib import Path
import json
import tempfile
import unittest

from scripts.episode_dataset import (
    MAX_SHARDS,
    aggregate,
    build_export_command,
    build_inspect_command,
    plan,
    shard_folder_name,
)


def report(
    *, total, finished, players, seed_min, seed_max, sha="abc",
    card_mode="none", corrupt=None, duplicate_seeds=None,
    engine_versions=None, formats=None, schema_versions=None,
):
    return {
        "total": total,
        "finished": finished,
        "schema_versions": schema_versions or [1],
        "player_counts": [players],
        "duplicate_seeds": duplicate_seeds or [],
        "corrupt": corrupt or [],
        "seed_min": seed_min,
        "seed_max": seed_max,
        "distinct_seeds": total - len(duplicate_seeds or []),
        "total_turns": total * 10,
        "total_events": total * 10,
        "formats": formats or ["agricola-episode"],
        "engine_versions": engine_versions or ["0.1.0"],
        "rules_versions": ["agricola-revised-2016-beginner"],
        "card_modes": [card_mode],
        "code_shas": [sha],
    }


def make_shard(root: Path, name: str, meta: dict, report_body: dict) -> None:
    folder = root / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "meta.json").write_text(json.dumps(meta), encoding="utf-8")
    (folder / "report.json").write_text(json.dumps(report_body), encoding="utf-8")


def meta_for(index, games, first_seed, shards, episodes, sha="abc"):
    return {
        "schema_version": 1,
        "shard_index": index,
        "shards": shards,
        "total_episodes": episodes,
        "episodes": games,
        "seed_start": first_seed,
        "seed_end": first_seed + games,
        "players": 2,
        "policy": "eval",
        "additional_action_space": False,
        "cards": False,
        "commit_sha": sha,
    }


class PlanTests(unittest.TestCase):
    def test_ranges_are_contiguous_and_disjoint(self):
        rows = plan(8, 2, 400, 4)
        self.assertEqual([row["games"] for row in rows], [2, 2, 2, 2])
        self.assertEqual([row["seed_start"] for row in rows], [400, 402, 404, 406])
        covered = set()
        for row in rows:
            seeds = set(range(row["seed_start"], row["seed_start"] + row["games"]))
            self.assertFalse(covered.intersection(seeds))
            covered.update(seeds)
        self.assertEqual(covered, set(range(400, 408)))

    def test_rejects_unverifiable_runs(self):
        with self.assertRaises(ValueError):
            plan(9, 2, 400, 4)  # not divisible
        with self.assertRaises(ValueError):
            plan(8, 5, 400, 4)  # unsupported player count
        with self.assertRaises(ValueError):
            plan(8, 2, 400, 0)  # no shards
        with self.assertRaises(ValueError):
            plan(8, 2, 400, MAX_SHARDS + 1)  # too many shards
        with self.assertRaises(ValueError):
            plan(0, 2, 400, 4)  # no episodes
        with self.assertRaises(ValueError):
            plan(8, 2, 10**12, 4)  # seed range beyond the limit
        with self.assertRaises(ValueError):
            plan(4, 2, 400, 8)  # fewer episodes than shards


class CommandTests(unittest.TestCase):
    def test_export_command_flags(self):
        command = build_export_command(
            Path("b"), 3, 100, 2, "greedy", True, False, "sha",
            Path("o.jsonl"), Path("e.jsonl"),
        )
        self.assertEqual(command[0], "b")
        self.assertIn("--policy", command)
        self.assertEqual(command[command.index("--policy") + 1], "greedy")
        self.assertIn("--additional-action-space", command)
        self.assertNotIn("--cards", command)
        self.assertEqual(command[command.index("--code-sha") + 1], "sha")

    def test_export_command_rejects_unknown_policy(self):
        with self.assertRaises(ValueError):
            build_export_command(
                Path("b"), 1, 1, 2, "best", False, False, "sha",
                Path("o"), Path("e"),
            )

    def test_inspect_command(self):
        command = build_inspect_command(Path("i"), Path("in.jsonl"), Path("r.json"))
        self.assertEqual(command, ["i", "--in", "in.jsonl", "--report", "r.json"])

    def test_shard_folder_name_matches_artifact(self):
        self.assertEqual(shard_folder_name(2), "episode-data-shard-2")


class AggregateTests(unittest.TestCase):
    def create_fixture(self, root: Path, sha="abc"):
        for index, first in ((0, 100), (1, 102)):
            make_shard(
                root,
                shard_folder_name(index),
                meta_for(index, 2, first, 2, 4, sha=sha),
                report(total=2, finished=2, players=2, seed_min=first, seed_max=first + 1, sha=sha),
            )

    def test_happy_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            summary = aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")
            self.assertEqual(summary["episodes"], 4)
            self.assertEqual(summary["finished"], 4)
            self.assertEqual(summary["total_turns"], 40)
            self.assertTrue(summary["not_win_rate_or_strength_claim"])
            self.assertEqual([row["seed_start"] for row in summary["shard_details"]], [100, 102])

    def test_rejects_corrupt_records(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=103,
                       corrupt=["1: replay mismatch"]),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_duplicate_seeds(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=103,
                       duplicate_seeds=[102]),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_incomplete_and_wrong_count(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=1, players=2, seed_min=102, seed_max=103),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_seed_range_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=104),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_commit_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4, sha="other"),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=103, sha="other"),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_provenance_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=103,
                       engine_versions=["9.9.9"]),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_schema_provenance_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            make_shard(
                root, shard_folder_name(1), meta_for(1, 2, 102, 2, 4),
                report(total=2, finished=2, players=2, seed_min=102, seed_max=103,
                       schema_versions=[2]),
            )
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")

    def test_rejects_missing_shard(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            (root / shard_folder_name(1) / "report.json").unlink()
            with self.assertRaises(ValueError):
                aggregate(root, 4, 2, 100, 2, "eval", False, False, "abc")


if __name__ == "__main__":
    unittest.main()
