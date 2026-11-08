#!/usr/bin/env python3
"""Expand the curated BGA effect table into per-card spec and test JSON files.

Issue #202.  The BGA wiki text for each card was read by a human (AI) reviewer and
reduced to a compact, machine-checkable effect table below.  This script only
expands that table plus the already-collected base ledger metadata into the files
required by ``schemas/bga-card-spec.schema.json`` and
``schemas/bga-card-test.schema.json``.  It never guesses: a card whose effect the
reviewer could not reduce to the documented vocabulary stays out of the table and
therefore keeps ``inventory_only`` in the base ledger.

Usage::

    python scripts/build_bga_card_specs.py [--cards A002,B004] [--force]

Without ``--force`` an existing ``data/bga-card-specs/<ID>.json`` is left alone so
hand-written snapshots are never clobbered.
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
TESTS = ROOT / "data" / "bga-card-tests"
JA_LIST = "https://agricola.cloudfree.jp/cardlist/"
EN_LIST = "https://agricola.cloudfree.jp/cardlist_en/"
CHECKED_ON = "2026-11-09"


# --- step helpers ---------------------------------------------------------
def gain(good, amount, **extra):
    params = {"good": good, "amount": amount}
    params.update(extra)
    return {"operation": "gain_resource", "target": "owner", "params": params}


def spend(good, amount, **extra):
    params = {"good": good, "amount": amount}
    params.update(extra)
    return {"operation": "spend_resource", "target": "owner", "params": params}


def points(amount, **extra):
    params = {"points": amount}
    params.update(extra)
    return {"operation": "gain_points", "target": "owner", "params": params}


def plow(amount=1, **extra):
    params = {"amount": amount}
    params.update(extra)
    return {"operation": "plow_field", "target": "board", "params": params}


def stable(amount=1, **extra):
    params = {"amount": amount}
    params.update(extra)
    return {"operation": "build_stable", "target": "board", "params": params}


def renovate(**extra):
    params = dict(extra)
    return {"operation": "renovate", "target": "board", "params": params}


def action(name, **extra):
    params = {"action": name}
    params.update(extra)
    return {"operation": "grant_action", "target": "owner", "params": params}


def capacity(**extra):
    params = dict(extra)
    return {"operation": "modify_capacity", "target": "board", "params": params}


def exchange(**extra):
    params = dict(extra)
    return {"operation": "exchange", "target": "owner", "params": params}


def choice(**extra):
    params = {"kind": "choice"}
    params.update(extra)
    return {"operation": "other", "target": "owner", "params": params}


def special(kind, **extra):
    params = {"kind": kind}
    params.update(extra)
    return {"operation": "other", "target": "owner", "params": params}


# --- quantity helpers ----------------------------------------------------
def count(source):
    return {"op": "count", "args": [source]}


def per(source, amount):
    return {"op": "per", "args": [source, amount]}


def res(good):
    return {"op": "resource_count", "args": [good]}


def table(key, mapping):
    return {"op": "table", "args": [key, mapping]}


def remaining_rounds():
    return {"op": "count", "args": ["remaining_rounds"]}


# --- effect helper -------------------------------------------------------
def effect(
    trigger,
    steps,
    cond=None,
    optional=False,
    freq="once_per_play",
    dur="instant",
    snap="after_event",
    order=0,
    decision="none",
    notes=None,
):
    item = {
        "trigger": trigger,
        "conditions": cond or [],
        "steps": steps,
        "optional": optional,
        "frequency": freq,
        "duration": dur,
        "snapshot": snap,
        "order": order,
        "decision_owner": decision,
    }
    if notes:
        item["notes"] = notes
    return item


def cond(predicate, **params):
    return {"predicate": predicate, "params": params}


def case(cid, setup, act, expected, scope="effect_only", notes=None):
    item = {"id": cid, "scope": scope, "setup": setup, "action": act, "expected": expected}
    if notes:
        item["notes"] = notes
    return item


# --- prerequisite structures --------------------------------------------
PREREQ = {
    "A008": [cond("occupation_count", n=2), cond("improvement_count", n=2)],
    "A013": [cond("house_is", house="wood"), cond("room_count", n=2)],
    "A021": [cond("occupation_count", n=1)],
    "A024": [cond("occupation_count", n=2)],
    "A026": [cond("grain_field_count", n=2)],
    "A029": [cond("occupation_count", n=2)],
    "A030": [cond("grain_field_count", n=0)],
    "A033": [cond("farmyard_all_used")],
    "A054": [cond("occupation_count", max=3)],
    "A068": [cond("empty_field_count", n=1)],
    "B006": [cond("occupation_count", n=1)],
    "B033": [cond("house_is", house=["clay", "stone"])],
    "B037": [cond("field_count", n=6), cond("all_animal_types")],
    "B073": [cond("occupation_count", n=3)],
    "C007": [cond("pasture_count", n=1)],
    "C017": [cond("field_count", n=3)],
    "C036": [cond("occupation_count", n=1)],
    "C050": [cond("stable_count", n=3), cond("pasture_count", n=3)],
    "C058": [cond("occupation_count", n=1)],
}

# --- curated card table --------------------------------------------------
CARDS = {
    "A002": {
        "post": "left_neighbor_hand",
        "effects": [effect("on_play", [plow(1)])],
        "notes": "Plow 1 field, then pass the card to the left neighbour's hand.",
        "tests": [
            case("A002-normal", {"fields": 2}, {"play": "A002"},
                 {"fields": 3, "post_play": "left_neighbor_hand"}),
            case("A002-boundary", {"fields": 5}, {"play": "A002"},
                 {"fields": 6, "post_play": "left_neighbor_hand"},
                 "effect_only", "field count is not capped by the card itself"),
        ],
    },
    "A007": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play", [gain("food", per(count("grain_field"), 1)),
                               gain("grain", per(count("vegetable_field"), 1))])
        ],
        "notes": "Gain 1 food per grain field and 1 grain per vegetable field; pass left.",
        "tests": [
            case("A007-normal", {"grain_fields": 2, "vegetable_fields": 1}, {"play": "A007"},
                 {"food_delta": 2, "grain_delta": 1}),
            case("A007-zero", {"grain_fields": 0, "vegetable_fields": 0}, {"play": "A007"},
                 {"food_delta": 0, "grain_delta": 0}),
        ],
    },
    "A008": {
        "post": "left_neighbor_hand",
        "effects": [effect("on_play", [gain("grain", 1), gain("vegetable", 1)])],
        "notes": "Gain 1 grain and 1 vegetable; pass left.",
        "tests": [
            case("A008-normal", {"grain": 0, "vegetable": 0}, {"play": "A008"},
                 {"grain_delta": 1, "vegetable_delta": 1}),
            case("A008-not-triggered", {"occupations_played": 1, "improvements_played": 2},
                 {"play": "A008"}, {"play_rejected": True}),
        ],
    },
    "A011": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("wild_boar", 1)]),
            effect("passive", [capacity(scope="unplanted_field", animal="wild_boar", animal_capacity=1)],
                   freq="always", dur="permanent", snap="live_state", order=1),
        ],
        "notes": "Gain 1 wild boar on play; each unplanted field tile holds 1 wild boar.",
        "tests": [
            case("A011-normal", {"wild_boar": 0}, {"play": "A011"},
                 {"wild_boar_delta": 1}),
            case("A011-capacity", {"unplanted_fields": 2, "wild_boar": 1},
                 {"play": "A011"}, {"boar_capacity": 2}),
        ],
    },
    "A016": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("clay", 1)]),
            effect("passive", [special("substitute_cost", material_from="wood", material_to="clay",
                                       scope="fence_building")],
                   freq="always", dur="permanent", snap="live_state", order=1),
        ],
        "notes": "Gain 1 clay on play; clay may replace wood when building fences.",
        "tests": [
            case("A016-normal", {"clay": 0}, {"play": "A016"}, {"clay_delta": 1}),
            case("A016-substitute", {"clay": 4, "wood": 0, "fence_segments": 4},
                 {"build_fences": 4}, {"wood_spent": 0, "clay_spent": 4}),
        ],
    },
    "A021": {
        "post": "played_area",
        "effects": [
            effect("after_worker_space",
                   [action("family_growth"), gain("food", 1)],
                   cond=[cond("worker_space_used", space="build_rooms"),
                         cond("rooms_exceed_people")],
                   freq="every_use", dur="permanent"),
        ],
        "notes": "After Build Rooms while rooms exceed people: also Family Growth and 1 food.",
        "tests": [
            case("A021-normal", {"rooms": 3, "people": 2}, {"build_rooms": 1},
                 {"family_growth_granted": True, "food_delta": 1}),
            case("A021-not-triggered", {"rooms": 2, "people": 2}, {"build_rooms": 1},
                 {"family_growth_granted": False, "food_delta": 0}),
        ],
    },
    "A024": {
        "post": "played_area",
        "effects": [
            effect("after_worker_space", [action("bake_bread")],
                   cond=[cond("worker_space_used", spaces=["farmland", "cultivation"])],
                   freq="every_use", dur="permanent"),
        ],
        "notes": "After Farmland or Cultivation: an additional Bake Bread action.",
        "tests": [
            case("A024-normal", {"bread_oven": True}, {"use_space": "cultivation"},
                 {"bake_bread_granted": True}),
            case("A024-not-triggered", {"bread_oven": True}, {"use_space": "forest"},
                 {"bake_bread_granted": False}),
        ],
    },
    "A026": {
        "post": "played_area",
        "effects": [
            effect("passive", [special("ignore_occupation", space="wish_for_children",
                                       occupied_by_others=True)],
                   freq="always", dur="permanent", snap="live_state"),
        ],
        "notes": "Wish for Children spaces stay usable when one other player occupies them.",
        "tests": [
            case("A026-normal", {"space_occupied_by_other": True}, {"use_space": "wish_for_children"},
                 {"action_allowed": True}),
            case("A026-not-triggered", {"space_occupied_by_other": True}, {"use_space": "farmland"},
                 {"action_allowed": False}),
        ],
    },
    "A029": {
        "post": "played_area",
        "effects": [
            effect("round_end",
                   [spend("grain", 1), points(1), gain("food", 1, target="all_other_players")],
                   optional=True, freq="once_per_round", dur="permanent",
                   decision="owner"),
        ],
        "notes": "Returning-home phase: pay exactly 1 grain for 1 point; each other player gets 1 food.",
        "tests": [
            case("A029-normal", {"grain": 2}, {"decline_round_bonus": False},
                 {"grain_delta": -1, "points_delta": 1, "other_players_food_delta": 1}),
            case("A029-not-triggered", {"grain": 0}, {"decline_round_bonus": False},
                 {"grain_delta": 0, "points_delta": 0, "other_players_food_delta": 0}),
            case("A029-declined", {"grain": 2}, {"decline_round_bonus": True},
                 {"grain_delta": 0, "points_delta": 0}),
        ],
    },
    "A030": {
        "post": "played_area",
        "effects": [
            effect("after_bake_bread", [spend("grain", 1), gain("food", 2), points(1)],
                   optional=True, freq="every_use", dur="permanent", decision="owner"),
        ],
        "notes": "Each Bake Bread: may exchange exactly 1 grain for 2 food and 1 point.",
        "tests": [
            case("A030-normal", {"grain": 1}, {"use_card": True},
                 {"grain_delta": -1, "food_delta": 2, "points_delta": 1}),
            case("A030-not-triggered", {"grain": 0}, {"use_card": True},
                 {"grain_delta": 0, "food_delta": 0, "points_delta": 0}),
        ],
    },
    "A033": {
        "post": "played_area",
        "effects": [
            effect("on_play",
                   [points(per(remaining_rounds(), 1)),
                    gain("food", per(remaining_rounds(), 2))]),
        ],
        "notes": "Gain 1 point and 2 food for each complete round still to play.",
        "tests": [
            case("A033-normal", {"remaining_rounds": 5}, {"play": "A033"},
                 {"points_delta": 5, "food_delta": 10}),
            case("A033-boundary", {"remaining_rounds": 0}, {"play": "A033"},
                 {"points_delta": 0, "food_delta": 0}),
        ],
    },
    "A041": {
        "post": "played_area",
        "effects": [
            effect("after_upgrade_fireplace",
                   [gain("wood", 2), gain("vegetable", 1)],
                   freq="every_use", dur="permanent",
                   notes="not retroactive"),
        ],
        "notes": "Each Fireplace-to-Cooking-Hearth upgrade: gain 2 wood and 1 vegetable.",
        "tests": [
            case("A041-normal", {"fireplaces": 1}, {"upgrade_fireplace": 1},
                 {"wood_delta": 2, "vegetable_delta": 1}),
            case("A041-not-triggered", {"cooking_hearths": 0}, {"bake_bread": 1},
                 {"wood_delta": 0, "vegetable_delta": 0}),
        ],
    },
    "A054": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 5)]),
            effect("round_end",
                   [spend("food", 1, otherwise="begging_marker")],
                   cond=[cond("round_has_no_harvest")],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 5 food on play; at the end of each non-harvest round pay 1 food or take a begging marker.",
        "tests": [
            case("A054-normal", {"food": 0}, {"play": "A054"}, {"food_delta": 5}),
            case("A054-pay", {"food": 3}, {"end_round": "no_harvest"},
                 {"food_delta": -1, "begging_markers": 0}),
            case("A054-cannot-pay", {"food": 0}, {"end_round": "no_harvest"},
                 {"food_delta": 0, "begging_markers": 1}),
        ],
    },
    "A055": {
        "post": "played_area",
        "effects": [
            effect("after_improvement", [gain("food", 1)],
                   freq="every_use", dur="permanent",
                   notes="includes playing this card"),
        ],
        "notes": "Each improvement built, including this card, gives 1 food.",
        "tests": [
            case("A055-normal", {"food": 0}, {"play": "A055"}, {"food_delta": 1}),
            case("A055-other-improvement", {"food": 0}, {"play_improvement": "B037"},
                 {"food_delta": 1}),
            case("A055-not-triggered", {"food": 0}, {"play_occupation": "A086"},
                 {"food_delta": 0}),
        ],
    },
    "A068": {
        "post": "played_area",
        "effects": [
            effect("after_fence_build", [gain("vegetable", 1)],
                   cond=[cond("fence_count_at_least_round")],
                   freq="every_use", dur="permanent"),
        ],
        "notes": "Each fence build with fences >= current round: gain 1 vegetable.",
        "tests": [
            case("A068-normal", {"round": 4, "fences_built": 4}, {"build_fences": 4},
                 {"vegetable_delta": 1}),
            case("A068-boundary", {"round": 5, "fences_built": 5}, {"build_fences": 5},
                 {"vegetable_delta": 1}),
            case("A068-not-triggered", {"round": 5, "fences_built": 4}, {"build_fences": 4},
                 {"vegetable_delta": 0}),
        ],
    },
    "A077": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("clay", 1)]),
            effect("after_worker_space", [gain("clay", 2)],
                   cond=[cond("worker_space_used", space="pig_market"),
                         cond("any_player")],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 1 clay on play; each Pig Market use by any player gives 2 clay.",
        "tests": [
            case("A077-normal", {"clay": 0}, {"play": "A077"}, {"clay_delta": 1}),
            case("A077-other-player", {"clay": 0}, {"other_player_uses": "pig_market"},
                 {"clay_delta": 2}),
            case("A077-not-triggered", {"clay": 0}, {"other_player_uses": "forest"},
                 {"clay_delta": 0}),
        ],
    },
    "A086": {
        "post": "played_area",
        "effects": [
            effect("on_play",
                   [choice(options=[{"good": "wood", "amount": 1},
                                    {"good": "grain", "amount": 1}])],
                   optional=True, decision="owner"),
            effect("passive", [capacity(scope="room", animal_capacity=1, any_type=True)],
                   freq="always", dur="permanent", snap="live_state", order=1),
        ],
        "notes": "Choice of 1 wood or 1 grain; each room holds 1 animal of any type.",
        "tests": [
            case("A086-normal", {"wood": 0, "grain": 0}, {"choose": "wood"},
                 {"wood_delta": 1, "grain_delta": 0}),
            case("A086-alt", {"wood": 0, "grain": 0}, {"choose": "grain"},
                 {"wood_delta": 0, "grain_delta": 1}),
            case("A086-capacity", {"rooms": 3}, {"play": "A086"}, {"animal_capacity": 3}),
        ],
    },
    "A112": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("grain", 1)]),
            effect("harvest_field_phase", [gain("grain", per(count("grain_field"), 1))],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 1 grain on play; in each field phase harvest 1 extra grain per grain field.",
        "tests": [
            case("A112-normal", {"grain": 0}, {"play": "A112"}, {"grain_delta": 1}),
            case("A112-harvest", {"grain_fields": 2}, {"harvest_field_phase": 1},
                 {"grain_delta": 2}),
            case("A112-zero", {"grain_fields": 0}, {"harvest_field_phase": 1},
                 {"grain_delta": 0}),
        ],
    },
    "A117": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("wood", per(count("improvement"), 1))]),
        ],
        "notes": "Gain 1 wood for each improvement already in front of the owner.",
        "tests": [
            case("A117-normal", {"improvements_played": 3}, {"play": "A117"},
                 {"wood_delta": 3}),
            case("A117-zero", {"improvements_played": 0}, {"play": "A117"},
                 {"wood_delta": 0}),
        ],
    },
    "A134": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("wood", 1), gain("clay", 1)]),
            effect("end_scoring", [points(per(count("pasture_at_max_capacity"), 1))],
                   freq="end_of_game", dur="permanent", snap="live_state", order=1),
        ],
        "notes": "Gain 1 wood and 1 clay on play; 1 point per pasture filled to capacity at scoring.",
        "tests": [
            case("A134-normal", {"wood": 0, "clay": 0}, {"play": "A134"},
                 {"wood_delta": 1, "clay_delta": 1}),
            case("A134-scoring", {"pastures_at_capacity": 2}, {"end_scoring": True},
                 {"points_delta": 2}),
            case("A134-scoring-zero", {"pastures_at_capacity": 0}, {"end_scoring": True},
                 {"points_delta": 0}),
        ],
    },
    "A153": {
        "post": "played_area",
        "effects": [
            effect("after_animal_acquisition", [points(3)],
                   cond=[cond("animal_count_at_least", animal="wild_boar", n=5)],
                   freq="once_per_game", dur="permanent"),
        ],
        "notes": "First time after playing that the owner has 5 wild boars: 3 points, once.",
        "tests": [
            case("A153-normal", {"wild_boar": 4}, {"acquire_wild_boar": 1},
                 {"points_delta": 3}),
            case("A153-boundary", {"wild_boar": 5}, {"acquire_wild_boar": 0},
                 {"points_delta": 3}),
            case("A153-not-triggered", {"wild_boar": 3}, {"acquire_wild_boar": 1},
                 {"points_delta": 0}),
            case("A153-once", {"wild_boar": 5, "already_scored": True},
                 {"acquire_wild_boar": 1}, {"points_delta": 0}),
        ],
    },
    "A160": {
        "post": "played_area",
        "effects": [
            effect("after_worker_space", [gain("food", 1), gain("wood", 1)],
                   cond=[cond("worker_space_used", space="traveling_players"),
                         cond("other_player_only")],
                   freq="every_use", dur="permanent"),
            effect("after_worker_space",
                   [exchange(give=[{"good": "food", "amount": 2}],
                             receive=[{"good": "vegetable", "amount": 1}])],
                   cond=[cond("worker_space_used", space="traveling_players"),
                         cond("other_player_only")],
                   optional=True, freq="every_use", dur="permanent", order=1,
                   decision="owner"),
        ],
        "notes": "When another player uses Traveling Players: 1 food and 1 wood; may then buy 1 vegetable for 2 food.",
        "tests": [
            case("A160-normal", {"food": 0, "wood": 0},
                 {"other_player_uses": "traveling_players"},
                 {"food_delta": 1, "wood_delta": 1}),
            case("A160-buy", {"food": 3},
                 {"other_player_uses": "traveling_players", "buy_vegetable": True},
                 {"food_delta": -1, "wood_delta": 1, "vegetable_delta": 1}),
            case("A160-not-triggered", {"food": 0, "wood": 0},
                 {"owner_uses": "traveling_players"}, {"food_delta": 0, "wood_delta": 0}),
        ],
    },
    "B001": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play", [gain("clay", 5),
                               action("renovation", must_pay_cost=True)]),
        ],
        "notes": "Gain 5 clay and a Renovation action; the renovation cost must still be paid.",
        "tests": [
            case("B001-normal", {"clay": 0}, {"play": "B001", "renovate": True},
                 {"clay_delta": 5, "renovation_granted": True, "post_play": "left_neighbor_hand"}),
            case("B001-no-renovate", {"clay": 0}, {"play": "B001", "renovate": False},
                 {"clay_delta": 5, "renovation_granted": False}),
        ],
    },
    "B004": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play", [gain("wood", per(count("person_on_accumulation_space"), 1))]),
        ],
        "notes": "Gain 1 wood per own person standing on accumulation spaces.",
        "tests": [
            case("B004-normal", {"people_on_accumulation_spaces": 2}, {"play": "B004"},
                 {"wood_delta": 2}),
            case("B004-zero", {"people_on_accumulation_spaces": 0}, {"play": "B004"},
                 {"wood_delta": 0}),
        ],
    },
    "B005": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play",
                   [gain(table("hand_occupation_count",
                               {"0-4": "stone", "5": "reed", "6": "clay", "7": "wood"}), 1)],
                   optional=False, snap="before_effect"),
        ],
        "notes": "0-4/5/6/7 occupations left in hand yield 1 stone/reed/clay/wood.",
        "tests": [
            case("B005-normal", {"hand_occupations": 3}, {"play": "B005"}, {"stone_delta": 1}),
            case("B005-boundary-4", {"hand_occupations": 4}, {"play": "B005"},
                 {"stone_delta": 1}),
            case("B005-5", {"hand_occupations": 5}, {"play": "B005"}, {"reed_delta": 1}),
            case("B005-7", {"hand_occupations": 7}, {"play": "B005"}, {"wood_delta": 1}),
        ],
    },
    "B006": {
        "post": "left_neighbor_hand",
        "effects": [effect("on_play", [gain("stone", per(count("person"), 1))])],
        "notes": "Gain 1 stone per person in the owner's supply.",
        "tests": [
            case("B006-normal", {"people": 3}, {"play": "B006"}, {"stone_delta": 3}),
            case("B006-boundary", {"people": 2}, {"play": "B006"}, {"stone_delta": 2}),
        ],
    },
    "B007": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play", [gain("food", 2),
                               gain("food", per(count("major_improvement_bottom_row"), 1))]),
        ],
        "notes": "Gain 2 food plus 1 food per major improvement taken from the bottom row.",
        "tests": [
            case("B007-normal", {"major_bottom_row": 2}, {"play": "B007"}, {"food_delta": 4}),
            case("B007-zero", {"major_bottom_row": 0}, {"play": "B007"}, {"food_delta": 2}),
        ],
    },
    "B009": {
        "post": "left_neighbor_hand",
        "effects": [
            effect("on_play",
                   [choice(options=[{"kind": "gain", "good": "reed", "amount": 1},
                                    {"kind": "exchange",
                                     "give": [{"good": "reed", "amount": 1}],
                                     "receive": [{"good": "cattle", "amount": 1}]}])],
                   optional=True, decision="owner"),
        ],
        "notes": "Gain 1 reed, or exchange 1 reed for 1 cattle.",
        "tests": [
            case("B009-normal", {"reed": 0}, {"choose": "gain_reed"}, {"reed_delta": 1}),
            case("B009-exchange", {"reed": 1}, {"choose": "exchange_cattle"},
                 {"reed_delta": -1, "cattle_delta": 1}),
            case("B009-not-triggered", {"reed": 0}, {"choose": "exchange_cattle"},
                 {"reed_delta": 0, "cattle_delta": 0}),
        ],
    },
    "B016": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 1)]),
            effect("after_renovation", [stable(1, wood_cost=0)],
                   optional=True, freq="every_use", dur="permanent", order=1,
                   decision="owner"),
        ],
        "notes": "Gain 1 food on play; each renovation may add a stable without paying wood.",
        "tests": [
            case("B016-normal", {"food": 0}, {"play": "B016"}, {"food_delta": 1}),
            case("B016-renovate", {"stables": 0}, {"renovate": 1, "build_stable": True},
                 {"stables_delta": 1, "wood_delta": 0}),
            case("B016-not-triggered", {"stables": 0}, {"renovate": 1, "build_stable": False},
                 {"stables_delta": 0}),
        ],
    },
    "B025": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 1)]),
            effect("after_occupation", [action("bake_bread")],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 1 food on play; each occupation played grants an extra Bake Bread action.",
        "tests": [
            case("B025-normal", {"food": 0}, {"play": "B025"}, {"food_delta": 1}),
            case("B025-occupation", {"bread_oven": True}, {"play_occupation": "A086"},
                 {"bake_bread_granted": True}),
        ],
    },
    "B033": {
        "post": "played_area",
        "effects": [
            effect("on_play", [points(per(remaining_rounds(), 1))]),
            effect("passive", [special("forbid_renovation")],
                   freq="always", dur="permanent", snap="live_state", order=1),
        ],
        "notes": "Gain 1 point per complete round left; may no longer renovate.",
        "tests": [
            case("B033-normal", {"remaining_rounds": 4}, {"play": "B033"},
                 {"points_delta": 4}),
            case("B033-boundary", {"remaining_rounds": 0}, {"play": "B033"},
                 {"points_delta": 0}),
            case("B033-renovation", {"renovations_left": 1}, {"renovate": 1},
                 {"renovation_allowed": False}),
        ],
    },
    "B037": {
        "post": "played_area",
        "effects": [effect("on_play", [gain("food", 1)])],
        "notes": "Gain 1 food on play; printed 3 points and requires 6 field tiles plus all animal types.",
        "tests": [
            case("B037-normal", {"food": 0}, {"play": "B037"}, {"food_delta": 1}),
            case("B037-not-triggered", {"field_tiles": 5, "animal_types": 3},
                 {"play": "B037"}, {"play_rejected": True}),
        ],
    },
    "B054": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 2)]),
            effect("after_sow", [gain("food", per(count("stable"), 1))],
                   cond=[cond("sow_unconditional")],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 2 food on play; after each unconditional Sow get 1 food per stable.",
        "tests": [
            case("B054-normal", {"food": 0}, {"play": "B054"}, {"food_delta": 2}),
            case("B054-sow", {"stables": 3}, {"sow": 1}, {"food_delta": 3}),
            case("B054-zero-stables", {"stables": 0}, {"sow": 1}, {"food_delta": 0}),
        ],
    },
    "B058": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 1)]),
            effect("harvest_field_phase",
                   [gain("food", per(count("vegetable_taken_from_field"), 1))],
                   freq="every_use", dur="permanent", order=1),
        ],
        "notes": "Gain 1 food on play; each vegetable taken from a field in the field phase gives 1 extra food.",
        "tests": [
            case("B058-normal", {"food": 0}, {"play": "B058"}, {"food_delta": 1}),
            case("B058-harvest", {"vegetables_from_fields": 2}, {"harvest_field_phase": 1},
                 {"food_delta": 2}),
            case("B058-zero", {"vegetables_from_fields": 0}, {"harvest_field_phase": 1},
                 {"food_delta": 0}),
        ],
    },
    "B059": {
        "post": "played_area",
        "effects": [
            effect("on_play",
                   [gain("food", {"op": "if_else",
                                  "args": [{"op": "played_on_space",
                                            "args": ["major_improvement"]}, 4, 2]})]),
        ],
        "notes": "4 food if played on the Major Improvement action space, otherwise 2 food.",
        "tests": [
            case("B059-normal", {"played_on_space": "major_improvement"}, {"play": "B059"},
                 {"food_delta": 4}),
            case("B059-other", {"played_on_space": "minor_improvement"}, {"play": "B059"},
                 {"food_delta": 2}),
        ],
    },
    "B071": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("food", 1), gain("grain", 1), gain("vegetable", 1)],
                   cond=[cond("harvests_equal_occupations")]),
        ],
        "notes": "If completed harvests equal occupations played: 1 food, 1 grain and 1 vegetable.",
        "tests": [
            case("B071-normal", {"completed_harvests": 2, "occupations_played": 2},
                 {"play": "B071"},
                 {"food_delta": 1, "grain_delta": 1, "vegetable_delta": 1}),
            case("B071-not-triggered", {"completed_harvests": 2, "occupations_played": 1},
                 {"play": "B071"}, {"food_delta": 0, "grain_delta": 0, "vegetable_delta": 0}),
        ],
    },
    "B073": {
        "post": "played_area",
        "effects": [
            effect("on_play",
                   [gain(table("room_count", {"2": "vegetable", "3": "food",
                                              "4": "grain", "5": "vegetable"}), 1)],
                   cond=[cond("room_count_in", values=[2, 3, 4, 5])]),
        ],
        "notes": "Exactly 2/3/4/5 rooms yield 1 vegetable/food/grain/vegetable.",
        "tests": [
            case("B073-normal", {"rooms": 3}, {"play": "B073"}, {"food_delta": 1}),
            case("B073-4", {"rooms": 4}, {"play": "B073"}, {"grain_delta": 1}),
            case("B073-not-triggered", {"rooms": 6}, {"play": "B073"},
                 {"food_delta": 0, "grain_delta": 0, "vegetable_delta": 0}),
        ],
    },
    "B088": {
        "post": "played_area",
        "effects": [
            effect("on_play",
                   [renovate(free_building_resources=True),
                    action("build_fences")],
                   cond=[cond("room_count", n=2)],
                   optional=True, decision="owner"),
        ],
        "notes": "If the house has exactly 2 rooms: renovate without building resources, then Build Fences.",
        "tests": [
            case("B088-normal", {"house": "wood", "rooms": 2}, {"play": "B088"},
                 {"house": "clay", "building_resources_spent": 0,
                  "build_fences_granted": True}),
            case("B088-not-triggered", {"house": "wood", "rooms": 3}, {"play": "B088"},
                 {"house": "wood", "build_fences_granted": False}),
        ],
    },
    "B103": {
        "post": "played_area",
        "effects": [
            effect("on_play", [gain("wood", 1), gain("reed", 1)]),
            effect("after_improvement_declined",
                   [gain("food", 1)],
                   cond=[cond("declined_improvement", kind="minor")],
                   optional=True, freq="every_use", dur="permanent", order=1,
                   decision="owner"),
            effect("after_improvement_declined",
                   [gain("vegetable", 1)],
                   cond=[cond("declined_improvement", kind="major")],
                   optional=True, freq="every_use", dur="permanent", order=2,
                   decision="owner"),
        ],
        "notes": "Gain 1 wood and 1 reed on play; declining a minor/major improvement action gives 1 food/vegetable instead.",
        "tests": [
            case("B103-normal", {"wood": 0, "reed": 0}, {"play": "B103"},
                 {"wood_delta": 1, "reed_delta": 1}),
            case("B103-minor", {"declined": "minor"}, {"decline_improvement": "minor"},
                 {"food_delta": 1}),
            case("B103-major", {"declined": "major"}, {"decline_improvement": "major"},
                 {"vegetable_delta": 1}),
            case("B103-not-triggered", {"declined": None}, {"use_space": "forest"},
                 {"food_delta": 0, "vegetable_delta": 0}),
        ],
    },
    "C004": {
        "post": "left_neighbor_hand",
        "effects": [effect("on_play", [gain("wood", per(count("occupation"), 1))])],
        "notes": "Gain 1 wood for each occupation already in front of the owner.",
        "tests": [
            case("C004-normal", {"occupations_played": 3}, {"play": "C004"},
                 {"wood_delta": 3}),
            case("C004-zero", {"occupations_played": 0}, {"play": "C004"},
                 {"wood_delta": 0}),
        ],
    },
}


def parse_cost(raw: str):
    if raw == "unknown":
        return None
    if raw in ("none", ""):
        return []
    entries = []
    for part in raw.split(";"):
        good, amount = part.split(":")
        entries.append({"good": good, "amount": int(amount)})
    return entries


def build_spec(row: dict) -> dict:
    card_id = row["card_id"]
    entry = CARDS[card_id]
    play = {
        "action": row["action"],
        "cost": parse_cost(row["cost"]),
        "prerequisites": PREREQ.get(card_id, []),
        "printed_points": None if row["printed_points"] == "unknown" else int(row["printed_points"]),
    }
    if row["cost_alternative"] not in ("none", "", "unknown"):
        play["alternative_cost_notes"] = row["cost_alternative"]
    tests = entry["tests"]
    review = {
        "status": "bga_checked",
        "unresolved_fields": [],
        "test_case_ids": [c["id"] for c in tests],
        "checked_by": "AI source comparison",
        "checked_on": CHECKED_ON,
        "implementation_status": "not_started",
        "notes": entry["notes"],
    }
    return {
        "schema_version": 1,
        "card_id": card_id,
        "identity": {
            "kind": row["kind"],
            "name_ja": row["name_ja"],
            "name_en": row["name_en"] or None,
        },
        "bga": {
            "listed": row["bga_listed"],
            "playable": row["bga_playable"],
            "ban_status": row["ban"],
            "base_2016": row["base_2016"],
            "eligibility": {
                "2": row["eligible_2"],
                "3": row["eligible_3"],
                "4": row["eligible_4"],
            },
            "references": [
                {
                    "url": row["source_url"],
                    "language": "ja",
                    "checked_on": CHECKED_ON,
                    "note": "third-party BGA wiki card page; cost, prerequisite, points and effect",
                },
                {
                    "url": EN_LIST,
                    "language": "en",
                    "checked_on": CHECKED_ON,
                    "note": "English list page row used to cross-check the card id and name",
                },
            ],
        },
        "play": play,
        "effects": entry["effects"],
        "post_play": {"destination": entry["post"]},
        "review": review,
    }


def build_tests(row: dict) -> dict:
    entry = CARDS[row["card_id"]]
    return {
        "schema_version": 1,
        "card_id": row["card_id"],
        "cases": entry["tests"],
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cards", default="")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args(argv[1:])
    only = {c.strip() for c in args.cards.split(",") if c.strip()}
    with BASE.open(encoding="utf-8-sig", newline="") as handle:
        rows = {row["card_id"]: row for row in csv.DictReader(handle)}
    written = 0
    skipped = []
    for card_id in sorted(CARDS):
        if only and card_id not in only:
            continue
        row = rows.get(card_id)
        if row is None:
            print(f"{card_id}: missing from the base ledger", file=sys.stderr)
            return 1
        spec_path = SPECS / f"{card_id}.json"
        test_path = TESTS / f"{card_id}.json"
        if not args.force and (spec_path.exists() or test_path.exists()):
            skipped.append(card_id)
            continue
        SPECS.mkdir(parents=True, exist_ok=True)
        TESTS.mkdir(parents=True, exist_ok=True)
        spec_path.write_text(
            json.dumps(build_spec(row), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        test_path.write_text(
            json.dumps(build_tests(row), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        written += 1
    print(json.dumps({"written": written, "skipped": skipped}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
