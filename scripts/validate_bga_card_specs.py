#!/usr/bin/env python3
"""Check BGA reference card-spec snapshots against the existing 840-card ledger.

Stdlib-only contract smoke test. Does not certify BGA semantics, full JSON Schema
validation, Rust engine coverage, or official-source verification.
"""
from __future__ import annotations

from collections import Counter
import csv
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "docs" / "card-catalog-ae-preliminary.csv"
SPECS = ROOT / "data" / "bga-card-specs"
TESTS = ROOT / "data" / "bga-card-tests"
KINDS = {"小進歩": "minor_improvement", "職業": "occupation"}
REQUIRED = {
    "schema_version", "card_id", "identity", "bga", "play",
    "effects", "post_play", "review",
}


def run() -> int:
    errors: list[str] = []
    with LEDGER.open(encoding="utf-8-sig", newline="") as f:
        rows = list(csv.DictReader(f))
    ids = [row["card_id"] for row in rows]
    if len(rows) != 840 or len(set(ids)) != 840:
        errors.append("existing primary ledger is not 840 unique IDs")
    ledger = {row["card_id"]: row for row in rows}

    test_cases: dict[str, set[str]] = {}
    for path in sorted(TESTS.glob("*.json")):
        try:
            item = json.loads(path.read_text(encoding="utf-8"))
            card_id = item["card_id"]
            cases = item["cases"]
            if item["schema_version"] != 1 or path.stem != card_id or card_id not in ledger:
                raise ValueError("invalid test ID / version / path")
            if not isinstance(cases, list) or not cases:
                raise ValueError("test cases missing")
            names = [case["id"] for case in cases]
            if len(names) != len(set(names)):
                raise ValueError("duplicate test case ID")
            for case in cases:
                if not all(key in case for key in ("scope", "setup", "action", "expected")):
                    raise ValueError("incomplete test case")
            test_cases[card_id] = set(names)
        except (ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
            errors.append(f"{path}: {exc}")

    statuses: Counter[str] = Counter()
    implemented: Counter[str] = Counter()
    counted_ids: set[str] = set()
    for path in sorted(SPECS.glob("*.json")):
        try:
            item = json.loads(path.read_text(encoding="utf-8"))
            missing = REQUIRED - item.keys()
            if missing:
                raise ValueError(f"missing keys: {sorted(missing)}")
            card_id = item["card_id"]
            if path.stem != card_id or card_id not in ledger or card_id in counted_ids:
                raise ValueError("invalid / duplicate spec card ID")
            if item["schema_version"] != 1:
                raise ValueError("unsupported spec version")
            ref = ledger[card_id]
            if item["identity"]["name_ja"] != ref["name_ja"]:
                raise ValueError("Japanese name differs from ledger")
            if ref["name_en"] and item["identity"]["name_en"] != ref["name_en"]:
                raise ValueError("English name differs from ledger")
            expected_kind = KINDS.get(ref["card_type"], "multi_type")
            if item["identity"]["kind"] != expected_kind:
                raise ValueError("card type differs from ledger")
            bga = item["bga"]
            if any(str(i) not in bga["eligibility"] for i in (2, 3, 4)):
                raise ValueError("player eligibility is incomplete")
            if any(v not in ("yes", "no", "unknown") for v in bga["eligibility"].values()):
                raise ValueError("invalid eligibility")
            review = item["review"]
            status = review["status"]
            if status not in ("inventory_only", "partial", "bga_checked", "needs_resolution", "blocked"):
                raise ValueError("unknown review status")
            cases = set(review["test_case_ids"])
            if not cases.issubset(test_cases.get(card_id, set())):
                raise ValueError("test case references are not present")
            if status == "bga_checked":
                play = item["play"]
                if any(play[key] is None for key in ("cost", "prerequisites", "printed_points")):
                    raise ValueError("BGA checked card has unresolved play fields")
                if item["effects"] is None or item["post_play"]["destination"] == "unknown":
                    raise ValueError("BGA checked card has unresolved effect/destination")
                if not bga["references"] or review["unresolved_fields"] or not cases:
                    raise ValueError("BGA checked card lacks evidence/tests")
            counted_ids.add(card_id)
            statuses[status] += 1
            implemented[review.get("implementation_status", "not_started")] += 1
        except (ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
            errors.append(f"{path}: {exc}")

    for card_id in set(test_cases) - counted_ids:
        errors.append(f"test cases without spec: {card_id}")
    output = {
        "ledger_ids": len(ledger),
        "spec_records": len(counted_ids),
        "without_spec": len(ledger) - len(counted_ids),
        "spec_by_review": dict(sorted(statuses.items())),
        "spec_by_implementation": dict(sorted(implemented.items())),
        "cards_with_test_contracts": len(test_cases),
        "official_verified_cards": "count separately using existing official audit",
        "errors": errors,
    }
    print(json.dumps(output, ensure_ascii=False, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(run())
