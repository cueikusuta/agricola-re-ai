#!/usr/bin/env python3
"""Cross-check the three BGA card data sources for the same card.

Sources:

* ``data/bga-card-base-ledger.csv`` (ledger: one row per card id)
* ``data/bga-card-specs/<ID>.json`` (spec: executable semantics)
* ``data/bga-card-tests/<ID>.json`` (test contract: expected outcomes)

The script reports where the three records disagree on identity, cost,
prerequisites, printed points and effect shape.  It does not decide which side
is right: the caller verifies each finding against the card face / literature.
Stdlib only, read-only.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import csv
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "data" / "bga-card-base-ledger.csv"
SPECS = ROOT / "data" / "bga-card-specs"
TESTS = ROOT / "data" / "bga-card-tests"

GOOD_ALIASES = {
    "food": "food",
    "wood": "wood",
    "clay": "clay",
    "brick": "clay",
    "stone": "stone",
    "reed": "reed",
    "reeds": "reed",
    "grain": "grain",
    "wheat": "grain",
    "vegetable": "vegetable",
    "sheep": "sheep",
    "wild_boar": "wild_boar",
    "boar": "wild_boar",
    "cattle": "cattle",
}
# Japanese ledger names of goods/resource columns -> canonical good
LEDGER_GOODS = {
    "食料": "food",
    "木材": "wood",
    "レンガ": "clay",
    "石": "stone",
    "葦": "reed",
    "小麦": "grain",
    "野菜": "vegetable",
}
# player-count eligibility from the Consul Dirigens card-number rule
ELIGIBILITY_RANGES = [
    (1, 84, "yes", "yes", "yes"),
    (85, 126, "yes", "yes", "yes"),
    (127, 147, "no", "yes", "yes"),
    (148, 168, "no", "no", "yes"),
]


def expected_eligibility(card_id: str):
    number = int(card_id[1:])
    for first, last, two, three, four in ELIGIBILITY_RANGES:
        if first <= number <= last:
            return {"2": two, "3": three, "4": four}
    return None


def read_ledger():
    with LEDGER.open(encoding="utf-8-sig", newline="") as handle:
        return list(csv.DictReader(handle))


def read_records(directory: Path):
    records = {}
    path_mismatch = []
    for path in sorted(directory.glob("*.json")):
        item = json.loads(path.read_text(encoding="utf-8"))
        card_id = item.get("card_id")
        if path.stem != card_id:
            path_mismatch.append(f"{path.name} holds card_id {card_id!r}")
        records[card_id] = item
    return records, path_mismatch


def ledger_cost(row):
    """Normalize the ledger cost column to {good: amount} (None when unknown)."""
    cost = (row.get("cost") or "").strip()
    if cost in ("", "none"):
        return {}
    if cost == "unknown":
        return None
    parts = {}
    for part in cost.split(";"):
        good, amount = part.split(":")
        parts[good] = int(amount)
    return parts


def spec_cost(spec):
    cost = spec.get("play", {}).get("cost")
    if cost is None:
        return None
    if not cost:
        return {}
    return {entry["good"]: entry["amount"] for entry in cost}


SPACE_IDS = {
    "漁": "fishing",
    "穀物活用": "grain_utilization",
    "日雇い労働者": "day_laborer",
}


def normalize_ledger_prerequisite(text):
    """Turn the short Japanese ledger prerequisite into canonical predicates.

    Only the frequent, unambiguous spellings are normalized.  Free prose becomes
    ``("prose", ...)`` so that it is reported as "not comparable" instead of a
    finding.  Canonical names are shared with ``normalize_spec_prerequisite``.
    """
    text = (text or "").strip()
    if text in ("", "none"):
        return []
    canonical = []

    def add(predicate, **params):
        canonical.append((predicate, tuple(sorted((k, str(v)) for k, v in params.items()))))

    match = re.fullmatch(r"(?:ちょうど)?職業(\d+)", text)
    if match:
        return [("occupations", (("n", match.group(1)),))]
    match = re.fullmatch(r"職業(?:最大)?(\d+)以下", text)
    if match:
        return [("occupations_at_most", (("n", match.group(1)),))]
    if text in ("職業なし", "職業を出していない"):
        return [("occupations", (("n", "0"),))]
    match = re.fullmatch(r"職業(\d+)と進歩(\d+)", text)
    if match:
        return [("occupations", (("n", match.group(1)),)),
                ("improvements", (("n", match.group(2)),))]
    match = re.fullmatch(r"職業(\d+)とパン焼き進歩(\d+)", text)
    if match:
        return [("occupations", (("n", match.group(1)),)),
                ("improvement", (("kind", "bake_bread"),))]
    match = re.fullmatch(r"職業(\d+)とストックに建設資材(\d+)", text)
    if match:
        return [("occupations", (("n", match.group(1)),)),
                ("prose", (("text", "ストックに建設資材" + match.group(2) + "以上"),))]
    match = re.fullmatch(r"小麦畑(\d+)", text)
    if match:
        return [("grain_fields", (("n", match.group(1)),))]
    if text == "小麦畑がない":
        return [("grain_fields", (("n", "0"),))]
    match = re.fullmatch(r"(?:畑|畑タイル|ちょうど畑タイル)(\d+)", text)
    if match:
        return [("fields", (("n", match.group(1)),))]
    if text == "畑タイルなし":
        return [("fields", (("n", "0"),))]
    match = re.fullmatch(r"(?:ちょうど)?牧場(\d+)", text)
    if match:
        return [("pastures", (("n", match.group(1)),))]
    match = re.fullmatch(r"野菜畑(\d+)", text)
    if match:
        return [("vegetable_fields", (("n", match.group(1)),))]
    match = re.fullmatch(r"(羊|牛|猪)(\d+)未満", text)
    if match:
        animal = {"羊": "sheep", "牛": "cattle", "猪": "wild_boar"}[match.group(1)]
        return [("animals_at_most", (("animal", animal), ("n", str(int(match.group(2)) - 1))))]
    match = re.fullmatch(r"(羊|牛|猪)(\d+)", text)
    if match:
        animal = {"羊": "sheep", "牛": "cattle", "猪": "wild_boar"}[match.group(1)]
        return [("animals", (("animal", animal), ("n", match.group(2))))]
    match = re.fullmatch(r"厩(\d+)", text)
    if match:
        return [("stables", (("n", match.group(1)),))]
    match = re.fullmatch(r"柵に囲まれた厩(\d+)", text)
    if match:
        return [("fenced_stable_count", (("n", match.group(1)),))]
    if text in ("木の家", "まだ木の家"):
        return [("house", (("house", "wood"),))]
    match = re.fullmatch(r"木の家ちょうど(\d+)部屋", text)
    if match:
        return [("house", (("house", "wood"),)), ("rooms", (("n", match.group(1)),))]
    if text == "レンガか石の家":
        return [("house", (("house", "clay,stone"),))]
    if text == "かまどと調理場両方":
        return [("improvement", (("kind", "cooking_hearth"),)),
                ("improvement", (("kind", "fireplace"),))]
    if text == ":パン:進歩":
        return [("improvement", (("kind", "bake_bread"),))]
    if text == ":鍋:の進歩":
        return [("improvement", (("kind", "cooking_hearth"),)),
                ("improvement", (("kind", "fireplace"),))]
    if text == "大きい進歩の井戸":
        return [("improvement", (("kind", "well"),))]
    match = re.fullmatch(r"ラウンド(\d+)(?:またはその前に出す|以前に出す)", text)
    if match:
        return [("round_lte", (("round", match.group(1)),))]
    match = re.fullmatch(r"ラウンド(\d+)(?:またはその後に出す|以降に出す)", text)
    if match:
        return [("round_gte", (("round", match.group(1)),))]
    match = re.fullmatch(r"ラウンド([\d、,または\s]+)に出す", text)
    if match:
        values = sorted(set(re.findall(r"\d+", match.group(1))), key=int)
        if len(values) > 1:
            return [("round_in", (("values", ",".join(values)),))]
    if text == "農場スペースが全て使用済み":
        return [("farmyard_all_used", ())]
    match = re.fullmatch(r"未使用の農場スペース(\d+)以上", text)
    if match:
        return [("unused_farmyard_spaces_at_least", (("n", match.group(1)),))]
    match = re.fullmatch(r"何も植えていない畑(\d+)", text)
    if match:
        return [("empty_fields", (("n", match.group(1)),))]
    match = re.fullmatch(r"人物が(\d+)人以下", text)
    if match:
        return [("family_size_lte", (("n", match.group(1)),))]
    if text == "製陶所(またはその改良進歩)":
        return [("has_pottery_or_upgrade", ())]
    match = re.fullmatch(r"ストックに小麦(\d+)", text)
    if match:
        return [("resource_in_hand", (("resource", "grain"), ("min", match.group(1))))]
    if text == "ストックに小麦なし":
        return [("resource_count", (("good", "grain"), ("n", "0")))]
    if text == "家畜がいない":
        return [("no_livestock", ())]
    if text == "家畜1":
        return [("animals", (("animal", "any"), ("n", "1")))]
    match = re.fullmatch(r"畑タイル(\d+)と、全種類の家畜", text)
    if match:
        return [("fields", (("n", match.group(1)),)), ("all_animal_types", ())]
    match = re.fullmatch(r"家族1人が「(.+)」にいる", text) or re.fullmatch(
        r"「(.+)」に人物を置いている", text
    )
    if match:
        space = SPACE_IDS.get(match.group(1), match.group(1))
        return [("worker_on", (("space", space),))]
    match = re.fullmatch(r"手元に(.+?)(\d+)以上", text)
    if match and match.group(1) in LEDGER_GOODS:
        return [("resource_in_hand", (("resource", LEDGER_GOODS[match.group(1)]),
                                     ("min", match.group(2))))]
    match = re.fullmatch(r"柵が(\d+)本建っている", text)
    if match:
        return [("fences_built_at_least", (("n", match.group(1)),))]
    if text == "各種類の家畜それぞれ1匹":
        return [("all_animal_types", ())]
    match = re.fullmatch(r"ちょうど大人(\d+)人", text)
    if match:
        return [("family_exactly", (("n", match.group(1)),))]
    match = re.fullmatch(r"あなたの前のカード\*(\d+)枚", text) or re.fullmatch(
        r"あなたの前にカードが(\d+)枚ある", text
    )
    if match:
        return [("cards_in_play_at_least", (("n", match.group(1)),))]
    return [("prose", (("text", text),))]


def _int(value):
    try:
        return int(value)
    except (TypeError, ValueError):
        return value


def normalize_spec_prerequisite(prerequisites):
    """Map a spec prerequisite list onto the shared canonical predicates."""
    if prerequisites is None:
        return None
    canonical = []
    for entry in prerequisites:
        predicate = entry.get("predicate")
        params = entry.get("params") or {}
        if entry.get("negate"):
            predicate = "not_" + predicate
        if predicate in ("occupation_count", "at_least_occupations"):
            if "n" in params:
                canonical.append(("occupations", (("n", str(_int(params["n"]))),)))
            elif "min" in params:
                canonical.append(("occupations", (("n", str(_int(params["min"]))),)))
            elif "max" in params:
                canonical.append(
                    ("occupations_at_most", (("n", str(_int(params["max"]))),))
                )
            else:
                canonical.append((predicate, tuple(sorted(params.items()))))
        elif predicate == "occupation_count_gte":
            canonical.append(("occupations", (("n", str(_int(params.get("count")))),)))
        elif predicate == "at_most_occupations":
            canonical.append(
                ("occupations_at_most", (("n", str(_int(params.get("n")))),))
            )
        elif predicate in ("field_count", "at_least_fields"):
            canonical.append(("fields", (("n", str(_int(params.get("n")))),)))
        elif predicate == "has_grain_field":
            canonical.append(("grain_fields", (("n", str(_int(params.get("count")))),)))
        elif predicate == "grain_field_count":
            canonical.append(("grain_fields", (("n", str(_int(params.get("n")))),)))
        elif predicate == "pasture_count":
            canonical.append(("pastures", (("n", str(_int(params.get("n")))),)))
        elif predicate == "vegetable_field_count":
            canonical.append(
                ("vegetable_fields", (("n", str(_int(params.get("n")))),))
            )
        elif predicate in ("at_least_animals", "animal_count_at_least"):
            canonical.append(
                (
                    "animals",
                    (("animal", params.get("animal")), ("n", str(_int(params.get("n"))))),
                )
            )
        elif predicate == "animal_count_less":
            canonical.append(
                (
                    "animals_at_most",
                    (
                        ("animal", params.get("animal")),
                        ("n", str(_int(params.get("n")) - 1)),
                    ),
                )
            )
        elif predicate == "animal_count_at_most":
            canonical.append(
                (
                    "animals_at_most",
                    (("animal", params.get("animal")), ("n", str(_int(params.get("n"))))),
                )
            )
        elif predicate == "house_is":
            house = params.get("house")
            if isinstance(house, list):
                house = ",".join(sorted(house))
            canonical.append(("house", (("house", house),)))
        elif predicate == "room_count":
            canonical.append(("rooms", (("n", str(_int(params.get("n")))),)))
        elif predicate == "improvement_count":
            canonical.append(("improvements", (("n", str(_int(params.get("n")))),)))
        elif predicate == "has_improvement":
            kind = params.get("kind") or params.get("type")
            if isinstance(kind, list):
                kind = ",".join(sorted(kind))
            canonical.append(("improvement", (("kind", kind),)))
        elif predicate == "round_lte":
            canonical.append(("round_lte", (("round", str(_int(params.get("round")))),)))
        elif predicate == "round_in":
            values = [_int(value) for value in (params.get("values") or [])]
            if values == list(range(1, max(values) + 1)) and max(values) < 14:
                canonical.append(("round_lte", (("round", str(max(values))),)))
            elif values == list(range(min(values), 15)):
                canonical.append(("round_gte", (("round", str(min(values))),)))
            else:
                canonical.append(
                    ("round_in", (("values", ",".join(str(v) for v in values)),))
                )
        elif predicate == "family_size_lte":
            canonical.append(
                ("family_size_lte", (("n", str(_int(params.get("size")))),))
            )
        elif predicate == "family_exactly":
            canonical.append(
                ("family_exactly", (("n", str(_int(params.get("count")))),))
            )
        elif predicate == "worker_on_space":
            space = params.get("space")
            canonical.append(("worker_on", (("space", SPACE_IDS.get(space, space)),)))
        elif predicate == "resource_in_hand":
            canonical.append(
                (
                    "resource_in_hand",
                    (
                        ("resource", params.get("resource")),
                        ("min", str(_int(params.get("min")))),
                    ),
                )
            )
        elif predicate == "stable_count":
            canonical.append(("stables", (("n", str(_int(params.get("n")))),)))
        elif predicate == "empty_field_count":
            canonical.append(("empty_fields", (("n", str(_int(params.get("n")))),)))
        elif predicate in (
            "fences_built_at_least",
            "fenced_stable_count",
            "cards_in_play_at_least",
            "unused_farmyard_spaces_at_least",
        ):
            canonical.append((predicate, (("n", str(_int(params.get("n")))),)))
        elif predicate in (
            "farmyard_all_used",
            "all_animal_types",
            "no_livestock",
            "has_pottery_or_upgrade",
        ):
            canonical.append((predicate, ()))
        elif predicate in ("custom", "raw_text"):
            canonical.append(
                ("prose", (("text", params.get("text") or params.get("note")),))
            )
        else:
            canonical.append(
                (predicate, tuple(sorted((k, str(v)) for k, v in params.items())))
            )
    return canonical


def _stringified(canonical):
    return sorted(
        (predicate, tuple(sorted((key, str(value)) for key, value in params)))
        for predicate, params in canonical
    )


def canonical_to_text(canonical):
    if canonical is None:
        return None
    return "; ".join(
        predicate + "(" + ",".join(f"{k}={v}" for k, v in params) + ")"
        for predicate, params in canonical
    )


def effect_shape(effect):
    """Compact description of trigger / target / amount for comparison."""
    parts = [effect.get("trigger", "?")]
    for condition in effect.get("conditions") or []:
        parts.append("if:" + condition.get("predicate", "?"))
    for step in effect.get("steps") or []:
        target = step.get("target", "?")
        params = step.get("params") or {}
        operation = step.get("operation", "?")
        detail = ""
        for key in ("good", "amount", "points", "kind", "resource"):
            if key in params and not isinstance(params[key], (dict, list)):
                detail += f" {key}={params[key]}"
        parts.append(f"{operation}->{target}{detail}")
    return " | ".join(parts)


def fixed_gains(effects):
    """Yield (good, amount) for steps that grant a fixed amount of one good."""
    for effect in effects or []:
        for step in effect.get("steps") or []:
            if step.get("operation") != "gain_resource":
                continue
            params = step.get("params") or {}
            good = params.get("good")
            amount = params.get("amount")
            target = step.get("target")
            if isinstance(good, str) and isinstance(amount, int):
                yield good, amount, target, effect.get("trigger")


def test_expected_amounts(test_record):
    """Collect (good, amount) the test contract asserts anywhere in expected."""
    found = set()

    def walk(node, good=None):
        if isinstance(node, dict):
            for key, value in node.items():
                key_good = good
                tokens = key.split("_")
                for alias, canonical in GOOD_ALIASES.items():
                    if alias in tokens:
                        key_good = canonical
                        break
                if isinstance(value, int) and key_good:
                    found.add((key_good, value))
                walk(value, key_good)
        elif isinstance(node, list):
            for item in node:
                walk(item, good)

    for case in test_record.get("cases") or []:
        walk(case.get("expected") or {})
    return found


def play_case_expected(test_record, card_id):
    """Expected dict of the case that plays the card itself, if any."""
    for case in test_record.get("cases") or []:
        action = case.get("action") or {}
        if action.get("play") == card_id or action.get("play_card") == card_id:
            return case.get("expected") or {}
    return None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = parser.parse_args(argv)

    ledger_rows = read_ledger()
    ledger = {row["card_id"]: row for row in ledger_rows}
    specs, spec_path_mismatch = read_records(SPECS)
    tests, test_path_mismatch = read_records(TESTS)

    findings = []

    def add(card_id, category, detail, **extra):
        finding = {"card_id": card_id, "category": category, "detail": detail}
        finding.update(extra)
        findings.append(finding)

    # --- presence / duplicates -------------------------------------------
    ledger_ids = [row["card_id"] for row in ledger_rows]
    for card_id, count in Counter(ledger_ids).items():
        if count > 1:
            add(card_id, "duplicate_ledger_row", f"ledger row repeated {count} times")
    for card_id in sorted(set(ledger) - set(specs)):
        add(card_id, "missing_spec", "ledger row has no spec record")
    for card_id in sorted(set(specs) - set(ledger)):
        add(card_id, "extra_spec", "spec record has no ledger row")
    for card_id in sorted(set(specs) - set(tests)):
        add(card_id, "missing_test", "spec record has no test contract")
    for card_id in sorted(set(tests) - set(specs)):
        add(card_id, "extra_test", "test contract has no spec record")
    for message in spec_path_mismatch + test_path_mismatch:
        add("?", "path_mismatch", message)

    names = defaultdict(list)
    for card_id, row in ledger.items():
        names[(row["name_ja"].strip(), row["name_en"].strip())].append(card_id)
    for (name_ja, name_en), ids in names.items():
        if len(ids) > 1:
            add(
                ",".join(ids),
                "duplicate_name",
                f"{name_ja} / {name_en} appears on {len(ids)} ledger rows",
            )

    # --- identity / cost / points / eligibility --------------------------
    for card_id, spec in sorted(specs.items()):
        row = ledger.get(card_id)
        if row is None:
            continue
        identity = spec.get("identity") or {}
        if identity.get("name_ja") != row["name_ja"]:
            add(
                card_id,
                "identity_name_ja",
                f"ledger {row['name_ja']!r} vs spec {identity.get('name_ja')!r}",
            )
        if (identity.get("name_en") or "") != (row["name_en"] or ""):
            add(
                card_id,
                "identity_name_en",
                f"ledger {row['name_en']!r} vs spec {identity.get('name_en')!r}",
            )
        if identity.get("kind") != row["kind"]:
            add(
                card_id,
                "identity_kind",
                f"ledger {row['kind']!r} vs spec {identity.get('kind')!r}",
            )
        play = spec.get("play") or {}
        if play.get("action") != row["action"]:
            add(
                card_id,
                "identity_action",
                f"ledger {row['action']!r} vs spec {play.get('action')!r}",
            )
        base_cost = ledger_cost(row)
        card_cost = spec_cost(spec)
        if base_cost is not None and card_cost is not None and base_cost != card_cost:
            add(
                card_id,
                "cost",
                f"ledger {row['cost']!r} vs spec {play.get('cost')!r}",
            )
        points = play.get("printed_points")
        if points is not None and str(points) != row["printed_points"]:
            add(
                card_id,
                "printed_points",
                f"ledger {row['printed_points']!r} vs spec {points!r}",
            )
        bga = spec.get("bga") or {}
        for ledger_field, spec_field in (
            ("bga_listed", "listed"),
            ("bga_playable", "playable"),
            ("ban", "ban_status"),
            ("base_2016", "base_2016"),
        ):
            if bga.get(spec_field) != row[ledger_field]:
                add(
                    card_id,
                    "bga_flag_" + spec_field,
                    f"ledger {row[ledger_field]!r} vs spec {bga.get(spec_field)!r}",
                )
        if row["eligibility_basis"] == "consul_dirigens_rule":
            expected = expected_eligibility(card_id)
            if expected is not None:
                actual = bga.get("eligibility") or {}
                if {k: actual.get(k) for k in ("2", "3", "4")} != expected:
                    add(
                        card_id,
                        "eligibility",
                        f"rule {expected} vs spec {actual}",
                    )

    # --- prerequisites ----------------------------------------------------
    for card_id, row in sorted(ledger.items()):
        spec = specs.get(card_id)
        if spec is None:
            continue
        ledger_norm = normalize_ledger_prerequisite(row["prerequisite"])
        spec_norm = normalize_spec_prerequisite(
            (spec.get("play") or {}).get("prerequisites")
        )
        if ledger_norm is None and spec_norm is None:
            continue
        if ledger_norm is None or spec_norm is None:
            if row["prerequisite"].strip() == "unknown" and spec_norm is None:
                continue
            if (ledger_norm or []) != (spec_norm or []):
                add(
                    card_id,
                    "prerequisite_uncomparable",
                    f"ledger {row['prerequisite']!r} vs spec {spec_norm!r}",
                )
            continue
        # The spec may keep a free-text prerequisite.  It is consistent when the
        # ledger wording is the head of that text (the spec usually appends an
        # explanation after ':' or '：').
        ledger_prose = [
            params[0][1] for predicate, params in ledger_norm if predicate == "prose"
        ]
        spec_prose = [
            params[0][1] for predicate, params in spec_norm if predicate == "prose"
        ]
        if ledger_prose and len(ledger_prose) == len(ledger_norm):
            ledger_text = ledger_prose[0].strip()
            if ledger_text == "下記参照" and spec_prose and len(spec_prose) == len(spec_norm):
                continue
            if spec_prose and len(spec_prose) == len(spec_norm):
                if any(
                    text and text.strip().startswith(ledger_text) for text in spec_prose
                ):
                    continue
            add(
                card_id,
                "prerequisite_prose",
                f"ledger {row['prerequisite']!r} vs spec {spec_norm!r}",
            )
            continue
        if spec_prose and len(spec_prose) == len(spec_norm):
            ledger_text = row["prerequisite"].strip()
            if ledger_text == "下記参照":
                continue
            if any(
                text and text.strip().startswith(ledger_text) for text in spec_prose
            ):
                continue
            add(
                card_id,
                "prerequisite_prose",
                f"ledger {row['prerequisite']!r} vs spec prose {spec_prose!r}",
            )
            continue
        if not spec_norm:
            if row["prerequisite"].strip() in ("", "none"):
                continue
            add(
                card_id,
                "prerequisite_missing_in_spec",
                f"ledger {row['prerequisite']!r} but spec prerequisites are empty",
            )
            continue
        # Mixed structured + free-text prerequisite: the structured parts must
        # agree and both sides must carry prose (which cannot be compared).
        ledger_structured = [entry for entry in ledger_norm if entry[0] != "prose"]
        spec_structured = [entry for entry in spec_norm if entry[0] != "prose"]
        if (
            ledger_prose
            and spec_prose
            and _stringified(ledger_structured) == _stringified(spec_structured)
        ):
            continue
        if _stringified(ledger_norm) != _stringified(spec_norm):
            add(
                card_id,
                "prerequisite",
                f"ledger {row['prerequisite']!r} vs spec {spec_norm!r}",
            )

    # --- effect structure vs test contract --------------------------------
    for card_id, spec in sorted(specs.items()):
        test = tests.get(card_id)
        if test is None:
            continue
        cases = test.get("cases") or []
        case_ids = {case.get("id") for case in cases}
        referenced = set((spec.get("review") or {}).get("test_case_ids") or [])
        for missing in sorted(referenced - case_ids):
            add(card_id, "test_reference", f"review references missing case {missing!r}")
        for orphan in sorted(case_ids - referenced):
            add(card_id, "test_orphan_case", f"case {orphan!r} is not referenced by review")
        if not cases:
            add(card_id, "test_empty", "test contract has no cases")
        status = (spec.get("review") or {}).get("status")
        for case in cases:
            expected = case.get("expected") or {}
            if status == "bga_checked" and expected.get("review_status") == "needs_resolution":
                add(
                    card_id,
                    "test_status_conflict",
                    f"spec is bga_checked but {case.get('id')!r} expects needs_resolution",
                )
        effects = spec.get("effects")
        if effects is None:
            continue
        gains = list(fixed_gains(effects))
        asserted_goods = {good for good, _amount in test_expected_amounts(test)}
        reported_goods = set()
        for good, amount, target, trigger in gains:
            if target not in (None, "owner", "board", "card"):
                continue
            if good not in GOOD_ALIASES:
                continue
            canonical_good = GOOD_ALIASES[good]
            if canonical_good in asserted_goods or canonical_good in reported_goods:
                continue
            reported_goods.add(canonical_good)
            add(
                card_id,
                "effect_good_untested",
                f"spec grants {good} ({trigger}) but no test asserts that good at all",
            )
        # Clear contradiction: an unconditional single-step on_play gain whose
        # own play case asserts a different amount or explicitly asserts none.
        on_play = [
            effect
            for effect in effects
            if effect.get("trigger") == "on_play"
            and not (effect.get("conditions") or [])
            and len(effect.get("steps") or []) == 1
            and (effect["steps"][0].get("operation") == "gain_resource")
            and effect["steps"][0].get("target") in (None, "owner")
            and effect["steps"][0]["params"].get("target") not in ("all_other_players", "other_player")
            and isinstance(effect["steps"][0].get("params", {}).get("amount"), int)
            and isinstance(effect["steps"][0].get("params", {}).get("good"), str)
            and effect["steps"][0]["params"].get("good") in GOOD_ALIASES
        ]
        if len(on_play) == 1:
            step = on_play[0]["steps"][0]
            good = GOOD_ALIASES[step["params"]["good"]]
            amount = step["params"]["amount"]
            expected = play_case_expected(test, card_id)
            if expected is not None:
                delta_key = f"{good}_delta"
                if delta_key not in expected:
                    alt = f"owner_{good}_delta"
                    delta_key = alt if alt in expected else delta_key
                value = expected.get(delta_key)
                if isinstance(value, int) and value != amount:
                    add(
                        card_id,
                        "effect_amount_contradicts_test",
                        f"spec grants {amount} {good} on play but play case expects {value}",
                    )
    # --- report -----------------------------------------------------------
    by_category = Counter(finding["category"] for finding in findings)
    by_band = Counter(finding["card_id"][0] for finding in findings if finding["card_id"] != "?")
    summary = {
        "ledger_rows": len(ledger_rows),
        "spec_records": len(specs),
        "test_records": len(tests),
        "cards_with_all_three": len(set(ledger) & set(specs) & set(tests)),
        "ledger_without_spec": len(set(ledger) - set(specs)),
        "finding_count": len(findings),
        "findings_by_category": dict(sorted(by_category.items())),
        "findings_by_band": dict(sorted(by_band.items())),
        "cards_with_findings": len(
            {finding["card_id"] for finding in findings if finding["card_id"] != "?"}
        ),
    }
    if args.json:
        print(json.dumps({"summary": summary, "findings": findings}, ensure_ascii=False, indent=2))
    else:
        print(json.dumps(summary, ensure_ascii=False, indent=2))
        for finding in findings:
            print(f"{finding['card_id']}\t{finding['category']}\t{finding['detail']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
