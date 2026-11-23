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
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = d063_set();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement)
        .map(|c| c.id.clone())
        .filter(|id| id != "D063")
        .collect();
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .collect();
    assert!(minors.len() >= 13 && occs.len() >= 14);
    let hands = vec![
        [
            vec!["D063".to_string()],
            minors[..6].to_vec(),
            occs[..7].to_vec(),
        ]
        .concat(),
        [minors[6..13].to_vec(), occs[7..14].to_vec()].concat(),
    ];
    let mut game = Game::with_cards_and_hands(config, set.clone(), hands.clone())
        .expect("explicit D063 hands build");

    // Seat 0 plays D063 for real (cost 0) through MeetingPlace in round 1.
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "D063".to_string(),
        },
    })
    .expect("D063 play through MeetingPlace is legal");
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "D063"));

    // Drive rounds 1-3 entirely through logged actions: seat 0 plays D063,
    // plows+sows cells 1 and 6 with grain (both orthogonally adjacent to the
    // starting rooms at cells 0 and 5), then stocks food; seat 1 stocks food.
    // Round 4's harvest therefore commits through the event log.
    for _ in 0..80 {
        if game.public.round >= 5 {
            break;
        }
        let legal = game.legal_spaces();
        assert!(
            !legal.is_empty(),
            "a legal placement must exist before round 5"
        );
        let active = game.public.active_player;
        let seat0 = active == 0;
        let p = &game.public.players[active];
        let seat0_d063_played = game.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == "D063");
        let cell1_empty = matches!(p.farm[1].tile, Tile::Empty);
        let cell1_fallow = matches!(p.farm[1].tile, Tile::Field { crop: None, .. });
        let cell6_empty = matches!(p.farm[6].tile, Tile::Empty);
        let cell6_fallow = matches!(p.farm[6].tile, Tile::Field { crop: None, .. });
        let sow = |cell: usize| Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow {
                plantings: vec![Plant {
                    cell,
                    crop: Crop::Grain,
                }],
                bake: 0,
            },
        };
        let action = if seat0 && !seat0_d063_played && legal.contains(&Space::MeetingPlace) {
            Action {
                space: Space::MeetingPlace,
                detail: Detail::PlayMinor {
                    card_id: "D063".to_string(),
                },
            }
        } else if seat0 && cell1_empty && legal.contains(&Space::Farmland) {
            Action {
                space: Space::Farmland,
                detail: Detail::Plow { cell: 1 },
            }
        } else if seat0 && cell1_fallow {
            if p.supply.grain >= 1 && legal.contains(&Space::GrainUtilization) {
                sow(1)
            } else if legal.contains(&Space::GrainSeeds) {
                Action {
                    space: Space::GrainSeeds,
                    detail: Detail::None,
                }
            } else {
                plain_action(&legal)
            }
        } else if seat0 && cell6_empty && legal.contains(&Space::Farmland) {
            Action {
                space: Space::Farmland,
                detail: Detail::Plow { cell: 6 },
            }
        } else if seat0 && cell6_fallow {
            if p.supply.grain >= 1 && legal.contains(&Space::GrainUtilization) {
                sow(6)
            } else if legal.contains(&Space::GrainSeeds) {
                Action {
                    space: Space::GrainSeeds,
                    detail: Detail::None,
                }
            } else {
                plain_action(&legal)
            }
        } else if p.supply.food < 4 && legal.contains(&Space::DayLaborer) {
            place_day_laborer()
        } else {
            plain_action(&legal)
        };
        game.apply(action).expect("driver placement must be legal");
        assert!(!game.has_pending(), "driver must never strand a pending");
    }
    assert_eq!(
        game.public.round, 5,
        "driver must reach round 5 (round-4 harvest committed)"
    );
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "D063"));
    // Cells 1 and 6 are growing grain fields adjacent to rooms 0 and 5.
    assert!(matches!(
        game.public.players[0].farm[1].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            ..
        }
    ));
    assert!(matches!(
        game.public.players[0].farm[6].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            ..
        }
    ));
    // Two growing grain fields yield 2 grain at the logged round-4 harvest.
    assert_eq!(
        game.public.players[0].supply.grain, 2,
        "round-4 harvest yield from the two logged grain fields"
    );

    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;
    let final_grain = game.public.players[0].supply.grain;

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved D063 game must resume");
    assert_eq!(loaded.state_hash(), final_hash);
    assert_eq!(loaded.public.players[0].supply.food, final_food);
    assert_eq!(loaded.public.players[0].supply.grain, final_grain);

    let replayed = Game::replay_with_setup(
        game.config.clone(),
        (*game.card_set).clone(),
        Some(hands.clone()),
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

/// Spaces whose `Detail::None` is always one complete legal action.
fn first_plain_space(legal: &[Space]) -> Space {
    [
        Space::Forest,
        Space::ClayPit,
        Space::ReedBank,
        Space::Fishing,
        Space::DayLaborer,
        Space::GrainSeeds,
        Space::VegetableSeeds,
    ]
    .iter()
    .copied()
    .find(|space| legal.contains(space))
    .expect("at least one detail-free space must be legal")
}

/// Detail::None-only action for a given legal set.
fn plain_action(legal: &[Space]) -> Action {
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}
