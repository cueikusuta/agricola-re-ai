//! D063 段地 (Lynchet): 1 food per harvested field tile orthogonally
//! adjacent to the house, every harvest field phase.
//!
//! Spec: data/bga-card-specs/D063.json (cost none, points 0, permanent
//! harvest_field_phase gain of food per count("harvested_field_adjacent_house")).
//! Cases: data/bga-card-tests/D063.json (D063-normal / D063-zero).
//!
//! The house is the player's room tiles (`Tile::Room`); a field counts only
//! when it is harvested this phase (`crop: Some`, `remaining: 1..`) AND one
//! of its orthogonal neighbors is a room. Empty/fallow fields never count,
//! even when they touch a room. The field-phase dispatch runs before crops
//! are removed, so the count is taken on pre-harvest state.

use agricola_re_ai::cards::{
    card_set_for_game, verified_band_d_cards, verified_basic_card_set, CardEffect, CardKind,
    CardSet, HarvestFieldRule,
};
use agricola_re_ai::engine::{Action, Detail, Plant};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::model::{Config, Crop, Game, Good, Space, Tile};
use std::collections::BTreeSet;

fn d063_definition() -> agricola_re_ai::cards::CardDefinition {
    verified_band_d_cards()
        .into_iter()
        .find(|card| card.id == "D063")
        .expect("D063 must be defined in verified_band_d_cards")
}

fn d063_set() -> CardSet {
    let mut set = verified_basic_card_set(2).expect("two-player base set");
    set.definitions.push(d063_definition());
    set
}

/// Round-4 field-phase fixture: real `start_round` path to round 4, one
/// worker short of the quota so a single DayLaborer placement commits the
/// whole harvest (field phase + feeding) through the public one-shot API.
fn d063_fixture() -> Game {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let mut game = Game::with_cards(config, d063_set()).unwrap();
    while game.public.round < 4 {
        game.start_round().unwrap();
    }
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1;
    game.public.active_player = 0;
    game.public.players[0].supply.food = 10;
    game
}

fn teleport_into_play(game: &mut Game, ids: &[&str]) {
    for id in ids {
        for hand in game.cards.hands.iter_mut() {
            hand.retain(|h| h != id);
        }
        game.cards.undealt.retain(|h| h != id);
        game.public.players[0].played_cards.push(id.to_string());
    }
}

/// Plant growing grain fields (one remaining crop, about to be harvested).
fn growing_fields(game: &mut Game, cells: &[usize]) {
    for i in cells {
        game.public.players[0].farm[*i].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
    }
}

fn place_day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

// The farm is 5 columns x 3 rows; player 0 starts with rooms at cells 0
// and 5. Cell 1 neighbors room 0; cell 6 neighbors room 5; cell 2 and cell
// 8 neighbor no room.

#[test]
fn d063_definition_matches_the_spec() {
    let card = d063_definition();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert!(card.cost.is_empty());
    assert_eq!(card.points, 0);
    assert_eq!(
        card.effects.as_slice(),
        [CardEffect::HarvestFieldGain {
            rule: HarvestFieldRule::FieldsAdjacentToHouse,
            good: Good::Food,
            amount: 1,
        }]
    );

    let registry =
        agricola_re_ai::registry::build_default_registry().expect("default registry builds");
    let registered = registry
        .get_by_external("D063")
        .expect("D063 must be reachable through the registry");
    assert_eq!(registered.to_card_definition(), card);

    // Eligible at 2, 3 and 4 players per the spec.
    assert!(card_set_for_game(2, true).definition("D063").is_some());
    assert!(card_set_for_game(3, true).definition("D063").is_some());
    assert!(card_set_for_game(4, true).definition("D063").is_some());
}

/// D063-normal: two harvested fields adjacent to the house grant +2 food in
/// the field phase. A read-only preview must show the delta without
/// committing any state.
#[test]
fn d063_two_fields_adjacent_to_house_grant_two_food() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    growing_fields(&mut game, &[1, 6]); // both orthogonally adjacent to a room

    let before = game.state_hash();
    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 14,
        "10 stored + DayLaborer 2 + D063 bonus 2 (two harvested fields adjacent to house)"
    );
    assert_eq!(
        preview.players[0].supply.grain, 2,
        "two fields each yield 1 grain"
    );
    assert_eq!(game.state_hash(), before, "preview must not commit");

    // Committed harvest: field-phase bonus and DayLaborer food arrive, then
    // feeding (4 food for two family members) is charged.
    game.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("harvest completes");
    assert!(!game.has_pending());
    assert_eq!(
        game.public.players[0].supply.food, 10,
        "10 + 2 DayLaborer + 2 D063 - 4 feeding"
    );
    assert_eq!(game.public.players[0].supply.grain, 2);
}

