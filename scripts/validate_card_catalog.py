#!/usr/bin/env python3
"""Validate preliminary A-E cards and the independent official-review ledger.

No third-party dependencies. A preliminary entry never becomes playable merely
because its ID, display name or source URL is present.

Two outcomes are reported separately on purpose:

* **structure_validation** - are the 840 rows well formed and internally
  consistent? This can pass while no card is officially verified.
* **official_verification_progress** - how many cards have actually been checked
  against official material? Only `verified` rows with a recorded official
  source are counted, so this is `0` until a human records real evidence.

The script never fetches or embeds card text or images.
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
DEFAULT_SAMPLE_SIZE = 12
# Records the independent review already flagged as risky (Issue #115).
HIGH_RISK_CARD_IDS = ("A005", "D159")
# G1/G2 gates from docs/roadmap-m7-onward.md section 4.3.
G1_MIN_PER_KIND = 40
G2_TARGET_PER_KIND = 80


def load_rows(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        if not reader.fieldnames or len(set(reader.fieldnames)) != len(reader.fieldnames):
            raise ValueError(f"{path}: missing/duplicate CSV headers")
        rows = list(reader)
    if any(None in row or any(value is None for value in row.values()) for row in rows):
        raise ValueError(f"{path}: malformed CSV fields")
    return rows


def stratified_audit_sample(
    audit: list[dict[str, str]], size: int = DEFAULT_SAMPLE_SIZE
) -> list[str]:
    """Deterministic stratification by (deck, normalized type).

    The result is stable across runs so a reviewer can work through the same
    queue, but it is a *work queue*, not evidence. Selecting a card does not
    verify it.
    """
    if size <= 0:
        return []
    buckets: dict[tuple[str, str], list[str]] = {}
    for row in audit:
        card_id = row.get("card_id", "")
        card_type = row.get("card_type_normalized", "")
        if not card_id:
            continue
        buckets.setdefault((card_id[:1], card_type), []).append(card_id)
    for ids in buckets.values():
        ids.sort()
    ordered_strata = sorted(buckets)
    sample: list[str] = []
    cursor = {key: 0 for key in ordered_strata}
    while len(sample) < size:
        added = False
        for key in ordered_strata:
            ids = buckets[key]
            if cursor[key] < len(ids):
                sample.append(ids[cursor[key]])
                cursor[key] += 1
                added = True
                if len(sample) >= size:
                    break
        if not added:
            break
    return sample


def review_ledger(
    preliminary: list[dict[str, str]],
    audit: list[dict[str, str]],
    *,
    require_full: bool = True,
    sample_size: int = DEFAULT_SAMPLE_SIZE,
) -> tuple[dict, list[str]]:
    errors: list[str] = []
    error_categories: Counter[str] = Counter()

    def fail(category: str, message: str) -> None:
        errors.append(message)
        error_categories[category] += 1

    preliminary_ids: dict[str, dict[str, str]] = {}
    audit_ids: set[str] = set()
    deck_counts: Counter[str] = Counter()
    verified_count: Counter[str] = Counter()
    reference_counts: Counter[str] = Counter()
    eligibility_counts: dict[str, Counter[str]] = {
        str(n): Counter() for n in (2, 3, 4)
    }
    eligible: dict[str, dict[str, int]] = {
        str(n): {"occupation": 0, "minor_improvement": 0} for n in (2, 3, 4)
    }
    for row_number, row in enumerate(preliminary, 2):
        card_id = row.get("card_id", "")
        deck = row.get("deck", "")
        if not re.fullmatch(r"[A-E](?:0[0-9]{2}|1[0-6][0-9])", card_id):
            fail("structure", f"preliminary row {row_number}: invalid ID {card_id!r}")
        else:
            suffix = int(card_id[1:])
            if not 1 <= suffix <= 168:
                fail("structure", f"{card_id}: ID out of range 1..168")
        if card_id in preliminary_ids:
            fail("structure", f"preliminary duplicate ID: {card_id}")
        preliminary_ids[card_id] = row
        deck_counts[deck] += 1
        if deck != card_id[:1]:
            fail("structure", f"{card_id}: deck mismatch {deck!r}")
        if row.get("card_type") not in TYPES:
            fail("type", f"{card_id}: unrecognized preliminary card_type")
        if not row.get("name_ja", "").strip():
            fail("structure", f"{card_id}: Japanese display name missing")

    for row_number, row in enumerate(audit, 2):
        card_id = row.get("card_id", "")
        if card_id in audit_ids:
            fail("structure", f"audit duplicate ID: {card_id}")
        audit_ids.add(card_id)
        source = preliminary_ids.get(card_id)
        if source is None:
            fail("structure", f"audit row {row_number}: unknown ID {card_id!r}")
            continue
        expected_type = TYPES.get(source.get("card_type"))
        kind = row.get("card_type_normalized")
        if kind != expected_type:
            fail("type", f"{card_id}: normalized type {kind!r} != {expected_type!r}")
        status = row.get("review_status", "")
        verified_count[status] += 1
        if status not in REVIEW_STATES:
            fail("review_state", f"{card_id}: unknown review status {status!r}")
        reference = row.get("reference_type", "")
        reference_counts[reference] += 1
        if reference not in SOURCES:
            fail("provenance", f"{card_id}: unknown reference type {reference!r}")
        flags = {}
        for n in (2, 3, 4):
            key = f"eligible_at_{n}"
            flag = row.get(key, "")
            flags[n] = flag
            eligibility_counts[str(n)][flag] += 1
            if flag not in ELIGIBILITY:
                fail("eligibility", f"{card_id}: unknown {key} {flag!r}")
        if row.get("ruleset_version") != "2016-revised":
            fail("provenance", f"{card_id}: expected ruleset_version 2016-revised")
        reviewed = row.get("reviewed_on", "")
        if reviewed:
            try:
                date.fromisoformat(reviewed)
            except ValueError:
                fail("provenance", f"{card_id}: reviewed_on must be ISO date")
        if status == "verified":
            if any(f == "unverified" for f in flags.values()):
                fail(
                    "eligibility",
                    f"{card_id}: verified row has unverified player eligibility",
                )
            if reference not in {"official_pdf", "official_print", "official_other"}:
                fail(
                    "provenance",
                    f"{card_id}: verified without official reference type",
                )
            for field in ("primary_reference_id", "source_language", "reviewed_by", "reviewed_on"):
                if not row.get(field, "").strip():
                    fail("provenance", f"{card_id}: verified without {field}")
            if reference in {"official_pdf", "official_other"} and not row.get("primary_reference_url", ""):
                fail(
                    "provenance",
                    f"{card_id}: verified digital source needs URL",
                )
            if not source.get("name_en", "").strip():
                fail(
                    "provenance",
                    f"{card_id}: verified without English name in preliminary catalog",
                )
            if kind in {"occupation", "minor_improvement"}:
                for n in (2, 3, 4):
                    if flags[n] == "yes":
                        eligible[str(n)][kind] += 1
        elif status == "unverified" and any(f == "yes" for f in flags.values()):
            fail(
                "eligibility",
                f"{card_id}: unverified card must not be marked eligible",
            )
        elif any(f == "yes" for f in flags.values()):
            # `reviewed-needs-official` and `rejected` are still not official.
            fail(
                "eligibility",
                f"{card_id}: unconfirmed card must not be marked eligible",
            )
    missing = set(preliminary_ids) - audit_ids
    if missing:
        fail(
            "structure",
            f"audit missing {len(missing)} preliminary IDs: {','.join(sorted(missing)[:8])}",
        )
    if require_full:
        expected = {f"{deck}{n:03d}" for deck in "ABCDE" for n in range(1, 169)}
        if set(preliminary_ids) != expected:
            fail("structure", "preliminary ID set is not exactly A001..E168 (840)")
        if len(preliminary) != 840 or len(audit) != 840:
            fail(
                "structure",
                f"expected 840+840 rows, got {len(preliminary)}+{len(audit)}",
            )
        if any(deck_counts[deck] != 168 for deck in "ABCDE"):
            fail("structure", f"expected 168 per deck, actual {dict(deck_counts)}")

    row_by_id = {row.get("card_id", ""): row for row in audit}
    multi_type_ids = sorted(
        card_id
        for card_id, row in row_by_id.items()
        if row.get("card_type_normalized") == "multi_type"
    )
    high_risk_queue = []
    for card_id in sorted({*HIGH_RISK_CARD_IDS, *multi_type_ids}):
        row = row_by_id.get(card_id)
        if row is None:
            continue
        reasons = []
        if card_id in HIGH_RISK_CARD_IDS:
            reasons.append("flagged by the independent #115 review")
        if row.get("card_type_normalized") == "multi_type":
            reasons.append("multi-type card needs dedicated design")
        if not preliminary_ids.get(card_id, {}).get("name_en", "").strip():
            reasons.append("preliminary English name is missing")
        high_risk_queue.append(
            {
                "card_id": card_id,
                "card_type_normalized": row.get("card_type_normalized", ""),
                "review_status": row.get("review_status", ""),
                "eligible_at_2": row.get("eligible_at_2", ""),
                "eligible_at_3": row.get("eligible_at_3", ""),
                "eligible_at_4": row.get("eligible_at_4", ""),
                "reasons": reasons,
            }
        )

    structural_errors = error_categories.get("structure", 0) + error_categories.get("type", 0)
    verified_4p = eligible["4"]["occupation"] >= G1_MIN_PER_KIND and eligible["4"][
        "minor_improvement"
    ] >= G1_MIN_PER_KIND
    g2_4p = eligible["4"]["occupation"] >= G2_TARGET_PER_KIND and eligible["4"][
        "minor_improvement"
    ] >= G2_TARGET_PER_KIND
    report = {
        "schema_version": 2,
        "preliminary_count": len(preliminary),
        "audit_count": len(audit),
        "deck_counts": dict(sorted(deck_counts.items())),
        "review_status_counts": dict(sorted(verified_count.items())),
        "reference_type_counts": dict(sorted(reference_counts.items())),
        "structure_validation": {
            "ok": structural_errors == 0 and len(audit) == len(preliminary),
            "row_count": len(audit),
            "expected_row_count": 840,
            "error_count": structural_errors,
        },
        "official_verification_progress": {
            "verified": verified_count.get("verified", 0),
            "reviewed-needs-official": verified_count.get("reviewed-needs-official", 0),
            "unverified": verified_count.get("unverified", 0),
            "rejected": verified_count.get("rejected", 0),
            "official_verification_complete": bool(audit)
            and verified_count.get("verified", 0) == len(audit),
        },
        "eligibility_progress": {
            n: dict(sorted(eligibility_counts[n].items())) for n in ("2", "3", "4")
        },
        "verified_eligible_by_players": eligible,
        "verified_4p_card_count_gate": verified_4p,
        "g2_target_4p_80_each": g2_4p,
        "gates": {
            "g1_min_per_kind": G1_MIN_PER_KIND,
            "g2_target_per_kind": G2_TARGET_PER_KIND,
        },
        "high_risk_audit_queue": high_risk_queue,
        "stratified_audit_sample": stratified_audit_sample(audit, sample_size),
        "error_categories": dict(sorted(error_categories.items())),
        "errors": errors,
    }
    return report, errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--preliminary", type=Path, default=PRELIMINARY)
    parser.add_argument("--audit", type=Path, default=AUDIT)
    parser.add_argument("--report", type=Path, default=None)
    parser.add_argument(
        "--sample-size",
        type=int,
        default=DEFAULT_SAMPLE_SIZE,
        help="size of the deterministic stratified reviewer work queue",
    )
    args = parser.parse_args()
    if args.sample_size < 0:
        print("catalog validation error: sample-size must be >= 0", file=sys.stderr)
        return 2
    try:
        preliminary = load_rows(args.preliminary)
        audit = load_rows(args.audit)
        missing_fields = set(AUDIT_FIELDS) - set(audit[0]) if audit else set(AUDIT_FIELDS)
        if missing_fields:
            raise ValueError(f"missing audit columns: {sorted(missing_fields)}")
        report, errors = review_ledger(
            preliminary, audit, sample_size=args.sample_size
        )
    except (OSError, ValueError, csv.Error) as exc:
        print(f"catalog validation error: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if args.report:
        args.report.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
