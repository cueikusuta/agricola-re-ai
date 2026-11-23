//! A153 豚持ち / B154 羊持ち — `AfterAnimalAcquisitionBonusPoints` (small chapter, 2 cards).
//!
//! The spec trigger `after_animal_acquisition` means "whenever an animal
//! acquisition completes": market / accumulation spaces,
//! card-granted placement (`engine::place_animals` via on-play and the
//! pending-resolution path), and breeding births. Each route calls
//! `card_events::award_animal_acquisition_bonuses` directly after placement,
//! so no new event site and no serialization change (the variant is appended
//! last; existing variants' byte layout is untouched).
//!
//! Tests cover:
//! - 正系: threshold reached via acquisition -> bonus points (+ food for B154)
//! - 不発: below threshold -> no award
//! - 境界: exactly at threshold -> award
//! - 頭数が減った後の再評価: once-per-game guard blocks re-award even if the
//!   count drops below and recovers above the threshold
//! - 非獲得アクションでは発火しない

use agricola_re_ai::breeding::BreedingPlan;
use agricola_re_ai::card_events::award_animal_acquisition_bonuses;
use agricola_re_ai::cards::card_set_for_game;
use agricola_re_ai::engine::{Action, AnimalPlan, Detail};
use agricola_re_ai::model::{pasture_border, Animal, Config, Game, Pasture, Space, Tile};
use agricola_re_ai::registry::build_default_registry;

fn set4() -> agricola_re_ai::cards::CardSet {
    let set = card_set_for_game(4, true);
    assert!(
        set.definitions.iter().any(|c| c.id == "A153"),
        "A153 must be in the 4-player pool"
    );
    assert!(
        set.definitions.iter().any(|c| c.id == "B154"),
        "B154 must be in the 4-player pool"
    );
    set
}

/// Seat 0's first occupation is `seeded`; all hands are explicit 7+7 deals so
/// the game builds deterministically from the 4-player experimental pool.
fn four_player_hands(seeded: &str) -> Vec<Vec<String>> {
    let set = set4();
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
    assert!(occupations.len() >= 7 * 4);
    assert!(minors.len() >= 7 * 4);
    for id in ["A153", "B154"] {
        occupations.retain(|x| x != id);
    }
    let mut occ_pool: Vec<String> = seeded.split(',').map(|s| s.to_string()).collect();
    occ_pool.extend(occupations);

    let mut hands: Vec<Vec<String>> = vec![Vec::new(); 4];
    let mut cursor = 0usize;
    for hand in hands.iter_mut() {
        for _ in 0..7 {
            hand.push(occ_pool[cursor].clone());
            cursor += 1;
        }
    }
    let mut cursor = 0usize;
    for hand in hands.iter_mut() {
        for _ in 0..7 {
            hand.push(minors[cursor].clone());
            cursor += 1;
        }
    }
    hands
}

fn four_player_game(seeded: &str) -> Game {
    let config = Config {
        players: 4,
        seed: 7,
        additional_action_space_enabled: true,
    };
    Game::with_cards_and_hands_with_start_player(config, set4(), four_player_hands(seeded), 0)
        .expect("4-player game with A153/B154 in seat 0's hand")
}

/// Put `seeded` directly onto seat 0's played_cards (canonical zone move
/// shortcut used by the ch01 tests — the scan only reads played_cards).
fn mark_played_seat0(game: &mut Game, card_id: &str) {
    game.public.players[0]
        .played_cards
        .push(card_id.to_string());
}

/// Give seat 0 exactly `n` boars in one 3-cell pasture (capacity 6, enough
/// room for the acquisitions used here). The cells must carry `Tile::Pasture`
/// so `validate_player_state` accepts the state.
fn give_seat0_boars(game: &mut Game, n: u32) {
    let p = &mut game.public.players[0];
    for &cell in &[0, 1, 2] {
        p.farm[cell].tile = Tile::Pasture { id: 1 };
    }
    p.fences.extend(pasture_border(&[0, 1, 2]));
    p.pastures.push(Pasture {
        id: 1,
        cells: vec![0, 1, 2],
        animal: Some(Animal::Boar),
        quantity: n,
    });
}

/// Give seat 0 exactly `n` sheep in one 4-cell pasture (capacity 8).
fn give_seat0_sheep(game: &mut Game, n: u32) {
    let p = &mut game.public.players[0];
    for &cell in &[5, 6, 7, 8] {
        p.farm[cell].tile = Tile::Pasture { id: 2 };
    }
    p.fences.extend(pasture_border(&[5, 6, 7, 8]));
    p.pastures.push(Pasture {
        id: 2,
        cells: vec![5, 6, 7, 8],
        animal: Some(Animal::Sheep),
        quantity: n,
    });
}

