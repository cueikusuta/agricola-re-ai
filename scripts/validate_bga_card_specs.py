#!/usr/bin/env python3
"""Check the BGA reference base ledger and per-card spec snapshots.

Stdlib-only contract smoke test for Issue #202.  It does **not** certify BGA
semantics, full JSON Schema validation, Rust engine coverage, or official-source
verification.  It checks that:

* ``data/bga-card-base-ledger.csv`` keeps all 840 existing card ids and agrees
  with ``docs/card-catalog-ae-preliminary.csv`` on id, name and card type;
* every per-card spec in ``data/bga-card-specs/`` points at a ledger id and
  agrees with the base ledger on identity, BGA status, cost and printed points;
* every spec's test-case references exist in ``data/bga-card-tests/``;
* ``bga_checked`` specs have resolved play fields, effects, a known destination,
  at least one reference and no unresolved fields;
* coverage is reported in separate buckets (base ledger / specs / BGA checked /
  tests) so that "record exists" is never confused with "card is ready".
"""
from __future__ import annotations

from collections import Counter
import csv
import json
from pathlib import Path
import re
import sys

from validate_json_schema import validate as validate_schema

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "docs" / "card-catalog-ae-preliminary.csv"
BASE = ROOT / "data" / "bga-card-base-ledger.csv"
SPECS = ROOT / "data" / "bga-card-specs"
TESTS = ROOT / "data" / "bga-card-tests"
SPEC_SCHEMA = ROOT / "schemas" / "bga-card-spec.schema.json"
TEST_SCHEMA = ROOT / "schemas" / "bga-card-test.schema.json"
KINDS = {"小進歩": "minor_improvement", "職業": "occupation"}
GOODS = {"food", "wood", "clay", "stone", "reed", "grain", "vegetable"}
STATUSES = {"inventory_only", "partial", "bga_checked", "needs_resolution", "blocked"}
ACTIONS = {"occupation", "minor_improvement", "major_improvement", "special", "unknown"}
RULEBOOK = "https://shop.asmodee.ca/img/product/description/EN-AGRICOLA-CONSUL_DIRIGENS-RULES.pdf"
# deck A-D card-number eligibility rule from the official Consul Dirigens rules
ELIGIBILITY_RANGES = [
    (1, 84, "yes", "yes", "yes"),
    (85, 126, "yes", "yes", "yes"),
    (127, 147, "no", "yes", "yes"),
    (148, 168, "no", "no", "yes"),
]
DECKS_WITH_RULE = {"A", "B", "C", "D"}
REQUIRED = {
    "schema_version",
    "card_id",
    "identity",
    "bga",
    "play",
    "effects",
    "post_play",
    "review",
}


def expected_kind(card_type: str) -> str:
    return KINDS.get(card_type, "multi_type")


