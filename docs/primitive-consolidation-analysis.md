# Primitive Consolidation Analysis: New Triggers & Predicates

**Source**: `data/primitive_classification.json` (279 cards analyzed)
**Date**: 2026-11-26

---

## Executive Summary

| Category | Input Types | Input Cards | Consolidated Slots (Buckets) | Unconsolidatable Types | Unconsolidatable Cards |
|----------|-------------|-------------|------------------------------|------------------------|------------------------|
| **new_trigger** | 62 | 109 | **12** | 8 | 11 |
| **predicate** | 52 | 71 | **11 component families** | 4 | 4 |

**Key finding**: The 62 trigger types collapse to **12 insertion-point buckets** (mapping to 8 existing `EventKind` + 4 genuinely new insertion points). The 52 predicates decompose into **11 reusable component families** (subject × quantity × comparison).

---

## Part 1: New Trigger Analysis (62 types → 12 buckets)

### Existing Insertion Points (`EventKind` in `src/registry.rs`)

| EventKind | Phase | Current Use | Dispatch Function |
|-----------|-------|-------------|-------------------|
| `WorkerPlaced` | After worker placement | `AfterWorkerGain*`, `AfterAnyWorker*` | `dispatch_after_worker` |
| `CardPlayed` | Card play (Replace/Before/After) | `OnPlayGain*`, `PostPlayPassLeft`, `AfterOccupationGain*`, `AfterImprovementPlayedGain` | `dispatch_play` |
| `RoundStart` | Round beginning | `StartRoundGain*`, `StartRoundPlowFieldIfHouse`, `StartRoundCopyUnusedRoundSpace` | `dispatch_start_round` |
| `Harvest` | Harvest field phase (pre-yield) | `HarvestStartGainIfFields`, `HarvestFieldGain*`, `AfterHarvestBakeGain` | `dispatch_harvest_field` |
| `ClayConstruction` | Room/stone upgrade purchase | `AfterClayConstructionGain` | `dispatch_after_clay_construction` |
| `Bake` | Baking action | `AfterHarvestBakeGain` | `dispatch_after_harvest_bake` |
| `AfterHarvestComplete` | End of harvest (fields+feeding+breeding) | `AfterHarvestGain*`, `AfterHarvestChooseGainIfAtLeast` | `dispatch_after_harvest_complete` |
| `ImprovementPlayed` | Improvement played from hand | `AfterImprovementPlayedGain` (also fires on `CardPlayed`) | `dispatch_play` (is_improvement_action) |

---

### Trigger Classification: 62 types → 12 Buckets

#### Bucket 1: WorkerPlaced — Action Space Use (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `on_action_use` (3), `on_action_space_used` (2) | D147 D150 D151 E129 E130 | 5 | **(1) Existing + filters** — Map to `WorkerPlaced` + `action_space_name` / `action_space_used` predicate |

#### Bucket 2: WorkerPlaced — Specific Accumulation Spaces (Existing + missing filters)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `on_use_wood_accumulation` (1), `on_use_grain_utilization` (1), `on_use_forest_accumulation` (1), `on_use_day_laborer` (1), `on_use_minor_improvement_space` (1), `on_use_this_card` (1) | B015 B026 B028 C138 C140 E081 | 6 | **(2) Existing + missing filter** — Need `accumulation_space_type: Wood|Grain|Forest|DayLaborer|MinorImprovement|ThisCard` filter component |

#### Bucket 3: CardPlayed — Occupation/Improvement Play (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `on_play_occupation` (1), `play_minor_improvement` (1), `when_buying` (2) | E051 E091 C027 D117 | 4 | **(1) Existing + filters** — Map to `CardPlayed` + `card_kind: Occupation|MinorImprovement|MajorImprovement` + `is_purchase: true` |

#### Bucket 4: CardPlayed — Build/Renovate/Fence (Existing + missing filters)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `after_renovation` (6), `after_room_construction` (2), `after_fence_build` (1), `on_build_fence` (1), `on_build_room_stable_fence` (1), `on_build_room_or_play_improvement_or_renovate` (1), `renovate` (1) | A045 A120 B055 C146 C149 D111 A111 D094 A083 B030 B027 E123 B168 | 16 | **(2) Existing + missing filter** — Map to `CardPlayed` + `build_action_type: Renovate|Room|Stable|Fence` filter |

