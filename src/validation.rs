//! Validation layering for game state transitions (Issue #255 Phase B).
//!
//! Two validation tiers share one implementation of the per-player invariants
//! (`crate::model::validate_player_state`), so the layers cannot drift:
//!
//! - [`validate_full`]: complete state validation, used at trust boundaries —
//!   save/load, replay, external data ingestion, strict/debug tests. It is a
//!   thin wrapper around `Game::validate`, which stays the single source of
//!   truth for the complete invariant set.
//! - [`validate_incremental`]: fast single-action validation for search
//!   paths (MCTS/PUCT). `pre` is assumed valid (inductive base); the check
//!   verifies only what a work move can change mid-round. Anything the cheap
//!   checks cannot decide returns [`Verdict::Unsure`], and the caller must
//!   fall back to [`validate_full`] — no invalid state is ever accepted.
//!
//! Soundness contract (review P1-A): `Pass` is returned only when every
//! difference between `pre` and `post` has been *proven legal* by the checks
//! below. Any component the incremental layer does not explicitly verify is
//! either shown unchanged, or downgrades the verdict to `Unsure` so the full
//! validator arbitrates. A `Pass` is never reachable from a post state that
//! `validate_full` rejects. This includes `post.cards`, `post.card_set`,
//! `post.delayed_well`, and `post.public.active_player` which are all fully
//! verified or downgraded to `Unsure`. The internal RNG state (`post.rng`) is
//! explicitly excluded from incremental checks and is only validated by
//! `validate_full` (if at all) — documented here per review P1-A. The same
//! axiomatic exclusion covers `log` (including `GameLog::initial_state_hash`):
//! both are history-borne state whose legitimacy cannot be decided from a
//! single transition, so their defense line is `Game::integrity_hash`
//! (stamped on save, verified in `load_json`) and the replay chain — the
//! incremental layer must not fabricate delta rules for them. The Step 3
//! bookkeeping fields that `state_hash` omits for v5 compatibility
//! (`well_food_schedule`, `pending`, `event_budget`, `card_uses`,
//! `card_use_limits`, `next_decision_id`, `record_events`) are covered by
//! checks 11b and 15–21 below: deltas that no committed transition can
//! produce are definite Fail; deltas whose legality the transition layer
//! cannot decide (provenance questions) decline to `Unsure`, which the
//! `_or_full` wrapper resolves against `validate_full`.

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

