"""Guard the BGA card-spec builder (Issue #202).

The committed ``data/bga-card-specs`` / ``data/bga-card-tests`` files are the
deliverable; ``scripts/build_bga_card_specs.py`` is the reproducible authoring
form.  These tests fail when the two drift apart, when a table entry loses its
test contract, or when the contract validator reports an error.
"""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC_DIR = ROOT / "data" / "bga-card-specs"
TEST_DIR = ROOT / "data" / "bga-card-tests"


def load_builder():
    spec = importlib.util.spec_from_file_location(
        "build_bga_card_specs", ROOT / "scripts" / "build_bga_card_specs.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class BuilderTest(unittest.TestCase):
    def setUp(self) -> None:
        self.builder = load_builder()

    def test_regeneration_matches_committed_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.builder.SPECS = Path(tmp) / "specs"
            self.builder.TESTS = Path(tmp) / "tests"
            code = self.builder.main(["build_bga_card_specs.py", "--force"])
            self.assertEqual(code, 0)
            for card_id in self.builder.CARDS:
                for built, committed in (
                    (self.builder.SPECS / f"{card_id}.json", SPEC_DIR / f"{card_id}.json"),
                    (self.builder.TESTS / f"{card_id}.json", TEST_DIR / f"{card_id}.json"),
                ):
                    self.assertTrue(built.exists(), f"{card_id}: builder produced no file")
                    self.assertTrue(
                        committed.exists(), f"{card_id}: committed file is missing"
                    )
                    self.assertEqual(
                        json.loads(built.read_text(encoding="utf-8")),
                        json.loads(committed.read_text(encoding="utf-8")),
                        f"{card_id}: committed file differs from the builder output",
                    )

    def test_every_card_has_referenced_tests(self) -> None:
        for card_id, entry in self.builder.CARDS.items():
            ids = [case["id"] for case in entry["tests"]]
            self.assertTrue(ids, f"{card_id}: no test cases")
            self.assertEqual(len(ids), len(set(ids)), f"{card_id}: duplicate test id")
            for case in entry["tests"]:
                self.assertTrue(
                    case["id"].startswith(card_id), f"{card_id}: test id {case['id']}"
                )
                self.assertIn(
                    case["scope"],
                    (
                        "effect_only",
                        "legal_play",
                        "integration",
                        "saved_replay",
                        "privacy",
                    ),
                )
            self.assertEqual(
                sorted(ids),
                sorted(
                    json.loads(
                        (TEST_DIR / f"{card_id}.json").read_text(encoding="utf-8")
                    )["cases"][i]["id"]
                    for i in range(len(ids))
                ),
            )

    def test_contract_validator_passes(self) -> None:
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "validate_bga_card_specs.py")],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
