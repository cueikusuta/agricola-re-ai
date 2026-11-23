//! A140 ショベル運搬人 (Shovel Bearer): after ClayPit or Hollow, gain food
//! equal to the OTHER clay accumulation's post-action amount.
//!
//! The dispatcher cases pin both directions, zero/used-side/owner boundaries,
//! and player-count eligibility. The engine cases use only recorded public
//! actions: A140 is actually played through Lessons, both clay spaces are
//! used through `Game::apply`, and save/load/replay must not grant twice.

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{
    card_set_for_game, verified_band_a_cards, CardKind, CardSet, CardState,
};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, Player, Space};
use std::collections::{BTreeMap, BTreeSet};

fn set_with_a140() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards()
            .into_iter()
            .filter(|card| card.id == "A140")
            .collect(),
    }
}

fn dispatch(
    set: &CardSet,
    played_by: &[usize],
    actor: usize,
    space: Space,
    accumulations: &BTreeMap<Space, u32>,
) -> Vec<Player> {
    let mut cards = CardState::default();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0), Player::new(2, 0)];
    for &owner in played_by {
        players[owner].played_cards.push("A140".to_string());
    }
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 100;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    let used = BTreeSet::new();
    dispatch_after_worker(
        set,
        actor,
        space,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &used,
        accumulations,
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        false,
    )
    .expect("dispatch_after_worker must succeed");
    players
}

#[test]
fn a140_normal_both_directions_via_dispatch() {
    let set = set_with_a140();

    let accums = BTreeMap::from([(Space::ClayPit, 0), (Space::Hollow, 2)]);
    let players = dispatch(&set, &[0], 0, Space::ClayPit, &accums);
    assert_eq!(players[0].supply.food, 2, "ClayPit reads Hollow");
    assert_eq!(players[0].supply.clay, 0, "the effect grants only food");

    let accums = BTreeMap::from([(Space::ClayPit, 3), (Space::Hollow, 0)]);
    let players = dispatch(&set, &[0], 0, Space::Hollow, &accums);
    assert_eq!(players[0].supply.food, 3, "Hollow reads ClayPit");
    assert_eq!(players[0].supply.clay, 0);
}

#[test]
fn a140_empty_other_accumulation_grants_zero() {
    let set = set_with_a140();
    let accums = BTreeMap::from([(Space::ClayPit, 0), (Space::Hollow, 0)]);
    let players = dispatch(&set, &[0], 0, Space::Hollow, &accums);
    assert_eq!(players[0].supply.food, 0);
    for good in [
        Good::Wood,
        Good::Clay,
        Good::Reed,
        Good::Stone,
        Good::Grain,
        Good::Vegetable,
    ] {
        assert_eq!(players[0].supply.get(good), 0, "{good:?} must be untouched");
    }

    let players = dispatch(&set, &[0], 0, Space::ClayPit, &BTreeMap::new());
    assert_eq!(players[0].supply.food, 0, "a missing entry is also zero");
}

#[test]
fn a140_never_reads_the_used_side_leftover() {
    let set = set_with_a140();
    let accums = BTreeMap::from([(Space::ClayPit, 5), (Space::Hollow, 0)]);
    let players = dispatch(&set, &[0], 0, Space::ClayPit, &accums);
    assert_eq!(players[0].supply.food, 0, "used-side clay is not food");

    let accums = BTreeMap::from([(Space::ClayPit, 0), (Space::Hollow, 4)]);
    let players = dispatch(&set, &[0], 0, Space::Hollow, &accums);
    assert_eq!(
        players[0].supply.food, 0,
        "mirror used-side clay is not food"
    );
}

#[test]
fn a140_only_owner_and_only_matching_space_benefit() {
    let set = set_with_a140();
    let accums = BTreeMap::from([(Space::ClayPit, 2), (Space::Hollow, 3)]);

    let players = dispatch(&set, &[0, 1], 0, Space::ClayPit, &accums);
    assert_eq!(players[0].supply.food, 3);
    assert_eq!(players[1].supply.food, 0, "non-actor owner gets nothing");
    assert_eq!(players[2].supply.food, 0, "non-owner gets nothing");

    let players = dispatch(&set, &[0], 0, Space::Forest, &accums);
    assert_eq!(
        players[0].supply.food, 0,
        "unrelated actions do not trigger"
    );
}

#[test]
fn a140_pool_eligibility_is_two_no_three_and_four_yes() {
    assert!(card_set_for_game(2, true).definition("A140").is_none());
    assert!(card_set_for_game(3, true).definition("A140").is_some());
    assert!(card_set_for_game(4, true).definition("A140").is_some());
}