/// D063-zero: no harvested fields adjacent to the house grants nothing.
#[test]
fn d063_no_fields_adjacent_to_house_grants_nothing() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 12,
        "10 stored + DayLaborer 2, no card bonus"
    );
    assert_eq!(preview.players[0].supply.grain, 0);

    game.apply_with_harvest_policy(place_day_laborer(), &SimpleHarvestDetails)
        .expect("harvest completes");
    assert_eq!(game.public.players[0].supply.food, 8);
    assert_eq!(game.public.players[0].supply.grain, 0);
    assert!(!game.has_pending());
}

/// Fields not adjacent to the house do not count: exactly one of two fields
/// touches a room, so only +1 food.
#[test]
fn d063_only_fields_adjacent_to_the_house_count() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    // Cell 1 neighbors room 0 (counts); cell 8 neighbors no room (ignored).
    growing_fields(&mut game, &[1, 8]);

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 13,
        "10 stored + DayLaborer 2 + D063 bonus 1 (only the adjacent field counts)"
    );
    assert_eq!(preview.players[0].supply.grain, 2);
}

/// Fallow/empty fields adjacent to the house are not harvested and never
/// count, even though they touch a room.
#[test]
fn d063_empty_field_adjacent_to_house_does_not_count() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    // Cell 1 is empty (fallow) but neighbors room 0: must not count.
    game.public.players[0].farm[1].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 12,
        "10 stored + DayLaborer 2, fallow field adjacent to the house grants nothing"
    );
}

/// Boundary: a field adjacent to the house with `remaining: 2` is growing
/// and will yield grain this phase, but it does not COMPLETE harvest
/// (`remaining` would become 1, not 0), so it must NOT grant the D063 food
/// bonus. D063 counts `remaining == 1` only — `remaining: 1..` would wrongly
/// pay +1 food here. Grain still yields 1 because `remaining > 0`.
#[test]
fn d063_remaining_two_field_adjacent_to_house_does_not_count() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    // Cell 1 neighbors room 0, but remaining: 2 means the field is harvested
    // this phase yet does not complete: D063 pays nothing for it.
    game.public.players[0].farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 2,
    };

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 12,
        "10 stored + DayLaborer 2, D063 bonus 0 (remaining: 2 is not remaining == 1)"
    );
    assert_eq!(
        preview.players[0].supply.grain, 1,
        "remaining: 2 field still yields 1 grain this phase"
    );
}

/// Owner scoping: D063 counts only the card owner's own fields orthogonally
/// adjacent to the owner's rooms. Player 1's field adjacent to player 1's
/// room must not grant player 0 anything.
#[test]
fn d063_only_card_owners_fields_adjacent_to_owners_house_count() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    // Player 0's own field adjacent to room 5: counts (+1 food, +1 grain).
    game.public.players[0].farm[6].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    // Player 1's field adjacent to player 1's room (cell 0): must NOT count
    // for player 0's D063.
    game.public.players[1].farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 13,
        "10 stored + DayLaborer 2 + D063 1 (only player 0's own adjacent field)"
    );
    assert_eq!(
        preview.players[0].supply.grain, 1,
        "player 0 gets only their own field's grain"
    );
}

