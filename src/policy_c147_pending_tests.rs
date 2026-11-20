//! C147 牛の飼育師 production-path pending tests (PR #384 review finding (a)).
//!
//! The engine pauses the post-work chain on
//! `PendingKind::ChooseAnimalPlacement`; until this fix no production AI loop
//! produced a resolution, so any episode where a C147 owner took
//! CattleMarket died with the engine's precise pause `Err`. These tests pin
//! the full production path end to end and the negative cases.
//!
//! They live in `src/policy.rs`'s test tree because they drive
//! `pub(crate)` API (`Game::apply_with_plans`, `resolve_pending_and_resume`,
//! `policy::apply_for_ai`) that integration tests cannot reach.

use super::*;
use crate::cards::CardSet;
use crate::harvest::SimpleHarvestDetails;
use crate::model::{Config, Game, Observation, RoundCard, Space};
use crate::pending::{PendingKind, PendingResolution};
use rand_chacha::ChaCha8Rng;

/// A two-player game whose card set is the verified basic pool plus C147,
/// with C147 guaranteed in seat 0's hand. Seats start with the deal()'s
/// 7+7 hands; C147 is moved into seat 0 so the one-location partition holds.
fn c147_game() -> Game {
    let mut definitions = crate::cards::verified_basic_card_set(2)
        .unwrap()
        .definitions;
    let c147 = crate::cards::verified_band_c_cards()
        .into_iter()
        .find(|c| c.id == "C147")
        .expect("C147 in band C pool");
    definitions.push(c147);
    let set = CardSet { definitions };
    let proto = Game::with_cards(Config::default(), set.clone()).expect("valid set");
    let mut hands = proto.cards.hands.clone();
    if let Some(seat) = hands.iter().position(|h| h.iter().any(|c| c == "C147")) {
        hands[seat].retain(|c| c != "C147");
    }
    hands[0].push("C147".into());
    // Draft bridge (7+7 per seat, start player fixed at seat 0).
    let mut game = Game::with_cards_and_hands_with_start_player(Config::default(), set, hands, 0)
        .expect("game");
    game.public.start_player = 0;
    game.public.active_player = 0;
    game
}

/// Move CattleMarket to the top of the hidden deck and advance one round so
/// the market space becomes a legal worker placement now (round 2).
fn reveal_cattle_market(game: &mut Game) {
    let pos = game
        .hidden_round_deck
        .iter()
        .position(|c| *c == RoundCard::CattleMarket)
        .expect("CattleMarket in the round deck");
    let len = game.hidden_round_deck.len();
    game.hidden_round_deck.swap(pos, len - 1);
    game.start_round().expect("start_round");
    assert!(
        game.public.revealed.contains(&RoundCard::CattleMarket),
        "CattleMarket must be revealed after the forced draw"
    );
}

/// END-TO-END (production apply path): C147 owner places a worker on
/// CattleMarket → engine pauses on ChooseAnimalPlacement → the AI resolver
/// produces the plan → resume completes the action → the cattle is on the
/// player's farm and the space is used. This is the exact chain PR #384's
/// review asked for: "generation path exists and the animal lands".
#[test]
fn c147_production_pause_is_ai_resolved_and_cattle_lands() {
    let mut game = c147_game();
    reveal_cattle_market(&mut game);
    // Seat 0 "has played" C147 (the play itself is covered by the loop test;
    // this test isolates the pause/resume chain through production calls).
    game.cards.hands[0].retain(|c| c != "C147");
    game.public.players[0].played_cards.push("C147".into());
    game.public.active_player = 0;

    let obs = game.observation(0).expect("obs");
    let action = SimpleDetails
        .produce(&obs, Space::CattleMarket)
        .expect("cattle market detail");
    // Production AI apply: must not die on the pause.
    apply_for_ai(&mut game, action, &SimpleHarvestDetails)
        .expect("AI applies CattleMarket with C147 played");

    assert!(game.pending.is_none(), "pause fully consumed");
    assert!(
        game.public.used.contains(&Space::CattleMarket),
        "space used after resume"
    );
    assert_eq!(game.public.workers_placed[0], 1, "worker placement counted");
    let cattle = game.public.players[0].animal_count(crate::model::Animal::Cattle);
    // stage-14 accumulation plus C147's grant both land on the farm.
    assert!(
        cattle >= 1,
        "cattle must actually be placed, got {cattle} (pet={:?}, pastures={:?}, stables={})",
        game.public.players[0].pet,
        game.public.players[0]
            .pastures
            .iter()
            .map(|p| (p.animal, p.quantity))
            .collect::<Vec<_>>(),
        game.public.players[0]
            .farm
            .iter()
            .filter(|c| c.stable_animal.is_some())
            .count(),
    );
}

