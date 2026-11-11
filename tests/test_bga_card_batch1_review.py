"""Focused schema-level regressions for Issue #226 batch 1 review fixes.

These contracts are not substitutes for an executable card-effects engine.
"""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def data(kind: str, card: str) -> dict:
    return json.loads((ROOT / "data" / kind / f"{card}.json").read_text(encoding="utf-8"))


class Batch1ReviewTest(unittest.TestCase):
    def test_work_certificate_uses_post_action_remaining_piles(self) -> None:
        spec = data("bga-card-specs", "A082")
        self.assertEqual(spec["effects"][0]["trigger"], "after_worker_space")
        self.assertEqual(spec["effects"][0]["snapshot"], "after_event")
        cases = {case["id"]: case for case in data("bga-card-tests", "A082")["cases"]}
        boundary = cases["A082-boundary"]
        self.assertEqual(boundary["setup"]["accumulation_counts"]["reed_bank"], 5)
        self.assertEqual(boundary["expected"]["reed_bank_count_after"], 0)
        self.assertEqual(boundary["expected"]["owner_reed_delta"], 5)
        self.assertEqual(boundary["expected"]["owner_wood_delta"], 1)
        self.assertEqual(cases["A082-normal-action-drains-space"]["expected"]
                         ["take_1_from_accumulation"], None)

    def test_clearing_spade_anytime_is_not_ready_for_engine(self) -> None:
        spec = data("bga-card-specs", "A071")
        self.assertEqual(spec["review"]["status"], "needs_resolution")
        self.assertIsNone(spec["effects"])
        self.assertIn("anytime_free_action_window_and_deterministic_replay",
                      spec["review"]["unresolved_fields"])


if __name__ == "__main__":
    unittest.main()
