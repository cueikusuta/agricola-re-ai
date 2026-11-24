//! A057 搾乳施設 (Milking Parlor): cost wood 2, printed 1 VP, prerequisite
//! >= 4 unused farmyard spaces. On play, per animal type the highest reached
//! > threshold stage pays exactly once 2014 stages never stack and sheep/cattle are
//! > never mixed: sheep 1/3/4+ -> 2/3/4 food, cattle 1/2/3+ -> 2/3/4 food.
//!
//! Covered per data/bga-card-tests/A057.json:
//! - A057-normal: sheep 3 + cattle 2 -> food 6
//! - A057-boundary: sheep 1 + cattle 1 -> food 4
//! - A057-zero: sheep 0 + cattle 0 -> food 0
//!
//! Plus prerequisite gating, stage non-stacking, the dispatcher route
//! (card_events::dispatch_play), the engine route (Game::apply via
//! MeetingPlace), and save/load/replay integrity (the on-play grant fires
//! exactly once and survives resume/replay).

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, can_play, verified_band_a_cards, verified_basic_card_set,
    CardDefinition, CardEffect, CardKind, CardRequirement, CardSet, CardState,
};
use agricola_re_ai::engine::{Action, AnimalPlan, Detail};
use agricola_re_ai::model::{Animal, Cell, Config, Game, Good, Pasture, Player, Space, Tile};
use agricola_re_ai::registry::build_default_registry;
use std::collections::{BTreeMap, BTreeSet};

fn set_with_a057() -> CardSet {
    CardSet {
        definitions: verified_band_a_cards()
            .into_iter()
            .filter(|card| card.id == "A057")
            .collect(),
    }
}

fn a057_definition() -> CardDefinition {
    set_with_a057()
        .definitions
        .into_iter()
        .next()
        .expect("A057 must exist in band A")
}

fn player_with_farm(wood: u32, cells: &[(usize, Tile)]) -> Player {
    let mut player = Player::new(0, 10);
    player.supply.wood = wood;
    for &(cell, tile) in cells {
        player.farm[cell] = Cell {
            tile,
            stable: false,
            stable_animal: None,
            field_stones: 0,
        };
    }
    player
}

fn pasture(id: u8, animal: Animal, quantity: u32) -> Pasture {
    Pasture {
        id,
        cells: vec![],
        animal: Some(animal),
        quantity,
    }
}

// ---------------- Registry reachability ----------------

#[test]
fn a057_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("A057").expect("A057 must be registered");
    let definition = registry
        .get(card_id)
        .expect("registered A057 resolves")
        .to_card_definition();
    assert_eq!(
        definition.kind,
        CardKind::MinorImprovement,
        "A057 is a minor improvement"
    );
    assert_eq!(definition.cost, vec![(Good::Wood, 2)], "cost wood 2");
    assert_eq!(definition.points, 1, "printed 1 VP");

    let mut prerequisite = false;
    let mut sheep_stages = 0usize;
    let mut cattle_stages = 0usize;
    for effect in &definition.effects {
        match effect {
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumUnusedFarmyardSpaces(4),
            } => prerequisite = true,
            CardEffect::OnPlayGainAnimalThreshold {
                animal: Animal::Sheep,
                ..
            } => sheep_stages += 1,
            CardEffect::OnPlayGainAnimalThreshold {
                animal: Animal::Cattle,
                ..
            } => cattle_stages += 1,
            _ => {}
        }
    }
    assert!(prerequisite, "prerequisite: >= 4 unused farmyard spaces");
    assert_eq!(sheep_stages, 1, "one sheep threshold table");
    assert_eq!(cattle_stages, 1, "one cattle threshold table");
}

// ---------------- Prerequisite gating (cards::can_play) ----------------

