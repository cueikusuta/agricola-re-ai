//! Validation layering for game state transitions (Issue #255 Phase B,
//! 設計判断 3 段階C: 増分/全件/リプレイの3層分離・契約文書化).
//!
//! Three layers with distinct contracts share one implementation of the
//! per-player invariants (`crate::model::validate_player_state`), so the
//! layers cannot drift:
//!
//! | # | Layer | Entry point | Guarantees | Tolerates | Fallback |
//! |---|-------|-------------|------------|-----------|----------|
//! | 1 | incremental | [`check_incremental`] (work moves), [`check_incremental_resume_completion`] (double-pause continuation), [`check_free_action`] (free actions) | `Pass` only when every `pre`→`post` difference is *proven legal* by the cheap checks — a `Pass` is never a state [`check_full`] rejects | says [`Verdict::Unsure`] instead of guessing for anything not explicitly verified (RNG stream, card ledgers, round transitions, start-player changes) | `Unsure` → the caller must run [`check_full`]; the `check_*_or_full` wrappers below do it automatically |
//! | 2 | full | [`check_full`] | the complete invariant set (thin wrapper over `Game::validate`, the single source of truth); the trust-boundary arbiter | structural state only — it does not prove replay or serialization integrity (layer 3) | none: it *is* the fallback; an `Err` rejects the state |
//! | 3 | replay | [`check_replay`] | layer 2 on the state, plus the recorded event log replays to the same state hash and `save_json`/`load_json` round-trips to the same state hash | self-consistency only — it cannot prove rules correctness or that the log was produced by this engine | none: any mismatch rejects the game at the boundary |
//!
//! Which layer a caller uses:
//!
//! - Search paths (MCTS/PUCT) call the layer-1 wrappers; the cost of a `Pass`
//!   is O(one move), and `Unsure` degrades to layer 2 without accepting
//!   anything invalid.
//! - Commit points, save/load, replay, external data ingestion and
//!   strict/debug tests call layer 2 or layer 3.
//!
//! Soundness contract (review P1-A): `Pass` is returned only when every
//! difference between `pre` and `post` has been *proven legal* by the checks
//! below. Any component the incremental layer does not explicitly verify is
//! either shown unchanged, or downgrades the verdict to `Unsure` so the full
//! validator arbitrates. A `Pass` is never reachable from a post state that
//! `check_full` rejects. This includes `post.cards`, `post.card_set`,
//! `post.delayed_well`, and `post.public.active_player` which are all fully
//! verified or downgraded to `Unsure`. The internal RNG state (`post.rng`) is
//! explicitly excluded from incremental checks and is only validated by
//! `check_full` (if at all) — documented here per review P1-A.

use crate::engine::expected_active_player;
use crate::model::{Game, Tile};

/// Result of incremental validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Incremental checks passed.
    Pass,
    /// Incremental checks found a definite violation.
    Fail(String),
    /// Incremental checks cannot decide; the caller must run full validation.
    Unsure,
}

// =====================================================================
// Layer 2 — full validation (the complete invariant set).
//
// Contract:
//   * Guarantees: every invariant `Game::validate` enforces, and nothing
//     less — it is the single source of truth the other layers arbitrate
//     against.
//   * Tolerates: structural state only. It does NOT prove replay
//     consistency or serialization integrity (layer 3's job).
//   * Fallback: none. It *is* the fallback target of layer 1; an `Err`
//     rejects the state at the trust boundary.
// =====================================================================

/// Full validation — the complete invariant set, identical to `Game::validate`.
///
/// Use at every trust boundary: save/load, replay, external data ingestion,
/// strict/debug tests, and as the arbiter for layer-1 `Unsure` verdicts.
pub fn check_full(game: &Game) -> Result<(), String> {
    game.validate()
}

// =====================================================================
// Layer 1 — incremental validation (fast per-action checks).
//
// Contract:
//   * Guarantees: `Pass` only when every pre→post difference is proven
//     legal by the checks below; a `Pass` is never a state layer 2
//     (`check_full`) rejects (soundness contract, review P1-A/2).
//   * Tolerates: `Verdict::Unsure` instead of guessing. The layer never
//     proves completeness — components it does not explicitly verify are
//     downgraded (`rng`, `cards`, `card_set`, `delayed_well`,
//     `public.active_player` rotation edges, round transitions, …).
//   * Fallback: `Unsure` → the caller MUST run `check_full`; the
//     `check_*_or_full` wrappers at the end of this section do it
//     automatically so no invalid state is ever accepted.
// =====================================================================

/// Fast incremental validation for one mid-round work move.
///
/// `pre` is the committed state before the action; `post` is the state the
/// action produced. Both must describe the same game (same config, player
/// count, round, revealed spaces and hidden deck). If `post` advanced to a
/// new round (last placement, harvest, round reveal), the cheap checks
/// cannot decide and return [`Verdict::Unsure`].
///
/// Soundness contract (review P1-2): `Pass` is returned only when every
/// difference between `pre` and `post` has been *proven legal* by the checks
/// below. Any component the incremental layer does not explicitly verify is
/// either shown unchanged, or downgrades the verdict to `Unsure` so the full
/// validator arbitrates. A `Pass` is never reachable from a post state that
/// `check_full` rejects.
pub fn check_incremental(post: &Game, pre: &Game) -> Verdict {
    check_incremental_impl(post, pre, PLAIN_MOVE)
}

