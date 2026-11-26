#!/usr/bin/env python3
"""Analyze implemented cards and unimplemented BGA spec unlock priorities.

Stdlib-only helper for card implementation planning.

It mechanically extracts implemented card IDs (``[A-E]\\d{3}``) from production
code in ``src/cards.rs`` (excluding all ``#[cfg(test)]`` items/blocks and
comments), cross-references them against ``data/bga-card-specs/*.json``, and
reports:

* count of implemented cards;
* count and sorted IDs of Lane-B ready cards (``review.status == "bga_checked"``
  and expressible using only the currently supported engine vocabulary);
* rankings of unimplemented ``trigger`` / ``predicate`` / ``operation`` /
  ``other.kind`` vocabulary items by total impacted cards and solo-unlock count
  (cards blocked solely by that one missing vocabulary item).

Usage:
    python3 scripts/analyze_card_unlocks.py
    python3 scripts/analyze_card_unlocks.py --json
    python3 scripts/analyze_card_unlocks.py --top 25
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CARDS_RS = ROOT / "src" / "cards.rs"
DEFAULT_SPECS_DIR = ROOT / "data" / "bga-card-specs"

CARD_ID_LITERAL_RE = re.compile(r'"([A-E]\d{3})"')
RAW_STRING_START_RE = re.compile(r'r(#*)"')

# Synonym trigger normalization.
TRIGGER_ALIASES: dict[str, str] = {
    "ongoing": "passive",
    "end_game_scoring": "end_scoring",
    "scoring": "end_scoring",
    "on_action_use": "after_worker_space",
    "on_action_space_use": "after_worker_space",
    "on_action_space_used": "after_worker_space",
    "returning_home_phase": "round_end",
}

# Vocabulary currently supported by reusable CardEffect / PassiveRule /
# HarvestFieldRule / CardRequirement / ScoreBonusRule variants in src/cards.rs.
SUPPORTED_TRIGGERS: frozenset[str] = frozenset(
    {
        "on_play",
        "after_worker_space",
        "passive",
        "round_start",
        "end_scoring",
        "harvest_field_phase",
        "harvest_feeding",
        "after_harvest",
        "after_occupation",
        "after_improvement",
        "after_clay_construction",
        "after_bake_bread",
        "after_fence_build",
        "harvest_start",
        "when_buying",
    }
)

SUPPORTED_OPERATIONS: frozenset[str] = frozenset(
    {
        "gain_resource",
        "gain_points",
        "discount_cost",
        "modify_capacity",
        "build_stable",
        "exchange",
        "other",  # Checked further via other.kind.
    }
)

SUPPORTED_PREDICATES: frozenset[str] = frozenset(
    {
        "worker_space_used",
        "any_player",
        "house_is",
        "room_count",
        "room_count_in",
        "rooms_at_least",
        "has_stone_house",
        "resource_at_least",
        "resource_greater_than",
        "accumulation_at_least",
        "accumulation_grew_this_round",
        "already_owns_animal_type",
        "animal_count_at_least",
        "at_least_animals",
        "all_animal_types",
        "at_least_fields",
        "field_count",
        "empty_field_count",
        "grain_field_count",
        "grain_harvested_at_least",
        "pasture_count",
        "pasture_size",
        "fence_count_at_least_round",
        "fences_built_at_least",
        "occupation_count",
        "at_least_occupations",
        "at_most_occupations",
        "improvement_count",
        "improvement_played_from_hand",
        "farmyard_all_used",
        "stone_rooms_exceed_people",
        "goods_in_fields_before_final_harvest_at_least",
        "round_has_no_harvest",
        "family_exactly",
        "lessons_occupants_at_least",
        "goods_on_upcoming_round_spaces",
        "space_occupied",
        "other_player_uses",
        "min_players",
        "max_players",
    }
)

SUPPORTED_OTHER_KINDS: frozenset[str] = frozenset(
    {
        "allow_major_via_minor",
        "gain_each_with_at_least",
        "fence_pasture",
        "add_good_to_empty_fields",
        "forbid_house_animals",
    }
)

CATEGORIES: tuple[str, ...] = ("trigger", "predicate", "operation", "other.kind")


def _skip_line_comment(code: str, pos: int) -> int:
    nl = code.find("\n", pos)
    return len(code) if nl == -1 else nl + 1


def _skip_block_comment(code: str, pos: int) -> int:
    n = len(code)
    depth = 1
    pos += 2
    while pos < n and depth > 0:
        if code.startswith("/*", pos):
            depth += 1
            pos += 2
        elif code.startswith("*/", pos):
            depth -= 1
            pos += 2
        else:
            pos += 1
    return pos


def _skip_string(code: str, pos: int) -> int:
    n = len(code)
    pos += 1
    while pos < n:
        ch = code[pos]
        if ch == "\\":
            pos += 2
        elif ch == '"':
            return pos + 1
        else:
            pos += 1
    return n


def _skip_raw_string(code: str, pos: int) -> int | None:
    match = RAW_STRING_START_RE.match(code, pos)
    if match is None:
        return None
    end_delim = '"' + match.group(1)
    end = code.find(end_delim, match.end())
    return len(code) if end == -1 else end + len(end_delim)


def _skip_char_or_lifetime(code: str, pos: int) -> int:
    n = len(code)
    if pos + 1 < n and code[pos + 1] == "\\":
        end = code.find("'", pos + 2)
        return pos + 1 if end == -1 else end + 1
    if pos + 2 < n and code[pos + 2] == "'":
        return pos + 3
    return pos + 1


def _skip_attribute(code: str, pos: int) -> int:
    """Skip a Rust attribute starting at ``#[``."""
    n = len(code)
    depth = 1
    pos += 2
    while pos < n and depth > 0:
        if code.startswith("//", pos):
            pos = _skip_line_comment(code, pos)
        elif code.startswith("/*", pos):
            pos = _skip_block_comment(code, pos)
        elif code[pos] == '"':
            pos = _skip_string(code, pos)
        elif code[pos] == "[":
            depth += 1
            pos += 1
        elif code[pos] == "]":
            depth -= 1
            pos += 1
        else:
            pos += 1
    return pos