/// Acquire exactly one boar via the AnimalMarket seat-0 action (boars are
/// free and give no food; placed into pasture id 1).
fn acquire_one_boar_via_market(game: &mut Game) {
    game.apply(Action {
        space: Space::AnimalMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Boar,
            to_pastures: vec![(1, 1)],
            to_stables: vec![],
            pet: false,
            cook: 0,
            discard: 0,
        }),
    })
    .expect("one boar via AnimalMarket");
}

/// Acquire exactly one sheep via the AnimalMarket seat-0 action (sheep grant
/// +1 food on top of the card bonus being tested; placed into pasture id 2).
fn acquire_one_sheep_via_market(game: &mut Game) {
    game.apply(Action {
        space: Space::AnimalMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![(2, 1)],
            to_stables: vec![],
            pet: false,
            cook: 0,
            discard: 0,
        }),
    })
    .expect("one sheep via AnimalMarket");
}

#[test]
fn registry_reaches_a153_and_b154_at_four_players() {
    let registry = build_default_registry().expect("default registry builds");
    for id in ["A153", "B154"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must exist"));
        let card = registry
            .get(card_id)
            .unwrap_or_else(|| panic!("{id} must be retrievable"));
        assert_eq!(card.external_id, id);
        assert_eq!(card.kind, agricola_re_ai::cards::CardKind::Occupation);
        assert!(card.effects.iter().any(|e| matches!(
            e,
            agricola_re_ai::cards::CardEffect::AfterAnimalAcquisitionBonusPoints { .. }
        )));
    }
}

#[test]
fn a153_normal_fires_at_five_boars() {
    let mut game = four_player_game("A153");
    mark_played_seat0(&mut game, "A153");
    give_seat0_boars(&mut game, 4);
    assert_eq!(game.public.players[0].animal_count(Animal::Boar), 4);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 0);

    acquire_one_boar_via_market(&mut game);

    assert_eq!(game.public.players[0].animal_count(Animal::Boar), 5);
    assert_eq!(
        game.public.players[0].animal_acquisition_bonus_points, 3,
        "A153: +3 at 5 boars"
    );
    assert!(game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("A153"));
}

#[test]
fn b154_normal_fires_at_seven_sheep() {
    let mut game = four_player_game("B154");
    mark_played_seat0(&mut game, "B154");
    give_seat0_sheep(&mut game, 6);
    let food_before = game.public.players[0].supply.food;

    acquire_one_sheep_via_market(&mut game);

    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 7);
    assert_eq!(
        game.public.players[0].animal_acquisition_bonus_points, 3,
        "B154: +3 at 7 sheep"
    );
    // AnimalMarket grants +1 food for sheep; the card grants +2 food: +3 total.
    assert_eq!(
        game.public.players[0].supply.food,
        food_before + 3,
        "B154: market food 1 + bonus food 2"
    );
    assert!(game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("B154"));
}

#[test]
fn a153_boundary_exactly_five_boars_fires() {
    let mut game = four_player_game("A153");
    mark_played_seat0(&mut game, "A153");
    give_seat0_boars(&mut game, 4);
    acquire_one_boar_via_market(&mut game);
    assert_eq!(game.public.players[0].animal_count(Animal::Boar), 5);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 3);
}

#[test]
fn b154_boundary_exactly_seven_sheep_fires() {
    let mut game = four_player_game("B154");
    mark_played_seat0(&mut game, "B154");
    give_seat0_sheep(&mut game, 6);
    acquire_one_sheep_via_market(&mut game);
    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 7);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 3);
}

#[test]
fn a153_not_triggered_at_four_boars() {
    let mut game = four_player_game("A153");
    mark_played_seat0(&mut game, "A153");
    give_seat0_boars(&mut game, 3);
    acquire_one_boar_via_market(&mut game);
    assert_eq!(game.public.players[0].animal_count(Animal::Boar), 4);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 0);
    assert!(!game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("A153"));
}

#[test]
fn b154_not_triggered_at_six_sheep() {
    let mut game = four_player_game("B154");
    mark_played_seat0(&mut game, "B154");
    give_seat0_sheep(&mut game, 5);
    acquire_one_sheep_via_market(&mut game);
    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 6);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 0);
    assert!(!game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("B154"));
}

#[test]
fn no_spurious_fire_on_non_acquisition_action() {
    // B154 already at 7 sheep when played; a plain Forest action is not an
    // acquisition, so nothing may be awarded (the scan only runs after
    // `place_animals` completions).
    let mut game = four_player_game("B154");
    give_seat0_sheep(&mut game, 7);
    mark_played_seat0(&mut game, "B154");
    let food_before = game.public.players[0].supply.food;
    game.apply(Action {
        space: Space::Forest,
        detail: Detail::None,
    })
    .expect("Forest");

    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 7);
    assert_eq!(
        game.public.players[0].animal_acquisition_bonus_points, 0,
        "no award without acquisition"
    );
    assert_eq!(game.public.players[0].supply.food, food_before);
    assert!(!game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("B154"));
}