/// Incremental validation for the completion segment of a resumed work
/// action (double-pause continuation, PR #352 Round 2 P0-2): the action's
/// space/worker bookkeeping was applied by the first segment of the same
/// logical move, so this segment adds no worker and no used space; every
/// other mid-round law is enforced exactly as for a plain move.
pub fn check_incremental_resume_completion(post: &Game, pre: &Game) -> Verdict {
    check_incremental_impl(post, pre, RESUME_COMPLETION)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeltaExpectation {
    worker_delta: i32,
    used_added: usize,
}

/// Delta expectations for a plain mid-round work move: the active player
/// places exactly one worker on exactly one previously unused space.
const PLAIN_MOVE: DeltaExpectation = DeltaExpectation {
    worker_delta: 1,
    used_added: 1,
};

/// Delta expectations for the completion segment of a resumed work action:
/// the space/worker bookkeeping was applied by the first segment of the same
/// logical move, so this segment adds no worker and no used space. Every
/// other mid-round law is unchanged.
const RESUME_COMPLETION: DeltaExpectation = DeltaExpectation {
    worker_delta: 0,
    used_added: 0,
};

fn check_incremental_impl(post: &Game, pre: &Game, expect: DeltaExpectation) -> Verdict {
    // 1. Identity of the game itself.
    if post.config != pre.config {
        return Verdict::Fail("config mutated".into());
    }
    if post.public.players.len() != pre.public.players.len() {
        return Verdict::Fail("player count changed".into());
    }
    if post.public.additional_action_space_enabled != pre.public.additional_action_space_enabled {
        return Verdict::Fail("setting mutated".into());
    }
    // 2. Mid-round only: a new round touches quota/deck/revealed everywhere.
    if post.public.round != pre.public.round || post.public.finished != pre.public.finished {
        return Verdict::Unsure;
    }
    // 3. Revealed spaces and hidden deck are frozen mid-round.
    if post.public.revealed != pre.public.revealed {
        return Verdict::Fail("revealed changed mid-round".into());
    }
    if post.hidden_round_deck != pre.hidden_round_deck {
        return Verdict::Fail("hidden_round_deck changed mid-round".into());
    }
    // 4. Round-start state that no work move may change. The MeetingPlace
    //    placement legitimately moves the start player, so treat a changed
    //    start player as "cannot decide" rather than a definite violation.
    if post.public.start_player != pre.public.start_player {
        return Verdict::Unsure;
    }
    if post.public.work_quota != pre.public.work_quota {
        return Verdict::Fail("work_quota changed mid-round".into());
    }
    // 5. Exactly one worker placed by the pre-action active player.
    let n = pre.public.players.len();
    if pre.public.active_player >= n {
        return Verdict::Unsure;
    }
    let actor = pre.public.active_player;
    for i in 0..n {
        let delta =
            i32::from(post.public.workers_placed[i]) - i32::from(pre.public.workers_placed[i]);
        if i == actor {
            if delta != expect.worker_delta {
                return Verdict::Fail("active worker placement delta mismatch".into());
            }
        } else if delta != 0 {
            return Verdict::Fail("non-actor worker count changed".into());
        }
    }
    // 5b. Verify post.active_player matches the expected next worker.
    // Round advancement is already excluded above (round == pre.round), so all
    // players should not have placed their full quota yet. If they have, the
    // round would advance and we would have returned Unsure.
    let expected = expected_active_player(
        pre.public.active_player,
        &post.public.workers_placed,
        &post.public.work_quota,
    );
    if expected >= n {
        // All workers placed but round hasn't advanced — engine invariant broken.
        return Verdict::Fail("all workers placed but round not advanced".into());
    }
    if post.public.active_player != expected {
        return Verdict::Fail("active_player does not match expected next worker".into());
    }
    // 6. Exactly one newly used space; `used` may only grow mid-round. A
    //    previously used space disappearing is a definite violation (the
    //    full validator does not check monotonicity — this catches a class
    //    it would otherwise miss, e.g. tampering that shrinks `used`).
    let removed_used: Vec<_> = pre.public.used.difference(&post.public.used).collect();
    if !removed_used.is_empty() {
        return Verdict::Fail("a used space disappeared mid-round".into());
    }
    let added: Vec<_> = post.public.used.difference(&pre.public.used).collect();
    if added.len() != expect.used_added {
        return Verdict::Fail(if expect.used_added == 1 {
            "a work move must add exactly one used space".into()
        } else {
            "a resumed completion must not add a used space".into()
        });
    }
    if expect.used_added == 1 {
        let added_space = added[0];
        // 6b. The added space must be available (same logic as full validation).
        // Pre is assumed valid, so we only need to check the newly added space.
        let players = pre.config.players;
        let additional_enabled = pre.public.additional_action_space_enabled;
        let card_set_enabled = pre.card_set.enabled();
        // 6b-i. Disabled additional space.
        if !additional_enabled && added_space.additional_for(players) {
            return Verdict::Fail("disabled additional space used".into());
        }
        // 6b-ii. Multiple mutually-exclusive additional spaces.
        // Count additional spaces in post.used; if > 1, reject.
        let additional_count = post
            .public
            .used
            .iter()
            .filter(|s| s.additional_for(players))
            .count();
        if additional_count > 1 {
            return Verdict::Fail("multiple mutually-exclusive additional spaces used".into());
        }
        // 6b-iii. Space not in available set.
        let available = pre
            .public
            .available_spaces(players, additional_enabled, card_set_enabled);
        if !available.contains(added_space) {
            return Verdict::Fail("a non-revealed action space was used".into());
        }
    }
    // 7. Major improvements: the union across market + player tables must keep
    //    every physical major improvement in exactly one location (the same
    //    invariant `check_full` enforces). A work move may move an item
    //    from market to the actor's table, or return an exchanged Fireplace
    //    to the market — but never duplicate or lose an item. Comparing the
    //    per-item multisets is O(market+tables), far cheaper than full
    //    validation, and shares the full validator's semantics.
    {
        let count_items = |market: &[crate::model::Improvement],
                           players: &[crate::model::Player]|
         -> std::collections::BTreeMap<crate::model::Improvement, u32> {
            let mut counts = std::collections::BTreeMap::new();
            for item in market
                .iter()
                .copied()
                .chain(players.iter().flat_map(|p| p.improvements.iter().copied()))
            {
                *counts.entry(item).or_insert(0u32) += 1;
            }
            counts
        };
        let post_counts = count_items(&post.public.improvement_market, &post.public.players);
        let pre_counts = count_items(&pre.public.improvement_market, &pre.public.players);
        if post_counts != pre_counts {
            return Verdict::Fail("major improvement duplicated or missing".into());
        }
        // The market may only contain items that were already in the
        // pre-market, or a Fireplace returned via a hearth exchange from a
        // pre-round table. A market is not provably legal mid-round.
        let pre_tables: std::collections::BTreeSet<_> = pre
            .public
            .players
            .iter()
            .flat_map(|p| p.improvements.iter().copied())
            .collect();
        for item in &post.public.improvement_market {
            let in_pre_market = pre.public.improvement_market.contains(item);
            let exchangeable = matches!(
                item,
                crate::model::Improvement::Fireplace2 | crate::model::Improvement::Fireplace3
            ) && pre_tables.contains(item);
            if !in_pre_market && !exchangeable {
                return Verdict::Fail("improvement appeared in market from nowhere".into());
            }
        }
    }
    // 8. Farm invariants. Non-actors cannot change in a work move: verify the
    //    cheap structural bounds and decline (Unsure) if anything looks different.
    //    The actor gets the full shared per-player check.
    //    Non-actor full equality (Player::PartialEq) covers id, pastures, pet,
    //    and all effect-counter fields — any drift downgrades to Unsure→full.
    for (i, player) in post.public.players.iter().enumerate() {
        if i != actor {
            if player.farm.len() != crate::model::FARM_CELLS
                || player.occupied_stables() > 4
                || player.fences.len() > 15
                || player.family < 2
                || player.family > 5
                || player.newborns > player.family
            {
                return Verdict::Unsure;
            }
            // Full structural equality for non-actors: any difference (including
            // id, pastures, pet, and all effect-counter fields) is Unsure.
            // The cheap bounds above remain as definite Fail guards matching
            // check_full's hard errors.
            if player != &pre.public.players[i] {
                return Verdict::Unsure;
            }
            continue;
        }
        if crate::model::validate_player_state(
            &post.public.work_quota,
            &post.public.workers_placed,
            i,
            player,
        )
        .is_err()
        {
            return Verdict::Fail("actor farm invariants violated".into());
        }
    }
    // 9. Tile-kind aggregate for the actor: a placement can add tiles but
    //    never demolish them mid-round (rooms/fields/pastures only grow).
    let count_tiles = |p: &crate::model::Player| {
        let mut rooms = 0usize;
        let mut fields = 0usize;
        let mut pastures = 0usize;
        for cell in &p.farm {
            match cell.tile {
                Tile::Room => rooms += 1,
                Tile::Field { .. } => fields += 1,
                Tile::Pasture { .. } => pastures += 1,
                Tile::Empty => {}
            }
        }
        (rooms, fields, pastures)
    };
    let (pre_rooms, pre_fields, pre_pastures) = count_tiles(&pre.public.players[actor]);
    let (post_rooms, post_fields, post_pastures) = count_tiles(&post.public.players[actor]);
    if post_rooms < pre_rooms || post_fields < pre_fields || post_pastures < pre_pastures {
        return Verdict::Fail("actor demolished tiles in a work move".into());
    }
    // 11. `accumulations` may only decrease (the placed space pays out);
    //     unseen growth or unknown keys decline to the full validator.
    for (space, post_n) in &post.public.accumulations {
        match pre.public.accumulations.get(space) {
            Some(pre_n) if post_n <= pre_n => {}
            _ => return Verdict::Unsure,
        }
    }
    if post.public.accumulations.len() > pre.public.accumulations.len() {
        return Verdict::Unsure;
    }
    // 11b. `card_accumulations`: card effects may legally modify per-card
    //      stored resources mid-round, and the cheap checks cannot prove the
    //      delta sound (which card, which resource, which amount). Any change
    //      therefore downgrades to Unsure so the full validator arbitrates —
    //      mirroring the soundness contract applied to other bookkeeping
    //      ledgers (PR #327's bonus_points pattern).
    if post.public.card_accumulations != pre.public.card_accumulations {
        return Verdict::Unsure;
    }
    // 12. post.cards: mid-round checks.
    //     - hands.len() must equal player count (full validator rejects otherwise).
    if post.cards.hands.len() != n {
        return Verdict::Fail("card hand count mismatch".into());
    }
    //     - card_set disabled => hands must be empty (full validator enforces).
    if !post.card_set.enabled() && post.cards != pre.cards {
        return Verdict::Fail("cards changed while card_set disabled".into());
    }
    //     - non-actor hand differences -> Unsure (passive card effects possible).
    //     - undealt differences -> Unsure.
    //     - actor hand: multiset post ⊆ pre (cards only removed via play).
    //       Removed multiset must equal played_cards multiset addition.
    {
        let pre_actor_hand: std::collections::BTreeMap<_, _> =
            pre.cards.hands[actor]
                .iter()
                .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                    *acc.entry(id.clone()).or_insert(0usize) += 1;
                    acc
                });
        let post_actor_hand: std::collections::BTreeMap<_, _> = post.cards.hands[actor]
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                *acc.entry(id.clone()).or_insert(0usize) += 1;
                acc
            });
        // Check post ⊆ pre for actor hand.
        for (id, count) in &post_actor_hand {
            let pre_count = pre_actor_hand.get(id).copied().unwrap_or(0);
            if *count > pre_count {
                return Verdict::Unsure;
            }
        }
        // Check removed cards match played_cards additions.
        let mut removed = std::collections::BTreeMap::new();
        for (id, pre_count) in &pre_actor_hand {
            let post_count = post_actor_hand.get(id).copied().unwrap_or(0);
            if *pre_count > post_count {
                removed.insert(id.clone(), pre_count - post_count);
            }
        }
        // Compute played_added as exact per-ID multiset count delta (post - pre where positive).
        // This catches duplicate pushes of already-played IDs which the old membership check missed.
        let pre_played: std::collections::BTreeMap<_, _> = pre.public.players[actor]
            .played_cards
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                *acc.entry(id.clone()).or_insert(0usize) += 1;
                acc
            });
        let post_played: std::collections::BTreeMap<_, _> = post.public.players[actor]
            .played_cards
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                *acc.entry(id.clone()).or_insert(0usize) += 1;
                acc
            });
        let mut played_added = std::collections::BTreeMap::new();
        for (id, post_count) in &post_played {
            let pre_count = pre_played.get(id).copied().unwrap_or(0);
            if *post_count > pre_count {
                played_added.insert(id.clone(), post_count - pre_count);
            }
        }
        // Explicit duplicate guard: any ID occurring more than once in post.played_cards is tampering.
        // Real engine cannot produce duplicates (cards::play removes from hand by value, then pushes once),
        // and full validation rejects duplicate IDs globally (Err "card missing, duplicate or unknown").
        for count in post_played.values() {
            if *count > 1 {
                return Verdict::Fail("actor played_cards contains duplicate card ID".into());
            }
        }
        if removed != played_added {
            return Verdict::Unsure;
        }
        // Non-actor hands: any difference -> Unsure.
        for i in 0..n {
            if i == actor {
                continue;
            }
            if post.cards.hands[i] != pre.cards.hands[i] {
                return Verdict::Unsure;
            }
        }
        // Undealt: any difference -> Unsure.
        if post.cards.undealt != pre.cards.undealt {
            return Verdict::Unsure;
        }
    }
    // 12b. Actor card consistency: card_points delta must equal points of newly played cards.
    // Free actions don't play cards (engine.rs execute_free has no cards::play calls), so this
    // applies only to work moves. If played_cards changes, the points delta must match exactly.
    {
        let pre_actor = &pre.public.players[actor];
        let post_actor = &post.public.players[actor];
        let played_delta: i32 = post_actor.card_points - pre_actor.card_points;
        // Compute multiset difference of played_cards.
        let pre_played: std::collections::BTreeMap<_, _> =
            pre_actor
                .played_cards
                .iter()
                .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                    *acc.entry(id.clone()).or_insert(0usize) += 1;
                    acc
                });
        let post_played: std::collections::BTreeMap<_, _> = post_actor.played_cards.iter().fold(
            std::collections::BTreeMap::new(),
            |mut acc, id| {
                *acc.entry(id.clone()).or_insert(0usize) += 1;
                acc
            },
        );
        // Cards cannot be removed from played_cards in a work move (engine has no such path).
        for (id, pre_count) in &pre_played {
            let post_count = post_played.get(id).copied().unwrap_or(0);
            if *pre_count > post_count {
                return Verdict::Fail("actor played_cards lost cards in work move".into());
            }
        }
        // Newly added played cards.
        let mut played_added = std::collections::BTreeMap::new();
        for (id, post_count) in &post_played {
            let pre_count = pre_played.get(id).copied().unwrap_or(0);
            if *post_count > pre_count {
                played_added.insert(id.clone(), post_count - pre_count);
            }
        }
        if !played_added.is_empty() {
            // Sum points of newly played cards.
            let set = &post.card_set;
            let mut expected_points_delta = 0i32;
            for (id, count) in &played_added {
                if let Some(card) = set.definition(id) {
                    expected_points_delta =
                        match expected_points_delta.checked_add(card.points * (*count as i32)) {
                            Some(v) => v,
                            None => {
                                return Verdict::Fail("card points delta overflow".into());
                            }
                        };
                } else {
                    // Unknown card ID in played_cards — full validator will reject.
                    return Verdict::Fail("actor played_cards contains unknown card ID".into());
                }
            }
            // card_set disabled => no cards can be played (hands empty, played_cards empty).
            if !set.enabled() {
                if played_delta != 0 || !played_added.is_empty() {
                    return Verdict::Fail(
                        "card_points/played_cards changed while card_set disabled".into(),
                    );
                }
            } else if played_delta != expected_points_delta {
                return Verdict::Fail(
                    "actor card_points inconsistent with played_cards delta".into(),
                );
            }
        } else if played_delta != 0 {
            // No played_cards change but card_points changed — invalid.
            return Verdict::Fail("actor card_points changed without played_cards delta".into());
        }
    }
    //     Engine has no path that swaps card_set mid-game (grep-confirmed).
    //     Different valid card_set -> Unsure (full accepts, incremental cannot prove).
    if !std::sync::Arc::ptr_eq(&post.card_set, &pre.card_set) {
        return Verdict::Unsure;
    }
    // 14. post.delayed_well: Well purchase adds entries; engine path is complex.
    //     Downgrade to Unsure→full (soundness: Fail only for proven violations).
    if post.delayed_well != pre.delayed_well {
        return Verdict::Unsure;
    }
    Verdict::Pass
}

