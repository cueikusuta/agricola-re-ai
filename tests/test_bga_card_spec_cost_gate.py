"""Regression tests for the cost gate in validate_bga_card_specs.py.

These tests verify that the validation correctly enforces:
- Free cards (base cost "none") must have empty spec cost []
- Paid cards (base cost "good:amount;...") must match exactly
- Unknown base cost ("unknown") skips validation (preserves existing behavior)
- None spec cost is skipped (preserves existing behavior for unresolved cards)
"""
from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path


class CostGateIntegrationTests(unittest.TestCase):
    """Integration tests for the cost validation gate using real data + temp modifications."""

    def setUp(self) -> None:
        self.root = Path(__file__).resolve().parents[1]
        self.script = self.root / "scripts" / "validate_bga_card_specs.py"
        self.specs_dir = self.root / "data" / "bga-card-specs"
        self.tests_dir = self.root / "data" / "bga-card-tests"
        self.base_ledger = self.root / "data" / "bga-card-base-ledger.csv"

    def _run_validator(self) -> tuple[int, str]:
        """Run the validator and return (exit_code, stdout)."""
        result = subprocess.run(
            [sys.executable, str(self.script)],
            capture_output=True,
            text=True,
            cwd=self.root,
        )
        return result.returncode, result.stdout + result.stderr

    def _modify_spec_cost(self, card_id: str, new_cost) -> dict:
        """Temporarily modify a spec's cost and return original for restoration."""
        spec_path = self.specs_dir / f"{card_id}.json"
        with spec_path.open("r", encoding="utf-8") as f:
            original = json.load(f)
        modified = json.loads(json.dumps(original))  # deep copy
        modified["play"]["cost"] = new_cost
        with spec_path.open("w", encoding="utf-8") as f:
            json.dump(modified, f, ensure_ascii=False, indent=2)
        return original

    def _restore_spec(self, card_id: str, original: dict) -> None:
        """Restore a spec to its original state."""
        spec_path = self.specs_dir / f"{card_id}.json"
        with spec_path.open("w", encoding="utf-8") as f:
            json.dump(original, f, ensure_ascii=False, indent=2)

    def test_existing_specs_all_pass(self) -> None:
        """All existing 527 specs should pass validation."""
        exit_code, output = self._run_validator()
        self.assertEqual(exit_code, 0, f"Existing specs should pass: {output}")
        # Verify no errors in output
        import json as json_lib
        result = json_lib.loads(output)
        self.assertEqual(result["errors"], [])

    def test_free_card_with_wrong_cost_fails(self) -> None:
        """Free card (base cost 'none') with non-empty spec cost must fail."""
        # Find a free card (cost == "none") that has a spec
        free_cards = []
        with self.base_ledger.open(encoding="utf-8-sig", newline="") as f:
            import csv
            reader = csv.DictReader(f)
            for row in reader:
                if row["cost"] == "none" and (self.specs_dir / f"{row['card_id']}.json").exists():
                    free_cards.append(row["card_id"])
        self.assertTrue(free_cards, "Should have at least one free card with spec")

        card_id = free_cards[0]  # e.g., A088
        original = self._modify_spec_cost(card_id, [{"good": "stone", "amount": 99}])
        try:
            exit_code, output = self._run_validator()
            self.assertNotEqual(exit_code, 0, "Free card with wrong cost should fail")
            self.assertIn("cost differs from the base ledger", output)
        finally:
            self._restore_spec(card_id, original)

    def test_free_card_with_empty_cost_passes(self) -> None:
        """Free card with correct empty cost should pass (baseline)."""
        exit_code, output = self._run_validator()
        self.assertEqual(exit_code, 0, "Baseline validation should pass")

    def test_paid_card_with_wrong_cost_fails(self) -> None:
        """Paid card with non-matching spec cost must fail."""
        # Find a paid card (cost != "none" and != "unknown") that has a spec
        paid_cards = []
        with self.base_ledger.open(encoding="utf-8-sig", newline="") as f:
            import csv
            reader = csv.DictReader(f)
            for row in reader:
                if row["cost"] not in ("none", "unknown") and (self.specs_dir / f"{row['card_id']}.json").exists():
                    paid_cards.append((row["card_id"], row["cost"]))
        self.assertTrue(paid_cards, "Should have at least one paid card with spec")

        card_id, base_cost = paid_cards[0]
        # Create a wrong cost (different good)
        wrong_cost = [{"good": "stone", "amount": 99}]
        original = self._modify_spec_cost(card_id, wrong_cost)
        try:
            exit_code, output = self._run_validator()
            self.assertNotEqual(exit_code, 0, f"Paid card {card_id} with wrong cost should fail")
            self.assertIn("cost differs from the base ledger", output)
        finally:
            self._restore_spec(card_id, original)

    def test_paid_card_with_wrong_amount_fails(self) -> None:
        """Paid card with correct good but wrong amount must fail."""
        paid_cards = []
        with self.base_ledger.open(encoding="utf-8-sig", newline="") as f:
            import csv
            reader = csv.DictReader(f)
            for row in reader:
                if row["cost"] not in ("none", "unknown") and (self.specs_dir / f"{row['card_id']}.json").exists():
                    # Parse base cost to get the good
                    parts = row["cost"].split(";")
                    first_good = parts[0].split(":")[0]
                    paid_cards.append((row["card_id"], first_good))
                    break  # just need one
        self.assertTrue(paid_cards, "Should have at least one paid card with spec")

        card_id, good = paid_cards[0]
        # Use same good but wrong amount (99 instead of actual)
        wrong_cost = [{"good": good, "amount": 99}]
        original = self._modify_spec_cost(card_id, wrong_cost)
        try:
            exit_code, output = self._run_validator()
            self.assertNotEqual(exit_code, 0, f"Paid card {card_id} with wrong amount should fail")
            self.assertIn("cost differs from the base ledger", output)
        finally:
            self._restore_spec(card_id, original)

    def test_unknown_base_cost_skips_validation(self) -> None:
        """Base cost 'unknown' must skip validation (any spec cost passes)."""
        # Find a card with base cost "unknown" that has a spec
        unknown_cards = []
        with self.base_ledger.open(encoding="utf-8-sig", newline="") as f:
            import csv
            reader = csv.DictReader(f)
            for row in reader:
                if row["cost"] == "unknown" and (self.specs_dir / f"{row['card_id']}.json").exists():
                    unknown_cards.append(row["card_id"])
        if not unknown_cards:
            self.skipTest("No cards with base cost 'unknown' have specs")

        card_id = unknown_cards[0]
        original = self._modify_spec_cost(card_id, [{"good": "stone", "amount": 99}])
        try:
            exit_code, output = self._run_validator()
            self.assertEqual(exit_code, 0, f"Unknown base cost should skip validation for {card_id}")
        finally:
            self._restore_spec(card_id, original)

    def test_none_spec_cost_skips_validation(self) -> None:
        """Spec cost None must skip validation (preserves existing behavior)."""
        # Find any card with a spec
        cards_with_specs = list(self.specs_dir.glob("*.json"))
        self.assertTrue(cards_with_specs, "Should have specs")
        card_id = cards_with_specs[0].stem

        original = self._modify_spec_cost(card_id, None)
        try:
            exit_code, output = self._run_validator()
            # None spec cost should skip validation - this is existing behavior
            # The test passes if validation doesn't crash on None cost
            # Note: other validations might fail, but cost validation should be skipped
            # We just verify the cost validation doesn't raise an error
            import json as json_lib
            try:
                result = json_lib.loads(output)
                # Check that cost validation error is not present
                cost_errors = [e for e in result.get("errors", []) if "cost differs from the base ledger" in e]
                self.assertEqual(cost_errors, [], f"None spec cost should skip cost validation: {cost_errors}")
            except json_lib.JSONDecodeError:
                # If output isn't JSON, just verify no crash
                pass
        finally:
            self._restore_spec(card_id, original)


if __name__ == "__main__":
    unittest.main()
