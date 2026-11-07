#!/usr/bin/env python3
"""M10 A-1: inventory of the A-E card ledger, separate from official review.

The task asks for six things that are easy to conflate. This tool keeps them
apart, and only answers the ones that can be answered without official
material:

1. present in the ledger            -> counted here (840 rows).
2. officially reviewed              -> NOT answered here; only
                                       docs/card-catalog-ae-audit.csv's
                                       review_status can say, see #115/#141.
3. effect implemented in Rust       -> counted here, from the preliminary
                                       ledger's implementation_reference.
4. distributed in a 4-player pool   -> NOT answered here; needs the verified
                                       pool gate (#139) plus M10 G1.

It also groups the *unimplemented* cards by preliminary effect category and
preliminary difficulty, which is the input for choosing reusable engine
work (A-4): a category with many simple unimplemented cards is a better
shared-mechanism candidate than a deep one that appears twice.

Nothing here is official. `effect_categories_preliminary` and
`difficulty_1_to_5_preliminary` are an automatic first pass; they prioritise
review work and must not be used as a rules source or as evidence that a card
is legal in a given player count.
"""
from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from collections import Counter
from pathlib import Path

IMPLEMENTED_REFERENCES = {
    "コード内に定義あり（配布対象要照合）",
    "コード内に定義あり・2人戦プリセット外",
}
UNIMPLEMENTED = "未実装または未照合"
CATEGORY_SEPARATOR = "／"
OCCUPATION = "職業"
MINOR = "小進歩"
# The ledger's own type strings, including the composite spellings. A word
# that matches none of these would silently disappear from every total, so the
# loader rejects it instead.
KNOWN_TYPES = {
    OCCUPATION: OCCUPATION,
    MINOR: MINOR,
    "小進歩  大進歩": "multi_type",
    "小進歩・大進歩・職業（複合）": "multi_type",
}


def load_rows(path: Path) -> list[dict]:
    with path.open(encoding="utf-8", newline="") as file:
        rows = list(csv.DictReader(file))
    if not rows:
        raise ValueError("empty card ledger")
    return rows


def normalize_type(raw: str) -> str:
    if raw not in KNOWN_TYPES:
        raise ValueError(f"unknown card type in ledger: {raw!r}")
    return KNOWN_TYPES[raw]


def is_implemented(reference: str) -> bool:
    if reference == UNIMPLEMENTED:
        return False
    if reference in IMPLEMENTED_REFERENCES:
        return True
    raise ValueError(f"unknown implementation_reference: {reference!r}")


def categories(raw: str) -> list[str]:
    return [part for part in raw.split(CATEGORY_SEPARATOR) if part.strip()]


def inventory(rows: list[dict], source: str = "docs/card-catalog-ae-preliminary.csv") -> dict:
    by_deck = Counter()
    by_type = Counter()
    by_base = Counter()
    implemented = Counter()
    unimplemented = Counter()
    difficulty = {"implemented": Counter(), "unimplemented": Counter()}
    category_total = Counter()
    category_by_type = {OCCUPATION: Counter(), MINOR: Counter()}
    category_unimplemented = Counter()
    category_unimplemented_by_level = {"1-3": Counter(), "4-5": Counter()}
    seen_ids = set()

    for row in rows:
        card_id = row["card_id"]
        if not card_id or card_id in seen_ids:
            raise ValueError(f"duplicate or empty card_id: {card_id!r}")
        seen_ids.add(card_id)
        deck = row["deck"]
        if deck not in "ABCDE" or re.fullmatch(r"[A-E][0-9]{3}", card_id) is None or card_id[0] != deck:
            raise ValueError(f"{card_id}: invalid deck/card_id pairing: {deck!r}")
        if row["in_base_2016"] not in ("YES", "NO"):
            raise ValueError(f"{card_id}: invalid in_base_2016")
        kind = normalize_type(row["card_type"])
        level = row["difficulty_1_to_5_preliminary"]
        if level not in {"1", "2", "3", "4", "5"}:
            raise ValueError(f"{card_id}: invalid preliminary difficulty {level!r}")
        by_deck[row["deck"]] += 1
        by_type[kind] += 1
        by_base[row["in_base_2016"]] += 1
        state = "implemented" if is_implemented(row["implementation_reference"]) else "unimplemented"
        (implemented if state == "implemented" else unimplemented)[kind] += 1
        difficulty[state][level] += 1
        for category in categories(row["effect_categories_preliminary"]):
            category_total[category] += 1
            if kind in category_by_type:
                category_by_type[kind][category] += 1
            if state == "unimplemented":
                category_unimplemented[category] += 1
                bucket = "1-3" if int(level) <= 3 else "4-5"
                category_unimplemented_by_level[bucket][category] += 1

    built = sum(implemented.values())
    return {
        "schema_version": 1,
        "source": source,
        "official_verification_included": False,
        "caveat": (
            "Ledger presence and Rust implementation only. Official review "
            "state lives in docs/card-catalog-ae-audit.csv and is reported by "
            "#115/#141; 4-player distributability needs the verified pool gate. "
            "effect_categories_preliminary and difficulty are an automatic "
            "first pass, not a rules source."
        ),
        "totals": {
            "cards": len(rows),
            "by_deck": dict(sorted(by_deck.items())),
            "by_type": dict(sorted(by_type.items())),
            "in_base_2016": dict(sorted(by_base.items())),
            "implemented": dict(sorted(implemented.items())),
            "implemented_total": built,
            "unimplemented": dict(sorted(unimplemented.items())),
            "unimplemented_total": sum(unimplemented.values()),
        },
        "implemented_share_by_type": {
            kind: (implemented[kind] / by_type[kind] if by_type[kind] else 0.0)
            for kind in sorted(by_type)
        },
        "difficulty": {
            state: {str(level): counts[str(level)] for level in range(1, 6)}
            for state, counts in (
                ("implemented", difficulty["implemented"]),
                ("unimplemented", difficulty["unimplemented"]),
            )
        },
        "effect_categories": {
            "total": dict(category_total.most_common()),
            "occupation": dict(category_by_type[OCCUPATION].most_common()),
            "minor_improvement": dict(category_by_type[MINOR].most_common()),
            "unimplemented": dict(category_unimplemented.most_common()),
            "unimplemented_by_difficulty": {
                bucket: dict(counts.most_common())
                for bucket, counts in category_unimplemented_by_level.items()
            },
        },
        "shared_mechanism_candidates": [
            {"category": category, "unimplemented_cards": count}
            for category, count in category_unimplemented_by_level["1-3"].most_common(6)
        ],
    }


