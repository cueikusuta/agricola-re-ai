# Primitive Mechanism Re-Classification Report

**Date**: 2026-11-26  
**Source**: `data/family_classification.json` → `data/primitive_classification.json`  
**Target**: `new_operation_other` (279 cards) + `new_passive_other` (45 cards) = **279 unique cards** (45 overlap)

---

## Executive Summary

| Metric | Value |
|--------|-------|
| **Original "other" cards** | 279 unique |
| **Primitives identified** | 153 distinct primitives |
| **High-level categories** | 14 categories |
| **Cards with `other:unknown`** | 63 (13 pure + 50 mixed) |
| **"Other" reduction** | **279 → 63 cards** (77% classified) |
| **Coverage by existing/known ops** | 0 fully, 72 partially, 207 none |

---

## High-Level Primitive Categories

| Category | Card-Refs | Distinct Primitives | Description |
|----------|-----------|---------------------|-------------|
| **new_trigger** | 109 | 51 | New trigger timings not in existing vocabulary |
| **new_predicate** | 71 | 51 | New condition predicates |
| **other** | 70 | 7 | Unclassified/unknown kinds (63 `other:unknown`) |
| **existing_operation** | 64 | 6 | Operations already in Rust (`gain_resource`, `spend_resource`, `gain_points`, `exchange`, `modify_capacity`, `discount_cost`) |
| **marker_placement** | 45 | 1 | Place markers/goods on round spaces or future rounds |
| **choice** | 38 | 1 | Player choice from options |
| **pass_to_left** | 28 | 1 | Post-play pass to left neighbor |
| **accumulate** | 24 | 1 | Stack goods/tiles on cards, card-based storage |
| **copy_move** | 17 | 1 | Move cards, workers, or resources between zones |
| **exchange** | 16 | 1 | Resource conversion/exchange (incl. conditional) |
| **known_operation** | 14 | 4 | Operations known but not in Rust vocab (`grant_action`, `plow_field`, `build_stable`, `renovate`) |
| **card_play** | 13 | 1 | Play cards (occupations/improvements) with modified rules |
| **build** | 12 | 1 | Build fences, rooms, improvements |
| **animal_housing** | 12 | 1 | Pasture/stable capacity, animal placement, breeding |
| **field_mod** | 11 | 1 | Field goods manipulation, crop conversion |
| **conditional_gain** | 8 | 1 | Gain resources based on conditions |
| **grant_action** | 6 | 1 | Grant additional actions/spaces |
| **action_order** | 6 | 1 | Modify turn/action sequence |
| **scoring** | 5 | 1 | End-game scoring modifications |
| **cook_bake** | 4 | 1 | Cooking/baking modifications |
| **random_selection** | 2 | 1 | Random pick from hand |
| **forbid_renovation** | 2 | 1 | Prevent renovation |
| **conditional_discount** | 2 | 1 | Cost discounts with conditions |
| **special_combine** | 1 | 1 | Combine specific spaces (reed bank + clay pit) |
| **modify_capacity** | 1 | 1 | Generic capacity modification |
| **forbid_family_growth** | 1 | 1 | Prevent family growth |
| **feeding_mod** | 1 | 1 | Modify feeding requirements |

---

## Top 20 Primitive Mechanisms (by card count)

| Primitive | Cards | Example Cards | Foundation Needed |
|-----------|-------|---------------|-------------------|
| `new_trigger:ongoing` | 16 | D121-D125 | New CardEffect variant for persistent triggers |
| `marker_placement` | 45 | A019, A022, A043... | Round-space marker system |
| `choice` | 38 | A086, A114, B009... | Choice effect handler |
| `existing:gain_resource` | 35 | A016, A027, A074... | ✅ Already implemented |
| `pass_to_left` | 28 | A003, A006, B002... | ✅ Already implemented (post_play) |
| `accumulate` | 24 | A040, A081, A082... | Card-based goods stacking |
| `copy_move` | 17 | A025, A081, A094... | Move cards/workers between zones |
| `exchange` | 16 | A016, A028, A070... | Resource conversion |
| `new_trigger:after_renovation` | 6 | A045, A120... | Renovation trigger |
| `new_trigger:round_end` | 6 | A058, A070... | Round-end trigger |
| `new_trigger:harvest_feeding` | 6 | A148, C062... | Harvest feeding trigger |
| `card_play` | 13 | A010, A026... | Modified card play rules |
| `build` | 12 | A027, A128... | Construction actions |
| `animal_housing` | 12 | A083, A084... | Pasture/stable capacity |
| `field_mod` | 11 | A019, A058... | Field goods manipulation |
| `known:grant_action` | 7 | B021, B093... | Grant action (known op) |
| `existing:spend_resource` | 10 | A084, A102... | ✅ Already implemented |
| `conditional_gain` | 8 | A006, A156... | Conditional resource gain |
| `grant_action` | 6 | A022, A137... | Grant action primitive |
| `action_order` | 6 | A129, A130... | Turn order modification |

---

## Remaining `other:unknown` Cards (63 total)

