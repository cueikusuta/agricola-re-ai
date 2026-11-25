//! B143 粘土守 (Clay Warden): on another player's Hollow use, each non-actor
//! B143 owner gains clay 1; in a 3p game each such owner gains one more clay
//! (clay 2 total); in a 4p game each such owner gains clay 1 + food 1. The
//! actor never gains. Owner's own Hollow use gives no bonus. 2p ineligible.
//!
//! Dispatcher cases pin the per-player-count bonus, the actor-exclusion,
//! multiple owners, and the own-use boundary. Engine cases pin that the
//! normal Hollow gain (actor) and the B143 bonus (owner) are distinct, and
//! that save/load/replay does not grant twice.

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{card_set_for_game, verified_band_b_cards, CardSet, CardState};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, Player, Space};
use std::collections::{BTreeMap, BTreeSet};

fn set_with_b143() -> CardSet {
    CardSet {
        definitions: verified_band_b_cards()
            .into_iter()
            .filter(|card| card.id == "B143")
            .collect(),
    }
}

fn dispatch(
    set: &CardSet,
    played_by: &[usize],
    actor: usize,
    space: Space,
    players: usize,
) -> Vec<Player> {
    let mut cards = CardState::default();
    let mut ps: Vec<Player> = (0..players).map(|i| Player::new(i, 0)).collect();
    for &owner in played_by {
        ps[owner].played_cards.push("B143".to_string());
    }
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 100;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    let used = BTreeSet::new();
    let mut accumulations = BTreeMap::new();
    let animal_accumulations = BTreeMap::new();
    dispatch_after_worker(
        set,
        actor,
        space,
        &mut cards,
        &mut ps,
        &mut market,
        &mut well,
        1,
        &used,
        &mut accumulations,
        &animal_accumulations,
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
        &mut std::collections::BTreeMap::new(),
    )
    .expect("dispatch_after_worker must succeed");
    ps
}

#[test]
fn b143_3p_other_uses_hollow_owner_gains_clay_two() {
    // 3p: actor seat 1 uses Hollow, owner seat 0 (non-actor) gains clay 2.
    let set = set_with_b143();
    let players = dispatch(&set, &[0], 1, Space::Hollow, 3);
    assert_eq!(
        players[0].supply.get(Good::Clay),
        2,
        "3p non-actor owner gains clay 2"
    );
    assert_eq!(players[1].supply.get(Good::Clay), 0, "actor gains no bonus");
    assert_eq!(
        players[2].supply.get(Good::Clay),
        0,
        "non-owner gains nothing"
    );
}

#[test]
fn b143_4p_other_uses_hollow_owner_gains_clay_and_food() {
    // 4p: actor seat 1 uses Hollow, owner seat 0 (non-actor) gains clay 1 + food 1.
    let set = set_with_b143();
    let players = dispatch(&set, &[0], 1, Space::Hollow, 4);
    assert_eq!(
        players[0].supply.get(Good::Clay),
        1,
        "4p owner gains clay 1"
    );
    assert_eq!(
        players[0].supply.get(Good::Food),
        1,
        "4p owner gains food 1"
    );
    assert_eq!(players[1].supply.get(Good::Clay), 0, "actor gains no bonus");
}

#[test]
fn b143_owner_own_hollow_use_grants_nothing() {
    // The card owner is also the actor -> no bonus.
    let set = set_with_b143();
    let players = dispatch(&set, &[0], 0, Space::Hollow, 4);
    assert_eq!(players[0].supply.get(Good::Clay), 0, "own use: no clay");
    assert_eq!(players[0].supply.get(Good::Food), 0, "own use: no food");
}

#[test]
fn b143_multiple_owners_all_non_actor_gain() {
    // 3p: every seat owns B143; only the non-actor owners gain clay 2.
    let set = set_with_b143();
    let players = dispatch(&set, &[0, 1, 2], 1, Space::Hollow, 3);
    assert_eq!(players[0].supply.get(Good::Clay), 2, "owner 0 gains clay 2");
    assert_eq!(players[2].supply.get(Good::Clay), 2, "owner 2 gains clay 2");
    assert_eq!(
        players[1].supply.get(Good::Clay),
        0,
        "actor is excluded even though they also own B143"
    );
}

#[test]
fn b143_only_hollow_triggers() {
    let set = set_with_b143();
    let players = dispatch(&set, &[0], 1, Space::ClayPit, 3);
    assert_eq!(
        players[0].supply.get(Good::Clay),
        0,
        "non-Hollow space does not trigger"
    );
}

#[test]
fn b143_pool_eligibility_three_and_four_yes_two_no() {
    assert!(card_set_for_game(2, true).definition("B143").is_none());
    assert!(card_set_for_game(3, true).definition("B143").is_some());
    assert!(card_set_for_game(4, true).definition("B143").is_some());
}

