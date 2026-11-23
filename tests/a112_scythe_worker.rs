//! A112 大鎌使い (Scythe Worker): gain 1 grain on play, then 1 additional
//! grain per growing grain field during every harvest field phase.
//!
//! The behavioral tests use the real engine/dispatcher. The replay fixture
//! plays A112 from hand, sows two grain fields, and reaches the round-4
//! harvest entirely through recorded `Game::apply` calls.

use agricola_re_ai::cards::{
    verified_band_a_cards, verified_basic_card_set, CardDefinition, CardKind, CardSet,
};
use agricola_re_ai::engine::{Action, Detail, Plant};
use agricola_re_ai::model::{Config, Crop, Game, Player, Space, Tile};
use agricola_re_ai::registry::build_default_registry;
use std::collections::BTreeSet;

fn a112_definition() -> CardDefinition {
    verified_band_a_cards()
        .into_iter()
        .find(|card| card.id == "A112")
        .expect("A112 must be defined in verified_band_a_cards")
}

fn set_with_a112() -> CardSet {
    let mut set = verified_basic_card_set(2).expect("two-player base set");
    set.definitions.push(a112_definition());
    set
}

fn hands_with_a112(set: &CardSet) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation && card.id != "A112")
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() >= 13 && minors.len() >= 14);
    vec![
        [
            vec!["A112".to_owned()],
            occupations[..6].to_vec(),
            minors[..7].to_vec(),
        ]
        .concat(),
        [occupations[6..13].to_vec(), minors[7..14].to_vec()].concat(),
    ]
}

fn game_with_a112() -> (Config, CardSet, Vec<Vec<String>>, Game) {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_a112();
    let hands = hands_with_a112(&set);
    let game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with A112 in seat 0's hand");
    (config, set, hands, game)
}

fn play_a112(game: &mut Game) {
    assert_eq!(game.public.active_player, 0, "seat 0 must start");
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "A112".to_owned(),
        },
    })
    .expect("A112 play through the production Lessons route");
}

fn day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

/// Put A112 in the owner's played area without granting its on-play effect,
/// then stop one DayLaborer placement short of the round-4 harvest.
fn harvest_fixture() -> Game {
    let (_config, _set, _hands, mut game) = game_with_a112();
    game.cards.hands[0].retain(|id| id != "A112");
    game.public.players[0].played_cards.push("A112".to_owned());
    while game.public.round < 4 {
        game.start_round().expect("advance to round 4");
    }
    game.public.used = BTreeSet::from([Space::Farmland]);
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1;
    game.public.active_player = 0;
    game.public.players[0].supply.food = 10;
    game.public.players[1].supply.food = 10;
    game
}

#[test]
fn a112_definition_and_registry_use_the_existing_effect_vocabulary() {
    let card = a112_definition();
    assert_eq!(card.kind, CardKind::Occupation);
    assert!(card.cost.is_empty());
    assert_eq!(card.points, 0);
    assert!(matches!(
        card.effects.as_slice(),
        [
            agricola_re_ai::cards::CardEffect::OnPlayGain {
                good: agricola_re_ai::model::Good::Grain,
                amount: 1,
            },
            agricola_re_ai::cards::CardEffect::HarvestFieldGain {
                good: agricola_re_ai::model::Good::Grain,
                amount: 1,
                ..
            },
        ]
    ));

    let registry = build_default_registry().expect("default registry builds");
    let registered = registry
        .get_by_external("A112")
        .expect("A112 must be reachable through the registry");
    assert_eq!(registered.to_card_definition(), card);
}

/// A112-normal: the production play route grants exactly one grain to the
/// owner and nothing to the other seat.
#[test]
fn a112_normal_play_grants_one_grain_only_to_the_owner() {
    let (_config, _set, _hands, mut game) = game_with_a112();
    let other_grain = game.public.players[1].supply.grain;
    play_a112(&mut game);

    assert_eq!(game.public.players[0].supply.grain, 1);
    assert_eq!(game.public.players[1].supply.grain, other_grain);
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "A112"));
    assert!(!game.has_pending());
}