/// Full validation — the complete invariant set, identical to `Game::validate`.
pub fn validate_full(game: &Game) -> Result<(), String> {
    game.validate()
}

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
/// `validate_full` rejects.
pub fn validate_incremental(post: &Game, pre: &Game) -> Verdict {
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
            if delta != 1 {
                return Verdict::Fail("active player did not place exactly one worker".into());
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
    if added.len() != 1 {
        return Verdict::Fail("a work move must add exactly one used space".into());
    }
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
    // 7. Major improvements: the union across market + player tables must keep
    //    every physical major improvement in exactly one location (the same
    //    invariant `validate_full` enforces). A work move may move an item
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
            // validate_full's hard errors.
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
    // 11b. Animal accumulations: the animal markets (Sheep/Pig/Cattle)
    //      accumulate only at start_round and a work move can only take from
    //      them (the take action removes the key when the market empties).
    //      Close-out (PR #358 review): growth is no longer a decline — the
    //      only writer is start_round, and a round change already returned
    //      Unsure at check 2, so any growth/new key on a same-round transition
    //      is not producible by any work move → definite Fail. (Full
    //      validation accepts any market-key accumulation, so an Unsure here
    //      used to let the tamper through.)
    for (space, post_n) in &post.public.animal_accumulations {
        match pre.public.animal_accumulations.get(space) {
            Some(pre_n) if post_n <= pre_n => {}
            _ => return Verdict::Fail("animal accumulation grew mid-round".into()),
        }
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
    // 15. post.well_food_schedule: public mirror of delayed_well (Finding A).
    //     Reachable only when delayed_well is unchanged (check 14 above). The
    //     only mid-round writer is the Well purchase, which writes both maps in
    //     the same step, and the round-start payout removes them together —
    //     but a round change already returned Unsure at check 2. A mirror
    //     delta on a delayed_well-stable, same-round transition is therefore
    //     not producible by any work move: definite Fail (close-out: full
    //     validation deliberately tolerates in-range orphan mirrors for legacy
    //     saves, so Unsure→full used to accept this tamper).
    if post.well_food_schedule != pre.well_food_schedule {
        return Verdict::Fail("well_food_schedule changed without paired delayed_well".into());
    }
    // 16. post.pending: a committed work move that pauses never reaches this
    //     validator (apply commits the paused state directly), and the resume
    //     path enters with the decision already cleared, so both sides are None
    //     here. Any pending drift is therefore not producible on a validated
    //     transition: definite Fail (close-out: full validation deliberately
    //     does not inspect `pending`, so Unsure→full used to accept it).
    if post.pending != pre.pending {
        return Verdict::Fail("pending decision drifted on a committed transition".into());
    }
    // 17. post.event_budget: replenished to DEFAULT_EVENT_BUDGET at the start
    //     of every committed action and only ever spent downward
    //     (consume_budget), so a value above the cap cannot be produced by any
    //     transition. The exact remaining budget is not provable (legit chains
    //     consume freely, like player supplies) and is left to the full
    //     validator, which enforces the same cap.
    if post.event_budget > crate::model::DEFAULT_EVENT_BUDGET {
        return Verdict::Fail("event_budget above replenish cap".into());
    }
    // 18. post.card_uses: the per-round use ledger is written only inside the
    //     card-event chains (check_card_use increments entries); a chain that
    //     pauses commits its mid-action state directly and never reaches this
    //     validator. On any validated transition the ledger entries that exist
    //     in `pre` therefore never decrease (a decrease or a vanished key
    //     re-opens a card's use budget, which no committed transition produces):
    //     definite Fail. New keys and growth are legitimate only when the card
    //     genuinely fired (card still in hands/played) — deciding that is
    //     provenance, exactly what validate_full checks, so unseen keys are
    //     delegated with Unsure instead of guessed here (Pass would
    //     short-circuit full validation and let fabricated keys through).
    for (id, pre_n) in &pre.card_uses {
        match post.card_uses.get(id) {
            Some(post_n) if post_n >= pre_n => {}
            _ => return Verdict::Fail("card use ledger decreased mid-round".into()),
        }
    }
    // Every ledger entry in `post` that the transition layer cannot speak for
    // (a key absent from `pre`) still needs provenance arbitration; returning
    // Pass here would skip validate_full's card-ownership check entirely.
    if post.card_uses.len() > pre.card_uses.len() {
        return Verdict::Unsure;
    }
    for id in post.card_uses.keys() {
        if !pre.card_uses.contains_key(id) {
            return Verdict::Unsure;
        }
    }
    // 19. post.card_use_limits: derived from CardDefinition::use_limit at game
    //     construction and never written again; with card_set pointer-identical
    //     (checked above), any drift is tampering.
    //     Deliberately NOT compared here (axiomatic, cross-cutting audit 4/4):
    //     `rng` and `log` (including GameLog::initial_state_hash) are
    //     history-borne state whose legitimacy cannot be decided from a single
    //     transition — fabricating a delta rule would be unsound. Their defense
    //     line is integrity_hash (stamped on save, verified in load_json) and
    //     the replay chain, not this layer.
    if post.card_use_limits != pre.card_use_limits {
        return Verdict::Fail("card_use_limits mutated mid-game".into());
    }
    // 20. post.next_decision_id: the counter only advances when a pause is
    //     committed (apply's Paused branch, engine.rs ~line 900), and that
    //     branch commits mid-action state directly without ever reaching this
    //     validator; the resume path enters with the decision resolved and
    //     the counter already advanced in `pre`. Any drift visible to this
    //     validator is therefore not producible by any committed transition:
    //     definite Fail (the full validator does not read the counter, so an
    //     Unsure here would delegate to a check that cannot see the tamper).
    if post.next_decision_id != pre.next_decision_id {
        return Verdict::Fail("next_decision_id drifted on a committed transition".into());
    }
    // 21. post.record_events: serde-skipped search flag; no apply/apply_free
    //     path writes it, so a flip between pre and post is not producible by
    //     any committed transition. It is excluded from saves by #[serde(skip)]
    //     and reconstructed as true on load, so integrity_hash covers
    //     everything durable.
    if post.record_events != pre.record_events {
        return Verdict::Fail("record_events flipped mid-transition".into());
    }
    Verdict::Pass
}

/// Incremental validation with automatic fallback to full validation.
pub fn validate_incremental_or_full(post: &Game, pre: &Game) -> Result<(), String> {
    match validate_incremental(post, pre) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => validate_full(post),
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
///
/// Soundness contract (free-side close-out, PR #358 review): `Pass` is
/// returned only when every difference between `pre` and `post` has been
/// proven legal by the checks below — including the Step 3 bookkeeping
/// fields `well_food_schedule`, `pending`, `event_budget` cap, `card_uses`,
/// `card_use_limits`, `next_decision_id` and `record_events` (checks
/// 11/15b–15h), which mirror the transition-layer checks 11b/15–21. The only
/// committed free path (`apply_free`→`execute_free`) resolves no card
/// effects and never pauses, so unlike a work move it cannot produce any
/// ledger/budget/decision delta; where the work side must delegate a
/// provenance question to `validate_full` with Unsure (e.g. a first
/// ledger use), the free side has no such producer and Drift is a definite
/// Fail. A `Pass` is therefore never reachable from a post state that
/// `validate_full` rejects — the Pass→Ok short-circuit in
/// `validate_free_action_or_full` is sound for the same reason as the work
/// side.
pub fn validate_free_action(post: &Game, pre: &Game, actor: usize) -> Verdict {
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
            // validate_full's hard errors.
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
    // 11. Animal accumulations: same-decrease rule for the general case,
    //     but growth is a definite Fail on the free path (work-side parity,
    //     check 11b): the only writer is start_round, and a round change
    //     already returned Unsure at check 2, so any same-round growth or
    //     new key is not producible by any committed free action. (Full
    //     validation accepts any market-key accumulation, so Unsure→full
    //     used to accept this tamper on the free path.)
    for (space, post_n) in &post.public.animal_accumulations {
        match pre.public.animal_accumulations.get(space) {
            Some(pre_n) if post_n <= pre_n => {}
            _ => return Verdict::Fail("animal accumulation grew mid-round".into()),
        }
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
    // 15b. post.well_food_schedule: public mirror of delayed_well. The only
    //     mid-round writer is the Well purchase (a work move), and a round
    //     change already returned Unsure at check 2, so a mirror delta on a
    //     delayed_well-stable, same-round free transition is not producible
    //     by any committed free action: definite Fail (work-side parity,
    //     check 15). Full validation tolerates in-range orphan mirrors, so
    //     Pass used to short-circuit the only check that can see this.
    if post.well_food_schedule != pre.well_food_schedule {
        return Verdict::Fail("well_food_schedule changed without paired delayed_well".into());
    }
    // 15c. post.pending: apply_free rejects a stored decision up front and
    //     free actions never pause (execute_free has no card chain), so any
    //     pending drift on a validated free transition is not producible:
    //     definite Fail (work-side parity, check 16). Full validation
    //     deliberately does not inspect `pending`.
    if post.pending != pre.pending {
        return Verdict::Fail("pending decision drifted on a committed transition".into());
    }
    // 15d. post.event_budget: replenished to DEFAULT_EVENT_BUDGET at the
    //     start of every committed action (apply_free resets it exactly like
    //     apply_with_plans) and only ever spent downward, so a value above
    //     the cap cannot be produced by any transition. The exact remaining
    //     budget is not provable (legit chains consume freely) and is left to
    //     the full validator, which enforces the same cap (work-side parity,
    //     check 17 — this is review claim (a): Pass used to skip it).
    if post.event_budget > crate::model::DEFAULT_EVENT_BUDGET {
        return Verdict::Fail("event_budget above replenish cap".into());
    }
    // 15e. post.card_uses: free actions never resolve card effects
    //      (execute_free contains no card-chain), so the use ledger must be
    //      byte-identical across a committed free transition. Unlike the
    //      work side (where a legitimate first use must stay possible and
    //      unseen keys delegate with Unsure), on the free path NO committed
    //      action can produce a ledger delta of any kind: any drift is a
    //      definite Fail. This is strictly stronger than the work side and
    //      safe for the same soundness reason: a Fail is only ever returned
    //      for deltas no committed free path can produce.
    if post.card_uses != pre.card_uses {
        return Verdict::Fail("card use ledger drifted in a free action".into());
    }
    // 15f. post.card_use_limits: derived from CardDefinition::use_limit at
    //      game construction and never written again. With the card set
    //      pointer-identical (check 14 above), any limits drift is
    //      tampering: definite Fail (work-side parity, check 19 — review
    //      claim (b): Pass used to skip the full validator's
    //      definition-agreement check).
    if post.card_use_limits != pre.card_use_limits {
        return Verdict::Fail("card_use_limits mutated mid-game".into());
    }
    // 15g. post.next_decision_id: the counter only advances when a pause is
    //      committed, and free actions never pause (15c); any drift visible
    //      to this validator is therefore not producible: definite Fail
    //      (work-side parity, check 20; full validation does not read the
    //      counter, so Pass used to short-circuit the only check that can
    //      see it).
    if post.next_decision_id != pre.next_decision_id {
        return Verdict::Fail("next_decision_id drifted on a committed transition".into());
    }
    // 15h. post.record_events: serde-skipped search flag; no apply/apply_free
    //      path writes it, so a flip between pre and post is not producible
    //      by any committed transition (work-side parity, check 21).
    if post.record_events != pre.record_events {
        return Verdict::Fail("record_events flipped mid-transition".into());
    }
    Verdict::Pass
}

/// Incremental free-action validation with automatic fallback to full validation.
pub fn validate_free_action_or_full(post: &Game, pre: &Game, actor: usize) -> Result<(), String> {
    match validate_free_action(post, pre, actor) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => validate_full(post),
    }
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
                    validate_incremental(post, pre),
                    Verdict::Pass,
                    "mid-round move must pass incremental validation"
                );
                validate_incremental_or_full(post, pre).unwrap();
                assert_eq!(validate_full(post), Ok(()));
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
                assert_eq!(validate_incremental(post, pre), Verdict::Unsure);
                validate_incremental_or_full(post, pre).unwrap();
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
        assert_eq!(validate_incremental(&game, &before), Verdict::Unsure);
        // Fallback path accepts it via the full validator.
        validate_incremental_or_full(&game, &before).unwrap();
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
        let verdict = validate_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "corrupted farm must not pass"
        );
        if let Verdict::Unsure = verdict {
            assert!(validate_full(&post).is_err());
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
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn full_validation_wraps_engine_validate() {
        let game = cardless(3, 37);
        assert_eq!(validate_full(&game), Ok(()));
        let mut broken = game.clone();
        broken.public.round = 99;
        assert!(validate_full(&broken).is_err());
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
        let verdict = validate_incremental(&post, &pre);
        assert!(matches!(verdict, Verdict::Fail(_) | Verdict::Unsure));
        // Restore the market and re-check: legal state again.
        let mut restored = post;
        let imp = restored.public.players[0].improvements.pop().unwrap();
        restored.public.improvement_market.push(imp);
        restored.public.improvement_market.sort();
        assert_eq!(validate_full(&restored), Ok(()));
    }

    /// Review P1-2 regression: a major improvement outright REMOVED (the
    /// counter-example from the review: `improvement_market.pop()` without
    /// re-adding it anywhere) must never reach `Pass`, because
    /// `validate_full` rejects it ("major improvement duplicated or missing").
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
        let verdict = validate_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_) | Verdict::Unsure),
            "vanished major improvement must not Pass (got {verdict:?})"
        );
        assert!(validate_full(&post).is_err());
        // And the fallback wrapper must reject it too.
        assert!(validate_incremental_or_full(&post, &pre).is_err());
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
        let verdict = validate_incremental(&post, &pre);
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
        let verdict = validate_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Unsure),
            "non-actor supply drift must be Unsure (got {verdict:?})"
        );
        // The full validator doesn't check supply reachability, only structural
        // invariants. This is correct — both validators accept, no disagreement.
        assert_eq!(validate_full(&post), Ok(()));
        // The fallback wrapper therefore accepts via the full path.
        assert_eq!(validate_incremental_or_full(&post, &pre), Ok(()));
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
    /// never yield `Pass`, and `validate_incremental_or_full` must reject what
    /// `validate_full` rejects.
    #[test]
    fn tampered_non_actor_id_declines_and_full_rejects() {
        let (pre, mut post) = mid_round_pair();
        let non_actor = 1 - pre.public.active_player;
        post.public.players[non_actor].id = 999;
        let verdict = validate_incremental(&post, &pre);
        assert_ne!(verdict, Verdict::Pass, "tampered id must not Pass");
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        assert_eq!(
            validate_full(&post).unwrap_err(),
            "farm shape/player mismatch"
        );
    }

    /// P1-A: post.active_player tampered to an out-of-range value fails fast.
    #[test]
    fn tampered_active_player_out_of_range_fails() {
        let (pre, mut post) = mid_round_pair();
        post.public.active_player = 999;
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// P1-A: post.active_player set back to the player who just moved (rotation
    /// says the other player is next) is a definite Fail.
    #[test]
    fn tampered_active_player_regression_fails() {
        let (pre, mut post) = mid_round_pair();
        post.public.active_player = pre.public.active_player;
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
    }

    /// P1-A: non-actor `pet` (uncompared in the old field list) drifts -> Unsure,
    /// so the full validator arbitrates.
    #[test]
    fn tampered_non_actor_pet_declines() {
        let (pre, mut post) = mid_round_pair();
        let non_actor = 1 - pre.public.active_player;
        post.public.players[non_actor].pet = Some(crate::model::Animal::Sheep);
        let verdict = validate_incremental(&post, &pre);
        assert_ne!(verdict, Verdict::Pass);
        if matches!(verdict, Verdict::Unsure) {
            // The full validator accepts this particular drift (pet is not
            // cross-checked there); the contract only requires incremental to
            // never Pass when it cannot prove the delta.
            validate_incremental_or_full(&post, &pre).unwrap();
        }
    }

    /// P1-A: swapping `post.card_set` for a different (even valid) Arc is not
    /// provable -> Unsure, never Pass.
    #[test]
    fn tampered_card_set_swap_declines() {
        let (pre, mut post) = mid_round_pair();
        post.card_set = std::sync::Arc::new(crate::cards::CardSet::default());
        assert_eq!(validate_incremental(&post, &pre), Verdict::Unsure);
    }

    /// P1-A: mutating `post.delayed_well` (hidden ledger) is not proved -> Unsure.
    #[test]
    fn tampered_delayed_well_declines() {
        let (pre, mut post) = mid_round_pair();
        post.delayed_well.insert(5, vec![pre.public.active_player]);
        assert_eq!(validate_incremental(&post, &pre), Verdict::Unsure);
    }

    /// P1-A: an untampered mid-round pair still passes — the soundness fixes
    /// must not degrade the legitimate fast path.
    #[test]
    fn untampered_mid_round_still_passes() {
        let (pre, post) = mid_round_pair();
        assert_eq!(validate_incremental(&post, &pre), Verdict::Pass);
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
        let verdict = validate_incremental(&post, &pre);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered card_points must not Pass (got {verdict:?})"
        );
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        assert!(validate_full(&post).is_err());
    }

    /// Review P1-A/2 counterexample 1 variant: card_set disabled but actor card_points changed.
    #[test]
    fn tampered_actor_card_points_cardless_declines() {
        let (pre, mut post) = mid_round_pair();
        let actor = pre.public.active_player;
        post.public.players[actor].card_points += 1;
        let verdict = validate_incremental(&post, &pre);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered card_points (cardless) must not Pass (got {verdict:?})"
        );
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        assert!(validate_full(&post).is_err());
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
        let verdict = validate_free_action(&post, &pre, actor);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered free action hand must not Pass (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
        assert!(validate_full(&post).is_err());
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
        let verdict = validate_free_action(&post, &pre, actor);
        assert_ne!(
            verdict,
            Verdict::Pass,
            "tampered free action card_points must not Pass (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
        assert!(validate_full(&post).is_err());
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
                let verdict = validate_incremental(&post, &pre);
                assert_eq!(
                    verdict,
                    Verdict::Pass,
                    "legal card play must Pass incremental (got {verdict:?})"
                );
                validate_incremental_or_full(&post, &pre).unwrap();
                assert_eq!(validate_full(&post), Ok(()));
                return; // Found at least one card play, test passes.
            }
        }
        panic!("no card play observed in 20 moves — test setup may need adjustment");
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// validate_incremental (work-path, 0-point card) must not Pass.
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
                && validate_incremental(&post_candidate, &pre_candidate) == Verdict::Pass
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
        let verdict = validate_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate 0-point card must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        assert!(validate_full(&post).is_err());
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// validate_incremental (work-path, non-zero-point card) must not Pass.
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
                && validate_incremental(&post_candidate, &pre_candidate) == Verdict::Pass
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
        let verdict = validate_incremental(&post, &pre);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate non-zero-point card must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        assert!(validate_full(&post).is_err());
    }

    /// 4th review P1 (issuecomment-6772921046): duplicate played-card ID in
    /// validate_free_action must not Pass.
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
        assert_eq!(validate_free_action(&post, &pre, actor), Verdict::Pass);
        // Check actor has played cards to duplicate.
        if post.public.players[actor].played_cards.is_empty() {
            panic!("actor has no played cards after free action — test needs different seed");
        }
        // Tamper: push a duplicate of an existing played card.
        let existing_id = post.public.players[actor].played_cards[0].clone();
        post.public.players[actor].played_cards.push(existing_id);
        // The exact Vec equality check must produce a definite Fail.
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "duplicate played card in free action must be definite Fail (got {verdict:?})"
        );
        // Full validation must also reject.
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
        assert!(validate_full(&post).is_err());
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
            validate_full(&post).is_err(),
            "full validator must reject unrevealed WesternQuarry"
        );
        // Incremental validation must NOT return Pass.
        let verdict = validate_incremental(&post, &pre);
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
            validate_full(&post).is_err(),
            "full validator must reject Copse when disabled"
        );
        // Incremental validation must NOT return Pass.
        let verdict = validate_incremental(&post, &pre);
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
        assert_eq!(validate_incremental(&post, &pre), Verdict::Pass);
    }

    // ---- Cross-cutting audit 4/4: state-element tampering must not pass ----

    /// Audit element 1: a `well_food_schedule` change without the paired
    /// `delayed_well` change (check 14) is not producible by any work move —
    /// the only mid-round writer is the Well purchase, which writes both.
    #[test]
    fn tampered_well_food_schedule_without_delayed_well_declines() {
        let (pre, mut post) = mid_round_pair();
        let actor = pre.public.active_player;
        post.well_food_schedule
            .entry(5)
            .or_default()
            .push(actor % pre.public.players.len());
        // Close-out (PR #358 review): mirror-only drift is not producible by
        // any committed work move — the only mid-round writer (Well purchase)
        // writes both ledgers, so check 14 fires first. Definite Fail; full
        // validation deliberately tolerates in-range orphan mirrors (P1-4).
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 2: a suspended decision appearing out of nowhere cannot
    /// Pass. Paused states are committed outside this validator and the resume
    /// enters with the decision cleared, so pending drift is unprovable here.
    #[test]
    fn tampered_pending_decision_declines() {
        let (pre, mut post) = mid_round_pair();
        post.pending = Some(crate::pending::PendingDecision::new(
            pre.next_decision_id,
            pre.public.active_player,
            crate::pending::PendingKind::ChooseGain {
                source_card: "A001".into(),
                options: vec![crate::pending::GainOption {
                    good: crate::model::Good::Food,
                    amount: 1,
                }],
            },
            post.state_hash(),
            serde_json::json!({}),
        ));
        // Close-out: validated transitions have `pending == None` on both
        // sides (apply rejects a stored decision, a pause commits outside this
        // validator, and the resume path enters with the decision cleared), so
        // any pending drift is not producible → definite Fail. Full validation
        // deliberately does not inspect `pending`.
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 3: the event budget is replenished to
    /// DEFAULT_EVENT_BUDGET at every committed action and only spent downward,
    /// so a post value above the cap is a definite Fail (and the full
    /// validator must reject the same state).
    #[test]
    fn tampered_event_budget_above_cap_fails() {
        let (pre, mut post) = mid_round_pair();
        post.event_budget = crate::model::DEFAULT_EVENT_BUDGET + 1;
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_full(&post).is_err());
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 4: decreasing (or dropping) a per-round use count would
    /// re-open a card's use budget — a work move can only grow the ledger.
    #[test]
    fn tampered_card_uses_decrease_declines() {
        let (mut pre, mut post) = mid_round_pair();
        pre.card_uses.insert("A001".into(), 2);
        post.card_uses.insert("A001".into(), 1);
        // Close-out: a same-round decrease re-opens a card's use budget and no
        // work move can produce it (the ledger only grows mid-round; the
        // round-start clear already returned Unsure at check 2) → definite Fail.
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        // Growth, however, is what legitimate first-use looks like and must
        // keep the fast path: only the increase is present, so Pass.
        let (mut pre, mut post) = mid_round_pair();
        pre.card_uses.insert("A001".into(), 1);
        post.card_uses.insert("A001".into(), 3);
        assert_eq!(validate_incremental(&post, &pre), Verdict::Pass);
    }

    /// Audit element 5: `card_use_limits` is derived from the card
    /// definitions at construction and never written again; with card_set
    /// pointer-identical, any drift is tampering (definite Fail, full rejects).
    #[test]
    fn tampered_card_use_limits_declines_and_full_rejects() {
        let (pre, mut post) = mid_round_pair();
        post.card_use_limits.insert("A001".into(), 1);
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_full(&post).is_err());
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 6: the decision counter only advances at pause commit,
    /// which never reaches this validator — drift is unprovable here.
    #[test]
    fn tampered_next_decision_id_declines() {
        let (pre, mut post) = mid_round_pair();
        post.next_decision_id += 1;
        // Close-out: the counter only advances at pause commit, which never
        // reaches this validator — any drift on a validated transition is not
        // producible → definite Fail. Full validation does not read the counter.
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 7/8 (axiomatic, deliberately NOT checked): `rng` and
    /// `log.initial_state_hash` are history-borne state. No single transition
    /// can prove a delta rule for them, so the incremental layer fabricates
    /// no judgment — the test pins that documented boundary instead of a
    /// check. Their defense lines are `Game::integrity_hash` (stamped by
    /// `save_json`, verified in `load_json`) for the durable bookkeeping,
    /// `load_json`'s recomputed `initial_state_hash` comparison, and the
    /// event-chain replay — not this layer.
    #[test]
    fn rng_and_log_initial_hash_stay_outside_incremental_scope_by_design() {
        let (pre, mut post) = mid_round_pair();
        post.rng.round = ChaCha8Rng::seed_from_u64(0xD06);
        post.log.initial_state_hash = "tampered".into();
        assert_eq!(validate_incremental(&post, &pre), Verdict::Pass);
        assert_eq!(validate_full(&post), Ok(()));
    }

    /// Audit element 9: `record_events` is a serde-skipped search flag no
    /// apply path writes; a flip between pre and post is a definite Fail.
    #[test]
    fn tampered_record_events_flip_fails() {
        let (pre, mut post) = mid_round_pair();
        post.record_events = !pre.record_events;
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        // Close-out element 5: already a direct Fail at the pre-fix HEAD, so
        // the wrapper rejects it too — no Unsure→full hole for this element.
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Audit element 10: animal accumulations obey the same monotone rule as
    /// `accumulations` mid-round (parity with validate_free_action check 11):
    /// they grow only at round start and a work move can only take from them.
    #[test]
    fn tampered_animal_accumulations_growth_declines() {
        let (pre, mut post) = mid_round_pair();
        // A new key (growth of an existing key exercises the same `_` arm).
        post.public.animal_accumulations.insert(Space::Forest, 1);
        // Close-out: mid-round growth has no producer (accumulation happens at
        // start_round, which check 2 already returned Unsure for) → definite
        // Fail; full additionally rejects this non-market key.
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        // Growth of an existing entry must also decline.
        let (mut pre, mut post) = mid_round_pair();
        pre.public
            .animal_accumulations
            .insert(Space::SheepMarket, 1);
        post.public
            .animal_accumulations
            .insert(Space::SheepMarket, 2);
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        // The full validator accepts a market-key accumulation of any size, so
        // only the transition check can prove this drift illegal.
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        // Taking (decrease/removal) stays legal — the take action removes the
        // key when the market empties.
        post.public.animal_accumulations.remove(&Space::SheepMarket);
        assert_eq!(validate_incremental(&post, &pre), Verdict::Pass);
        validate_incremental_or_full(&post, &pre).unwrap();
    }

    /// Full validation must reject a use-ledger entry for a card nobody has
    /// played and a count above the card's declared per-round limit.
    #[test]
    fn full_rejects_card_use_ledger_drift() {
        use crate::cards::CardKind;
        // The verified basic set has no use-limited card, so declare a
        // per-round limit of 1 on one occupation; `with_cards` derives
        // `card_use_limits` from the modified definitions.
        let mut set = crate::cards::verified_basic_card_set(2).expect("verified set");
        let limited_id = set
            .definitions
            .iter()
            .find(|c| c.kind == CardKind::Occupation)
            .expect("set has occupations")
            .id
            .clone();
        for card in set.definitions.iter_mut() {
            if card.id == limited_id {
                card.use_limit = Some(1);
            }
        }
        let mut game = Game::with_cards(
            Config {
                players: 2,
                seed: 71,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        // Move the limited card from a hand onto its owner's table: the
        // location partition stays intact and card_points tracks the move,
        // so the ledger entry below is the only plausible state change.
        let owner = (0..game.cards.hands.len())
            .find(|&i| game.cards.hands[i].contains(&limited_id))
            .expect("dealt card sits in some hand");
        let points = game
            .card_set
            .definition(&limited_id)
            .expect("definition")
            .points;
        let hand = &mut game.cards.hands[owner];
        let pos = hand
            .iter()
            .position(|id| *id == limited_id)
            .expect("in hand");
        hand.remove(pos);
        game.public.players[owner]
            .played_cards
            .push(limited_id.clone());
        game.public.players[owner].card_points += points;
        assert_eq!(validate_full(&game), Ok(()));
        // Over the declared limit: definite Err.
        let mut over = game.clone();
        over.card_uses.insert(limited_id.clone(), 2);
        assert_eq!(
            validate_full(&over),
            Err("card use count exceeds its per-round limit".into())
        );
        // Entry for an unplayed card: definite Err.
        let unplayed_id = game.cards.hands[0][0].clone();
        let mut unplayed = game.clone();
        unplayed.card_uses.insert(unplayed_id, 1);
        assert_eq!(
            validate_full(&unplayed),
            Err("card use ledger entry for an unplayed card".into())
        );
        // Limits disagreeing with the card definitions: definite Err.
        let mut drifted = game.clone();
        drifted.card_use_limits.insert("NOT_A_CARD".into(), 3);
        assert_eq!(
            validate_full(&drifted),
            Err("card use limits disagree with card definitions".into())
        );
    }

    /// Full validation must reject well schedules that name impossible rounds
    /// or unknown players (the round-start payout would index out of bounds),
    /// while a plausible future-round entry stays accepted.
    #[test]
    fn full_rejects_well_schedule_out_of_range_entries() {
        let game = cardless(2, 61);
        assert_eq!(validate_full(&game), Ok(()));
        let mut bad_player = game.clone();
        bad_player.delayed_well.insert(3, vec![99]);
        assert_eq!(
            validate_full(&bad_player),
            Err("delayed well schedule names an unknown player".into())
        );
        let mut bad_round = game.clone();
        bad_round.well_food_schedule.insert(15, vec![0]);
        assert_eq!(
            validate_full(&bad_round),
            Err("delayed well schedule round out of range".into())
        );
        // A well entry for a future round with a real player index is legal.
        let mut plausible = game.clone();
        plausible.delayed_well.insert(5, vec![1]);
        plausible.well_food_schedule.insert(5, vec![1]);
        assert_eq!(validate_full(&plausible), Ok(()));
    }

    /// Full validation must reject animal accumulations keyed by a space that
    /// never accumulates animals. (A key whose market card is not currently
    /// revealed stays accepted: engine states always correlate the two, but
    /// directly constructed test states legitimately re-swap revealed cards,
    /// so the correlation is not an invariant of every accepted state.)
    #[test]
    fn full_rejects_animal_accumulation_for_non_market_space() {
        let game = cardless(2, 67);
        let mut not_a_market = game.clone();
        not_a_market
            .public
            .animal_accumulations
            .insert(Space::Forest, 1);
        assert_eq!(
            validate_full(&not_a_market),
            Err("animal accumulation for a non-market space".into())
        );
        // An animal-market key is in the decidable domain even mid-test swaps.
        let mut market_key = game.clone();
        market_key
            .public
            .animal_accumulations
            .insert(Space::SheepMarket, 1);
        assert_eq!(validate_full(&market_key), Ok(()));
    }

    // ---- Close-out (PR #358 review): Unsure→full holes for transition-only
    // provable tampering. Each test changes exactly one element on a normal
    // mid-round (pre, post) pair and asserts the FINAL wrapper result. The
    // `validate_full` assertion next to each tamper documents why the hole
    // existed: the full validator alone cannot see these drifts. ----

    /// Element 1: an orphan `well_food_schedule` entry without the paired
    /// `delayed_well` change. Full validation accepts any in-range mirror
    /// entry, so only the transition check can prove it illegal.
    #[test]
    fn closeout_well_food_schedule_mirror_orphan_rejected() {
        let (pre, mut post) = mid_round_pair();
        let actor = pre.public.active_player;
        post.well_food_schedule.entry(5).or_default().push(actor);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 2: a suspended decision appearing on a committed work move.
    /// Full validation deliberately does not inspect `pending`.
    #[test]
    fn closeout_pending_decision_appearing_rejected() {
        let (pre, mut post) = mid_round_pair();
        post.pending = Some(crate::pending::PendingDecision::new(
            pre.next_decision_id,
            pre.public.active_player,
            crate::pending::PendingKind::ChooseGain {
                source_card: "A001".into(),
                options: vec![crate::pending::GainOption {
                    good: crate::model::Good::Food,
                    amount: 1,
                }],
            },
            post.state_hash(),
            serde_json::json!({}),
        ));
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 3a: a brand-new use-ledger key for a card nobody has played.
    /// At the pre-fix HEAD the incremental layer returned `Pass` (its loop only
    /// covers keys present in `pre`), which short-circuits full validation —
    /// even though full validation itself rejects this state.
    #[test]
    fn closeout_card_uses_new_unplayed_key_rejected() {
        let (pre, mut post) = mid_round_pair();
        post.card_uses.insert("ZZ_NOT_A_CARD".into(), 1);
        assert!(validate_full(&post).is_err());
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 3b: a same-round count decrease on a PLAYED card. Full
    /// validation accepts the post state (played card, no per-round limit), so
    /// only the transition check can see the reopened use budget.
    #[test]
    fn closeout_card_uses_same_round_decrease_on_played_card_rejected() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut game = Game::with_cards(
            Config {
                players: 2,
                seed: 29,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xFEED);
        // Find a REAL mid-round work-move pair whose incremental verdict is
        // Pass (so every check up to the ledger check is exercised) and where
        // some card is already on a table.
        let mut found = None;
        for _ in 0..40 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails.produce(&obs, space).expect("detail");
            let pre = game.clone();
            game.apply(action).expect("random move");
            let post = game.clone();
            if post.public.round == pre.public.round
                && post.public.start_player == pre.public.start_player
                && post
                    .public
                    .players
                    .iter()
                    .any(|p| !p.played_cards.is_empty())
                && validate_incremental(&post, &pre) == Verdict::Pass
            {
                found = Some((pre, post));
                break;
            }
        }
        let (mut pre, mut post) = found.expect("a mid-round pair with a played card");
        let played_id = post
            .public
            .players
            .iter()
            .flat_map(|p| p.played_cards.iter())
            .next()
            .expect("played card on some table")
            .clone();
        pre.card_uses.insert(played_id.clone(), 2);
        post.card_uses.insert(played_id.clone(), 1);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 4: decision-counter drift. Full validation does not read the
    /// counter, so only the transition check can prove the drift illegal.
    #[test]
    fn closeout_next_decision_id_drift_rejected() {
        let (pre, mut post) = mid_round_pair();
        post.next_decision_id += 1;
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 5: `record_events` flip — does NOT reproduce at the pre-fix
    /// HEAD: check 21 already returns a direct Fail, so the wrapper rejects.
    #[test]
    fn closeout_record_events_flip_already_direct_fail() {
        let (pre, mut post) = mid_round_pair();
        post.record_events = !pre.record_events;
        assert!(matches!(
            validate_incremental(&post, &pre),
            Verdict::Fail(_)
        ));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    /// Element 6: same-round growth of an animal-market accumulation. Full
    /// validation accepts any market-key accumulation, so only the transition
    /// check can prove the growth illegal.
    #[test]
    fn closeout_animal_accumulation_same_round_growth_rejected() {
        let (mut pre, mut post) = mid_round_pair();
        pre.public
            .animal_accumulations
            .insert(Space::SheepMarket, 1);
        post.public
            .animal_accumulations
            .insert(Space::SheepMarket, 2);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
        // A brand-new market key mid-round is the same unproducible growth.
        let (mut pre, mut post) = mid_round_pair();
        pre.public.animal_accumulations.remove(&Space::SheepMarket);
        post.public
            .animal_accumulations
            .insert(Space::SheepMarket, 1);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_incremental_or_full(&post, &pre).is_err());
    }

    // ---- Normal pairs that must stay accepted after the Fail upgrades ----

    /// Legitimate Well purchase: the only mid-round writer of the well mirror
    /// writes BOTH ledgers together, so the incremental layer declines (check
    /// 14) and full validation accepts — before and after the fix.
    #[test]
    fn closeout_legit_well_purchase_accepted() {
        for seed in 0u64..64 {
            let mut game = cardless(2, seed);
            let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xBEEF);
            let mut steps = 0;
            while !game.available_spaces().contains(&Space::MajorImprovement)
                && steps < 40
                && !game.public.finished
            {
                let id = game.public.active_player;
                let obs = game.observation(id).unwrap();
                let legal = game.legal_spaces();
                let space = RandomPolicy
                    .select(&obs, &legal, &mut rng)
                    .expect("legal space");
                let action = CardChoiceDetails.produce(&obs, space).expect("detail");
                game.apply(action).expect("random move");
                steps += 1;
            }
            if game.public.finished
                || !game.available_spaces().contains(&Space::MajorImprovement)
                || !game
                    .public
                    .improvement_market
                    .contains(&crate::model::Improvement::Well)
            {
                continue;
            }
            let mut pre = game;
            let actor = pre.public.active_player;
            // Well costs 1 wood + 3 stone (Improvement::cost); the granted
            // supply keeps `pre` full-valid (supply provenance is not part of
            // the full invariant set).
            pre.public.players[actor]
                .supply
                .set(crate::model::Good::Wood, 1);
            pre.public.players[actor]
                .supply
                .set(crate::model::Good::Stone, 3);
            assert_eq!(validate_full(&pre), Ok(()));
            let round = pre.public.round;
            let mut post = pre.clone();
            post.apply(crate::engine::Action {
                space: Space::MajorImprovement,
                detail: crate::engine::Detail::Buy {
                    improvement: crate::model::Improvement::Well,
                    instant_bake: 0,
                },
            })
            .expect("legitimate Well purchase applies");
            assert!(!post.delayed_well.is_empty());
            for r in (round + 1)..=14u8.min(round + 5) {
                assert_eq!(
                    post.delayed_well.get(&r),
                    post.well_food_schedule.get(&r),
                    "the purchase must write both ledgers identically"
                );
            }
            validate_incremental_or_full(&post, &pre)
                .expect("legitimate Well purchase must stay accepted");
            return;
        }
        panic!("no seed produced a legitimate Well purchase window");
    }

    /// Resource-market take-all on a 2p additional-space game: a normal work
    /// move that empties an accumulation pile must stay accepted.
    #[test]
    fn closeout_legit_resource_market_take_all_accepted() {
        let mut game = Game::new(Config {
            players: 2,
            seed: 5,
            additional_action_space_enabled: true,
        });
        let actor = game.public.active_player;
        let pre = game.clone();
        let stone_before = pre.public.players[actor].supply.stone;
        let food_before = pre.public.players[actor].supply.food;
        game.apply(crate::engine::Action {
            space: Space::ResourceMarket,
            detail: crate::engine::Detail::None,
        })
        .expect("take-all applies");
        let post = game;
        assert!(post.public.players[actor].supply.stone > stone_before);
        assert!(post.public.players[actor].supply.food > food_before);
        assert_eq!(post.event_budget, crate::model::DEFAULT_EVENT_BUDGET);
        validate_incremental_or_full(&post, &pre).expect("legitimate take-all must stay accepted");
    }

    /// Event-budget boundaries: the replenish cap (64) and a fully spent
    /// chain (0) both stay accepted — only above-cap is a definite Fail.
    #[test]
    fn closeout_event_budget_zero_and_cap_accepted() {
        let (pre, post) = mid_round_pair();
        assert_eq!(post.event_budget, crate::model::DEFAULT_EVENT_BUDGET);
        validate_incremental_or_full(&post, &pre).unwrap();
        let mut depleted = post;
        depleted.event_budget = 0;
        assert_eq!(validate_incremental(&depleted, &pre), Verdict::Pass);
        validate_incremental_or_full(&depleted, &pre).unwrap();
    }

    // ---- Free-side close-out (PR #358 review, card t_51c3a9e7): the free
    // validator's Pass short-circuits validate_full for the Step 3
    // bookkeeping fields. Each tamper test changes exactly one element on a
    // normal engine-produced free-action (pre, post) pair and asserts the
    // wrapper's FINAL verdict; the validate_full result asserted next to it
    // documents whether the full validator could ever have seen the tamper.
    // The free/isolated sibling of the transition-only close-out block above:
    // execute_free touches no bookkeeping field, so every drift below is
    // unproducible by the only committed free path (apply_free). ----

    /// Drive a cardless game to a plain mid-round committed state and take one
    /// real engine free action (Discard 1 Food), producing the shared
    /// (pre, post, actor) fixture for the free-side close-out tamper tests.
    fn free_action_pair() -> (Game, Game, usize) {
        let (committed, _) = mid_round_pair();
        let pre = committed;
        let actor = pre.public.active_player;
        // Supply provenance is outside the full invariant set (see
        // closeout_legit_well_purchase_accepted), so topping the actor up
        // keeps `pre` full-valid while guaranteeing the discard is legal.
        let mut pre = pre;
        if pre.public.players[actor].supply.food == 0 {
            pre.public.players[actor].supply.food = 1;
        }
        assert_eq!(validate_full(&pre), Ok(()));
        let mut post = pre.clone();
        post.apply_free(
            actor,
            FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("engine free action must apply");
        (pre, post, actor)
    }

    /// Baseline: the engine-produced free transition itself must keep the
    /// fast Pass — the new bookkeeping checks accept every committed free
    /// action unchanged (apply_free replenishes the budget to the cap and
    /// execute_free touches no other bookkeeping field).
    #[test]
    fn closeout_free_committed_transition_stays_pass() {
        let (pre, post, actor) = free_action_pair();
        assert_eq!(validate_free_action(&post, &pre, actor), Verdict::Pass);
        validate_free_action_or_full(&post, &pre, actor).unwrap();
        assert_eq!(validate_full(&post), Ok(()));
        assert_eq!(post.event_budget, crate::model::DEFAULT_EVENT_BUDGET);
    }

    /// Free-side event-budget boundary: a depleted post budget stays
    /// accepted — only above-cap is a definite Fail. The budget is
    /// replenished to the cap, so the free check must NOT demand pre/post
    /// equality (a prior card chain can leave pre legitimately below cap).
    #[test]
    fn closeout_free_event_budget_depleted_accepted() {
        let (pre, mut post, actor) = free_action_pair();
        post.event_budget = 0;
        assert!(validate_free_action_or_full(&post, &pre, actor).is_ok());
    }

    /// Free-side review claim (a): post.event_budget above the replenish cap.
    /// The full validator rejects the same state, but the free validator used
    /// to Pass it straight through to Ok.
    #[test]
    fn closeout_free_event_budget_above_cap_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.event_budget = crate::model::DEFAULT_EVENT_BUDGET + 1;
        assert!(validate_full(&post).is_err());
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "above-cap budget must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Free-side review claim (b): a fabricated key in card_use_limits. No
    /// committed transition writes the limits map, and the full validator
    /// rejects a map that disagrees with the card definitions.
    #[test]
    fn closeout_free_card_use_limits_fabricated_key_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.card_use_limits.insert("ZZ_NOT_A_CARD".into(), 1);
        assert!(validate_full(&post).is_err());
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "card_use_limits drift must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 1, free path: an orphan well_food_schedule entry without the
    /// paired delayed_well change. Full validation accepts any in-range
    /// mirror entry, so only the free transition check can prove it illegal.
    #[test]
    fn closeout_free_well_food_schedule_mirror_orphan_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.well_food_schedule.entry(5).or_default().push(actor);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 2, free path: a suspended decision appearing on a committed
    /// free action. Full validation deliberately does not inspect `pending`.
    #[test]
    fn closeout_free_pending_decision_appearing_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.pending = Some(crate::pending::PendingDecision::new(
            pre.next_decision_id,
            actor,
            crate::pending::PendingKind::ChooseGain {
                source_card: "A001".into(),
                options: vec![crate::pending::GainOption {
                    good: crate::model::Good::Food,
                    amount: 1,
                }],
            },
            post.state_hash(),
            serde_json::json!({}),
        ));
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 3a, free path: a brand-new use-ledger key must reach the full
    /// validator (Pass would short-circuit the ownership check); the
    /// decrease/new-key split mirrors the transition-side semantics exactly.
    #[test]
    fn closeout_free_card_uses_new_unplayed_key_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.card_uses.insert("ZZ_NOT_A_CARD".into(), 1);
        assert!(validate_full(&post).is_err());
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 3a companion, free path: a fabricated ledger key for a card
    /// that IS on someone's table. The full validator accepts this state
    /// (played card, no per-round limit entry), so delegating with Unsure
    /// would let the tamper through — only a definite Fail closes it. On the
    /// free path this is sound: execute_free has no card-chain, and no
    /// committed free action can add a use-ledger key.
    #[test]
    fn closeout_free_card_uses_new_key_on_played_card_rejected() {
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut game = Game::with_cards(
            Config {
                players: 2,
                seed: 29,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("card game");
        let mut rng = ChaCha8Rng::seed_from_u64(0xFEED);
        // Find a committed mid-round state where some card is on a table.
        let mut committed = None;
        for _ in 0..40 {
            let id = game.public.active_player;
            let obs = game.observation(id).unwrap();
            let legal = game.legal_spaces();
            let space = RandomPolicy
                .select(&obs, &legal, &mut rng)
                .expect("legal space");
            let action = CardChoiceDetails.produce(&obs, space).expect("detail");
            game.apply(action).expect("random move");
            if game
                .public
                .players
                .iter()
                .any(|p| !p.played_cards.is_empty())
                && game.pending.is_none()
            {
                committed = Some(game.clone());
                break;
            }
        }
        let pre = committed.expect("a state with a played card on a table");
        let played_id = pre
            .public
            .players
            .iter()
            .flat_map(|p| p.played_cards.iter())
            .next()
            .cloned()
            .expect("played card on some table");
        let actor = pre.public.active_player;
        let mut pre = pre;
        if pre.public.players[actor].supply.food == 0 {
            pre.public.players[actor].supply.food = 1;
        }
        let mut post = pre.clone();
        post.apply_free(
            actor,
            FreeAction::DiscardGood {
                good: crate::model::Good::Food,
                quantity: 1,
            },
        )
        .expect("engine free action must apply");
        assert!(!pre.card_uses.contains_key(&played_id));
        post.card_uses.insert(played_id.clone(), 1);
        // Full validation cannot see this tamper: the card is played and the
        // declared set has no per-round limit for it.
        assert_eq!(validate_full(&post), Ok(()));
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "fabricated ledger key on a played card must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 3b, free path: a same-round ledger decrease re-opens a card's
    /// use budget and no committed free action can produce it.
    #[test]
    fn closeout_free_card_uses_decrease_rejected() {
        let (mut pre, mut post, actor) = free_action_pair();
        pre.card_uses.insert("A001".into(), 2);
        post.card_uses.insert("A001".into(), 1);
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "ledger decrease must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 4, free path: decision-counter drift. Full validation does
    /// not read the counter, so only the free transition check can see it.
    #[test]
    fn closeout_free_next_decision_id_drift_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.next_decision_id += 1;
        assert_eq!(validate_full(&post), Ok(()));
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "next_decision_id drift must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 5, free path: `record_events` is a serde-skipped search flag
    /// no apply/apply_free path writes; a flip between pre and post is not
    /// producible by any committed transition. Full validation cannot see it.
    #[test]
    fn closeout_free_record_events_flip_rejected() {
        let (pre, mut post, actor) = free_action_pair();
        post.record_events = !pre.record_events;
        let verdict = validate_free_action(&post, &pre, actor);
        assert!(
            matches!(verdict, Verdict::Fail(_)),
            "record_events flip must be definite Fail (got {verdict:?})"
        );
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
    }

    /// Element 6, free path: same-round growth of an animal-market
    /// accumulation. Full validation accepts any market-key accumulation,
    /// so only the transition check can prove the growth illegal; taking
    /// (decrease/removal) stays legal on the free path.
    #[test]
    fn closeout_free_animal_accumulation_same_round_growth_rejected() {
        let (mut pre, mut post, actor) = free_action_pair();
        pre.public
            .animal_accumulations
            .insert(Space::SheepMarket, 1);
        post.public
            .animal_accumulations
            .insert(Space::SheepMarket, 2);
        assert_eq!(validate_full(&post), Ok(()));
        assert!(validate_free_action_or_full(&post, &pre, actor).is_err());
        // Taking (decrease/removal) stays legal — the take action removes
        // the key when the market empties.
        post.public.animal_accumulations.remove(&Space::SheepMarket);
        validate_free_action_or_full(&post, &pre, actor).unwrap();
    }
}
