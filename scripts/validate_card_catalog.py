#!/usr/bin/env python3
"""Validate preliminary A-E cards and the independent official-review ledger.

No third-party dependencies. A preliminary entry never becomes playable merely
because its ID, display name or source URL is present.
"""
from __future__ import annotations

import argparse
from collections import Counter
import csv
from datetime import date
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
PRELIMINARY = ROOT / "docs" / "card-catalog-ae-preliminary.csv"
AUDIT = ROOT / "docs" / "card-catalog-ae-audit.csv"
TYPES = {
    "職業": "occupation",
    "小進歩": "minor_improvement",
    "小進歩  大進歩": "multi_type",
    "小進歩・大進歩・職業（複合）": "multi_type",
}
ELIGIBILITY = {"yes", "no", "unverified"}
REVIEW_STATES = {"unverified", "reviewed-needs-official", "verified", "rejected"}
SOURCES = {"secondary_catalog", "official_pdf", "official_print", "official_other"}
AUDIT_FIELDS = (
    "card_id", "card_type_normalized", "eligible_at_2", "eligible_at_3",
    "eligible_at_4", "review_status", "reference_type",
    "primary_reference_id", "primary_reference_url", "source_language",
    "ruleset_version", "reviewed_by", "reviewed_on", "review_note",
)


def load_rows(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        if not reader.fieldnames or len(set(reader.fieldnames)) != len(reader.fieldnames):
            raise ValueError(f"{path}: missing/duplicate CSV headers")
        rows = list(reader)
    if any(None in row or any(value is None for value in row.values()) for row in rows):
        raise ValueError(f"{path}: malformed CSV fields")
    return rows


def review_ledger(
    preliminary: list[dict[str, str]],
    audit: list[dict[str, str]],
    *,
    require_full: bool = True,
) -> tuple[dict, list[str]]:
    errors: list[str] = []
    preliminary_ids: dict[str, dict[str, str]] = {}
    audit_ids: set[str] = set()
    deck_counts: Counter[str] = Counter()
    verified_count: Counter[str] = Counter()
    eligible: dict[str, dict[str, int]] = {
        str(n): {"occupation": 0, "minor_improvement": 0} for n in (2, 3, 4)
    }
    for row_number, row in enumerate(preliminary, 2):
        card_id = row.get("card_id", "")
        deck = row.get("deck", "")
        if not re.fullmatch(r"[A-E](?:0[0-9]{2}|1[0-6][0-9])", card_id):
            errors.append(f"preliminary row {row_number}: invalid ID {card_id!r}")
        else:
            suffix = int(card_id[1:])
            if not 1 <= suffix <= 168:
                errors.append(f"{card_id}: ID out of range 1..168")
        if card_id in preliminary_ids:
            errors.append(f"preliminary duplicate ID: {card_id}")
        preliminary_ids[card_id] = row
        deck_counts[deck] += 1
        if deck != card_id[:1]:
            errors.append(f"{card_id}: deck mismatch {deck!r}")
        if row.get("card_type") not in TYPES:
            errors.append(f"{card_id}: unrecognized preliminary card_type")
        if not row.get("name_ja", "").strip():
            errors.append(f"{card_id}: Japanese display name missing")

    for row_number, row in enumerate(audit, 2):
        card_id = row.get("card_id", "")
        if card_id in audit_ids:
            errors.append(f"audit duplicate ID: {card_id}")
        audit_ids.add(card_id)
        source = preliminary_ids.get(card_id)
        if source is None:
            errors.append(f"audit row {row_number}: unknown ID {card_id!r}")
            continue
        expected_type = TYPES.get(source.get("card_type"))
        kind = row.get("card_type_normalized")
        if kind != expected_type:
            errors.append(f"{card_id}: normalized type {kind!r} != {expected_type!r}")
        status = row.get("review_status", "")
        verified_count[status] += 1
        if status not in REVIEW_STATES:
            errors.append(f"{card_id}: unknown review status {status!r}")
        reference = row.get("reference_type", "")
        if reference not in SOURCES:
            errors.append(f"{card_id}: unknown reference type {reference!r}")
        flags = {}
        for n in (2, 3, 4):
            key = f"eligible_at_{n}"
            flag = row.get(key, "")
            flags[n] = flag
            if flag not in ELIGIBILITY:
                errors.append(f"{card_id}: unknown {key} {flag!r}")
        if row.get("ruleset_version") != "2016-revised":
            errors.append(f"{card_id}: expected ruleset_version 2016-revised")
        reviewed = row.get("reviewed_on", "")
        if reviewed:
            try:
                date.fromisoformat(reviewed)
            except ValueError:
                errors.append(f"{card_id}: reviewed_on must be ISO date")
        if status == "verified":
            if any(f == "unverified" for f in flags.values()):
                errors.append(f"{card_id}: verified row has unverified player eligibility")
            if reference not in {"official_pdf", "official_print", "official_other"}:
                errors.append(f"{card_id}: verified without official reference type")
            for field in ("primary_reference_id", "source_language", "reviewed_by", "reviewed_on"):
                if not row.get(field, "").strip():
                    errors.append(f"{card_id}: verified without {field}")
            if reference in {"official_pdf", "official_other"} and not row.get("primary_reference_url", ""):
                errors.append(f"{card_id}: verified digital source needs URL")
            if not source.get("name_en", "").strip():
                errors.append(f"{card_id}: verified without English name in preliminary catalog")
            if kind in {"occupation", "minor_improvement"}:
                for n in (2, 3, 4):
                    if flags[n] == "yes":
                        eligible[str(n)][kind] += 1
        elif status == "unverified" and any(f == "yes" for f in flags.values()):
            errors.append(f"{card_id}: unverified card must not be marked eligible")
    missing = set(preliminary_ids) - audit_ids
    if missing:
        errors.append(f"audit missing {len(missing)} preliminary IDs: {','.join(sorted(missing)[:8])}")
    if require_full:
        expected = {f"{deck}{n:03d}" for deck in "ABCDE" for n in range(1, 169)}
        if set(preliminary_ids) != expected:
            errors.append("preliminary ID set is not exactly A001..E168 (840)")
        if len(preliminary) != 840 or len(audit) != 840:
            errors.append(f"expected 840+840 rows, got {len(preliminary)}+{len(audit)}")
        if any(deck_counts[deck] != 168 for deck in "ABCDE"):
            errors.append(f"expected 168 per deck, actual {dict(deck_counts)}")
    report = {
        "schema_version": 1,
        "preliminary_count": len(preliminary),
        "audit_count": len(audit),
        "deck_counts": dict(sorted(deck_counts.items())),
        "review_status_counts": dict(sorted(verified_count.items())),
        "verified_eligible_by_players": eligible,
        "verified_4p_10_7_ready": (
            eligible["4"]["occupation"] >= 40
            and eligible["4"]["minor_improvement"] >= 40
        ),
        "errors": errors,
    }
    return report, errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--preliminary", type=Path, default=PRELIMINARY)
    parser.add_argument("--audit", type=Path, default=AUDIT)
    parser.add_argument("--report", type=Path, default=None)
    args = parser.parse_args()
    try:
        preliminary = load_rows(args.preliminary)
        audit = load_rows(args.audit)
        missing_fields = set(AUDIT_FIELDS) - set(audit[0]) if audit else set(AUDIT_FIELDS)
        if missing_fields:
            raise ValueError(f"missing audit columns: {sorted(missing_fields)}")
        report, errors = review_ledger(preliminary, audit)
    except (OSError, ValueError, csv.Error) as exc:
        print(f"catalog validation error: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if args.report:
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