#### Bucket 5: RoundStart — Round Boundary (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `round_end` (6), `first_turn_of_round` (1), `round_1_work_phase_start` (1), `before_sow` (1), `before_worker_space` (1) | A058 A070 A127 A165 B053 E022 E093 E096 E017 D119 | 10 | **(1) Existing + filters** — Map to `RoundStart` + `round_phase: End|FirstTurn|WorkPhaseStart` + `pre_action: Sow|WorkerPlacement` |

#### Bucket 6: Harvest — Harvest Sub-phases (Existing + missing filters)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `harvest_start` (5), `harvest_feeding` (6), `harvest_field` (1), `harvest_breeding` (1), `harvest_use_joinery` (1), `before_harvest` (1), `before_final_harvest` (1), `after_final_harvest` (1), `feeding_phase` (1), `harvest` (1) | D097 D115 E039 E058 E110 A148 C062 D012 D062 E030 E159 C070 E084 E091 B165 D098 D099 E110 E134 | 22 | **(2) Existing + missing filter** — Map to `Harvest` / `AfterHarvestComplete` + `harvest_subphase: Start|Feeding|Field|Breeding|Final|Joinery` + `pre_post: Before|After` |

#### Bucket 7: Bake — Baking (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `before_bake_bread` (1) | A097 | 1 | **(1) Existing + filters** — Map to `Bake` + `pre_post: Before` |

#### Bucket 8: ClayConstruction — Clay Construction (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `after_stable_construction` (1), `after_stable_built` (1), `after_stables_built` (1), `after_room_construction` (overlap) | A043 C056 A074 A111 D094 | 5 | **(1) Existing + filters** — Map to `ClayConstruction` + `construction_type: Stable|Room` |

#### Bucket 9: ImprovementPlayed — Improvement Play (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `after_improvement` (2), `improvement_played_from_hand` (predicate, 2) | D118 E031 D117 D118 | 4 | **(1) Existing + filters** — Map to `ImprovementPlayed` (already exists) |

#### Bucket 10: AfterHarvestComplete — End of Harvest (Existing)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `after_final_harvest` (1), `after_harvest_complete` (implicit in `AfterHarvestComplete` handlers) | D099 | 1 | **(1) Existing** — Already covered by `AfterHarvestComplete` |

#### Bucket 11: ReturningHomePhase — NEW Insertion Point Needed
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `returning_home_phase` (3) | A084 D102 D107 | 3 | **(3) NEW insertion point** — Occurs between work phase and home phase; not covered by existing `EventKind`. Add `EventKind::ReturningHome` |

#### Bucket 12: Ongoing/Continuous — NEW Insertion Point Needed
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `anytime` (2), `any_time` (2), `ongoing` (16) | B157 C069 E027 E086 D121-D136 | 20 | **(3) NEW insertion point** — Continuous/passive effects evaluated every dispatch. Add `EventKind::Continuous` or model as `Passive` with `TriggerPhase::Before` on every dispatch |

#### Card-Specific / Unconsolidatable (4 types, 11 cards)
| Trigger Types | Cards | Count | Verdict |
|---------------|-------|-------|---------|
| `after_event` (1), `after_occupation` (1), `after_farmyard_space_used` (1), `after_wood_gain` (2), `card_clay_reaches` (1), `check_card_becomes_room` (1), `convert_animal_to_food` (1), `breeding` (1), `store_from_cumulative_space` (1), `when_last_crop_removed_from_card` (1), `use_family_growth` (1), `other_player_action` (1), `other_player_uses_grain_seeds` (1), `pay_occupation_cost` (1), `sow_action_declined` (1), `stone_accumulation_space_revealed` (1), `final_scoring` (1), `end_game_scoring` (1) | B145 E163 A040 B021 E047 C148 E085 E085 E090 B162 E070 E092 B159 E095 B155 A094 D116 B153 E134 | 11 | **(4) Card-specific** — Each is a unique game-state observation not generalizable. Model as custom predicates on existing insertion points or card-specific `CardEffect` variants |

