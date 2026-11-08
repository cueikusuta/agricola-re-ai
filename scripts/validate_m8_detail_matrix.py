#!/usr/bin/env python3
"""Fail-closed mapping between seven detailed-action categories and Rust tests.

The existing Rust CI executes these tests. This audit catches documentation
drift, missing tests, wrong category counts and silently dropped rejection
coverage without re-running every Rust test a second time.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

CATEGORIES = (
    "fencing",
    "plowing",
    "sowing",
    "animals",
    "rooms_stables",
    "renovation",
    "free_actions",
)
IDENTIFIER = re.compile(r"^[a-z][a-z0-9_]*$")


def validate_matrix(data: Any, repo_root: Path) -> dict[str, Any]:
    if not isinstance(data, dict) or set(data) != {
        "schema_version",
        "description",
        "categories",
    }:
        raise ValueError("matrix must contain exactly schema_version, description and categories")
    if type(data["schema_version"]) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported matrix schema_version")
    if not isinstance(data["description"], str) or not data["description"].strip():
        raise ValueError("matrix description must not be empty")
    categories = data["categories"]
    if not isinstance(categories, dict) or set(categories) != set(CATEGORIES):
        raise ValueError("matrix must cover exactly the seven M8 detailed-action categories")

    seen_tests: set[tuple[str, str]] = set()
    for category in CATEGORIES:
        cases = categories[category]
        if not isinstance(cases, dict) or set(cases) != {"legal", "illegal"}:
            raise ValueError(f"{category}: require exactly one legal and one illegal test")
        for outcome in ("legal", "illegal"):
            case = cases[outcome]
            if not isinstance(case, dict) or set(case) != {"target", "test"}:
                raise ValueError(f"{category}.{outcome}: require target and test")
            target, test = case["target"], case["test"]
            if not isinstance(target, str) or not IDENTIFIER.fullmatch(target):
                raise ValueError(f"{category}.{outcome}: invalid Rust test target")
            if not isinstance(test, str) or not IDENTIFIER.fullmatch(test):
                raise ValueError(f"{category}.{outcome}: invalid Rust test name")
            signature = (target, test)
            if signature in seen_tests:
                raise ValueError(f"{category}.{outcome}: reused test {target}::{test}")
            seen_tests.add(signature)
            source = repo_root / "tests" / f"{target}.rs"
            if not source.is_file():
                raise ValueError(f"{category}.{outcome}: missing Rust test target {source}")
            text = source.read_text(encoding="utf-8")
            pattern = re.compile(r"(?m)^\s*#\[test\]\s*\n\s*fn\s+" + re.escape(test) + r"\s*\(")
            if not pattern.search(text):
                raise ValueError(f"{category}.{outcome}: missing #[test] function {target}::{test}")

    return {
        "schema_version": 1,
        "categories": len(CATEGORIES),
        "legal_tests": len(CATEGORIES),
        "illegal_tests": len(CATEGORIES),
        "distinct_rust_tests": len(seen_tests),
        "status": "pass",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    root = Path(__file__).resolve().parents[1]
    parser.add_argument("--repo-root", type=Path, default=root)
    parser.add_argument(
        "--matrix",
        type=Path,
        default=root / "docs" / "m8-detail-test-matrix.json",
    )
    args = parser.parse_args()
    data = json.loads(args.matrix.read_text(encoding="utf-8"))
    print(json.dumps(validate_matrix(data, args.repo_root), sort_keys=True))


if __name__ == "__main__":
    main()