#[test]
fn once_per_game_blocks_re_award_after_count_drops_and_recovers() {
    let set = set4();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("A153".to_string());
    player.pastures.push(Pasture {
        id: 1,
        cells: vec![0, 1, 2],
        animal: Some(Animal::Boar),
        quantity: 4, // below the 5-boar threshold
    });
    player.fences.extend(pasture_border(&[0, 1, 2]));
    assert_eq!(player.animal_count(Animal::Boar), 4);

    // Below threshold: no award.
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(player.animal_acquisition_bonus_points, 0);
    assert!(!player.animal_acquisition_bonus_fired.contains("A153"));

    // Acquisition: 4 -> 5 boars -> award.
    player
        .pastures
        .iter_mut()
        .find(|p| p.id == 1)
        .unwrap()
        .quantity = 5;
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(player.animal_acquisition_bonus_points, 3);
    assert!(player.animal_acquisition_bonus_fired.contains("A153"));

    // Count drops below the threshold: no change (already fired).
    player
        .pastures
        .iter_mut()
        .find(|p| p.id == 1)
        .unwrap()
        .quantity = 4;
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(player.animal_acquisition_bonus_points, 3);
    assert_eq!(player.animal_acquisition_bonus_fired.len(), 1);

    // Count recovers above the threshold: still no second award (once per game).
    player
        .pastures
        .iter_mut()
        .find(|p| p.id == 1)
        .unwrap()
        .quantity = 5;
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(
        player.animal_acquisition_bonus_points, 3,
        "once per game: no second award after recovery"
    );
    assert_eq!(
        player.animal_acquisition_bonus_fired.len(),
        1,
        "A153 recorded as fired exactly once"
    );
}

#[test]
fn on_play_route_does_not_spuriously_fire_at_zero_boars() {
    let mut game = four_player_game("A153");
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "A153".to_string(),
        },
    })
    .expect("A153 playable through the engine route at 4 players");
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "A153"));
    assert_eq!(game.public.players[0].animal_count(Animal::Boar), 0);
    assert_eq!(game.public.players[0].animal_acquisition_bonus_points, 0);
    assert!(!game.public.players[0]
        .animal_acquisition_bonus_fired
        .contains("A153"));
}

#[test]
fn unit_scan_awards_when_threshold_met() {
    let set = set4();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("A153".to_string());
    player.pastures.push(Pasture {
        id: 1,
        cells: vec![0, 1, 2],
        animal: Some(Animal::Boar),
        quantity: 5,
    });
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(player.animal_acquisition_bonus_points, 3);
    assert!(player.animal_acquisition_bonus_fired.contains("A153"));
}

#[test]
fn unit_scan_does_not_award_below_threshold() {
    let set = set4();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("B154".to_string());
    player.pastures.push(Pasture {
        id: 2,
        cells: vec![3, 4, 5],
        animal: Some(Animal::Sheep),
        quantity: 6,
    });
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(player.animal_acquisition_bonus_points, 0);
    assert!(!player.animal_acquisition_bonus_fired.contains("B154"));
}

#[test]
fn unit_scan_idempotent_after_fire() {
    let set = set4();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("A153".to_string());
    player.pastures.push(Pasture {
        id: 1,
        cells: vec![0, 1, 2],
        animal: Some(Animal::Boar),
        quantity: 5,
    });
    award_animal_acquisition_bonuses(&set, &mut player).unwrap();
    award_animal_acquisition_bonuses(&set, &mut player).unwrap();
    assert_eq!(player.animal_acquisition_bonus_points, 3);
    assert_eq!(player.animal_acquisition_bonus_fired.len(), 1);
}

#[test]
fn breeding_birth_triggers_a153() {
    let set = set4();
    let mut player = agricola_re_ai::model::Player::new(0, 0);
    player.played_cards.push("A153".to_string());
    // 4 boars in a 3-cell pasture: capacity 6, so one birth fits.
    player.pastures.push(Pasture {
        id: 1,
        cells: vec![0, 1, 2],
        animal: Some(Animal::Boar),
        quantity: 4,
    });
    assert_eq!(player.animal_count(Animal::Boar), 4);
    let plan = BreedingPlan::simple(&player);
    assert_eq!(plan.births.len(), 1);
    plan.apply(&mut player)
        .expect("single boar birth must be legal");
    assert_eq!(player.animal_count(Animal::Boar), 5);
    award_animal_acquisition_bonuses(&set, &mut player).expect("scan must not fail");
    assert_eq!(
        player.animal_acquisition_bonus_points, 3,
        "breeding birth counts as acquisition"
    );
    assert!(player.animal_acquisition_bonus_fired.contains("A153"));
}
