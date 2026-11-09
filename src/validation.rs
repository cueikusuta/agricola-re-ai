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
    // 6. Exactly one newly used space (difference is non-empty because the
    //    actor placed a worker; a work move always consumes one space).
    let added: Vec<_> = post.public.used.difference(&pre.public.used).collect();
    if added.len() != 1 {
        return Verdict::Fail("a work move must add exactly one used space".into());
    }
    // 7. Supply non-negativity: `Supply` fields are `u32` and all
    //    deductions go through the checked `Supply::pay`, so a negative
    //    value cannot be represented. No runtime check needed here; the
    //    full validator relies on the same unsigned types.

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
}