def _skip_attributed_item(code: str, pos: int) -> int:
    """Skip any stacked attributes and the Rust item/variant/arm following ``#[cfg(test)]``."""
    n = len(code)
    while pos < n:
        if code[pos].isspace():
            pos += 1
        elif code.startswith("//", pos):
            pos = _skip_line_comment(code, pos)
        elif code.startswith("/*", pos):
            pos = _skip_block_comment(code, pos)
        elif code.startswith("#[", pos):
            pos = _skip_attribute(code, pos)
        else:
            break

    brace_depth = 0
    seen_brace = False
    while pos < n:
        if code.startswith("//", pos):
            pos = _skip_line_comment(code, pos)
        elif code.startswith("/*", pos):
            pos = _skip_block_comment(code, pos)
        elif code[pos] == '"':
            pos = _skip_string(code, pos)
        elif code[pos] == "r" and pos + 1 < n and code[pos + 1] in '#"':
            raw_end = _skip_raw_string(code, pos)
            pos = raw_end if raw_end is not None else pos + 1
        elif code[pos] == "'":
            pos = _skip_char_or_lifetime(code, pos)
        elif code[pos] == "{":
            brace_depth += 1
            seen_brace = True
            pos += 1
        elif code[pos] == "}":
            brace_depth -= 1
            pos += 1
            if seen_brace and brace_depth == 0:
                j = pos
                while j < n and code[j] in " \t":
                    j += 1
                if j < n and code[j] == ",":
                    pos = j + 1
                return pos
        elif brace_depth == 0 and code[pos] in ";,":
            return pos + 1
        else:
            pos += 1
    return pos


def strip_cfg_test_and_comments(code: str) -> str:
    """Return Rust source with ``#[cfg(test)]`` items/blocks and comments removed."""
    n = len(code)
    out: list[str] = []
    i = 0
    while i < n:
        if code.startswith("//", i):
            i = _skip_line_comment(code, i)
            out.append("\n")
        elif code.startswith("/*", i):
            i = _skip_block_comment(code, i)
            out.append(" ")
        elif code[i] == '"':
            end = _skip_string(code, i)
            out.append(code[i:end])
            i = end
        elif code[i] == "r" and i + 1 < n and code[i + 1] in '#"':
            raw_end = _skip_raw_string(code, i)
            if raw_end is not None:
                out.append(code[i:raw_end])
                i = raw_end
            else:
                out.append(code[i])
                i += 1
        elif code[i] == "'":
            end = _skip_char_or_lifetime(code, i)
            out.append(code[i:end])
            i = end
        elif code.startswith("#[cfg(test)]", i):
            i = _skip_attributed_item(code, i + len("#[cfg(test)]"))
        else:
            out.append(code[i])
            i += 1
    return "".join(out)


def extract_implemented_card_ids(cards_rs: Path) -> list[str]:
    """Mechanically extract implemented ``[A-E]\\d{3}`` IDs from production code."""
    source = cards_rs.read_text(encoding="utf-8")
    prod_code = strip_cfg_test_and_comments(source)
    return sorted(set(CARD_ID_LITERAL_RE.findall(prod_code)))


