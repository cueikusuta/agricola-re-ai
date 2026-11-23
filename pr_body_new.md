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
- Pass-left: `CardDefinition.post_play` / `RegisteredCard.post_play` (`PostPlayDestination::LeftNeighborHand`) with symmetric `#[serde(default, skip_serializing_if)]` attributes. `dispatch_play`'s post-play block (runs after every phase) adds the card's points, then moves the card through the canonical `card_zones::move_card` (Issue #266 single source of truth) and subtracts the points back — no underflow for non-zero-point pass-left cards. Pass-left is a no-op in 1-player games (`players.len() >= 2` guard), including B003's own pass branch, which now routes through the canonical `card_zones::hand_to_hand` transfer.

## Tests

Added `tests/ch02_b_onplay_simple.rs` (25 tests) covering:
- Registry reachability and cost/prerequisite parity with BGA specs
- Effect-only projection path (`apply_on_play_effects`)
- Dispatcher route (`dispatch_play`) for hand-dependent effects
- Edge cases: no empty cells, no occupations in hand, no grain for exchange, etc.
- Cost/prerequisite pins and spec parity verification
- Pass-left regression: zone transition via `move_card`, `card_points` add/subtract balance (0-point and non-zero-point cards), left-neighbour reachability of the drawn occupation, 1-player no-op
- Pass-left through the engine production path (`Space::MeetingPlace` → `Detail::PlayMinor` → `dispatch_play`), asserting zone arrival, `card_points`, and `Game::validate` state-consistency invariants

All tests pass. `cargo fmt --check` and `cargo clippy --all-targets --all-features --locked -- -D warnings` clean.

## Rule References

- B002: agricola.cloudfree.jp/?p=3255 (base 2016)
- B003: agricola.cloudfree.jp/?p=3254
- B005: agricola.cloudfree.jp/?p=3252
- B008: agricola.cloudfree.jp/?p=3249 (base 2016)
- B009: agricola.cloudfree.jp/?p=3248

All cross-checked against English list page. Cards are printed "pass left" (LeftNeighborHand).

## Ambiguous rules and interpretation (AGENTS.md §3 record)

The following official-wording ambiguities were resolved by interpretation; recorded here per §3 (no guessing beyond implementation):

1. **B002 adjacency when no pasture exists (spec deviation).** The BGA spec JSON carries `must_be_adjacent: true` with notes "must touch an existing pasture". When the owner has no pasture yet there is nothing to touch, so the implementation falls back to the first empty, unstabled cell in farm order (`handle_on_play_build_pasture`; pinned by `b002_normal_grants_free_pasture`). With an existing pasture the new cell is adjacent (`b002_adjacent_to_existing_pasture`). No legal cell: the effect does nothing and the play still succeeds (`b002_no_pasture_when_no_empty_cells`).
2. **B003 "2 food replaces the drawn occupation's own play cost".** The 2-food payment is charged instead of the drawn occupation's printed cost when it is played immediately (`handle_moonshine`, pay branch). The official wording leaves this ambiguous; recorded as our interpretation.
3. **B003 deterministic draw policy.** The spec leaves the draw random (`decision_owner=random`); for deterministic replay the engine draws the ID-sorted-first occupation still in hand (same policy as the A-deck's A003).
4. **B003/B009 deterministic choice policy.** B003 pays when food is affordable, otherwise passes (the play executor cannot represent a play-time pause, Issue #268 R2-P0-2). B009 exchanges whenever the payment resource is available (exchange yields the strictly larger reward) and gains otherwise.
5. **Deterministic placement of on-play gains.** Animal gains (B009 cattle, C083 vocabulary) and B002's pasture cell are placed by a derived plan (free capacity first, overflow discarded) rather than a play-time pause.

## Gates

Verification is GitHub Actions-first per AGENTS.md §18: the gate evidence is
the check-runs of the review HEAD (`0e75b83`), not local measurements. Run:
<https://github.com/cueikusuta/agricola-re-ai/actions/runs/45135469985>
(Rust CI: Detect changed files / rustfmt / clippy / cargo test / M1-M2-M7
seeded performance) plus
<https://github.com/cueikusuta/agricola-re-ai/actions/runs/45135469958>
(Pipeline Gates), <https://github.com/cueikusuta/agricola-re-ai/actions/runs/45135469937>
(Script test suites), <https://github.com/cueikusuta/agricola-re-ai/actions/runs/45135469926>
(Browser UI E2E browser-smoke) and
<https://github.com/cueikusuta/agricola-re-ai/actions/runs/45135469916>
(Paired AI Evaluation) — all `pass` on `0e75b83`.

Local pre-push runs (developer machine, rustc 1.99 and the CI toolchain
1.100.0): `cargo fmt --check` clean; `cargo clippy --all-targets
--all-features --locked -- -D warnings` clean on both toolchains (the
needless_range_loop finding in `b002_no_pasture_when_no_empty_cells` was fixed
for clippy 1.100); `cargo test --locked` → 965 tests, 0 failures.

## Merge with main

The branch was rebased forward by merging `origin/main` (102 commits ahead of the
merge base). Resolution highlights:

- `CardEffect` / `public_tag` / policy / registry match unions: both sides'
  variants kept (B-deck chapter 1 + main's A109/D065/A090/A151/A152/B097/B106/
  B135/C064/B114/A157/A049/C067/B073 chapters).
- `dispatch_play`: main's Phase 4b (`AfterImprovementPlayedGain`, A109) kept;
  this PR's card-points update and pass-left post-play block run after it.
- Main's `CardDefinition` literals received the mechanical `post_play` field.
- Conversion paths preserve the field instead of hardcoding `PlayedArea`:
  `CardSet::projected_for_players` (game entry projection) and
  `RegisteredCard::to_card_definition_for_players` now carry the source card's
  `post_play` — without this, every game-entry projection silently dropped the
  printed pass-left behaviour (caught by the new engine-path regression tests).
- Main's registry builder (`collect_cards` + `merge_card_variants`) is intact;
  `verified_b_onplay_simple_cards()` was added to its independent-card list and
  `merge_card_variants`/`add_card` carry `post_play` through.