QUEUE_FIELDS = (
    "card_id",
    "deck",
    "card_type",
    "name_en",
    "name_ja",
    "difficulty_1_to_5_preliminary",
    "effect_categories_preliminary",
    "in_base_2016",
    "implementation_state",
    "official_review_required",
    "distributable_now",
)


def verification_queue(rows: list[dict], max_difficulty: int = 3) -> list[dict]:
    """Per-card work queue for the 40+40 gate, ordered by preliminary difficulty.

    The queue lets a human reviewer work through candidates in the A-3 priority
    order (cheap effects first). It is **not** a verification: every row says
    `official_review_required: true` and `distributable_now: false`, because no
    card in this repository is officially verified and this tool cannot verify
    one. A queue row never adds a card to a distributable pool.
    """
    if not 1 <= max_difficulty <= 5:
        raise ValueError("max_difficulty must be 1..=5")
    queue = []
    for row in rows:
        if is_implemented(row["implementation_reference"]):
            continue
        level = row["difficulty_1_to_5_preliminary"]
        if level not in {"1", "2", "3", "4", "5"}:
            raise ValueError(
                f"{row['card_id']}: invalid preliminary difficulty {level!r}")
        if int(level) > max_difficulty:
            continue
        queue.append({
            "card_id": row["card_id"],
            "deck": row["deck"],
            "card_type": normalize_type(row["card_type"]),
            "name_en": row.get("name_en", ""),
            "name_ja": row.get("name_ja", ""),
            "difficulty_1_to_5_preliminary": level,
            "effect_categories_preliminary": row["effect_categories_preliminary"],
            "in_base_2016": row["in_base_2016"],
            "implementation_state": "unimplemented",
            # Never true until docs/card-catalog-ae-audit.csv records an
            # official source for this card.
            "official_review_required": True,
            "distributable_now": False,
        })
    queue.sort(
        key=lambda entry: (int(entry["difficulty_1_to_5_preliminary"]), entry["card_id"])
    )
    return queue


def write_queue_csv(queue: list[dict], path: Path) -> None:
    with path.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=QUEUE_FIELDS)
        writer.writeheader()
        writer.writerows(queue)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--ledger",
        type=Path,
        default=Path("docs/card-catalog-ae-preliminary.csv"),
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--cards-csv",
        type=Path,
        help="Write the per-card official-review queue (unimplemented, difficulty<=3)",
    )
    parser.add_argument(
        "--max-difficulty",
        type=int,
        default=3,
        help="Highest preliminary difficulty to include in --cards-csv (1..5)",
    )
    args = parser.parse_args()
    try:
        rows = load_rows(args.ledger)
        report = inventory(rows, source=str(args.ledger))
        if args.cards_csv:
            queue = verification_queue(rows, max_difficulty=args.max_difficulty)
            args.cards_csv.parent.mkdir(parents=True, exist_ok=True)
            write_queue_csv(queue, args.cards_csv)
            report["verification_queue"] = {
                "path": str(args.cards_csv),
                "cards": len(queue),
                "max_difficulty": args.max_difficulty,
                "official_review_required": True,
                "distributable_now": False,
                "note": (
                    "A work queue for human official review. A queue row is not "
                    "a verified card and never enters a distributable pool."
                ),
            }
    except (OSError, ValueError, KeyError) as exc:
        print(f"card inventory failed: {exc}", file=sys.stderr)
        return 1
    text = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