def normalize_trigger(trigger: str) -> str:
    return TRIGGER_ALIASES.get(trigger, trigger)


def find_missing_vocabulary(spec: dict[str, Any]) -> set[tuple[str, str]] | None:
    """Return missing ``(category, name)`` pairs for a spec, or None if effects is null."""
    effects = spec.get("effects")
    if effects is None:
        return None

    missing: set[tuple[str, str]] = set()

    play = spec.get("play") or {}
    for prereq in play.get("prerequisites") or []:
        if isinstance(prereq, dict):
            pred = prereq.get("predicate") or prereq.get("kind")
            if isinstance(pred, str) and pred and pred not in SUPPORTED_PREDICATES:
                missing.add(("predicate", pred))

    for effect in effects:
        if not isinstance(effect, dict):
            continue
        raw_trigger = effect.get("trigger")
        if isinstance(raw_trigger, str) and raw_trigger:
            norm_trigger = normalize_trigger(raw_trigger)
            if norm_trigger not in SUPPORTED_TRIGGERS:
                missing.add(("trigger", norm_trigger))

        for cond in effect.get("conditions") or []:
            if isinstance(cond, dict):
                pred = cond.get("predicate")
                if isinstance(pred, str) and pred and pred not in SUPPORTED_PREDICATES:
                    missing.add(("predicate", pred))

        for step in effect.get("steps") or []:
            if not isinstance(step, dict):
                continue
            op = step.get("operation")
            if isinstance(op, str) and op and op not in SUPPORTED_OPERATIONS:
                missing.add(("operation", op))
            if op == "other":
                params = step.get("params") if isinstance(step.get("params"), dict) else {}
                kind = params.get("kind") or step.get("kind")
                if not isinstance(kind, str) or not kind:
                    kind = "<unspecified>"
                if kind not in SUPPORTED_OTHER_KINDS:
                    missing.add(("other.kind", kind))

    return missing


