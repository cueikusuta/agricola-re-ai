"""Guard the player-count eligibility rule (Issue #202).

The BGA base ledger resolves ``eligible_2/3/4`` from the official card-number
rule for decks A-D and leaves deck E unknown.  These tests fail when the ledger
drifts from the rule, when the basis columns are lost, or when the script stops
being idempotent.
"""
from __future__ import annotations

import csv
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "data" / "bga-card-base-ledger.csv"
RULEBOOK = "https://shop.asmodee.ca/img/product/description/EN-AGRICOLA-CONSUL_DIRIGENS-RULES.pdf"
RANGES = [
    (1, 84, "yes", "yes", "yes"),
    (85, 126, "yes", "yes", "yes"),
    (127, 147, "no", "yes", "yes"),
    (148, 168, "no", "no", "yes"),
]
RULE_DECKS = {"A", "B", "C", "D"}


def load_rows() -> list[dict]:
    with BASE.open(encoding="utf-8-sig", newline="") as handle:
        return list(csv.DictReader(handle))


def expected(number: int):
    for first, last, two, three, four in RANGES:
        if first <= number <= last:
            return two, three, four
    return None


def load_script():
    spec = importlib.util.spec_from_file_location(
        "apply_card_eligibility", ROOT / "scripts" / "apply_card_eligibility.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class EligibilityTest(unittest.TestCase):
    def setUp(self) -> None:
        self.rows = load_rows()

    def test_ledger_matches_the_card_number_rule(self) -> None:
        self.assertEqual(len(self.rows), 840)
        for row in self.rows:
            card_id = row["card_id"]
            number = int(card_id[1:])
            if row["deck"] in RULE_DECKS:
                two, three, four = expected(number)
                self.assertEqual(
                    (row["eligible_2"], row["eligible_3"], row["eligible_4"]),
                    (two, three, four),
                    f"{card_id}: eligibility drifted from the rule",
                )
                self.assertEqual(row["eligibility_basis"], "consul_dirigens_rule")
                self.assertEqual(row["eligibility_source_url"], RULEBOOK)
            else:
                self.assertEqual(row["eligibility_basis"], "unknown", card_id)
                self.assertEqual(row["eligibility_source_url"], "", card_id)
                self.assertEqual(
                    (row["eligible_2"], row["eligible_3"], row["eligible_4"]),
                    ("unknown", "unknown", "unknown"),
                    f"{card_id}: deck E must stay unknown",
                )

    def test_card_number_matches_the_card_kind(self) -> None:
        for row in self.rows:
            number = int(row["card_id"][1:])
            kind = row["kind"]
            if number <= 84:
                self.assertIn(kind, ("minor_improvement", "multi_type"), row["card_id"])
            else:
                self.assertEqual(kind, "occupation", row["card_id"])

    def test_rulebook_url_is_recorded(self) -> None:
        bases = {row["eligibility_basis"] for row in self.rows}
        self.assertEqual(bases, {"consul_dirigens_rule", "unknown"})
        self.assertTrue(RULEBOOK.startswith("https://"))

    def test_script_reapply_is_idempotent(self) -> None:
        script = load_script()
        before = load_rows()
        code = script.main(["apply_card_eligibility.py", "--check"])
        self.assertEqual(code, 0)
        after = load_rows()
        self.assertEqual(
            json.dumps(before, ensure_ascii=False), json.dumps(after, ensure_ascii=False)
        )

    def test_spec_snapshots_follow_the_ledger(self) -> None:
        by_id = {row["card_id"]: row for row in self.rows}
        for path in sorted((ROOT / "data" / "bga-card-specs").glob("*.json")):
            item = json.loads(path.read_text(encoding="utf-8"))
            row = by_id[item["card_id"]]
            self.assertEqual(
                item["bga"]["eligibility"],
                {
                    "2": row["eligible_2"],
                    "3": row["eligible_3"],
                    "4": row["eligible_4"],
                },
                f"{item['card_id']}: spec eligibility differs from the base ledger",
            )


if __name__ == "__main__":
    unittest.main()
