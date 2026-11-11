"""Focused regression contracts for the 2026-11-09 independent card review.

These are source-to-spec consistency checks, NOT card-engine E2E tests.
"""
from __future__ import annotations

import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPECS = ROOT / "data" / "bga-card-specs"
TESTS = ROOT / "data" / "bga-card-tests"


def read(directory: Path, card: str) -> dict:
    return json.loads((directory / f"{card}.json").read_text(encoding="utf-8"))


class ReviewedCardContracts(unittest.TestCase):
    def test_night_loot_takes_exactly_two_distinct_materials(self) -> None:
        spec, tests = read(SPECS, "E005"), read(TESTS, "E005")
        params = spec["effects"][0]["steps"][0]["params"]
        self.assertEqual(params["count"], 2)
        self.assertTrue(params["distinct_types"])
        normal = next(c for c in tests["cases"] if c["id"] == "E005-normal")
        self.assertEqual(normal["expected"]["wood_delta"], 1)
        self.assertEqual(normal["expected"]["clay_delta"], 1)
        self.assertEqual(normal["expected"]["forest_wood_after"], 1)
        self.assertEqual(normal["expected"]["clay_pit_clay_after"], 0)

    def test_barter_hut_accepts_mixed_and_repeated_exchanges(self) -> None:
        spec, tests = read(SPECS, "E009"), read(TESTS, "E009")
        options = spec["effects"][0]["steps"][0]["params"]
        self.assertEqual(options["max_uses"], 2)
        self.assertTrue(options["allow_repeat_option"])
        for amount, choice in zip((2, 3, 4), options["options"]):
            payment = choice["give"][0]
            self.assertEqual(payment["total_count"], amount)
            self.assertTrue(payment["allow_mixed"])
            self.assertCountEqual(payment["allowed_goods"], ["wood", "clay", "reed", "stone"])
        ids = {c["id"] for c in tests["cases"]}
        self.assertTrue({"E009-mixed-2", "E009-double-same-option",
                         "E009-third-exchange-rejected"} <= ids)

    def test_shelter_stable_must_be_inside_single_cell_pasture(self) -> None:
        spec, tests = read(SPECS, "A001"), read(TESTS, "A001")
        params = spec["effects"][0]["steps"][0]["params"]
        self.assertEqual(params["wood_cost"], 0)
        self.assertEqual(params["placement_scope"], "one_cell_pasture")
        self.assertEqual(params["pasture_size"], 1)
        self.assertIn("A001-outside-pasture", {c["id"] for c in tests["cases"]})

    def test_beer_tap_has_shared_grain_choice(self) -> None:
        spec = read(SPECS, "D062")
        feed = spec["effects"][1]["steps"][0]["params"]
        self.assertEqual(feed["kind"], "choice")
        self.assertEqual(feed["max_uses"], 1)
        self.assertEqual(
            [(x["give"][0]["amount"], x["receive"][0]["amount"]) for x in feed["options"]],
            [(2, 3), (3, 6), (4, 9)],
        )

    def test_ox_skull_source_conflict_is_fail_closed(self) -> None:
        spec = read(SPECS, "E037")
        self.assertEqual(spec["review"]["status"], "needs_resolution")
        self.assertIn("on_play_food_ja_en_discrepancy",
                      spec["review"]["unresolved_fields"])


if __name__ == "__main__":
    unittest.main()
