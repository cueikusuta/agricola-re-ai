"""Contracts for the explicitly authorized M13 4-player direct comparison (#214).

The lightweight CI verifies the workflow is not inadvertently changed into an
unconditional push job and that shard planning covers 250 seed blocks exactly.
Actual GitHub event authorization and 2,000 games are evaluated on Actions.
"""
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "m13-direct-puct-mcts.yml"
COMMAND = "/run-m13-4p-puct-vs-mcts-20261109"


class ExplicitM13RunContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workflow = WORKFLOW.read_text(encoding="utf-8")

    def test_only_explicit_issue_comment_or_workflow_dispatch(self):
        workflow = self.workflow
        self.assertIn("  issue_comment:\n    types: [created]", workflow)
        self.assertIn("  workflow_dispatch:", workflow)
        self.assertNotIn("  push:", workflow)
        self.assertNotIn("  pull_request:", workflow)
        self.assertIn("github.repository == 'cueikusuta/agricola-re-ai'", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("github.event.issue.number == 106", workflow)
        self.assertIn("github.event.issue.pull_request == null", workflow)
        self.assertIn(f"github.event.comment.body == '{COMMAND}'", workflow)
        self.assertIn("github.event.comment.user.login == 'cueikusuta'", workflow)
        self.assertIn("github.actor == 'cueikusuta'", workflow)
        self.assertNotIn("permissions:\n  contents: write", workflow)

    def test_formal_fixed_experiment_matches_preregistered_protocol(self):
        workflow = self.workflow
        for token in (
            "--players 4", "--seed-start 965000",
            "--candidate puct --baseline mcts --opponent mcts",
            "--additional-action-space false --cards none",
            "--search-mode simulation-capped --decision-budget-ms 0",
            "--mcts-simulations 16 --mcts-tree-depth 2",
            "--puct-simulations 16 --puct-tree-depth 2",
            "--puct-leaf terminal",
            "--blocks 250",
            '"games_per_arm"] == 1000',
            '"total_games"] == 2000',
            '"head_to_head"] is True',
            '"mirror_baseline"] is True',
        ):
            with self.subTest(token=token):
                self.assertIn(token, workflow)
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertIn("retention-days: 30", workflow)
        self.assertIn("cancel-in-progress: false", workflow)

    def test_eight_shards_partition_250_seed_blocks_once(self):
        self.assertIn("shard: [0, 1, 2, 3, 4, 5, 6, 7]", self.workflow)
        covered = []
        for shard in range(8):
            first = 250 * shard // 8
            limit = 250 * (shard + 1) // 8
            self.assertGreater(limit, first)
            covered.extend(range(first, limit))
        self.assertEqual(covered, list(range(250)))
        self.assertEqual(len(covered) * 4 * 2, 2000)

    def test_aggregate_requires_success_and_matching_run_artifacts(self):
        workflow = self.workflow
        self.assertIn('if [[ "$SHARD_RESULT" != "success" ]]', workflow)
        self.assertIn('data.get("code_sha") != os.environ["GITHUB_SHA"]', workflow)
        self.assertIn('data.get("run_id") != os.environ["GITHUB_RUN_ID"]', workflow)
        self.assertIn('data.get("block_start") != expected_start', workflow)
        self.assertIn('data.get("blocks") != expected_count', workflow)
        self.assertIn("pattern: shard-*", workflow)
        self.assertIn("name: shard-", workflow)
        self.assertIn("python3 scripts/paired_eval_stats.py", workflow)
        self.assertIn("if-no-files-found: warn", workflow)

    def test_same_time_claim_is_not_made(self):
        self.assertIn("NOT a fixed-time comparison", self.workflow)
        self.assertIn("NOT same-time 10s proof", self.workflow)


if __name__ == "__main__":
    unittest.main()