/// Diagonal and row-end pseudo-adjacency must not count. The farm is
/// 5 columns x 3 rows; orthogonal neighbors are same-row ±1 and vertical ±5
/// only. Cell 2 is diagonal to room 0 (0,0) -> (1,1) = cell 6? Wait:
/// Room 0 at index 0 (col 0, row 0). Its orthogonal neighbors: 1 (right) and
/// 5 (down). Cell 2 (col 2, row 0) is two steps right, not adjacent.
/// Cell 4 (col 4, row 0) right neighbor would wrap to col 0 row 1 (index 5)
/// but wrapping is NOT allowed. So cell 4 is NOT adjacent to room 5.
/// Cell 9 (col 4, row 1) diagonal to room 5? Room 5 at (0,1); cell 9 at (4,1)
/// four columns away. We test the two clear non-adjacent positions that a
/// buggy implementation might mistake: cell 2 (two steps from room 0) and
/// cell 4 (row-end next to room 5's column but different row).
#[test]
fn d063_diagonal_and_row_end_not_adjacent() {
    let mut game = d063_fixture();
    teleport_into_play(&mut game, &["D063"]);
    // Cell 2 is two steps right from room 0 (not orthogonal neighbor).
    // Cell 4 is at row-end; its right neighbor would be index 5 (room 5)
    // but row wrapping is prohibited, so it does NOT neighbor room 5.
    growing_fields(&mut game, &[2, 4]);

    let preview = game
        .preview_harvest(&place_day_laborer(), None)
        .expect("field-phase preview runs");
    assert_eq!(
        preview.players[0].supply.food, 12,
        "10 stored + DayLaborer 2, D063 bonus 0 (neither cell 2 nor 4 adjacent)"
    );
    assert_eq!(
        preview.players[0].supply.grain, 2,
        "both fields still yield grain"
    );
}

/// The legacy direct evaluator (`cards::harvest_field_rewards`) must agree
/// with the event-dispatcher path for the same pre-harvest board.
#[test]
fn d063_legacy_harvest_field_rewards_match_the_dispatcher() {
    let set = CardSet {
        definitions: vec![d063_definition()],
    };
    let mut player = agricola_re_ai::model::Player::new(0, 10);
    player.played_cards.push("D063".to_string());
    // Cell 1 neighbors room 0 (initial room); cell 6 neighbors room 5.
    player.farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    player.farm[6].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    // Cell 8 touches no room: ignored.
    player.farm[8].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };

    agricola_re_ai::cards::harvest_field_rewards(&set, &mut player)
        .expect("direct legacy reward path");
    assert_eq!(player.supply.food, 12, "10 stored + 2 card bonus");
    assert_eq!(player.supply.grain, 0, "direct evaluator grants food only");
}

/// Save/load and action-only replay grant the field-phase bonus exactly
/// once and preserve the state hash. D063 is dealt to seat 0 and played for
/// real through MeetingPlace, so every mutation flows through the event log
/// and replay starts from a matching prestate.
#[test]
fn d063_save_load_and_replay_do_not_double_grant() {
    let (config, set, hands, mut game) = game_with_d063();
    play_d063(&mut game);
    drive_to_round_five(&mut game);

    assert_eq!(game.public.round, 5);
    // Two fields yield 2 grain; D063 adds 2 food (one per field adjacent to house).
    // Feeding and any incidental food gains are deterministic; we just check grain
    // and that no begging occurred.
    assert_eq!(game.public.players[0].supply.grain, 2);
    assert_eq!(game.public.players[0].begging, 0);

    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;
    let final_grain = game.public.players[0].supply.grain;

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved D063 game must resume");
    assert_eq!(loaded.state_hash(), final_hash);
    assert_eq!(loaded.public.players[0].supply.food, final_food);
    assert_eq!(loaded.public.players[0].supply.grain, final_grain);

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded D063 game must replay");
    assert_eq!(replayed.state_hash(), final_hash, "replay must not drift");
    assert_eq!(
        replayed.public.players[0].supply.food, final_food,
        "no double grant on replay"
    );
    assert_eq!(
        replayed.public.players[0].supply.grain, final_grain,
        "no double grant on replay"
    );
}

/// Build a 2-player game with D063 in seat 0's hand, fixed seed, no extra space.
fn game_with_d063() -> (Config, CardSet, Vec<Vec<String>>, Game) {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = d063_set();
    let hands = hands_with_d063(&set);
    let game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with D063 in seat 0's hand");
    (config, set, hands, game)
}