def load_base() -> tuple[list[dict], list[str]]:
    errors: list[str] = []
    if not BASE.exists():
        return [], ["base ledger is missing: data/bga-card-base-ledger.csv"]
    with BASE.open(encoding="utf-8-sig", newline="") as handle:
        rows = list(csv.DictReader(handle))
    ids = [row.get("card_id", "") for row in rows]
    if len(rows) != 840:
        errors.append(f"base ledger must hold 840 rows, found {len(rows)}")
    if len(set(ids)) != len(ids):
        errors.append("base ledger contains duplicate card ids")
    for row in rows:
        card_id = row.get("card_id", "")
        if not re.fullmatch(r"[A-E]\d{3}", card_id):
            errors.append(f"base ledger row has invalid id {card_id!r}")
            continue
        if row.get("kind") not in ("occupation", "minor_improvement", "multi_type"):
            errors.append(f"{card_id}: invalid kind {row.get('kind')!r}")
        if row.get("action") not in ACTIONS:
            errors.append(f"{card_id}: invalid action {row.get('action')!r}")
        for field in ("bga_listed", "bga_playable", "bga_unimplemented", "ban", "base_2016"):
            if row.get(field) not in ("yes", "no", "unknown", "none", "weak", "strong"):
                errors.append(f"{card_id}: invalid {field} {row.get(field)!r}")
        for field in ("eligible_2", "eligible_3", "eligible_4"):
            if row.get(field) not in ("yes", "no", "unknown"):
                errors.append(f"{card_id}: invalid {field} {row.get(field)!r}")
        cost = row.get("cost", "")
        if cost not in ("none", "unknown"):
            for part in cost.split(";"):
                match = re.fullmatch(r"([a-z_]+):(\d+)", part)
                if not match or match.group(1) not in GOODS:
                    errors.append(f"{card_id}: invalid cost {cost!r}")
                    break
        points = row.get("printed_points", "")
        if points != "unknown" and not re.fullmatch(r"-?\d+", points):
            errors.append(f"{card_id}: invalid printed_points {points!r}")
        if row.get("review_status") not in STATUSES:
            errors.append(f"{card_id}: invalid review_status {row.get('review_status')!r}")
        basis = row.get("eligibility_basis", "")
        if basis not in ("consul_dirigens_rule", "unknown", ""):
            errors.append(f"{card_id}: invalid eligibility_basis {basis!r}")
        if basis == "consul_dirigens_rule":
            if row.get("eligibility_source_url") != RULEBOOK:
                errors.append(f"{card_id}: rule-based eligibility lacks the rulebook URL")
            expected = next(
                (
                    (two, three, four)
                    for first, last, two, three, four in ELIGIBILITY_RANGES
                    if first <= int(card_id[1:]) <= last
                ),
                None,
            )
            if expected and (
                row.get("eligible_2"),
                row.get("eligible_3"),
                row.get("eligible_4"),
            ) != expected:
                errors.append(f"{card_id}: eligibility disagrees with the card-number rule")
        elif card_id[0] in DECKS_WITH_RULE:
            errors.append(f"{card_id}: A-D row has no eligibility basis")
        if row.get("bga_listed") == "yes" and not row.get("source_url", "").startswith(
            "https://agricola.cloudfree.jp/"
        ):
            errors.append(f"{card_id}: BGA-listed row lacks a wiki source URL")
    return rows, errors


