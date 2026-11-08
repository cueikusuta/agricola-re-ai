"""Fail-closed tests for the G0 evidence generator (#167 C review P1)."""
import json
import tempfile
import unittest
from pathlib import Path

from scripts.g0_evidence import build, normalise_card_mode, sha256_hex


def rows_jsonl(blocks=2, players=2, first_seed=120, block_offset=0):
    lines = []
    for block in range(block_offset, block_offset + blocks):
        for arm in ("candidate", "baseline"):
            for seat in range(players):
                lines.append(json.dumps({
                    "block_index": block,
                    "seed": first_seed + block,
                    "arm_id": arm,
                    "candidate_seat": seat,
                }))
    return "\n".join(lines) + "\n"


class G0EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)

    def tearDown(self):
        self.folder.cleanup()

    def write(self, name, text):
        path = self.root / name
        path.write_text(text, encoding="utf-8")
        return path

    def generate(self, matches, **overrides):
        arguments = {
            "mode": "cardless_2p",
            "players": 2,
            "card_mode": "none",
            "matches": matches if isinstance(matches, list) else [matches],
            "artifact_url": "https://example.invalid/run/1",
            "code_sha": "abc123",
            "checks": None,
        }
        arguments.update(overrides)
        return build(**arguments)

    def test_digest_is_computed_from_the_artifact(self):
        matches = self.write("matches.jsonl", rows_jsonl())
        evidence = self.generate(matches)
        expected = sha256_hex(matches.read_bytes())
        self.assertEqual(evidence["modes"][0]["matches_sha256"], expected)
        self.assertEqual(evidence["schema_version"], 2)
        self.assertEqual(evidence["modes"][0]["card_mode"], "none")

    def test_a_sharded_run_hashes_the_concatenation(self):
        first = self.write("a.jsonl", rows_jsonl(blocks=1))
        # Shards must cover *distinct* global blocks; reusing block 0 in a second
        # shard is a duplicate, not a second shard.
        second = self.write("b.jsonl", rows_jsonl(blocks=1, first_seed=200, block_offset=1))
        evidence = self.generate([first, second])
        expected = sha256_hex(first.read_bytes() + second.read_bytes())
        self.assertEqual(evidence["modes"][0]["matches_sha256"], expected)
        self.assertEqual(len(evidence["modes"][0]["matches_paths"]), 2)

    def test_duplicate_block_arm_seat_rows_are_rejected(self):
        text = rows_jsonl() + json.dumps({
            "block_index": 0, "seed": 120, "arm_id": "candidate", "candidate_seat": 0,
        }) + "\n"
        matches = self.write("dup.jsonl", text)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.generate(matches)

    def test_unverified_card_modes_are_rejected(self):
        matches = self.write("matches.jsonl", rows_jsonl())
        with self.assertRaisesRegex(ValueError, "not verified"):
            self.generate(matches, card_mode="experimental_pool")

    def test_verified_spellings_are_normalised(self):
        self.assertEqual(normalise_card_mode("verified_basic_2p"), "verified_basic")
        self.assertEqual(normalise_card_mode("verified"), "verified_basic")
        self.assertEqual(normalise_card_mode("none"), "none")
        matches = self.write("matches.jsonl", rows_jsonl())
        evidence = self.generate(matches, card_mode="verified_basic_2p")
        self.assertEqual(evidence["modes"][0]["card_mode"], "verified_basic")

    def test_no_matches_file_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "at least one"):
            self.generate([])

    def test_checks_file_digest_is_recorded_when_bound(self):
        matches = self.write("matches.jsonl", rows_jsonl())
        checks = self.write("checks.json", json.dumps({"schema_version": 2}))
        evidence = self.generate(matches, checks=checks)
        self.assertEqual(
            evidence["modes"][0]["checks_sha256"], sha256_hex(checks.read_bytes()))
        self.assertEqual(evidence["modes"][0]["checks_path"], str(checks))

    def test_the_evidence_never_contains_a_declared_scale(self):
        # Regression for the review: the old format let a human type
        # `games_per_arm: 1000`. The generated evidence must not carry it.
        matches = self.write("matches.jsonl", rows_jsonl())
        evidence = self.generate(matches)
        text = json.dumps(evidence)
        for forbidden in ("games_per_arm", "anomalies", "replay_mismatches",
                          "legal_action_violations"):
            self.assertNotIn(forbidden, text)



