"""Unit tests for the A-1 card inventory (ledger presence and Rust coverage)."""
import unittest
from pathlib import Path

from scripts.card_inventory_report import (
    QUEUE_FIELDS,
    UNIMPLEMENTED,
    categories,
    inventory,
    is_implemented,
    load_rows,
    normalize_type,
    verification_queue,
    validate_queue_paths,
    write_queue_csv,
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
        # A familiar prefix with an unknown suffix must not be counted as Rust implementation.
        with self.assertRaisesRegex(ValueError, "unknown implementation_reference"):
            is_implemented("コード内に定義あり（未確認の新しい状態）")

    def test_unknown_card_type_is_rejected(self):
        self.assertEqual(normalize_type("職業"), "職業")
        self.assertEqual(normalize_type("小進歩  大進歩"), "multi_type")
        with self.assertRaisesRegex(ValueError, "unknown card type"):
            normalize_type("大進歩")

    def test_duplicate_card_id_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate or empty card_id"):
            inventory([row(), row()])

    def test_malformed_deck_and_base_flag_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "invalid deck/card_id pairing"):
            inventory([row(card_id="A001", deck="B")])
        with self.assertRaisesRegex(ValueError, "invalid in_base_2016"):
            changed = row()
            changed["in_base_2016"] = "UNKNOWN"
            inventory([changed])

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


class VerificationQueueTests(unittest.TestCase):
    """The per-card queue is a work list for human review, never a verification."""

    def test_every_queue_row_demands_official_review(self):
        rows = [row(difficulty="1"), row(card_id="A002", difficulty="2")]
        queue = verification_queue(rows)
        self.assertEqual([entry["card_id"] for entry in queue], ["A001", "A002"])
        for entry in queue:
            self.assertTrue(entry["official_review_required"])
            self.assertFalse(entry["distributable_now"])
            self.assertEqual(entry["implementation_state"], "unimplemented")

    def test_implemented_cards_are_not_in_the_queue(self):
        rows = [
            row(card_id="A001", difficulty="1"),
            row(card_id="A002", difficulty="1",
                reference="コード内に定義あり（配布対象要照合）"),
        ]
        queue = verification_queue(rows)
        self.assertEqual([entry["card_id"] for entry in queue], ["A001"])

    def test_the_queue_is_ordered_by_difficulty_then_card_id(self):
        rows = [
            row(card_id="A003", difficulty="3"),
            row(card_id="A002", difficulty="2"),
            row(card_id="A001", difficulty="2"),
            row(card_id="A004", difficulty="4"),
        ]
        queue = verification_queue(rows)
        self.assertEqual(
            [entry["card_id"] for entry in queue], ["A001", "A002", "A003"])
        self.assertEqual(
            [entry["difficulty_1_to_5_preliminary"] for entry in queue],
            ["2", "2", "3"],
        )

    def test_the_difficulty_limit_is_adjustable_and_validated(self):
        rows = [row(card_id="A001", difficulty="1"), row(card_id="A002", difficulty="3")]
        self.assertEqual(len(verification_queue(rows, max_difficulty=1)), 1)
        with self.assertRaisesRegex(ValueError, "max_difficulty"):
            verification_queue(rows, max_difficulty=0)
        with self.assertRaisesRegex(ValueError, "max_difficulty"):
            verification_queue(rows, max_difficulty=6)

    def test_the_real_ledger_queue_is_large_and_unverified(self):
        rows = load_rows(LEDGER)
        queue = verification_queue(rows)
        self.assertEqual(len(queue), 559)
        self.assertTrue(all(entry["official_review_required"] for entry in queue))
        self.assertTrue(all(not entry["distributable_now"] for entry in queue))
        self.assertTrue(all(
            int(entry["difficulty_1_to_5_preliminary"]) <= 3 for entry in queue))

    def test_queue_output_must_not_overwrite_ledger_or_json_report(self):
        with self.assertRaisesRegex(ValueError, "--ledger"):
            validate_queue_paths(
                Path("data/../ledger.csv"), None, Path("ledger.csv")
            )
        with self.assertRaisesRegex(ValueError, "--output"):
            validate_queue_paths(
                Path("ledger.csv"), Path("out/../report.json"), Path("report.json")
            )
        # Different paths are valid, including no CSV export.
        validate_queue_paths(Path("ledger.csv"), Path("report.json"), Path("queue.csv"))
        validate_queue_paths(Path("ledger.csv"), Path("report.json"), None)

    def test_writing_the_queue_csv_keeps_the_review_flags(self):
        import csv
        import tempfile

        queue = verification_queue([row(card_id="A001", difficulty="1")])
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "queue.csv"
            write_queue_csv(queue, path)
            with path.open(encoding="utf-8", newline="") as file:
                written = list(csv.DictReader(file))
        self.assertEqual(len(written), 1)
        self.assertEqual(written[0]["official_review_required"], "True")
        self.assertEqual(written[0]["distributable_now"], "False")
        self.assertEqual(list(written[0].keys()), list(QUEUE_FIELDS))


if __name__ == "__main__":
    unittest.main()
