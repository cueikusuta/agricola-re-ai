#!/usr/bin/env python3
"""Collect the third-party BGA-oriented Agricola card wiki into a base ledger.

Issue #202.  The wiki at https://agricola.cloudfree.jp/ is a *third-party,
BGA-oriented reference*.  It is not an official Lookout or BGA source and it is
never treated as official verification (that stays in
``docs/card-catalog-ae-audit.csv``).

This script only reads the wiki's *structured* fields: card id, deck, kind,
Japanese/English name, play cost, prerequisite phrase, printed points, rating,
role/goods tags, ban status, base-set flag and BGA implementation status.  The
card description text is used only to locate those fields and is **not** copied
into the repository: the raw HTML stays in ``--cache`` (git-ignored) and the
emitted CSV contains short condition fragments and independently-worded notes.

Usage::

    python3 scripts/collect_bga_card_data.py \
        --cache .work/bga-card-cache \
        --out data/bga-card-base-ledger.csv

The collector is reproducible: every page is cached by URL hash and re-used
unless ``--refresh`` is passed.  It refuses URLs disallowed by robots.txt.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "docs" / "card-catalog-ae-preliminary.csv"
RUST_CARDS = ROOT / "src" / "cards.rs"
LIST_JA = "https://agricola.cloudfree.jp/cardlist/"
LIST_EN = "https://agricola.cloudfree.jp/cardlist_en/"
UNIMPLEMENTED = "https://agricola.cloudfree.jp/unimplemented/"
ROBOTS = "https://agricola.cloudfree.jp/robots.txt"
USER_AGENT = "agricola-re-ai-card-data/1.0 (reproducible reference collector)"

GOODS = {
    "飯": "food",
    "食料": "food",
    "木": "wood",
    "木材": "wood",
    "レンガ": "clay",
    "粘土": "clay",
    "石": "stone",
    "葦": "reed",
    "葦原": "reed",
    "小麦": "grain",
    "穀物": "grain",
    "野菜": "vegetable",
}
KIND_TO_SCHEMA = {
    "職業": "occupation",
    "小進歩": "minor_improvement",
    "大進歩": "major_improvement",
}
KIND_TO_ACTION = {
    "職業": "occupation",
    "小進歩": "minor_improvement",
    "大進歩": "major_improvement",
}
NONE_MARKERS = {"ー", "-", "‐", "なし", "", "0"}
BAN = {"": "none", "弱": "weak", "強": "strong"}


def cells(row: str) -> list[str]:
    """Return the text of every ``<td>``/``<th>`` cell in one table row."""
    out = []
    for match in re.finditer(r"<t[dh][^>]*>(.*?)</t[dh]>", row, re.S):
        text = re.sub(r"<[^>]+>", "", match.group(1))
        for entity, char in (
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", '"'),
            ("&#039;", "'"),
            ("&nbsp;", " "),
        ):
            text = text.replace(entity, char)
        out.append(re.sub(r"\s+", " ", text).strip())
    return out


def table_rows(html: str) -> list[list[str]]:
    """All non-empty table rows on a page."""
    rows = [cells(row) for row in re.findall(r"<tr[^>]*>.*?</tr>", html, re.S)]
    return [row for row in rows if row]


def cache_file(cache: Path, url: str) -> Path:
    return cache / (hashlib.sha256(url.encode("utf-8")).hexdigest()[:20] + ".html")


def load_robots(cache: Path, refresh: bool) -> list[str]:
    try:
        text = fetch(ROBOTS, cache, refresh, 0.2, [])
    except SystemExit:
        return []
    patterns: list[str] = []
    active = False
    for line in text.splitlines():
        line = line.strip()
        lowered = line.lower()
        if lowered.startswith("user-agent:"):
            active = line.split(":", 1)[1].strip() == "*"
        elif active and lowered.startswith("disallow:"):
            pattern = line.split(":", 1)[1].strip()
            if pattern:
                patterns.append(pattern)
    return patterns


def disallowed(url: str, robots: list[str]) -> bool:
    path = re.sub(r"^https?://[^/]+", "", url)
    for pattern in robots:
        if pattern.endswith("$"):
            if path.endswith(pattern[:-1]):
                return True
        elif pattern and path.startswith(pattern):
            return True
    return False


def fetch(url: str, cache: Path, refresh: bool, delay: float, robots: list[str]) -> str:
    """Fetch (or read from cache) one URL, honouring robots.txt."""
    if disallowed(url, robots):
        raise SystemExit(f"robots.txt disallows {url}")
    path = cache_file(cache, url)
    if path.exists() and not refresh:
        return path.read_text(encoding="utf-8", errors="replace")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            body = response.read()
    except urllib.error.URLError as exc:  # pragma: no cover - network dependent
        raise SystemExit(f"failed to fetch {url}: {exc}") from exc
    cache.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)
    time.sleep(delay)
    return body.decode("utf-8", errors="replace")


def parse_cost(raw: str) -> tuple[str, str]:
    """Turn a wiki cost cell into ``(cost, alternative)``.

    ``cost`` is ``none``, ``unknown`` or ``good:amount;good:amount``.  A cell
    that names cards instead of resources becomes an ``alternative`` phrase and
    leaves ``cost`` as ``unknown`` so a reader cannot mistake a card-return
    cost for "free".
    """
    text = raw.strip()
    if text in NONE_MARKERS:
        return "none", "none"
    parts = [part.strip() for part in re.split(r"[,、]", text) if part.strip()]
    parsed = []
    for part in parts:
        match = re.fullmatch(r"(\d+)\s*(.+)", part)
        if not match:
            return "unknown", text
        amount, good = match.group(1), match.group(2).strip()
        if good not in GOODS:
            return "unknown", text
        parsed.append(f"{GOODS[good]}:{amount}")
    return ";".join(parsed), "none"


def parse_points(raw: str) -> str:
    text = raw.strip()
    if text in NONE_MARKERS:
        return "0"
    if re.fullmatch(r"-?\d+", text):
        return text
    return "unknown"


def split_kind(raw: str) -> tuple[str, str]:
    """Return ``(schema_kind, action)`` for the wiki's 区分 cell."""
    parts = [part.strip() for part in re.split(r"[|、,]", raw) if part.strip()]
    if len(parts) == 1 and parts[0] in KIND_TO_SCHEMA:
        return KIND_TO_SCHEMA[parts[0]], KIND_TO_ACTION[parts[0]]
    if parts and all(part in KIND_TO_SCHEMA for part in parts):
        return "multi_type", "special"
    return "multi_type", "special"


