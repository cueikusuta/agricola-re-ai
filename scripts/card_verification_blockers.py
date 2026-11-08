#!/usr/bin/env python3
"""Check that no blocked card was promoted to `verified` (#105 / #115).

`docs/card-verification-blockers.json` records, for the highest-risk cards, what
is still unknown and which official source would resolve it. This script is the
fail-closed link between that register and the audit ledger:

* every card named in the register must exist in `docs/card-catalog-ae-audit.csv`;
* a card that the register still lists as blocked must not be `verified` in the
  ledger, and must not be marked eligible for any player count;
* a register entry must name at least one unknown item, so an "empty" blocker
  cannot be used to look verified without evidence.

It is deliberately *not* a review queue: `scripts/card_inventory_report.py
--cards-csv` produces the ordered list of cards still waiting for official
review (PR #172). This script only guards the cards whose risk is already known.

Usage:

    python3 scripts/card_verification_blockers.py
    python3 scripts/card_verification_blockers.py --json
"""
from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

REGISTER = Path("docs/card-verification-blockers.json")
LEDGER = Path("docs/card-catalog-ae-audit.csv")
ELIGIBILITY_FIELDS = ("eligible_at_2", "eligible_at_3", "eligible_at_4")


def load_ledger(path: Path) -> dict[str, dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as handle:
        return {row["card_id"]: row for row in csv.DictReader(handle)}


def check(register: dict, ledger: dict[str, dict[str, str]]) -> list[str]:
    """Return the list of problems. An empty list means the register holds."""
    problems: list[str] = []
    if register.get("schema_version") != 1:
        problems.append("register: unsupported schema_version")
    cards = register.get("cards")
    if not isinstance(cards, list) or not cards:
        problems.append("register: no cards listed")
        return problems
    seen: set[str] = set()
    for entry in cards:
        card_id = entry.get("card_id")
        if not card_id:
            problems.append("register: entry without card_id")
            continue
        if card_id in seen:
            problems.append(f"{card_id}: listed twice")
        seen.add(card_id)
        if not entry.get("unknown"):
            problems.append(f"{card_id}: no unknown item recorded")
        if not entry.get("needed_source"):
            problems.append(f"{card_id}: no needed_source recorded")
        row = ledger.get(card_id)
        if row is None:
            problems.append(f"{card_id}: not in the audit ledger")
            continue
        if row.get("review_status") == "verified":
            problems.append(
                f"{card_id}: the register still lists it as blocked but the ledger says "
                "verified; resolve the blockers and remove the entry first")
        for field in ELIGIBILITY_FIELDS:
            if row.get(field) == "yes":
                problems.append(
                    f"{card_id}: {field}=yes while the card is still blocked; player "
                    "eligibility must be confirmed before it can enter a pool")
    return problems


def summary(register: dict, ledger: dict[str, dict[str, str]]) -> dict:
    blocked = [entry["card_id"] for entry in register["cards"]]
    verified = sorted(
        card_id for card_id, row in ledger.items()
        if row.get("review_status") == "verified")
    eligible_4 = sorted(
        card_id for card_id, row in ledger.items()
        if row.get("review_status") == "verified" and row.get("eligible_at_4") == "yes")
    return {
        "ledger_cards": len(ledger),
        "blocked_cards": blocked,
        "verified_cards": verified,
        "verified_eligible_at_4": eligible_4,
        "four_player_pool_gate_open": bool(eligible_4),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--register", type=Path, default=REGISTER)
    parser.add_argument("--ledger", type=Path, default=LEDGER)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        register = json.loads(args.register.read_text(encoding="utf-8"))
        ledger = load_ledger(args.ledger)
        problems = check(register, ledger)
        report = summary(register, ledger)
        report["problems"] = problems
        report["register_holds"] = not problems
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if not problems else 1
    except (OSError, KeyError, json.JSONDecodeError) as exc:
        print(f"card verification blockers failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