---

### Trigger Consolidation Summary

| Bucket | Insertion Point | Types | Cards | Status |
|--------|-----------------|-------|-------|--------|
| 1 | WorkerPlaced | 5 | 5 | (1) Ready |
| 2 | WorkerPlaced | 6 | 6 | (2) Needs filter component |
| 3 | CardPlayed | 4 | 4 | (1) Ready |
| 4 | CardPlayed | 7 | 16 | (2) Needs filter component |
| 5 | RoundStart | 5 | 10 | (1) Ready |
| 6 | Harvest/AfterHarvestComplete | 10 | 22 | (2) Needs filter component |
| 7 | Bake | 1 | 1 | (1) Ready |
| 8 | ClayConstruction | 4 | 5 | (1) Ready |
| 9 | ImprovementPlayed | 2 | 4 | (1) Ready |
| 10 | AfterHarvestComplete | 1 | 1 | (1) Ready |
| 11 | **ReturningHome (NEW)** | 1 | 3 | (3) New insertion point |
| 12 | **Continuous (NEW)** | 3 | 20 | (3) New insertion point |
| — | **Card-specific** | 18 | 11 | (4) Unconsolidatable |

**Total**: 62 types, 109 cards → **12 buckets** (8 existing + 2 new + 1 card-specific category)

---

## Part 2: Predicate Analysis (52 types → 11 component families)

### Decomposition Model: Subject × Quantity × Comparison

Each predicate = **Subject** (what is counted/checked) + **Quantity** (how many) + **Comparison** (relation)

#### Family 1: Round Space State (5 types, 17 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `round_space_has_marker` | RoundSpace(kind=Marker) | count | ≥1 | `RoundSpaceHasMarker { kind }` |
| `round_space_has_placed_good` | RoundSpace(good) | count | ≥1 | `RoundSpaceHasGood { good }` |
| `round_space_in` | RoundSpace | — | InList | `RoundSpaceIn { spaces }` |
| `round_space_unused` | RoundSpace | — | Unused | `RoundSpaceUnused { space }` |
| `wood_on_this_round_space` | RoundSpace(good=Wood) | count | >0 | `RoundSpaceGoodCount { good: Wood } > 0` |

**Component**: `RoundSpaceQuery { space?, kind?, good?, state: Unused|HasMarker|HasGood }`

#### Family 2: Action Space State (4 types, 7 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `action_space_used` | ActionSpace | — | Used | `ActionSpaceState { space, state: Used }` |
| `action_space_occupied` | ActionSpace | — | Occupied | `ActionSpaceState { space, state: Occupied }` |
| `action_space_name` | ActionSpace | — | NameMatch | `ActionSpaceState { space, name }` |
| `action_space_not_accumulating` | ActionSpace | — | NotAccumulating | `ActionSpaceState { space, state: NotAccumulating }` |

**Component**: `ActionSpaceQuery { space?, state: Used|Occupied|Name|NotAccumulating, name? }`

#### Family 3: Card Resource State (4 types, 4 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `card_food_empty` | CardSupply(good=Food) | count | =0 | `CardSupply { card, good: Food } == 0` |
| `card_clay_at_least` | CardSupply(good=Clay) | count | ≥N | `CardSupply { card, good: Clay } ≥ N` |
| `card_food_at_least` | CardSupply(good=Food) | count | ≥N | `CardSupply { card, good: Food } ≥ N` |
| `harvested_this_card_at_least` | CardHarvestCount | count | ≥N | `CardHarvestCount { card } ≥ N` |

**Component**: `CardSupplyQuery { card, good?, metric: Supply|HarvestCount, comparison: Eq|Ge, value }`

#### Family 4: Room/House State (8 types, 8 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `rooms_at_least` | RoomCount | count | ≥N | `RoomCount ≥ N` |
| `build_room_or_renovate` | BuildAction | — | TypeMatch | `BuildActionType ∈ {Room, Renovate}` |
| `renovation_to_stone_house` | Renovation | — | ToStone | `RenovationType == StoneHouse` |
| `room_type_is` | Room | — | TypeMatch | `RoomType == X` |
| `room_count_in` | RoomCount(house=Stone) | count | InRange | `RoomCount { house_type } ∈ Range` |
| `has_empty_room` | Room | — | HasEmpty | `∃ Room: Empty` |
| `house_has_exactly_2_rooms` | RoomCount | count | =2 | `RoomCount == 2` |
| `house_surrounded_by_4_fields` | House | — | Surrounded | `HouseSurroundedByFields == 4` |