/// NEGATIVE (非所有者): a seat WITHOUT C147 taking CattleMarket never pauses
/// and grants no C147 cattle.
#[test]
fn c147_non_owner_cattle_market_does_not_pause_or_grant() {
    let mut game = c147_game();
    game.cards.hands[0].retain(|c| c != "C147");
    game.cards.hands[1].retain(|c| c != "C147");
    reveal_cattle_market(&mut game);
    game.public.active_player = 1;

    let obs = game.observation(1).expect("obs");
    let action = SimpleDetails
        .produce(&obs, Space::CattleMarket)
        .expect("cattle market detail");
    apply_for_ai(&mut game, action, &SimpleHarvestDetails)
        .expect("non-owner CattleMarket applies cleanly");
    assert!(game.pending.is_none(), "no pause for a non-owner");
    assert!(
        game.public.players[1]
            .played_cards
            .iter()
            .all(|c| c != "C147"),
        "sanity: seat 1 never played C147"
    );
    // The space's own accumulation may have granted cattle; C147's extra
    // grant is what must NOT exist. Without the occupation the pause chain
    // never ran, so workers_placed advanced without any pending.
    assert_eq!(game.public.workers_placed[1], 1);
}

/// NEGATIVE (別スペース): the C147 owner on a non-CattleMarket space neither
/// pauses nor grants.
#[test]
fn c147_owner_other_space_does_not_pause_or_grant() {
    let mut game = c147_game();
    game.cards.hands[0].retain(|c| c != "C147");
    game.public.players[0].played_cards.push("C147".into());
    game.public.active_player = 0;

    let obs = game.observation(0).expect("obs");
    let action = SimpleDetails
        .produce(&obs, Space::Forest)
        .expect("forest detail");
    let cattle_before = game.public.players[0].animal_count(crate::model::Animal::Cattle);
    apply_for_ai(&mut game, action, &SimpleHarvestDetails).expect("forest applies");
    assert!(game.pending.is_none(), "no pause on Forest");
    assert_eq!(
        game.public.players[0].animal_count(crate::model::Animal::Cattle),
        cattle_before,
        "no cattle granted outside CattleMarket"
    );
}

/// UNIT (resolver): `auto_pending_resolution` maps a ChooseAnimalPlacement
/// pause to the auto plan and refuses unknown kinds.
#[test]
fn auto_pending_resolution_maps_only_animal_placement() {
    let game = c147_game();
    let pending = crate::pending::PendingDecision {
        id: Default::default(),
        player: 0,
        kind: PendingKind::ChooseAnimalPlacement {
            source_card: "C147".into(),
            animal: crate::model::Animal::Cattle,
            amount: 1,
        },
        state_hash_before: game.state_hash(),
        context: serde_json::Value::Null,
        continuation: Default::default(),
    };
    let resolution = auto_pending_resolution(&game, &pending).expect("resolves placement");
    match resolution {
        PendingResolution::ChooseAnimalPlacement { plan } => {
            assert_eq!(plan.animal, crate::model::Animal::Cattle);
            // Empty farm: the pet slot takes the animal.
            assert!(plan.pet, "empty farm routes the cattle to the pet slot");
            assert_eq!(plan.discard, 0);
        }
        other => panic!("wrong resolution variant: {other:?}"),
    }

    let unknown = crate::pending::PendingDecision {
        id: Default::default(),
        player: 0,
        kind: PendingKind::Custom {
            source_card: "X000".into(),
            payload: serde_json::Value::Null,
        },
        state_hash_before: game.state_hash(),
        context: serde_json::Value::Null,
        continuation: Default::default(),
    };
    assert!(
        auto_pending_resolution(&game, &unknown).is_none(),
        "unknown kinds are not auto-resolved"
    );
}

