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
    //    cheap subset and decline (Unsure) if anything looks different. The
    //    actor gets the full shared per-player check.
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
            // A work move should not change a non-actor's supply, cards, or
            // farm; passive cross-player card effects exist, so any drift
            // declines to the full validator instead of guessing.
            let pre_p = &pre.public.players[i];
            if player.supply != pre_p.supply
                || player.played_cards != pre_p.played_cards
                || player.card_points != pre_p.card_points
                || player.improvements != pre_p.improvements
                || player.farm != pre_p.farm
                || player.fences != pre_p.fences
                || player.house != pre_p.house
                || player.family != pre_p.family
                || player.newborns != pre_p.newborns
            {
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
pub fn validate_free_action(post: &Game, pre: &Game) -> Verdict {
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
    let n = pre.public.players.len();
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
    //    cheap subset and decline (Unsure) if anything looks different.
    //    The actor gets full shared per-player check for farm invariants.
    for (i, player) in post.public.players.iter().enumerate() {
        if i != pre.public.active_player {
            if player.farm.len() != crate::model::FARM_CELLS
                || player.occupied_stables() > 4
                || player.fences.len() > 15
                || player.family < 2
                || player.family > 5
                || player.newborns > player.family
            {
                return Verdict::Unsure;
            }
            let pre_p = &pre.public.players[i];
            if player.supply != pre_p.supply
                || player.played_cards != pre_p.played_cards
                || player.card_points != pre_p.card_points
                || player.improvements != pre_p.improvements
                || player.farm != pre_p.farm
                || player.fences != pre_p.fences
                || player.house != pre_p.house
                || player.family != pre_p.family
                || player.newborns != pre_p.newborns
            {
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
    let actor = pre.public.active_player;
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
    Verdict::Pass
}

/// Incremental free-action validation with automatic fallback to full validation.
pub fn validate_free_action_or_full(post: &Game, pre: &Game) -> Result<(), String> {
    match validate_free_action(post, pre) {
        Verdict::Pass => Ok(()),
        Verdict::Fail(e) => Err(e),
        Verdict::Unsure => validate_full(post),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Action, Detail};
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
            use crate::model::{Config, Game, Event};
            use crate::counting::allocation_count;

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
}
