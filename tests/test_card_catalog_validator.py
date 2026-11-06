"""Unit tests for the independent, fail-closed card eligibility ledger."""
import unittest

from scripts.validate_card_catalog import review_ledger, stratified_audit_sample


def prelim(card_id="A001", card_type="小進歩", english="Shelter"):
    return {
        "card_id": card_id, "deck": card_id[0], "card_type": card_type,
        "name_ja": "雨除け", "name_en": english,
    }


def audit(card_id="A001", **overrides):
    row = {
        "card_id": card_id, "card_type_normalized": "minor_improvement",
        "eligible_at_2": "unverified", "eligible_at_3": "unverified",
        "eligible_at_4": "unverified", "review_status": "unverified",
        "reference_type": "secondary_catalog",
        "primary_reference_id": "", "primary_reference_url": "",
        "source_language": "", "ruleset_version": "2016-revised",
        "reviewed_by": "", "reviewed_on": "", "review_note": "",
    }
    row.update(overrides)
    return row


class CardCatalogAuditTests(unittest.TestCase):
    def test_preliminary_never_counts_as_verified(self):
        result, errors = review_ledger([prelim()], [audit()], require_full=False)
        self.assertEqual(errors, [])
        self.assertFalse(result["verified_4p_card_count_gate"])
        self.assertEqual(result["verified_eligible_by_players"]["4"]["minor_improvement"], 0)

    def test_missing_official_source_is_rejected(self):
        row = audit(review_status="verified", eligible_at_2="yes",
                    eligible_at_3="yes", eligible_at_4="yes")
        _, errors = review_ledger([prelim()], [row], require_full=False)
        self.assertTrue(any("official reference" in message for message in errors))
        self.assertTrue(any("primary_reference_id" in message for message in errors))

    def test_verified_official_record_counts_eligibility(self):
        row = audit(review_status="verified", reference_type="official_pdf",
                    primary_reference_id="appendix:A001",
                    primary_reference_url="https://lookout-spiele.de/supported-a001.pdf",
                    source_language="en", reviewed_by="reviewer",
                    reviewed_on="2026-11-06", eligible_at_2="no",
                    eligible_at_3="yes", eligible_at_4="yes")
        result, errors = review_ledger([prelim()], [row], require_full=False)
        self.assertEqual(errors, [])
        self.assertEqual(result["verified_eligible_by_players"]["2"]["minor_improvement"], 0)
        self.assertEqual(result["verified_eligible_by_players"]["4"]["minor_improvement"], 1)

    def test_duplicate_ids_are_rejected(self):
        _, errors = review_ledger([prelim(), prelim()], [audit()], require_full=False)
        self.assertTrue(any("duplicate ID" in message for message in errors))

    def test_invalid_type_and_eligibility_are_rejected(self):
        row = audit(card_type_normalized="occupation", eligible_at_4="yes")
        _, errors = review_ledger([prelim()], [row], require_full=False)
        self.assertTrue(any("normalized type" in message for message in errors))
        self.assertTrue(any("unverified card must not be marked eligible" in message for message in errors))

    def test_missing_audit_record_is_rejected(self):
        _, errors = review_ledger([prelim()], [], require_full=False)
        self.assertTrue(any("missing" in message for message in errors))

    def test_invalid_iso_date_is_rejected(self):
        _, errors = review_ledger(
            [prelim()], [audit(reviewed_on="2026-15-99")], require_full=False
        )
        self.assertTrue(any("ISO date" in message for message in errors))

    def test_reviewed_needs_official_is_not_eligible(self):
        row = audit(review_status="reviewed-needs-official", eligible_at_4="yes")
        _, errors = review_ledger([prelim()], [row], require_full=False)
        self.assertTrue(
            any("unconfirmed card must not be marked eligible" in message for message in errors)
        )

    def test_rejected_card_is_not_eligible(self):
        row = audit(review_status="rejected", eligible_at_4="yes")
        _, errors = review_ledger([prelim()], [row], require_full=False)
        self.assertTrue(
            any("unconfirmed card must not be marked eligible" in message for message in errors)
        )

    def test_verified_needs_english_name(self):
        row = audit(
            review_status="verified",
            reference_type="official_print",
            primary_reference_id="printed-card:A001:edition-2016",
            source_language="ja",
            reviewed_by="reviewer",
            reviewed_on="2026-11-06",
            eligible_at_2="yes",
            eligible_at_3="yes",
            eligible_at_4="yes",
        )
        _, errors = review_ledger([prelim(english="")], [row], require_full=False)
        self.assertTrue(any("English name" in message for message in errors))

    def test_structure_and_verification_are_reported_separately(self):
        result, errors = review_ledger([prelim()], [audit()], require_full=False)
        self.assertEqual(errors, [])
        self.assertTrue(result["structure_validation"]["ok"])
        self.assertFalse(
            result["official_verification_progress"]["official_verification_complete"]
        )
        self.assertEqual(result["official_verification_progress"]["unverified"], 1)
        self.assertEqual(result["reference_type_counts"]["secondary_catalog"], 1)
        self.assertFalse(result["g2_target_4p_80_each"])

    def test_high_risk_and_sample_queues_are_work_lists(self):
        result, _ = review_ledger([prelim("A005")], [audit("A005")], require_full=False)
        queue = {entry["card_id"] for entry in result["high_risk_audit_queue"]}
        self.assertIn("A005", queue)
        self.assertIn("A005", result["stratified_audit_sample"])

    def test_stratified_sample_is_deterministic_and_bounded(self):
        rows = []
        for deck in "AB":
            for index in range(1, 5):
                rows.append(
                    audit(
                        f"{deck}{index:03d}",
                        card_type_normalized="minor_improvement",
                    )
                )
        for index in range(5, 9):
            rows.append(audit(f"A{index:03d}", card_type_normalized="occupation"))
        sample = stratified_audit_sample(rows, 3)
        self.assertEqual(len(sample), 3)
        self.assertEqual(sample, stratified_audit_sample(rows, 3))
        self.assertEqual(len(set(sample)), 3)
        self.assertEqual(stratified_audit_sample(rows, 0), [])


if __name__ == "__main__":
    unittest.main()