/// Incremental validation with automatic fallback to full validation.
pub fn check_incremental_or_full(post: &Game, pre: &Game) -> Result<(), String> {
    match check_incremental(post, pre) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => check_full(post),
    }
}

/// Incremental validation for the completion segment of a resumed work
/// action (double-pause continuation) with automatic fallback to full
/// validation. The segment adds no worker and no used space; every other
/// mid-round law is enforced by [`check_incremental_resume_completion`].
pub fn check_incremental_resume_completion_or_full(post: &Game, pre: &Game) -> Result<(), String> {
    match check_incremental_resume_completion(post, pre) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => check_full(post),
    }
}

/// Fast incremental validation for one free action (not a work move).
///
/// `pre` is the committed state before the action; `post` is the state the
/// action produced. Both must describe the same game (same config, player
/// count, round, revealed spaces and hidden deck).
///
/// `actor` is the player who performed the free action. This may differ from
/// `pre.public.active_player` because free actions can be taken out of turn.
///
/// Free actions do NOT place a worker or add a used space. They can change:
/// - Actor's supply (conversions, discards, cooking, baking)
/// - Actor's farm (fence building, plowing, sowing - but these are work moves usually)
/// - Actor's played_cards, improvements (for card effects)
/// - Actor's animals/pastures (rearrangement during breeding)
/// - Accumulations (taking from them)
///
/// Free actions must NOT change:
/// - Non-actor state (supply, cards, farm, fences, house, family, newborns, improvements)
/// - Round, revealed, hidden_round_deck, work_quota, start_player (except meeting place but that's a work move)
/// - Used spaces, workers_placed
/// - Major improvement multiset
/// - Accumulations can only decrease
pub fn check_free_action(post: &Game, pre: &Game, actor: usize) -> Verdict {
    // 1. Identity of the game itself.
    if post.config != pre.config {
        return Verdict::Fail("config mutated".into());
    }
    let n = pre.public.players.len();
    if post.public.players.len() != n {
        return Verdict::Fail("player count changed".into());
    }
    if actor >= n {
        return Verdict::Fail("actor index out of bounds".into());
    }
    if post.public.additional_action_space_enabled != pre.public.additional_action_space_enabled {
        return Verdict::Fail("setting mutated".into());
    }
    // 2. Free actions don't advance round.
    if post.public.round != pre.public.round || post.public.finished != pre.public.finished {
        return Verdict::Unsure;
    }
    // 3. Revealed spaces and hidden deck are frozen.
    if post.public.revealed != pre.public.revealed {
        return Verdict::Fail("revealed changed mid-round".into());
    }
    if post.hidden_round_deck != pre.hidden_round_deck {
        return Verdict::Fail("hidden_round_deck changed mid-round".into());
    }
    // 4. Start player and work quota unchanged.
    if post.public.start_player != pre.public.start_player {
        return Verdict::Unsure;
    }
    if post.public.work_quota != pre.public.work_quota {
        return Verdict::Fail("work_quota changed mid-round".into());
    }
    // 5. Workers placed MUST NOT change (free actions don't place workers).
    for i in 0..n {
        if post.public.workers_placed[i] != pre.public.workers_placed[i] {
            return Verdict::Fail("workers_placed changed in free action".into());
        }
    }
    // 6. Used spaces MUST NOT change (free actions don't use spaces).
    if post.public.used != pre.public.used {
        return Verdict::Fail("used changed in free action".into());
    }
    // 7. Major improvements: the union across market + player tables must keep
    //    every physical major improvement in exactly one location.
    {
        let count_items = |market: &[crate::model::Improvement],
                           players: &[crate::model::Player]|
         -> std::collections::BTreeMap<crate::model::Improvement, u32> {
            let mut counts = std::collections::BTreeMap::new();
            for item in market
                .iter()
                .copied()
                .chain(players.iter().flat_map(|p| p.improvements.iter().copied()))
            {
                *counts.entry(item).or_insert(0u32) += 1;
            }
            counts
        };
        let post_counts = count_items(&post.public.improvement_market, &post.public.players);
        let pre_counts = count_items(&pre.public.improvement_market, &pre.public.players);
        if post_counts != pre_counts {
            return Verdict::Fail("major improvement duplicated or missing".into());
        }
        let pre_tables: std::collections::BTreeSet<_> = pre
            .public
            .players
            .iter()
            .flat_map(|p| p.improvements.iter().copied())
            .collect();
        for item in &post.public.improvement_market {
            let in_pre_market = pre.public.improvement_market.contains(item);
            let exchangeable = matches!(
                item,
                crate::model::Improvement::Fireplace2 | crate::model::Improvement::Fireplace3
            ) && pre_tables.contains(item);
            if !in_pre_market && !exchangeable {
                return Verdict::Fail("improvement appeared in market from nowhere".into());
            }
        }
    }
    // 8. Non-actors cannot change in a free action: verify the
    //    cheap structural bounds and decline (Unsure) if anything looks different.
    //    The actor gets full shared per-player check for farm invariants.
    //    Non-actor full equality (Player::PartialEq) covers id, pastures, pet,
    //    and all effect-counter fields — any drift downgrades to Unsure→full.
    for (i, player) in post.public.players.iter().enumerate() {
        if i != actor {
            if player.farm.len() != crate::model::FARM_CELLS
                || player.occupied_stables() > 4
                || player.fences.len() > 15
                || player.family < 2
                || player.family > 5
                || player.newborns > player.family
            {
                return Verdict::Unsure;
            }
            // Full structural equality for non-actors: any difference (including
            // id, pastures, pet, and all effect-counter fields) is Unsure.
            // The cheap bounds above remain as definite Fail guards matching
            // check_full's hard errors.
            if player != &pre.public.players[i] {
                return Verdict::Unsure;
            }
            continue;
        }
        // Actor: validate farm invariants (shared with full validator)
        if crate::model::validate_player_state(
            &post.public.work_quota,
            &post.public.workers_placed,
            i,
            player,
        )
        .is_err()
        {
            return Verdict::Fail("actor farm invariants violated".into());
        }
    }
    // 9. Tile-kind aggregate for the actor: free actions can add tiles (plow/sow)
    //    but never demolish them.
    let count_tiles = |p: &crate::model::Player| {
        let mut rooms = 0usize;
        let mut fields = 0usize;
        let mut pastures = 0usize;
        for cell in &p.farm {
            match cell.tile {
                crate::model::Tile::Room => rooms += 1,
                crate::model::Tile::Field { .. } => fields += 1,
                crate::model::Tile::Pasture { .. } => pastures += 1,
                crate::model::Tile::Empty => {}
            }
        }
        (rooms, fields, pastures)
    };
    let (pre_rooms, pre_fields, pre_pastures) = count_tiles(&pre.public.players[actor]);
    let (post_rooms, post_fields, post_pastures) = count_tiles(&post.public.players[actor]);
    if post_rooms < pre_rooms || post_fields < pre_fields || post_pastures < pre_pastures {
        return Verdict::Fail("actor demolished tiles in a free action".into());
    }
    // 10. `accumulations` may only decrease (the free action takes from them);
    //     unseen growth or unknown keys decline to the full validator.
    for (space, post_n) in &post.public.accumulations {
        match pre.public.accumulations.get(space) {
            Some(pre_n) if post_n <= pre_n => {}
            _ => return Verdict::Unsure,
        }
    }
    if post.public.accumulations.len() > pre.public.accumulations.len() {
        return Verdict::Unsure;
    }
    // 11. Animal accumulations same rule.
    for (space, post_n) in &post.public.animal_accumulations {
        match pre.public.animal_accumulations.get(space) {
            Some(pre_n) if post_n <= pre_n => {}
            _ => return Verdict::Unsure,
        }
    }
    if post.public.animal_accumulations.len() > pre.public.animal_accumulations.len() {
        return Verdict::Unsure;
    }
    // 11b. `card_accumulations` must not change in a free action: the free
    //      executor has no card-effect dispatch (same certainty as
    //      `card_uses`/`bonus_points` in PR #327's free-action contract),
    //      so any difference is a definite violation.
    if post.public.card_accumulations != pre.public.card_accumulations {
        return Verdict::Fail("card_accumulations changed in free action".into());
    }
    // 12. active_player must not change in a free action.
    if post.public.active_player != pre.public.active_player {
        return Verdict::Fail("active_player changed in free action".into());
    }
    // 13. post.cards: free action checks.
    if post.cards.hands.len() != n {
        return Verdict::Fail("card hand count mismatch".into());
    }
    if !post.card_set.enabled() && post.cards != pre.cards {
        return Verdict::Fail("cards changed while card_set disabled".into());
    }
    {
        let pre_actor_hand: std::collections::BTreeMap<_, _> =
            pre.cards.hands[actor]
                .iter()
                .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                    *acc.entry(id.clone()).or_insert(0usize) += 1;
                    acc
                });
        let post_actor_hand: std::collections::BTreeMap<_, _> = post.cards.hands[actor]
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut acc, id| {
                *acc.entry(id.clone()).or_insert(0usize) += 1;
                acc
            });
        // Check post ⊆ pre for actor hand (free actions don't play cards from hand).
        for (id, count) in &post_actor_hand {
            let pre_count = pre_actor_hand.get(id).copied().unwrap_or(0);
            if *count > pre_count {
                return Verdict::Unsure;
            }
        }
        // Free actions don't add to played_cards, so hand should be unchanged or have
        // cards removed only by card effects (which we can't fully verify -> Unsure).
        let mut removed = std::collections::BTreeMap::new();
        for (id, pre_count) in &pre_actor_hand {
            let post_count = post_actor_hand.get(id).copied().unwrap_or(0);
            if *pre_count > post_count {
                removed.insert(id.clone(), pre_count - post_count);
            }
        }
        // Review P1-A/2 counterexample: if actor hand changed in free action, that's a
        // definite violation (execute_free never touches hands). Fail instead of Unsure.
        if !removed.is_empty() {
            return Verdict::Fail("actor hand changed in free action".into());
        }
        // Actor played_cards must not change in free action (no cards::play calls in execute_free).
        // Use exact Vec equality (sequence + multiplicity) — any change including duplicate pushes is a violation.
        if post.public.players[actor].played_cards != pre.public.players[actor].played_cards {
            return Verdict::Fail("actor played_cards changed in free action".into());
        }
        // Actor card_points must not change in free action.
        if pre.public.players[actor].card_points != post.public.players[actor].card_points {
            return Verdict::Fail("actor card_points changed in free action".into());
        }
        // Non-actor hands: any difference -> Unsure.
        for i in 0..n {
            if i == actor {
                continue;
            }
            if post.cards.hands[i] != pre.cards.hands[i] {
                return Verdict::Unsure;
            }
        }
        // Undealt: any difference -> Unsure.
        if post.cards.undealt != pre.cards.undealt {
            return Verdict::Unsure;
        }
    }
    // 14. post.card_set: Arc pointer equality required.
    if !std::sync::Arc::ptr_eq(&post.card_set, &pre.card_set) {
        return Verdict::Unsure;
    }
    // 15. post.delayed_well: Well purchase not possible in free action, but downgrade.
    if post.delayed_well != pre.delayed_well {
        return Verdict::Unsure;
    }
    Verdict::Pass
}