def run() -> int:
    errors: list[str] = []
    with LEDGER.open(encoding="utf-8-sig", newline="") as handle:
        ledger_rows = list(csv.DictReader(handle))
    ids = [row["card_id"] for row in ledger_rows]
    if len(ledger_rows) != 840 or len(set(ids)) != 840:
        errors.append("existing primary ledger is not 840 unique IDs")
    ledger = {row["card_id"]: row for row in ledger_rows}

    base_rows, base_errors = load_base()
    errors.extend(base_errors)
    base = {row["card_id"]: row for row in base_rows if row.get("card_id")}
    if base and set(base) != set(ledger):
        missing = sorted(set(ledger) - set(base))
        extra = sorted(set(base) - set(ledger))
        errors.append(f"base ledger ids differ from primary ledger (missing={missing}, extra={extra})")
    for card_id, row in base.items():
        ref = ledger.get(card_id)
        if not ref:
            continue
        if row["name_ja"] != ref["name_ja"]:
            errors.append(f"{card_id}: base ledger Japanese name differs from primary ledger")
        if ref["name_en"] and row["name_en"] != ref["name_en"]:
            errors.append(f"{card_id}: base ledger English name differs from primary ledger")
        if row["kind"] != expected_kind(ref["card_type"]):
            errors.append(f"{card_id}: base ledger kind differs from primary ledger")

    test_schema = json.loads(TEST_SCHEMA.read_text(encoding="utf-8"))
    spec_schema = json.loads(SPEC_SCHEMA.read_text(encoding="utf-8"))
    test_cases: dict[str, set[str]] = {}
    for path in sorted(TESTS.glob("*.json")):
        try:
            item = json.loads(path.read_text(encoding="utf-8"))
            schema_errors = validate_schema(item, test_schema)
            if schema_errors:
                raise ValueError("schema: " + "; ".join(schema_errors))
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
                if case["scope"] not in (
                    "effect_only",
                    "legal_play",
                    "integration",
                    "saved_replay",
                    "privacy",
                ):
                    raise ValueError(f"unknown test scope {case['scope']!r}")
            test_cases[card_id] = set(names)
        except (ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
            errors.append(f"{path}: {exc}")

    statuses: Counter[str] = Counter()
    implemented: Counter[str] = Counter()
    counted_ids: set[str] = set()
    for path in sorted(SPECS.glob("*.json")):
        try:
            item = json.loads(path.read_text(encoding="utf-8"))
            schema_errors = validate_schema(item, spec_schema)
            if schema_errors:
                raise ValueError("schema: " + "; ".join(schema_errors))
            missing = REQUIRED - item.keys()
            if missing:
                raise ValueError(f"missing keys: {sorted(missing)}")
            card_id = item["card_id"]
            if path.stem != card_id or card_id not in ledger or card_id in counted_ids:
                raise ValueError("invalid / duplicate spec card ID")
            if item["schema_version"] != 1:
                raise ValueError("unsupported spec version")
            ref = ledger[card_id]
            base_row = base.get(card_id)
            if base_row is None:
                raise ValueError("card is missing from the base ledger")
            if item["identity"]["name_ja"] != ref["name_ja"]:
                raise ValueError("Japanese name differs from ledger")
            if ref["name_en"] and item["identity"]["name_en"] != ref["name_en"]:
                raise ValueError("English name differs from ledger")
            if item["identity"]["kind"] != expected_kind(ref["card_type"]):
                raise ValueError("card type differs from ledger")
            bga = item["bga"]
            if any(str(i) not in bga["eligibility"] for i in (2, 3, 4)):
                raise ValueError("player eligibility is incomplete")
            if any(v not in ("yes", "no", "unknown") for v in bga["eligibility"].values()):
                raise ValueError("invalid eligibility")
            if bga["listed"] != base_row["bga_listed"]:
                raise ValueError("bga.listed differs from the base ledger")
            if bga["playable"] != base_row["bga_playable"]:
                raise ValueError("bga.playable differs from the base ledger")
            if bga["ban_status"] != base_row["ban"]:
                raise ValueError("bga.ban_status differs from the base ledger")
            if bga["base_2016"] != base_row["base_2016"]:
                raise ValueError("bga.base_2016 differs from the base ledger")
            play = item["play"]
            if play["printed_points"] is not None and str(play["printed_points"]) != base_row["printed_points"]:
                raise ValueError("printed_points differs from the base ledger")
            if play["cost"] is not None and base_row["cost"] not in ("none", "unknown"):
                base_cost = sorted(
                    part.split(":")[0] + ":" + part.split(":")[1]
                    for part in base_row["cost"].split(";")
                )
                spec_cost = sorted(
                    f"{entry['good']}:{entry['amount']}" for entry in play["cost"]
                )
                if base_cost != spec_cost:
                    raise ValueError("cost differs from the base ledger")
            review = item["review"]
            status = review["status"]
            if status not in STATUSES:
                raise ValueError("unknown review status")
            cases = set(review["test_case_ids"])
            if not cases.issubset(test_cases.get(card_id, set())):
                raise ValueError("test case references are not present")
            if status == "bga_checked":
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

    base_status = Counter(row.get("review_status", "") for row in base_rows)
    base_eligible4 = Counter(row.get("eligible_4", "") for row in base_rows)
    base_eligible_basis = Counter(row.get("eligibility_basis", "") for row in base_rows)
    output = {
        "primary_ledger_ids": len(ledger),
        "base_ledger_rows": len(base_rows),
        "base_by_review": dict(sorted(base_status.items())),
        "base_bga_listed": sum(1 for row in base_rows if row.get("bga_listed") == "yes"),
        "base_eligible_4": dict(sorted(base_eligible4.items())),
        "base_eligibility_basis": dict(sorted(base_eligible_basis.items())),
        "base_bga_unimplemented": sum(
            1 for row in base_rows if row.get("bga_unimplemented") == "yes"
        ),
        "spec_records": len(counted_ids),
        "without_spec": len(ledger) - len(counted_ids),
        "spec_by_review": dict(sorted(statuses.items())),
        "spec_by_implementation": dict(sorted(implemented.items())),
        "cards_with_test_contracts": len(test_cases),
        "official_verified_cards": "count separately using the official audit ledger",
        "errors": errors,
    }
    print(json.dumps(output, ensure_ascii=False, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(run())