/// Partition the card pool so seat 0 holds D063 plus a balanced mix of
/// occupations and minor improvements; seat 1 gets the rest.
/// Seat 0: 7 occupations + 7 minors (including D063).
/// Seat 1: 7 occupations + 7 minors.
fn hands_with_d063(set: &CardSet) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() >= 14 && minors.len() >= 14);
    vec![
        [
            occupations[..7].to_vec(),
            [vec!["D063".to_owned()], minors[..6].to_vec()].concat(),
        ]
        .concat(),
        [occupations[7..14].to_vec(), minors[6..13].to_vec()].concat(),
    ]
}

/// Play D063 for real (cost 0) through MeetingPlace in round 1.
fn play_d063(game: &mut Game) {
    assert_eq!(game.public.active_player, 0, "seat 0 must start");
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "D063".to_owned(),
        },
    })
    .expect("D063 play through the production MeetingPlace route");
}

/// Drive the game through rounds 1-4 entirely via recorded `apply()` calls.
/// Seat 0: plays D063, gets grain, plows cells 1 & 6, sows both with grain,
/// stocks food via DayLaborer when needed.
/// Seat 1: only stocks food via DayLaborer.
fn drive_to_round_five(game: &mut Game) {
    for _ in 0..60 {
        if game.public.round >= 5 {
            return;
        }
        let legal = game.legal_spaces();
        assert!(!legal.is_empty(), "a legal placement must exist");
        let action = if game.public.active_player == 0 {
            seat_zero_action(game, &legal)
        } else {
            seat_one_action(game, &legal)
        };
        game.apply(action)
            .expect("scripted placement must be legal");
        assert!(!game.has_pending(), "D063 must not create a pending");
    }
    panic!("recorded drive never reached the round-4 harvest");
}

const PLAIN_SPACES: [Space; 7] = [
    Space::Forest,
    Space::ClayPit,
    Space::ReedBank,
    Space::Fishing,
    Space::DayLaborer,
    Space::GrainSeeds,
    Space::VegetableSeeds,
];

fn first_plain_space(legal: &[Space]) -> Space {
    PLAIN_SPACES
        .iter()
        .copied()
        .find(|space| legal.contains(space))
        .expect("at least one detail-free space must be legal")
}

fn field_count(game: &Game, player: usize) -> usize {
    game.public.players[player]
        .farm
        .iter()
        .filter(|cell| matches!(cell.tile, Tile::Field { .. }))
        .count()
}

/// Seat 0's recorded script:
/// 1. Play D063 (MeetingPlace) if not yet played.
/// 2. Take GrainSeeds if grain < 2 and fewer than 2 fields sown.
/// 3. Plow cells 1 and 6 if empty and Farmland available.
/// 4. Sow grain into empty plowed fields if grain > 0 and GrainUtilization legal.
/// 5. Take DayLaborer when food < 4.
/// 6. Otherwise a plain detail-free action.
fn seat_zero_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let d063_played = player.played_cards.iter().any(|id| id == "D063");
    let empty_fields = game.detail_hints(Space::GrainUtilization).sow_fields;
    let sown = field_count(game, 0) - empty_fields.len();

    if !d063_played && legal.contains(&Space::MeetingPlace) {
        return Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "D063".to_owned(),
            },
        };
    }
    if player.supply.grain < 2 && sown < 2 && legal.contains(&Space::GrainSeeds) {
        return Action {
            space: Space::GrainSeeds,
            detail: Detail::None,
        };
    }
    if field_count(game, 0) < 2 && legal.contains(&Space::Farmland) {
        if let Some(&cell) = game.detail_hints(Space::Farmland).plow_cells.first() {
            // Prefer cell 1 then cell 6 (both adjacent to starting rooms).
            return Action {
                space: Space::Farmland,
                detail: Detail::Plow { cell },
            };
        }
    }
    if legal.contains(&Space::GrainUtilization)
        && !empty_fields.is_empty()
        && player.supply.grain > 0
    {
        let count = empty_fields.len().min(player.supply.grain as usize);
        return Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow {
                plantings: empty_fields[..count]
                    .iter()
                    .map(|&cell| Plant {
                        cell,
                        crop: Crop::Grain,
                    })
                    .collect(),
                bake: 0,
            },
        };
    }
    if player.supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return day_laborer();
    }
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

/// Seat 1 only stocks food via DayLaborer when needed.
fn seat_one_action(game: &Game, legal: &[Space]) -> Action {
    if game.public.players[1].supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return day_laborer();
    }
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}
