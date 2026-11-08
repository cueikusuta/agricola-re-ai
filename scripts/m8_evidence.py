#!/usr/bin/env python3
"""Produce the attested M8 evidence the G0 gate requires (#108 / #109 / #110).

The G0 gate can recompute everything that is derivable from a matches artifact,
but four checks depend on code that ran outside that artifact:

* `m8_rule_evidence` - official-rule expected-result tests (#108)
* `m8_legal_api_evidence` - legal detail API accepts/rejects (#109)
* `evaluation_observation_noninterference` - hidden state never reaches an observation
* `evaluation_ci_reproducible` - the same seed reproduces the same game

Those four used to be booleans a human could type. This script derives them from
an actual test run instead:

1. `collect` parses one `cargo test` log per target and checks every test listed
   in `docs/m8-evidence-requirements.json`. A renamed, removed, ignored or
   failing test makes its check `false`; it never silently disappears.
2. `checks` binds the derived booleans to the same code revision and the same
   matches bytes the evaluation produced, and writes the
   `checks_evidence.schema_version=2` file the gate reads, plus an attestation
   block describing how each boolean was derived.
3. `verify` re-derives the booleans from a report and compares them with an
   existing checks file, so CI can detect a hand-edited claim.

Usage:

    # 1. run the required test targets, one log per target
    cargo test --release --test rule_regressions 2>&1 | tee m8-logs/rule_regressions.log
    ...
    python scripts/m8_evidence.py collect \
        --requirements docs/m8-evidence-requirements.json \
        --log-dir m8-logs --code-sha "$GITHUB_SHA" \
        --run-url "$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID" \
        --out m8-evidence.json

    # 2. bind it to the evaluation artifact
    python scripts/m8_evidence.py checks \
        --tests m8-evidence.json \
        --requirements docs/m8-evidence-requirements.json \
        --matches results/shard-0/matches.jsonl ... \
        --code-sha "$GITHUB_SHA" \
        --run-url "$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID" \
        --out checks.json

The file is written even when a check fails, so the failing artifact can be
downloaded; the exit status is non-zero so a workflow cannot treat it as a pass.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from scripts.g0_evidence import sha256_hex  # noqa: E402  (same digest semantics)

TESTS_SCHEMA_VERSION = 1
CHECKS_SCHEMA_VERSION = 2
ATTESTATION_SCHEMA_VERSION = 1
PRODUCER = "scripts/m8_evidence.py"
ATTESTED_CHECKS = (
    "m8_rule_evidence",
    "m8_legal_api_evidence",
    "evaluation_observation_noninterference",
    "evaluation_ci_reproducible",
)
# `cargo test` prints one line per test: "test name ... ok|FAILED|ignored".
LIBTEST_LINE = re.compile(r"^test\s+(?P<name>\S+)\s+\.\.\.\s+(?P<status>ok|FAILED|ignored)\b")
PASSING = {"ok"}


class EvidenceError(ValueError):
    """A claim that cannot be supported by the inputs."""


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise EvidenceError(f"{path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise EvidenceError(f"{path}: not valid JSON ({exc})") from exc


def load_requirements(path: Path) -> dict:
    requirements = load_json(path)
    if requirements.get("schema_version") != 1:
        raise EvidenceError(f"{path}: unsupported requirements schema_version")
    checks = requirements.get("checks")
    if not isinstance(checks, dict):
        raise EvidenceError(f"{path}: missing checks map")
    missing = [check for check in ATTESTED_CHECKS if check not in checks]
    if missing:
        raise EvidenceError(f"{path}: no requirements for {', '.join(missing)}")
    for check, entry in checks.items():
        tests = entry.get("tests")
        if not isinstance(tests, list) or not tests:
            raise EvidenceError(f"{path}: check {check} lists no tests")
        for test in tests:
            for field in ("id", "mode", "expectation", "basis"):
                if not test.get(field):
                    raise EvidenceError(f"{path}: check {check} test missing {field}")
    return requirements


def parse_log(text: str) -> dict[str, str]:
    """Map test name to status from one `cargo test` log."""
    statuses: dict[str, str] = {}
    for line in text.splitlines():
        match = LIBTEST_LINE.match(line.strip())
        if match:
            statuses[match.group("name")] = match.group("status")
    return statuses


def collect_statuses(log_dir: Path) -> tuple[dict[str, str], list[str]]:
    """Read every `<target>.log` in a directory as target `stem`."""
    if not log_dir.is_dir():
        raise EvidenceError(f"{log_dir}: not a directory")
    logs = sorted(log_dir.glob("*.log"))
    if not logs:
        raise EvidenceError(f"{log_dir}: no *.log test output")
    statuses: dict[str, str] = {}
    for log in logs:
        target = log.stem
        for name, status in parse_log(log.read_text(encoding="utf-8")).items():
            statuses[f"{target}::{name}"] = status
    return statuses, [log.stem for log in logs]


def evaluate_requirements(requirements: dict, statuses: dict[str, str]) -> dict:
    """Derive per-check counts. Nothing is assumed: a test must be present."""
    report: dict[str, dict] = {}
    for check in ATTESTED_CHECKS:
        entry = requirements["checks"][check]
        rows = []
        for test in entry["tests"]:
            status = statuses.get(test["id"])
            if status is None:
                outcome = "missing"
            elif status in PASSING:
                outcome = "passed"
            elif status == "ignored":
                outcome = "ignored"
            else:
                outcome = "failed"
            rows.append({**test, "status": outcome})
        counts = {
            "issue": int(entry.get("issue", 0)),
            "required": len(rows),
            "passed": sum(1 for row in rows if row["status"] == "passed"),
            "failed": sum(1 for row in rows if row["status"] == "failed"),
            "ignored": sum(1 for row in rows if row["status"] == "ignored"),
            "missing": sum(1 for row in rows if row["status"] == "missing"),
            "tests": rows,
        }
        counts["ok"] = counts["passed"] == counts["required"]
        report[check] = counts
    return report


def report_digest(report: dict) -> str:
    """Digest over the derived outcomes only, so it is reproducible."""
    canonical = [
        {"id": row["id"], "status": row["status"]}
        for check in ATTESTED_CHECKS
        for row in report[check]["tests"]
    ]
    canonical.sort(key=lambda row: row["id"])
    return sha256_hex(
        json.dumps(canonical, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))


def totals(report: dict) -> dict:
    return {
        "required_tests": sum(report[check]["required"] for check in ATTESTED_CHECKS),
        "passed_tests": sum(report[check]["passed"] for check in ATTESTED_CHECKS),
        "failed_tests": sum(report[check]["failed"] for check in ATTESTED_CHECKS),
        "ignored_tests": sum(report[check]["ignored"] for check in ATTESTED_CHECKS),
        "missing_tests": sum(report[check]["missing"] for check in ATTESTED_CHECKS),
    }


def validate_run_url(run_url: str) -> str:
    if not run_url.startswith("https://github.com/") or "/actions/runs/" not in run_url:
        raise EvidenceError(
            f"run URL {run_url!r} is not a GitHub Actions run URL; the gate rejects it")
    return run_url


def build_tests(
    *,
    requirements_path: Path,
    requirements: dict,
    statuses: dict[str, str],
    targets: list[str],
    code_sha: str,
    run_url: str,
    command: str,
) -> dict:
    report = evaluate_requirements(requirements, statuses)
    summary = totals(report)
    return {
        "schema_version": TESTS_SCHEMA_VERSION,
        "producer": PRODUCER,
        "code_sha": code_sha,
        "run_url": validate_run_url(run_url),
        "command": command,
        "requirements_sha256": sha256_hex(requirements_path.read_bytes()),
        "targets": targets,
        "test_report_sha256": report_digest(report),
        "summary": summary,
        "checks": report,
        "all_checks_ok": all(report[check]["ok"] for check in ATTESTED_CHECKS),
    }


def matches_digest(paths: list[Path]) -> tuple[str, int]:
    if not paths:
        raise EvidenceError("at least one --matches file is required")
    payload = b""
    rows = 0
    for path in paths:
        try:
            data = path.read_bytes()
        except OSError as exc:
            raise EvidenceError(f"{path}: {exc}") from exc
        payload += data
        rows += sum(1 for line in data.splitlines() if line.strip())
    if rows == 0:
        raise EvidenceError("the matches files contain no rows")
    return sha256_hex(payload), rows


def build_checks(
    *,
    tests: dict,
    requirements_path: Path,
    requirements: dict,
    matches: list[Path],
    code_sha: str,
    run_url: str,
) -> dict:
    if tests.get("schema_version") != TESTS_SCHEMA_VERSION:
        raise EvidenceError("tests report has an unsupported schema_version")
    if tests.get("code_sha") != code_sha:
        raise EvidenceError(
            f"tests report was produced at code {tests.get('code_sha')!r}, not {code_sha!r}; "
            "the gate binds the evidence to one revision")
    if tests.get("requirements_sha256") != sha256_hex(requirements_path.read_bytes()):
        raise EvidenceError("requirements changed after the tests ran; re-collect the evidence")
    # Re-derive from the recorded statuses instead of trusting the stored
    # booleans, so an edited report is caught here rather than at the gate.
    statuses = {
        row["id"]: "ok" if row["status"] == "passed" else row["status"]
        for check in ATTESTED_CHECKS
        for row in tests["checks"][check]["tests"]
    }
    report = evaluate_requirements(requirements, statuses)
    if report_digest(report) != tests.get("test_report_sha256"):
        raise EvidenceError("tests report digest does not match its own test outcomes")
    digest, rows = matches_digest(matches)
    summary = totals(report)
    booleans = {check: report[check]["ok"] for check in ATTESTED_CHECKS}
    return {
        "schema_version": CHECKS_SCHEMA_VERSION,
        "code_sha": code_sha,
        "matches_sha256": digest,
        "m8_rule_evidence": booleans["m8_rule_evidence"],
        "m8_legal_api_evidence": booleans["m8_legal_api_evidence"],
        "observation_noninterference_passed": booleans[
            "evaluation_observation_noninterference"],
        "ci_reproducible": booleans["evaluation_ci_reproducible"],
        "attestation": {
            "schema_version": ATTESTATION_SCHEMA_VERSION,
            "producer": PRODUCER,
            "run_url": validate_run_url(run_url),
            "test_report_sha256": tests["test_report_sha256"],
            "requirements_sha256": tests["requirements_sha256"],
            "required_tests": summary["required_tests"],
            "passed_tests": summary["passed_tests"],
            "failed_tests": summary["failed_tests"] + summary["ignored_tests"],
            "missing_tests": summary["missing_tests"],
            "matches_rows": rows,
            "checks": {
                check: {
                    "issue": report[check]["issue"],
                    "required": report[check]["required"],
                    "passed": report[check]["passed"],
                    "failed": report[check]["failed"] + report[check]["ignored"],
                    "missing": report[check]["missing"],
                }
                for check in ATTESTED_CHECKS
            },
        },
    }


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8")


def command_collect(args: argparse.Namespace) -> int:
    requirements = load_requirements(args.requirements)
    statuses, targets = collect_statuses(args.log_dir)
    tests = build_tests(
        requirements_path=args.requirements,
        requirements=requirements,
        statuses=statuses,
        targets=targets,
        code_sha=args.code_sha,
        run_url=args.run_url,
        command=args.command,
    )
    write_json(args.out, tests)
    print(json.dumps({
        "out": str(args.out),
        "targets": targets,
        "test_report_sha256": tests["test_report_sha256"],
        "summary": tests["summary"],
        "all_checks_ok": tests["all_checks_ok"],
    }, ensure_ascii=False, indent=2))
    if not tests["all_checks_ok"]:
        failing = {
            check: {
                "failed": entry["failed"],
                "missing": entry["missing"],
                "ignored": entry["ignored"],
                "ids": [row["id"] for row in entry["tests"]
                        if row["status"] != "passed"],
            }
            for check, entry in tests["checks"].items() if not entry["ok"]
        }
        print(f"m8_evidence collect: incomplete evidence {failing}", file=sys.stderr)
        return 1
    return 0


def command_checks(args: argparse.Namespace) -> int:
    requirements = load_requirements(args.requirements)
    tests = load_json(args.tests)
    checks = build_checks(
        tests=tests,
        requirements_path=args.requirements,
        requirements=requirements,
        matches=args.matches,
        code_sha=args.code_sha,
        run_url=args.run_url,
    )
    write_json(args.out, checks)
    attested = checks["attestation"]
    print(json.dumps({
        "out": str(args.out),
        "code_sha": checks["code_sha"],
        "matches_sha256": checks["matches_sha256"],
        "booleans": {
            "m8_rule_evidence": checks["m8_rule_evidence"],
            "m8_legal_api_evidence": checks["m8_legal_api_evidence"],
            "observation_noninterference_passed":
                checks["observation_noninterference_passed"],
            "ci_reproducible": checks["ci_reproducible"],
        },
        "required_tests": attested["required_tests"],
        "passed_tests": attested["passed_tests"],
    }, ensure_ascii=False, indent=2))
    if not all(checks[check] for check in (
            "m8_rule_evidence", "m8_legal_api_evidence",
            "observation_noninterference_passed", "ci_reproducible")):
        print("m8_evidence checks: at least one check is false", file=sys.stderr)
        return 1
    return 0


def command_verify(args: argparse.Namespace) -> int:
    """Re-derive from a report and compare with an existing checks file."""
    requirements = load_requirements(args.requirements)
    tests = load_json(args.tests)
    checks = load_json(args.checks)
    if checks.get("schema_version") != CHECKS_SCHEMA_VERSION:
        raise EvidenceError("checks file has an unsupported schema_version")
    statuses = {
        row["id"]: "ok" if row["status"] == "passed" else row["status"]
        for check in ATTESTED_CHECKS
        for row in tests["checks"][check]["tests"]
    }
    report = evaluate_requirements(requirements, statuses)
    expected = {
        "m8_rule_evidence": report["m8_rule_evidence"]["ok"],
        "m8_legal_api_evidence": report["m8_legal_api_evidence"]["ok"],
        "observation_noninterference_passed":
            report["evaluation_observation_noninterference"]["ok"],
        "ci_reproducible": report["evaluation_ci_reproducible"]["ok"],
    }
    mismatched = [field for field, value in expected.items() if checks.get(field) != value]
    if mismatched:
        raise EvidenceError(
            f"checks file claims {mismatched} contrary to the test report; "
            "the evidence was hand-edited or produced from a different run")
    attestation = checks.get("attestation") or {}
    if attestation.get("test_report_sha256") != report_digest(report):
        raise EvidenceError("checks attestation does not match the test report digest")
    if attestation.get("requirements_sha256") != sha256_hex(args.requirements.read_bytes()):
        raise EvidenceError("checks attestation was produced from different requirements")
    print(json.dumps({"verified": True, "booleans": expected}, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    collect = sub.add_parser("collect", help="derive check booleans from test logs")
    collect.add_argument("--requirements", type=Path, required=True)
    collect.add_argument("--log-dir", type=Path, required=True,
                         help="directory with one <target>.log per test binary")
    collect.add_argument("--code-sha", required=True)
    collect.add_argument("--run-url", required=True)
    collect.add_argument("--command", default="",
                         help="the exact cargo test command that produced the logs")
    collect.add_argument("--out", type=Path, required=True)
    collect.set_defaults(handler=command_collect)

    checks = sub.add_parser("checks", help="bind the evidence to a matches artifact")
    checks.add_argument("--tests", type=Path, required=True)
    checks.add_argument("--requirements", type=Path, required=True)
    checks.add_argument("--matches", type=Path, action="append", default=[])
    checks.add_argument("--code-sha", required=True)
    checks.add_argument("--run-url", required=True)
    checks.add_argument("--out", type=Path, required=True)
    checks.set_defaults(handler=command_checks)

    verify = sub.add_parser("verify", help="re-derive and compare with a checks file")
    verify.add_argument("--tests", type=Path, required=True)
    verify.add_argument("--requirements", type=Path, required=True)
    verify.add_argument("--checks", type=Path, required=True)
    verify.set_defaults(handler=command_verify)

    args = parser.parse_args()
    try:
        return args.handler(args)
    except (EvidenceError, OSError, KeyError) as exc:
        print(f"m8_evidence {args.command} failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