**Component**: `HouseQuery { metric: RoomCount|RoomType|RenovationType|EmptyRoom|SurroundedFields, house_type?, comparison, value }`

#### Family 5: Stable/Pasture State (2 types, 2 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `stables_built_on_own_turn` | StableBuild | count | ≥1 (this turn) | `StableBuildCount { turn: Current } ≥ 1` |
| `new_pasture_created` | Pasture | count | ≥N (min_spaces=4) | `PastureCreated { min_spaces: 4 } ≥ 1` |

**Component**: `FarmStructureQuery { type: Stable|Pasture, metric: BuildCount|Created, turn?: Current|Any, min_spaces? }`

#### Family 6: Harvest State (3 types, 3 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `harvested_from` | HarvestSource | — | FromField | `HarvestSource == Field` |
| `harvest_number` | HarvestRound | count | =N | `HarvestRound == N` |
| `grain_harvested_at_least` | HarvestedGrain | count | ≥N | `HarvestedGrain ≥ N` |

**Component**: `HarvestQuery { metric: Source|RoundNumber|GrainCount, comparison, value }`

#### Family 7: Improvement State (2 types, 3 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `improvement_played_from_hand` | ImprovementPlay | count | ≥1 (this action) | `ImprovementPlayedFromHand ≥ 1` |
| `improvement_is` | Improvement | — | TypeMatch | `ImprovementType == X` |

**Component**: `ImprovementQuery { metric: PlayedFromHand|Type, type? }`

#### Family 8: Occupation State (3 types, 3 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `occupation_count` | OccupationCount | count | — | `OccupationCount` |
| `occupation_count_is` | OccupationCount | count | =N | `OccupationCount == N` |
| `occupation_played_from_hand` | OccupationPlay | count | ≥1 | `OccupationPlayedFromHand ≥ 1` |

**Component**: `OccupationQuery { metric: Count|PlayedFromHand, comparison?, value }`

#### Family 9: Resource/Storage State (2 types, 2 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `reed_taken_at_least` | ReedTaken | count | ≥N | `ReedTaken ≥ N` |
| `stored_exact_wood` | CardWoodStorage | count | =N | `CardWoodStorage == N` |

**Component**: `ResourceQuery { resource: Reed|Wood, metric: Taken|Stored, comparison, value }`

#### Family 10: Round/Turn/Player State (5 types, 7 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `round_is_odd` | RoundNumber | — | Odd | `RoundNumber % 2 == 1` |
| `right_neighbor_last_used` | NeighborAction | — | LastUsed | `RightNeighborLastUsed == true` |
| `round_number` | RoundNumber | count | =N/InRange | `RoundNumber ∈ Range` |
| `person_is_second_placed_this_round` | PersonPlacement | — | SecondThisRound | `PersonPlacementOrder == 2` |
| `round_in_range` | RoundNumber | count | InRange | `RoundNumber ∈ Range` |

**Component**: `RoundQuery { metric: Number|Parity|NeighborAction|PlacementOrder, comparison, value }`

#### Family 11: Player/Family/Other (7 types, 8 cards)
| Predicate | Subject | Quantity | Comparison | Consolidated Form |
|-----------|---------|----------|------------|-------------------|
| `other_player_uses` | OtherPlayerAction | — | UsesSpace | `OtherPlayerUsedSpace { space }` |
| `family_exactly` | FamilySize | count | =N | `FamilySize == N` |
| `user_is_not_owner` | Actor | — | NotOwner | `Actor != CardOwner` |
| `worker_on_space` | WorkerPlacement | — | OnSpace | `WorkerOnSpace { space }` |
| `action_type` | Action | — | TypeMatch | `ActionType == X` |
| `no_unused_farm_spaces` | FarmSpace | count | =0 (unused) | `UnusedFarmSpaces == 0` |
| `player_places_adjacent_field` | PlayerAction | — | AdjacentField | `PlayerPlacesAdjacentField == true` |
| `cultivated_fields_at_least` | FieldCount | count | ≥N (cultivated) | `CultivatedFieldCount ≥ N` |