#[test]
fn a057_prerequisite_unused_farmyard_spaces() {
    let set = CardSet {
        definitions: vec![a057_definition()],
    };
    let card = &a057_definition();

    // Fresh farm: 2 rooms + 13 empty cells -> 13 unused spaces, cost payable.
    assert!(
        can_play(&set, card, &player_with_farm(2, &[]), 0),
        "13 unused spaces and wood 2 -> playable"
    );
    // Cost unmet.
    assert!(
        !can_play(&set, card, &player_with_farm(1, &[]), 0),
        "wood 1 -> not playable"
    );
    // 11 rooms leaves 3 unused spaces -> prerequisite unmet.
    let rooms: Vec<(usize, Tile)> = (1..=11).map(|i| (i, Tile::Room)).collect();
    assert!(
        !can_play(&set, card, &player_with_farm(2, &rooms), 0),
        "3 unused spaces -> not playable"
    );
    // Four empty spaces with no stable -> playable.
    let empties: Vec<(usize, Tile)> = (1..=4).map(|i| (i, Tile::Empty)).collect();
    assert!(
        can_play(&set, card, &player_with_farm(2, &empties), 0),
        "4 unused spaces -> playable"
    );
    // A stable on an empty space counts as used: with rooms everywhere else,
    // four stables on the only four empty cells leave 0 unused spaces.
    let mut player = player_with_farm(2, &empties);
    for cell in player.farm.iter_mut().take(15).skip(6) {
        *cell = Cell {
            tile: Tile::Room,
            stable: false,
            stable_animal: None,
            field_stones: 0,
        };
    }
    for &cell in &[1, 2, 3, 4] {
        player.farm[cell].stable = true;
    }
    assert!(
        !can_play(&set, card, &player, 0),
        "4 empties all with stables and the rest rooms -> 0 unused -> not playable"
    );
}

// ---------------- Effect-only play (cards::apply_on_play_effects) ----------------

fn apply_a057(set: &CardSet, sheep: u32, cattle: u32) -> u32 {
    let mut player = Player::new(0, 0);
    player.pastures = vec![
        pasture(0, Animal::Sheep, sheep),
        pasture(1, Animal::Cattle, cattle),
    ];
    apply_on_play_effects(set, &mut player, "A057").expect("apply A057");
    player.supply.food
}

#[test]
fn a057_normal_sheep3_cattle2_food6() {
    let set = set_with_a057();
    assert_eq!(
        apply_a057(&set, 3, 2),
        6,
        "A057-normal: sheep 3 -> 3 food, cattle 2 -> 3 food"
    );
}

#[test]
fn a057_boundary_sheep1_cattle1_food4() {
    let set = set_with_a057();
    assert_eq!(
        apply_a057(&set, 1, 1),
        4,
        "A057-boundary: sheep 1 -> 2 food, cattle 1 -> 2 food"
    );
}

#[test]
fn a057_zero_food0() {
    let set = set_with_a057();
    assert_eq!(
        apply_a057(&set, 0, 0),
        0,
        "A057-zero: no animals -> no food"
    );
}

#[test]
fn a057_stages_do_not_stack() {
    let set = set_with_a057();
    assert_eq!(
        apply_a057(&set, 4, 0),
        4,
        "sheep 4 pays the highest stage only (4), never 2+3+4"
    );
    assert_eq!(
        apply_a057(&set, 0, 3),
        4,
        "cattle 3 pays the highest stage only (4)"
    );
    assert_eq!(apply_a057(&set, 2, 0), 2, "sheep 2 pays stage 1 (2)");
    assert_eq!(apply_a057(&set, 0, 2), 3, "cattle 2 pays stage 2 (3)");
}

// ---------------- Dispatcher route (card_events::dispatch_play) ----------------

fn dispatch_card(set: &CardSet, player: &mut Player, id: &str) {
    let card = set.definition(id).expect("card missing");
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|e| panic!("pay {id} cost: {e}"));
    }
    let mut cards = CardState {
        hands: vec![vec![id.to_owned()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    // Caller-side zone move: the card is already in played_cards when
    // dispatch_play runs (see card_events dispatch_play Phase 3).
    players[0].played_cards.push(id.to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        id,
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        true, // is_improvement_action: tests simulate the improvement action path
    )
    .unwrap_or_else(|e| panic!("dispatch_play {id}: {e}"));
    *player = players[0].clone();
}

#[test]
fn a057_dispatcher_route_sheep3_cattle2_food6() {
    let set = set_with_a057();
    let mut player = Player::new(0, 0);
    player.supply.wood = 2;
    player.pastures = vec![pasture(0, Animal::Sheep, 3), pasture(1, Animal::Cattle, 2)];
    dispatch_card(&set, &mut player, "A057");
    assert_eq!(
        player.supply.food, 6,
        "dispatcher: sheep 3 + cattle 2 -> food 6"
    );
    assert!(player.played_cards.contains(&"A057".to_string()));
}

// ---------------- Engine route (Game::apply via MeetingPlace) ----------------

fn set_with_a057_in_pool() -> CardSet {
    let mut set = verified_basic_card_set(2).unwrap();
    set.definitions.push(a057_definition());
    set
}

fn relocate_a057_to_hand(game: &mut Game) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "A057");
    }
    game.cards.undealt.retain(|id| id != "A057");
    game.cards.hands[0].push("A057".to_owned());
}