class AutoGateWorkflowContractTests(unittest.TestCase):
    """Regression guard: heavy jobs run only on relevant main pushes or dispatch."""

    @staticmethod
    def workflow(name):
        return (Path(__file__).resolve().parents[1] /
                ".github" / "workflows" / name).read_text(encoding="utf-8")

    def test_g0_main_push_and_manual_dispatch_are_both_supported(self):
        content = self.workflow("g0-gate.yml")
        self.assertIn("  push:\n    branches: [main]\n    paths:", content)
        self.assertIn("  workflow_dispatch:", content)
        self.assertNotIn("  pull_request:", content)
        self.assertIn("'!src/web.rs'", content)
        self.assertIn("'!src/draft*.rs'", content)
        self.assertIn("'src/bin/dataset_split.rs'", content)
        self.assertIn("'scripts/g0_evidence.py'", content)
        self.assertIn("  cancel-in-progress: false", content)

    def test_g0_push_defaults_preserve_manual_invalid_values(self):
        content = self.workflow("g0-gate.yml")
        defaults = {
            "EPISODES": ("episodes", "1000"),
            "DATASET_SHARDS": ("dataset_shards", "4"),
            "DATASET_POLICY": ("dataset_policy", "eval"),
            "DATASET_SEED_START": ("dataset_seed_start", "920000"),
            "TRAIN_RANGE": ("train_range", "920000:920249"),
            "VALIDATION_RANGE": ("validation_range", "920250:920399"),
            "TEST_RANGE": ("test_range", "920400:920999"),
            "PLAYERS": ("players", "2"),
            "BLOCKS": ("blocks", "500"),
            "EVAL_SEED_START": ("eval_seed_start", "920500"),
            "CANDIDATE": ("candidate", "eval"),
            "BASELINE": ("baseline", "random"),
            "OPPONENT": ("opponent", "greedy"),
            "CARDS": ("cards", "none"),
            "EXTRA": ("extra", "false"),
            "SEARCH_MODE": ("search_mode", "simulation-capped"),
            "DECISION_BUDGET_MS": ("decision_budget_ms", "0"),
        }
        for env_name, (input_name, default_value) in defaults.items():
            with self.subTest(name=env_name):
                expected = (f"{env_name}: ${{{{ github.event_name == 'push' "
                            f"&& '{default_value}' || inputs.{input_name} }}}}")
                self.assertIn(expected, content)
                self.assertNotIn(f"inputs.{input_name} ||", content)
        self.assertIn("--trusted-current-run", content)
        self.assertIn('test "$SEARCH_MODE" == "simulation-capped"', content)
        self.assertIn('test "$DECISION_BUDGET_MS" -eq 0', content)

    def test_six_mode_main_push_keeps_manual_override_and_fail_closed(self):
        content = self.workflow("mode-verification.yml")
        self.assertIn("  push:\n    branches: [main]\n    paths:", content)
        self.assertIn("  workflow_dispatch:", content)
        self.assertNotIn("  pull_request:", content)
        self.assertIn("'src/bin/mode_verify.rs'", content)
        self.assertIn("'!src/web.rs'", content)
        for env_name, input_name, default_value in (
            ("SEEDS", "seeds", "1000"),
            ("SEED_START", "seed_start", "700000"),
            ("POLICY", "policy", "eval"),
        ):
            with self.subTest(name=env_name):
                expected = (f"{env_name}: ${{{{ github.event_name == 'push' "
                            f"&& '{default_value}' || inputs.{input_name} }}}}")
                self.assertEqual(content.count(expected), 2)
        self.assertIn("needs.verify.result", content)
        self.assertIn("if: always()", content)

    def test_long_running_ai_comparisons_are_not_unconditionally_push_triggered(self):
        content = self.workflow("paired-eval.yml")
        self.assertIn("  workflow_dispatch:", content)
        self.assertNotIn("  push:", content)


if __name__ == "__main__":
    unittest.main()