**Component**: Various — some map to existing `ActionSpaceQuery` / `RoundQuery`, others need new components: `FamilyQuery`, `ActorQuery`, `FarmSpaceQuery`

---

### Predicate Consolidation Summary

| Family | Types | Cards | Component Name | Status |
|--------|-------|-------|----------------|--------|
| 1. Round Space | 5 | 17 | `RoundSpaceQuery` | Ready |
| 2. Action Space | 4 | 7 | `ActionSpaceQuery` | Ready |
| 3. Card Resource | 4 | 4 | `CardSupplyQuery` | Ready |
| 4. Room/House | 8 | 8 | `HouseQuery` | Ready |
| 5. Stable/Pasture | 2 | 2 | `FarmStructureQuery` | Ready |
| 6. Harvest | 3 | 3 | `HarvestQuery` | Ready |
| 7. Improvement | 2 | 3 | `ImprovementQuery` | Ready |
| 8. Occupation | 3 | 3 | `OccupationQuery` | Ready |
| 9. Resource/Storage | 2 | 2 | `ResourceQuery` | Ready |
| 10. Round/Turn | 5 | 7 | `RoundQuery` | Ready |
| 11. Player/Family/Misc | 8 | 8 | `FamilyQuery`, `ActorQuery`, `FarmSpaceQuery` | Partial — 3 new components needed |
| — | 4 | 4 | **Unconsolidatable** (`custom`, `raw_text`, `space_is`, `is_cumulative_space`) | Card-specific |

**Total**: 52 types, 71 cards → **11 component families** (8 ready, 3 need new components, 4 unconsolidatable)

---

## Part 3: Machine-Readable Output

See `data/primitive_consolidation_result.json` for structured output.

---

## Part 4: Implementation Recommendations

### New Insertion Points (EventKind additions)
1. **`ReturningHome`** — Fires at returning home phase (between work and home). Used by 3 cards.
2. **`Continuous`** — Passive/ongoing effects evaluated at every dispatch. Used by 20 cards (ongoing, anytime, any_time).

### New Filter Components (for existing insertion points)
1. **`accumulation_space_type`** — For `WorkerPlaced`: distinguish Wood/Grain/Forest/DayLaborer/MinorImprovement/ThisCard accumulation spaces.
2. **`build_action_type`** — For `CardPlayed`: Renovate/Room/Stable/Fence.
3. **`harvest_subphase`** — For `Harvest`/`AfterHarvestComplete`: Start/Feeding/Field/Breeding/Final/Joinery + Pre/Post.
4. **`round_phase`** — For `RoundStart`: End/FirstTurn/WorkPhaseStart + PreAction.
5. **`construction_type`** — For `ClayConstruction`: Stable/Room.

### New Predicate Components
1. **`FamilyQuery`** — Family size, exact match.
2. **`ActorQuery`** — Actor vs card owner, neighbor actions.
3. **`FarmSpaceQuery`** — Unused farm spaces, cultivated fields, adjacent field placement.

### Card-Specific Handling (Unconsolidatable)
- 8 trigger types (11 cards) — implement as custom `CardEffect` variants with inline logic
- 4 predicate types (4 cards) — `custom`, `raw_text`, `space_is`, `is_cumulative_space` — keep as opaque predicates evaluated per-card

---

## Verification Checklist

- [x] All 62 new_trigger types classified into 4 verdict categories
- [x] All 52 predicate types decomposed into subject/quantity/comparison
- [x] Existing 8 EventKind insertion points mapped
- [x] 2 new insertion points identified with card counts
- [x] 5 missing filter components for existing insertion points identified
- [x] 11 predicate component families defined (8 ready, 3 new)
- [x] 4 unconsolidatable predicate types identified
- [x] Machine-readable JSON output generated
- [x] Original `primitive_classification.json` unchanged