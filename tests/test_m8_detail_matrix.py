"""Regression tests for the M8 legal detail coverage registry (#208)."""
import copy
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_m8_detail_matrix import validate_matrix  # noqa: E402


class DetailMatrixContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.example = json.loads(
            (ROOT / "docs" / "m8-detail-test-matrix.json").read_text(encoding="utf-8")
        )

    def reject(self, change, message):
        document = copy.deepcopy(self.example)
        change(document)
        with self.assertRaisesRegex(ValueError, message):
            validate_matrix(document, ROOT)

    def test_actual_seven_category_registry_resolves_to_executable_rust_tests(self):
        result = validate_matrix(self.example, ROOT)
        self.assertEqual(result["categories"], 7)
        self.assertEqual(result["legal_tests"], 7)
        self.assertEqual(result["illegal_tests"], 7)
        self.assertEqual(result["distinct_rust_tests"], 14)
        self.assertEqual(result["status"], "pass")

    def test_missing_category_fails_closed(self):
        self.reject(lambda d: d["categories"].pop("free_actions"), "seven M8")

    def test_extra_category_fails_closed(self):
        self.reject(lambda d: d["categories"].update({"other": {}}), "seven M8")

    def test_missing_illegal_case_fails_closed(self):
        self.reject(
            lambda d: d["categories"]["sowing"].pop("illegal"),
            "exactly one legal and one illegal",
        )

    def test_unknown_test_function_fails_closed(self):
        self.reject(
            lambda d: d["categories"]["fencing"]["illegal"].update(
                {"test": "does_not_exist"}
            ),
            "missing #\\[test\\] function",
        )

    def test_missing_target_fails_closed(self):
        self.reject(
            lambda d: d["categories"]["animals"]["legal"].update(
                {"target": "not_a_test_target"}
            ),
            "missing Rust test target",
        )

    def test_reused_test_cannot_fill_two_categories(self):
        self.reject(
            lambda d: d["categories"]["animals"].update(
                {"legal": copy.deepcopy(d["categories"]["fencing"]["legal"])}
            ),
            "reused test",
        )

    def test_path_traversal_and_unknown_fields_are_rejected(self):
        self.reject(
            lambda d: d["categories"]["fencing"]["legal"].update({"target": "../bad"}),
            "invalid Rust test target",
        )
        self.reject(
            lambda d: d["categories"]["plowing"]["illegal"].update({"extra": 1}),
            "require target and test",
        )

    def test_wrong_schema_and_empty_description_are_rejected(self):
        self.reject(lambda d: d.update({"schema_version": 2}), "unsupported matrix")
        self.reject(lambda d: d.update({"description": " "}), "description")


if __name__ == "__main__":
    unittest.main()
