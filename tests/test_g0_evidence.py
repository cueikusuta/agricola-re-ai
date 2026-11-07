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


if __name__ == "__main__":
    unittest.main()