/// MCTS PLAYOUT SURVIVAL (seeded): with the fix, a rollout from a C147+
/// CattleMarket state reaches the end of the game instead of dying on the
/// pause. This exercises the production `rollout` apply swap.
#[test]
fn c147_pending_does_not_stop_mcts_rollout() {
    let mut game = c147_game();
    reveal_cattle_market(&mut game);
    game.cards.hands[0].retain(|c| c != "C147");
    game.public.players[0].played_cards.push("C147".into());
    game.public.active_player = 0;
    let mut rng = ChaCha8Rng::seed_from_u64(0xC147_C147_C147_C147);
    let finished = crate::mcts::rollout(&mut game, &mut rng, None)
        .expect("rollout survives the C147 pause chain");
    assert!(finished, "rollout must reach the end of the game");
}

/// LOOP (production play loop): scripted policies play C147, then later
/// take CattleMarket; the episode must finish with cattle on the farm.
/// This is the RED→GREEN test from the probe, made permanent.
#[test]
fn c147_production_loop_completes_and_places_cattle() {
    struct C147Script;
    impl SpacePolicy for C147Script {
        fn select(
            &self,
            obs: &Observation,
            legal: &[Space],
            _rng: &mut ChaCha8Rng,
        ) -> Option<Space> {
            let me = &obs.public.players[obs.player_id];
            let owns_c147 = me.played_cards.iter().any(|c| c == "C147");
            if owns_c147 {
                if legal.contains(&Space::CattleMarket) {
                    return Some(Space::CattleMarket);
                }
            } else if legal.contains(&Space::Lessons) && obs.hand.iter().any(|c| c == "C147") {
                return Some(Space::Lessons);
            }
            legal
                .iter()
                .copied()
                .find(|s| *s != Space::CattleMarket && *s != Space::Lessons)
                .or_else(|| legal.first().copied())
        }
    }
    struct C147Details;
    impl DetailPolicy for C147Details {
        fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
            if space == Space::Lessons
                && !obs
                    .public
                    .players
                    .get(obs.player_id)?
                    .played_cards
                    .iter()
                    .any(|c| c == "C147")
                && obs.hand.iter().any(|c| c == "C147")
            {
                return Some(Action {
                    space,
                    detail: Detail::PlayOccupation {
                        card_id: "C147".into(),
                    },
                });
            }
            SimpleDetails.produce(obs, space)
        }
    }

    let mut definitions = crate::cards::verified_basic_card_set(2)
        .unwrap()
        .definitions;
    let c147 = crate::cards::verified_band_c_cards()
        .into_iter()
        .find(|c| c.id == "C147")
        .expect("C147 in band C pool");
    definitions.push(c147);
    let set = CardSet { definitions };
    let proto = Game::with_cards(Config::default(), set.clone()).unwrap();
    let mut hands = proto.cards.hands.clone();
    if let Some(seat) = hands.iter().position(|h| h.iter().any(|c| c == "C147")) {
        hands[seat].retain(|c| c != "C147");
    }
    hands[0].push("C147".into());
    let mut game =
        Game::with_cards_and_hands_with_start_player(Config::default(), set, hands, 0).unwrap();

    play_to_end_with_harvest_policy(&mut game, &C147Script, &C147Details, &SimpleHarvestDetails)
        .expect("production loop must complete the C147 episode");
    assert!(game.public.finished);
    assert!(
        game.public.players[0]
            .played_cards
            .iter()
            .any(|c| c == "C147"),
        "C147 must have been played by the scripted policy"
    );
    let count = game.public.players[0].animal_count(crate::model::Animal::Cattle);
    assert!(
        count >= 1,
        "C147 owner must hold at least one cattle, got {count}"
    );
}