fn hands_with_b143_in_seat0(set: &CardSet, players: usize) -> Vec<Vec<String>> {
    let mut occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == agricola_re_ai::cards::CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == agricola_re_ai::cards::CardKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect();
    assert!(
        occupations.len() + 1 >= 7 * players,
        "enough occupations to deal 7 per player plus B143"
    );
    assert!(
        minors.len() >= 7 * players,
        "enough minors to deal 7 per player"
    );
    occupations.insert(0, "B143".to_string());
    let mut hands = vec![Vec::new(); players];
    let mut occ_cursor = 0;
    let mut minor_cursor = 0;
    for hand in &mut hands {
        hand.extend_from_slice(&occupations[occ_cursor..occ_cursor + 7]);
        occ_cursor += 7;
    }
    for hand in &mut hands {
        hand.extend_from_slice(&minors[minor_cursor..minor_cursor + 7]);
        minor_cursor += 7;
    }
    hands
}

fn apply(game: &mut Game, space: Space, detail: Detail) {
    let player = game.public.active_player;
    let round = game.public.round;
    game.apply(Action { space, detail })
        .unwrap_or_else(|e| panic!("round {round} player {player} {space:?} must be legal: {e}"));
}

fn run_engine(players: usize, seed: u64) -> Game {
    let set = card_set_for_game(players, true);
    let hands = hands_with_b143_in_seat0(&set, players);
    let config = Config {
        seed,
        additional_action_space_enabled: false,
        players,
    };
    Game::with_cards_and_hands_with_start_player(config, set, hands, 0)
        .expect("valid explicit B143 game")
}

#[test]
fn b143_engine_3p_normal_hollow_gain_vs_bonus() {
    let mut game = run_engine(3, 0xC0FFEE);
    apply(
        &mut game,
        Space::Lessons,
        Detail::PlayOccupation {
            card_id: "B143".to_string(),
        },
    );
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B143"));
    let owner_clay_before = game.public.players[0].supply.get(Good::Clay);
    let actor_clay_before = game.public.players[1].supply.get(Good::Clay);
    let hollow_accum_before = game
        .public
        .accumulations
        .get(&Space::Hollow)
        .copied()
        .unwrap_or(0);
    // Seat 1 (actor) uses Hollow: normal Hollow gain is the accumulated clay.
    apply(&mut game, Space::Hollow, Detail::None);
    assert_eq!(
        game.public.players[1].supply.get(Good::Clay),
        actor_clay_before + hollow_accum_before,
        "actor gets normal Hollow clay"
    );
    assert_eq!(
        game.public.players[0].supply.get(Good::Clay),
        owner_clay_before + 2,
        "3p non-actor owner gets B143 bonus clay 2"
    );
}

#[test]
fn b143_engine_4p_normal_hollow_gain_vs_bonus() {
    let mut game = run_engine(4, 0xC1FFEE);
    apply(
        &mut game,
        Space::Lessons,
        Detail::PlayOccupation {
            card_id: "B143".to_string(),
        },
    );
    let owner_clay_before = game.public.players[0].supply.get(Good::Clay);
    let owner_food_before = game.public.players[0].supply.get(Good::Food);
    let actor_clay_before = game.public.players[1].supply.get(Good::Clay);
    let hollow_accum_before = game
        .public
        .accumulations
        .get(&Space::Hollow)
        .copied()
        .unwrap_or(0);
    // Seat 1 (actor) uses Hollow: normal Hollow gain is the accumulated clay.
    apply(&mut game, Space::Hollow, Detail::None);
    assert_eq!(
        game.public.players[1].supply.get(Good::Clay),
        actor_clay_before + hollow_accum_before,
        "actor gets normal Hollow clay"
    );
    assert_eq!(
        game.public.players[0].supply.get(Good::Clay),
        owner_clay_before + 1,
        "4p non-actor owner gets B143 bonus clay 1"
    );
    assert_eq!(
        game.public.players[0].supply.get(Good::Food),
        owner_food_before + 1,
        "4p non-actor owner gets B143 bonus food 1"
    );
}

#[test]
fn b143_engine_replay_no_double_grant() {
    let mut game = run_engine(3, 0xDEADBEEF);
    apply(
        &mut game,
        Space::Lessons,
        Detail::PlayOccupation {
            card_id: "B143".to_string(),
        },
    );
    apply(&mut game, Space::Hollow, Detail::None);
    let final_hash = game.state_hash();
    let owner_clay = game.public.players[0].supply.get(Good::Clay);
    let loaded = Game::load_json(&game.save_json().expect("save")).expect("load");
    assert_eq!(loaded.state_hash(), final_hash, "save/load preserves state");
    assert_eq!(
        loaded.public.players[0].supply.get(Good::Clay),
        owner_clay,
        "save/load does not double-grant"
    );
    let replayed = Game::replay_with_setup_and_start_player(
        loaded.config.clone(),
        (*loaded.card_set).clone(),
        loaded.log.initial_hands.clone(),
        loaded.log.initial_start_player_override,
        &loaded.log.events,
    )
    .expect("logged actions replay");
    assert_eq!(replayed.state_hash(), final_hash, "replay reproduces state");
    assert_eq!(
        replayed.public.players[0].supply.get(Good::Clay),
        owner_clay,
        "replay does not double-grant"
    );
}
