#!/usr/bin/env python3
"""Resolve per-card player-count eligibility from the official card-number rule.

Issue #202.  The third-party BGA wiki does not state per-card player counts, so
``eligible_2/3/4`` stayed ``unknown``.  The Lookout rulebook "Consul Dirigens
Rules" (p.4) defines the numbering rule for the revised-edition card sets::

    001-084  minor improvements, usable with 2-4 players
    085-126  occupations,        usable with 2-4 players
    127-147  occupations,        usable with 3-4 players
    148-168  occupations,        usable with 4 players only

This script applies that rule to the BGA reference base ledger for decks A-D and
records the basis in two dedicated columns.  Deck E is deliberately left
``unknown``: the task requires confirming E separately instead of extending the
rule unconditionally.  The official-verification gate
(``docs/card-catalog-ae-audit.csv``) is never touched here.

Usage::

    python3 scripts/apply_card_eligibility.py [--check]

``--check`` reports the diff without writing.
"""
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "data" / "bga-card-base-ledger.csv"
SPECS = ROOT / "data" / "bga-card-specs"
RULEBOOK = "https://shop.asmodee.ca/img/product/description/EN-AGRICOLA-CONSUL_DIRIGENS-RULES.pdf"
# (first, last, eligible_2, eligible_3, eligible_4)
RANGES = [
    (1, 84, "yes", "yes", "yes"),
    (85, 126, "yes", "yes", "yes"),
    (127, 147, "no", "yes", "yes"),
    (148, 168, "no", "no", "yes"),
]
DECKS_WITH_RULE = {"A", "B", "C", "D"}
EXTRA_COLUMNS = ["eligibility_basis", "eligibility_source_url"]


def rule_for(number: int) -> tuple[str, str, str] | None:
    for first, last, two, three, four in RANGES:
        if first <= number <= last:
            return two, three, four
    return None


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv[1:])

    with BASE.open(encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle)
        fieldnames = list(reader.fieldnames or [])
        rows = list(reader)
    for column in EXTRA_COLUMNS:
        if column not in fieldnames:
            fieldnames.append(column)

    changed = 0
    unresolved = 0
    for row in rows:
        card_id = row["card_id"]
        deck = row["deck"]
        number = int(card_id[1:])
        resolved = rule_for(number) if deck in DECKS_WITH_RULE else None
        if resolved is None:
            unresolved += 1
            row["eligibility_basis"] = "unknown"
            row["eligibility_source_url"] = ""
            continue
        two, three, four = resolved
        before = (row.get("eligible_2"), row.get("eligible_3"), row.get("eligible_4"))
        row["eligible_2"], row["eligible_3"], row["eligible_4"] = two, three, four
        row["eligibility_basis"] = "consul_dirigens_rule"
        row["eligibility_source_url"] = RULEBOOK
        if before != (two, three, four):
            changed += 1

    summary = {
        "rows": len(rows),
        "changed": changed,
        "unresolved_deck_E": unresolved,
        "eligible_4_yes": sum(1 for row in rows if row.get("eligible_4") == "yes"),
        "eligible_4_no": sum(1 for row in rows if row.get("eligible_4") == "no"),
        "eligible_4_unknown": sum(1 for row in rows if row.get("eligible_4") == "unknown"),
        "check_only": args.check,
    }
    if not args.check:
        with BASE.open("w", encoding="utf-8-sig", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=fieldnames)
            writer.writeheader()
            writer.writerows(rows)
        # keep the per-card snapshots in step with the ledger
        updated = 0
        for path in sorted(SPECS.glob("*.json")):
            item = json.loads(path.read_text(encoding="utf-8"))
            row = next(r for r in rows if r["card_id"] == item["card_id"])
            eligibility = {
                "2": row["eligible_2"],
                "3": row["eligible_3"],
                "4": row["eligible_4"],
            }
            if item["bga"]["eligibility"] != eligibility:
                item["bga"]["eligibility"] = eligibility
                path.write_text(
                    json.dumps(item, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8",
                )
                updated += 1
        summary["spec_snapshots_updated"] = updated
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