def parse_list(html: str, language: str) -> list[dict[str, str]]:
    """Parse a list page.  Only the English page carries the card-id column."""
    cards = []
    for row in table_rows(html):
        if language == "en":
            if len(row) < 14 or not re.fullmatch(r"[A-E]\d{3}", row[2]):
                continue
            cards.append(
                {
                    "deck": row[0],
                    "kind_raw": row[1],
                    "card_id": row[2],
                    "name_ja": row[3],
                    "name_en": row[4],
                    "rating": row[6],
                    "upgrade": row[7],
                    "goods": row[8],
                    "role": row[9],
                    "ban": row[10],
                    "implemented": row[11],
                    "unimplemented": row[12],
                    "base_set": row[13],
                }
            )
        else:
            if len(row) != 13 or row[0] not in list("ABCDE") or row[2] == "名称":
                continue
            cards.append(
                {
                    "deck": row[0],
                    "kind_raw": row[1],
                    "card_id": None,
                    "name_ja": row[2],
                    "name_en": row[3],
                    "rating": row[5],
                    "upgrade": row[6],
                    "goods": row[7],
                    "role": row[8],
                    "ban": row[9],
                    "implemented": row[10],
                    "unimplemented": row[11],
                    "base_set": row[12],
                }
            )
    return cards


def parse_links(html: str) -> dict[str, str]:
    """Map card id -> detail page URL from the English list page."""
    links: dict[str, str] = {}
    for row in re.findall(r"<tr[^>]*>.*?</tr>", html, re.S):
        id_match = re.search(r">([A-E]\d{3})<", row)
        url_match = re.search(r"href='(https://agricola\.cloudfree\.jp/\?p=\d+)'", row)
        if id_match and url_match:
            links[id_match.group(1)] = url_match.group(1)
    return links


