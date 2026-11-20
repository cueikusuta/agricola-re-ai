"""Focused review checks for Issue #226 A091–A114 (schema-level only)."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def get(card: str, kind: str = "bga-card-specs") -> dict:
    path = ROOT / "data" / kind / f"{card}.json"
    return json.loads(path.read_text(encoding="utf-8"))


class Batch2TimingRegression(unittest.TestCase):
    def test_angler_before_collection(self) -> None:
        card = get("A095")
        self.assertEqual(card["effects"][0]["snapshot"], "before_event")
        cases = {c["id"]: c for c in get("A095", "bga-card-tests")["cases"]}
        denied = cases["A095-not-triggered"]
        self.assertEqual(denied["setup"]["space_food_before_use"], 3)
        self.assertEqual(denied["setup"]["space_food_after_use"], 0)
        self.assertEqual(denied["expected"]["major_or_minor_improvement_action_granted"], 0)

    def test_catcher_uses_pre_collection_pile(self) -> None:
        effects = get("A107")["effects"]
        self.assertEqual([e["snapshot"] for e in effects],
                         ["before_event", "before_event", "before_event"])
        cases = {c["id"]: c for c in get("A107", "bga-card-tests")["cases"]}
        self.assertEqual(cases["A107-second-worker-correct"]["expected"]["owner_food_delta"], 1)

    def test_wall_builder_consumes_all_own_markers(self) -> None:
        step = get("A111")["effects"][1]["steps"][0]
        self.assertEqual(step["params"]["kind"], "take_all_goods_from_round_space")
        self.assertTrue(step["params"]["consume_markers"])
        self.assertEqual(step["params"]["amount"], "all")
        cases = {c["id"]: c for c in get("A111", "bga-card-tests")["cases"]}
        overlapping = cases["A111-overlapping-events"]
        self.assertEqual(overlapping["expected"]["owner_food_delta"], 2)
        self.assertEqual(overlapping["expected"]["food_round_markers_by_card_after"]["A111"], 0)

    def test_resolved_effects_are_filled_and_open_timing_stays_blocked(self) -> None:
        a092 = get("A092")
        self.assertEqual(a092["review"]["status"], "needs_resolution")
        self.assertTrue(a092["review"]["unresolved_fields"])
        self.assertIsNotNone(a092["effects"])
        self.assertEqual(a092["effects"][0]["steps"][0]["params"]["good"], "food")

        a103 = get("A103")
        # 2026-11 audit: positional vs cumulative tier reading is ambiguous in both the
        # Japanese and English wording, so the card stays fail-closed until it is decided
        # (see docs/card-data-3way-audit.md Q-03). The three-tier structure itself is fixed.
        self.assertEqual(a103["review"]["status"], "needs_resolution")
        self.assertEqual(len(a103["review"]["unresolved_fields"]), 1)
        self.assertIn("cumulative", a103["review"]["unresolved_fields"][0])
        self.assertEqual(len(a103["effects"]), 3)
        self.assertEqual([e["steps"][0]["params"]["good"] for e in a103["effects"]],
                         ["vegetable", "grain", "reed"])


if __name__ == "__main__":
    unittest.main()
