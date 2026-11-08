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

# Reuse the inventory's canonical meaning of an implemented effect.
# The ledger alone records official review, not Rust implementation.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from scripts.card_inventory_report import is_implemented  # noqa: E402

REGISTER = Path("docs/card-verification-blockers.json")
LEDGER = Path("docs/card-catalog-ae-audit.csv")
PRELIMINARY = Path("docs/card-catalog-ae-preliminary.csv")
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


def load_preliminary(path: Path) -> dict[str, dict[str, str]]:
    """Map source-card IDs to implementation references; reject duplicate IDs."""
    rows: dict[str, dict[str, str]] = {}
    with path.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            card_id = row["card_id"]
            if not card_id or card_id in rows:
                raise ValueError(f"preliminary ledger has a duplicate/blank card id: {card_id!r}")
            rows[card_id] = row
    if not rows:
        raise ValueError("preliminary ledger is empty")
    return rows


def summary(
    register: dict, ledger: dict[str, dict[str, str]],
    preliminary: dict[str, dict[str, str]] | None = None,
) -> dict:
    blocked = [entry["card_id"] for entry in register["cards"]]
    verified = sorted(
        card_id for card_id, row in ledger.items()
        if row.get("review_status") == "verified")
    eligible_4 = sorted(
        card_id for card_id, row in ledger.items()
        if row.get("review_status") == "verified" and row.get("eligible_at_4") == "yes")
    # A verified 4p card is NOT necessarily playable: a genuine 10-7 pool
    # needs >=40 implemented AND officially reviewed occupation cards and
    # >=40 implemented AND officially reviewed minor improvements.
    # The preliminary file provides implementation references only; its
    # preliminary rules classification does NOT itself grant eligibility.
    implemented_by_kind = {"occupation": 0, "minor_improvement": 0}
    for card_id in eligible_4:
        if card_id in blocked:
            continue
        row = (preliminary or {}).get(card_id)
        if row is None:
            continue  # fail closed if the implementation record is unavailable
        if not is_implemented(row.get("implementation_reference", "")):
            continue
        kind = ledger[card_id].get("card_type_normalized")
        if kind in implemented_by_kind:
            implemented_by_kind[kind] += 1
        # Composite types never count toward either 40-card minimum.
    shortages = {kind: max(0, 40 - count) for kind, count in implemented_by_kind.items()}
    return {
        "ledger_cards": len(ledger),
        "blocked_cards": blocked,
        "verified_cards": verified,
        "verified_eligible_at_4": eligible_4,
        "implemented_verified_eligible_at_4_by_kind": implemented_by_kind,
        "four_player_pool_shortages": shortages,
        "four_player_pool_gate_open": all(count >= 40 for count in implemented_by_kind.values()),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--register", type=Path, default=REGISTER)
    parser.add_argument("--ledger", type=Path, default=LEDGER)
    parser.add_argument("--preliminary", type=Path, default=PRELIMINARY)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        register = json.loads(args.register.read_text(encoding="utf-8"))
        ledger = load_ledger(args.ledger)
        preliminary = load_preliminary(args.preliminary)
        problems = check(register, ledger)
        report = summary(register, ledger, preliminary)
        report["problems"] = problems
        report["register_holds"] = not problems
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if not problems else 1
    except (OSError, KeyError, json.JSONDecodeError) as exc:
        print(f"card verification blockers failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