LABELS = {"No.", "区分", "条件", "コスト", "強化", "品物", "役割", "勝利点", "評価", "効果"}


def parse_card_page(html: str) -> dict[str, str]:
    """Parse one card detail page into its labelled fields.

    A detail page lays the fields out as a label row followed by a value row
    (``No. | 区分 | 条件 | コスト`` then ``A005 | 小進歩 | ー | 1 飯``), so the
    label and its value share a column index rather than being adjacent.
    """
    fields: dict[str, str] = {}
    rows = table_rows(html)
    consumed: set[int] = set()
    for index, row in enumerate(rows):
        if index in consumed:
            continue
        labels = [cell for cell in row if cell in LABELS]
        if not labels or index + 1 >= len(rows):
            continue
        values = rows[index + 1]
        consumed.add(index + 1)
        if len(labels) >= 2:
            for column, label in enumerate(row):
                if label in LABELS and column < len(values):
                    fields[label] = values[column]
        else:
            fields[labels[0]] = values[0]
    return fields


def parse_unimplemented(html: str) -> list[dict[str, str]]:
    cards = []
    for row in table_rows(html):
        if len(row) >= 10 and re.fullmatch(r"[A-E]\d{3}", row[2]):
            cards.append(
                {
                    "deck": row[0],
                    "kind_raw": row[1],
                    "card_id": row[2],
                    "name_ja": row[3],
                    "upgrade": row[5] if len(row) > 5 else "",
                    "goods": row[6] if len(row) > 6 else "",
                    "role": row[7] if len(row) > 7 else "",
                    "ban": row[8] if len(row) > 8 else "",
                    "unimplemented": row[9],
                }
            )
    return cards