fn a057_engine_game() -> (Game, CardSet) {
    let config = Config {
        players: 2,
        seed: 2,
        additional_action_space_enabled: false,
    };
    let set = set_with_a057_in_pool();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    relocate_a057_to_hand(&mut game);
    game.public.players[0].supply.wood = 2; // cost wood 2
                                            // Sheep 3 and cattle 1 on stable cells (4 stables total = the limit);
                                            // animal_count reads stable_animal cells too.
    for (cell, animal) in [
        (1usize, Animal::Sheep),
        (2, Animal::Sheep),
        (3, Animal::Sheep),
        (4, Animal::Cattle),
    ] {
        game.public.players[0].farm[cell] = Cell {
            tile: Tile::Empty,
            stable: true,
            stable_animal: Some(animal),
            field_stones: 0,
        };
    }
    (game, set)
}

#[test]
fn a057_engine_play_grants_food_via_meeting_place() {
    let (mut game, _set) = a057_engine_game();
    game.public.active_player = 0;
    let initial_food = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "A057".to_owned(),
        },
    })
    .expect("A057 play via the Minor Improvement action");
    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food - initial_food,
        5,
        "engine: sheep 3 -> 3 food, cattle 1 -> 2 food"
    );
    assert_eq!(p.supply.wood, 0, "cost wood 2 paid");
    assert!(
        p.played_cards.iter().any(|id| id == "A057"),
        "A057 moved to played area"
    );
    assert_eq!(p.card_points, 1, "printed 1 VP");
    assert!(
        !game.cards.hands[0].iter().any(|id| id == "A057"),
        "A057 left the hand"
    );
}

#[test]
fn a057_engine_play_fails_when_prerequisite_or_cost_unmet() {
    // Prerequisite unmet: 11 rooms leaves 3 unused spaces.
    let (mut game, _set) = a057_engine_game();
    for cell in game.public.players[0].farm.iter_mut().take(12).skip(1) {
        *cell = Cell {
            tile: Tile::Room,
            stable: false,
            stable_animal: None,
            field_stones: 0,
        };
    }
    game.public.players[0].supply.wood = 2;
    let err = game
        .apply(Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "A057".to_owned(),
            },
        })
        .unwrap_err();
    assert!(
        err.contains("cannot pay card costs or prerequisites are unmet"),
        "unexpected error: {err}"
    );

    // Cost unmet on a fresh farm.
    let (mut game, _set) = a057_engine_game();
    for cell in game.public.players[0].farm.iter_mut().take(15) {
        *cell = Cell {
            tile: Tile::Empty,
            stable: false,
            stable_animal: None,
            field_stones: 0,
        };
    }
    game.public.players[0].farm[0].tile = Tile::Room;
    game.public.players[0].farm[5].tile = Tile::Room;
    game.public.players[0].supply.wood = 1;
    let err = game
        .apply(Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "A057".to_owned(),
            },
        })
        .unwrap_err();
    assert!(
        err.contains("cannot pay card costs or prerequisites are unmet"),
        "unexpected error: {err}"
    );
}

// ---------------- Save/load/replay integrity ----------------

