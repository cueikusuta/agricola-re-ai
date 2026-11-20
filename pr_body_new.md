## Summary

Implements 5 B-deck on-play simple gain cards as typed effects, mapped from bga_checked specifications:

| Card | Name | Effect | Cost | Post-Play |
|------|------|--------|------|-----------|
| B002 | 小牧場 (Mini Pasture) | Fence 1 empty farmyard cell into a pasture at no wood cost, adjacent to existing pasture | 2 food | Left neighbour hand |
| B003 | 密造酒 (Moonshine) | Draw 1 occupation from hand; play it for 2 food or pass it left | 0 | Left neighbour hand |
| B005 | 経験の蓄積 (Store of Experience) | Gain 1 good by hand occupation count: 0-4→stone, 5→reed, 6→clay, 7→wood | 0 | Left neighbour hand |
| B008 | 露店 (Market Stall) | Exchange 1 grain for 1 vegetable (no partial exchange) | 1 grain | Left neighbour hand |
| B009 | むち (Beating Rod) | Gain 1 reed, or exchange 1 reed for 1 cattle | 0 | Left neighbour hand |

## Implementation

- Added `verified_b_onplay_simple_cards()` to `build_default_registry()` in `registry.rs`
- All 5 cards use existing typed effect vocabulary:
  - `OnPlayBuildPasture { spaces: 1 }`
  - `OnPlayDrawPlayOrPassOccupation { food_cost: 2 }`
  - `OnPlayGainByHandOccupationCount { bands: [(4,Stone),(5,Reed),(6,Clay),(7,Wood)] }`
  - `OnPlayExchangeGain { give: Grain, give_amount: 1, receive: Vegetable, receive_amount: 1 }`
  - `OnPlayGainOrExchangeAnimal { gain_good: Reed, gain_amount: 1, pay_good: Reed, pay_amount: 1, animal: Cattle }`

## Tests

Added `tests/ch02_b_onplay_simple.rs` with 23 tests covering:
- Registry reachability and cost/prerequisite parity with BGA specs
- Effect-only projection path (`apply_on_play_effects`)
- Dispatcher route (`dispatch_play`) for hand-dependent effects
- Edge cases: no empty cells, no occupations in hand, no grain for exchange, etc.
- Cost/prerequisite pins and spec parity verification

All tests pass. `cargo fmt` and `cargo clippy --all-targets --all-features --locked -- -D warnings` clean.

## Rule References

- B002: agricola.cloudfree.jp/?p=3255 (base 2016)
- B003: agricola.cloudfree.jp/?p=3254
- B005: agricola.cloudfree.jp/?p=3252
- B008: agricola.cloudfree.jp/?p=3249 (base 2016)
- B009: agricola.cloudfree.jp/?p=3248

All cross-checked against English list page. Cards are printed "pass left" (LeftNeighborHand).