def build(args: argparse.Namespace) -> tuple[list[dict[str, str]], dict[str, str]]:
    robots = load_robots(args.cache, args.refresh)
    ja_rows = parse_list(fetch(LIST_JA, args.cache, args.refresh, args.delay, robots), "ja")
    en_html = fetch(LIST_EN, args.cache, args.refresh, args.delay, robots)
    en_rows = parse_list(en_html, "en")
    if len(ja_rows) != len(en_rows):
        raise SystemExit(f"list pages differ: ja={len(ja_rows)} en={len(en_rows)}")
    links = parse_links(en_html)
    unimplemented = parse_unimplemented(fetch(UNIMPLEMENTED, args.cache, args.refresh, args.delay, robots))
    unimpl_by_id = {card["card_id"]: card for card in unimplemented}

    ledger_rows = list(csv.DictReader(LEDGER.open(encoding="utf-8-sig", newline="")))
    ledger = {row["card_id"]: row for row in ledger_rows}
    if len(ledger) != 840:
        raise SystemExit("existing ledger is not 840 unique ids")
    # Cross-check: which ids already have a Rust card definition?  This is the
    # engine's implementation status, not official verification.
    implemented = set(re.findall(r'"([A-E]\d{3})"', RUST_CARDS.read_text(encoding="utf-8")))

    by_id: dict[str, dict[str, str]] = {}
    for ja_row, en_row in zip(ja_rows, en_rows):
        if ja_row["name_ja"] != en_row["name_ja"] or ja_row["name_en"] != en_row["name_en"]:
            raise SystemExit(f"list rows misaligned for {en_row['card_id']}")
        by_id[en_row["card_id"]] = {"ja": ja_row, "en": en_row}

    out_rows: list[dict[str, str]] = []
    detail_pages = 0
    for card_id in sorted(ledger):
        ledger_row = ledger[card_id]
        row: dict[str, str] = {
            "card_id": card_id,
            "deck": ledger_row["deck"],
            "kind": "",
            "name_ja": ledger_row["name_ja"],
            "name_en": ledger_row["name_en"],
            "action": "unknown",
            "cost": "unknown",
            "cost_alternative": "unknown",
            "prerequisite": "unknown",
            "printed_points": "unknown",
            "rating": "unknown",
            "goods": "",
            "role": "",
            "upgrade": "",
            "ban": "unknown",
            "base_2016": "unknown",
            "bga_listed": "no",
            "bga_playable": "no",
            "bga_unimplemented": "yes",
            "bga_implemented_at": "",
            "eligible_2": "unknown",
            "eligible_3": "unknown",
            "eligible_4": "unknown",
            "source_url": UNIMPLEMENTED,
            "source_language": "ja",
            "checked_on": args.checked_on,
            "review_status": "inventory_only",
            "implementation_status": "implemented" if card_id in implemented else "not_started",
            "engine_features": "",
            "notes": "",
        }
        if card_id in by_id:
            en_row = by_id[card_id]["en"]
            ja_row = by_id[card_id]["ja"]
            kind, action = split_kind(en_row["kind_raw"])
            row.update(
                {
                    "kind": kind,
                    "action": action,
                    "rating": en_row["rating"],
                    "goods": en_row["goods"],
                    "role": en_row["role"],
                    "upgrade": en_row["upgrade"],
                    "ban": BAN.get(en_row["ban"], "unknown"),
                    "base_2016": "yes" if en_row["base_set"] == "ON" else "no",
                    "bga_listed": "yes",
                    "bga_playable": "yes" if en_row["unimplemented"] != "ON" else "no",
                    "bga_unimplemented": "yes" if en_row["unimplemented"] == "ON" else "no",
                    "bga_implemented_at": en_row["implemented"],
                    "source_url": links.get(card_id, ""),
                }
            )
            # Guard against a ledger/wiki name disagreement: keep the ledger name
            # and record the wiki spelling instead of silently replacing it.
            if ja_row["name_ja"] != ledger_row["name_ja"]:
                row["notes"] = "wiki_ja_name_differs"
            if en_row["name_en"] != ledger_row["name_en"]:
                row["notes"] = "wiki_en_name_differs"
            if args.limit is not None and detail_pages >= args.limit:
                out_rows.append(row)
                continue
            detail = parse_card_page(
                fetch(row["source_url"], args.cache, args.refresh, args.delay, robots)
            )
            detail_pages += 1
            cost, alternative = parse_cost(detail.get("コスト", "unknown"))
            row.update(
                {
                    "cost": cost,
                    "cost_alternative": alternative,
                    "prerequisite": "none"
                    if detail.get("条件", "") in NONE_MARKERS
                    else detail.get("条件", "unknown"),
                    "printed_points": parse_points(detail.get("勝利点", "unknown")),
                }
            )
            if detail.get("No.") != card_id:
                row["notes"] = (row["notes"] + ";" if row["notes"] else "") + "detail_id_mismatch"
        else:
            unimpl = unimpl_by_id.get(card_id)
            if unimpl:
                kind, action = split_kind(unimpl["kind_raw"])
                row.update(
                    {
                        "kind": kind,
                        "action": action,
                        "goods": unimpl["goods"],
                        "role": unimpl["role"],
                        "upgrade": unimpl["upgrade"],
                        "ban": BAN.get(unimpl["ban"], "unknown"),
                        "bga_listed": "no",
                        "bga_playable": "no",
                        "bga_unimplemented": "yes",
                        "source_url": UNIMPLEMENTED,
                        "notes": "unimplemented_page_only",
                    }
                )
            else:
                row["notes"] = "missing_from_wiki"
        out_rows.append(row)
    meta = {"ja_rows": len(ja_rows), "en_rows": len(en_rows), "detail_pages": detail_pages}
    return out_rows, meta


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / ".work" / "bga-card-cache")
    parser.add_argument("--out", type=Path, default=ROOT / "data" / "bga-card-base-ledger.csv")
    parser.add_argument("--delay", type=float, default=0.4)
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--checked-on", default="2026-11-09")
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args()

    rows, meta = build(args)
    columns = list(rows[0].keys())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)
    summary = {
        "rows": len(rows),
        "bga_listed": sum(1 for row in rows if row["bga_listed"] == "yes"),
        "bga_unimplemented": sum(1 for row in rows if row["bga_unimplemented"] == "yes"),
        "cost_known": sum(1 for row in rows if row["cost"] not in ("unknown",)),
        "points_known": sum(1 for row in rows if row["printed_points"] != "unknown"),
        "source": "https://agricola.cloudfree.jp/",
        "out": str(args.out),
        **meta,
    }
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