/// Incremental free-action validation with automatic fallback to full validation.
pub fn check_free_action_or_full(post: &Game, pre: &Game, actor: usize) -> Result<(), String> {
    match check_free_action(post, pre, actor) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => check_full(post),
    }
}

// =====================================================================
// Layer 3 — replay validation (save/load/replay round-trip consistency).
//
// Contract:
//   * Guarantees: (a) the state satisfies layer 2 (`check_full`);
//     (b) the recorded event log, replayed from the same initial setup
//     (config, card set, drafted hands, first start player), reproduces
//     the exact `state_hash`; (c) `save_json` -> `load_json` round-trips
//     to the same `state_hash` — and `load_json` itself re-verifies the
//     complete event chain, the stamped integrity hash and the version
//     metadata, so the serialized form cannot smuggle in a state the
//     log does not support.
//   * Tolerates: self-consistency only. It does NOT prove rules
//     correctness, nor that the log was produced by this engine — a
//     faithfully recorded illegal game would still fail layer 2's
//     invariants or the engine's per-action validation during replay,
//     but layer 3 adds no rule judgement of its own.
//   * Applies only to committed games with a complete, hash-checked
//     event log. Hypothetical search positions (`record_events=false`)
//     log nothing, so their replay trivially mismatches — never call
//     this layer on them.
//   * Fallback: none. Any mismatch rejects the game at the boundary.
// =====================================================================