### Pure Unknown (13 cards - only `other:unknown`)
These have `needs_resolution` specs with only `description` field:
- **B011** 畝, **B012** 荒地開拓, **B014** 石切り場, **B023** 採石事業所
- **B086** 貯蔵庫, **B089** 大工の手斧, **B113** 革なめし
- **C046** 大進歩, **E073** 野外焼却炉, **E074** 共同浴場
- **E075** 土器窯, **E080** 灌漑用水路, **E088** 大工場

### Mixed Unknown (50 cards - `other:unknown` + other primitives)
These have partial classification but one effect uses `other` without `kind`:
- **B015** 釜, **B024** 陶工, **B026** 穀物利用, **B027** 納屋
- **B028** 釣り船, **B030** 柵, **B036** 穀物蔵, **B069** 薪置き場
- **B072** 牧草地, **B076** 木材置き場, **B083** 猪小屋, **B093** 葦置き場
- **B096** 釣り堀, **B114** 革なめし職人, **B119** 葦置き場
- **D121-D136** (E帯 16 cards - ongoing triggers)
- **D147, D150, D151** (E帯)
- **E070, E072, E076, E078, E081, E084, E085, E086, E090, E091, E092, E093, E094, E095, E096, E097**

---

## Existing Vocabulary Coverage

| Status | Count | Description |
|--------|-------|-------------|
| **Fully covered** | 0 | No cards use ONLY existing/known operations |
| **Partially covered** | 72 | Mix of existing/known ops + new primitives |
| **Not covered** | 207 | Only new primitives (no existing ops) |

**Key insight**: The 6 existing operations (`gain_resource`, `spend_resource`, `gain_points`, `exchange`, `modify_capacity`, `discount_cost`) appear in 64 card-refs across 72 cards, but always alongside new primitives.

---

## Foundation Implementation Priority (Recommended Order)

Based on card count, dependency, and reuse potential:

| Priority | Primitive Category | Cards | Foundation Card | Notes |
|----------|-------------------|-------|-----------------|-------|
| 1 | **marker_placement** | 45 | A019 食材かご | Core round-space marker system; enables 7 predicates |
| 2 | **accumulate** | 24 | A040 納屋 | Card-based goods stacking; used by 24 cards |
| 3 | **choice** | 38 | A086 踏み粘土 | Player choice handler; high reuse |
| 4 | **copy_move** | 17 | A025 大工の金槌 | Move cards/workers between zones |
| 5 | **exchange** | 16 | A016 大工の手斧 | Resource conversion (partial existing) |
| 6 | **new_trigger:ongoing** | 16 | D121 綿羊飼い | Persistent trigger type |
| 7 | **card_play** | 13 | A010 板張りの小屋 | Modified card play rules |
| 8 | **build** | 12 | A027 大工の金槌 | Construction actions |
| 9 | **animal_housing** | 12 | A083 猪小屋 | Pasture/stable capacity |
| 10 | **field_mod** | 11 | A019 食材かご | Field goods manipulation |
| 11 | **known:grant_action** | 7 | B021 子供の動物市場 | Grant action (known op) |
| 12 | **conditional_gain** | 8 | A006 納屋 | Conditional resource gain |
| 13 | **action_order** | 6 | A129 大工の手斧 | Turn order modification |
| 14 | **scoring** | 5 | B136 情報屋 | End-game scoring |
| 15 | **cook_bake** | 4 | A061 幅木 | Cooking/baking mods |
| 16 | **forbid_renovation** | 2 | A010 板張りの小屋 | Prevent renovation |
| 17 | **conditional_discount** | 2 | A087 板張りの小屋 | Conditional cost discount |
| 18 | **random_selection** | 2 | A003 ペーパーナイフ | Random pick from hand |
| 19 | **special_combine** | 1 | A162 納屋 | Combine reed+clay spaces |
| 20 | **forbid_family_growth** | 1 | E155 | Prevent family growth |
| 21 | **feeding_mod** | 1 | E159 | Modify feeding reqs |

**Total foundation cards**: ~21 (one per category above)

---

## Complex Cards (Multi-Primitive) - Implement Last

Cards with 5+ primitives (wait for all foundations):
- **E027** (6): accumulate, grant_action, new_trigger:after_work_phase, new_trigger:any_time, other:discard_food_from_card, predicate:card_food_at_least
- **A074** (5): existing:gain_resource, marker_placement, new_trigger:after_stables_built, predicate:round_space_has_placed_good, predicate:stables_built_on_own_turn
- **B021** (5): accumulate, existing:gain_resource, known:grant_action, new_trigger:after_wood_gain, predicate:card_food_empty
- **B096** (5): existing:gain_resource, known:grant_action, other:unknown, predicate:round_is_odd, predicate:wood_on_this_round_space
- **C148** (5): accumulate, existing:exchange, new_trigger:card_clay_reaches, predicate:card_clay_at_least, predicate:is_cumulative_space
- **D099** (5): accumulate, existing:gain_points, existing:spend_resource, new_trigger:after_final_harvest, predicate:round_number

---

## Next Steps

1. **Add missing `kind` mappings** for the 63 `other:unknown` cards by reading their `description` fields
2. **Create foundation cards** in priority order above
3. **Batch implement** cards by primitive category once foundation lands
4. **Track progress** by daily main-merged count (target: 52/day)

---

## Files Generated

- `data/primitive_classification.json` - Full machine-readable classification
- This report (markdown)

The original `data/family_classification.json` is preserved unchanged.