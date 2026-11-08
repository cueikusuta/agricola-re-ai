"""The blocked-card register must stay fail closed (#105 / #115)."""
import csv
import json
import tempfile
import unittest
from pathlib import Path

from scripts.card_verification_blockers import check, load_ledger, summary

REPOSITORY = Path(__file__).resolve().parent.parent
REQUIRED_BLOCKERS = ("A005", "D159", "A060", "D025", "D059", "D060")


def row(card_id, *, status="unverified", at4="unverified"):
    return {
        "card_id": card_id,
        "review_status": status,
        "eligible_at_2": "unverified",
        "eligible_at_3": "unverified",
        "eligible_at_4": at4,
    }


def register(cards):
    return {"schema_version": 1, "cards": cards}


def entry(card_id, **overrides):
    base = {
        "card_id": card_id,
        "unknown": ["official player eligibility"],
        "needed_source": "2016 revised printed card",
    }
    base.update(overrides)
    return base


class CardVerificationBlockerTests(unittest.TestCase):
    def test_the_committed_register_holds_against_the_committed_ledger(self):
        loaded = json.loads(
            (REPOSITORY / "docs" / "card-verification-blockers.json")
            .read_text(encoding="utf-8"))
        ledger = load_ledger(REPOSITORY / "docs" / "card-catalog-ae-audit.csv")
        self.assertEqual(check(loaded, ledger), [])

    def test_the_committed_register_covers_the_high_risk_cards(self):
        loaded = json.loads(
            (REPOSITORY / "docs" / "card-verification-blockers.json")
            .read_text(encoding="utf-8"))
        listed = {item["card_id"] for item in loaded["cards"]}
        for card_id in REQUIRED_BLOCKERS:
            self.assertIn(card_id, listed)

    def test_the_four_player_gate_is_closed_while_no_card_is_verified(self):
        loaded = json.loads(
            (REPOSITORY / "docs" / "card-verification-blockers.json")
            .read_text(encoding="utf-8"))
        ledger = load_ledger(REPOSITORY / "docs" / "card-catalog-ae-audit.csv")
        report = summary(loaded, ledger)
        self.assertFalse(report["four_player_pool_gate_open"])
        self.assertEqual(report["verified_eligible_at_4"], [])

    def test_a_blocked_card_cannot_become_verified(self):
        ledger = {"A005": row("A005", status="verified")}
        problems = check(register([entry("A005")]), ledger)
        self.assertTrue(any("still lists it as blocked" in p for p in problems), problems)

    def test_a_blocked_card_cannot_be_marked_eligible(self):
        ledger = {"A005": row("A005", at4="yes")}
        problems = check(register([entry("A005")]), ledger)
        self.assertTrue(any("eligible_at_4=yes" in p for p in problems), problems)

    def test_a_missing_card_is_reported(self):
        problems = check(register([entry("Z999")]), {})
        self.assertTrue(any("not in the audit ledger" in p for p in problems), problems)

    def test_an_empty_blocker_is_rejected(self):
        problems = check(register([entry("A005", unknown=[])]), {"A005": row("A005")})
        self.assertTrue(any("no unknown item" in p for p in problems), problems)

    def test_a_missing_source_is_rejected(self):
        problems = check(
            register([entry("A005", needed_source="")]), {"A005": row("A005")})
        self.assertTrue(any("no needed_source" in p for p in problems), problems)

    def test_duplicate_entries_are_rejected(self):
        problems = check(
            register([entry("A005"), entry("A005")]), {"A005": row("A005")})
        self.assertTrue(any("listed twice" in p for p in problems), problems)

    def test_an_unsupported_register_schema_is_rejected(self):
        loaded = {"schema_version": 99, "cards": [entry("A005")]}
        problems = check(loaded, {"A005": row("A005")})
        self.assertTrue(any("schema_version" in p for p in problems), problems)

    def test_a_verified_card_with_complete_metadata_opens_no_gate_by_itself(self):
        # One verified card is not the 40+40 pool; the summary must still report
        # the gate as closed unless verified eligible cards exist.
        ledger = {"A005": row("A005", status="verified", at4="no")}
        report = summary(register([]), ledger)
        self.assertEqual(report["verified_cards"], ["A005"])
        self.assertFalse(report["four_player_pool_gate_open"])


if __name__ == "__main__":
    unittest.main()