fn hands_with_a140_in_seat0(set: &CardSet, players: usize) -> Vec<Vec<String>> {
    let mut occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation && card.id != "A140")
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() + 1 >= 7 * players);
    assert!(minors.len() >= 7 * players);
    occupations.insert(0, "A140".to_string());

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

/// Play A140 and use ClayPit in round 1, then Hollow in round 2. Every setup
/// step is an ordinary logged action, so the final save and replay share the
/// exact same initial hands and public accumulation history.
fn run_engine_directions_and_replay(players: usize, additional_spaces: bool, seed: u64) {
    let set = card_set_for_game(players, true);
    let hands = hands_with_a140_in_seat0(&set, players);
    let config = Config {
        seed,
        additional_action_space_enabled: additional_spaces,
        players,
    };
    let mut game = Game::with_cards_and_hands_with_start_player(config, set, hands, 0)
        .expect("valid explicit A140 game");

    apply(
        &mut game,
        Space::Lessons,
        Detail::PlayOccupation {
            card_id: "A140".to_string(),
        },
    );
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "A140"));

    for space in [Space::Forest, Space::ReedBank, Space::Grove]
        .into_iter()
        .take(players - 1)
    {
        apply(&mut game, space, Detail::None);
    }

    let food_before = game.public.players[0].supply.food;
    let other_food_before: Vec<u32> = game.public.players[1..]
        .iter()
        .map(|player| player.supply.food)
        .collect();
    let expected = game
        .public
        .accumulations
        .get(&Space::Hollow)
        .copied()
        .unwrap_or(0);
    assert!(expected > 0, "round-1 Hollow must hold clay");
    apply(&mut game, Space::ClayPit, Detail::None);
    assert_eq!(game.public.players[0].supply.food, food_before + expected);
    assert!(!game.public.accumulations.contains_key(&Space::ClayPit));
    assert_eq!(
        game.public.accumulations.get(&Space::Hollow).copied(),
        Some(expected),
        "A140 reads but never consumes the other accumulation"
    );
    for (actual, before) in game.public.players[1..]
        .iter()
        .map(|player| player.supply.food)
        .zip(other_food_before)
    {
        assert_eq!(actual, before, "A140 must benefit only its owner");
    }

    for space in [Space::Fishing, Space::GrainSeeds, Space::DayLaborer]
        .into_iter()
        .take(players - 1)
    {
        apply(&mut game, space, Detail::None);
    }
    assert_eq!(game.public.round, 2);
    assert_eq!(game.public.active_player, 0);

    let food_before = game.public.players[0].supply.food;
    let expected = game
        .public
        .accumulations
        .get(&Space::ClayPit)
        .copied()
        .unwrap_or(0);
    assert!(expected > 0, "round-2 ClayPit must have refilled");
    apply(&mut game, Space::Hollow, Detail::None);
    assert_eq!(game.public.players[0].supply.food, food_before + expected);
    assert!(!game.public.accumulations.contains_key(&Space::Hollow));
    assert_eq!(
        game.public.accumulations.get(&Space::ClayPit).copied(),
        Some(expected)
    );

    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;
    let loaded = Game::load_json(&game.save_json().expect("save")).expect("load");
    assert_eq!(loaded.state_hash(), final_hash);
    assert_eq!(loaded.public.players[0].supply.food, final_food);

    let replayed = Game::replay_with_setup_and_start_player(
        loaded.config.clone(),
        (*loaded.card_set).clone(),
        loaded.log.initial_hands.clone(),
        loaded.log.initial_start_player_override,
        &loaded.log.events,
    )
    .expect("logged actions replay");
    assert_eq!(replayed.state_hash(), final_hash);
    assert_eq!(replayed.public, loaded.public);
    assert_eq!(replayed.public.players[0].supply.food, final_food);
}

#[test]
fn a140_engine_three_players_additional_off() {
    run_engine_directions_and_replay(3, false, 0xC0FFEE);
}

#[test]
fn a140_engine_three_players_additional_on() {
    run_engine_directions_and_replay(3, true, 0xBADF00D);
}

#[test]
fn a140_engine_four_players_additional_off() {
    run_engine_directions_and_replay(4, false, 0xDEADBEEF);
}

#[test]
fn a140_engine_four_players_additional_on() {
    run_engine_directions_and_replay(4, true, 0xFEEDFACE);
}