/// A112-harvest: two growing grain fields add two grain beyond their ordinary
/// yield. Vegetable fields, harvested/empty fields, and grain already in the
/// supply do not increase the card bonus.
#[test]
fn a112_harvest_counts_only_growing_grain_fields() {
    let mut game = harvest_fixture();
    let owner = &mut game.public.players[0];
    owner.supply.grain = 5;
    owner.farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 2,
    };
    owner.farm[2].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    owner.farm[3].tile = Tile::Field {
        crop: Some(Crop::Vegetable),
        remaining: 2,
    };
    owner.farm[4].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    owner.farm[6].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    game.public.players[1].supply.grain = 4;

    let before = game.state_hash();
    let preview = game
        .preview_harvest(&day_laborer(), None)
        .expect("field-phase preview");
    assert_eq!(
        preview.players[0].supply.grain, 9,
        "supply 5 + ordinary yield 2 + A112 bonus 2"
    );
    assert_eq!(preview.players[0].supply.vegetable, 1);
    assert_eq!(preview.players[1].supply.grain, 4);
    assert_eq!(game.state_hash(), before, "preview must not commit");

    game.apply(day_laborer()).expect("harvest completes");
    assert_eq!(game.public.players[0].supply.grain, 9);
    assert_eq!(game.public.players[0].supply.vegetable, 1);
    assert_eq!(game.public.players[1].supply.grain, 4);
    assert_eq!(game.public.players[0].supply.food, 8);
    assert!(!game.has_pending());
}

/// A112-zero: with no growing grain field, the card adds zero grain; the
/// other seat receives no owner-only effect.
#[test]
fn a112_zero_grain_fields_adds_no_grain() {
    let mut game = harvest_fixture();
    let owner = &mut game.public.players[0];
    owner.supply.grain = 7;
    owner.farm[1].tile = Tile::Field {
        crop: Some(Crop::Vegetable),
        remaining: 2,
    };
    owner.farm[2].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    owner.farm[3].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    game.public.players[1].supply.grain = 4;

    game.apply(day_laborer()).expect("harvest completes");
    assert_eq!(game.public.players[0].supply.grain, 7);
    assert_eq!(game.public.players[0].supply.vegetable, 1);
    assert_eq!(game.public.players[1].supply.grain, 4);
}

/// The retained direct reward evaluator uses the same pre-harvest count. It
/// grants only the card bonus; ordinary field yield is handled by its caller,
/// so two growing grain fields produce exactly +2 here.
#[test]
fn a112_direct_harvest_reward_path_counts_growing_grain_fields() {
    let set = CardSet {
        definitions: vec![a112_definition()],
    };
    let mut player = Player::new(0, 0);
    player.played_cards.push("A112".to_owned());
    player.supply.grain = 3;
    player.farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 2,
    };
    player.farm[2].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    player.farm[3].tile = Tile::Field {
        crop: Some(Crop::Vegetable),
        remaining: 2,
    };
    // Direct evaluators also reject remaining=0 as a growing grain field;
    // production harvest validation normalizes this state to crop=None.
    player.farm[4].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 0,
    };

    agricola_re_ai::cards::harvest_field_rewards(&set, &mut player)
        .expect("direct legacy reward path");
    assert_eq!(player.supply.grain, 5);
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

/// Seat 0 stocks feeding food, plays A112, obtains two grain (one comes from
/// A112 itself), plows two fields, and sows both before the round-4 harvest.
fn seat_zero_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let a112_played = player.played_cards.iter().any(|id| id == "A112");
    let empty_fields = game.detail_hints(Space::GrainUtilization).sow_fields;
    if player.supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return day_laborer();
    }
    if !a112_played && legal.contains(&Space::Lessons) {
        return Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "A112".to_owned(),
            },
        };
    }
    let sown = field_count(game, 0) - empty_fields.len();
    if player.supply.grain < 2 && sown < 2 && legal.contains(&Space::GrainSeeds) {
        return Action {
            space: Space::GrainSeeds,
            detail: Detail::None,
        };
    }
    if field_count(game, 0) < 2 && legal.contains(&Space::Farmland) {
        if let Some(&cell) = game.detail_hints(Space::Farmland).plow_cells.first() {
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
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn seat_one_action(game: &Game, legal: &[Space]) -> Action {
    if game.public.players[1].supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return day_laborer();
    }
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn drive_to_round_five(game: &mut Game) {
    for _ in 0..40 {
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
        assert!(!game.has_pending(), "A112 must not create a pending");
    }
    panic!("recorded drive never reached the round-4 harvest");
}

/// Save/resume and action-only replay reproduce exactly one A112 bonus. The
/// two fields yield two grain and A112 adds two more; neither save loading nor
/// replay grants the effect a second time.
#[test]
fn a112_save_resume_and_replay_do_not_double_grant() {
    let (config, set, hands, mut game) = game_with_a112();
    drive_to_round_five(&mut game);
    assert_eq!(game.public.round, 5);
    assert_eq!(game.public.players[0].supply.grain, 4);
    assert_eq!(game.public.players[0].begging, 0);

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved A112 game must resume");
    assert_eq!(loaded.state_hash(), game.state_hash());
    assert_eq!(loaded.public.players[0].supply.grain, 4);

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded A112 game must replay");
    assert_eq!(replayed.state_hash(), game.state_hash());
    assert_eq!(replayed.public.players[0].supply.grain, 4);
}
