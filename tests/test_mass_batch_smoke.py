"""No-network tests for the GitHub-hosted batch smoke aggregator."""
from pathlib import Path
import json
import tempfile
import unittest

from scripts.mass_batch_smoke import aggregate, parse_batch_output, plan


def output(games=2, players=2, extra=False):
    # Each player earns 10 in this synthetic fixture.
    histogram = {"10": games * players}
    return (
        f"games: {games}, additional_action_space_enabled: {str(extra).lower()}, "
        "mean_game_seconds: 0.001000, score_mean: 10.00, score_min: 10, score_max: 10\n"
        f"score_histogram: {json.dumps(histogram)}\n"
    )


class MassBatchSmokeTests(unittest.TestCase):
    def test_plan_nonoverlap(self):
        self.assertEqual(plan(8, 2, 400, 0), (2, 400))
        self.assertEqual(plan(8, 2, 400, 3), (2, 406))
        with self.assertRaises(ValueError):
            plan(9, 2, 400, 0)
        with self.assertRaises(ValueError):
            plan(8, 5, 400, 0)
        with self.assertRaises(ValueError):
            plan(8, 2, 10**12, 0)

    def test_output_uses_games_times_players(self):
        self.assertEqual(parse_batch_output(output(), 2, 2, False)["games"], 2)
        with self.assertRaises(ValueError):
            parse_batch_output(output(), 3, 2, False)
        with self.assertRaises(ValueError):
            parse_batch_output(output(), 2, 4, False)
        with self.assertRaises(ValueError):
            parse_batch_output(output(), 2, 2, True)

    def create_fixture(self, root: Path, corrupt_shard=None):
        for index in range(4):
            folder = root / f"mass-smoke-shard-{index}"
            folder.mkdir(parents=True)
            meta = {
                "schema_version": 1, "shard_index": index, "shards": 4,
                "total_games": 8, "games": 2, "seed_start": 400 + index * 2,
                "players": 2, "policy": "random",
                "additional_action_space": False, "commit_sha": "abc",
            }
            if corrupt_shard == index:
                meta["seed_start"] = 400
            (folder / "meta.json").write_text(json.dumps(meta), encoding="utf-8")
            (folder / "batch.log").write_text(output(), encoding="utf-8")

    def test_aggregate_counts_and_fails_on_overlap(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            report = aggregate(root, 8, 2, 400, "random", False, "abc")
            self.assertEqual(report["games"], 8)
            self.assertEqual(report["score_histogram"], {"10": 16})
            self.assertTrue(report["not_win_rate_or_replay_proof"])
            (root / "mass-smoke-shard-1" / "meta.json").write_text(
                json.dumps({
                    "schema_version": 1, "shard_index": 1, "shards": 4,
                    "total_games": 8, "games": 2, "seed_start": 400,
                    "players": 2, "policy": "random",
                    "additional_action_space": False, "commit_sha": "abc",
                }), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                aggregate(root, 8, 2, 400, "random", False, "abc")

    def test_aggregate_fails_when_shard_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.create_fixture(root)
            (root / "mass-smoke-shard-2" / "batch.log").unlink()
            with self.assertRaises(OSError):
                aggregate(root, 8, 2, 400, "random", False, "abc")


if __name__ == "__main__":
    unittest.main()
