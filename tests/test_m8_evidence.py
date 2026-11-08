"""Fail-closed tests for the M8 evidence producer (#108/#109/#110)."""
import json
import tempfile
import unittest
from pathlib import Path

from scripts.m8_evidence import (
    ATTESTED_CHECKS,
    EvidenceError,
    build_checks,
    build_tests,
    collect_statuses,
    evaluate_requirements,
    load_requirements,
    parse_log,
    report_digest,
)


def requirements_payload(tests_per_check=2):
    checks = {}
    for index, check in enumerate(ATTESTED_CHECKS):
        checks[check] = {
            "issue": 108 + index,
            "summary": "test fixture",
            "tests": [
                {
                    "id": f"target_{check}::test_{n}",
                    "mode": "cardless_2p",
                    "expectation": "the fixture test passes",
                    "basis": "rules_p8",
                }
                for n in range(tests_per_check)
            ],
        }
    return {"schema_version": 1, "checks": checks}


class M8EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)
        self.requirements_path = self.write_json(
            "requirements.json", requirements_payload())

    def tearDown(self):
        self.folder.cleanup()

    def write_json(self, name, payload):
        path = self.root / name
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def write_logs(self, statuses=None):
        """One log per target, every required test `ok` unless overridden."""
        statuses = statuses or {}
        log_dir = self.root / "logs"
        log_dir.mkdir(exist_ok=True)
        for check in ATTESTED_CHECKS:
            target = f"target_{check}"
            lines = []
            for n in range(2):
                name = f"test_{n}"
                status = statuses.get(f"{target}::{name}", "ok")
                lines.append(f"test {name} ... {status}")
            (log_dir / f"{target}.log").write_text("\n".join(lines) + "\n", encoding="utf-8")
        return log_dir

    def collect(self, log_dir, **overrides):
        arguments = {
            "requirements_path": self.requirements_path,
            "requirements": load_requirements(self.requirements_path),
            "code_sha": "sha1",
            "run_url": "https://github.com/o/r/actions/runs/7",
            "command": "cargo test",
        }
        statuses, targets = collect_statuses(log_dir)
        arguments.update(overrides)
        return build_tests(statuses=statuses, targets=targets, **arguments)

    def checks(self, tests, matches, **overrides):
        arguments = {
            "tests": tests,
            "requirements_path": self.requirements_path,
            "requirements": load_requirements(self.requirements_path),
            "matches": matches if isinstance(matches, list) else [matches],
            "code_sha": "sha1",
            "run_url": "https://github.com/o/r/actions/runs/7",
        }
        arguments.update(overrides)
        return build_checks(**arguments)

    # --- parsing ---------------------------------------------------------

    def test_libtest_lines_are_parsed(self):
        parsed = parse_log(
            "running 3 tests\n"
            "test a ... ok\n"
            "test b ... FAILED\n"
            "test c ... ignored\n"
            "test result: FAILED. 1 passed; 1 failed; 1 ignored\n")
        self.assertEqual(parsed, {"a": "ok", "b": "FAILED", "c": "ignored"})

    def test_a_log_directory_without_logs_is_rejected(self):
        empty = self.root / "empty"
        empty.mkdir()
        with self.assertRaisesRegex(EvidenceError, "no \\*\\.log"):
            collect_statuses(empty)

    def test_a_missing_log_directory_is_rejected(self):
        with self.assertRaisesRegex(EvidenceError, "not a directory"):
            collect_statuses(self.root / "absent")

    # --- derived booleans -------------------------------------------------

    def test_every_passing_test_makes_every_check_true(self):
        tests = self.collect(self.write_logs())
        self.assertTrue(tests["all_checks_ok"])
        self.assertEqual(tests["summary"]["required_tests"], 8)
        self.assertEqual(tests["summary"]["passed_tests"], 8)
        for check in ATTESTED_CHECKS:
            self.assertTrue(tests["checks"][check]["ok"], check)

    def test_a_missing_test_makes_its_check_false(self):
        # A renamed test must not disappear from the count: it becomes
        # `missing`, which fails closed instead of shrinking the denominator.
        log_dir = self.write_logs()
        target = (log_dir / "target_m8_rule_evidence.log")
        target.write_text("test test_0 ... ok\n", encoding="utf-8")
        tests = self.collect(log_dir)
        rule = tests["checks"]["m8_rule_evidence"]
        self.assertEqual(rule["required"], 2)
        self.assertEqual(rule["missing"], 1)
        self.assertFalse(rule["ok"])
        self.assertFalse(tests["all_checks_ok"])

    def test_a_failed_test_makes_its_check_false(self):
        log_dir = self.write_logs(
            {"target_m8_legal_api_evidence::test_1": "FAILED"})
        tests = self.collect(log_dir)
        self.assertFalse(tests["checks"]["m8_legal_api_evidence"]["ok"])
        self.assertEqual(tests["summary"]["failed_tests"], 1)

    def test_an_ignored_test_is_not_a_pass(self):
        log_dir = self.write_logs(
            {"target_evaluation_ci_reproducible::test_0": "ignored"})
        tests = self.collect(log_dir)
        entry = tests["checks"]["evaluation_ci_reproducible"]
        self.assertEqual(entry["ignored"], 1)
        self.assertFalse(entry["ok"])

    def test_the_report_digest_is_stable_and_sensitive(self):
        tests = self.collect(self.write_logs())
        self.assertEqual(tests["test_report_sha256"], report_digest(tests["checks"]))
        changed = self.collect(
            self.write_logs({"target_m8_rule_evidence::test_0": "FAILED"}))
        self.assertNotEqual(tests["test_report_sha256"], changed["test_report_sha256"])

    def test_another_run_cannot_rebind_the_test_report(self):
        tests = self.collect(self.write_logs())
        matches = self.root / "matches.jsonl"
        matches.write_text('{"block_index":0}\n', encoding="utf-8")
        with self.assertRaisesRegex(EvidenceError, "different Actions run"):
            self.checks(tests, matches, run_url="https://github.com/o/r/actions/runs/999")

    def test_trusted_run_requires_real_matching_actions_environment(self):
        from unittest.mock import patch
        from scripts.m8_evidence import validate_trusted_current_run
        url = "https://github.com/cueikusuta/agricola-re-ai/actions/runs/123"
        vars = {
            "GITHUB_ACTIONS": "true",
            "GITHUB_SHA": "abc123",
            "GITHUB_RUN_ID": "123",
            "GITHUB_REPOSITORY": "cueikusuta/agricola-re-ai",
            "GITHUB_SERVER_URL": "https://github.com",
        }
        with patch.dict("os.environ", vars, clear=True):
            validate_trusted_current_run(code_sha="abc123", run_url=url)
            with self.assertRaisesRegex(EvidenceError, "current Actions run"):
                validate_trusted_current_run(code_sha="abc123",
                                            run_url="https://github.com/o/r/actions/runs/999")
            with self.assertRaisesRegex(EvidenceError, "current Actions revision"):
                validate_trusted_current_run(code_sha="forged", run_url=url)

    # --- binding to the matches artifact ---------------------------------

    def test_the_checks_file_is_bound_to_code_and_matches(self):
        tests = self.collect(self.write_logs())
        matches = self.root / "matches.jsonl"
        matches.write_text('{"block_index":0}\n', encoding="utf-8")
        checks = self.checks(tests, matches)
        self.assertEqual(checks["schema_version"], 2)
        self.assertTrue(checks["m8_rule_evidence"])
        attestation = checks["attestation"]
        self.assertEqual(attestation["producer"], "scripts/m8_evidence.py")
        self.assertEqual(attestation["required_tests"], 8)
        self.assertEqual(attestation["failed_tests"], 0)
        self.assertEqual(attestation["missing_tests"], 0)
        self.assertEqual(attestation["matches_rows"], 1)
        self.assertEqual(
            attestation["test_report_sha256"], tests["test_report_sha256"])

    def test_a_different_code_sha_is_rejected(self):
        tests = self.collect(self.write_logs())
        matches = self.root / "matches.jsonl"
        matches.write_text('{"block_index":0}\n', encoding="utf-8")
        with self.assertRaisesRegex(EvidenceError, "produced at code"):
            self.checks(tests, matches, code_sha="sha2")

    def test_a_hand_edited_report_is_rejected(self):
        tests = self.collect(self.write_logs())
        tests["checks"]["m8_rule_evidence"]["tests"][0]["status"] = "failed"
        matches = self.root / "matches.jsonl"
        matches.write_text('{"block_index":0}\n', encoding="utf-8")
        with self.assertRaisesRegex(EvidenceError, "digest does not match"):
            self.checks(tests, matches)

    def test_changed_requirements_invalidate_the_report(self):
        tests = self.collect(self.write_logs())
        self.requirements_path.write_text(
            json.dumps(requirements_payload(tests_per_check=3)), encoding="utf-8")
        matches = self.root / "matches.jsonl"
        matches.write_text('{"block_index":0}\n', encoding="utf-8")
        with self.assertRaisesRegex(EvidenceError, "requirements changed"):
            self.checks(tests, matches)

    def test_no_matches_file_is_rejected(self):
        tests = self.collect(self.write_logs())
        with self.assertRaisesRegex(EvidenceError, "at least one --matches"):
            self.checks(tests, [])

    def test_a_missing_matches_file_is_rejected(self):
        tests = self.collect(self.write_logs())
        with self.assertRaisesRegex(EvidenceError, "No such file"):
            self.checks(tests, self.root / "missing.jsonl")

    def test_an_empty_matches_file_is_rejected(self):
        tests = self.collect(self.write_logs())
        matches = self.root / "matches.jsonl"
        matches.write_text("\n\n", encoding="utf-8")
        with self.assertRaisesRegex(EvidenceError, "no rows"):
            self.checks(tests, matches)

    def test_a_non_actions_run_url_is_rejected(self):
        log_dir = self.write_logs()
        with self.assertRaisesRegex(EvidenceError, "not a GitHub Actions run URL"):
            self.collect(log_dir, run_url="local-run")

    def test_a_requirements_file_without_a_check_is_rejected(self):
        payload = requirements_payload()
        del payload["checks"]["evaluation_ci_reproducible"]
        path = self.write_json("broken.json", payload)
        with self.assertRaisesRegex(EvidenceError, "no requirements for"):
            load_requirements(path)

    def test_a_requirements_file_with_an_empty_check_is_rejected(self):
        payload = requirements_payload()
        payload["checks"]["m8_rule_evidence"]["tests"] = []
        path = self.write_json("broken.json", payload)
        with self.assertRaisesRegex(EvidenceError, "lists no tests"):
            load_requirements(path)

    def test_the_requirements_manifest_in_the_repository_is_valid(self):
        repository = Path(__file__).resolve().parent.parent
        requirements = load_requirements(
            repository / "docs" / "m8-evidence-requirements.json")
        for check, entry in requirements["checks"].items():
            self.assertTrue(entry["tests"], check)
            for test in entry["tests"]:
                self.assertIn("::", test["id"])
                self.assertTrue(test["basis"], test["id"])

    def test_a_test_id_is_never_used_twice(self):
        repository = Path(__file__).resolve().parent.parent
        requirements = load_requirements(
            repository / "docs" / "m8-evidence-requirements.json")
        ids = [test["id"] for entry in requirements["checks"].values()
               for test in entry["tests"]]
        self.assertEqual(len(ids), len(set(ids)), "duplicate requirement entries")

    def test_every_required_test_exists_in_the_repository(self):
        """A requirement must name a test that is really defined, so a typo
        cannot be mistaken for a missing-but-intended test."""
        repository = Path(__file__).resolve().parent.parent
        requirements = load_requirements(
            repository / "docs" / "m8-evidence-requirements.json")
        for entry in requirements["checks"].values():
            for test in entry["tests"]:
                target, name = test["id"].split("::", 1)
                source = repository / "tests" / f"{target}.rs"
                self.assertTrue(source.exists(), f"{target}: no test file")
                text = source.read_text(encoding="utf-8")
                self.assertIn(f"fn {name}(", text, f"{test['id']} is not defined")


if __name__ == "__main__":
    unittest.main()