/// Seat 0 script (deterministic for the fixed seed/2 players used by this
/// test; no deck-order assumption beyond the SheepMarket accumulation check):
/// - Gather wood (Forest), then build a stable on cell 1 exactly once
///   (wood 2). The stable must exist before any sheep is taken into it.
/// - First SheepMarket reveal with accumulation == 1: take 1 sheep into
///   cell 1's stable.
/// - Afterwards: MeetingPlace -> play A057 (wood 2, food +2 for the sheep).
fn seat_zero_a057_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let played = player.played_cards.iter().any(|id| id == "A057");
    if played {
        return Action {
            space: Space::DayLaborer,
            detail: Detail::None,
        };
    }
    let used = &game.public.used;
    let sheep = player.animal_count(Animal::Sheep);
    let wood = player.supply.wood;
    let stable_done = player.farm[1].stable;
    // Build the stable on cell 1 exactly once. Gather wood first if needed.
    if !stable_done {
        if wood < 2 && !used.contains(&Space::Forest) {
            return Action {
                space: Space::Forest,
                detail: Detail::None,
            };
        }
        if legal.contains(&Space::FarmExpansion) && wood >= 2 {
            return Action {
                space: Space::FarmExpansion,
                detail: Detail::Build {
                    rooms: vec![],
                    stables: vec![1],
                },
            };
        }
    }
    // First revealed SheepMarket: accumulation is exactly 1 -> one sheep into
    // the stable at cell 1 (only once the stable exists).
    if sheep == 0
        && stable_done
        && legal.contains(&Space::SheepMarket)
        && game.public.animal_accumulations.get(&Space::SheepMarket) == Some(&1)
    {
        return Action {
            space: Space::SheepMarket,
            detail: Detail::Animals(AnimalPlan {
                animal: Animal::Sheep,
                to_pastures: vec![],
                to_stables: vec![1],
                pet: false,
                cook: 0,
                discard: 0,
            }),
        };
    }
    // Play A057 once we own sheep and can afford the wood cost.
    if sheep > 0 && legal.contains(&Space::MeetingPlace) && wood >= 2 {
        return Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "A057".to_owned(),
            },
        };
    }
    // Filler: gather wood first, then any basic space, else DayLaborer.
    if !used.contains(&Space::Forest) {
        return Action {
            space: Space::Forest,
            detail: Detail::None,
        };
    }
    for &space in legal {
        if matches!(
            space,
            Space::DayLaborer | Space::ReedBank | Space::ClayPit | Space::Fishing
        ) {
            return Action {
                space,
                detail: Detail::None,
            };
        }
    }
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

#[test]
fn a057_play_and_grant_survives_save_load_replay() {
    let config = Config {
        players: 2,
        seed: 2,
        additional_action_space_enabled: false,
    };
    let set = set_with_a057_in_pool();
    // Explicit hands so A057 is deterministically in seat 0's hand.
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| matches!(c.kind, CardKind::Occupation))
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| matches!(c.kind, CardKind::MinorImprovement) && c.id != "A057")
        .map(|c| c.id.clone())
        .collect();
    assert!(occs.len() >= 14 && minors.len() >= 14);
    let mut hands = vec![occs[..7].to_vec(), occs[7..14].to_vec()];
    hands[0].push("A057".to_string());
    hands[0].extend(minors[0..6].iter().cloned());
    hands[1].extend(minors[7..14].iter().cloned());
    let mut game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("a057 engine setup");

    for _ in 0..200 {
        if game.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == "A057")
        {
            break;
        }
        let legal = game.legal_spaces();
        assert!(!legal.is_empty(), "a legal placement must exist");
        let action = if game.public.active_player == 0 {
            seat_zero_a057_action(&game, &legal)
        } else {
            seat_one_action(&legal)
        };
        game.apply(action).unwrap_or_else(|e| {
            panic!(
                "round {} seat {} must be legal: {e}",
                game.public.round, game.public.active_player
            )
        });
    }

    assert!(
        game.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == "A057"),
        "seat 0 must have played A057"
    );
    assert_eq!(
        game.public.players[0].supply.food, 4,
        "start food 2 + one sheep -> stage 1 -> food 4"
    );
    assert_eq!(
        game.public.players[0].animal_count(Animal::Sheep),
        1,
        "stable sheep counted"
    );

    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;

    let loaded =
        Game::load_json(&game.save_json().expect("save recorded game")).expect("load saved game");
    assert_eq!(
        loaded.state_hash(),
        final_hash,
        "resume must reproduce state hash"
    );
    assert_eq!(
        loaded.public.players[0].supply.food, final_food,
        "resume must reproduce food"
    );

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands.clone()),
        Some(0),
        &game.log.events,
    )
    .expect("recorded game must replay");
    assert_eq!(
        replayed.state_hash(),
        final_hash,
        "replay must reproduce state hash"
    );
    assert_eq!(
        replayed.public.players[0].supply.food, final_food,
        "replay must grant the on-play gain exactly once"
    );
    assert_eq!(
        replayed.public.players[0].animal_count(Animal::Sheep),
        1,
        "replay must reproduce the sheep"
    );
}

fn seat_one_action(legal: &[Space]) -> Action {
    // Seat 1 (opponent): just pick any legal space, prefer non-competitive ones
    for &space in legal {
        if matches!(
            space,
            Space::DayLaborer | Space::ReedBank | Space::ClayPit | Space::Fishing
        ) {
            return Action {
                space,
                detail: Detail::None,
            };
        }
    }
    // Fallback: first legal space
    Action {
        space: legal[0],
        detail: Detail::None,
    }
}
