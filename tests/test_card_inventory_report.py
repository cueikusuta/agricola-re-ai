"""Unit tests for the A-1 card inventory (ledger presence and Rust coverage)."""
import unittest
from pathlib import Path

from scripts.card_inventory_report import (
    UNIMPLEMENTED,
    categories,
    inventory,
    is_implemented,
    load_rows,
    normalize_type,
)

LEDGER = Path("docs/card-catalog-ae-preliminary.csv")


def row(card_id="A001", card_type="小進歩", difficulty="3",
        reference=UNIMPLEMENTED, effect="即時獲得", deck=None):
    return {
        "card_id": card_id,
        "deck": deck or card_id[0],
        "card_type": card_type,
        "difficulty_1_to_5_preliminary": difficulty,
        "implementation_reference": reference,
        "effect_categories_preliminary": effect,
        "in_base_2016": "NO",
    }


class LedgerShapeTests(unittest.TestCase):
    def test_the_real_ledger_has_the_expected_structure(self):
        report = inventory(load_rows(LEDGER))
        totals = report["totals"]
        self.assertEqual(totals["cards"], 840)
        self.assertEqual(totals["by_deck"], {deck: 168 for deck in "ABCDE"})
        self.assertEqual(totals["by_type"]["職業"], 420)
        self.assertEqual(totals["by_type"]["小進歩"], 416)
        self.assertEqual(totals["by_type"]["multi_type"], 4)
        self.assertEqual(totals["in_base_2016"], {"NO": 744, "YES": 96})
        self.assertFalse(report["official_verification_included"])

    def test_the_real_ledger_separates_implemented_from_unimplemented(self):
        report = inventory(load_rows(LEDGER))
        totals = report["totals"]
        self.assertEqual(totals["implemented_total"], 34)
        self.assertEqual(
            totals["implemented_total"] + totals["unimplemented_total"], 840)
        # Every ledger row is classified exactly once.
        self.assertEqual(
            sum(totals["implemented"].values()), totals["implemented_total"])
        self.assertEqual(
            sum(report["difficulty"]["implemented"].values()),
            totals["implemented_total"])
        self.assertEqual(
            sum(report["difficulty"]["unimplemented"].values()),
            totals["unimplemented_total"])

    def test_implemented_share_is_reported_per_type(self):
        report = inventory(load_rows(LEDGER))
        share = report["implemented_share_by_type"]
        self.assertAlmostEqual(share["職業"], 18 / 420)
        self.assertAlmostEqual(share["小進歩"], 16 / 416)
        self.assertEqual(share["multi_type"], 0.0)

    def test_shared_mechanism_candidates_are_ranked_descending(self):
        report = inventory(load_rows(LEDGER))
        candidates = report["shared_mechanism_candidates"]
        self.assertTrue(candidates)
        counts = [item["unimplemented_cards"] for item in candidates]
        self.assertEqual(counts, sorted(counts, reverse=True))
        for item in candidates:
            self.assertLessEqual(
                item["unimplemented_cards"],
                report["effect_categories"]["unimplemented"][item["category"]],
            )


class ClassificationTests(unittest.TestCase):
    def test_implementation_reference_is_fail_closed(self):
        self.assertFalse(is_implemented(UNIMPLEMENTED))
        self.assertTrue(is_implemented("コード内に定義あり（配布対象要照合）"))
        self.assertTrue(is_implemented("コード内に定義あり・2人戦プリセット外"))
        with self.assertRaisesRegex(ValueError, "unknown implementation_reference"):
            is_implemented("実装済み")

    def test_unknown_card_type_is_rejected(self):
        self.assertEqual(normalize_type("職業"), "職業")
        self.assertEqual(normalize_type("小進歩  大進歩"), "multi_type")
        with self.assertRaisesRegex(ValueError, "unknown card type"):
            normalize_type("大進歩")

    def test_duplicate_card_id_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate or empty card_id"):
            inventory([row(), row()])

    def test_invalid_difficulty_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "invalid preliminary difficulty"):
            inventory([row(difficulty="0")])

    def test_categories_split_on_the_fullwidth_separator(self):
        self.assertEqual(categories("即時獲得／建築・費用"), ["即時獲得", "建築・費用"])
        self.assertEqual(categories(""), [])
        # A half-width slash is part of a category name, not a separator.
        self.assertEqual(categories("資源交換・食料化"), ["資源交換・食料化"])

    def test_counts_are_split_by_implementation_state(self):
        rows = [
            row("A001", difficulty="1", reference="コード内に定義あり（配布対象要照合）"),
            row("A002", difficulty="5", effect="ラウンド効果／収穫"),
        ]
        report = inventory(rows)
        self.assertEqual(report["totals"]["implemented"], {"小進歩": 1})
        self.assertEqual(report["totals"]["unimplemented"], {"小進歩": 1})
        self.assertEqual(report["difficulty"]["implemented"]["1"], 1)
        self.assertEqual(report["difficulty"]["unimplemented"]["5"], 1)
        # Only unimplemented cards are proposed as shared-mechanism work.
        proposed = {item["category"] for item in report["shared_mechanism_candidates"]}
        self.assertEqual(proposed, set())

    def test_easy_unimplemented_categories_are_prioritised(self):
        rows = [
            row("A001", difficulty="2", effect="ラウンド効果"),
            row("A002", difficulty="3", effect="ラウンド効果"),
            row("A003", difficulty="5", effect="他プレイヤー"),
        ]
        report = inventory(rows)
        self.assertEqual(
            report["shared_mechanism_candidates"],
            [{"category": "ラウンド効果", "unimplemented_cards": 2}],
        )
        self.assertEqual(
            report["effect_categories"]["unimplemented_by_difficulty"]["4-5"],
            {"他プレイヤー": 1},
        )


if __name__ == "__main__":
    unittest.main()