/// Replay validation — save/load/replay round-trip consistency.
///
/// The single entry point for trust boundaries where a game crosses a
/// serialization boundary: restoring a save, replaying a recorded log,
/// ingesting external game data. It runs all three gates in order and
/// fails on the first mismatch with a message naming the gate:
///
/// 1. [`check_full`] — the complete invariant set on the state itself.
/// 2. `Game::replay_with_setup_and_start_player_inner` on the recorded
///    log, starting from the same initial setup the game recorded
///    (`log.initial_hands` / `log.initial_start_player_override`), with
///    pause-at-end handling exactly as `Game::load_json` uses it — so a
///    game paused mid-action replays to the paused state, not past it.
/// 3. `save_json` -> `load_json`, which additionally re-verifies the
///    full event chain and the stamped integrity hash.
///
/// Gates 2 and 3 compare `state_hash` values: equality means the log
/// and the serialized form describe exactly this state, including
/// hidden state covered by the hash (`rng`, `delayed_well`, `pending`).
pub fn check_replay(game: &Game) -> Result<(), String> {
    // Gate 1: the state itself must satisfy the complete invariant set.
    check_full(game)?;
    // Gate 2: replaying the recorded event log must reproduce the state.
    let replayed = Game::replay_with_setup_and_start_player_inner(
        game.config.clone(),
        (*game.card_set).clone(),
        game.log.initial_hands.clone(),
        game.log.initial_start_player_override,
        &game.log.events,
        game.pending.is_some(),
    )
    .map_err(|error| format!("replay failed: {error}"))?;
    if replayed.state_hash() != game.state_hash() {
        return Err("replayed state hash differs from the recorded state".into());
    }
    // Gate 3: the JSON save must restore to the same state hash.
    let saved = game
        .save_json()
        .map_err(|error| format!("save_json failed: {error}"))?;
    let restored = Game::load_json(&saved).map_err(|error| format!("load_json failed: {error}"))?;
    if restored.state_hash() != game.state_hash() {
        return Err("restored state hash differs from the saved state".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Action, Detail, FreeAction};
    use crate::model::{Config, Game, Space};
    use crate::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn cardless(players: usize, seed: u64) -> Game {
        Game::new(Config {
            players,
            seed,
            additional_action_space_enabled: false,
        })
    }

    /// Drive a cardless game with a fixed-seed random policy and produce
    /// (pre, post) pairs around every applied work move.
    fn transitions(players: usize, seed: u64, moves: usize) -> Vec<(Game, Game)> {
        let mut game = cardless(players, seed);
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xBEEF);
        let mut result = Vec::new();
        for _ in 0..moves {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("random policy must find a legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("detail producer must supply a legal action");
            let pre = game.clone();
            game.apply(action).expect("random policy action must apply");
            result.push((pre, game.clone()));
        }
        result
    }

    #[test]
    fn mid_round_moves_pass_incremental_and_match_full() {
        for players in [2usize, 3, 4] {
            let pairs = transitions(players, 7, 24);
            let mid_round: Vec<_> = pairs
                .iter()
                .filter(|(pre, post)| {
                    post.public.round == pre.public.round
                        && post.public.start_player == pre.public.start_player
                })
                .collect();
            assert!(
                !mid_round.is_empty(),
                "no plain mid-round moves sampled for {players} players"
            );
            for (pre, post) in mid_round {
                assert_eq!(
                    check_incremental(post, pre),
                    Verdict::Pass,
                    "mid-round move must pass incremental validation"
                );
                check_incremental_or_full(post, pre).unwrap();
                assert_eq!(check_full(post), Ok(()));
            }
            // MeetingPlace moves legitimately change the start player; they
            // must still be accepted, via the fallback path.
            let start_moves: Vec<_> = pairs
                .iter()
                .filter(|(pre, post)| {
                    post.public.round == pre.public.round
                        && post.public.start_player != pre.public.start_player
                })
                .collect();
            for (pre, post) in start_moves {
                assert_eq!(check_incremental(post, pre), Verdict::Unsure);
                check_incremental_or_full(post, pre).unwrap();
            }
        }
    }

    #[test]
    fn round_transitions_decline_to_full() {
        let mut game = cardless(2, 19);
        let before = game.clone();
        let mut rng = ChaCha8Rng::seed_from_u64(31);
        for _ in 0..4 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        assert_eq!(game.public.round, 2);
        assert_eq!(check_incremental(&game, &before), Verdict::Unsure);
        // Fallback path accepts it via the full validator.
        check_incremental_or_full(&game, &before).unwrap();
    }

    #[test]
    fn tampered_actor_farm_is_never_accepted() {
        let (pre, mut post) = transitions(2, 23, 6)
            .into_iter()
            .next()
            .expect("at least one transition");
        // Corrupt the actor's farm: a stable on a room tile.
        let actor = pre.public.active_player;
        post.public.players[actor].farm[0].stable = true;
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "corrupted farm must not pass"
        );
        if let Verdict::Unsure = verdict {
            assert!(check_full(&post).is_err());
        }
    }

    #[test]
    fn missing_used_space_is_rejected() {
        let (pre, mut post) = transitions(2, 29, 6)
            .into_iter()
            .next()
            .expect("at least one transition");
        // Roll back the used-set change: worker counts then disagree.
        let space = *post
            .public
            .used
            .difference(&pre.public.used)
            .next()
            .expect("one new space");
        post.public.used.remove(&space);
        assert!(matches!(check_incremental(&post, &pre), Verdict::Fail(_)));
    }

    #[test]
    fn full_validation_wraps_engine_validate() {
        let game = cardless(3, 37);
        assert_eq!(check_full(&game), Ok(()));
        let mut broken = game.clone();
        broken.public.round = 99;
        assert!(check_full(&broken).is_err());
    }

    #[test]
    fn search_clone_is_lightweight_and_independent() {
        let mut game = cardless(4, 41);
        let mut rng = ChaCha8Rng::seed_from_u64(41);
        for _ in 0..8 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let original_hash = game.state_hash();
        let original_events = game.log.events.len();
        let probe = game.search_clone();
        // Lightweight: no events, hash preserved, record disabled.
        assert_eq!(probe.state_hash(), original_hash);
        assert_eq!(probe.log.events.len(), 0);
        assert!(!probe.record_events);
        // Independence: a failed application must not touch the original.
        let mut failing = probe.clone();
        let bad = Action {
            space: Space::Forest,
            detail: Detail::Fence {
                pastures: vec![vec![0, 5]],
            },
        };
        assert!(failing.apply(bad).is_err());
        assert_eq!(game.state_hash(), original_hash);
        assert_eq!(game.log.events.len(), original_events);
        // Card definitions are shared, mutating a clone never writes through.
        assert!(std::sync::Arc::ptr_eq(&probe.card_set, &game.card_set));
    }

    #[test]
    fn improvement_market_and_cards_decline_to_full() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 43,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let pre = game.clone();
        let mut post = game.clone();
        let id = post
            .public
            .improvement_market
            .pop()
            .expect("non-empty market");
        post.public.players[0].improvements.push(id);
        // A purchase outside the engine's work-move shape: the incremental
        // layer must decline (Fail/Unsure), never guess.
        let verdict = check_incremental(&post, &pre);
        assert!(matches!(verdict, Verdict::Fail(_) | Verdict::Unsure));
        // Restore the market and re-check: legal state again.
        let mut restored = post;
        let imp = restored.public.players[0].improvements.pop().unwrap();
        restored.public.improvement_market.push(imp);
        restored.public.improvement_market.sort();
        assert_eq!(check_full(&restored), Ok(()));
    }

    /// Review P1-2 regression: a major improvement outright REMOVED (the
    /// counter-example from the review: `improvement_market.pop()` without
    /// re-adding it anywhere) must never reach `Pass`, because
    /// `check_full` rejects it ("major improvement duplicated or missing").
    /// The per-item multiset check makes this a definite `Fail`.
    #[test]
    fn removed_major_improvement_is_never_pass() {
        let (pre, mut post) = transitions(2, 47, 6)
            .into_iter()
            .next()
            .expect("at least one transition");
        assert!(
            !post.public.improvement_market.is_empty(),
            "setup: market must be non-empty"
        );
        post.public.improvement_market.pop();
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "vanished major improvement must not Pass (got {verdict:?})"
        );
        assert!(check_full(&post).is_err());
        // And the fallback wrapper must reject it too.
        assert!(check_incremental_or_full(&post, &pre).is_err());
    }

    /// Review P1-2 regression: removing an already-used space from `used`
    /// (monotonicity is a hard mid-round law; the full validator's
    /// available-space check does not cover every removal).
    #[test]
    fn removed_used_space_is_a_definite_fail() {
        let pairs = transitions(3, 53, 8);
        // Use a transition where pre already has used spaces (not the first move).
        let (pre, mut post) = pairs
            .into_iter()
            .nth(2)
            .expect("need at least 3 transitions");
        let any_pre_used = *pre.public.used.iter().next().expect("used spaces exist");
        assert!(post.public.used.remove(&any_pre_used));
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "removing a used space must be a definite Fail (got {verdict:?})"
        );
    }

    /// Review P1-2 regression: a non-actor's supply changing cannot `Pass`.
    #[test]
    fn non_actor_supply_drift_declines() {
        let (pre, mut post) = transitions(2, 59, 6)
            .into_iter()
            .next()
            .expect("at least one transition");
        let actor = pre.public.active_player;
        let other = (actor + 1) % pre.public.players.len();
        post.public.players[other].supply.food += 10;
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Unsure),
            "non-actor supply drift must be Unsure (got {verdict:?})"
        );
        // The full validator doesn't check supply reachability, only structural
        // invariants. This is correct — both validators accept, no disagreement.
        assert_eq!(check_full(&post), Ok(()));
        // The fallback wrapper therefore accepts via the full path.
        assert_eq!(check_incremental_or_full(&post, &pre), Ok(()));
    }

    #[cfg(feature = "perf-count-alloc")]
    #[test]
    fn search_clone_allocs_do_not_scale_with_log_size() {
        use crate::counting::allocation_count;
        use crate::model::{Config, Event, Game};

        // Warmup: run once to initialize allocator internals
        let mut warmup_game = Game::new(Config {
            players: 2,
            seed: 999,
            additional_action_space_enabled: false,
        });
        for i in 0..10 {
            warmup_game.log.events.push(Event {
                player: Some(i % 2),
                kind: "test".into(),
                payload: serde_json::json!({}),
                before: "{}".into(),
                after: "{}".into(),
            });
        }
        let _ = warmup_game.search_clone();

        // Test with large log (500 events)
        let mut game_large = Game::new(Config {
            players: 2,
            seed: 1,
            additional_action_space_enabled: false,
        });
        for i in 0..500 {
            game_large.log.events.push(Event {
                player: Some(i % 2),
                kind: "test".into(),
                payload: serde_json::json!({}),
                before: "{}".into(),
                after: "{}".into(),
            });
        }
        assert_eq!(game_large.log.events.len(), 500);

        // Measure allocations for search_clone with large log (run 3x, take median)
        let mut large_allocs = Vec::new();
        for _ in 0..3 {
            let before = allocation_count();
            let _probe = game_large.search_clone();
            large_allocs.push(allocation_count() - before);
        }
        large_allocs.sort();
        let median_large = large_allocs[1];

        // Test with small log (5 events)
        let mut game_small = Game::new(Config {
            players: 2,
            seed: 2,
            additional_action_space_enabled: false,
        });
        for i in 0..5 {
            game_small.log.events.push(Event {
                player: Some(i % 2),
                kind: "test".into(),
                payload: serde_json::json!({}),
                before: "{}".into(),
                after: "{}".into(),
            });
        }
        assert_eq!(game_small.log.events.len(), 5);

        // Measure allocations for search_clone with small log (run 3x, take median)
        let mut small_allocs = Vec::new();
        for _ in 0..3 {
            let before = allocation_count();
            let _probe = game_small.search_clone();
            small_allocs.push(allocation_count() - before);
        }
        small_allocs.sort();
        let median_small = small_allocs[1];

        // Allocations should not scale with log size (O(1) wrt log size).
        // Median allocations should be equal (or very close).
        let diff = (median_large as i64 - median_small as i64).abs();
        assert!(
                diff <= 10,
                "search_clone allocations must not scale with log size: large={median_large} small={median_small} diff={diff} (raw: large={large_allocs:?} small={small_allocs:?})"
            );
    }

    #[cfg(feature = "perf-count-alloc")]
    #[test]
    fn search_from_parts_log_card_set_shares_arc() {
        use crate::cards::verified_basic_card_set;
        use crate::model::{Config, Game};
        use std::sync::Arc;

        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        // search_from_parts shares the card_set Arc with the log
        assert!(
            Arc::ptr_eq(&game.log.card_set, &game.card_set),
            "log.card_set and game.card_set must be the same Arc"
        );
    }

    #[cfg(feature = "perf-count-alloc")]
    #[test]
    fn search_clone_log_card_set_shares_arc() {
        use crate::cards::verified_basic_card_set;
        use crate::model::{Config, Game};
        use std::sync::Arc;

        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        let clone = game.search_clone();
        // search_clone shares the card_set Arc with the original log
        assert!(
            Arc::ptr_eq(&clone.log.card_set, &game.log.card_set),
            "cloned log.card_set and original log.card_set must be the same Arc"
        );
        assert!(
            Arc::ptr_eq(&clone.card_set, &game.card_set),
            "cloned game.card_set and original game.card_set must be the same Arc"
        );
    }

    /// Review re-review P1-A counterexample reproduction (issuecomment-6766412202):
    /// a valid mid-round transition whose non-actor `Player.id` is tampered must
    /// never yield `Pass`, and `check_incremental_or_full` must reject what
    /// `check_full` rejects.
    #[test]
    fn tampered_non_actor_id_declines_and_full_rejects() {
        let (pre, mut post) = mid_round_pair();
        let non_actor = 1 - pre.public.active_player;
        post.public.players[non_actor].id = 999;
        let verdict = check_incremental(&post, &pre);
        assert_ne!(verdict, Verdict::Pass, "tampered id must not Pass");
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert_eq!(check_full(&post).unwrap_err(), "farm shape/player mismatch");
    }

    /// P1-A: post.active_player tampered to an out-of-range value fails fast.
    #[test]
    fn tampered_active_player_out_of_range_fails() {
        let (pre, mut post) = mid_round_pair();
        post.public.active_player = 999;
        assert!(matches!(check_incremental(&post, &pre), Verdict::Fail(_)));
        assert!(check_incremental_or_full(&post, &pre).is_err());
    }

    /// P1-A: post.active_player set back to the player who just moved (rotation
    /// says the other player is next) is a definite Fail.
    #[test]
    fn tampered_active_player_regression_fails() {
        let (pre, mut post) = mid_round_pair();
        post.public.active_player = pre.public.active_player;
        assert!(matches!(check_incremental(&post, &pre), Verdict::Fail(_)));
    }

    /// P1-A: non-actor `pet` (uncompared in the old field list) drifts -> Unsure,
    /// so the full validator arbitrates.
    #[test]
    fn tampered_non_actor_pet_declines() {
        let (pre, mut post) = mid_round_pair();
        let non_actor = 1 - pre.public.active_player;
        post.public.players[non_actor].pet = Some(crate::model::Animal::Sheep);
        let verdict = check_incremental(&post, &pre);
        assert_ne!(verdict, Verdict::Pass);
        if matches!(verdict, Verdict::Unsure) {
            // The full validator accepts this particular drift (pet is not
            // cross-checked there); the contract only requires incremental to
            // never Pass when it cannot prove the delta.
            check_incremental_or_full(&post, &pre).unwrap();
        }
    }

    /// P1-A: swapping `post.card_set` for a different (even valid) Arc is not
    /// provable -> Unsure, never Pass.
    #[test]
    fn tampered_card_set_swap_declines() {
        let (pre, mut post) = mid_round_pair();
        post.card_set = std::sync::Arc::new(crate::cards::CardSet::default());
        assert_eq!(check_incremental(&post, &pre), Verdict::Unsure);
    }

    /// P1-A: mutating `post.delayed_well` (hidden ledger) is not proved -> Unsure.
    #[test]
    fn tampered_delayed_well_declines() {
        let (pre, mut post) = mid_round_pair();
        post.delayed_well.insert(5, vec![pre.public.active_player]);
        assert_eq!(check_incremental(&post, &pre), Verdict::Unsure);
    }

    /// P1-A: an untampered mid-round pair still passes — the soundness fixes
    /// must not degrade the legitimate fast path.
    #[test]
    fn untampered_mid_round_still_passes() {
        let (pre, post) = mid_round_pair();
        assert_eq!(check_incremental(&post, &pre), Verdict::Pass);
    }

    /// Review P1-A/2 counterexample 1 (issuecomment-6769764161): actor card_points
    /// tampered in a mid-round work move (cards enabled or disabled) must not Pass.
    /// incremental must reject (Fail), or_full must reject, and full must reject.
    // P2 fix: exercise the 12b consistency path on a genuine transition by applying
    // one legal action first, then tampering post's actor.card_points.
    #[test]
    fn tampered_actor_card_points_declines_and_full_rejects() {
        // Cards-enabled variant: drive to a mid-round state, apply ONE legal action,
        // then tamper card_points on the resulting post state.
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        // Drive to a mid-round state.
        let mut rng = ChaCha8Rng::seed_from_u64(0xBEEF);
        let mut game = game;
        for _ in 0..6 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let pre = game.clone();
        let actor = pre.public.active_player;
        // Apply ONE real work move to get a genuine pre/post pair.
        let id = game.public.active_player;
        let obs = game.observation(id).unwrap();
        let legal = game.legal_spaces();
        let space = RandomPolicy
            .select(&obs, &legal, &mut rng)
            .expect("legal space");
        let action = CardChoiceDetails
            .produce(&obs, space)
            .expect("legal detail");
        game.apply(action).unwrap();
        let post = game.clone();
        // Tamper: increment actor card_points by 1 on the real post state.
        let mut post = post;
        post.public.players[actor].card_points += 1;
        let verdict = check_incremental(&post, &pre);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered card_points must not Pass (got {verdict:?})"
        );
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert!(check_full(&post).is_err());
    }

    /// Review P1-A/2 counterexample 1 variant: card_set disabled but actor card_points changed.
    #[test]
    fn tampered_actor_card_points_cardless_declines() {
        let (pre, mut post) = mid_round_pair();
        let actor = pre.public.active_player;
        post.public.players[actor].card_points += 1;
        let verdict = check_incremental(&post, &pre);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered card_points (cardless) must not Pass (got {verdict:?})"
        );
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert!(check_full(&post).is_err());
    }

    /// Review P1-A/2 counterexample 2 (issuecomment-6769764161): free action with
    /// actor hand card removed must not Pass.
    #[test]
    fn tampered_free_action_hand_removal_declines() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 11,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        // Drive to a state where free actions are available (e.g., after some moves).
        let mut rng = ChaCha8Rng::seed_from_u64(0xFEED);
        let mut game = game;
        for _ in 0..8 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let pre = game.clone();
        let actor = pre.public.active_player;
        // Apply a free action (DiscardGood is always legal if actor has food).
        // Use the engine's free action execution via Game::apply_free.
        let mut post = pre.clone();
        // Ensure actor has food to discard.
        post.public.players[actor].supply.food = 3;
        post.apply_free(
            actor,
            FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("free action must apply");
        // Now tamper: remove a card from actor's hand.
        assert!(
            !post.cards.hands[actor].is_empty(),
            "setup: actor must have cards"
        );
        post.cards.hands[actor].pop();
        let verdict = check_free_action(&post, &pre, actor);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered free action hand must not Pass (got {verdict:?})"
        );
        assert!(check_free_action_or_full(&post, &pre, actor).is_err());
        assert!(check_full(&post).is_err());
    }

    /// Review P1-A/2 counterexample 2 variant: free action with actor card_points tampered.
    #[test]
    fn tampered_free_action_actor_card_points_declines() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 13,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE);
        let mut game = game;
        for _ in 0..8 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let pre = game.clone();
        let actor = pre.public.active_player;
        let mut post = pre.clone();
        post.public.players[actor].supply.food = 3;
        post.apply_free(
            actor,
            FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("free action must apply");
        // Tamper: increment actor card_points.
        post.public.players[actor].card_points += 1;
        let verdict = check_free_action(&post, &pre, actor);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered free action card_points must not Pass (got {verdict:?})"
        );
        assert!(check_free_action_or_full(&post, &pre, actor).is_err());
        assert!(check_full(&post).is_err());
    }

    /// Normal card play regression: a legal work move that plays a card (Occupation/Minor
    /// on Lessons space) must still Pass incremental validation.
    #[test]
    fn legal_card_play_passes_incremental() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 17,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xBABA);
        let mut game = game;
        // Drive until we hit a Lessons/ExtraLessons space or similar.
        for _ in 0..20 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            let pre = game.clone();
            game.apply(action).unwrap();
            let post = game.clone();
            // If this was a card play (actor played_cards grew), verify incremental passes.
            if post.public.players[id].played_cards.len()
                > pre.public.players[id].played_cards.len()
            {
                let verdict = check_incremental(&post, &pre);
                assert_eq!(
                    verdict,
                    Verdict::Pass,
                    "legal card play must Pass incremental (got {verdict:?})"
                );
                check_incremental_or_full(&post, &pre).unwrap();
                assert_eq!(check_full(&post), Ok(()));
                return; // Found at least one card play, test passes.
            }
        }
        panic!("no card play observed in 20 moves — test setup may need adjustment");
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// check_incremental (work-path, 0-point card) must not Pass.
    /// Drive a real transition where actor has played cards, then tamper by
    /// pushing a duplicate of a 0-point played card. The explicit duplicate
    /// guard in section 12 must catch this with a definite Fail.
    #[test]
    fn duplicate_played_card_zero_point_declines_incremental() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 19,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD);
        let mut game = game;
        let mut found = false;
        let mut pre = Game::new(Config::default());
        let mut post = Game::new(Config::default());
        let mut actor = 0;
        // Drive up to 50 moves to find a transition where actor has at least one played card.
        for _ in 0..50 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            let pre_candidate = game.clone();
            game.apply(action).unwrap();
            let post_candidate = game.clone();
            let mover = pre_candidate.public.active_player;
            if !post_candidate.public.players[mover].played_cards.is_empty()
                && check_incremental(&post_candidate, &pre_candidate) == Verdict::Pass
            {
                pre = pre_candidate;
                post = post_candidate;
                actor = mover;
                found = true;
                break;
            }
        }
        assert!(found, "could not find a played-card transition in 50 moves");
        // Find a 0-point card in actor's played_cards to duplicate.
        let zero_point_id = post.public.players[actor]
            .played_cards
            .iter()
            .find(|id| {
                post.card_set
                    .definition(id)
                    .map(|c| c.points == 0)
                    .unwrap_or(false)
            })
            .cloned();
        let Some(zero_point_id) = zero_point_id else {
            panic!("no 0-point card in actor's played_cards — test needs different seed");
        };
        // Tamper: push a duplicate of the 0-point played card.
        post.public.players[actor].played_cards.push(zero_point_id);
        // The duplicate guard must produce a definite Fail.
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate 0-point card must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert!(check_full(&post).is_err());
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// check_incremental (work-path, non-zero-point card) must not Pass.
    /// Duplicate a non-zero-point card AND bump card_points by its points so
    /// the points check alone would pass — the duplicate guard must still catch it.
    #[test]
    fn duplicate_played_card_nonzero_point_declines_incremental() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 23,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xBEEF);
        let mut game = game;
        let mut found = false;
        let mut pre = Game::new(Config::default());
        let mut post = Game::new(Config::default());
        let mut actor = 0;
        // Drive up to 50 moves to find a transition where actor has a non-zero-point played card.
        for _ in 0..50 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            let pre_candidate = game.clone();
            game.apply(action).unwrap();
            let post_candidate = game.clone();
            let mover = pre_candidate.public.active_player;
            if !post_candidate.public.players[mover].played_cards.is_empty()
                && check_incremental(&post_candidate, &pre_candidate) == Verdict::Pass
            {
                // Check if actor has a non-zero-point played card.
                let has_nonzero = post_candidate.public.players[mover]
                    .played_cards
                    .iter()
                    .any(|id| {
                        post_candidate
                            .card_set
                            .definition(id)
                            .map(|c| c.points != 0)
                            .unwrap_or(false)
                    });
                if has_nonzero {
                    pre = pre_candidate;
                    post = post_candidate;
                    actor = mover;
                    found = true;
                    break;
                }
            }
        }
        assert!(
            found,
            "could not find a non-zero-point played-card transition in 50 moves"
        );
        // Find a non-zero-point card in actor's played_cards to duplicate.
        let nonzero_id = post.public.players[actor]
            .played_cards
            .iter()
            .find(|id| {
                post.card_set
                    .definition(id)
                    .map(|c| c.points != 0)
                    .unwrap_or(false)
            })
            .cloned();
        let Some(nonzero_id) = nonzero_id else {
            panic!("no non-zero-point card in actor's played_cards — test needs different seed");
        };
        let card_points = post.card_set.definition(&nonzero_id).unwrap().points;
        // Tamper: push a duplicate of the non-zero-point played card AND bump card_points.
        post.public.players[actor]
            .played_cards
            .push(nonzero_id.clone());
        post.public.players[actor].card_points += card_points;
        // The duplicate guard must produce a definite Fail (points check would pass alone).
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate non-zero-point card must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert!(check_full(&post).is_err());
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// check_free_action must not Pass.
    /// Drive a real free action transition, then tamper by pushing a duplicate
    /// of an existing played card. The exact Vec equality check must catch this.
    #[test]
    fn duplicate_played_card_declines_free_action() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 29,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xFEED);
        let mut game = game;
        // Drive up to 30 moves, then apply a free action, looking for a state
        // where the actor has at least one played card.
        for _ in 0..30 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        // Now apply a free action on the current state.
        let mut pre = game.clone();
        let actor = pre.public.active_player;
        // Ensure actor has food for DiscardGood.
        if pre.public.players[actor].supply.food < 3 {
            pre.public.players[actor].supply.food = 3;
        }
        let mut post = pre.clone();
        post.apply_free(
            actor,
            FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("free action must apply");
        // Verify this is a legitimate free action transition.
        assert_eq!(check_free_action(&post, &pre, actor), Verdict::Pass);
        // Check actor has played cards to duplicate.
        if post.public.players[actor].played_cards.is_empty() {
            panic!("actor has no played cards after free action — test needs different seed");
        }
        // Tamper: push a duplicate of an existing played card.
        let existing_id = post.public.players[actor].played_cards[0].clone();
        post.public.players[actor].played_cards.push(existing_id);
        // The exact Vec equality check must produce a definite Fail.
        let verdict = check_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate played card in free action must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(check_free_action_or_full(&post, &pre, actor).is_err());
        assert!(check_full(&post).is_err());
    }

    /// A plain mid-round (pre, post) pair: same round, quota not exhausted.
    fn mid_round_pair() -> (Game, Game) {
        transitions(2, 7, 24)
            .into_iter()
            .find(|(pre, post)| {
                post.public.round == pre.public.round
                    && post.public.start_player == pre.public.start_player
                    && post.public.workers_placed != post.public.work_quota
            })
            .expect("no plain mid-round move sampled")
    }

    /// Reproduction test: using an unrevealed round-card space (WesternQuarry)
    /// must not return Pass from incremental validation.
    /// WesternQuarry is a RoundCard-derived space that only becomes available
    /// when the corresponding RoundCard is in `revealed`. Mid-round, revealed is
    /// frozen, so a move onto WesternQuarry when it's not revealed must Fail/Unsure.
    #[test]
    fn unrevealed_round_card_space_western_quarry_not_pass() {
        let (pre, mut post) = mid_round_pair();
        let _actor = pre.public.active_player;
        // Replace the added space with WesternQuarry (unrevealed round card space).
        let added = *post
            .public
            .used
            .difference(&pre.public.used)
            .next()
            .expect("one new space");
        post.public.used.remove(&added);
        post.public.used.insert(Space::WesternQuarry);
        // Verify the full validator rejects this state.
        assert!(
            check_full(&post).is_err(),
            "full validator must reject unrevealed WesternQuarry"
        );
        // Incremental validation must NOT return Pass.
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "unrevealed WesternQuarry must not Pass (got {verdict:?})"
        );
    }

    /// Reproduction test: using Copse with additional_action_space_enabled=false
    /// must not return Pass from incremental validation.
    /// Copse is an additional space (Space::extra_for) and is unavailable when
    /// the setting is OFF.
    #[test]
    fn additional_space_copse_disabled_not_pass() {
        let mut game = cardless(2, 99);
        // Drive to a mid-round state.
        let mut rng = ChaCha8Rng::seed_from_u64(0xC0FFEE);
        for _ in 0..6 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let pre = game.clone();
        // Verify additional spaces are OFF.
        assert!(!pre.public.additional_action_space_enabled);
        // Apply one more action to get a legitimate post state.
        let actor = pre.public.active_player;
        let obs = pre.observation(actor).unwrap();
        let legal = pre.legal_spaces();
        let space = RandomPolicy
            .select(&obs, &legal, &mut rng)
            .expect("legal space");
        let action = CardChoiceDetails
            .produce(&obs, space)
            .expect("legal detail");
        let mut post = pre.clone();
        post.apply(action).expect("legitimate action must apply");
        // Now tamper: replace the added space with Copse (additional space).
        let added = *post
            .public
            .used
            .difference(&pre.public.used)
            .next()
            .expect("one new space");
        post.public.used.remove(&added);
        post.public.used.insert(Space::Copse);
        // Verify the full validator rejects this state.
        assert!(
            check_full(&post).is_err(),
            "full validator must reject Copse when disabled"
        );
        // Incremental validation must NOT return Pass.
        let verdict = check_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "disabled Copse must not Pass (got {verdict:?})"
        );
    }

    /// Sanity test: Forest (regular_for space) must still Pass.
    #[test]
    fn regular_forest_space_still_passes() {
        let (pre, post) = mid_round_pair();
        // Forest is a regular space (BASE). The added space in mid_round_pair
        // should already be valid, so just verify Pass.
        assert_eq!(check_incremental(&post, &pre), Verdict::Pass);
    }

    /// Regression (card accumulation state, PR #357-series follow-up): a work
    /// move whose `card_accumulations` ledger changed must never return
    /// `Pass` — card effects can legally modify it mid-round, but the cheap
    /// checks cannot prove the delta, so the transition must downgrade to
    /// `Unsure` and fall back to full validation.
    #[test]
    fn tampered_card_accumulations_decline_incremental() {
        let (pre, mut post) = mid_round_pair();
        // Add an entry for the actor's own played card (or any card id — the
        // cheap layer cannot prove legality either way).
        post.public
            .card_accumulations
            .entry(post.public.active_player)
            .or_default()
            .entry("B118".to_string())
            .or_default()
            .insert(crate::model::Good::Wood, 1);
        let verdict = check_incremental(&post, &pre);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "changed card_accumulations must not Pass (got {verdict:?})"
        );
        assert!(matches!(verdict, Verdict::Unsure));
    }

    /// Regression (card accumulation state): a free action that changes
    /// `card_accumulations` must be a definite `Fail` — the free executor has
    /// no card-effect dispatch, so the ledger cannot legally change there.
    #[test]
    fn tampered_card_accumulations_decline_free_action() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let game = Game::with_cards(
            Config {
                players: 2,
                seed: 61,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xFADE);
        let mut game = game;
        for _ in 0..8 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).unwrap();
        }
        let pre = game.clone();
        let actor = pre.public.active_player;
        let mut post = pre.clone();
        post.public.players[actor].supply.food = 3;
        post.apply_free(
            actor,
            crate::engine::FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("free action must apply");
        // Sanity: the legitimate free action passes.
        assert_eq!(check_free_action(&post, &pre, actor), Verdict::Pass);
        // Now tamper: add a card accumulation entry.
        post.public
            .card_accumulations
            .entry(actor)
            .or_default()
            .entry("X000".to_string())
            .or_default()
            .insert(crate::model::Good::Clay, 2);
        let verdict = check_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "changed card_accumulations in free action must be definite Fail (got {verdict:?})"
        );
        assert!(check_free_action_or_full(&post, &pre, actor).is_err());
    }
    /// Layer-1 contract (design decision 3, stage C): across real
    /// fixed-seed games and player counts, every layer-1 verdict satisfies
    /// the documented contract:
    /// - Pass   => layer 2 (`check_full`) accepts the same state (soundness);
    /// - Fail   => a definite violation, and the fallback wrapper rejects;
    /// - Unsure => not an acceptance: the `*_or_full` wrapper must agree
    ///   with `check_full` verbatim, never accepting what it rejects.
    #[test]
    fn incremental_layer_contract_holds_over_real_games() {
        for players in [2usize, 3, 4] {
            for seed in [7u64, 23, 101] {
                for (pre, post) in transitions(players, seed, 20) {
                    match check_incremental(&post, &pre) {
                        Verdict::Pass => assert_eq!(
                            check_full(&post),
                            Ok(()),
                            "Pass must imply full acceptance (players={players} seed={seed})"
                        ),
                        Verdict::Fail(_) => assert!(
                            check_incremental_or_full(&post, &pre).is_err(),
                            "Fail must reject via the wrapper (players={players} seed={seed})"
                        ),
                        Verdict::Unsure => assert_eq!(
                            check_incremental_or_full(&post, &pre),
                            check_full(&post),
                            "Unsure must defer to the full layer verbatim (players={players} seed={seed})"
                        ),
                    }
                }
            }
        }
    }

    /// Layer-2 contract: `check_full` is exactly `Game::validate` — the same
    /// accept/reject decision and the same arbiter message — and every other
    /// layer defers to it verbatim (it is the fallback target of layer 1 and
    /// gate 1 of layer 3, never re-implemented per caller).
    #[test]
    fn full_layer_contract_is_the_complete_arbiter() {
        let game = cardless(3, 37);
        let mut broken = game.clone();
        broken.public.round = 99;
        assert_eq!(check_full(&game), game.validate());
        assert_eq!(check_full(&game), Ok(()));
        assert_eq!(check_full(&broken), broken.validate());
        assert!(check_full(&broken).is_err());
        // A definite layer-1 violation propagates its own message (full
        // validation is not even needed), and either way the wrapper rejects.
        let (pre, mut post) = mid_round_pair();
        post.public.players[pre.public.active_player].card_points += 1;
        assert!(check_incremental_or_full(&post, &pre).is_err());
        assert!(check_full(&post).is_err());
        // The Unsure path defers to layer 2 verbatim: the wrapper decides
        // exactly what `check_full` decides — no second implementation.
        let (pre2, mut post2) = mid_round_pair();
        let other = (pre2.public.active_player + 1) % pre2.public.players.len();
        post2.public.players[other].supply.food += 10;
        assert!(matches!(check_incremental(&post2, &pre2), Verdict::Unsure));
        assert_eq!(
            check_incremental_or_full(&post2, &pre2),
            check_full(&post2),
            "the Unsure fallback must defer to the full layer verbatim"
        );
    }

    /// Layer-3 contract: a committed game with a complete logged history
    /// passes all three gates at every checkpoint; a log with a dropped
    /// event fails at the replay gate; a tampered save fails to load (the
    /// mechanism gate 3 relies on).
    #[test]
    fn replay_layer_contract_round_trip_consistency() {
        let mut game = cardless(2, 83);
        let mut rng = ChaCha8Rng::seed_from_u64(0x5EED);
        for _ in 0..6 {
            let actor = game.public.active_player;
            let obs = game.observation(actor).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails
                .produce(&obs, space)
                .expect("legal detail");
            game.apply(action).expect("action must apply");
            // Every intermediate committed state is a trust boundary.
            check_replay(&game)
                .unwrap_or_else(|error| panic!("check_replay failed mid-game: {error}"));
        }
        assert!(game.log.events.len() >= 6, "setup: logged actions");

        // Dropping the last event desynchronizes the log from the state:
        // the replay gate must reject it.
        let mut truncated = game.clone();
        truncated.log.events.pop();
        let error =
            check_replay(&truncated).expect_err("a truncated log must fail replay validation");
        assert!(
            error.contains("replay"),
            "truncated log must fail at the replay gate, got: {error}"
        );

        // Tampering with the serialized save: `load_json` (the gate-3
        // mechanism) must reject it.
        let saved = game.save_json().expect("save must succeed");
        let mut value: serde_json::Value = serde_json::from_str(&saved).expect("valid json");
        value["public"]["round"] = serde_json::json!(99);
        let tampered = serde_json::to_string(&value).expect("serialize");
        assert!(
            Game::load_json(&tampered).is_err(),
            "a tampered save must not load"
        );
    }
}