def analyze(
    cards_rs: Path,
    specs_dir: Path,
    *,
    include_all_statuses: bool = False,
) -> dict[str, Any]:
    implemented_ids = extract_implemented_card_ids(cards_rs)
    implemented_set = set(implemented_ids)

    specs: dict[str, dict[str, Any]] = {}
    for path in sorted(specs_dir.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        specs[data["card_id"]] = data

    bga_checked_ids: list[str] = []
    bga_checked_unimplemented_ids: list[str] = []
    lane_b_ready_ids: list[str] = []

    impact_cards: dict[tuple[str, str], list[str]] = defaultdict(list)
    solo_cards: dict[tuple[str, str], list[str]] = defaultdict(list)

    for card_id, spec in sorted(specs.items()):
        review_status = (spec.get("review") or {}).get("status")
        is_checked = review_status == "bga_checked"
        if is_checked:
            bga_checked_ids.append(card_id)

        if card_id in implemented_set:
            continue

        if is_checked:
            bga_checked_unimplemented_ids.append(card_id)

        missing = find_missing_vocabulary(spec)
        if missing is None:
            continue

        if is_checked and not missing:
            lane_b_ready_ids.append(card_id)
            continue

        if not is_checked and not include_all_statuses:
            continue

        if not missing:
            continue

        for feat in sorted(missing):
            impact_cards[feat].append(card_id)
        if len(missing) == 1:
            solo_cards[next(iter(missing))].append(card_id)

    def build_entries(items: list[tuple[str, str]]) -> list[dict[str, Any]]:
        entries = []
        for cat, name in items:
            imp = impact_cards[(cat, name)]
            sol = solo_cards[(cat, name)]
            entries.append(
                {
                    "category": cat,
                    "name": name,
                    "impact_count": len(imp),
                    "solo_unlock_count": len(sol),
                    "solo_unlock_ids": sol,
                    "impact_sample_ids": imp[:10],
                }
            )
        entries.sort(
            key=lambda e: (-e["impact_count"], -e["solo_unlock_count"], e["category"], e["name"])
        )
        return entries

    all_missing_keys = list(impact_cards.keys())
    overall_ranking = build_entries(all_missing_keys)
    by_category: dict[str, list[dict[str, Any]]] = {}
    for cat in CATEGORIES:
        cat_keys = [k for k in all_missing_keys if k[0] == cat]
        by_category[cat] = build_entries(cat_keys)

    status_counts = Counter(
        (spec.get("review") or {}).get("status", "unknown") for spec in specs.values()
    )

    return {
        "cards_rs": str(cards_rs.relative_to(ROOT)) if cards_rs.is_relative_to(ROOT) else str(cards_rs),
        "specs_dir": str(specs_dir.relative_to(ROOT)) if specs_dir.is_relative_to(ROOT) else str(specs_dir),
        "ranking_scope": "all_specs_with_effects" if include_all_statuses else "bga_checked_unimplemented",
        "implemented_count": len(implemented_ids),
        "implemented_with_spec_count": sum(1 for cid in implemented_ids if cid in specs),
        "implemented_without_spec_ids": [cid for cid in implemented_ids if cid not in specs],
        "implemented_ids": implemented_ids,
        "total_specs": len(specs),
        "specs_by_review_status": dict(sorted(status_counts.items())),
        "bga_checked_count": len(bga_checked_ids),
        "bga_checked_unimplemented_count": len(bga_checked_unimplemented_ids),
        "lane_b_ready_count": len(lane_b_ready_ids),
        "lane_b_ready_ids": lane_b_ready_ids,
        "trigger_aliases": TRIGGER_ALIASES,
        "missing_overall": overall_ranking,
        "missing_by_category": by_category,
    }


def format_text_report(report: dict[str, Any], top: int) -> str:
    lines: list[str] = []
    lines.append("=== Card Unlock Analysis ===")
    lines.append(f"Implemented cards (src/cards.rs prod): {report['implemented_count']}")
    lines.append(f"Total BGA card specs:                  {report['total_specs']}")
    lines.append(f"  bga_checked total:                   {report['bga_checked_count']}")
    lines.append(
        f"  bga_checked unimplemented:           {report['bga_checked_unimplemented_count']}"
    )
    lines.append(f"Lane-B ready (existing vocab only):    {report['lane_b_ready_count']}")
    lines.append("")
    lines.append("Lane-B ready card IDs:")
    ready_ids: list[str] = report["lane_b_ready_ids"]
    if ready_ids:
        for i in range(0, len(ready_ids), 12):
            lines.append("  " + " ".join(ready_ids[i : i + 12]))
    else:
        lines.append("  (none)")

    scope_label = report["ranking_scope"]
    lines.append("")
    lines.append(f"=== Overall Unimplemented Vocabulary Ranking ({scope_label}, top {top}) ===")
    lines.append(f"  {'Rank':>4}  {'Category':<10}  {'Name':<34}  {'Impact':>6}  {'Solo':>4}")
    for idx, entry in enumerate(report["missing_overall"][:top], 1):
        lines.append(
            f"  {idx:>4}  {entry['category']:<10}  {entry['name']:<34}  "
            f"{entry['impact_count']:>6}  {entry['solo_unlock_count']:>4}"
        )

    for cat in CATEGORIES:
        entries = report["missing_by_category"][cat]
        lines.append("")
        lines.append(f"--- Missing {cat} (top {top} / {len(entries)} total) ---")
        if not entries:
            lines.append("  (none)")
            continue
        lines.append(f"  {'Rank':>4}  {'Name':<38}  {'Impact':>6}  {'Solo':>4}")
        for idx, entry in enumerate(entries[:top], 1):
            lines.append(
                f"  {idx:>4}  {entry['name']:<38}  "
                f"{entry['impact_count']:>6}  {entry['solo_unlock_count']:>4}"
            )

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Analyze implemented cards and unimplemented BGA card-spec unlock priorities."
    )
    parser.add_argument(
        "--cards-rs",
        type=Path,
        default=DEFAULT_CARDS_RS,
        help="Path to src/cards.rs (default: repo src/cards.rs)",
    )
    parser.add_argument(
        "--specs-dir",
        type=Path,
        default=DEFAULT_SPECS_DIR,
        help="Path to data/bga-card-specs directory",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output machine-readable JSON",
    )
    parser.add_argument(
        "--top",
        type=int,
        default=15,
        help="Number of rows to display per ranking in text mode (default: 15)",
    )
    parser.add_argument(
        "--include-all-statuses",
        action="store_true",
        help="Include non-bga_checked specs with non-null effects in missing-vocabulary rankings",
    )
    args = parser.parse_args(argv)

    if args.top < 1:
        print("error: --top must be >= 1", file=sys.stderr)
        return 2
    if not args.cards_rs.is_file():
        print(f"error: cards.rs not found: {args.cards_rs}", file=sys.stderr)
        return 2
    if not args.specs_dir.is_dir():
        print(f"error: specs directory not found: {args.specs_dir}", file=sys.stderr)
        return 2

    report = analyze(
        args.cards_rs,
        args.specs_dir,
        include_all_statuses=args.include_all_statuses,
    )
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        print(format_text_report(report, args.top))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
